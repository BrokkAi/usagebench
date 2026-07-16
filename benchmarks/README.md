# Benchmark Cases

Benchmark cases are authored as YAML and validated against
`schema/benchmark-case.schema.json`. The source-location model intentionally
mirrors the Language Server Protocol `Location` shape: each symbol location has
`location.uri` and `location.range`, where ranges contain zero-based `line` and
`character` positions and the end position is exclusive.

The baseline corpus is manually curated. Expected locations should be verified
by reading the checked-in fixture source and recorded with
`verification.method: manual_inspection`.

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

Exact token ranges are preferred. Fixture validation checks that each range is
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

The disambiguation rule means the runner should select the first symbol on that
line matching the location's `kind` and `displayName`.

## Case Semantics

Each case supports both benchmark directions:

- `declaration` plus `expectedUsages` tests declaration-to-usage lookup.
- `expectedUnprovenUsages` lists required conservative candidates. Each may be
  returned as either proven or unproven, so increasing analyzer confidence does
  not break the case.
- `usageLookups` tests usage-to-declaration lookup.
- `allowedExtraUsages` documents acceptable analyzer-specific broader matches.
- `allowedUnprovenUsages` documents optional conservative candidates that are
  acceptable only while they remain unproven.
- Proven locations outside `expectedUsages`, `expectedUnprovenUsages`, and
  `allowedExtraUsages`, and unproven locations outside
  `expectedUnprovenUsages` and `allowedUnprovenUsages`, fail the case.
- Import or re-export binding sites are not authored as true-positive usages.
  Do not put them in case-level expectations or allowances merely to match an
  LSP's “find references” policy.
- Exact scoring continues to treat binding sites as a precision difference for
  the Bifrost product contract. The LSP runner classifies observed import
  bindings, re-export bindings, and export metadata as `allowed_policy_extra`
  and may report `near_miss` when they are the only difference and every
  required reference and reverse lookup succeeds. The locations remain in
  `actual` and `extraUsages`; they are never silently discarded.
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
