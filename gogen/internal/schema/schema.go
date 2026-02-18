package schema

// ProgramUsages represents the top-level structure for code usage analysis.
type ProgramUsages struct {
	CodeUnits []CodeUnitUsages `json:"codeUnits"`
}

// CodeUnitUsages represents a single code unit (Class, Function, Field) and its usages.
type CodeUnitUsages struct {
	FullyQualifiedName    string          `json:"fullyQualifiedName"`
	DeclarationLineNumber int             `json:"declarationLineNumber"`
	Type                  string          `json:"type"`
	Usages                []UsageLocation `json:"usages"`
}

// UsageLocation represents a specific location where a code unit is used.
type UsageLocation struct {
	FullyQualifiedName string `json:"fullyQualifiedName"`
	LineNumber         int    `json:"lineNumber"`
	Snippet            string `json:"snippet"`
	FilePath           string `json:"filePath"`
	SyntaxStyle        string `json:"syntaxStyle"`
}
