---
title: Python — Bifrost and Pyright
description: Compare imports, barrels, aliases, class methods, properties, and cursor-addressable modules.
---

| Runner | Exact | Policy near | Hard | Unsupported | Not planned |
|---|---:|---:|---:|---:|---:|
| Bifrost | 11 | 0 | 0 unexpected | 0 | 2 |
| Pyright | 6 | 4 | 1 | 1 | 2 |

## Agreement behind import policy

Four Pyright cases are complete except for import bindings, re-export bindings,
or `__all__` metadata. They cover direct class construction and one- or two-hop
barrels. Bifrost filters those binding surfaces from `scan_usages` by product
policy.

## Bifrost identity edge

`python-parity-reexported-class-alias-classmethod` passes in Bifrost. Pyright
omits two alias-site references and returns one original-symbol location outside
the binding policy. The observed distinction is alias/re-export canonical
identity, not a demonstrated flow approximation.

Bifrost also executes `python-module-import`, whose authored declaration selector
is zero-width and resolved by symbol selection. A standard LSP references request
needs a cursor on a source token, so the generic Pyright runner reports the case
unsupported instead of querying an unrelated token on the line.

## Shared dynamic boundary

Dynamic `getattr` and `__getattr__` cases are not planned for either runner.
Their presence keeps the corpus honest about runtime-driven attribute lookup.

## Architecture tradeoff

Bifrost's source-location runner can recover a symbol from an authored line and
kind, and its Python graph explicitly canonicalizes project re-exports and
aliases. Pyright's compiler-style type analysis has broader potential on typed
Python programs and external environments, but that is not represented by this
small corpus. Comparative performance is unmeasured.
