# usagebench

Repository for curated benchmarks around the static analysis task of
discovering usages of source symbols.

The benchmark corpus is authored by source location instead of by an
analyzer-specific symbol ID. Each case points at a declaration, expected usage
sites, and reverse usage-to-declaration probes using LSP-shaped ranges.

## Directory Structure

* `benchmarks`: Authored benchmark case files and corpus documentation.
* `fixtures`: Small in-repository source corpora used by the baseline cases.
* `schema`: JSON Schema for benchmark case documents.
* `src`: Rust validation CLI and schema model.

## Validating Benchmark Cases

Benchmark cases use YAML authored around an LSP-shaped location model. Validate
them with:

```bash
cargo run -- validate benchmarks/cases
```

CI runs the same Rust test and validation path:

```bash
cargo test
cargo run -- validate benchmarks/cases
```

## Baseline Corpus

The initial corpus uses small checked-in fixtures for Java, Go, Python, and
TypeScript. These fixtures are the source of truth for issue #8; the older broad
Java/Go/Python generator stack has been removed from the active benchmark path.

Each fixture case records `verification.method: manual_inspection` with a short
note explaining how the expected declaration and usage locations were checked.
