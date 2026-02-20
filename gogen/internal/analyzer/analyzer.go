package analyzer

import (
	"bufio"
	"fmt"
	"go/ast"
	"go/token"
	"go/types"
	"log"
	"os"
	"os/exec"
	"path/filepath"
	"strings"

	"gogen/internal/schema"
	"golang.org/x/tools/go/packages"
)

const (
	CLASS    = "CLASS"
	FUNCTION = "FUNCTION"
	FIELD    = "FIELD"
)

func getObjectFQN(pkg *types.Package, obj types.Object, parentMap map[types.Object]string) string {
	pkgName := pkg.Name()

	// 1. Method: Check if *types.Func has a receiver.
	if fn, ok := obj.(*types.Func); ok {
		sig := fn.Type().(*types.Signature)
		if sig.Recv() != nil {
			recvType := sig.Recv().Type()
			// Dereference pointers
			if ptr, ok := recvType.(*types.Pointer); ok {
				recvType = ptr.Elem()
			}
			// Use the receiver's type name
			if named, ok := recvType.(*types.Named); ok {
				return fmt.Sprintf("%s.%s.%s", pkgName, named.Obj().Name(), obj.Name())
			}
		}
	}

	// 2. Field/Interface Method: Check if object is in parentMap.
	if parentName, ok := parentMap[obj]; ok {
		return fmt.Sprintf("%s.%s.%s", pkgName, parentName, obj.Name())
	}

	// 3. Global: Check if parent scope is package scope.
	if obj.Parent() == pkg.Scope() {
		isAlias := false
		if tn, ok := obj.(*types.TypeName); ok && tn.IsAlias() {
			isAlias = true
		}

		switch obj.(type) {
		case *types.Var, *types.Const:
			return fmt.Sprintf("%s._module_.%s", pkgName, obj.Name())
		case *types.TypeName:
			if isAlias {
				return fmt.Sprintf("%s._module_.%s", pkgName, obj.Name())
			}
		}
	}

	// 4. Default: pkg.Name
	return pkgName + "." + obj.Name()
}

func captureSnippet(path string, line int) string {
	f, err := os.Open(path)
	if err != nil {
		return ""
	}
	defer f.Close()

	var lines []string
	scanner := bufio.NewScanner(f)
	current := 1
	start := line - 3
	end := line + 3

	for scanner.Scan() {
		if current >= start && current <= end {
			lines = append(lines, scanner.Text())
		}
		if current > end {
			break
		}
		current++
	}
	return strings.Join(lines, "\n")
}

func prepareModule(projectPath string) {
	modFile := filepath.Join(projectPath, "go.mod")
	if _, err := os.Stat(modFile); os.IsNotExist(err) {
		return
	}

	log.Printf("Preparing dependencies in %s...", projectPath)
	cmd := exec.Command("go", "mod", "tidy")
	cmd.Dir = projectPath
	if output, err := cmd.CombinedOutput(); err != nil {
		log.Printf("Warning: 'go mod tidy' failed in %s: %v\nOutput: %s", projectPath, err, string(output))
		// We proceed anyway as some analysis might still be possible or local files might be sufficient.
	}
}

