---
title: Scala — Bifrost and Metals
description: Compare imports, companion objects, synthetic apply calls, traits, and build hydration.
---

This profile compares Bifrost with Metals across twelve shared Scala usage and
navigation cases. The fixtures cover imports, companions, extension methods,
traits, overrides, case-class members, and synthetic `apply` calls.

| Runner | Required destinations found | Strict exact | Hard | Outside shared: unsupported |
|---|---:|---:|---:|---:|
| Bifrost | 11/12 | 10 | 2 | 0 |
| Metals 1.6.8 | 10/12 | 9 | 3 | 3 |

The [Metals 1.6.8 release](https://scalameta.org/metals/blog/) was rerun against
the complete Scala corpus after its release. It produced the same case-level
outcomes as 1.6.7: 9 exact, 3 hard non-exact, 3 unsupported, and no runner
errors.

## Readiness first

Metals initially had no usable build target. The runner now accepts its build
import prompt, continues serving bidirectional requests, and waits for the SBT
workspace. Its measured results begin only after that hydration; the earlier
state was a harness failure, not an analyzer verdict.

## Current split

On the user-facing metric, Bifrost finds every required destination in 11 of
the 12 shared cases and Metals in 10. Both are strictly exact on 8. Bifrost
alone is exact on class construction and companion `apply`; Metals alone is
exact on generated case-class construction and `copy`. Neither is exact on the
trait-method implementation case.

Metals' class-construction case still counts as a destination success because
both required targets are present alongside one additional result. Its two
destination misses are the trait implementation edge and the synthetic
companion `apply` call.

## Bifrost recall edges

Bifrost's other generated component-access gap falls outside the shared
denominator because Metals does not advertise Declaration. These are distinct
mechanisms: type-family linking and synthetic member modeling should not be
collapsed into a single approximation label.

## Fairness gap

The current case-class controls provide a direct generated-member comparison,
but broader compiler-generated and SDK semantics remain underrepresented.

## Architecture tradeoff

Metals relies on a real Scala build import and compiler ecosystem. Bifrost's
language-specific graph obtains the measured edges from indexed source facts
without the same build target, but must reproduce selected Scala conventions
such as companions and traits itself. The current benchmark contains no
comparative startup, query-latency, or memory measurements.
