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
- `usageLookups` tests usage-to-declaration lookup.
- `allowedExtraUsages` documents acceptable analyzer-specific broader matches.
- Actual usage locations outside `expectedUsages` and `allowedExtraUsages` are
  unexpected false positives and fail the case.
- Import or re-export binding sites are not true-positive usages. Do not include
  them in `expectedUsages`, `usageLookups`, or `allowedExtraUsages`; analyzers
  that report them should surface those locations as unexpected extras.
- `expectedFailure.reason` keeps a known analyzer gap in the baseline while
  still running the case and failing if the case unexpectedly starts passing.
- `notPlanned.reason` keeps runtime-dynamic or generated-code expectations in
  the corpus and runs them without including them in the planned-case total.
- `unsupported.reason` documents out-of-boundary cases and reports them as
  unsupported by default; `--include-unsupported` opts into running them.
- `verification` records how the expected locations were confirmed.

The current issue #8 corpus stops at validated source-location cases. Bifrost
execution, result normalization, and scoring are separate milestones.
