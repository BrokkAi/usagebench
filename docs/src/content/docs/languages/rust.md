---
title: Rust — Bifrost and rust-analyzer
description: Compare re-exports, traits, UFCS, Self aliases, modules, and generated code.
---

| Runner | Exact | Position unverified | Hard or expected gap | Unsupported |
|---|---:|---:|---:|---:|
| Bifrost | 14 | 0 | 1 expected | 0 |
| rust-analyzer | 12 | 0 | 3 hard | 0 |

## rust-analyzer recall edges

Import and re-export bindings are optional in the current scorer. Both analyzers
are exact on 11 cases. Bifrost alone is exact on the two associated-type
navigation cases and the UFCS trait-method-through-barrel case.

rust-analyzer alone is exact on the macro-generated function case, Bifrost's one
expected Rust gap.

These results are consistent with compiler-aware re-export and trait resolution.

## Bifrost precision and navigation edges

Bifrost's separating exact results keep associated-type owner identity and the
UFCS trait member narrower than rust-analyzer. These are reviewed navigation and
reference-set differences, not claims that rust-analyzer lacks the underlying
semantic model.

## Approximation assessment

The remaining Bifrost miss is the declarative-macro-generated function. The
rust-analyzer extras reflect `Self` equivalence, trait-declaration inclusion,
and navigation policy. No current case establishes flow or object sensitivity.

## Macro-generated minimal pair

The scored direct and `macro_rules!`-generated function cases have identical
file-backed declaration/reference expectations. rust-analyzer resolves both
exactly. Bifrost resolves the direct control but misses the generated call and
its reverse definition lookup, so the generated case is an expected failure.
This is one declarative-macro scenario, not broad proc-macro or derive coverage.

## Architecture tradeoff

rust-analyzer's semantic project model provides macro expansion and name
resolution. Bifrost's persistent syntax/fact index can operate without a full
compiler workspace and produces a deliberately narrow usage surface, but it must
implement re-export and trait canonicalization itself. Performance costs have
not been measured here.
