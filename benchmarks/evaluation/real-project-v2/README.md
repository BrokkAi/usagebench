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
