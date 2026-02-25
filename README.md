# usagebench

Repository for creating and running benchmarks around the static analysis task of discovering usages of certain code units.

## Directory Structure

* `javagen`: A Joern-based Java usage extractor.
* `gogen`: A Go-based usage extractor.

## Generating Java Usages

1. Build `javagen`:
   ```
   cd javagen
   sbt stage
   ```
2. Build Java usages from `dataset/java_repositories.csv`
   ```
   ./javagen/javagen dataset/java_repositories.csv dataset/java
   ```

## Generating Go Usages

1. Build `gogen`:
   ```
   cd gogen
   go build -o gogen
   ```
2. Build Go usages from `dataset/go_repositories.csv`
   ```
   ./gogen/gogen --input dataset/go_repositories.csv --output dataset/go
   ```

## Generating Python Usages

1. Install dependencies:
   ```bash
   cd pygen
   uv sync
   ```
2. Build Python usages from `dataset/python_repositories.csv`:
   ```bash
   uv run main.py ../dataset/python_repositories.csv ../dataset/python
   ```