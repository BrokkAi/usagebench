# Real-project evaluation v2 preregistration

This directory is the preregistration boundary for UsageBench's second
independent real-project slice. The protocol was written before the v2
population, repository draw, declaration choices, source review, Bifrost
results, or reference-language-server results were inspected.

V2 covers three languages absent from v1:

| Language | Reference profile | Repositories | Declarations per repository | Reserved repositories |
| --- | --- | ---: | ---: | ---: |
| Java | `eclipse-jdtls` | 4 | 3 | at least 4 |
| Rust | `rust-analyzer` | 4 | 3 | at least 4 |
| C++ | `apple-clangd-21` | 4 | 3 | at least 4 |

The common 30,000-star frame balances a bounded public-repository population
with enough expected headroom for the recorded license, size, build-marker,
source-count, and prior-slice exclusions. The population capture itself is the
authority for eligibility; if a profile has fewer than eight eligible
repositories, selection fails closed rather than weakening the protocol.

## Ordered source-only boundary

1. Commit `protocol.json` and the tooling that validates it.
2. Run `capture-real-project-population` with the v2 protocol. Commit the
   completed `population.json`; never commit `population.partial.json`.
3. Run `draw-real-project-selection` with the exact protocol-introducing
   commit. The command requires `population.json` to be committed and
   byte-identical to `HEAD`. Commit `selection.json` before authoring a
   declaration.
4. In follow-up work, materialize the selected commits, produce the recorded
   source-only declaration ranking, author cases, obtain two independent
   reviews, and adjudicate disagreements.
5. Only after those artifacts are frozen may Bifrost or a reference language
   server execute against v2.

## Source materialization and review packets

The retained source-only tooling is intentionally separate from analyzer
execution. Materialize each selected commit outside the repository, then build
the source lock and declaration ranking:

```bash
python3 scripts/real_project_v1_sources.py \
  --selection benchmarks/evaluation/real-project-v2/selection.json \
  --archives benchmarks/evaluation/real-project-v2/sources \
  --checkouts /path/to/pinned-checkouts \
  --output benchmarks/evaluation/real-project-v2/sources.json
python3 scripts/real_project_v1_candidates.py \
  --selection benchmarks/evaluation/real-project-v2/selection.json \
  --sources-root /path/to/pinned-checkouts \
  --output benchmarks/evaluation/real-project-v2/declarations.json
```

Build the six balanced blinded-review input sets with:

```bash
python3 scripts/build_real_project_review_packets.py \
  --declarations benchmarks/evaluation/real-project-v2/declarations.json \
  --sources benchmarks/evaluation/real-project-v2/sources.json \
  --output-root benchmarks/review-protocol/runs
```

The generated `packet-manifest.json` files describe prepared inputs only. They
are not provider-session, comparison, human-adjudication, ground-truth, or
publication evidence. Complete and retain each milestone's 12 genuine fresh
provider sessions and accountable human adjudication before proceeding to the
next milestone. Do not execute an analyzer or reference language server until
all 36 adjudicated contracts are frozen.

After all six milestones are adjudicated, the retained builders reproduce the
published review links and case documents without consulting analyzer output:

```bash
python3 scripts/build-real-project-v2-publication-review.py --check
python3 scripts/build_real_project_v2_cases.py --check
cargo run -- validate-evaluation benchmarks/cases/evaluation/real-project-v2
```

The immutable evaluation release resolves its candidate set from this freeze's
protocol. It executes Bifrost plus `eclipse-jdtls`, `rust-analyzer`, and
`apple-clangd-21`; the Apple profile runs on the registered macOS/Xcode host
and does not introduce a second-host evidence gate.

The v0.3.0 execution identities are frozen separately in
`candidates-v0.3.0.json`. Its Bifrost candidate is pinned to public release
v0.10.2 at `d1a7c0cc1cf58d0c0789476ad42a92318bb8da49`. It runs natively on the
registered macOS host and has no canonical reference runner. Any later Bifrost
run is evidence for a subsequent snapshot or release, never a replacement for
v0.3.0 evidence.

The v1 selection is hash-linked by the v2 protocol. Every v1-selected
repository is ineligible even if it appears in a v2 GitHub language frame.
Capture, rank, and replacement decisions use public source metadata only.

## Reporting boundary

V2 results must retain their own freeze ID and per-profile repository and case
denominators. A future combined v1/v2 presentation may aggregate only within
documented language/profile strata and must retain slice-specific
denominators. Neither slice supports ecosystem-wide or language-wide
estimates, cross-language rankings, causal defect claims, or latency, memory,
and cold-start claims.
