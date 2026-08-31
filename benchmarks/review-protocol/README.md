# Agent-assisted ground-truth review

UsageBench permits a publication-eligible `human_adjudicated_agent_panel`
tier when one accountable human adjudicator signs off on independently blinded
derivations from at least two agents spanning at least two providers. Agent
judgments are never described as independent human reviews.

`blinded-agent-review-v3.json` is the canonical, content-addressed protocol
manifest. Every qualifying agent review must link that manifest and its exact
methodology, prompt, and response-schema digests. Each participant records the
provider, exact model, provider-native execution ID, execution timestamp, and
a hash-bound raw response. V3 publication records those values in one per-case
session, together with the exact case-packet digest. The packet embeds the
pinned source-archive digest. For each logical reviewer, the validator requires
one provider/model cohort across all sessions, exact coverage of every selected
case, and agreement with the normalized reviewer evidence. It also requires at
least two providers for every case and rejects reused provider/execution-ID
pairs. Legacy schema-v2 singleton evidence remains readable, but a panel cannot
mix singleton and per-case representations.

V3 uses one fresh session and one complete pinned project snapshot per case.
Primary consensus compares required semantic locations and the deterministic
definition target; optional bindings and reviewed exclusions remain auditable
but do not manufacture disagreement. The rejected v2 draft and its incomplete
dry-run artifacts are retained because they exposed an ambiguous declaration
boundary before publication.

Exact high-confidence agreement is advisory consensus. Any disagreement,
abstention, ambiguity, replacement, missing location, or medium/low confidence
requires case-level human adjudication. Exact consensus still requires an
accountable human batch sign-off with a recorded identity and attestation.
Analyzer outputs may be revealed only after
the ground-truth review and adjudication have been frozen; subsequent
anonymized comparisons cannot change ground truth without a new freeze.

The publication review manifest and its two normalized reviewer artifacts are
derived deterministically from the retained v3 runs. Verify that checked-in
provenance has not drifted with:

```bash
python3 scripts/build-real-project-v1-publication-review.py --check
```

## Retrospective legacy-promotion milestones

Legacy-promotion review proceeds one frozen case per language at a time. Each
milestone retains the 11 blinded packets, 22 provider-native raw responses,
provider execution metadata, mechanical comparison, and accountable human
adjudication before the next selection order begins. It remains
`retrospectively_selected`; review does not convert the analyzer-informed
legacy selection into preregistered evidence.

Cases with a declaration-centered References contract continue to use the
retained v3 usage-review profile. Navigation-only cases use the separate
`blinded-navigation-review-v1.json` profile. Its packet records one or more
Declaration, Definition, or Type Definition queries without revealing authored
targets. In particular, Type Definition reviewers derive the expression's type
and its declaration target; they do not reinterpret the expression as a textual
usage of the type name. Unsupported candidate capability remains an execution
status and never changes this source-derived contract.

The completed review contains milestones 1 through 10: 110 balanced-core
cases, 220 fresh provider-native sessions, mechanical comparison, and accountable
human adjudication. The hash-bound
`benchmarks/promotion/legacy-v1/manifest.json` publishes that complete cohort as
the corpus-bounded `legacy_promoted` tier without changing the immutable
`N = 10` denominator or its retrospective selection provenance.
`benchmarks/promotion/legacy-v2/manifest.json` supersedes it, carrying the same
reviewed membership and retiring one stale `expectedFailure` annotation.

Validate each adjudicated milestone with:

```bash
for run in benchmarks/review-protocol/runs/legacy-promotion-v1-milestone-*/run.json; do
  python3 scripts/validate-legacy-promotion-milestone.py "$run"
done
```

Validate the complete promotion manifest with:

```bash
cargo run -- validate-legacy-promotion benchmarks/promotion/legacy-v1/manifest.json
cargo run -- validate-legacy-promotion benchmarks/promotion/legacy-v2/manifest.json
```
