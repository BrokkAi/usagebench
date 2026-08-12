# Retrospective legacy-promotion manifest

`schema/legacy-promotion.schema.json` defines a publication boundary for
strengthening selected cases from the 158-case analyzer-informed development
corpus without rewriting that history.

The v1 manifest freezes, before agent review, the source-only eligibility
policy; the eligible count for each of the 11 languages; all six balance
strata; and `N = min(10, lowest eligible count)`. Every balanced-core language
must contain exactly N cases. `overflow` and `control` membership are distinct;
controls must state `unsupported` or `not_planned` and never enter the core
denominator.

The pre-review population, inventory, membership, and replacement ordering live
in the separately validated `legacy-promotion-cohort` artifact. It contains no
review or analyzer-run evidence. After review and human adjudication, its frozen
membership feeds this publication manifest, whose per-case evidence links are
then mandatory. This separation prevents placeholder or fabricated review
records while keeping the selection fixed before reviewers begin.

Each entry binds the historical YAML hash, exact case ID, raw independent
review records, human adjudication, and balance strata. The validator rejects
prospective provenance, analyzer-outcome use, changed source or evidence
hashes, duplicate cases, denominator drift, missing strata, and mixing of
controls with core/overflow.

Corrections are append-only: publish a new promotion ID with a content-addressed
`supersedes` link. Do not overwrite raw review evidence, adjudication, the
historical YAML, or previously generated result pages. A replacement is a new
reviewed entry in a later manifest and cannot change an already frozen N.

Allowed wording is “reviewed conformance on the named promoted legacy corpus.”
The tier does not support claims of preregistration, general accuracy,
language-wide accuracy, ecosystem coverage, or analyzer superiority.
