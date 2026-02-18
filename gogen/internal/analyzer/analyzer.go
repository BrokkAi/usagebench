package analyzer

import (
	"bufio"
	"fmt"
	"go/types"
	"os"
	"strings"

	"gogen/internal/schema"
	"golang.org/x/tools/go/packages"
)

const (
	CLASS    = "CLASS"
	FUNCTION = "FUNCTION"
	FIELD    = "FIELD"
)

func getObjectFQN(pkgPath string, obj types.Object) string {
	switch t := obj.(type) {
	case *types.Func:
		sig := t.Type().(*types.Signature)
		if sig.Recv() != nil {
			recvType := sig.Recv().Type()
			if ptr, ok := recvType.(*types.Pointer); ok {
				recvType = ptr.Elem()
			}
			if named, ok := recvType.(*types.Named); ok {
				return pkgPath + "." + named.Obj().Name() + "." + obj.Name()
			}
		}
	case *types.Var:
		if t.IsField() {
			// In Go, fields are often accessed via the struct. 
			// For FQN consistency with the scala logic, we'd ideally want Type.Field.
			// This is a simplified version.
			return pkgPath + "." + obj.Name()
		}
	}
	return pkgPath + "." + obj.Name()
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

func Analyze(projectPath string) (*schema.ProgramUsages, error) {
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
		for ident, obj := range pkg.TypesInfo.Defs {
			if obj == nil {
				continue
			}

			pos := pkg.Fset.Position(ident.Pos())
			if strings.HasSuffix(pos.Filename, "_test.go") {
				continue
			}

			var unitType string
			switch obj.(type) {
			case *types.TypeName:
				unitType = CLASS
			case *types.Func:
				unitType = FUNCTION
			case *types.Var:
				t := obj.(*types.Var)
				if t.IsField() || t.Parent() == pkg.Types.Scope() {
					unitType = FIELD
				} else {
					continue
				}
			case *types.Const:
				unitType = FIELD
			default:
				continue
			}

			fqn := getObjectFQN(pkg.PkgPath, obj)
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
		for ident, obj := range pkg.TypesInfo.Uses {
			if obj == nil {
				continue
			}

			// If this object is one of our tracked code units
			if meta, ok := objToUnit[obj]; ok {
				pos := pkg.Fset.Position(ident.Pos())
				
				// Generate a key for uniqueness: file:line
				locKey := fmt.Sprintf("%s:%d", pos.Filename, pos.Line)
				if _, exists := meta.usages[locKey]; !exists {
					// Determine enclosing context name (FQN of the function/type containing the usage)
					// For simplicity in this pass, we use the filename as context if scope traversal is deep,
					// or resolve the nearest enclosing object if available.
					
					meta.usages[locKey] = schema.UsageLocation{
						FullyQualifiedName: pkg.PkgPath, // Simplified: should ideally find enclosing func
						LineNumber:         pos.Line,
						Snippet:            captureSnippet(pos.Filename, pos.Line),
						FilePath:           pos.Filename,
						SyntaxStyle:        "go",
					}
				}
			}
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
