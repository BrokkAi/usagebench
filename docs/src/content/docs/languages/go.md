---
title: Go — Bifrost and gopls
description: Compare package, embedding, receiver, and interface-method reference behavior.
---

| Runner | Exact | Hard or expected gap | Unsupported |
|---|---:|---:|---:|
| Bifrost | 9 | 1 expected gap | 1 |
| gopls | 9 | 1 | 1 |

## Strong agreement

Both analyzers satisfy package functions and values, struct fields, value and
pointer receiver methods, dot imports, cross-package aliases, and embedded
promoted fields and methods. This is one of the closest comparisons in the
snapshot.

## The shared interface-family disagreement

For `go-interface-receiver-method-call`, both analyzers add two calls made on
concrete receivers when the queried declaration is the interface method.
Bifrost preserves those as unproven candidates; gopls returns them as ordinary
reference locations. Neither result exactly matches UsageBench's narrow
interface-declaration identity.

This consistent behavior may reflect an intentional method-family policy. It is
not enough to claim object insensitivity: the case does not vary allocation or
receiver contexts while keeping every other factor fixed.

## Relative strengths

Bifrost's explicit proof tier is useful when a consumer wants definite edges
and review candidates separately. gopls's single reference set is simpler for
editor use and may intentionally favor discovering related implementations.

## Architecture tradeoff

gopls uses Go's package and type-checking ecosystem. Bifrost uses indexed
language-specific package, embedding, and receiver facts and can retain
ambiguity as unproven output. The current fixture shows equivalent broad
coverage, not a performance comparison.

## Next isolating cases

- Two implementations with the same method and calls through both interface and
  concrete variables.
- A type assertion that narrows one implementation.
- Build-tag variants with a portable runner policy.
