---
title: Current synchronized result
description: Exact-range Bifrost and language-server results from the reviewed development corpus on 24 July 2026.
---

> **Development evidence, not an evaluation leaderboard.** All 158 cases have
> completed a first human review, but the corpus remains analyzer-informed and
> `legacy_unattributed`. A second independent review, preregistration, and an
> immutable freeze are still required for evaluation promotion.

This synchronized native run replaces the legacy 16 July figures. Every runner
used the same UsageBench revision and hardened scorer. All eleven processes—one
Bifrost run and ten primary LSP profiles—completed without runner errors.

| Run fact | Value |
|---|---|
| Date | 24 July 2026 |
| UsageBench revision | `78e66e5dd3589d4543f1b19a8b3566fa9afd644a` |
| Bifrost revision | `782522b245fc86e3d39b1cdc0488553a1d262212` (`origin/master`) |
| Host | macOS arm64, native and host-specific |
| Scoring | Exact complete ranges, strict singleton navigation, optional classified bindings |

## Shared-case parity matrix

The primary comparison uses the 131 cases whose authored operation is scoreable
by both Bifrost and the corresponding language server.

| Outcome | Cases |
|---|---:|
| Exact for both | 84 |
| Exact only for Bifrost | 32 |
| Exact only for the language server | 11 |
| Exact for neither | 4 |
| **Shared scoreable total** | **131** |

Bifrost is therefore exact on **116/131** shared cases; the reference LSPs are
exact on **95/131**. Nine of the 32 Bifrost-only exact cases are LSP
`position_unverified` results: the LSP reached the expected line but did not
return the one complete target range required by the contract. The other 23 are
hard LSP disagreements.

This supports the project's parity-or-better direction on the current corpus.
It does not establish general superiority: the corpus is small, analyzer-
informed, and still underrepresents compiler-generated and external-dependency
semantics.

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

## Language-server result

The ten primary profiles can score 131 cases. Another 23 require an operation
the server does not advertise, and 4 runtime-driven cases are not planned.

| Language | Server | Exact | Position unverified | Hard | Scoreable | Unsupported | Not planned |
|---|---|---:|---:|---:|---:|---:|---:|
| C++ | clangd | 12 | 0 | 3 | 15 | 1 | 0 |
| C# | Roslyn | 13 | 0 | 3 | 16 | 0 | 0 |
| Go | gopls | 6 | 0 | 0 | 6 | 6 | 0 |
| Java | Eclipse JDT LS | 3 | 5 | 3 | 11 | 0 | 0 |
| JavaScript, TypeScript | TypeScript LS | 13 | 2 | 2 | 17 | 5 | 1 |
| PHP | Intelephense | 9 | 0 | 1 | 10 | 4 | 0 |
| Python | Pyright | 13 | 0 | 0 | 13 | 0 | 2 |
| Ruby | Ruby LSP | 5 | 3 | 8 | 16 | 4 | 1 |
| Rust | rust-analyzer | 12 | 0 | 3 | 15 | 0 | 0 |
| Scala | Metals | 9 | 0 | 3 | 12 | 3 | 0 |
| **Total** | **10 servers** | **95** | **10** | **26** | **131** | **23** | **4** |

The previous `policy near` category is now zero by construction. Import,
re-export, and export-metadata locations are classified as optional bindings:
they remain recorded in raw results but do not make an otherwise exact case
non-exact.

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
| Metals | 1.6.7 | 1.6.7 |

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
