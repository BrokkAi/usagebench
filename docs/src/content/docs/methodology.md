---
title: Comparison methodology
description: Classify observed differences without overclaiming analyzer defects or approximation mechanisms.
---

UsageBench reports contract agreement first and causal interpretation second.
Every current expected location has completed one human source review, but the
corpus remains a development set rather than independently reviewed evaluation
ground truth. An analyzer may also expose a different public grouping policy
without containing an implementation bug.

## Result categories

| Category | Meaning |
|---|---|
| Exact | Required complete token ranges and navigation targets match, with no unallowed extras. Under `bindings_optional`, classified binding/export extras may be present and remain recorded. |
| Position unverified | Path and line agree, but the analyzer did not return enough range information to prove the token. |
| Recall difference | At least one reviewed expected location or target is absent. |
| Precision or identity difference | The analyzer returns another declaration, same-name symbol, constructor, implementation-family member, or other unallowed location. |
| Navigation-target difference | The analyzer navigates to a related but different surface, such as an alias binding or module file. |
| Unsupported | The runner cannot express the authored selector or the server does not advertise the required operation. |
| Harness failure | The server did not become ready, the project did not load, or the protocol failed. This is not scored as an analyzer correctness result. |

Navigation responses are intentionally strict: declaration and definition
requests are not unioned, and an expected target among multiple alternates does
not pass. The selected operation must return exactly one target with the
authored complete range. Every evaluation lookup explicitly selects declaration
or definition; a server that does not advertise that operation is unsupported
for the case rather than silently queried through the other endpoint.

## Corpus partitions and ground truth

Development cases may be analyzer-informed and may retain legacy review notes.
They are appropriate for regression work and diagnosis, but their aggregate is
not an evaluation claim. Each current case document declares this status.

The first human audit is complete for all 158 current cases in 35 documents.
That review corrected and explained individual source contracts, but it does
not change their `development`, `analyzer_informed`, or
`legacy_unattributed` metadata. See the
[human ground-truth audit](../ground-truth-review/) for coverage, procedure, and
the precise trust boundary.

An evaluation document is accepted only when all of these are true:

1. case selection was pre-registered before running the compared analyzers;
2. the document has an immutable `freezeId`;
3. at least two named reviewers independently checked source locations; and
4. disagreements were adjudicated before the freeze.

Changing a frozen assertion creates a new freeze and preserves the old report.
Reports include partition, selection, review status, and reference policy, and
their totals separate development from evaluation cases.

## Import and binding policy

`external_usages` excludes binding-only imports/re-exports;
`bindings_optional` accepts their presence or absence; and `bindings_required`
requires binding locations to be authored as expectations. The current
development corpus uses `bindings_optional`, matching Bifrost's optional binding
surface while keeping all returned binding locations visible for audit.

## Claim strength

Every causal explanation should carry one of these strengths:

1. **Observed:** the report and source establish only the returned, missing, or
   alternate locations.
2. **Supported explanation:** the result forms a consistent pattern and the
   analyzer's public contract or implementation evidence supports the mechanism.
3. **Isolated mechanism:** a minimal-pair fixture changes one semantic dimension
   and reproduces the predicted result.
4. **Confirmed defect:** the relevant project accepts the behavior as a bug, or
   a documented operation fails its own stated contract.

The current language pages mostly use the first two levels. They say “does not
satisfy the UsageBench contract,” not “the LSP is wrong.”

## Approximation labels require a minimal pair

Do not infer **flow insensitivity** merely because a result crosses assignments,
branches, or factory returns. The fixture must hold names and types constant,
vary only control-flow ordering or path feasibility, and produce the predicted
change.

Do not infer **object insensitivity** merely because interface and implementation
members are grouped. The fixture must use two distinguishable allocation or
receiver contexts with the same member name and show context collapse. Many LSPs
intentionally return an implementation family for “find references”; that is a
symbol-family policy, not proof of object-insensitive analysis.

Likewise, distinguish alias canonicalization, declaration inclusion, overload
grouping, generated symbols, and cursor-token limitations before reaching for a
general static-analysis label.

## Calling an analyzer wrong

Use that wording only when all of the following hold:

1. The source expectation has been manually rechecked.
2. The fixture builds or otherwise reaches the server's intended ready state.
3. The result repeats on the pinned release without runner errors.
4. The query maps to a documented operation rather than an inferred private
   capability.
5. Competing contract interpretations—imports, declaration grouping, aliases,
   constructors, hierarchy families, generated code—have been considered.
6. Preferably, a minimal pair or upstream acknowledgement confirms the defect.

Until then, report a benchmark disagreement and its evidence.

## Execution and workspace policy

Bifrost is evaluated as a fully static analyzer. It reads and indexes the
checked-in source, but it does not execute fixture code or invoke the fixture's
project build.

Language servers receive a different, deliberately favorable setup. Their
profiles may add minimal project files, configure toolchains, restore
dependencies, generate compilation metadata, accept build-import prompts, and
wait for a real project-loaded signal. A server may build or compile the fixture
when that is part of its supported semantic-workspace flow.

This asymmetry is intentional. Preventing a compiler-backed language server from
hydrating its normal workspace would turn missing results into a harness
configuration test. UsageBench compares each analyzer's returned locations
after its intended environment is ready, while preserving Bifrost's notable
property that its analysis does not depend on running or building the target
project. The current snapshot does not compare the time, resource, dependency,
or security costs of those execution models.

## Explaining a Bifrost advantage

When Bifrost returns an expected edge that an LSP omits, the page names the
specific implemented surface that supports the result: for example CommonJS
binding extraction, re-export canonicalization, language-specific receiver
filtering, or a usage graph that keeps declaration identities separate.

Architecture is not performance evidence. Bifrost's current design indexes
durable repository facts and computes deeper relationships on demand, while
several measured LSPs required a hydrated compiler or build workspace. That can
explain build independence and the shape of available facts, but UsageBench does
not yet publish comparable cold-start time, warm latency, or peak-memory data.
See Bifrost's own [capability boundary](https://brokkai.github.io/bifrost/capabilities/)
and [evaluation methodology](https://brokkai.github.io/bifrost/evaluation-evidence/);
those pages explicitly separate architecture intent and returned proven edges
from aggregate accuracy or performance evidence.

## Known fairness gaps

The corpus must grow toward capabilities that compiler-backed LSPs may handle
better, including macro expansion, generated declarations, synthetic members,
conditional compilation, external dependency symbols, and richer override or
effective-member rules. Until those cases exist, the comparison is evidence for
the current usage corpus—not a general analyzer ranking.
