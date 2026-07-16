# Benchmark releases

UsageBench publishes immutable benchmark releases from Git tags named
`vMAJOR.MINOR.PATCH`. A release identifies the cases, fixture sources, expected
locations, adapter profiles, schemas, and harness code needed to reproduce a
benchmark result.

## Version boundaries

The benchmark release, Rust package, and YAML schema have separate version
contracts:

- The Git tag is the public benchmark-corpus version. It is the version to cite
  and use when comparing published results.
- `Cargo.toml` is the Rust CLI and adapter implementation version. It may change
  without creating a new corpus release, and a corpus release is not required to
  use the same number.
- `schemaVersion` in benchmark YAML describes file-format compatibility only. It
  changes when readers can no longer interpret the document shape, not whenever
  cases or assertions change.

Benchmark release versions follow SemVer:

- **Major** releases make existing results structurally or semantically
  incompatible, such as changing scoring meaning or the ground-truth contract.
- **Minor** releases add languages, cases, fixtures, scored operations, or
  materially revised assertions while retaining the existing contracts.
- **Patch** releases correct case metadata, source ranges, or harness behavior
  without intentionally expanding the scored corpus or redefining its ground
  truth.

Every published comparison should retain the complete JSON report. The report's
`usagebenchRevision` is the exact source commit; `usagebenchRelease` contains the
release tag when the run came from a clean tagged checkout. A revision ending in
`-dirty` records that local changes were present and the run is not reproducible
from the commit alone.

## Release contents

The tag workflow publishes a curated `usagebench-vMAJOR.MINOR.PATCH.tar.gz`
asset containing only the public benchmark surface:

- `benchmarks/` and `fixtures/` for assertions and code examples;
- `adapters/`, `schema/`, and `src/` for profiles, contracts, and harness code;
- Cargo metadata, citation metadata, the license, and concise reproduction docs.

The docs site sources and internal execution plans are intentionally omitted.
GitHub also creates its standard repository source archives automatically; use
the curated release asset when a minimal reproducibility bundle is preferred.

## Preparing a release

1. Choose the benchmark version from the policy above.
2. Update `CITATION.cff` with that version and the intended release date. Do not
   add a placeholder DOI.
3. Merge and validate the release commit, then create and push an annotated
   `vMAJOR.MINOR.PATCH` tag at that commit.
4. Confirm that the release workflow validates the corpus and citation file,
   publishes the curated archive and checksum, and creates the GitHub Release.

Repository administrators should protect the `v*` tag namespace with a ruleset
that limits tag creation and updates to release maintainers. The workflow's
`release` environment should require an approving reviewer and allow deployment
only from protected release tags. The workflow also rejects release commits that
are not reachable from `main`; tag protection and environment approval ensure a
tagged commit cannot replace or bypass that workflow policy.

Actions are pinned to full commit SHAs. Keep their trailing version comments and
use Dependabot or an equivalent reviewed update process when advancing them.

If a DOI or archival identifier is assigned later, add the real version-specific
identifier to `CITATION.cff` and the corresponding release notes. Never invent or
reserve a DOI-shaped placeholder.
