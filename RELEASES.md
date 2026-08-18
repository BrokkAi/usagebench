# Benchmark releases

UsageBench publishes immutable benchmark releases from Git tags named
`vMAJOR.MINOR.PATCH`. A release identifies the cases, fixture sources, expected
locations, adapter profiles, schemas, and harness code needed to reproduce a
benchmark result.

## Version boundaries

The benchmark release, reference environment, Rust package, and YAML schema
have separate version contracts:

- The Git tag is the public benchmark-corpus version. It is the version to cite
  and use when comparing published results.
- `containers/reference/vN` is the canonical execution-environment contract.
  It changes when the platform, isolation, build, or analyzer-packaging
  contract changes, independently of corpus content.
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

Only documents marked `corpus.partition: evaluation` belong in a published
accuracy aggregate. Validation requires those documents to be pre-registered,
assigned a `freezeId`, and supported either by independent human review or by
one accountable human adjudicating a blinded panel spanning at least two model
providers. Agent-only review is never publication-qualifying. Any assertion
change after review creates a new freeze and at least a minor corpus release;
the prior report and freeze remain available.

## Release contents

The tag workflow publishes a curated `usagebench-vMAJOR.MINOR.PATCH.tar.gz`
asset containing only the public benchmark surface:

- `benchmarks/` and `fixtures/` for assertions and code examples;
- `adapters/`, `schema/`, and `src/` for profiles, contracts, and harness code;
- `containers/` and `scripts/` for digest-pinned local image construction,
  offline execution, and semantic report comparison; and
- Cargo metadata, citation metadata, the license, `ARTIFACT.md`, and concise
  reproduction docs.

The bundle does not contain a built OCI image. CI builds Bifrost and gopls
images ephemerally but never pushes them to GitHub Container Registry or
uploads them as workflow artifacts. A future version-specific Zenodo deposit
may add OCI archives after review without changing the checked-in build-only
contract.

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

## Freezing a benchmark snapshot

Use the **Freeze benchmark snapshot** manual workflow to turn a reviewed main
revision into a release. It accepts the new benchmark version, an exact source
revision (or `main`), `development`, `evaluation`, or `legacy-promoted`
evidence, and a comma-separated list of candidate IDs from
`adapters/candidates.json`.

`adapters/candidates.json` is the release registry for analyzer identities. It
records each candidate's requested version, source, pinned revision where one
exists, and LSP profile. It also names the candidates whose installation and
execution contract is currently reproducible in the protected runner. Update
the registry when adding or advancing a candidate; do not put a release ref in
the workflow input. A candidate becomes executable by assigning its
`referenceRunner` to a corresponding versioned reference environment.

Registry schema version 3 separates public advertisement from execution mode.
An advertised candidate may name a protected `referenceRunner`; otherwise its
checked-in LSP profile is run once natively during the freeze. Non-primary
alternatives remain registered but cannot be selected for generated public
results.

The reference-environment manifest retains the candidate values needed for a
self-contained release bundle, but its build scripts reject any mismatch with
this registry. The registry is therefore the maintained source of truth rather
than a second independent release setting.

The workflow resolves the selected source to a full commit, proves it is
reachable from `main`, checks that `CITATION.cff` agrees with the requested
version, validates the corpus, and freezes one report per selected candidate.
It then writes a versioned manifest with the scoring contract, corpus policy,
candidate identities, environment provenance, and SHA-256 report digests. A
development snapshot remains explicitly labeled as development evidence. An
evaluation snapshot fails unless every executed document has the existing
evaluation, preregistration, and independent-review metadata. It also binds an
evaluation audit containing the freeze ID, bounded claim scope, hashed protocol,
selection, review, and source-lock provenance, per-profile repository and case
denominators, and recorded exclusions and replacements.

The only job with `contents: write` waits for the protected `release`
environment. Immediately after approval it checks the tag again, creates an
annotated tag, and pushes it without force. Existing tags always fail; the
workflow never moves or replaces a release tag. The curated release archive
contains the source bundle plus `evidence/freeze-manifest.json`, the selected
reports, and `evidence/SHA256SUMS`.

The manifest binds exactly one report to each selected candidate. Canonical
reports must contain the protected reference-environment and container
provenance. Native LSP reports must match the pinned candidate and profile
identity, cover the selected cases, and contain no runner errors. Result
generation verifies every report checksum and the copied manifest metadata
before rendering a row.

For `real-project-v1`, first materialize and verify Git LFS plus the source,
selection, and review evidence; then run `validate-evaluation`; finally freeze
the evaluation directory with exactly Bifrost, gopls, Pyright, and TypeScript
LS. The freeze runs Pyright and TypeScript LS once from their pinned profiles.
Development freezes retain the full development-corpus/full-matrix behavior
while excluding the separately governed evaluation partition. Generated
evaluation pages remain partition-labeled and limited to descriptive
per-profile comparisons; they exclude language-wide or ecosystem-wide
estimates, cross-language ranking, causal defect claims, and latency, memory,
or cold-start claims.

The `legacy-promoted` path is a separate retrospective release contract. It
is bound to `benchmarks/promotion/legacy-v1/manifest.json`, stages an
execution-only corpus containing exactly its 110 `balanced_core` IDs across
30 documents, and keeps overflow and control cases outside the run. Two
checksum-bound canonical reference reports (Bifrost and gopls) run on Ubuntu;
the nine remaining advertised profiles run one at a time on the selected
native macOS runner. Candidate reports remain language-scoped; the frozen
manifest's union is the 110-case balanced core. This host split is an
execution detail, not a two-host evidence requirement.

This workflow trusts the native execution environment to provision the
requested payload behind each recorded launcher. Profile and executable hashes
are verified, but full package and dependency closures are not. Release notes
must not describe native rows as cryptographic payload attestation or
cross-host reproduction.

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
