---
title: Comparison methodology
description: Classify observed differences without overclaiming analyzer defects or approximation mechanisms.
---

UsageBench reports required-destination recall first, strict contract agreement
second, and causal interpretation third. The fixture-backed development corpus
has completed one human source review. The separate 36-case `real-project-v1`
slice is preregistered, source-locked, agent-reviewed, and adjudicated source
evidence; it is not publication-eligible until a cross-provider panel and
accountable human adjudication are recorded. Analyzer results for it have not
yet been published.
An analyzer may also expose a different public grouping policy without
containing an implementation bug.

## Headline metric: required destinations found

The user-facing metric counts a case when:

1. every required reference or usage is present, including reviewed
   conservative candidates;
2. every navigation lookup includes its expected destination; and
3. every type lookup includes its expected type destination.

A line-only location, a broader containing range, and additional returned
results are tolerated. This models the basic editor question—“did the operation
take me to, or list, the code I needed?”—without pretending that range
precision and result clutter are invisible to users. A case remains in this
shared denominator when both analyzers can execute every required lookup
through either the canonical operation or an explicitly reviewed compatible
operation. Operations unavailable through both paths remain outside it; the
strict metric continues to use only the authored canonical operation.

This is best read as **required-destination recall**, not an all-purpose quality
score. A response containing the expected target among many unrelated targets
can pass this headline metric while still being frustrating. The strict metric
and case pages preserve that precision evidence.

## Location-level recall, precision, and range quality

Reports produced by UsageBench 0.2.0 and newer retain raw location-level
counts for reference and navigation queries. A required location is a true
positive when the response returns its exact token, a containing range, or a
line-only location. An omitted required location is a false negative. A
returned location is a false positive only when it is neither required nor
allowed by the authored or reference-binding policy. True negatives are not
defined: there is no meaningful finite universe of source tokens that an
analyzer could have returned but did not.

Returned locations remain separated into four machine-readable categories:

| Category | Meaning |
|---|---|
| Required | Matches an authored required location. |
| Policy allowed | Matches `allowedExtraUsages`, `allowedExtraTargets`, `allowedUnprovenUsages`, or a classified optional binding/export location. |
| Related, unallowed | Is recognized as a binding, re-export, export-metadata, declaration, or definition location, but the case policy does not allow it. |
| Unrelated | Does not match an authored location and has no recognized related-location classification. |

An unauthored navigation response is related-unallowed because the analyzer
presented it as a declaration or definition candidate. Reference responses can
also be unrelated when source classification finds no recognized relationship.

Destination recall is `TP / (TP + FN)`. Exact-token recall uses the same
denominator but counts only exact token matches. Strict precision is
`TP / (TP + all extras)`, so even policy-allowed clutter remains visible;
policy-adjusted precision is `TP / (TP + related-unallowed + unrelated)`.
The exact-set case rate requires every scored query in a case to return every
required location at its exact token range and no extra locations. Extra-result
burden is the number of all extras per query that found every required
location.

Range quality is independent of destination correctness. Required locations
are counted as exact token, containing range, line-only, or missing. Returned
related-unallowed and unrelated locations are additionally counted as wrong
locations. A policy-allowed extra is recorded as clutter but not as a wrong
location. Proven and unproven result channels contribute the same location
evidence; proof degradation remains visible through the unchanged strict case
status instead of becoming an unrelated location.

For reviewed compatible navigation operations, the recall-forward metric uses
the scoreable response with the best range match, preferring the canonical
operation on a tie. The strict case status remains the canonical operation's
result. No-movement lookups are excluded from location precision because an
empty response can satisfy their contract without returning a destination.

The report stores integer evidence rather than rounded rates. Public result
pages derive pooled micro rates, per-case macro means, and equal-profile means
from the same counts. Reports from UsageBench 0.1.0 remain readable for their
existing status fields, but location tables reject them explicitly rather than
interpreting absent metrics as zero.

## Secondary metric: strict UsageBench contract

### Result categories

| Category | Meaning |
|---|---|
| Exact | Required complete token ranges and navigation targets match, with no unallowed extras. Under `bindings_optional`, classified binding/export extras may be present and remain recorded. |
| Position unverified | Path and line agree, but the analyzer returned either a line-only location or a broader range containing the expected token rather than the exact token range. |
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

