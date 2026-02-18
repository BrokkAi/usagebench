# GoGen

A tool to extract usages from Go source code.

## Requirements

* Go 1.21+
* Git

## Getting Started

```
go build -o gogen
./gogen --help
Usage of ./gogen:
  -input string
    	Input directory of a Go project or CSV file of Git repositories ('repoUrl,commitSha')
  -output string
    	Output directory (default "./gogen_output")
```

The tool accepts either a local directory containing Go source code or a CSV file containing a list of Git repositories and specific commit hashes to analyze.
