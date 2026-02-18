package analyzer

import (
	"os"
	"path/filepath"
	"testing"
)

func TestAnalyze(t *testing.T) {
	tempDir := t.TempDir()

	// 1. Create go.mod
	goMod := `module testproj
go 1.21
`
	if err := os.WriteFile(filepath.Join(tempDir, "go.mod"), []byte(goMod), 0644); err != nil {
		t.Fatal(err)
	}

	// 2. Create lib/lib.go
	libDir := filepath.Join(tempDir, "lib")
	if err := os.Mkdir(libDir, 0755); err != nil {
		t.Fatal(err)
	}
	libGo := `package lib
type Data struct {
    Value int
}
func DoWork(d Data) int {
    return d.Value
}
`
	if err := os.WriteFile(filepath.Join(libDir, "lib.go"), []byte(libGo), 0644); err != nil {
		t.Fatal(err)
	}

	// 3. Create main.go
	mainGo := `package main
import "testproj/lib"
func main() {
    d := lib.Data{Value: 42}
    lib.DoWork(d)
}
`
	if err := os.WriteFile(filepath.Join(tempDir, "main.go"), []byte(mainGo), 0644); err != nil {
		t.Fatal(err)
	}

	// 4. Run Analyze
	usages, err := Analyze(tempDir)
	if err != nil {
		t.Fatalf("Analyze failed: %v", err)
	}

	// 5. Assertions
	foundData := false
	foundDoWork := false
	foundValue := false

	for _, unit := range usages.CodeUnits {
		switch unit.FullyQualifiedName {
		case "testproj/lib.Data":
			foundData = true
			if unit.Type != CLASS {
				t.Errorf("Expected Type CLASS for lib.Data, got %s", unit.Type)
			}
			if !hasUsage(unit.Usages, "testproj") {
				t.Errorf("Expected usage of lib.Data in testproj/main")
			}

		case "testproj/lib.DoWork":
			foundDoWork = true
			if unit.Type != FUNCTION {
				t.Errorf("Expected Type FUNCTION for lib.DoWork, got %s", unit.Type)
			}
			if !hasUsage(unit.Usages, "testproj") {
				t.Errorf("Expected usage of lib.DoWork in testproj/main")
			}

		case "testproj/lib.Value": // Based on current getObjectFQN implementation for fields
			foundValue = true
			if unit.Type != FIELD {
				t.Errorf("Expected Type FIELD for lib.Value, got %s", unit.Type)
			}
			if !hasUsage(unit.Usages, "testproj") {
				t.Errorf("Expected usage of lib.Value in testproj/main")
			}
		}
	}

	if !foundData {
		t.Error("testproj/lib.Data not found in results")
	}
	if !foundDoWork {
		t.Error("testproj/lib.DoWork not found in results")
	}
	if !foundValue {
		t.Error("testproj/lib.Value field not found in results")
	}
}

func hasUsage(usages []struct {
	FullyQualifiedName string `json:"fullyQualifiedName"`
	LineNumber         int    `json:"lineNumber"`
	Snippet            string `json:"snippet"`
	FilePath           string `json:"filePath"`
	SyntaxStyle        string `json:"syntaxStyle"`
}, pkgPath string) bool {
	for _, u := range usages {
		if u.FullyQualifiedName == pkgPath {
			return true
		}
	}
	return false
}
