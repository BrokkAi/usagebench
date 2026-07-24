---
title: Python — Bifrost and Pyright
description: Compare imports, barrels, aliases, class methods, properties, and cursor-addressable modules.
---

| Runner | Exact | Position unverified | Hard | Unsupported | Not planned |
|---|---:|---:|---:|---:|---:|
| Bifrost | 10 | 0 | 3 | 0 | 2 |
| Pyright | 13 | 0 | 0 | 0 | 2 |

## Agreement behind import policy

Import bindings, re-export bindings, and `__all__` metadata are optional in the
hardened scorer. With those surfaces classified, Pyright is exact on all 13
scoreable Python cases.

## Current Pyright edge

Pyright alone is exact on the module-import declaration, re-exported class-alias
classmethod, and barrel-inherited member cases. Bifrost currently misses an
alias-site usage in the classmethod case, returns a broader inherited-member
usage, and cannot complete the module declaration lookup from the import token.

## Shared dynamic boundary

Dynamic `getattr` and `__getattr__` cases are not planned for either runner.
Their presence keeps the corpus honest about runtime-driven attribute lookup.

## Architecture tradeoff

Bifrost's source-location runner and Python graph can canonicalize many project
re-exports and aliases, but the synchronized run exposes three remaining gaps.
Pyright's compiler-style type analysis is the stronger current result on this
small corpus. Comparative performance is unmeasured.
