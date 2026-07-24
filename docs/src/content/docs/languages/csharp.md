---
title: C# — Bifrost and Roslyn
description: Compare project-loaded Roslyn references with Bifrost's narrower usage identities.
---

| Runner | Exact | Position unverified | Hard | Unsupported |
|---|---:|---:|---:|---:|
| Bifrost | 14 | 0 | 2 | 0 |
| Roslyn | 13 | 0 | 3 | 0 |

## Shared strengths

After the runner restored the project, sent Roslyn's project-open notification,
waited for project initialization, and attached project context, both analyzers
resolved ordinary classes, constructors, methods, properties, attributes,
extensions, partial properties, constants, and repository calls well.

This readiness change matters: the earlier poor Roslyn result was a harness and
workspace-loading problem, not evidence about C# semantic quality.

## Bifrost precision edge

Bifrost is exact on all three interface/concrete implementation-family cases
where Roslyn returns broader related calls. Roslyn is exact on the namespace-
alias constructor and generic extension cases that currently fail Bifrost
navigation. Eleven cases are exact for both.

Those are contract differences, not proven Roslyn defects. An editor can
reasonably expose related implementations or make the alias declaration the
navigation target.

## Current Bifrost gaps

The namespace-alias constructor currently navigates to the aliased namespace
rather than the class, and the extension-method call does not resolve its
receiver. Both are exact in Roslyn and remain concrete Bifrost follow-up work.

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
