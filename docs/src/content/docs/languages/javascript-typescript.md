---
title: JavaScript and TypeScript — Bifrost and TypeScript LS
description: Compare ES modules, CommonJS, barrels, types, JSX, and conservative receiver evidence.
---

| Runner | Exact | Policy near | Hard or expected gap | Not planned |
|---|---:|---:|---:|---:|
| Bifrost | 21 | 0 | 0 | 1 |
| TypeScript LS | 10 | 9 | 2 | 1 |

## TypeScript agreement after import policy

Most TypeScript differences are not semantic misses. TypeScript LS returns
import or re-export bindings in nine otherwise complete cases involving named
exports, default imports, JSX, static methods, chained barrels, and type
annotations. Bifrost deliberately filters those binding-only locations from its
usage surface.

Once that policy is separated, the TypeScript corpus has strong agreement.

## CommonJS split

Bifrost satisfies all three authored CommonJS cases. Its JavaScript usage graph
contains explicit CommonJS binding/export handling, which supports the
destructured-function and barrel-class edges that TypeScript LS omits.

Both analyzers exactly resolve `js-commonjs-barrel-member-call`, which is now an
expected Bifrost baseline pass.

This remains more informative than a single “CommonJS support” label: the
barrel-member shape agrees, while destructuring and barrel-class construction
separate the analyzers.

## Approximation assessment

The TypeScript LS destructuring/barrel misses are compatible with CommonJS
binding-resolution boundaries, not necessarily value-flow limitations. The
current cases do not prove a general flow-sensitivity difference.

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
