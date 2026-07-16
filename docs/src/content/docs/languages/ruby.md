---
title: Ruby — Bifrost and Ruby LSP
description: Compare constants, mixins, singleton members, aliases, generated readers, and dynamic calls.
---

| Runner | Exact | Hard or expected gap | Not planned |
|---|---:|---:|---:|
| Bifrost | 18 | 2 expected gaps | 1 |
| Ruby LSP | 1 | 19 | 1 |

## Bifrost corpus strengths

Bifrost's Ruby-specific usage graph satisfies reviewed cases involving nested
and script-level constants, superclass references, `include`, `prepend`,
`extend`, singleton methods and fields, instance and class variables, autoload,
generated attribute readers, method aliases, module functions, and lexical
factory constants.

Ruby LSP often omits the expected edge or returns a declaration/same-name
location despite `includeDeclaration: false`. Those observations explain its
contract disagreement; they do not prove that one common approximation causes
all nineteen cases.

## Bifrost weaknesses

- `ruby-factory-return-member-call` is retained only as an unproven Bifrost
  candidate, not a proven usage.
- `ruby-require-relative-class-construction` includes a real `Invoice`
  self-construction that Bifrost misses.
- Dynamic `public_send` remains not planned.

Bifrost therefore has much stronger coverage on this fixture, but not complete
Ruby runtime modeling.

## Approximation assessment

It would be especially risky to label Ruby LSP flow- or object-insensitive from
this result. The failures span constant lookup, metaprogrammed declarations,
mixin precedence, aliasing, and singleton/class scopes. Several independent
indexing or references-contract boundaries are more plausible than one global
mechanism.

Minimal pairs should isolate direct versus factory receiver creation, lexical
versus inherited constants, one mixin at a time, and generated versus explicit
reader methods.

## Architecture tradeoff

Bifrost spends implementation complexity on explicit Ruby language facts and
can expose dynamic candidates as unproven rather than definite. Ruby LSP may
prioritize editor navigation surfaces and runtime-assisted workflows outside
this corpus. Neither scalability nor large-application indexing cost has been
compared.
