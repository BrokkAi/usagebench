---
title: PHP — Bifrost and Intelephense
description: Compare namespaces, imports, traits, interfaces, receivers, properties, and static members.
---

| Runner | Required destinations found | Strict exact | Position unverified | Hard | Strict unsupported |
|---|---:|---:|---:|---:|---:|
| Bifrost | 12/14 | 12 | 0 | 2 | 0 |
| Intelephense | 14/14 | 9 | 0 | 1 | 4 |

## Shared strengths

Both analyzers can score ten common cases and are exact together on nine.
Bifrost alone is exact on class construction because Intelephense returns both
the class and explicit constructor as navigation targets.

Four canonical Declaration contracts—properties, a constant, and a bodyless
interface member—also record Definition as reviewed compatibility evidence.
They raise the shared destination-scoreable denominator from ten to fourteen;
strict operation precision and capability counts remain unchanged.

## Bifrost recall edge

Bifrost also scores four Declaration-oriented cases that Intelephense does not
advertise. Its two current hard results are exact-range mismatches on ordinary
and static property tokens, both outside the shared denominator.

The Bifrost result is supported by its language-specific namespace, hierarchy,
receiver, and member-resolution facts. The fixture establishes the returned
edges; it does not show that Bifrost models every PHP runtime dispatch rule.

## Approximation assessment

The Intelephense pattern is an interface/implementation and typed-receiver
boundary. Calling it object-insensitive would be backwards and unsupported: the
observed problem is missing family/receiver connections, and the fixture does
not isolate heap contexts.

## Architecture tradeoff

Bifrost's PHP graph can compose indexed hierarchy and receiver facts without a
successful framework build. Intelephense provides editor-oriented PHP semantics
and may apply a narrower references contract. No timing, memory, or large
framework comparison has been run.
