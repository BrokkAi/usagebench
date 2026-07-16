---
title: JavaScript and TypeScript — Bifrost and TypeScript LS
description: Compare ES modules, CommonJS, barrels, types, JSX, and conservative receiver evidence.
---

| Runner | Exact | Policy near | Hard or expected gap | Not planned |
|---|---:|---:|---:|---:|
| Bifrost | 20 | 0 | 1 expected gap | 1 |
| TypeScript LS | 10 | 9 | 2 | 1 |

## TypeScript agreement after import policy

Most TypeScript differences are not semantic misses. TypeScript LS returns
import or re-export bindings in nine otherwise complete cases involving named
exports, default imports, JSX, static methods, chained barrels, and type
annotations. Bifrost deliberately filters those binding-only locations from its
usage surface.

Once that policy is separated, the TypeScript corpus has strong agreement.

## Split CommonJS strengths

Bifrost satisfies the destructured-function and barrel-class cases that
TypeScript LS omits. Its JavaScript usage graph contains explicit CommonJS
binding/export handling, which supports those expected edges.

TypeScript LS, however, exactly resolves `js-commonjs-barrel-member-call`.
Bifrost finds the factory-result member candidate but retains it as unproven, so
it does not satisfy the case's proven expectation.

This split is more informative than saying either analyzer “supports CommonJS.”
Different binding and receiver-return shapes exercise different machinery.

## Approximation assessment

The factory-result case is compatible with a conservative return-summary or
receiver-provenance boundary, but it does not prove flow insensitivity. The
destructuring/barrel misses are compatible with CommonJS binding-resolution
boundaries, not necessarily value-flow limitations.

## Architecture tradeoff

Bifrost exposes proven versus unproven graph candidates and has a bounded,
demand-driven JavaScript/TypeScript receiver provider rather than general
whole-program points-to. TypeScript LS draws on the TypeScript compiler's module
and type system. The benchmark currently measures result precision/recall only;
it does not quantify the cost of either model.

## Next isolating cases

- Direct export versus one- and two-hop CommonJS barrels.
- Factory returns through straight-line, reassigned, and branched variables.
- TypeScript project references and generated declaration files.
