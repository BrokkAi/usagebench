---
title: Current corrected result
description: Required-destination and strict-contract results from the reviewed development corpus on 24 July 2026.
---

> **Development evidence, not an evaluation leaderboard.** All 158 cases have
> completed a first human review, but the corpus remains analyzer-informed and
> `legacy_unattributed`. A second independent review, preregistration, and an
> immutable freeze are still required for evaluation promotion.

> **Historical identity limitation.** This pre-schema-v2 page requested the
> upstream clangd 22.1.6 profile while the host resolved Apple clangd 21.0.0.
> It is preserved as development history, but that row is not valid published
> candidate evidence. New freezes use the distinct `apple-clangd-21`
> candidate and reject upstream or unverified executables.

This corrected native result replaces the legacy 16 July figures. Bifrost was
rerun across the full corpus at the pinned revision. gopls, TypeScript LS, and
Intelephense were rerun after adding reviewed compatible-operation evidence and
the machine-readable destination scorer. Java had already been rerun with the
same pinned JDT LS after correcting its workspace path and ordinary
source-navigation operations; Metals had been refreshed at 1.6.8. The other
five reference profiles retain their synchronized 24 July reports because
their cases and operations did not change. All fresh reports record zero runner
errors.

| Run fact | Value |
|---|---|
| Date | 24 July 2026 |
| UsageBench parent revision | `d01a4aa60b4516529a63009ef312c6f34d21b80a` |
| Corrections | Java workspace isolation and Definition authoring; Metals 1.6.8 refresh; 13 reviewed compatible-operation annotations and report-derived destination scoring |
| Bifrost revision | `782522b245fc86e3d39b1cdc0488553a1d262212` (pinned synchronized revision) |
| Host | macOS arm64, native and host-specific |
| Scoring | Headline required-destination recall; secondary strict range and identity conformance |

## Required destinations found

The primary comparison uses the 144 cases scoreable by both Bifrost and the
corresponding reference server through the canonical operation or an explicitly
reviewed compatible operation. It asks whether every reviewed reference was
returned and every navigation or type lookup included its expected
destination. Line-only or broader containing ranges and additional results are
tolerated in this view because an editor user can still reach the required
code. Compatible operations are reported separately; they do not alter the
strict canonical results below.

| Language | Shared destination-scoreable | Bifrost destinations | Reference destinations |
|---|---:|---:|---:|
| C++ | 15 | 12/15 (80.0%) | 14/15 (93.3%) |
| C# | 16 | 14/16 (87.5%) | 15/16 (93.8%) |
| Go | 11 | 9/11 (81.8%) | 11/11 (100.0%) |
| Java | 11 | 11/11 (100.0%) | 11/11 (100.0%) |
| JavaScript, TypeScript | 21 | 19/21 (90.5%) | 19/21 (90.5%) |
| PHP | 14 | 12/14 (85.7%) | 14/14 (100.0%) |
| Python | 13 | 11/13 (84.6%) | 13/13 (100.0%) |
| Ruby | 16 | 15/16 (93.8%) | 11/16 (68.8%) |
| Rust | 15 | 14/15 (93.3%) | 13/15 (86.7%) |
| Scala | 12 | 11/12 (91.7%) | 10/12 (83.3%) |
| **Pooled** | **144** | **128/144 (88.9%)** | **131/144 (91.0%)** |

The fresh report fields reproduce the affected rows directly: Go pairs to
9/11 versus 11/11, JavaScript/TypeScript to 19/21 versus 19/21, and PHP to
12/14 versus 14/14. Bifrost's full report finds 135 of its 152 individually
scoreable cases; intersecting scoreable case IDs with each reference profile
produces the 128/144 paired total above.

With each profile weighted equally, Bifrost averages **88.9%** and the
reference servers **91.6%**; the median paired profile difference is **−3.1
percentage points**. Bifrost leads three profiles, ties two, and trails five.

