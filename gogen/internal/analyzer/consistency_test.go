package analyzer

import (
	"os"
	"path/filepath"
	"testing"

	"gogen/internal/schema"
)

func TestAnalyzeConsistency(t *testing.T) {
	tempDir := t.TempDir()

	// 1. Create go.mod
	goMod := `module consistency
go 1.21
`
	if err := os.WriteFile(filepath.Join(tempDir, "go.mod"), []byte(goMod), 0644); err != nil {
		t.Fatal(err)
	}

	// 2. Create consistency.go with a comprehensive set of declarations
	code := `package consistency

// Regular Struct
type MyStruct struct {
    FieldA int // Struct Field
}

// Struct Method
func (s MyStruct) GetFieldA() int { return s.FieldA }

// Interface
type MyInterface interface {
    DoSomething() // Interface Method
}

// Global Var
var GlobalVar = 42

// Global Const
const GlobalConst = "hello"

// Type Alias
type MyAlias = string

// Regular Function
func MyFunc() {}
`
	if err := os.WriteFile(filepath.Join(tempDir, "consistency.go"), []byte(code), 0644); err != nil {
		t.Fatal(err)
	}

	// 3. Run Analyze
	usages, err := Analyze(tempDir)
	if err != nil {
		t.Fatalf("Analyze failed: %v", err)
	}

	// 4. Assertions based on Brokk GoAnalyzer spec
	expected := map[string]string{
		"consistency.MyStruct":           CLASS,    // Regular Struct
		"consistency.MyStruct.GetFieldA": FUNCTION, // Struct Method
		"consistency.MyStruct.FieldA":    FIELD,    // Struct Field
		"consistency.MyInterface":        CLASS,    // Interface
		"consistency.MyInterface.DoSomething": FUNCTION, // Interface Method
		"consistency._module_.GlobalVar":   FIELD,    // Global Var
		"consistency._module_.GlobalConst": FIELD,    // Global Const
		"consistency._module_.MyAlias":     FIELD,    // Type Alias (FIELD per spec)
		"consistency.MyFunc":             FUNCTION, // Regular Function
	}

	foundCount := 0
	for _, unit := range usages.CodeUnits {
		if expectedType, ok := expected[unit.FullyQualifiedName]; ok {
			foundCount++
			if unit.Type != expectedType {
				t.Errorf("FQN %s: expected type %s, got %s", unit.FullyQualifiedName, expectedType, unit.Type)
			}
		}
	}

	if foundCount != len(expected) {
		t.Errorf("Expected %d units to be verified, but only found %d in results", len(expected), foundCount)
		// Log missing to help debug
		for fqn := range expected {
			found := false
			for _, unit := range usages.CodeUnits {
				if unit.FullyQualifiedName == fqn {
					found = true
					break
				}
			}
			if !found {
				t.Errorf("Missing expected FQN: %s", fqn)
			}
		}
	}
}
