# Legacy promotion v1 source-only selection policy

This policy freezes the pre-review selection for the 158-case analyzer-informed
development corpus. It is an input to, not a substitute for, the independently
reviewed and human-adjudicated promotion manifest.

Selection must not inspect historical analyzer or language-server outputs,
scores, expected-failure markers, outcome-derived prose, regressions, or tool
disagreements. `expectedFailure` is deliberately ignored. Existing
`unsupported` and `notPlanned` source-contract states place cases in a separate
control set; they are independently checked during review and never enter a
correctness denominator.

The population is the 158 fixture-backed cases that predate the two published
semantic-pack navigation cases. Every source document is SHA-256 bound. Normal
benchmark validation establishes that its checked-in fixture exists and that
non-zero source ranges select their declared tokens. Project-file presence is
inventory metadata only: no compilation or project-load execution is claimed.

Semantic families are derived deterministically from case IDs using the ordered
rules implemented in `src/promotion_cohort.rs`. Duplication groups hash language,
symbol kind, semantic family, source complexity, operation/status set, and
location-count shape. These groups identify structurally near-duplicate cases;
they do not use analyzer behavior.

For each language, eligible cases are greedily ordered to maximize previously
unseen operation/status pairs, semantic families, symbol kinds, and duplication
groups, in that order. Ties use document path and case ID. The balanced core is
the first `N = min(10, lowest eligible language count)` cases. The remaining
eligible order is simultaneously frozen as overflow and deterministic
replacement order. Controls follow eligible cases in stable document/case-ID
order and have their own denominator.

Once review begins, this artifact is immutable. A rejection may consume only
the next frozen overflow entry for that language and cannot change N. Any other
selection or denominator change requires a new versioned cohort that
content-addresses and supersedes this one. Agentic re-review can strengthen the
named source contracts, but cannot make their original selection preregistered
or support language-wide, ecosystem-wide, or analyzer-superiority claims.
