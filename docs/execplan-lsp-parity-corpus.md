# ExecPlan: LSP Parity Corpus

## Goal

Build a living benchmark plan for mining correctness and precision scenarios
from mature language servers, then converting those scenarios into reviewed
`usagebench` cases.

The corpus should answer a practical maturity question: given behavior that
rust-analyzer, Eclipse JDT LS, gopls, TypeScript Server, or another mature
language tool already treats as important, how close is Bifrost on the same
source-location contract?

The benchmark remains analyzer-neutral. Cases should use LSP-shaped source
locations, not upstream-internal symbol IDs or Bifrost fully-qualified names.

## Current State

`usagebench` already has the right benchmark shape for parity cases:

- source locations use `benchmark://source/...` URIs and LSP-style ranges;
- `usageLookups` covers usage-to-declaration behavior;
- `declaration` plus `expectedUsages` covers declaration-to-usage behavior;
- `typeLookups`, `expectedFailure`, `notPlanned`, and `unsupported` keep future
  maturity gaps visible without weakening the source-location contract.

The active corpus is the reviewed fixture corpus under `benchmarks/cases` and
`fixtures`. This plan should extend that path, not revive generator-heavy
corpora or analyzer-specific expected output.

## Guiding Rules

- Prefer small checked-in fixtures with provenance notes. Use pinned upstream
  git sources only when project shape is essential to the behavior under test.
- Mine upstream scenarios, not upstream harnesses. The adapted case should be
  readable and reviewable in `usagebench`.
- Keep import, re-export, generated-code, macro, and dynamic-language cases
  explicit. Use `expectedFailure` for planned analyzer gaps, `notPlanned` for
  runnable but intentionally unscored dynamic/generated behavior, and
  `unsupported` for cases that need runner or project-shape support before they
  should run by default.
- Treat false positives as first-class precision failures. A mature analyzer
  parity case should say what should not count as a usage when that distinction
  matters.
- Do not change the schema in the foundation milestone. Revisit only after the
  Rust and Java pilots prove that `verification.notes` cannot carry provenance
  cleanly.

## Scenario Taxonomy

Use this taxonomy when mining upstream tests and naming future cases.

| Family | Benchmark surface | Scenario examples |
| --- | --- | --- |
| Definition lookup | `usageLookups` | local variables, functions, constructors, fields, associated items, imports, aliases |
| References | `declaration` and `expectedUsages` | call sites, reads/writes, inheritance references, constructor uses, JSX or macro-expanded references |
| Type definition | `typeLookups` | expression type, return type, alias target, generic associated type, inferred interface/trait type |
| Implementation | `usageLookups` or future implementation-specific cases | trait/interface method implementation, superclass method override, abstract method implementation |
| Imports and re-exports | `usageLookups`, `expectedUsages`, `allowedExtraUsages` | module exports, package aliases, wildcard imports, static imports, re-exported functions/classes |
| Inheritance and overrides | `usageLookups`, `expectedUsages`, `typeLookups` | virtual dispatch, overridden methods, anonymous classes, embedded/anonymous fields |
| Generated or expanded code | `notPlanned` initially | Rust macros/proc macros, Java annotation processors, Lombok-style methods |
| Dynamic behavior | `notPlanned` initially | Python `__getattr__`, Ruby dynamic dispatch, JavaScript computed properties |
| Precision negatives | strict extras handling | import bindings that are not usages, unrelated same-name symbols, ambiguous wildcard imports |

## Provenance Contract

Every mined parity case should record enough provenance to be audited later.
Until a dedicated schema field exists, put this in `verification.notes`.

Use this format as the default note shape:

```yaml
verification:
  method: analyzer_comparison
  notes: >
    Parity source: rust-lang/rust-analyzer@<commit>.
    Upstream test: crates/ide/src/goto_definition.rs::<test_name>.
    Adapted fixture: fixtures/rust/lsp-parity/<case-id>/.
    Maturity: expected to pass; no macro expansion required.
```

For manually inspected fixture-only cases, use `manual_inspection` and keep the
same provenance fields when the scenario was inspired by an upstream test. For
cases verified directly against an upstream language server, use `lsp` and name
the exact server, version or commit, request, source file, and cursor position.

