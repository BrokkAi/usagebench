---
title: Evidence map
description: Machine-derived UsageBench evidence boundaries and immutable publication status.
---

> **Generated evidence boundary.** This page is generated from the checked-in selection, review, source-lock, promotion, and cohort manifests. It does not run Bifrost and it does not copy provisional scores.

The shortest honest answer is currently the published `real-project-v1` result: its historical v0.2.0 page reports the measured Bifrost/reference comparison. The independent v2 slice and the retrospectively reviewed legacy core have frozen source/review boundaries, but this checkout does not contain checksum-verified analyzer result reports for either slice. They remain visibly pending rather than being scored or pooled.

## Evidence breadth

| Slice | Frozen identity | Selection and review tier | Frozen denominator | Result publication |
| --- | --- | --- | ---: | --- |
| Prospective v1 | `real-project-v1` · `v0.2.0` | `prospective_pre_registered` · `human_adjudicated_agent_panel` | 36 cases (12 per profile) | Historical release; [current v1 result](../) |
| Prospective v2 | `real-project-v2` · no result release yet | `prospective_pre_registered` · `human_adjudicated_agent_panel` | 36 cases (12 per profile) | Awaiting an immutable result report |
| Reviewed legacy core | `legacy-promotion-v1-balanced-core` · no result release yet | `retrospectively_selected` · `legacy_promoted` | 110 cases (10 × 11 languages) | Awaiting an immutable result report |

The denominators above are not interchangeable. In particular, v1 and v2 are prospective source-only selections, while the legacy core is a separately frozen retrospective promotion of analyzer-informed development cases. A later report may present a documented stratified aggregate, but it may not flatten these trust tiers into one accuracy score.

## Prospective profile denominators

Each profile remains visible before any aggregate. The numbers here are selected-case counts, not analyzer outcomes.

| Slice | Language | Candidate/reference profile | Repositories | Cases |
| --- | --- | --- | ---: | ---: |
| v1 | go | `gopls` | 4 | 12 |
| v1 | python | `pyright` | 4 | 12 |
| v1 | typescript | `typescript-language-server` | 4 | 12 |
| v2 | java | `eclipse-jdtls` | 4 | 12 |
| v2 | rust | `rust-analyzer` | 4 | 12 |
| v2 | cpp | `apple-clangd-21` | 4 | 12 |

## Reviewed legacy boundaries

The immutable promotion is `legacy-promotion-v1-balanced-core`. Its balanced core is **110 cases**, with **42 overflow** candidates and **6 controls** kept outside the correctness denominator. The source-only legacy inventory contains 158 cases; the remaining development corpus also contains 2 semantic-pack cases that were never part of that inventory.

| Language | Balanced-core cases |
| --- | ---: |
| cpp | 10 |
| csharp | 10 |
| go | 10 |
| java | 10 |
| javascript | 10 |
| php | 10 |
| python | 10 |
| ruby | 10 |
| rust | 10 |
| scala | 10 |
| typescript | 10 |

## Immutable report provenance

The entries below are derived only from checksum-verified release bundles. The generated result pages remain the score authority; this index records their exact release, snapshot, manifest, and report identities without retyping score totals.

| Slice | Release | Revision | Freeze manifest SHA-256 | Report artifacts |
| --- | --- | --- | --- | --- |
| v1 | `v0.2.0` | — | — | historical release; bundle not supplied in this generation |
| v2 | — | — | — | pending immutable bundle |
| legacy | — | — | — | pending immutable bundle |

## Remaining development evidence

The checked-in development corpus contains **160 cases**. The reviewed legacy core accounts for 110; the 50 cases outside that core comprise 42 frozen overflow candidates, 6 unsupported/not-planned controls, and 2 semantic-pack cases. This remainder is retained for regression and diagnosis; it is not silently added to v1, v2, or the legacy denominator.

## Publication safeguards

- Prospective v1 and v2 keep separate profile/language denominators; v2 permits only documented stratified aggregation.
- The legacy manifest records `retrospectively_selected`, `legacy_promoted`, `source_only`, and `analyzerOutcomeUse: forbidden`; re-review cannot make the source contract preregistered.
- Controls and overflow remain explicit partitions and cannot enter the balanced-core score.
- Score tables must be generated from a checksum-verified immutable report artifact bound to the matching manifest. This page records published provenance when such a bundle is supplied and otherwise reports readiness, never guessed scores.

Manifest provenance is machine-readable in `docs/src/data/evidence.json` and is checked in CI with `scripts/generate-docs-evidence.py --check`. See the [current v1 result](../), the [historical development result](../development-2026-07-24/), and the [human ground-truth audit](../../ground-truth-review/) for retained historical evidence.
