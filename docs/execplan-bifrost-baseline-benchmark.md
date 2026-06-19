# ExecPlan: Bifrost-backed Usage Benchmark

## Goal

Turn `usagebench` from a set of analyzer-specific dataset generators into a
small, reviewed benchmark corpus for usage resolution. The benchmark should use
`bifrost` as the primary analyzer under test, retain language diversity, and
avoid baking Brokk/Bifrost-specific fully-qualified-name conventions into the
input format.

The benchmark must evaluate both lookup directions:

- declaration to usages: given a symbol declaration, find the known usage sites.
- usage to declaration: given a usage site, find the expected declaration.

## Current State

The repository now uses a curated, analyzer-neutral fixture corpus as the
primary benchmark input. Baseline cases live under `benchmarks/cases`, fixture
sources live under `fixtures`, and the Rust CLI validates the case schema plus
fixture-backed source locations.

The old broad Java, Go, and Python generator stack has been removed from the
active benchmark path. Future corpus growth should add reviewed cases rather
than analyzer-generated expected results.

## Proposed Direction

Use curated benchmark cases instead of whole-repository extraction.

Each case should identify a symbol by source location rather than by analyzer
FQN. The benchmark input should be close to:

```yaml
repo: https://github.com/example/project.git
commit: abc123
language: java
cases:
  - id: java-overridden-method
    declaration:
      file: src/main/java/example/Service.java
      line: 42
      column: 15
      kind: method
      name: execute
    expectedUsages:
      - file: src/test/java/example/ServiceTest.java
        line: 88
        column: 21
    usageLookups:
      - usage:
          file: src/test/java/example/ServiceTest.java
          line: 88
          column: 21
        expectedDeclaration:
          file: src/main/java/example/Service.java
          line: 42
          column: 15
```

Columns should be included when practical, but line-only cases should remain
valid with an explicit disambiguation rule. That keeps authoring easy for manual
inspection and still lets runners narrow to the intended token when needed.

## Benchmark Semantics

The benchmark should distinguish these concepts:

- `declaration`: the source location of the symbol being tested.
- `expectedUsages`: all usage sites that must be found for the declaration.
- `allowedExtraUsages`: optional sites that are acceptable when analyzers choose
  a broader semantic interpretation.
- `usageLookups`: usage-site probes whose result should resolve to a known
  declaration.
- `unsupported`: cases retained in the corpus but temporarily excluded from
  scoring, with a reason.

The first scoring pass should be strict about missing expected sites and lenient
about documented extras. Later work can add precision scoring once Bifrost's
result shape and symbol identity behavior are stable enough.

## Language Coverage

Build a small set of representative cases per language instead of a large CSV of
repositories. The initial matrix should cover:

- Java: classes, constructors, methods, fields, inheritance/overrides, imports,
  nested classes, test-to-production usages.
- Go: package functions, methods with pointer/value receivers, struct fields,
  interface methods, package constants/vars, cross-package references.
- Python: modules, functions, classes, methods, attributes, imports/re-exports,
  dynamic cases that are deliberately marked unsupported or partial.
- TypeScript/JavaScript: exported functions/classes, methods, object properties,
  default/named imports, JSX/TSX component references where supported by
  Bifrost.
- Rust or C# as a follow-up only if Bifrost support is ready enough to avoid
  spending the first milestone on harness plumbing.

Each language should start with a handful of cases from stable repositories or
small in-repo fixtures. Public repository cases should pin commits.

## Implementation Milestones

1. Define the analyzer-neutral case schema.

   Add a documented YAML or JSON schema for source-indexed benchmark cases. The
   schema must support both declaration-to-usage and usage-to-declaration probes,
   with optional columns and per-case skip reasons.

2. Create the first curated baseline corpus.

   Select a small number of cases across Java, Go, Python, and TypeScript. For
   each case, verify expected results by LSP and/or manual source inspection.
   Keep notes about how each baseline was verified.

3. Build a Bifrost runner.

   Add a runner that checks out or reuses pinned repositories, invokes Bifrost,
   maps source-location inputs to Bifrost queries, and normalizes results back to
   file/line/column locations for scoring.

4. Score declaration-to-usage lookups.

   Compare Bifrost usage results against `expectedUsages`, report missing and
   extra sites, and produce stable machine-readable output plus a human-readable
   summary.

5. Score usage-to-declaration lookups.

   Add the reverse-direction runner and scorer. The result should answer whether
   a usage site resolves to the expected declaration location.

6. Remove existing generators from the primary benchmark path.

   Done for the issue #8 baseline: the legacy Java, Go, and Python generators
   are no longer part of the active repo surface or CI. The source of truth is
   the manually reviewed fixture corpus.

## Validation Strategy

- Run benchmark schema validation in CI.
- Run unit tests for result normalization and scoring without requiring Bifrost.
- Add an integration job only when Bifrost can be installed or checked out
  reliably in CI.
- Keep each curated case small enough that failures can be reviewed manually.

## Open Questions

- Should curated cases live entirely in this repository as fixtures, or should
  they reference pinned external repositories by commit?
- Should the runner shell out to the `bifrost` CLI/MCP server, call a library
  API, or use an ACP-facing integration?
- What is the canonical Bifrost query surface for declaration-to-usage and
  usage-to-declaration lookups?
- How should columns be represented when manual inspection only gives a line
  number?
- Should precision/extra-result scoring be included in the first implementation
  or deferred until recall is reliable?
