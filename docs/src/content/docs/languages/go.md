---
title: Go — Bifrost and gopls
description: Compare package, embedding, receiver, and interface-method reference behavior.
---

| Runner | Exact | Position unverified | Hard | Unsupported |
|---|---:|---:|---:|---:|
| Bifrost | 9 | 0 | 2 | 1 |
| gopls | 6 | 0 | 0 | 6 |

## Strong agreement

Both analyzers can score six common cases and are exact together on five.
gopls alone is exact on the dot-import concrete-receiver call; Bifrost currently
misclassifies the imported `Record` selectors as shadowed local bindings.

## The interface-family split

Six Go cases require the distinct Declaration operation, which gopls does not
advertise. They are reported as unsupported rather than retried through
Definition. Bifrost can score five of those six, but its interface receiver case
still misses the two conservative concrete candidates and the declaration
lookup.

This consistent behavior may reflect an intentional method-family policy. It is
not enough to claim object insensitivity: the case does not vary allocation or
receiver contexts while keeping every other factor fixed.

## Relative strengths

Bifrost exposes a broader operation surface on this corpus; gopls is exact on
every case it can score. The result is therefore a capability distinction plus
one concrete Bifrost navigation gap, not a broad accuracy verdict.

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