func Analyze(projectPath string) (*schema.ProgramUsages, error) {
	prepareModule(projectPath)

	cfg := &packages.Config{
		Mode: packages.NeedName | packages.NeedFiles | packages.NeedSyntax | packages.NeedTypes | packages.NeedTypesInfo,
		Dir:  projectPath,
	}

	pkgs, err := packages.Load(cfg, "./...")
	if err != nil {
		return nil, err
	}

	type unitMeta struct {
		unit   *schema.CodeUnitUsages
		usages map[string]schema.UsageLocation // Map to ensure uniqueness per location
	}

	objToUnit := make(map[types.Object]*unitMeta)

	// Phase 1: Collect Definitions (excluding tests)
	for _, pkg := range pkgs {
		parentMap := make(map[types.Object]string)
		scope := pkg.Types.Scope()
		for _, name := range scope.Names() {
			obj := scope.Lookup(name)
			if tn, ok := obj.(*types.TypeName); ok {
				underlying := tn.Type().Underlying()
				if st, ok := underlying.(*types.Struct); ok {
					for i := 0; i < st.NumFields(); i++ {
						parentMap[st.Field(i)] = tn.Name()
					}
				} else if it, ok := underlying.(*types.Interface); ok {
					for i := 0; i < it.NumMethods(); i++ {
						parentMap[it.Method(i)] = tn.Name()
					}
				}
			}
		}

		for ident, obj := range pkg.TypesInfo.Defs {
			if obj == nil {
				continue
			}

			pos := pkg.Fset.Position(ident.Pos())
			if strings.HasSuffix(pos.Filename, "_test.go") {
				continue
			}

			var unitType string
			switch t := obj.(type) {
			case *types.TypeName:
				if t.IsAlias() {
					unitType = FIELD
				} else {
					unitType = CLASS
				}
			case *types.Func:
				unitType = FUNCTION
			case *types.Var:
				v := obj.(*types.Var)
				if v.IsField() || v.Parent() == pkg.Types.Scope() {
					unitType = FIELD
				} else {
					continue
				}
			case *types.Const:
				unitType = FIELD
			default:
				continue
			}

			fqn := getObjectFQN(pkg.Types, obj, parentMap)
			unit := &schema.CodeUnitUsages{
				FullyQualifiedName:    fqn,
				DeclarationLineNumber: pos.Line,
				Type:                  unitType,
				Usages:                []schema.UsageLocation{},
			}
			objToUnit[obj] = &unitMeta{
				unit:   unit,
				usages: make(map[string]schema.UsageLocation),
			}
		}
	}

	// Phase 2: Collect Usages (including tests)
	for _, pkg := range pkgs {
		for _, file := range pkg.Syntax {
			var currentContextFQN string

			ast.Inspect(file, func(n ast.Node) bool {
				switch node := n.(type) {
				case *ast.FuncDecl:
					if obj, ok := pkg.TypesInfo.Defs[node.Name]; ok && obj != nil {
						currentContextFQN = getObjectFQN(pkg.Types, obj, nil)
					}
				case *ast.GenDecl:
					if node.Tok == token.VAR || node.Tok == token.CONST {
						for _, spec := range node.Specs {
							if vs, ok := spec.(*ast.ValueSpec); ok {
								for _, name := range vs.Names {
									if obj, ok := pkg.TypesInfo.Defs[name]; ok && obj != nil {
										currentContextFQN = getObjectFQN(pkg.Types, obj, nil)
										break // Use first variable in block as context
									}
								}
							}
						}
					}
				case *ast.Ident:
					if obj, ok := pkg.TypesInfo.Uses[node]; ok && obj != nil {
						if meta, exists := objToUnit[obj]; exists {
							pos := pkg.Fset.Position(node.Pos())
							locKey := fmt.Sprintf("%s:%d", pos.Filename, pos.Line)

							if _, seen := meta.usages[locKey]; !seen {
								contextFQN := currentContextFQN
								if contextFQN == "" {
									contextFQN = pkg.Name
								}

								meta.usages[locKey] = schema.UsageLocation{
									FullyQualifiedName: contextFQN,
									LineNumber:         pos.Line,
									Snippet:            captureSnippet(pos.Filename, pos.Line),
									FilePath:           pos.Filename,
									SyntaxStyle:        "go",
								}
							}
						}
					}
				}
				return true
			})
		}
	}

	var result []schema.CodeUnitUsages
	for _, meta := range objToUnit {
		for _, loc := range meta.usages {
			meta.unit.Usages = append(meta.unit.Usages, loc)
		}
		result = append(result, *meta.unit)
	}

	return &schema.ProgramUsages{CodeUnits: result}, nil
}
