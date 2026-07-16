# usagebench

Repository for curated benchmarks around the static analysis task of
discovering usages of source symbols.

The Starlight site under [`docs/`](docs/README.md) explains the comparison
methodology, current Bifrost-versus-LSP results, and case-level language
differences.

The benchmark corpus is authored by source location instead of by an
analyzer-specific symbol ID. Each case points at a declaration, expected usage
sites, and reverse usage-to-declaration probes using LSP-shaped ranges.

## Directory Structure

* `benchmarks`: Authored benchmark case files and corpus documentation.
* `fixtures`: Small in-repository source corpora used by the baseline cases.
* `schema`: JSON Schema for benchmark case documents.
* `src`: Rust validation CLI, schema model, and analyzer runner adapters.
* `adapters/lsp`: Versioned language-server profiles and reproduction notes.
* `docs`: Public Starlight content plus adapter design notes and execution plans.

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
`unsupported`). Reports distinguish exact passes, classified policy-only near
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

UsageBench deliberately stays focused on the LSP-shaped task of finding symbol
references and navigating those references back to declarations and types.
Broader analysis contracts should use sibling suites—for example, a future
`callbench` for call-graph resolution and `taintbench` for taint-flow analysis—
so each benchmark can model its own ground truth without tool-specific private
hooks or weakened semantics.

## Releases, Citation, and License

Benchmark corpus releases use immutable SemVer tags such as `v0.1.0`. The
benchmark release version is independent from the Rust package version in
`Cargo.toml` and the benchmark document `schemaVersion`. See
[`RELEASES.md`](RELEASES.md) for the version policy and curated release contents.

Use the root [`CITATION.cff`](CITATION.cff) when citing UsageBench. It describes
the latest release and intentionally contains no placeholder DOI. If an archival
service assigns a DOI later, the real version-specific identifier will be added
to the citation file and that release's notes.

A run report records:

- `usagebenchVersion`: the Rust CLI and adapter implementation version;
- `usagebenchRevision`: the exact UsageBench commit, with `-dirty` when local
  changes prevent commit-only reproduction;
- `usagebenchRelease`: the `vMAJOR.MINOR.PATCH` corpus tag when available; and
- the runner's requested and resolved version, including
  `bifrostResolvedCommit` for Bifrost runs.

To reproduce a clean published Bifrost result, save its JSON report as
`report.json` and run:

```bash
usagebench_ref="$(jq -r '.usagebenchRelease // .usagebenchRevision' report.json)"
bifrost_ref="$(jq -r '.bifrostResolvedCommit' report.json)"
git clone https://github.com/BrokkAi/usagebench.git
git -C usagebench checkout --detach "$usagebench_ref"
git clone https://github.com/BrokkAi/bifrost.git
git -C bifrost checkout --detach "$bifrost_ref"
cargo run --manifest-path usagebench/Cargo.toml -- run-bifrost \
  usagebench/benchmarks/cases \
  --bifrost-repo ../bifrost \
  --bifrost-working-tree \
  --output benchmark-output/reproduced.json
```

If `usagebenchRevision` ends in `-dirty`, the report identifies an uncommitted
run and cannot be reproduced from the named commit alone.

UsageBench is licensed under the permissive [MIT License](LICENSE.md), covering
the corpus fixtures, assertions, adapter profiles, and harness code in this
repository.

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
* `usagebench_revision`
* `usagebench_release`
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
