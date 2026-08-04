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

The Bifrost runner records and sends a 300-second wall-clock budget for each
usage scan by default, rather than relying on Bifrost's five-second interactive
default. For a quicker development pass, override the per-scan budget with
`--scan-usages-max-duration-secs <0-300>`; the effective value is retained in
the report's invocation metadata. Runs with `--output` atomically update a
sibling `*.partial.json` checkpoint after each completed benchmark document;
the requested output path is written only when the full run completes.
Checkpoints identify themselves with `completed: false`, retain the full
`requestedCaseFiles` scope, and cannot be used as completed snapshot or
reproduction evidence.

These commands preserve analyzer and host provenance but are not the canonical
cross-machine reproducibility claim.

## Independently reproduced native profiles

Public profiles that do not have a canonical image use the `native_two_host`
evidence class. The primary report and one corroborating report must come from
distinct documented hosts and identify the same UsageBench release, profile,
invocation, and case selection. Compare them with:

```bash
cargo run -- compare-reports \
  primary.json corroborating.json \
  --scope native-results \
  --output-diff semantic-diff.json
```

After collecting both reports into one staging directory, create the typed
evidence manifest with `create-native-evidence`. Host IDs must identify two
independent executions, and provenance values should link to the corresponding
workflow run, machine record, or other reviewable host documentation:

```bash
PROFILE_SHA="$(shasum -a 256 adapters/lsp/pyright.json | awk '{print $1}')"
cargo run -- create-native-evidence \
  --candidate pyright \
  --primary-report evidence/pyright-primary.json \
  --primary-host-id machine-a \
  --primary-runner-name usagebench-runner-a \
  --primary-host-provider github-actions \
  --primary-host-provenance https://github.com/example/actions/runs/1 \
  --primary-requested-version 1.1.411 \
  --primary-profile-sha256 "$PROFILE_SHA" \
  --corroborating-report evidence/pyright-corroborating.json \
  --corroborating-host-id machine-b \
  --corroborating-runner-name usagebench-runner-b \
  --corroborating-host-provider independent-runner \
  --corroborating-host-provenance https://example.test/runs/2 \
  --corroborating-requested-version 1.1.411 \
  --corroborating-profile-sha256 "$PROFILE_SHA" \
  --output evidence/pyright-evidence.json \
  --diff-output evidence/pyright-diff.json
```

Both executions must use the same operating system, architecture, pinned LSP
profile, and analyzer executable checksum on different machines. The native
comparison removes volatile host fields and toolchain observations, but retains
the analyzer command and executable SHA-256. Resolved version, capabilities,
locations, diagnostics, outcomes, and totals also remain semantic inputs.
Equivalent reports can be accepted as two-host evidence. Unequal reports and
their complete JSON diff remain useful evidence, but the profile is not
eligible for a generated public aggregate until an equivalent pair exists.

Every evidence manifest records typed host identities and provenance links,
both report checksums, and the comparison outcome. The freeze manifest links
that file by SHA-256, and result generation refuses missing, altered, same-host,
or non-equivalent evidence.

For a development release, **Native two-host reproduction** may schedule the
full advertised `native_two_host` matrix. For a `real-project-v1` evaluation
release, run it at the exact release revision for only Pyright and TypeScript LS
against `benchmarks/cases/evaluation/real-project-v1`. Two separately labeled
self-hosted runners execute each selected profile; the workflow rejects two jobs
that resolve to the same runner name, creates the evidence manifests, and
uploads one `native-reproduction-evidence` artifact. Pass that workflow run ID
to the freeze workflow. Freeze verifies the producer workflow path, source
revision, event, and successful conclusion before downloading the artifact.

Both hosts must be pre-provisioned with Bash, jq, Git, Rust, and every executable
named by the nine native profiles: `clangd`,
`Microsoft.CodeAnalysis.LanguageServer`, `jdtls`, `npx`, `ruby-lsp`,
`rust-analyzer`, and `cs`. Their exact versions and project-hydration behavior
must match `adapters/candidates.json` and the linked LSP profiles. Roslyn must be
allowed to create the local MSBuild named pipes described in the profile guide.
The workflow intentionally does not install or silently substitute host tools;
missing executables fail before collection.

Each runner administrator must also create
`/etc/usagebench-native-host.json`. Its `hostId` identifies the physical
machine, and every advertised native candidate records the requested release,
the checked-in profile SHA-256, and the installed command's SHA-256:

```json
{
  "schemaVersion": 1,
  "hostId": "machine-a",
  "candidates": {
    "pyright": {
      "requestedVersion": "1.1.411",
      "profileSha256": "<64 lowercase hex characters>",
      "executableSha256": "<64 lowercase hex characters>"
    }
  }
}
```

The real manifest contains all nine candidates. The workflow verifies these
values against the frozen registry, profile bytes, and executable before the
run. It also requires distinct machine IDs and GitHub runner registrations,
and rejects empty reports or any runner/session error.

This is an explicit trusted-administrator boundary, not a cryptographic
attestation of every transitive server payload. For launchers such as `npx`,
`cs`, and `jdtls`, the executable checksum covers the launcher while the pinned
profile command and administrator manifest assert the requested server release.
It does not independently hash the complete downloaded package, JAR, VSIX, or
dependency closure. Release reviewers must therefore trust the administrators
of both native hosts and their provisioning records. Payload-closure hashing
and offline cache verification are intentionally outside the current contract.

The current advertised native profiles are Apple clangd 21, Roslyn, Eclipse
JDT LS, TypeScript LS, Pyright, Intelephense, Ruby LSP, rust-analyzer, and
Metals. Upstream clangd 22 is a separate, currently unadvertised candidate;
Apple clangd results are never treated as upstream clangd reproduction.
The candidate contract also requires the server-reported version to begin with
`Apple clangd 21.0.0`, so an upstream clangd executable cannot satisfy that row.

## Evaluation release order

For `real-project-v1`, keep the evidence boundary and execution order explicit:

1. materialize Git LFS, then verify the source lock, preregistered selection,
   independent review records, and adjudication evidence;
2. run
   `cargo run -- validate-evaluation benchmarks/cases/evaluation/real-project-v1`;
3. collect accepted two-host evidence for Pyright and TypeScript LS at the exact
   release revision; and
4. freeze the evaluation directory with exactly Bifrost, gopls, Pyright, and
   TypeScript LS.

The generated result pages verify the manifest's evaluation audit and label the
partition explicitly. They publish descriptive per-profile denominators and
recorded exclusions and replacements. They do not support language-wide or
ecosystem-wide estimates, cross-language ranking, causal defect claims, or
latency, memory, and cold-start claims.

## Evidence scope

The checked-in fixture cases remain a development and diagnosis corpus. All 158
have completed a first human review, but retain `legacy_unattributed`
ground-truth metadata. The separate `real-project-v1` evaluation partition has
36 preregistered, independently reviewed, adjudicated, source-locked cases.
Its analyzer results have not yet been published. Container reproducibility
makes execution repeatable; it does not by itself upgrade review status.

See the [human ground-truth audit](../ground-truth-review/) for the review
procedure and the distinction between reviewed development assertions and a
publishable evaluation partition.

Use `CITATION.cff` for citation metadata and retain the complete JSON reports.
Benchmark release tags, reference-environment versions, the Rust CLI version,
and YAML `schemaVersion` are separate compatibility boundaries.