## Milestones

### 1. Plan and Taxonomy Foundation

Status: complete.

Deliverables:

- Add and maintain this living plan.
- Keep the taxonomy above aligned with the case names that land in the corpus.
- Add no schema changes unless the Rust and Java pilots show a real need for
  structured provenance fields.

Exit criteria:

- Future agents can identify where to mine scenarios, how to adapt them, how to
  record provenance, and how to verify the result.

### 2. Rust Parity Pilot

Status: first parity fixture landed.

Primary upstream sources:

- `rust-lang/rust-analyzer/crates/ide/src/goto_definition.rs`
- `rust-lang/rust-analyzer/crates/ide/src/references.rs`
- `rust-lang/rust-analyzer/crates/ide/src/goto_type_definition.rs`
- `rust-lang/rust-analyzer/crates/ide/src/goto_implementation.rs`

Initial scenario set:

- module declarations and sibling modules;
- re-exported free functions and types;
- associated items, associated types, and trait item definitions;
- trait/impl method lookup, including UFCS-style calls;
- type aliases and generic type parameters;
- operator and indexing calls where a trait implementation is the meaningful
  target;
- declarative macro-generated references, marked `notPlanned` when expansion
  support is outside the planned scoring surface;
- proc-macro cases marked `notPlanned` until macro expansion parity becomes an
  intentional target.

Implementation notes:

- Prefer `fixtures/rust/lsp-parity/<case-id>/` for adapted fixtures.
- Keep macro and trait cases small enough that expected ranges can be manually
  reviewed.
- Mark known Bifrost gaps with `expectedFailure` rather than weakening expected
  locations.

Current progress:

- Added `benchmarks/cases/rust-lsp-parity.yaml` with module path, trait method,
  associated type, and macro-generated function scenarios.
- Added `fixtures/rust/lsp-parity/` as the first minimal Rust parity fixture.
- Verified the first Bifrost run against
  `origin/master` resolved to `6e0b54063ec3cf43c13cd489051cce35c34e22dc`:
  three cases are expected failures and the macro-generated function case is
  marked not planned.

Exit criteria:

- Add a reviewed Rust parity case file or extend the existing Rust case file
  with a clearly separated parity section.
- Run `cargo run -- validate benchmarks/cases`.
- Run Bifrost against the exact target checkout and inspect missing and extra
  locations before filing follow-up issues.

### 3. Java Parity Pilot

Status: first parity fixture landed.

Primary upstream sources:

- `eclipse-jdtls/eclipse.jdt.ls/org.eclipse.jdt.ls.tests/src/.../NavigateToDefinitionHandlerTest.java`
- `eclipse-jdtls/eclipse.jdt.ls/org.eclipse.jdt.ls.tests/src/.../ReferencesHandlerTest.java`
- `eclipse-jdtls/eclipse.jdt.ls/org.eclipse.jdt.ls.tests/src/.../ImplementationsHandlerTest.java`
- `eclipse-jdtls/eclipse.jdt.ls/org.eclipse.jdt.ls.tests/src/.../NavigateToTypeDefinitionHandlerTest.java`

Use `redhat-developer/vscode-java` only for client workflow inspiration. The
semantic oracle for Java parity is Eclipse JDT LS.

Initial scenario set:

- classes, constructors, methods, and fields;
- inheritance, overrides, interface implementations, and abstract methods;
- nested classes and anonymous classes;
- static imports and wildcard imports;
- same-name ambiguity cases that should return no misleading target;
- test-to-production references;
- JAR/classfile-adjacent source navigation, marked `unsupported` unless the
  case can be represented as a small fixture;
- annotation-processor or Lombok-like generated members, marked `notPlanned`
  until generated-member parity becomes an intentional target.

Implementation notes:

- Prefer `fixtures/java/lsp-parity/<case-id>/` for adapted fixtures.
- Keep project-setup-sensitive cases separate from simple source-only cases.
- Do not count import bindings as usages unless the case explicitly documents an
  analyzer-specific broader interpretation under `allowedExtraUsages`.

Current progress:

- Added `benchmarks/cases/java-lsp-parity.yaml` with static import method,
  interface receiver method, concrete implementation method, and local
  interface type scenarios.
