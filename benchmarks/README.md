# Benchmark Cases

Benchmark cases are authored as YAML and validated against
`schema/benchmark-case.schema.json`. The source-location model intentionally
mirrors the Language Server Protocol `Location` shape: each symbol location has
`location.uri` and `location.range`, where ranges contain zero-based `line` and
`character` positions and the end position is exclusive.

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

## Positions

Positions are LSP-shaped and zero-based. `positionEncoding` defaults to
`utf-16`, matching LSP's default, and can be set to `utf-8` or `utf-32` when a
case corpus requires it.

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
- `unsupported.reason` keeps useful future cases in the corpus without scoring
  them yet.
- `verification` records how the expected locations were confirmed.
