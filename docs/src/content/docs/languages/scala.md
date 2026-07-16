---
title: Scala — Bifrost and Metals
description: Compare imports, companion objects, synthetic apply calls, traits, and build hydration.
---

| Runner | Exact | Policy near | Hard | Not planned |
|---|---:|---:|---:|---:|
| Bifrost | 12 | 0 | 0 unexpected | 1 |
| Metals | 8 | 2 | 2 | 1 |

## Readiness first

Metals initially had no usable build target. The runner now accepts its build
import prompt, continues serving bidirectional requests, and waits for the SBT
workspace. Its measured results begin only after that hydration; the earlier
state was a harness failure, not an analyzer verdict.

## Policy-only differences

The two renamed/import-alias companion cases contain every required result.
Metals additionally returns the alias binding, so they are near misses rather
than hard failures.

## Bifrost recall edges

Bifrost satisfies the trait-method implementation and companion `apply` cases.
Metals omits the expected implementation references and the synthetic companion
call. These are distinct mechanisms: type-family linking and synthetic member
modeling should not be collapsed into a single approximation label.

## Fairness gap

A generated/synthetic parity case remains not planned. The corpus needs direct
versus compiler-generated minimal pairs before claiming general superiority on
Scala synthetic semantics.

## Architecture tradeoff

Metals relies on a real Scala build import and compiler ecosystem. Bifrost's
language-specific graph obtains the measured edges from indexed source facts
without the same build target, but must reproduce selected Scala conventions
such as companions and traits itself. The current benchmark contains no
comparative startup, query-latency, or memory measurements.