This is a recall-forward measure, not a declaration that all returned results
are equally good. A case can surface every required destination while also
returning distracting family members or unrelated targets. Those differences
remain failures in the strict contract view.

## Strict UsageBench contract conformance

The secondary comparison requires complete reviewed token ranges, no unallowed
identity-family extras, and a strict singleton navigation target. This is
UsageBench's machine-consumer contract, not a generic LSP 3.18 compliance test:
the protocol defines `Location` and `Range` shapes but does not require the
range to select exactly one terminal identifier.

| Outcome | Cases |
|---|---:|
| Exact for both | 85 |
| Exact only for Bifrost | 31 |
| Exact only for the language server | 11 |
| Exact for neither | 4 |
| **Shared scoreable total** | **131** |

Bifrost is therefore exact on **116/131** shared cases; the reference servers
are exact on **96/131**. Nine of the 31 Bifrost-only exact cases are LSP
`position_unverified` results: the LSP reached the expected line but returned a
line-only or broader containing range instead of the one complete target token
required by the contract. The other 22 are hard contract disagreements.

### Strict aggregation and sensitivity

No single aggregate answers every comparison question. The pooled rate weights
each authored case equally; the equal-profile mean weights each of the ten
reference-server profiles equally; and the median shows the middle profile
without letting the two largest gaps determine the result.

| View | Bifrost | Reference servers | Interpretation |
|---|---:|---:|---|
| Pooled, case-weighted exactness | 116/131 (88.5%) | 96/131 (73.3%) | Outcome across all shared cases |
| Equal-profile mean exactness | 88.6% | 75.0% | Each server profile has equal influence |
| Median profile exactness | 87.9% | 80.0% | Typical profile, less affected by the largest gaps |

The median of the ten paired per-profile differences is **+9.2 percentage
points** for Bifrost. Bifrost leads seven profiles, ties one, and trails two.

The pooled advantage is concentrated: Ruby contributes 10 of the net 20-case
gap and Java contributes 7. Without Ruby, the pooled result is **101/115
(87.8%)** versus **91/115 (79.1%)**. Without both Ruby and Java, it is **90/104
(86.5%)** versus **87/104 (83.7%)**. Removing any one profile leaves Bifrost
ahead, with a pooled gap between 8.7 and 19.5 percentage points. These
leave-out figures measure sensitivity to corpus composition; they are not a
reason to discard any language.

Java is additionally sensitive to the strict contract rather than recall. JDT
LS returns every expected Java usage site. Five cases are non-exact because its
reference ranges contain the expected identifier but span a qualified name or
complete invocation. The two hard Java cases group interface and
override-family calls beyond the authored static identity. A previous field
disagreement was an authored-operation mismatch: the case now uses Definition,
which JDT LS and Bifrost both satisfy exactly.

The user-facing destination metric supports a corpus-bounded parity conclusion;
the strict view shows higher Bifrost conformance on most current profiles.
Neither establishes general superiority. The corpus is small,
analyzer-informed, and still underrepresents compiler-generated and
external-dependency semantics.

## Authored-operation coverage

Exactness is conditional on an operation being scoreable. Across all 158
authored cases, Bifrost can score 152 (96.2%). The reference-server profiles can
score 131 (82.9%); 23 cases require an operation the corresponding server does
not advertise, and 4 runtime-driven cases are not planned for either side.
Unsupported is a capability boundary rather than an incorrect answer, so it is
reported separately from shared-case exactness.

## Bifrost full-corpus result

Bifrost can score 152 of the 158 authored cases.

