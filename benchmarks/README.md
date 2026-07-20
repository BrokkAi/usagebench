# Benchmark Cases

Benchmark cases are authored as YAML and validated against
`schema/benchmark-case.schema.json`. The source-location model intentionally
mirrors the Language Server Protocol `Location` shape: each symbol location has
`location.uri` and `location.range`, where ranges contain zero-based `line` and
`character` positions and the end position is exclusive.

Every schema-v2 document declares its methodology state. The current checked-in
corpus is explicitly `development`, `analyzer_informed`, and
`legacy_unattributed`; it is useful for regression work, but is not a frozen,
independently reviewed evaluation set.

```yaml
schemaVersion: 2
corpus:
  partition: development
  selection: analyzer_informed
groundTruth:
  status: legacy_unattributed
  reviewers: []
referencePolicy: bindings_optional
```

Expected locations should still be verified by reading the checked-in fixture
source and recorded with `verification.method: manual_inspection`. Promotion to
`evaluation` additionally requires `selection: pre_registered`, a non-empty
`freezeId`, `groundTruth.status: independently_reviewed`, and at least two named
reviewers. Validation enforces those requirements.

## Source URIs

Use portable corpus URIs instead of checkout-specific file paths:

```yaml
location:
  uri: benchmark://source/src/main/java/example/Service.java
  range:
    start:
      line: 41
      character: 14
    end:
      line: 41
      character: 21
```

The URI path is relative to the pinned source root declared by the document's
`source` block. Public repositories must use `source.kind: git` with a pinned
commit. In-repository fixtures use `source.kind: fixture`.

For fixture-backed cases, `source.path` is resolved relative to the repository
working directory, and validation requires every referenced
`benchmark://source/...` file to exist under that fixture root.

## Positions

Positions are LSP-shaped and zero-based. `positionEncoding` defaults to
`utf-16`, matching LSP's default, and can be set to `utf-8` or `utf-32` when a
case corpus requires it.

Exact token ranges are required for an exact result. Fixture validation checks that each range is
within the referenced file's line bounds using UTF-16 character offsets.
Non-zero fixture ranges must select text equal to the location's `displayName`.
Keep fixtures ASCII unless a case is specifically intended to exercise encoding.

Line-only authoring is represented as a zero-width range on the intended line:

```yaml
range:
  start: { line: 12, character: 0 }
  end: { line: 12, character: 0 }
disambiguation: first_matching_symbol
```

The disambiguation rule means the runner may select the first symbol on that
line matching the location's `kind` and `displayName`. A result that supplies
only a path and line is `position_unverified`, never exact, even when that line
contains the expected token.

## Case Semantics

Each case supports both benchmark directions:

- `declaration` plus `expectedUsages` tests declaration-to-usage lookup.
- `expectedUnprovenUsages` lists required conservative candidates. Each may be
  returned as either proven or unproven, so increasing analyzer confidence does
  not break the case.
- `usageLookups` tests usage-to-declaration lookup.
  Each lookup has an `operation`: `declaration`, `definition`, or the temporary
  development-only `profile_default`. Evaluation cases must choose explicitly;
  declaration lookups never fall back to definition, or vice versa.
  A reviewed negative lookup may set `expectNoMovement: true` and repeat the
  usage location as `expectedDeclaration`; no result and an exact self-target
  pass, while navigation to any other token fails.
- `allowedExtraUsages` documents acceptable analyzer-specific broader matches.
- `allowedUnprovenUsages` documents optional conservative candidates that are
  acceptable only while they remain unproven.
- Proven locations outside `expectedUsages`, `expectedUnprovenUsages`, and
  `allowedExtraUsages`, and unproven locations outside
  `expectedUnprovenUsages` and `allowedUnprovenUsages`, fail the case.
- `referencePolicy` defines the document-wide reference surface:
  `external_usages` excludes binding-only imports/re-exports;
  `bindings_optional` accepts their presence or absence; and
  `bindings_required` requires authored binding locations in `expectedUsages`.
  The current development corpus uses `bindings_optional`.
- Under `bindings_optional`, classified import bindings, re-export bindings,
  and export metadata are recorded in `actual` and `extraUsages` but do not
  prevent an exact pass. They are never silently discarded. Unclassified extras
  still fail. Under the other policies, unauthored bindings fail like any other
  unexpected location.
- Any other extra—including declarations, definitions, same-name symbols, and
  implementation-family expansion—remains unexpected and fails the case until
  its cause is investigated and the corpus or policy is deliberately changed.
- Runtime export expressions that read a local value are usages. For example,
  the `Client` on the right-hand side of `module.exports = { Client }` or
  `exports.Client = Client` belongs in `expectedUsages`.
- `expectedFailure.reason` keeps a known analyzer gap in the baseline while
  still running the case and reporting it as improved if it unexpectedly starts
  passing.
- `notPlanned.reason` keeps runtime-dynamic or generated-code expectations in
  the corpus and runs them without including them in the planned-case total.
- `unsupported.reason` documents out-of-boundary cases and reports them as
  unsupported by default; `--include-unsupported` opts into running them.
- `verification` records how the expected locations were confirmed.

The current issue #8 corpus stops at validated source-location cases. Bifrost
execution, result normalization, and scoring are separate milestones.
