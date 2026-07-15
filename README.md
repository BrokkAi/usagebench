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
* `src`: Rust validation CLI, schema model, and analyzer runner adapters.
* `adapters/lsp`: Versioned language-server profiles and reproduction notes.
* `docs/runner-adapters.md`: Adapter contract, version policy, and peer-tool
  feasibility notes.

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

The corpus uses small checked-in fixtures for Java, Go, Python, TypeScript,
JavaScript, Rust, Scala, C#, PHP, C++, and Ruby. These fixtures are the source
of truth for issue #8; the older broad
Java/Go/Python generator stack has been removed from the active benchmark path.

Each fixture case records `verification.method: manual_inspection` with a short
note explaining how the expected declaration and usage locations were checked.

## Analyzer Runners

Runner adapters under `src/runners` translate tool-specific output into one
analyzer-neutral report shape. Every report records the requested and resolved
tool version plus per-operation capability levels (`native`, `recovered`, or
`unsupported`). Reports distinguish exact passes, complete-superset near
misses, hard failures, and runner errors. Print the JSON Schema with:

```bash
cargo run -- report-schema
```

The existing Bifrost command remains stable:

```bash
cargo run -- run-bifrost benchmarks/cases \
  --bifrost-repo /path/to/bifrost \
  --bifrost-working-tree
```

The Repowise adapter is intentionally pinned to the one verified response
contract, v0.31.0. By default it uses `uvx` to install/cache and run that exact
release, creates isolated source copies, disables telemetry and global editor
registration, and removes its index copies after the run. A version-specific
Python hook observes Repowise's `CallResolver` output before individual calls
are collapsed into graph edges:

```bash
cargo run -- run-repowise benchmarks/cases \
  --repowise-version 0.31.0 \
  --include-unsupported \
  --output benchmark-output/repowise-v0.31.0.json
```

Use `--repowise-command /path/to/repowise` for an existing executable; its
reported version must still match and the command must honor Python's
`sitecustomize` hook. Resolved call sites at Repowise's public confidence floor
are scored normally; lower-confidence calls remain unproven. Mixed or non-call
references and type lookups are reported as unsupported. See
[runner adapter details](docs/runner-adapters.md) for the evidence, full
language probe, and capability boundary.

The generic LSP adapter starts a versioned stdio language server, opens an
isolated fixture workspace, and translates the protocol's native references,
definition, and type-definition responses into the same report:

```bash
cargo run -- run-lsp benchmarks/cases \
  --profile adapters/lsp/gopls.json \
  --output benchmark-output/gopls-v0.23.0.json
```

Profiles cover all eleven corpus languages through clangd, Roslyn and
csharp-ls, gopls, Eclipse JDT LS, Pyright, Ruby LSP, Metals, Intelephense,
rust-analyzer, and typescript-language-server. The executable named by a
profile must be installed or available through the profile's package launcher;
`--server-command` can override only the executable while preserving its
arguments. See [the LSP profile guide](adapters/lsp/README.md) for setup and
the measured comparison.

## Daily Bifrost Benchmark

The daily GitHub Actions workflow in `.github/workflows/benchmark.yml` runs the
curated corpus against Bifrost on `ubuntu-latest`.

The workflow:

* validates `benchmarks/cases`
* checks out `BrokkAi/bifrost`
* builds `usagebench`
* runs `usagebench run-bifrost benchmarks/cases`
* uploads the JSON report from `benchmark-output`
* publishes a GitHub step summary
* optionally posts a payload to Slack

Scheduled runs use Bifrost `master`. Manual `workflow_dispatch` runs can set a
specific `bifrost_ref` and can opt into cases marked `unsupported`.

Without `--bifrost-working-tree`, `run-bifrost` creates an isolated checkout
under `target/usagebench` and checks out `--bifrost-commit`.

If the default `GITHUB_TOKEN` cannot read `BrokkAi/bifrost`, configure a
repository secret named `BIFROST_CHECKOUT_TOKEN` with read access to that repo.

Slack delivery is best-effort and does not change the benchmark result. To
enable it, configure the repository secret
`SLACK_DAILY_USAGEBENCH_WEBHOOK_URL`. The workflow sends a benchmark-specific
payload with:

* `ok`
* `error_text`
* `workflow_run_url`
* `head_sha_short`
* `bifrost_ref`
* `bifrost_sha_short`
* `run_outcome`
* `cases_count`
* `passed_count`
* `improved_count`
* `total_passed_count`
* `failed_count`
* `expected_failures_count`
* `not_planned_count`
* `unsupported_count`
* `skipped_count`
* `errors_count`
