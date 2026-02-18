package analyzer

import (
	"go/types"
	"strings"

	"gogen/internal/schema"
	"golang.org/x/tools/go/packages"
)

const (
	CLASS    = "CLASS"
	FUNCTION = "FUNCTION"
	FIELD    = "FIELD"
)

func Analyze(projectPath string) (*schema.ProgramUsages, error) {
	cfg := &packages.Config{
		Mode: packages.NeedName | packages.NeedFiles | packages.NeedSyntax | packages.NeedTypes | packages.NeedTypesInfo,
		Dir:  projectPath,
	}

	pkgs, err := packages.Load(cfg, "./...")
	if err != nil {
		return nil, err
	}

	var codeUnits []schema.CodeUnitUsages

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
			var fqn string

			switch t := obj.(type) {
			case *types.TypeName:
				unitType = CLASS
				fqn = pkg.PkgPath + "." + obj.Name()
			case *types.Func:
				unitType = FUNCTION
				sig := t.Type().(*types.Signature)
				if sig.Recv() != nil {
					recvType := sig.Recv().Type()
					if ptr, ok := recvType.(*types.Pointer); ok {
						recvType = ptr.Elem()
					}
					if named, ok := recvType.(*types.Named); ok {
						fqn = pkg.PkgPath + "." + named.Obj().Name() + "." + obj.Name()
					} else {
						fqn = pkg.PkgPath + "." + obj.Name()
					}
				} else {
					fqn = pkg.PkgPath + "." + obj.Name()
				}
			case *types.Var:
				if t.IsField() || t.Parent() == pkg.Types.Scope() {
					unitType = FIELD
					// Simplification for fields: usually we want Type.Field, 
					// but for this pass pkgpath.Name is a good start or pkgpath.Type.Name
					fqn = pkg.PkgPath + "." + obj.Name()
				} else {
					continue
				}
			case *types.Const:
				unitType = FIELD
				fqn = pkg.PkgPath + "." + obj.Name()
			default:
				continue
			}

			codeUnits = append(codeUnits, schema.CodeUnitUsages{
				FullyQualifiedName:    fqn,
				DeclarationLineNumber: pos.Line,
				Type:                  unitType,
				Usages:                []schema.UsageLocation{},
			})
		}
	}

	return &schema.ProgramUsages{CodeUnits: codeUnits}, nil
}
