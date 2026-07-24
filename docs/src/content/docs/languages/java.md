---
title: Java — Bifrost and JDT LS
description: Compare nested types, static imports, receiver identity, and implementation families.
---

| Runner | Exact | Position unverified | Hard | Unsupported |
|---|---:|---:|---:|---:|
| Bifrost | 11 | 0 | 0 | 0 |
| JDT LS | 3 | 5 | 3 | 0 |

## Bifrost reaches the reviewed Java baseline

JDT LS exposed two real omissions during the audit:

- two `Service` qualifiers in `Service.Repository`; and
- the nested `Repository` field and constructor-parameter type usages.

Bifrost now satisfies all eleven reviewed Java cases, including those corrected
type-reference locations.

## Bifrost precision edge

JDT LS is exact on three cases, position-unverified on five, and hard non-exact
on three. The exact-range results include implementation-family expansion,
missing Declaration targets, and multi-target navigation ranges. Import
bindings are optional and no longer create policy near misses.

## Approximation assessment

The JDT result shows hierarchy-family grouping and range/operation differences,
not proven object insensitivity. Bifrost's earlier nested-type extraction gap is
resolved in the current `origin/master` run.

## Architecture tradeoff

JDT LS benefits from a hydrated Java project and compiler semantics. Bifrost's
narrow identities are produced by its indexed usage graph without requiring a
long-lived compiler workspace. UsageBench does not yet measure whether either
approach is faster or smaller.
