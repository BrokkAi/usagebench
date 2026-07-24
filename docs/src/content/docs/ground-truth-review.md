---
title: Human ground-truth audit
description: Status, procedure, and trust boundary for the first human review of the development corpus.
---

As of 24 July 2026, every current UsageBench case has completed a first
independent human review: **158 cases in 35 schema-v2 documents across 11
languages**. The reviewer was
[`DavidBakerEffendi`](https://github.com/DavidBakerEffendi), and the complete
case-by-case decisions are preserved in the
[review log](https://github.com/BrokkAi/usagebench/blob/main/benchmarks/reviews/2026-07-17-DavidBakerEffendi.md).

This is a meaningful increase in case-level trust, but it is not evaluation
promotion. Every document deliberately remains:

```yaml
corpus:
  partition: development
  selection: analyzer_informed
groundTruth:
  status: legacy_unattributed
  reviewers: []
```

One completed review does not satisfy UsageBench's
`independently_reviewed` contract, and the current cases were not selected
through preregistration.

## What can be trusted now

| Question | Current evidence |
|---|---|
| Is an individual expected token or navigation target suitable for regression testing and diagnosis? | **Yes, with the development-corpus qualification.** A human inspected the fixture and classified the source contract before seeing the measured analyzer outcomes. |
| Can a case-level Bifrost/LSP disagreement be discussed? | **Yes.** The review log records the semantic decision, post-review analyzer checks, and notable policy differences. |
| Do the headline scores reflect the reviewed contracts and hardened scorer? | **Yes, as development evidence.** The synchronized 24 July native run uses exact ranges and strict singleton navigation, but it is not an independently reviewed evaluation result. |
| Is this a general analyzer accuracy ranking? | **No.** The corpus is analyzer-informed, small, and still has known coverage gaps. |
| Is this an independently reviewed evaluation set? | **No.** It still needs a second independent reviewer, preregistered selection, and an immutable freeze. |

The practical result is a reviewed development benchmark, not a publishable
leaderboard. Use it to reproduce a specific edge, guard a regression, or
investigate a contract disagreement. The current synchronized score supports
corpus-bounded parity claims, not general analyzer superiority.

## Review coverage

| Language | Documents | Cases |
|---|---:|---:|
| C++ | 3 | 16 |
| C# | 4 | 16 |
| Go | 3 | 12 |
| Java | 3 | 11 |
| JavaScript | 3 | 11 |
| PHP | 3 | 14 |
| Python | 3 | 15 |
| Ruby | 3 | 21 |
| Rust | 4 | 15 |
| Scala | 3 | 15 |
| TypeScript | 3 | 12 |
| **Total** | **35** | **158** |

## Review procedure

Each case followed the same evidence order:

1. Inspect the fixture and authored locations without revealing Bifrost or LSP
   outcomes.
2. Classify required usages, optional bindings, conservative unproven
   candidates, and excluded false positives.
3. Choose the source operation: declaration, definition, type definition, or
   reviewed no movement.
4. Verify that the fixture compiles or that the language server loaded the
   intended project context.
5. Reveal and compare the pinned reference LSP and Bifrost results.
6. Preserve genuine policy differences and file narrow analyzer issues for
   confirmed gaps.

A popular language server was strong comparison evidence, not automatic ground
truth. Contradicting it required a language-level reason, a minimal
reproduction, compiler behavior, or another authoritative signal.

## What the audit changed

Recurring corrections included:

- separating declaration navigation from executable definition navigation;
- keeping import and re-export bindings optional rather than treating them as
  concrete usages;
- distinguishing class tokens, constructor calls, and explicit constructor
  bodies;
- retaining static interface identity at dynamically ambiguous call sites
  while resolving statically concrete receivers;
- excluding declarations, override bodies, and same-spelled local symbols from
  ordinary usage sets;
- representing conservative implementation-family candidates separately from
  required usages; and
- treating project hydration, request ordering, unsupported LSP operations,
  and imprecise ranges as harness or capability evidence rather than semantic
  failures.

The [case comparison](results/case-comparison/) relates the qualitative
adjudication notes to the synchronized 24 July run.

## Path to an evaluation release

Promotion requires a new evidence phase:

1. preregister the selected cases and assertions;
2. obtain a second independent review of every frozen assertion;
3. adjudicate disagreements without using analyzer output to define truth;
4. assign an immutable `freezeId`; and
5. rerun all analyzers under the hardened exact-range scorer and versioned
   reference environments.

Until then, the correct public description is: **fully first-reviewed
development corpus with a synchronized exact-range regression run**.