Definition is the default authoring choice for ordinary source navigation.
Declaration is selected only when the distinction is material to the reviewed
case—for example, navigation from an implementation to a separate interface,
prototype, or forward declaration. The optional
[`textDocument/declaration`](https://microsoft.github.io/language-server-protocol/specifications/specification-3-14/#goto-declaration-request-leftwards_arrow_with_hook)
request was added in LSP 3.14, while Definition was already the conventional
navigation request. The protocol defines their result shapes but leaves the
language-specific meaning of “declaration” and “definition” to servers.
UsageBench therefore does not treat the newer endpoint as a generally stricter
or more canonical form of Definition.

This is intentional lenience at the case-authoring boundary, justified by the
protocol's history and capability model. A server that does not advertise
Declaration is classified as unsupported and excluded from that shared
denominator. Penalizing another server for advertising Declaration with a
narrower, language-specific meaning would reward non-advertisement. Choosing
Definition for an ordinary, undifferentiated source target avoids that
capability-advertisement bias. It does **not** make the scorer lenient: once a
case selects Declaration or Definition, that exact request must satisfy the
strict singleton target contract, with no fallback to the other endpoint.

Cases may additionally declare reviewed `compatibleOperations` when another
endpoint reaches the exact same authored target. The runner records these as
separate alternate results: `status` remains the canonical protocol-specific
score, while `requiredDestinationStatus` is computed from the raw canonical
and reviewed-compatible locations. It records `found` when all expected
destinations are present even if ranges contain the expected token or the
response includes extras. An alternate is never queried silently, never merged
into the canonical response, and cannot improve the strict endpoint result.
Per-report `totals.requiredDestinations` makes this metric machine-readable
without changing the canonical counters.

The strict result is not labeled generic LSP compliance. The current
[`Location`](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.18/specification/#location)
shape requires a URI and range, and the protocol defines how ranges are
encoded, but it does not require a references or navigation result to select
exactly one terminal identifier. Identifier-tight ranges, singleton navigation,
and UsageBench's identity-family exclusions are additional benchmark
requirements for machine consumers.

## Aggregation and sensitivity

UsageBench keeps raw counts and denominators visible. Location metrics report
three complementary summaries:

- **Pooled micro rate** sums location outcomes before dividing. Cases with more
  required or returned locations have more influence.
- **Per-case macro mean** computes each case's rate first and weights scoreable
  cases equally.
- **Equal-profile mean** computes a rate for each reference profile and gives
  each profile equal weight, regardless of its current case count.

Required-destination and strict case comparisons continue to publish their
case-weighted totals, profile rates, and sensitivity views separately.

The profile table remains primary evidence because the products, case counts,
and language semantics are heterogeneous. A leave-one-profile-out sensitivity
check shows whether the pooled direction depends on any single profile; it is
not an invitation to remove an inconvenient language.

The development corpus is not sampled from a defined population of repositories
or developers. UsageBench therefore does not apply language-popularity weights
or attach sampling confidence intervals to the development result. The
`real-project-v1` evaluation slice uses a preregistered, source-only repository
draw, but supports only the bounded descriptive per-profile claim in its
protocol; it is not a sample of all repositories or developers.

## Corpus partitions and ground truth

Development cases may be analyzer-informed and may retain legacy review notes.
They are appropriate for regression work and diagnosis, but their aggregate is
not an evaluation claim. Every document declares its partition explicitly, and
public generation rejects a snapshot that mixes development and evaluation
claims.

The first human audit is complete for all 158 current cases in 35 documents.
That review corrected and explained individual source contracts, but it does
not change their `development`, `analyzer_informed`, or
`legacy_unattributed` metadata. See the
[human ground-truth audit](../ground-truth-review/) for coverage, procedure, and
the precise trust boundary.

An evaluation document may be authored with agent-only evidence, but a public
evaluation freeze is accepted only when all of these are true:

1. case selection was pre-registered before running the compared analyzers;
2. the document has an immutable `freezeId`;
3. either at least two humans independently checked source locations, or one
   accountable human adjudicated blinded agents from at least two providers;
4. agent reviewers disclose provider, model, prompt/schema hashes, and raw
   responses and are never represented as human reviewers;
5. disagreements, abstentions, ambiguity, replacements, and non-high confidence
   were adjudicated before the freeze; and
6. the selection, reviews, and exact public Git source are bound by hashed
   manifests, including a source-lock entry for the offline materialization.

The protocol comes first: it records the target language/profile strata,
population snapshot, eligibility and exclusion rules, sampling/replacement
procedure, and the limited claim scope before repositories or declarations are
selected. A selection manifest then commits to the actual repository commits
and case IDs; the reviewers and source lock hash that manifest. This makes a
later analyzer run an outcome of the frozen slice, not an input to its choice.

Changing a frozen assertion creates a new freeze and preserves the old report.
Reports include partition, selection, review status, and reference policy, and
their totals separate development from evaluation cases.

The initial `real-project-v1` evaluation partition contains four repositories
and three declarations for each of the gopls, Pyright, and TypeScript LS
profiles: 12 repositories and 36 cases in total. Published evaluation pages
must show those per-profile denominators, the recorded population exclusions
and source-review replacements, the freeze ID and claim scope, and the hashed
protocol, selection, review, and source-lock provenance. Its historical review
used two same-provider Codex agents and is classified as `agent_reviewed`; it
cannot be publicly frozen until a qualifying cross-provider panel and
accountable human adjudication are recorded. Evaluation tables are descriptive
only: they exclude language-wide or ecosystem-wide estimates,
cross-language ranking, causal defect attribution, and latency, memory, or
cold-start claims.

The preregistered `real-project-v2` slice is independent of v1 and targets
Java/Eclipse JDT LS, Rust/rust-analyzer, and C++/Apple clangd. Its protocol
hash-links the v1 selection so prior repositories cannot be reused, requires
four selected and at least four reserved eligible repositories per profile,
and preserves the same source-only, analyzer-blind ordering. V2 must be
reported with its own per-profile denominators. A future combined view may
aggregate only within documented language/profile strata while retaining the
v1 and v2 denominators; it may not pool unlike profiles into an ecosystem-wide
or cross-language claim.

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

The products also have different primary consumers. An LSP normally supports a
developer's live editor session, where low-latency cursor operations sit beside
completion, diagnostics, and refactoring. Bifrost exposes repository analysis
and navigation to machine consumers, including coding agents and static-analysis
tools. UsageBench compares only their overlapping source-usage and navigation
contract; it is not a benchmark of the LSP's full editor experience or
Bifrost's broader analysis surface.

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
project. The current corrected result does not compare the time, resource,
dependency, or security costs of those execution models.

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