- Added `fixtures/java/lsp-parity/` as the first minimal Java parity fixture.
- Verified the first Bifrost run against
  `origin/master` resolved to `6e0b54063ec3cf43c13cd489051cce35c34e22dc`:
  three cases pass and `java-parity-concrete-implementation-method-call` is an
  expected failure because Bifrost resolves the override call to the annotation
  line instead of the method declaration token.
- Filed the concrete Java override location miss as `bifrost#404`.

Exit criteria:

- Add a reviewed Java parity case file or extend the existing Java case file
  with a clearly separated parity section.
- Validate all benchmark cases.
- File standalone Bifrost issues for meaningful misses with the usagebench case
  path and one reproducible `run-bifrost` command.

### 4. Existing Language Expansion

Status: first-pass baseline-language parity fixtures landed.

Extend the same mining pattern to current `usagebench` languages after the Rust
and Java pilots prove the authoring workflow.

Use the current baselines as the starting map for each language. Each language
should get its own parity milestone with the same loop: read the baseline,
choose mature upstream scenarios, adapt small fixtures, validate, run Bifrost,
and file standalone product issues for meaningful misses.

| Language | Current baseline handles | Parity mining process |
| --- | --- | --- |
| JavaScript | CommonJS exported members, named export/import functions, class construction, `js-method-call` expected failure, `js-class-property-access` expected failure | Mine TypeScript Server JavaScript tests and VS Code JS language-feature behavior for CommonJS aliases, default/named imports, prototype/class methods, instance properties, object-literal members, and computed-property not-planned cases. Keep import bindings out of expected usages unless documented as `allowedExtraUsages`. |
| TypeScript | named exports, default class import/construction, class method calls, `ts-object-property-access` expected failure, TSX component references, Ky-style static factory calls | Mine TypeScript Server tests for TS/TSX references, JSX component definitions, type-only imports, static methods, object/type-literal properties, interface members, generic aliases, and default export wrappers. Separate syntax-only TSX fixtures from project-shape cases that need `tsconfig.json`. |
| Ruby | `require_relative`, nested constants, superclass references, include/prepend/extend lookup, class constants, bare method calls, singleton methods, `ruby-dynamic-public-send` not planned, script constants, instance/class/singleton fields | Mine Ruby LSP and Solargraph-style scenarios for constant lookup, mixins, singleton/class methods, `require`/`autoload`, field readers/writers, and receiver-aware dynamic dispatch. Mark runtime-dispatch cases such as `send`, `__send__`, and `public_send` as `notPlanned` unless they are deliberately promoted into the scored surface. |
| Scala | class construction, companion object calls, `scala-method-call` expected failure, `scala-field-and-val-access` expected failure, object val access | Mine Metals scenarios for companions, imports, inheritance, traits, extension methods, symbolic operators, infix/postfix calls, givens/using clauses, and generated-synthetic surfaces. Keep sbt/build-shape cases separate from single-fixture source cases. |
| PHP | class construction, direct method calls, `php-repository-method-call` expected failure, property access, class constants | Mine Intelephense or PHP language server scenarios for namespaces, imports, traits, static calls, instance properties, class constants, Composer autoload roots, fluent receiver chains, and magic method/property not-planned cases. Split Composer/autoload cases from simple source-only fixtures. |
| Python | module imports, function re-export calls, class instantiation, `python-method-call` expected failure, dynamic `getattr` not planned, `python-attribute-access` expected failure | Mine Pyright and pylsp scenarios for imports/re-exports, class methods, attributes, protocol-like methods, package `__init__` exports, decorators, and dynamic lookup. Keep `__getattr__`, monkey-patching, dynamic imports, and string-based `getattr` not planned unless the target is deliberately promoted into the scored surface. |
| Go | package functions, value receiver methods, pointer receiver methods, struct fields, package constants/vars | Mine gopls scenarios for pointer/value receiver equivalence, interfaces, embedding, promoted fields/methods, package aliases, cross-package references, constants/vars, and build-tag unsupported cases. Preserve package-level fixture shape with minimal `go.mod` where needed. |
| C# | class references, constructors, methods, repository methods, properties, constants | Mine Roslyn LSP or OmniSharp scenarios for properties, fields, constructors, partial classes, interfaces, inheritance, extension methods, namespace aliases, and generated/source-generator not-planned cases. Keep source-generator cases unscored until generated-source parity becomes an intentional target. |
| C++ | functions, class references, constructors, methods, `cpp-field-access` expected failure, constants | Mine clangd scenarios for declarations versus definitions, headers and implementations, overloads, namespaces, constructors, fields, constants, templates, using aliases, and include-boundary precision. Mark template-heavy or compile-command-sensitive cases conservatively until fixtures include enough project shape. |

