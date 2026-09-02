---
title: Reproduce the comparison
description: Rebuild a versioned reference environment, rerun a published report offline, and compare it semantically.
---

## Canonical reproduction

Reference environment version 1 fixes the canonical platform at
`linux/amd64` and covers Bifrost and gopls. Given a published canonical report,
the release bundle can reproduce it with one command:

```bash
./scripts/reproduce-report.sh published-report.json reproduced-report.json
```

The command reads the exact UsageBench release and revision, environment
version, runner, case selection, and inclusion policy from the report. It then
builds the matching local image, reruns with networking disabled, and invokes
the semantic comparator inside the image.

A successful reproduction ends with:

```text
reports are semantically equivalent
```

The comparator ignores timestamps, temporary workspace roots, local paths,
and the locally rebuilt image identity. It still requires matching release and
revision provenance, environment definition, executable checksum, requested
and resolved analyzer versions, capabilities, locations, diagnostics, case
outcomes, and totals.

## Checksum-addressed distribution

Trusted `main` CI publishes canonical images to
`ghcr.io/brokkai/usagebench-reference`. The lookup tag is the SHA-256 of the
runner, UsageBench release and revision, environment definition digest,
analyzer identity, and canonical platform. The script resolves that tag to an
immutable registry digest, pulls by digest, and verifies all identity labels
plus the loaded image ID before reuse. A mutable tag is never publication
evidence by itself.

Image construction needs network access to retrieve digest-pinned bases and
checksum-protected analyzer inputs. Benchmark execution itself uses
`--network none`, runs as a non-root user, mounts the released corpus read-only,
and writes only to isolated work and private output staging. The wrapper copies
only the completed report to its requested host path.

The full reviewer procedure, resource expectations, integrity boundaries, and
troubleshooting guidance are in the repository's `ARTIFACT.md`.

## Direct inspection

Restore or build either version 1 image using the release tag recorded in a report:

```bash
./scripts/reference-image.sh bifrost vMAJOR.MINOR.PATCH
./scripts/reference-image.sh gopls vMAJOR.MINOR.PATCH
```

The scripts write local metadata under `target/reference/`. To validate the
recipe itself without local or registry reuse, run:

```bash
USAGEBENCH_REFERENCE_IMAGE_FORCE_REBUILD=1 \
  ./scripts/reference-image.sh bifrost vMAJOR.MINOR.PATCH
```

Forced rebuilds cannot publish. Trusted CI publishes only after authenticating
to GHCR and setting `USAGEBENCH_REFERENCE_IMAGE_PUBLISH=1`. To run a selected
gopls case against an extracted release bundle:

```bash
./scripts/run-reference.sh \
  gopls \
  /path/to/usagebench-vMAJOR.MINOR.PATCH \
  benchmark-output/gopls.json \
  benchmarks/cases/go-baseline.yaml \
  go-package-function-call
```

Every canonical report records `executionMode: container`,
`platformScope: canonical_reference`, the environment digest and locally loaded
image ID, the actual analyzer executable SHA-256, and declared toolchain
versions. The wrapper binds that identity to the corpus release and revision
before executing the immutable local image ID.
Reference metadata also retains `imageResolutionMs`, `imageConstructionMs`,
and `reuseStatus`, keeping local-cache, registry-restore, and actual build time
distinct in freeze and reproduction logs.

## Native development runs

Native runners remain useful for development and are explicitly labeled
`host_specific` in their reports:

```bash
cargo test
cargo run -- validate benchmarks/cases
cargo run -- run-bifrost benchmarks/cases \
  --bifrost-repo ../bifrost \
  --bifrost-commit origin/master \
  --output benchmark-output/bifrost-native.json
cargo run -- run-lsp benchmarks/cases \
  --profile adapters/lsp/rust-analyzer.json \
  --output benchmark-output/rust-analyzer-native.json
```

