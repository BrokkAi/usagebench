---
title: C# — Bifrost and Roslyn
description: Compare project-loaded Roslyn references with Bifrost's narrower usage identities.
---

| Runner | Exact | Policy near | Hard | Not planned |
|---|---:|---:|---:|---:|
| Bifrost | 14 | 0 | 0 | 1 |
| Roslyn | 11 | 0 | 3 | 1 |

## Shared strengths

After the runner restored the project, sent Roslyn's project-open notification,
waited for project initialization, and attached project context, both analyzers
resolved ordinary classes, constructors, methods, properties, attributes,
extensions, partial properties, constants, and repository calls well.

This readiness change matters: the earlier poor Roslyn result was a harness and
workspace-loading problem, not evidence about C# semantic quality.

## Bifrost precision edge

Bifrost satisfies the two interface/concrete-method cases with the narrower
authored identity, while Roslyn returns one related implementation-family call
in each. Bifrost also navigates through the namespace alias to the underlying
class declaration expected by the benchmark; Roslyn returns the alias binding.

Those are contract differences, not proven Roslyn defects. An editor can
reasonably expose related implementations or make the alias declaration the
navigation target.

## Resolved extension gap

Roslyn and current Bifrost both exactly resolve
`csharp-generic-extension-call`; it is now an expected pass in the Bifrost
baseline.

## Approximation assessment

Implementation-family expansion is observed. Object insensitivity is not: the
fixture does not isolate two allocation contexts whose identities collapse.
The namespace-alias result is an alias-navigation policy. A minimal pair with
two implementations and two receiver contexts is required before assigning a
receiver-sensitivity label.

## Architecture tradeoff

Roslyn's compiler workspace provides rich C# semantic identity once the full
project is loaded. Bifrost uses a persistent repository index and targeted
language-specific resolution without the same project handshake. Timing and
memory tradeoffs remain unmeasured.
