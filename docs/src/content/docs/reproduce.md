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

## Build-only distribution

UsageBench checks in the complete image definitions but does not publish
ready-built images to GHCR. CI builds and smoke-tests both reference images
ephemerally without a registry login, push, OCI export, or image artifact
upload. A future archival release may place reviewed OCI archives on Zenodo.

Image construction needs network access to retrieve digest-pinned bases and
checksum-protected analyzer inputs. Benchmark execution itself uses
`--network none`, runs as a non-root user, mounts the released corpus read-only,
and writes only to isolated work and private output staging. The wrapper copies
only the completed report to its requested host path.

The full reviewer procedure, resource expectations, integrity boundaries, and
troubleshooting guidance are in the repository's `ARTIFACT.md`.

## Direct inspection

Build either version 1 image using the release tag recorded in a report:

```bash
./scripts/reference-image.sh bifrost vMAJOR.MINOR.PATCH
./scripts/reference-image.sh gopls vMAJOR.MINOR.PATCH
```

The scripts write local metadata under `target/reference/` and never push an
image. To run a selected gopls case against an extracted release bundle:

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

These commands preserve analyzer and host provenance but are not the canonical
cross-machine reproducibility claim.

## Evidence scope

The current checked-in cases are a development and diagnosis corpus. All 158
cases have completed a first human review, but every document intentionally
retains `legacy_unattributed` ground-truth metadata pending a second independent
review and preregistered freeze. Container reproducibility makes execution
repeatable; it does not upgrade the review status of the expected locations.

See the [human ground-truth audit](../ground-truth-review/) for the review
procedure and the distinction between reviewed development assertions and a
publishable evaluation partition.

Use `CITATION.cff` for citation metadata and retain the complete JSON reports.
Benchmark release tags, reference-environment versions, the Rust CLI version,
and YAML `schemaVersion` are separate compatibility boundaries.
