---
title: JavaScript and TypeScript — Bifrost and TypeScript LS
description: Compare ES modules, CommonJS, barrels, types, JSX, and conservative receiver evidence.
---

| Runner | Exact | Position unverified | Hard | Unsupported | Not planned |
|---|---:|---:|---:|---:|---:|
| Bifrost | 20 | 0 | 2 | 0 | 1 |
| TypeScript LS | 13 | 2 | 2 | 5 | 1 |

## TypeScript agreement after import policy

Import and re-export bindings are now optional in the scorer, so the former nine
policy near misses no longer obscure semantic agreement. On the 17 cases both
sides can score, 11 are exact for both, 4 only for Bifrost, and 2 only for the
TypeScript language server.

## CommonJS split

Bifrost satisfies the destructured-function and barrel-class CommonJS cases
that TypeScript LS omits. TypeScript LS is exact on the barrel-member call that
current Bifrost cannot navigate immediately after `new Client()`.

This remains more informative than a single “CommonJS support” label: the
three shapes separate the analyzers in both directions.

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
