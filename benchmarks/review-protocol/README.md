# Agent-assisted ground-truth review

UsageBench permits a publication-eligible `human_adjudicated_agent_panel`
tier when one accountable human adjudicator signs off on independently blinded
derivations from at least two agents spanning at least two providers. Agent
judgments are never described as independent human reviews.

`blinded-agent-review-v1.json` is the canonical, content-addressed protocol
manifest. Every qualifying agent review must link that manifest and its exact
prompt and response-schema digests. Each participant records the provider,
exact model, provider-native execution ID, execution timestamp, and a
hash-bound raw response. The validator requires the raw response to cover every
selected case exactly once and to match the normalized reviewer evidence.

Exact high-confidence agreement is advisory consensus. Any disagreement,
abstention, ambiguity, replacement, missing location, or medium/low confidence
requires case-level human adjudication. Exact consensus still requires an
accountable human batch sign-off with a recorded identity and attestation.
Analyzer outputs may be revealed only after
the ground-truth review and adjudication have been frozen; subsequent
anonymized comparisons cannot change ground truth without a new freeze.
