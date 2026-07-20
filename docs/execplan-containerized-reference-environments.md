# ExecPlan: Containerized Reference Environments

## Goal

Make a released UsageBench result independently reproducible by an academic
artifact reviewer. Native execution remains available for development, while a
canonical `linux/amd64` reference path builds versioned OCI images for Bifrost
and gopls, runs them without network access, and compares a reproduced report
semantically with the published report.

The repository publishes the build contract, not ready-built images. CI builds
and exercises images ephemerally but never logs in to a registry, pushes an
image, or uploads an OCI archive. A future archival release may deposit built
OCI archives with Zenodo.

## Current State

UsageBench releases already identify the exact corpus revision and analyzer
revision. Reports do not yet identify the execution environment or analyzer
executable bytes. Runners prepare analyzers on the host, reproduction is a
manual sequence, and no container definitions exist.

The implementation starts from commit `54fdbd4010d7880b126ddc0aac800803f17d51c2`
on branch
`53-add-containerized-reference-environments-for-reproducible-benchmark-runs`.

## Contract

Reference environments have an independent monotonically versioned contract.
Version `1` uses:

- canonical platform `linux/amd64`;
- Docker/BuildKit as the supported builder and runtime;
- a shared UsageBench harness image plus Bifrost and gopls runner images;
- digest-pinned base images and checksum-verified analyzer inputs;
- non-root execution with the released corpus mounted read-only;
- network access during image construction only; and
- semantic report equality after documented volatile fields are removed.

Every containerized report records both the stable definition digest and the
OCI digest of the locally built image. Rebuilding the same definition is a
functional reproducibility promise. Byte-for-byte identical rebuilt images are
only claimed if clean double-build validation demonstrates it.

## Milestones

### 1. Version the reference-environment contract

Add this ExecPlan, a JSON Schema, and a versioned manifest declaring the
canonical platform, local tag convention, and build-only distribution policy.

### 2. Record invocation and execution provenance

Extend `RunReport` with the logical invocation and structured environment
metadata. Record native runs as host-specific. Let container wrappers provide a
validated environment descriptor, then checksum the actual analyzer executable
used by Bifrost or the LSP runner.

### 3. Compare reports semantically

Deserialize reports and add a `compare-reports` command. Ignore only timestamps
and run-specific workspace roots. Compare release and invocation preconditions,
capabilities, outcomes, locations, diagnostics, and scoring at case granularity.

### 4. Build and exercise Bifrost and gopls images

Add digest-pinned Dockerfiles and build/run helpers. Bifrost must support an
already-built executable so runtime never fetches or compiles it. Run both
images with networking disabled, a non-root user, a read-only corpus, and
isolated writable work/output mounts.

### 5. Reproduce from a prior report

Add one command that reads a prior report, selects its UsageBench release and
reference-environment version, builds the local image if necessary, reruns the
same selection and inclusion policy, and invokes the semantic comparator.

### 6. Package and document the artifact contract

Build and smoke-test images ephemerally in CI without publication. Include the
container definitions and reproduction tooling in curated release bundles.
Add a root artifact-review guide and update public reproduction/versioning
documentation.

### 7. Validate and review

Run Rust formatting/tests, corpus validation, report-schema generation, clean
container builds, offline runtime checks, intentional semantic mismatch tests,
and reproduction from an extracted release bundle. Perform focused security,
infrastructure, intent, duplication, and architecture review before opening the
pull request.

## Progress

- [x] 2026-07-20: Confirmed the issue branch is clean and aligned with
  `origin/main` at `54fdbd4`.
- [x] 2026-07-20: Agreed that checked-in build definitions are the published
  artifact and that hosted images are deferred to a possible Zenodo deposit.
- [x] 2026-07-20: Added and validated the version 1 reference-environment
  manifest and JSON Schema.
- [x] Milestone 1: reference-environment contract.
- [x] 2026-07-20: Added invocation, native/container platform scope,
  reference-image identity, analyzer executable checksums, and observed
  toolchains to the shared report schema.
- [x] Milestone 2: report provenance.
- [x] 2026-07-20: Added typed report deserialization and a case-keyed semantic
  comparator that ignores only timestamps, local source/executable paths, and
  the newly rebuilt OCI digest.
