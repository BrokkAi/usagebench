---
title: PHP — Bifrost and Intelephense
description: Compare namespaces, imports, traits, interfaces, receivers, properties, and static members.
---

| Runner | Exact | Policy near | Hard | Not planned |
|---|---:|---:|---:|---:|
| Bifrost | 12 | 0 | 0 unexpected | 1 |
| Intelephense | 9 | 1 | 2 | 1 |

## Shared strengths

Both analyzers satisfy class construction, constants, ordinary methods and
properties, repository calls, static qualifiers, static properties, trait
method calls, and `use`-alias static calls.

Intelephense's imported-function case contains every required call plus the
`use function` binding, so it is a policy-only near miss.

## Bifrost recall edge

Bifrost returns the interface implementation reference and the call through an
interface-typed receiver expected by the two remaining cases. Intelephense omits
those edges, with one associated reverse-navigation miss.

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
