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

The initial corpus uses small checked-in fixtures for the Bifrost-covered
language set: Java, Go, Python, TypeScript, JavaScript, Rust, Scala, C#, PHP,
and C++. These fixtures are the source of truth for issue #8; the older broad
Java/Go/Python generator stack has been removed from the active benchmark path.

Each fixture case records `verification.method: manual_inspection` with a short
note explaining how the expected declaration and usage locations were checked.

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

For local Bifrost changes, run the benchmark against the checkout directly:

```bash
cargo run -- run-bifrost benchmarks/cases \
  --bifrost-repo /path/to/bifrost \
  --bifrost-working-tree
```

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
* `failed_count`
* `expected_failures_count`
* `skipped_count`
* `errors_count`