Each language milestone should add a small reviewed scenario set before
broadening coverage.

Current JavaScript/TypeScript progress:

- Added `benchmarks/cases/javascript-lsp-parity.yaml` with CommonJS destructured
  function, object-literal method, and computed method-name scenarios.
- Added `fixtures/javascript/lsp-parity/` as the first minimal JavaScript
  parity fixture.
- Added `benchmarks/cases/typescript-lsp-parity.yaml` with default function
  import, static method, type-only interface type, and interface property
  scenarios.
- Added `fixtures/typescript/lsp-parity/` as the first minimal TypeScript
  parity fixture.
- Verified focused Bifrost runs against `origin/master` resolved to
  `6e0b54063ec3cf43c13cd489051cce35c34e22dc`.
- Filed the JS object-literal method miss as `bifrost#406`, the TS static method
  miss as `bifrost#407`, and the TS interface property miss as `bifrost#408`.

Current Ruby progress:

- Added `benchmarks/cases/ruby-lsp-parity.yaml` with autoload constant,
  attr-reader, singleton-class method, alias-method, and `module_function`
  scenarios.
- Added `fixtures/ruby/lsp-parity/` as the first minimal Ruby parity fixture.
- Tightened `benchmarks/cases/ruby-baseline.yaml` so declaration scans for
  `Billing::Auditable.audit` treat both `invoice.audit` and
  `invoice.public_send(:audit)` as true usages of the same method; the dynamic
  `public_send` usage-to-definition gap is now marked not planned.
- Verified focused Bifrost runs against `origin/master` resolved to
  `6e0b54063ec3cf43c13cd489051cce35c34e22dc`.
- Filed Ruby autoload/constant-path misses as `bifrost#409`, generated reader
  and alias-method receiver misses as `bifrost#410`, and singleton/module
  function misses as `bifrost#411`.

Current Python/PHP progress:

- Added `benchmarks/cases/python-lsp-parity.yaml` with re-exported class alias,
  classmethod, staticmethod, property getter, and not-planned `__getattr__`
  scenarios.
- Added `fixtures/python/lsp-parity/` as the first minimal Python parity
  fixture.
- Added `benchmarks/cases/php-lsp-parity.yaml` with namespace alias static
  method, trait method, interface method implementation, static property, and
  not-planned magic `__get` scenarios.
- Added `fixtures/php/lsp-parity/` as the first minimal PHP parity fixture.
- Verified focused Bifrost runs against `origin/master` resolved to
  `96f5f3a9b099cfe72e83994dbc99dcad3db6b516`.
- Filed Python re-exported class/static member misses as `bifrost#413` and
  Python property getter misses as `bifrost#414`.
- Filed PHP static member misses as `bifrost#415`, trait method misses as
  `bifrost#416`, and interface method implementation misses as `bifrost#417`.

Current Scala/Go progress:

- Added `benchmarks/cases/go-lsp-parity.yaml` with cross-package import alias,
  embedded promoted method, embedded promoted field, and unsupported build-tag
  scenarios.
- Added `fixtures/go/lsp-parity/` as the first minimal Go parity fixture.
- Added `benchmarks/cases/scala-lsp-parity.yaml` with trait method
  implementation, renamed import, extension method, local method call, and
  not-planned generated/synthetic scenarios.
- Added `fixtures/scala/lsp-parity/` as the first minimal Scala parity fixture.
- Verified focused Bifrost runs against `origin/master` resolved to
  `96f5f3a9b099cfe72e83994dbc99dcad3db6b516`.
