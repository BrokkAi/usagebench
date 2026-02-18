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

## Development

If you encounter dependency errors or missing `go.sum` entries, run:
```bash
cd gogen && go mod tidy
```

### Build
To build the project:
```bash
cd gogen && go build .
```

### Test
To run all tests:
```bash
cd gogen && go test ./...
```

To run a specific test:
```bash
cd gogen && go test -run <TestNameRegex> ./...
```

### Build
To build the project:
```bash
cd gogen && go build .
```

### Test
To run all tests:
```bash
cd gogen && go test ./...
```

To run a specific test:
```bash
cd gogen && go test -run <TestNameRegex> ./...
```
