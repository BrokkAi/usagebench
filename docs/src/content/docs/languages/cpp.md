---
title: C++ — Bifrost and clangd
description: Compare declaration identity, out-of-line definitions, constructors, aliases, and overrides.
---

| Runner | Exact | Position unverified | Hard or expected gap | Unsupported |
|---|---:|---:|---:|---:|
| Bifrost | 12 | 0 | 2 hard, 1 expected | 1 |
| clangd | 12 | 0 | 3 hard | 1 |

## Where Bifrost satisfies more cases

Bifrost and clangd are both exact on 12 of 15 scoreable cases, with 11 cases
exact for both. Bifrost alone is exact on the concrete-override case; clangd
alone is exact on the function-like macro-expanded call. Both remain non-exact
on the class-reference and using-alias-constructor contracts.

The clangd pattern is not evidence that it cannot resolve the program. With
`includeDeclaration: false`, its references result omits several out-of-line
definitions that UsageBench expects, while other queries group class and
constructor-family locations. This is largely a public reference-set semantics
difference.

## Where clangd may have broader capability

clangd is compiler-backed and can consume compilation databases, headers,
templates, preprocessing, and macro expansion in editor workspaces. The new
source-backed minimal pair provides one narrow example: both runners resolve a
direct function call, while clangd alone resolves the same function name when
it is supplied as a function-like macro argument. One compile-command-dependent
case remains unsupported.

It would therefore be unjustified to generalize the current 12/15 exact result
into a claim that Bifrost has stronger C++ analysis overall.

## Approximation assessment

No current failure proves flow or object insensitivity. The observed categories
are declaration exclusion, constructor/class grouping, alias-target navigation,
override-family expansion, and one function-like macro-expansion gap. More
macro forms plus conditional compilation and multiple receiver objects are
still needed before stronger claims.

## Architecture tradeoff

Bifrost obtains the measured precision from language-specific facts over a
Tree-sitter-based index without requiring a successful project build for these
fixtures. clangd's compiler model can expose semantics that syntax-derived facts
cannot reproduce, but it depends more heavily on compile configuration. This is
an architectural explanation; UsageBench has not measured comparative time or
memory.
