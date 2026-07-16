---
title: Go — Bifrost and gopls
description: Compare package, embedding, receiver, and interface-method reference behavior.
---

| Runner | Exact | Hard or expected gap | Unsupported |
|---|---:|---:|---:|
| Bifrost | 10 | 0 | 1 |
| gopls | 9 | 1 | 1 |

## Strong agreement

Both analyzers satisfy package functions and values, struct fields, value and
pointer receiver methods, dot imports, cross-package aliases, and embedded
promoted fields and methods. This is one of the closest comparisons in the
snapshot.

## The interface-family split

For `go-interface-receiver-method-call`, current Bifrost satisfies UsageBench's
narrow interface-declaration identity and the case is now an expected baseline
pass. gopls broadens the interface method to two calls made on concrete
receivers and therefore remains a hard contract disagreement.

This consistent behavior may reflect an intentional method-family policy. It is
not enough to claim object insensitivity: the case does not vary allocation or
receiver contexts while keeping every other factor fixed.

## Relative strengths

Bifrost's narrower current result is useful when a consumer wants definite
interface-declaration edges. gopls's broader reference set may intentionally
favor discovering related implementations.

## Architecture tradeoff

gopls uses Go's package and type-checking ecosystem. Bifrost uses indexed
language-specific package, embedding, and receiver facts and can retain
ambiguity separately from proven output. The current fixture isolates one
reference-family policy difference, not a performance comparison.

## Next isolating cases

- Two implementations with the same method and calls through both interface and
  concrete variables.
- A type assertion that narrows one implementation.
- Build-tag variants with a portable runner policy.
