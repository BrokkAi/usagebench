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

func TestCrossFileMethodUsage(t *testing.T) {
	tempDir := t.TempDir()

	// 1. Create go.mod
	goMod := `module crossfile
go 1.21
`
	if err := os.WriteFile(filepath.Join(tempDir, "go.mod"), []byte(goMod), 0644); err != nil {
		t.Fatal(err)
	}

	// 2. Create types.go
	typesCode := `package testpkg

type Command struct {
    Name string
}

func (c *Command) MarkFlagFilename(name string) error {
    return nil
}
`
	if err := os.WriteFile(filepath.Join(tempDir, "types.go"), []byte(typesCode), 0644); err != nil {
		t.Fatal(err)
	}

	// 3. Create usage.go
	usageCode := `package testpkg

func UseCommand() {
    cmd := &Command{}
    cmd.MarkFlagFilename("file")
}
`
	if err := os.WriteFile(filepath.Join(tempDir, "usage.go"), []byte(usageCode), 0644); err != nil {
		t.Fatal(err)
	}

	// 4. Run Analyze
	usages, err := Analyze(tempDir)
	if err != nil {
		t.Fatalf("Analyze failed: %v", err)
	}

	// 5. Assertions
	var methodUnit *schema.CodeUnitUsages
	expectedFQN := "testpkg.Command.MarkFlagFilename"

	for _, unit := range usages.CodeUnits {
		if unit.FullyQualifiedName == expectedFQN {
			methodUnit = &unit
			break
		}
	}

	if methodUnit == nil {
		t.Fatalf("Method %s not found in results", expectedFQN)
	}

	if methodUnit.Type != FUNCTION {
		t.Errorf("Expected type FUNCTION for %s, got %s", expectedFQN, methodUnit.Type)
	}

	// Verify usage is detected from usage.go
	foundUsage := false
	for _, usage := range methodUnit.Usages {
		if filepath.Base(usage.FilePath) == "usage.go" {
			foundUsage = true
			// The usage is inside func UseCommand() in testpkg
			expectedUsageFQN := "testpkg.UseCommand"
			if usage.FullyQualifiedName != expectedUsageFQN {
				t.Errorf("Expected usage FQN to be '%s', got '%s'", expectedUsageFQN, usage.FullyQualifiedName)
			}
		}
	}

	if !foundUsage {
		t.Errorf("Expected usage of %s in usage.go was not detected", expectedFQN)
	}
}

func TestInternalTestUsage(t *testing.T) {
	tempDir := t.TempDir()

	// 1. Create go.mod
	goMod := `module internaltest
go 1.21
`
	if err := os.WriteFile(filepath.Join(tempDir, "go.mod"), []byte(goMod), 0644); err != nil {
		t.Fatal(err)
	}

	// 2. Create lib.go
	libCode := `package lib
func Hello() {}
`
	if err := os.WriteFile(filepath.Join(tempDir, "lib.go"), []byte(libCode), 0644); err != nil {
		t.Fatal(err)
	}

	// 3. Create lib_test.go
	testCode := `package lib
import "testing"
func TestHello(t *testing.T) {
    Hello()
}
`
	if err := os.WriteFile(filepath.Join(tempDir, "lib_test.go"), []byte(testCode), 0644); err != nil {
		t.Fatal(err)
	}

	// 4. Run Analyze
	usages, err := Analyze(tempDir)
	if err != nil {
		t.Fatalf("Analyze failed: %v", err)
	}

	// 5. Verify lib.Hello unit exists and has usages
	var helloUnit *schema.CodeUnitUsages
	expectedFQN := "lib.Hello"
	for _, unit := range usages.CodeUnits {
		if unit.FullyQualifiedName == expectedFQN {
			helloUnit = &unit
			break
		}
	}

	if helloUnit == nil {
		t.Fatalf("Code unit %s not found", expectedFQN)
	}

	// 6. Verify usage FQN matches expected context (lib.TestHello)
	foundUsage := false
	expectedUsageContext := "lib.TestHello"
	for _, usage := range helloUnit.Usages {
		if filepath.Base(usage.FilePath) == "lib_test.go" {
			foundUsage = true
			if usage.FullyQualifiedName != expectedUsageContext {
				t.Errorf("Expected usage context %s, got %s", expectedUsageContext, usage.FullyQualifiedName)
			}
		}
	}

	if !foundUsage {
		t.Error("Usage from lib_test.go was not captured")
	}
}
