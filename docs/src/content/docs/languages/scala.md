---
title: Scala — Bifrost and Metals
description: Compare imports, companion objects, synthetic apply calls, traits, and build hydration.
---

| Runner | Exact | Position unverified | Hard | Unsupported |
|---|---:|---:|---:|---:|
| Bifrost | 12 | 0 | 3 | 0 |
| Metals | 9 | 0 | 3 | 3 |

## Readiness first

Metals initially had no usable build target. The runner now accepts its build
import prompt, continues serving bidirectional requests, and waits for the SBT
workspace. Its measured results begin only after that hydration; the earlier
state was a harness failure, not an analyzer verdict.

## Current split

On 12 shared scoreable cases, both are exact on 8. Bifrost alone is exact on
class construction and companion `apply`; Metals alone is exact on generated
case-class construction and `copy`. Neither is exact on the trait-method
implementation case.

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
