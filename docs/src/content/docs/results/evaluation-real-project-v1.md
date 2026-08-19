---
title: Historical v0.2.0 evaluation result
description: Immutable v0.2.0 results for the independently reviewed real-project-v1 evaluation slice.
---

> **Evaluation evidence.** This page reports only the independently reviewed
> `real-project-v1` evaluation partition frozen in
> [UsageBench v0.2.0](https://github.com/BrokkAi/usagebench/releases/tag/v0.2.0).
> It does not pool the broader analyzer-informed development corpus.

The [generated evidence map](../evidence/) is the site-wide index of the frozen
v1 and v2 prospective slices, the separately reviewed legacy core, controls,
overflow, and remaining development cases. This page remains the detailed
historical v1 result; it does not become a proxy for the other strata.

The active development candidate is public Bifrost v0.10.1 at immutable commit
`511adaa2733067bb1b7809ab79e06ec0e3d2a146`. No result for that release is
published on this page: the tables below remain historical v0.2.0 evidence
produced with Bifrost v0.8.8, and any upgraded result must be published as a
subsequent snapshot or release with its own provenance.

The first immutable UsageBench evaluation compares Bifrost with gopls,
Pyright, and TypeScript language server across 12 source-only sampled public
repositories and 36 reviewed cases. The claim is deliberately bounded to these
repositories, profiles, and the recorded References and Definition operations;
it does not estimate language-wide accuracy, latency, memory, or cold-start
performance.

| Release fact | Value |
|---|---|
| Freeze | `real-project-v1`, evaluation `v0.2.0` |
| Source revision | [`6ea6056fa6b3eb52a656a2b4a62c57956771de78`](https://github.com/BrokkAi/usagebench/tree/6ea6056fa6b3eb52a656a2b4a62c57956771de78) |
| Evidence | [Release archive and checksum](https://github.com/BrokkAi/usagebench/releases/tag/v0.2.0) |
| Manifest SHA-256 | `3258e3269c98baa980883e969df97e1c9f4920503072ca8797e2600e2b268614` |
| Publication run | [Freeze benchmark snapshot](https://github.com/BrokkAi/usagebench/actions/runs/31510039551) |

## Evaluation scope and audit

### Per-profile denominators

| Language | Reference profile | Repositories | Cases | Population exclusions | Source-review replacements |
|---|---|---:|---:|---|---|
| Go | `gopls` | 4 | 12 | 5 (small repository × 1; missing root build marker × 2; source size over 150 MiB × 4) | 6 (independent source review rejected every earlier assigned candidate × 6) |
| Python | `pyright` | 4 | 12 | 35 (small repository × 13; missing SPDX license × 13; missing root build marker × 19; source size over 150 MiB × 18) | 4 (independent source review rejected every earlier assigned candidate × 4) |
| TypeScript | `typescript-language-server` | 4 | 12 | 27 (small repository × 1; missing SPDX license × 8; missing root build marker × 3; source size over 150 MiB × 23; truncated source tree × 1) | 2 (independent source review rejected every earlier assigned candidate × 2) |

Exclusion reasons can overlap, so reason counts are not repository totals. The
selection, replacements, reviews, and adjudication are hash-bound in the
release audit rather than reconstructed from these summary counts.

### Hash-bound review provenance

| Artifact | Frozen source | SHA-256 |
|---|---|---|
| Protocol | [`protocol.json`](https://github.com/BrokkAi/usagebench/blob/v0.2.0/benchmarks/evaluation/real-project-v1/protocol.json) | `9780dc32b34de862f51588bcdf036a004b9a4de76a3c4c52eb667fbbc51ad501` |
| Selection | [`selection.json`](https://github.com/BrokkAi/usagebench/blob/v0.2.0/benchmarks/evaluation/real-project-v1/selection.json) | `964f23de0130dfd48f38b12bfa35806659be12176b828fdc97fa7ef3dfcc46e8` |
| Independent review | [`review.json`](https://github.com/BrokkAi/usagebench/blob/v0.2.0/benchmarks/evaluation/real-project-v1/review.json) | `a801484c37f9071db95359050db098ec282bb75b64e94cf87e8df2cd66a2ad6c` |
| Source lock | [`sources.json`](https://github.com/BrokkAi/usagebench/blob/v0.2.0/benchmarks/evaluation/real-project-v1/sources.json) | `700a9b6927d2fbb57fae005b3f95057132902fba9eeb86ef7deb73db77cde45e` |
| Sol review | [`openai-gpt-5.6-sol.json`](https://github.com/BrokkAi/usagebench/blob/v0.2.0/benchmarks/evaluation/real-project-v1/reviews/openai-gpt-5.6-sol.json) | `4f77c34ba904ea8ad2652d518ff750f66cc32b789d4a4cf6c947c88c8a7486cd` |
| Fable review | [`anthropic-claude-fable-5.json`](https://github.com/BrokkAi/usagebench/blob/v0.2.0/benchmarks/evaluation/real-project-v1/reviews/anthropic-claude-fable-5.json) | `a26abea9f9f265d262d3440bbd82df96a489c960b41e7b07557781d84eae8637` |
| Human adjudication | [`adjudication.json`](https://github.com/BrokkAi/usagebench/blob/v0.2.0/benchmarks/evaluation/real-project-v1/reviews/adjudication.json) | `2e1ce6a986e399330b73e9558d3267a113d737e3d77038d2c6b2927f342f6900` |

## Snapshot inputs

| Candidate | Requested version | Environment | Report SHA-256 |
|---|---|---|---|
| Bifrost | `a54be9be9b08b9d9ddbab1c471e26d7f8bd932df` | Linux x86-64 canonical container | `702a73c97d86a4c9c6b2b576c3d411306788581e4659b1ff9e82fae491c7459b` |
| gopls | `0.23.0` | Linux x86-64 canonical container | `679796a7a8f3d5b5eb44cad9a9ef06a2b512f4d53fdd503414842a4024026b94` |
| Pyright | `1.1.411` | Linux x86-64 native | `c6899baca9154f464c7baefe1984d0078175813138d6c428234f3573d0665099` |
| TypeScript language server | `5.3.0` with TypeScript `5.9.3` | Linux x86-64 native | `8dda75467e0bd2498905abb8e8e6702c185e9af5954c5513a55248ecda9a2ad4` |

## Required destinations found

This view asks whether each tool reached every required destination for a case.
It does not forgive extra results; those are exposed by the location-level and
strict-contract tables below.

| Reference profile | Shared | Bifrost found | Reference found |
|---|---:|---:|---:|
| gopls 0.23.0 | 12 | 8/12 (66.7%) | 9/12 (75.0%) |
| Pyright 1.1.411 | 12 | 8/12 (66.7%) | 11/12 (91.7%) |
| typescript-language-server 5.3.0 (TypeScript 5.9.3) | 12 | 10/12 (83.3%) | 8/12 (66.7%) |

## Location-level precision and recall

TP, FP, and FN are reported without true negatives. Strict precision counts
every extra result; policy-adjusted precision excludes authored and
policy-allowed extras.

| Reference profile | Analyzer | Cases | TP | FP | FN | Destination recall | Exact-token recall | Strict precision | Policy-adjusted precision | Exact-set case rate | Extras/success |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| gopls 0.23.0 | Bifrost | 12 | 71 | 0 | 6 | 92.2% | 48.1% | 100.0% | 100.0% | 75.0% | 0.00 |
| gopls 0.23.0 | gopls | 12 | 60 | 0 | 17 | 77.9% | 77.9% | 100.0% | 100.0% | 75.0% | 0.00 |
| Pyright 1.1.411 | Bifrost | 12 | 73 | 2 | 20 | 78.5% | 48.4% | 97.3% | 97.3% | 58.3% | 0.05 |
| Pyright 1.1.411 | Pyright | 12 | 90 | 1 | 3 | 96.8% | 96.8% | 69.2% | 98.9% | 33.3% | 1.70 |
| typescript-language-server 5.3.0 (TypeScript 5.9.3) | Bifrost | 12 | 43 | 0 | 11 | 79.6% | 66.7% | 100.0% | 100.0% | 83.3% | 0.00 |
| typescript-language-server 5.3.0 (TypeScript 5.9.3) | typescript-language-server | 12 | 43 | 2 | 11 | 79.6% | 79.6% | 79.6% | 95.6% | 41.7% | 0.56 |

## Strict contract conformance

Strict conformance requires the complete reviewed location set and exact token
ranges. A separating result is a contract disagreement, not automatically an
analyzer defect.

| Reference profile | Shared | Both exact | Bifrost only | Reference only | Neither |
|---|---:|---:|---:|---:|---:|
| gopls 0.23.0 | 12 | 7 | 1 | 2 | 2 |
| Pyright 1.1.411 | 12 | 6 | 1 | 4 | 1 |
| typescript-language-server 5.3.0 (TypeScript 5.9.3) | 12 | 5 | 5 | 2 | 0 |
| **Total** | **36** | **18** | **7** | **8** | **3** |

The [case comparison](../evaluation-real-project-v1-case-comparison/) lists every case where exactly one side
satisfies the strict contract. The broader
[24 July development result](../development-2026-07-24/) remains available as
historical regression evidence and is not part of these denominators.
