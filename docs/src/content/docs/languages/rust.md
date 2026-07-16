---
title: Rust — Bifrost and rust-analyzer
description: Compare re-exports, traits, UFCS, Self aliases, modules, and generated code.
---

| Runner | Exact | Policy near | Hard or expected gap | Not planned |
|---|---:|---:|---:|---:|
| Bifrost | 13 | 0 | 1 expected gap | 0 |
| rust-analyzer | 9 | 2 | 3 | 0 |

## rust-analyzer recall edges

rust-analyzer finds every required `Worker` qualifier through the chained trait
re-export and is a policy near miss only because it also returns re-export
bindings. Bifrost satisfies those qualifiers as an expected baseline pass.

For the UFCS trait-method-through-barrel case, rust-analyzer finds both expected
calls but also returns the trait declaration. Its hard status therefore combines
successful recall with a reference-set precision difference. Bifrost now
satisfies the authored contract as an expected baseline pass.

These results are consistent with compiler-aware re-export and trait resolution.

## Bifrost precision and navigation edges

Bifrost keeps `rust-struct-construction` to the authored type usages.
rust-analyzer also returns a re-export plus `Self`/declaration-like locations;
only the re-export is policy-allowed.

For module navigation, Bifrost returns the authored `mod workflow` declaration.
rust-analyzer opens the module file at its start. Both surfaces are useful, but
only the former matches this benchmark's declaration target.

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
