# usagebench Agent Notes

`usagebench` is a Rust CLI and curated benchmark corpus for evaluating static
analysis usage lookup. The active benchmark format is analyzer-neutral and
source-location based: cases point at declarations and usages with
LSP-shaped ranges rather than analyzer-specific symbol IDs.

## Repository Layout

* `src/` contains the Rust CLI, schema model, validator, and Bifrost runner.
* `benchmarks/cases/` contains authored YAML benchmark case documents.
* `benchmarks/README.md` documents the YAML format and authoring rules.
* `fixtures/` contains small checked-in source corpora used by baseline cases.
* `schema/benchmark-case.schema.json` is the checked-in JSON Schema contract.
* `docs/` contains execution plans and design notes.

The old broad generator paths such as `pygen`, `javagen`, and `gogen` are not
part of the active benchmark path. If they appear in a local checkout, treat
them as legacy or local artifacts unless the task explicitly asks about them.

## Common Commands

Run the normal validation path before considering benchmark edits complete:

```bash
cargo test
cargo run -- validate benchmarks/cases
```

For Rust code edits, also run formatting:

```bash
cargo fmt
```

The CI workflow runs `cargo test` and `cargo run -- validate benchmarks/cases`.
There is no Gradle build in this repository.

Useful CLI entry points:

```bash
cargo run -- schema
cargo run -- bifrost-report-schema
cargo run -- run-bifrost benchmarks/cases --bifrost-repo ../bifrost
```

Use `run-bifrost` when changing Bifrost execution, result normalization, or
scoring behavior. It may fetch, build, or create temporary worktrees under
`target/usagebench`.

## Benchmark Case Authoring

* Use portable `benchmark://source/...` URIs, never checkout-specific absolute
  paths.
* Ranges are LSP-shaped and zero-based; range end positions are exclusive.
* `positionEncoding` defaults to `utf-16`, matching LSP defaults.
* Fixture-backed cases use `source.kind: fixture` and a `source.path` resolved
  relative to the repository root.
* Expected locations should be verified against checked-in fixture source and
  recorded with `verification.method: manual_inspection`.
* Non-zero fixture ranges should select text equal to the location's
  `displayName`.
* Use `allowedExtraUsages`, `expectedFailure`, and `unsupported` to document
  known analyzer behavior without weakening the source-location contract.

Keep benchmark cases small and reviewed. Prefer adding focused fixture source
over reviving generator-heavy corpora.

## Rust Style

Follow the existing Rust style in `src/`:

* Use `anyhow::Context` for errors that cross file, process, or analyzer
  boundaries.
* Keep validation failures precise and tied to the case file or source URI.
* Prefer structured model changes in `src/lib.rs` plus schema updates over
  ad hoc YAML handling.
* Keep CLI output stable enough for CI and automation consumers.
* Avoid broad refactors when adjusting benchmark data or runner behavior.

