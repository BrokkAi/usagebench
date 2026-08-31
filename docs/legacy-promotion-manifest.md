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

## Retiring a stale expected failure

`expectedFailure` in a historical case document records an analyzer outcome,
not reviewed ground truth, so it can outlive the defect it describes. The
runner already surfaces that: a case that passes while annotated is reported
`improved` rather than counted as a pass.

The annotation cannot be edited away. The document is content-addressed by
every manifest that binds it, and history is not rewritten. A superseding
manifest retires it instead, through `retiredExpectedFailure` on the case
entry:

- `supersededReason` repeats the annotation verbatim, so the retirement is a
  record of what was withdrawn rather than a deletion, and cannot drift from
  the frozen text.
- `evidence` binds the artifact showing the annotated navigation now succeeds.
- The manifest must carry `supersedes`. A retirement is a correction, and
  corrections are append-only.

Execution staging drops the annotation from its filtered, execution-only copy
of the corpus, so the case is scored as an ordinary pass. The historical YAML
stays byte-identical and every earlier manifest keeps validating against it.

Retirement is one-way and only ever tightens the corpus: it removes an excuse
and can never add one. A superseding manifest may retire more expectations, but
may not restore one its predecessor retired — the validator rejects that,
so a later manifest cannot quietly reinstate an excuse for a case already held
to an ordinary pass.

`legacy-promotion-v2-balanced-core` supersedes
`legacy-promotion-v1-balanced-core` on exactly these terms. It carries the same
110 reviewed balanced-core cases and retires one annotation, on
`cpp-parity-function-like-macro-expanded-call`.
