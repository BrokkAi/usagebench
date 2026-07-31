# usagebench

UsageBench is Bifrost's curated LSP-parity and recurring regression suite for
the static-analysis task of discovering usages of source symbols.

The Starlight site under [`docs/`](docs/README.md) explains the comparison
methodology, current Bifrost-versus-LSP results, and case-level language
differences.

Mature language servers provide the baseline and calibration evidence. Bifrost
is expected to match them where their behavior agrees with reviewed language
semantics, and may preserve justified precision improvements or additional
static coverage. The benchmark format remains analyzer-neutral so future
competitors can be evaluated against the same source contracts.

LSPs primarily serve developers through interactive editor features. Bifrost
serves repository code analysis and navigation for machine consumers, especially
coding agents and static-analysis tools. UsageBench measures their overlapping
usage and navigation contract: parity preserves the quality developers expect,
while reviewed Bifrost-specific edges exercise the analysis substrate agents
depend on. It does not compare either product's complete feature surface.

The synchronized 24 July 2026 development run compares the 131 cases scoreable
by both sides: Bifrost is exact on 116 and the reference language servers on 95.
Both are exact on 84 cases; 32 are exact only for Bifrost, 11 only for the LSP,
and 4 for neither. See the
[current result](docs/src/content/docs/results/index.md) for the full
denominators, capability boundaries, versions, and evidence limitations.

The benchmark corpus is authored by source location instead of by an
analyzer-specific symbol ID. Each case points at a declaration, expected usage
sites, and reverse usage-to-declaration probes using LSP-shaped ranges.

## Immutable result pages

Benchmark releases package each raw report, a manifest with its SHA-256 digest,
and generated result fragments. Generate (or check) those fragments from an
extracted release evidence directory with:

```bash
cargo run -- generate-results \
  --manifest evidence/freeze-manifest.json \
  --output-directory generated-results

cargo run -- generate-results \
  --manifest evidence/freeze-manifest.json \
  --output-directory generated-results \
  --check
```

The generator rejects missing, altered, mismatched, or non-snapshot reports
before it writes a page. Evaluation pages additionally verify the freeze ID,
bounded claim scope, artifact provenance, denominators, exclusions, and
replacements, and cannot mix development documents into an evaluation claim.
Location-level precision, recall, and range-quality tables require reports
produced by UsageBench 0.2.0 or newer; older reports remain readable for their
existing strict and required-destination fields but are never interpreted as
having zero location metrics. The current hand-authored 24 July summary
predates the release evidence pipeline; `real-project-v1` analyzer results have
not yet been published.

## Directory Structure

* `benchmarks`: Authored benchmark case files and corpus documentation.
* `fixtures`: Small in-repository source corpora used by the baseline cases.
* `schema`: JSON Schema for benchmark case documents.
* `src`: Rust validation CLI, schema model, and analyzer runner adapters.
* `adapters/lsp`: Versioned language-server profiles and reproduction notes.
* `containers/reference`: Versioned, digest-pinned reference environments.
* `scripts`: Local image build, offline execution, and report reproduction tools.
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

### Git LFS

The pinned real-project evaluation source archives under
`benchmarks/evaluation/real-project-v1/sources/` are tracked with Git LFS. A
checkout must materialize those objects before validating or running the full
corpus:

```bash
git lfs install
git lfs pull
```

GitHub workflows that consume the full corpus enable LFS during checkout.
Self-hosted runners used by the native-reproduction workflow must have the
`git-lfs` executable installed.

## Development Corpus

The current corpus uses small checked-in fixtures for Java, Go, Python, TypeScript,
JavaScript, Rust, Scala, C#, PHP, C++, and Ruby. These fixtures are the source
of truth for issue #8; the older broad
Java/Go/Python generator stack has been removed from the active benchmark path.

Each document is schema v2 and explicitly labeled `development`,
`analyzer_informed`, and `legacy_unattributed`. All 158 current cases across 35
documents have completed a first human review, preserved in
[`benchmarks/reviews/2026-07-17-DavidBakerEffendi.md`](benchmarks/reviews/2026-07-17-DavidBakerEffendi.md).
This remains a regression and diagnosis corpus, not an independently reviewed
evaluation partition: promotion still requires a second reviewer,
preregistered selection, and a freeze ID.

## Evaluation Corpus

`benchmarks/cases/evaluation/real-project-v1` is a separate, preregistered and
independently reviewed slice: four repositories and three declarations for each
of Go/gopls, Python/Pyright, and TypeScript/TypeScript LS, for 12 repositories
and 36 cases. Its source archives, selection, review, adjudication, and source
lock are checked in and hash-bound. Its analyzer results have not yet been
published. Any eventual aggregate is evaluation-only and limited to the
protocol's descriptive per-profile claim; it is not pooled with development
fixtures or presented as language-wide, ecosystem-wide, causal, or performance
evidence.

## Analyzer Runners

Runner adapters under `src/runners` translate tool-specific output into one
analyzer-neutral report shape. Every report records the requested and resolved
tool version plus per-operation capability levels (`native`, `recovered`, or
`unsupported`). Reports distinguish exact passes, position-unverified results,
hard failures, and runner errors, and separate development from evaluation
totals. Print the JSON Schema with:

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

The primary public profile registry distinguishes canonical reference runners
from independently reproduced native profiles. Bifrost and gopls use the
canonical `linux/amd64` environment. The remaining advertised LSP rows require
equivalent reports from two documented hosts before generated result pages will
include them. Apple clangd 21 and upstream clangd 22 are separate candidate
identities.

Release maintainers produce those pairs with the manual **Native two-host
reproduction** workflow on two distinct pre-provisioned self-hosted runners,
then pass its verified run ID to the freeze workflow.

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
- the logical invocation, native or container platform scope, analyzer
  executable checksum, toolchains, reference-environment definition, and
  runner's requested and resolved version.

Reference environment version 1 provides the canonical `linux/amd64` path for
Bifrost and gopls. Save a published container report as `report.json`, extract
the release bundle named by the report, and run one command:

```bash
./scripts/reproduce-report.sh report.json reproduced.json
```

The command builds the recorded environment locally, reruns without network
access, and compares the reports semantically. The project intentionally does
not publish images to GHCR or promise a ready-built image; release bundles
contain everything needed to build them. See [`ARTIFACT.md`](ARTIFACT.md) for
the artifact-review procedure, security boundary, expected build cost, and
manual commands. Native runner commands remain available for development but
are labeled host-specific and are not the canonical reproducibility claim.

If `usagebenchRevision` ends in `-dirty` or `usagebenchRelease` is absent, the
report identifies a development run and is rejected by the canonical
reproduction command.

UsageBench is licensed under the permissive [MIT License](LICENSE.md), covering
the corpus fixtures, assertions, adapter profiles, and harness code in this
repository.

## Recurring Bifrost Regression Run

The daily GitHub Actions workflow in `.github/workflows/benchmark.yml` runs the
curated corpus against Bifrost `master` on `ubuntu-latest`. This makes every
reviewed parity decision, precision edge, and known gap part of a recurring
regression signal rather than a one-time comparison.

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