| Language | Exact | Expected gap | Other non-exact | Scoreable | Unsupported | Not planned |
|---|---:|---:|---:|---:|---:|---:|
| C++ | 12 | 1 | 2 | 15 | 1 | 0 |
| C# | 14 | 0 | 2 | 16 | 0 | 0 |
| Go | 9 | 0 | 2 | 11 | 1 | 0 |
| Java | 11 | 0 | 0 | 11 | 0 | 0 |
| JavaScript, TypeScript | 20 | 0 | 2 | 22 | 0 | 1 |
| PHP | 12 | 0 | 2 | 14 | 0 | 0 |
| Python | 10 | 0 | 3 | 13 | 0 | 2 |
| Ruby | 19 | 1 | 0 | 20 | 0 | 1 |
| Rust | 14 | 1 | 0 | 15 | 0 | 0 |
| Scala | 12 | 0 | 3 | 15 | 0 | 0 |
| **Total** | **133** | **3** | **16** | **152** | **2** | **4** |

The three expected gaps are the C++ function-like macro expansion, Ruby
self-construction through `require_relative`, and Rust declarative-macro-
generated function reference. The 16 other non-exact results remain visible as
current analyzer gaps or newly reviewed contract differences.

## Reference-server result

The ten primary profiles can score 131 cases. Another 23 require an operation
the server does not advertise, and 4 runtime-driven cases are not planned.

| Language | Server | Exact | Position unverified | Hard | Scoreable | Unsupported | Not planned |
|---|---|---:|---:|---:|---:|---:|---:|
| C++ | clangd | 12 | 0 | 3 | 15 | 1 | 0 |
| C# | Roslyn | 13 | 0 | 3 | 16 | 0 | 0 |
| Go | gopls | 6 | 0 | 0 | 6 | 6 | 0 |
| Java | Eclipse JDT LS | 4 | 5 | 2 | 11 | 0 | 0 |
| JavaScript, TypeScript | TypeScript LS | 13 | 2 | 2 | 17 | 5 | 1 |
| PHP | Intelephense | 9 | 0 | 1 | 10 | 4 | 0 |
| Python | Pyright | 13 | 0 | 0 | 13 | 0 | 2 |
| Ruby | Ruby LSP | 5 | 3 | 8 | 16 | 4 | 1 |
| Rust | rust-analyzer | 12 | 0 | 3 | 15 | 0 | 0 |
| Scala | Metals | 9 | 0 | 3 | 12 | 3 | 0 |
| **Total** | **10 servers** | **96** | **10** | **25** | **131** | **23** | **4** |

The previous `policy near` category is now zero by construction. Import,
re-export, and export-metadata locations are classified as optional bindings:
they remain recorded in raw results but do not make an otherwise exact case
non-exact. The ten `position_unverified` results are also shown separately:
they reached the expected path and line but returned either line-only locations
or broader ranges containing the expected token, so they should not be read as
equivalent to the 25 hard disagreements.

## Version envelope

| Server | Requested release | Server-reported release |
|---|---|---|
| clangd | 22.1.6 | Apple clangd 21.0.0 |
| gopls | 0.23.0 | v0.23.0 |
| rust-analyzer | 2026-07-13 | 0.3.2971-standalone, 2026-07-13 |
| TypeScript language server | 5.3.0 with TypeScript 5.9.3 | Not reported |
| Pyright | 1.1.411 | Not reported |
| Intelephense | 1.18.5 | Not reported |
| Ruby LSP | 0.26.10 | 0.26.10 |
| Eclipse JDT LS | 1.61.0-202607142124 | 1.61.0-SNAPSHOT |
| Roslyn | vscode-csharp 2.140.9 | Not reported |
| Metals | 1.6.8 | 1.6.8 |

The clangd row is explicitly for the resolved Apple clangd build, not upstream
clangd 22.1.6. Package-launched servers retain their exact requested versions
even when the protocol does not report a version.

## What the aggregate hides

The [case comparison](case-comparison/) separates Bifrost-only exact, LSP-only
exact, neither-exact, unsupported, and not-planned cases. The language pages
explain the reviewed semantics behind important deltas.

No current result measures indexing time, warm-query latency, peak memory,
external dependencies, or broad real-world accuracy. Compiler-backed language
servers are also likely to be stronger on macro expansion, generated
declarations, synthetic members, conditional compilation, and SDK symbols.
Those areas should grow as reviewed parity cases rather than being inferred from
this score.