- [x] Milestone 3: semantic comparison.
- [x] 2026-07-20: Built digest-pinned Bifrost and gopls images for
  `linux/amd64`; both exact smoke cases passed with networking disabled, a
  non-root user, a read-only released corpus, and isolated tmpfs workspaces.
- [x] 2026-07-20: Repeated fully cached builds produced unchanged definition
  and OCI image digests for both reference runners.
- [x] Milestone 4: reference images and offline execution.
- [x] 2026-07-20: Added a one-command reproduction workflow that resolves the
  exact released corpus, rebuilds the recorded environment, reruns the same
  case selection and policy, and performs semantic comparison in the image.
- [x] 2026-07-20: Reproduced a gopls report from a synthetic extracted release
  bundle and confirmed that an intentionally changed case outcome is rejected.
- [x] Milestone 5: report-driven reproduction.
- [ ] Milestone 6: CI, release packaging, and documentation.
- [ ] Milestone 7: final validation and review.

## Decision Log

- 2026-07-20: Use `linux/amd64` as the canonical reference platform because it
  matches the standard GitHub-hosted Linux runner and is broadly available to
  paper reviewers.
- 2026-07-20: Version the environment contract independently from the corpus,
  CLI, and benchmark YAML schema.
- 2026-07-20: Record both a stable build-definition digest and the locally
  produced OCI digest; do not imply that a local tag is globally retrievable.
- 2026-07-20: Preserve the repository's current GitHub Action version-tag
  policy. Reproducibility is rooted in pinned container inputs and the released
  local build contract, not in retained CI images.

## Risks and Mitigations

- Bifrost builds are expensive. Keep compilation in image construction, use
  BuildKit caches locally/ephemerally, and make runtime use a prebuilt binary.
- Package repositories drift. Pin base digests and use fixed snapshot/package
  inputs rather than resolving mutable packages during future rebuilds.
- Reports contain absolute temporary paths. Normalize only known workspace-root
  fields and test that semantic path/location changes still fail comparison.
- A rebuilt OCI digest may differ despite identical inputs. Double-build and
  report this honestly; a later Zenodo OCI archive can provide byte-identical
  distribution when required.
- Cross-platform emulation can be slow on arm64 developer machines. Keep
  `linux/amd64` canonical and make the expected cost explicit in `ARTIFACT.md`.

## Surprises and Discoveries

- 2026-07-20: The gopls `v0.23.0` module requires Go 1.26 or newer. The first
  clean build deliberately disabled automatic toolchain download and rejected
  the initially selected Go 1.25.6 builder, so the manifest was corrected to a
  digest-pinned Go 1.26.0 image.
- 2026-07-20: The UsageBench binary embeds the checked-in benchmark JSON
  Schema with `include_str!`, so the harness build stage must copy `schema/`
  even though the runtime image contains only the compiled executable.
- 2026-07-20: BuildKit's default provenance attestation changes the loaded
  manifest-list digest between otherwise identical builds. Local reference
  builds disable that attestation and retain explicit definition digests plus
  executable checksums in benchmark reports.
- 2026-07-20: gopls shells out to the Go command while loading a workspace.
  The offline runtime therefore includes the same pinned Go toolchain used by
  the analyzer builder, rather than only the statically built gopls binary.
- 2026-07-20: The pinned Bifrost revision declares Rust 1.96.0 in its
  `rust-toolchain.toml`. Its analyzer builder therefore uses a separate
  digest-pinned Rust 1.96.0 base instead of allowing rustup to download that
  toolchain into the shared Rust 1.95.0 harness builder.

## Acceptance Evidence

The finished branch must demonstrate:

1. Native and containerized reports are distinguishable and fully attributed.
2. Bifrost and gopls reference images build from checked-in, pinned definitions.
3. Both run successfully with `--network none` as non-root against a read-only
   released corpus.
4. One command reproduces a prior report and emits a case-level semantic diff
   on meaningful disagreement.
5. CI builds and tests images but contains no registry login, push, or image
   artifact upload.
6. The curated release bundle contains everything needed to build locally.
7. `cargo fmt -- --check`, `cargo test --locked`, and
   `cargo run --locked -- validate benchmarks/cases` pass.
