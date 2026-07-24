---
title: Go — Bifrost and gopls
description: Compare package, embedding, receiver, and interface-method reference behavior.
---

| Runner | Required destinations found | Strict exact | Position unverified | Hard | Strict unsupported |
|---|---:|---:|---:|---:|---:|
| Bifrost | 9/11 | 9 | 0 | 2 | 1 |
| gopls | 11/11 | 6 | 0 | 0 | 6 |

## Strong agreement

In the compatibility-aware editor view, eleven cases are destination-scoreable
by both analyzers. gopls reaches all eleven destinations; Bifrost misses the
dot-import concrete-receiver call and the interface-family reference set. The
strict canonical comparison still has only six shared cases, where gopls is
exact on all six and Bifrost is exact on five.

## The interface-family split

Six Go cases require the distinct Declaration operation, which gopls does not
advertise. They are reported as unsupported rather than retried through
Definition. Bifrost can score five of those six, but its interface receiver case
still misses the two conservative concrete candidates and the declaration
lookup.

Five of those canonical Declaration contracts also record reviewed Definition
compatibility. They raise the shared destination-scoreable denominator from six
to eleven, but do not change the strict gopls Declaration denominator or
capability result.

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
