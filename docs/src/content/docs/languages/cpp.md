---
title: C++ — Bifrost and clangd
description: Compare declaration identity, out-of-line definitions, constructors, aliases, and overrides.
---

| Runner | Exact | Policy near | Hard | Unsupported |
|---|---:|---:|---:|---:|
| Bifrost | 13 planned passes | 0 | 0 unexpected | 1 |
| clangd | 4 | 0 | 9 | 1 |

## Where Bifrost satisfies more cases

Bifrost's C++ usage graph treats an out-of-line definition as a usage of the
header declaration and keeps class, constructor, overload, alias, and receiver
identities narrow enough for the authored cases. That accounts for the function,
method, overload, class, constructor, alias, and override cases that clangd does
not satisfy exactly.

The clangd pattern is not evidence that it cannot resolve the program. With
`includeDeclaration: false`, its references result omits several out-of-line
definitions that UsageBench expects, while other queries group class and
constructor-family locations. This is largely a public reference-set semantics
difference.

## Where clangd may have broader capability

clangd is compiler-backed and can consume compilation databases, headers,
templates, preprocessing, and macro expansion in editor workspaces. The current
portable corpus barely tests those advantages: one compile-command-dependent
case is marked unsupported, and macro-expanded references are not represented.

It would therefore be unjustified to generalize the current 4/13 exact result
into a claim that Bifrost has stronger C++ analysis overall.

## Approximation assessment

No current failure proves flow or object insensitivity. The observed categories
are declaration exclusion, constructor/class grouping, alias-target navigation,
and override-family expansion. Minimal pairs for macro expansion, conditional
compilation, and multiple receiver objects should be added before stronger
claims.

## Architecture tradeoff

Bifrost obtains the measured precision from language-specific facts over a
Tree-sitter-based index without requiring a successful project build for these
fixtures. clangd's compiler model can expose semantics that syntax-derived facts
cannot reproduce, but it depends more heavily on compile configuration. This is
an architectural explanation; UsageBench has not measured comparative time or
memory.
