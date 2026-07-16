---
title: Result snapshot
description: Current Bifrost and language-server results, with policy-only near misses separated from hard disagreements.
---

This snapshot was captured on macOS arm64 on 2026-07-16. Every measured server
completed with zero runner errors. “Allowed-policy” means that all required
locations and navigation checks passed and the only extras were import bindings,
re-export bindings, or export metadata.

| Language | Server | Exact | Allowed-policy | Hard disagreement | Planned |
|---|---|---:|---:|---:|---:|
| C++ | clangd | 4 | 0 | 9 | 13 |
| Go | gopls | 9 | 0 | 1 | 10 |
| Rust | rust-analyzer | 7 | 2 | 3 | 12 |
| JavaScript, TypeScript | TypeScript language server | 10 | 9 | 2 | 21 |
| Python | Pyright | 6 | 4 | 1 | 11 |
| PHP | Intelephense | 9 | 1 | 2 | 12 |
| Ruby | Ruby LSP | 1 | 0 | 19 | 20 |
| Java | Eclipse JDT LS | 9 | 1 | 1 | 11 |
| C# | Roslyn | 11 | 0 | 3 | 14 |
| Scala | Metals | 8 | 2 | 2 | 12 |
| **Total** | **10 servers** | **74** | **19** | **43** | **136** |

There are also 8 not-planned and 3 unsupported LSP cases outside the planned
denominator. The same corpus run against Bifrost commit
`4051809aea27b59accb2180a29a6ef2b365f1613` produced 128 exact passes, 9 known
expected failures, 8 not-planned cases, 2 unsupported cases, and no unexpected
hard failures or runner errors.

## Version envelope

| Server | Requested release | Server-reported release |
|---|---|---|
| clangd | 22.1.6 | Apple clangd 21.0.0 |
| gopls | 0.23.0 | v0.23.0 |
| rust-analyzer | 2026-07-13 | 0.3.2971-standalone |
| TypeScript language server | 5.3.0 with TypeScript 5.9.3 | Not reported |
| Pyright | 1.1.411 | Not reported |
| Intelephense | 1.18.5 | Not reported |
| Ruby LSP | 0.26.10 | 0.26.10 |
| Eclipse JDT LS | 1.61.0-202607142124 | 1.61.0-SNAPSHOT |
| Roslyn | vscode-csharp 2.140.9 | Not reported |
| Metals | 1.6.7 | 1.6.7 |

The clangd row is explicitly a result for the resolved Apple clangd build, not
for upstream clangd 22.1.6. Missing server-reported versions are retained as
missing rather than replaced with the requested release.

## What the aggregate hides

The hard-disagreement column combines several different phenomena:

- missing reviewed usage locations;
- extra declarations despite `includeDeclaration: false`;
- intentional implementation-family or constructor grouping;
- alias or module navigation to a related surface;
- a selector that cannot be represented as an LSP cursor token; and
- dynamic-language or generated-symbol boundaries.

Consequently, `74/136` is not a global LSP accuracy score, and `128/137` is not
a global Bifrost accuracy score. Read the [case comparison](case-comparison/)
and the language pages before interpreting a row.

## Cross-analyzer pattern summary

| Pattern | Evidence in this snapshot | Interpretation |
|---|---|---|
| Binding-surface policy | 19 LSP near misses | The server includes imports/re-exports that UsageBench excludes from true usages. Not a correctness verdict. |
| Implementation-family expansion | Go, Java, C#, Rust | Related interface, trait, anonymous, or concrete members are grouped. This is not by itself object-insensitivity. |
| Alias/re-export identity | C++, C#, Python, Rust | Navigation or reference identity stops at a binding, original symbol, or module surface. |
| CommonJS resolution | JavaScript | Each analyzer wins a different CommonJS case; no single “JS support” score explains the split. |
| Dynamic Ruby semantics | Ruby | Bifrost's language-specific graph covers many corpus constructs that Ruby LSP omits or broadens, while two Bifrost cases remain conservative/missing. |
| Build and readiness | Roslyn, Metals, TypeScript LS, rust-analyzer | Correct project hydration and settle behavior materially changed results; runner readiness must be removed before correctness interpretation. |

## Missing evidence

The current comparison does not measure time or memory, and it underrepresents
macro expansion, generated declarations, synthetic members, conditional
compilation, and external dependency symbols. Those gaps matter because they
are plausible strengths of compiler-backed language servers.