- Filed Go embedded promoted member misses as `bifrost#418`.
- Filed Scala trait method misses as `bifrost#419`, renamed import misses as
  `bifrost#420`, and extension method misses as `bifrost#421`.

Current C#/C++ progress:

- Added `benchmarks/cases/csharp-lsp-parity.yaml` with namespace alias,
  interface receiver method, concrete implementation method, extension method,
  partial property, and not-planned source-generator scenarios.
- Added `fixtures/csharp/lsp-parity/` as the first minimal C# parity fixture.
- Added `benchmarks/cases/cpp-lsp-parity.yaml` with using alias, virtual base
  method, concrete override method, overload precision, template call, and
  unsupported compile-command-sensitive scenarios.
- Added `fixtures/cpp/lsp-parity/` as the first minimal C++ parity fixture.
- Verified focused Bifrost runs against `origin/master` resolved to
  `96f5f3a9b099cfe72e83994dbc99dcad3db6b516`.
- The focused C# corpus includes a not-planned source-generator case; the
  focused C++ corpus still includes an unsupported compile-command case.
- Filed C# alias misses as `bifrost#422`, receiver method reference-scan misses
  as `bifrost#423`, partial property receiver misses as `bifrost#424`, and
  extension method reference misses as `bifrost#425`.
- Filed C++ using-alias class resolution misses as `bifrost#426`, overload
  precision misses as `bifrost#427`, concrete member reference-scan misses as
  `bifrost#428`, and template reference-site misses as `bifrost#429`.

First-pass baseline language coverage:

- All languages with current baseline case files now have a corresponding
  `*-lsp-parity.yaml` seed file and a minimal checked-in `fixtures/<language>/lsp-parity/`
  source tree, except special Rust follow-up cases that already had separate
  benchmark files before this plan.
- Second-round milestones should deepen one language at a time rather than
  broadening every parity fixture at once.

### 5. Reporting and Follow-through

Status: active.

After each language milestone:

- Run `cargo test`.
- Run `cargo run -- validate benchmarks/cases`.
- Run:

```bash
cargo run -- run-bifrost benchmarks/cases \
  --bifrost-repo /Users/dave/Workspace/BrokkAi/bifrost \
  --bifrost-commit origin/master \
  --output target/usagebench/lsp-parity-<language>.json \
  --keep-worktrees
```

- Confirm the report metadata includes the resolved Bifrost commit before using
  results for issue triage.
- Separate stale `expectedFailure` blocks from real analyzer gaps.
- Convert meaningful misses into standalone Bifrost issues. Each issue should
  include the case path, the reproducible command, and the behavior expectation
  stated as an analyzer problem rather than benchmark fallout.

## Case Authoring Workflow

For each new parity case:

1. Select one upstream test scenario and record its upstream commit, path, and
   test name.
2. Decide whether the behavior needs a pinned upstream project. If not, create a
   minimal checked-in fixture.
3. Author exact LSP ranges where practical. If only line-level selection is
   realistic, use the existing disambiguation mechanism.
4. Record provenance and maturity status in `verification.notes`.
5. Validate the corpus.
6. Run Bifrost and inspect both missing expected locations and unexpected extra
   locations.
7. If Bifrost misses a meaningful case, file or update the product issue without
   weakening the benchmark assertion.

## Validation Strategy

For this living-plan document:

- Check that it remains consistent with `benchmarks/README.md` and
  `schema/benchmark-case.schema.json`.
- Keep links and paths stable enough for future agents to follow.

For future corpus milestones:

- `cargo test`
- `cargo run -- validate benchmarks/cases`
- `cargo run -- run-bifrost ...` against the exact target Bifrost checkout
- Manual inspection of report metadata, missing locations, and unexpected extra
  locations before changing expectations

## Open Questions

- Does `verification.notes` remain sufficient once Rust and Java each have
  several mined cases, or should provenance become structured schema data?
- Should implementation-style cases get a dedicated benchmark surface, or is
  `usageLookups` plus expected declaration enough for the first pass?
- Which second-round language milestone gives the highest maturity signal for
  the current Bifrost roadmap now that every baseline language has a parity
  seed: Rust/Java depth, JS/TS precision, or C#/C++ semantic resolution?
