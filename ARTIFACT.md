# UsageBench artifact review guide

## Reproducibility claim

A released UsageBench report produced by reference environment version 1 can
be rebuilt and rerun from the report alone. The canonical platform is
`linux/amd64`; version 1 covers Bifrost and gopls. The reproduction command
selects the exact UsageBench release and analyzer input, builds the matching
local image, runs the analyzer without network access, and compares the new
report semantically with the published report.

This repository publishes a versioned build contract, not ready-built images.
CI builds and exercises both images ephemerally, but does not log in to a
registry, push an image, export an OCI archive, or upload an image artifact.
Built images may be archived with a future Zenodo deposit after the artifact
has been reviewed.

The checked-in corpus is currently a development and diagnosis corpus. Its
reproducible execution environment does not change the separate ground-truth
review status recorded in each benchmark document.

## Requirements

- Docker Engine or Docker Desktop with `docker buildx` and amd64 container
  support;
- Bash, Git, and jq;
- outbound network access while building images; and
- enough free disk for Rust, Go, Bifrost, and gopls build layers.

Runtime analysis is network-disabled. On an Apple Silicon development machine,
the clean amd64 Bifrost analyzer build took about nine minutes under emulation;
the first gopls image build took about five minutes. Native amd64 systems and
cached rebuilds can be substantially faster.

## Reproduce a published report

From an extracted `usagebench-vMAJOR.MINOR.PATCH.tar.gz` release bundle, run:

```bash
./scripts/reproduce-report.sh /path/to/published-report.json reproduced-report.json
```

The command verifies that the input is a canonical container report, then:

1. reads its UsageBench release, exact revision, environment version, runner,
   case selection, and inclusion policy;
2. uses the current bundle when it matches, or clones and verifies the exact
   release tag and commit;
3. builds the local `linux/amd64` image from digest-pinned definitions;
4. reruns the same selection with networking disabled, as a non-root user,
   with the release corpus mounted read-only; and
5. runs `compare-reports` inside the image.

Success ends with:

```text
reports are semantically equivalent
reproduced report: .../reproduced-report.json
```

The comparator ignores only timestamps, temporary workspace roots, local
filesystem paths, and the locally rebuilt OCI digest. It still compares the
release and revision, reference-environment definition digest, executable
checksum, resolved analyzer version, capabilities, case statuses, locations,
diagnostics, and totals. A changed outcome exits nonzero and identifies the
case-level field that differs.

## Build and inspect the environments directly

Use the release tag recorded in the report:

```bash
./scripts/reference-image.sh bifrost vMAJOR.MINOR.PATCH
./scripts/reference-image.sh gopls vMAJOR.MINOR.PATCH
```

Build metadata is written under `target/reference/`. It contains the local tag,
canonical platform, stable definition digest, and locally built image digest.
No command pushes the image.

To run one released case directly:

```bash
./scripts/run-reference.sh \
  gopls \
  /path/to/extracted/usagebench-vMAJOR.MINOR.PATCH \
  gopls-report.json \
  benchmarks/cases/go-baseline.yaml \
  go-package-function-call
```

The corpus root must contain `.usagebench-release.json`. The wrapper enforces
`--network none`, a read-only root filesystem, a non-root UID/GID, a read-only
corpus mount, and isolated writable tmpfs work directories. Only the selected
output directory is writable on the host.

Inspect the evidence envelope with:

```bash
jq '{usagebenchRelease, usagebenchRevision, runner, invocation, environment}' \
  reproduced-report.json
```

## Version and integrity boundaries

- `usagebenchRelease` and `usagebenchRevision` identify the immutable corpus
  and harness source.
- `environment.referenceEnvironment.version` identifies the container contract
  independently of the benchmark and YAML schema versions.
- `definitionDigest` covers the environment manifest, schema, Dockerfile, and
  build/run wrappers.
- Dockerfile frontend, builder, and runtime images are pinned by amd64 digest.
- Bifrost is fetched at an exact Git commit. gopls is fetched at an exact module
  version and its Go module checksum is verified before compilation.
- Cargo lockfile checksums and the pinned Go module graph protect transitive
  build inputs.
- Reports checksum the analyzer executable that actually ran.

Repeated cached builds during development produced identical OCI digests for
both version 1 images. Cross-builder byte identity is not the scientific claim:
the semantic comparator intentionally permits the local OCI digest to differ
while requiring the stable definition digest, executable checksum, and result
semantics to match.

## Troubleshooting

- `Cannot connect to the Docker daemon`: start Docker Engine or Docker Desktop.
- amd64 warnings or very slow builds on arm64: ensure Docker's amd64 emulation
  is enabled; the canonical platform is intentionally fixed.
- build download failures: construction needs network access even though the
  benchmark runtime does not.
- `reference definition mismatch`: use the release bundle named by the report;
  do not mix container scripts from another revision.
- a semantic diff after a successful run: preserve both JSON reports. The diff
  is evidence of a result, capability, executable, or environment discrepancy,
  not a reason to relax the benchmark assertion.
