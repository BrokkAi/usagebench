# Per-case blinded agent-panel methodology v3

This is the retained methodology for heterogeneous agent-assisted ground-truth
review after the six-case v1 pilot showed complete agreement on required
locations but avoidable variance in exhaustive negatives and arbitrary
definition-target selection.

An earlier v2 prompt draft was rejected because it did not explicitly exclude
the selected declaration token from the usage set. V3 makes that boundary
normative; evidence produced under the draft does not qualify.

Each case is reviewed in a fresh model session. The reviewer receives exactly:

- the versioned prompt and response schema;
- one case manifest containing the declaration and reference policy; and
- a complete pinned source snapshot mounted as `source/`.

The packet excludes authored expectations, prior-result strata, analyzer
identity and output, other reviewers' responses, prior adjudication, and git
history. Packet and source digests are retained with the raw response.

Use at least two providers. Publication requires one accountable human to
review every case and explicitly adjudicate any disagreement, non-high
confidence, abstention, replacement proposal, or required-contract ambiguity.

Primary automatic consensus requires exact agreement on:

- decision and declaration;
- the complete set of `required` locations;
- the deterministic definition usage;
- high confidence; and
- no required-contract ambiguity.

Optional bindings and reviewed exclusions are preserved but are not part of
primary consensus. They are intentionally advisory because source-equivalent
reviewers can investigate different negative candidates without disagreeing
about the semantic contract.

The supplied declaration location is query metadata, not a usage, and must not
appear among required locations.

The human adjudicator may retain an existing authored definition target only
after confirming that it is in the agreed required set. Corpus corrections
discovered by both blinded reviewers are applied before promotion.