The Bifrost runner records and applies a 300-second wall-clock budget for each
usage scan by default. For a quicker development pass, override the per-scan
budget with `--scan-usages-max-duration-secs <0-300>`; the effective value is
retained in the report's invocation metadata. The budget is applied through
`BIFROST_MCP_REQUEST_BUDGET_SECS` on the Bifrost server the runner launches:
Bifrost no longer accepts a per-request deadline and leaves deadline policy to
the frontend, which sets it once per server. Raising it matters, because a cold
workspace otherwise falls back to a 4.5-second interactive budget under which a
large repository returns partial usage sets that score as failures. The runner
keeps a backstop deadline a minute longer than the budget so Bifrost reports
incomplete evidence rather than having the call abandoned. Runs with `--output` atomically update a
sibling `*.partial.json` checkpoint after each completed benchmark document;
the requested output path is written only when the full run completes.
Checkpoints identify themselves with `completed: false`, retain the full
`requestedCaseFiles` scope, and cannot be used as completed snapshot or release
evidence.

Keep `--work-dir` stable across focused profiling invocations to reuse an exact
Bifrost build and its verified executable digest. Reuse is invalidated by a
different resolved revision, toolchain, build plan/environment, or executable
metadata/content change. Each report still contains the exact resolved commit
and executable SHA-256. Its `timings` object separates checkout/setup, build,
provenance hashing, workspace readiness, and analyzer query work; timing fields
are diagnostic and do not participate in semantic report comparison.

These commands preserve analyzer and host provenance but do not use the
canonical container environment.

## Published native profiles

Advertised profiles without a canonical image run once during the freeze from
the release-shaped corpus. The freeze validates the release and revision,
checked-in profile checksum, requested and resolved analyzer identity,
executable provenance, selected case coverage, and zero runner errors. The
report and its SHA-256 digest are then bound directly into the freeze manifest.

The execution host needs Bash, jq, Git, Rust, Git LFS, and the command or package
launcher named by each selected profile. Project hydration and runtime-network
requirements are recorded in `adapters/candidates.json`. Roslyn must be allowed
to create its local MSBuild named pipes. Package launchers such as `npx`, `cs`,
and `jdtls` may resolve transitive payloads, so native rows are not a
cryptographic payload attestation or cross-host reproducibility claim.

Apple clangd 21 remains distinct from upstream clangd 22. The candidate
contract requires the server-reported version to begin with
`Apple clangd 21.0.0`, so an upstream clangd executable cannot satisfy that row.
Its profile opens only benchmark query documents and waits for clangd's
per-document file-status state to become `idle` before dispatching benchmark
requests. Query timeouts remain 60 seconds, are logged with request timing and
attribution, and trigger `$/cancelRequest`; they are still publication-blocking
runner errors.

## Evaluation release order

For `real-project-v1`, keep the evidence boundary and execution order explicit:

1. materialize Git LFS, then verify the source lock, preregistered selection,
   typed review records, and adjudication evidence;
2. run
   `cargo run -- validate-evaluation benchmarks/cases/evaluation/real-project-v1`;
3. freeze the evaluation directory with exactly Bifrost, gopls, Pyright, and
   TypeScript LS; the two native LSP profiles each run once from the staged
   release corpus.

The generated result pages verify the manifest's evaluation audit and label the
partition explicitly. They publish descriptive per-profile denominators and
recorded exclusions and replacements. They do not support language-wide or
ecosystem-wide estimates, cross-language ranking, causal defect claims, or
latency, memory, and cold-start claims.

## Evidence scope

The checked-in fixture cases remain a development and diagnosis corpus. All 158
have completed a first human review, but retain `legacy_unattributed`
ground-truth metadata. The separate `real-project-v1` evaluation partition has
36 preregistered, source-locked cases with a qualifying blinded OpenAI and
Anthropic panel and accountable human adjudication. Its analyzer results are
frozen in immutable release
[`v0.2.0`](https://github.com/BrokkAi/usagebench/releases/tag/v0.2.0).
Container reproducibility makes execution repeatable; the hash-bound review
evidence is what qualifies the ground-truth status.

See the [human ground-truth audit](../ground-truth-review/) for the review
procedure and the distinction between reviewed development assertions and a
publishable evaluation partition.

Use `CITATION.cff` for citation metadata and retain the complete JSON reports.
Benchmark release tags, reference-environment versions, the Rust CLI version,
and YAML `schemaVersion` are separate compatibility boundaries.
