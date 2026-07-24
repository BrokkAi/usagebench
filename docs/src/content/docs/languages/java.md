---
title: Java — Bifrost and JDT LS
description: Compare nested types, static imports, receiver identity, and implementation families.
---

This profile compares Bifrost with Eclipse JDT LS across eleven reviewed Java
usage and navigation cases. The fixtures cover method calls, static fields and
imports, nested types, lambdas, and interface/implementation identity.

| Runner | Required destinations found | Strict exact | Position unverified | Hard |
|---|---:|---:|---:|---:|
| Bifrost | 11/11 | 11 | 0 | 0 |
| JDT LS | 11/11 | 4 | 5 | 2 |

“Required destinations found” asks whether all reviewed targets were surfaced,
while “strict exact” additionally requires complete identifier ranges, no
unallowed extras, and singleton navigation.

## Result summary

Both analyzers surface every required Java destination. Bifrost also satisfies
the strict contract in all eleven cases. JDT LS is exact on four,
position-unverified on five, and hard non-exact on two. The difference is not a
recall gap; it comes from returned range shape and interface-family grouping.

## What separates the results

The seven cases that are exact only for Bifrost break down as follows:

| Distinction | Cases | What JDT LS returns |
|---|---:|---|
| Broader containing ranges | 5 | The expected identifier is present, but the location spans the full invocation or qualified name. All five definition lookups pass. |
| Interface/override-family grouping | 2 | References from either `Handler.handle` or `ConsoleHandler.handle` include all three interface-typed and concrete calls. |

Definition navigation from both interface-typed calls reaches
`Handler.handle`; the case remains hard only because the references response
also includes the concrete `direct.handle` family member.

## Why Definition is used

LSP added the optional
[`textDocument/declaration`](https://microsoft.github.io/language-server-protocol/specifications/specification-3-14/#goto-declaration-request-leftwards_arrow_with_hook)
request in version 3.14, after Definition was already the conventional source
navigation request. The protocol does not prescribe one cross-language meaning
for the distinction.

JDT LS uses Declaration narrowly to find a method's overridden declaration; it
does not provide field declarations through that endpoint. These cases instead
ask for an ordinary source target, so all Java usage lookups now explicitly use
`textDocument/definition`. The benchmark still keeps Declaration separate for
cases where a declaration/definition distinction is itself under review.
Previously this was asymmetric: a server that did not advertise Declaration
would be reported as unsupported and excluded from the shared denominator,
while JDT LS was scored as wrong because it advertised the endpoint with
narrower semantics. Using Definition for the ordinary Java targets removes that
capability-advertisement penalty.

## Reproduction finding

The JDT profile now places each Eclipse `-data` directory beside its generated
source root rather than inside it. The corrected run created a distinct data
directory for each Java document and imported all three fixtures as Maven
projects. It produced 4 exact / 5 position-unverified / 2 hard results, with no
runner or project-configuration errors.

## Approximation assessment

The JDT result shows hierarchy-family grouping and range-shape differences—not
proven object insensitivity or broad Java semantic failure. JDT's reference
handler forwards Eclipse search-match spans without narrowing them to the
terminal identifier. Bifrost's earlier nested-type extraction gap is resolved
in the pinned Bifrost run.

## Architecture tradeoff

JDT LS benefits from a hydrated Java project and compiler semantics. Bifrost's
narrow identities are produced by its indexed usage graph without requiring a
long-lived compiler workspace. UsageBench does not yet measure whether either
approach is faster or smaller.
