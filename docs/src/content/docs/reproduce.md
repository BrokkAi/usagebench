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
and the locally rebuilt OCI digest. It still requires matching release and
revision provenance, environment definition, executable checksum, resolved
analyzer version, capabilities, locations, diagnostics, case outcomes, and
totals.

## Build-only distribution

UsageBench checks in the complete image definitions but does not publish
ready-built images to GHCR. CI builds and smoke-tests both reference images
ephemerally without a registry login, push, OCI export, or image artifact
upload. A future archival release may place reviewed OCI archives on Zenodo.

Image construction needs network access to retrieve digest-pinned bases and
checksum-protected analyzer inputs. Benchmark execution itself uses
`--network none`, runs as a non-root user, mounts the released corpus read-only,
and writes only to isolated work and output mounts.

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
`platformScope: canonical_reference`, the environment and local image digests,
the actual analyzer executable SHA-256, and declared toolchain versions.

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

The current checked-in cases are a development and diagnosis corpus. Their
ground-truth metadata remains `legacy_unattributed` unless a document says
otherwise. Container reproducibility makes execution repeatable; it does not
upgrade the independent-review status of the expected locations.

Use `CITATION.cff` for citation metadata and retain the complete JSON reports.
Benchmark release tags, reference-environment versions, the Rust CLI version,
and YAML `schemaVersion` are separate compatibility boundaries.
