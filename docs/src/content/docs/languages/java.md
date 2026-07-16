---
title: Java — Bifrost and JDT LS
description: Compare nested types, static imports, receiver identity, and implementation families.
---

| Runner | Exact | Policy near | Hard or expected gap |
|---|---:|---:|---:|
| Bifrost | 9 | 0 | 2 expected gaps |
| JDT LS | 9 | 1 | 1 |

## JDT LS recall edge

JDT LS exposed two real omissions in the authored corpus and then satisfied the
corrected expectations exactly:

- two `Service` qualifiers in `Service.Repository`; and
- the nested `Repository` field and constructor-parameter type usages.

Bifrost misses those four type-reference locations. These are confirmed Bifrost
gaps in the current fixture, not LSP-policy disagreements.

## Bifrost precision edge

For `java-parity-concrete-implementation-method-call`, Bifrost returns the
narrow concrete method call expected by the case. JDT LS also returns a call on
an anonymous implementation. That is consistent with implementation-family
expansion, which may be useful in an editor but is broader than this benchmark's
identity contract.

The static-import case is otherwise complete; JDT LS additionally returns the
import binding and is therefore a policy near miss.

## Approximation assessment

The Bifrost nested-type misses point to incomplete type-reference extraction or
resolution for qualifier/field/parameter positions. The evidence does not
implicate flow analysis. The JDT result shows hierarchy-family grouping, not
proven object insensitivity.

## Architecture tradeoff

JDT LS benefits from a hydrated Java project and compiler semantics. Bifrost's
narrow method identity is produced by its indexed usage graph without requiring
a long-lived compiler workspace, but the same fact extraction currently misses
some nested-type positions. UsageBench does not yet measure whether either
approach is faster or smaller.
