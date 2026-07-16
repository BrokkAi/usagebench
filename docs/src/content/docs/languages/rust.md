---
title: Rust — Bifrost and rust-analyzer
description: Compare re-exports, traits, UFCS, Self aliases, modules, and generated code.
---

| Runner | Exact | Policy near | Hard or expected gap | Not planned |
|---|---:|---:|---:|---:|
| Bifrost | 10 | 0 | 2 expected gaps | 1 |
| rust-analyzer | 7 | 2 | 3 | 1 |

## rust-analyzer recall edges

rust-analyzer finds every required `Worker` qualifier through the chained trait
re-export and is a policy near miss only because it also returns re-export
bindings. Bifrost misses those qualifiers.

For the UFCS trait-method-through-barrel case, rust-analyzer finds both expected
calls but also returns the trait declaration. Bifrost misses the calls. The hard
status therefore hides a real rust-analyzer recall advantage alongside a
reference-set precision difference.

These results are consistent with compiler-aware re-export and trait resolution.

## Bifrost precision and navigation edges

Bifrost keeps `rust-struct-construction` to the authored type usages.
rust-analyzer also returns a re-export plus `Self`/declaration-like locations;
only the re-export is policy-allowed.

For module navigation, Bifrost returns the authored `mod workflow` declaration.
rust-analyzer opens the module file at its start. Both surfaces are useful, but
only the former matches this benchmark's declaration target.

## Approximation assessment

The Bifrost misses are specifically chained re-export canonicalization gaps.
The rust-analyzer extras reflect `Self` equivalence, trait-declaration inclusion,
and navigation policy. No current case establishes flow or object sensitivity.

## Fairness gap

The macro-generated function case is not planned even though rust-analyzer
passes it in this fixture. Macro expansion must become a scored, representative
category before drawing broad Rust conclusions.

## Architecture tradeoff

rust-analyzer's semantic project model provides macro expansion and name
resolution. Bifrost's persistent syntax/fact index can operate without a full
compiler workspace and produces a deliberately narrow usage surface, but it must
implement re-export and trait canonicalization itself. Performance costs have
not been measured here.
