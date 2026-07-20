# Ground-truth review: DavidBakerEffendi

- Reviewer: `DavidBakerEffendi` (GitHub username)
- Started: 2026-07-17
- Review stream: first independent human review
- Corpus provenance: cases and expected locations were generated agentically
  from upstream LSP tests, existing analyzer tests, and observed edge cases.
- Procedure: inspect fixture source and the authored contract before revealing
  analyzer outcomes; then adjudicate any contract or adapter mismatch.

Document-level `groundTruth` metadata remains `legacy_unattributed` until every
case in that document has completed this review. A second independent human
review is still required before promotion to the evaluation partition.

## Decisions

### rust-parity-module-declaration-definition

- Source: `fixtures/rust/lsp-parity/src/lib.rs`
- Authored query: `workflow` in `use workflow::{Job, Named};`
- Authored target: `workflow` in `pub mod workflow;`
- Ground-truth decision: **correct**
- Operation decision: **declaration-only**
- Reviewer rationale: the imported module name is a usage of the module
  declaration token, consistent with imports and namespace declarations.
- Outcome revealed after review: the legacy Bifrost run reached the authored
  declaration; rust-analyzer, queried through `textDocument/definition`, opened
  the module body at `workflow.rs`.
- Adjudication: retain the declaration token. Mark the lookup explicitly as a
  declaration operation; an LSP without declaration support is unsupported for
  this case and must not fall back to definition.
- Harness follow-up: added explicit per-lookup navigation operations. Legacy
  development cases retain `profile_default`, while evaluation cases must choose
  `declaration` or `definition`.

### rust-parity-ufcs-trait-method-definition

- Source: `fixtures/rust/lsp-parity/src/lib.rs`
- Authored query: `run` in `LocalRunner::run(job)`.
- Authored target: `run` in the default method body on `Runner`.
- Ground-truth decision: **correct**
- Operation decision: **definition**
- Reviewer rationale: `LocalRunner` is statically known and does not override
  `run`, so the resolved executable definition is the trait's default method.
  A concrete override should take precedence when one exists.
- Ambiguity boundary: do not guess a merely likely runtime implementation. A
  dynamically ambiguous receiver needs an implementation-set or conservative
  multi-target contract rather than a single exact definition.
- Harness follow-up: set this lookup to `operation: definition`.

## Issue candidates discovered during review

1. Add a distinct implementation-navigation benchmark operation mapped to LSP
   `textDocument/implementation` and an equivalent Bifrost language-adapter
   contract.
2. Model ambiguous runtime dispatch as reviewed multi-target results, preserving
   confidence, rather than selecting a most-likely implementation.
3. Rename or compatibility-alias `expectedDeclaration` in the schema/report to
   `expectedTarget`, since a reviewed lookup may now request a definition.
4. Add Rust minimal pairs for inherited default methods, concrete overrides,
   and `dyn Trait` dispatch; use them to harden the Bifrost Rust adapter before
   making claims about runtime implementation resolution.

### rust-parity-associated-type-definition-no-movement

- Source: `fixtures/rust/lsp-parity/src/lib.rs`
- Authored query: `Output` in the defining impl item `type Output = String;`.
- Authored target: `Output` in the trait declaration `type Output;`.
- Ground-truth decision: **incorrect**
- Reviewer rationale: the cursor is already on the associated-type definition;
  ordinary declaration/definition navigation should not move to another token.
  Reaching the trait member should require an explicit overriding or
  implemented-member operation.
- Outcome revealed after review: the assertion came from rust-analyzer's
  `goto_def_of_trait_impl_type_alias` test, but no raw UsageBench result was
  retained. This is an intentional contract disagreement, not evidence that the
  authored expectation is correct.
- Adjudication: retain this shape as a negative precision control. An empty
  result or an exact self-target is acceptable; navigation to the trait member
  is a false positive.
- Harness follow-up: renamed the case and added `expectNoMovement: true`, with
  the expected target repeated as the exact usage token.

### rust-parity-associated-type-use-definition

- Source: `fixtures/rust/lsp-parity/src/lib.rs`
- Authored query: `Output` in `<LocalRunner as Runner>::Output`.
- Authored target: `Output` in `type Output = String;` on the `LocalRunner`
  implementation.
- Ground-truth decision: **correct**
- Operation decision: **definition**
- Reviewer rationale: the fully qualified use statically selects the concrete
  `Runner` implementation for `LocalRunner`.
- Adjudication: keep both associated-type cases. Together they test a positive
  use-to-definition result and guard against a false-positive jump from the
  definition token itself.

### rust-parity-direct-function-reference

- Source: `fixtures/rust/lsp-parity/src/lib.rs`
- Authored definition: `direct_job` in `pub fn direct_job() -> Job`.
- Authored usage: `direct_job()` in `call_direct`.
- Ground-truth decision: **correct**
- Operation decision: **definition**
- Reviewer rationale: the function body establishes the definition, the call
  is its usage, and the definition token itself is not an expected reference
  result.
- Outcome revealed after review: both Bifrost and rust-analyzer matched the
  authored reference and reverse-definition expectations exactly in the
  retained comparison.

### rust-parity-macro-generated-function-reference

- Source: `fixtures/rust/lsp-parity/src/lib.rs`
- Authored definition anchor: `generated_job` in
  `define_job_maker!(generated_job)`.
- Authored usage: `generated_job()` in `call_generated`.
- Ground-truth decision: **correct**
- Operation decision: **definition**
- Reviewer rationale: the invocation argument is the available file-backed
  anchor for the macro-generated function definition; the later call is its
  usage and the anchor itself is not an expected usage.
- Outcome revealed after review: rust-analyzer matched both directions exactly.
  Bifrost missed both because it does not expand `define_job_maker!`, matching
  the reviewer's prediction.

The first independent human review of every case currently in
`rust-lsp-parity.yaml` is complete. Its document metadata remains
`legacy_unattributed` until a second independent reviewer completes the
promotion requirement; this log preserves the first review meanwhile.

## rust-baseline.yaml

### rust-function-call-and-reexport

- Source: `fixtures/rust/baseline/src/service.rs` and `src/lib.rs`.
- Authored definition: `build_service` in its function body in `service.rs`.
- Required usage: the `build_service(repository)` call in `run_demo`.
- Optional binding: the `pub use service::{build_service, ...}` re-export.
- Ground-truth decision: **correct and clear**
- Operation decision: **definition**
- Reviewer rationale: the call should navigate to the `service.rs` function
  definition, while including or omitting the re-export binding is acceptable.
- Outcome revealed after review: Bifrost satisfied the authored result. The
  retained rust-analyzer comparison was marked near only because it additionally
  returned the re-export binding, which the reviewed `bindings_optional` policy
  now accepts.

### rust-struct-construction

- Source: `fixtures/rust/baseline/src/service.rs` and `src/lib.rs`.
- Authored definition: `Service` in `pub struct Service`.
- Required usages: `impl Service`, the `build_service` return type,
  `Service::new`, and both capital-`Self` type references inside the
  implementation.
- Excluded location: lowercase `self` in `self.repository`, which is the method
  value receiver rather than a type reference.
- Optional binding: the `pub use service::{Service, ...}` re-export.
- Ground-truth decision: **correct**
- Operation decision: **definition**
- Revised reviewer rationale: after reproducing the editor behavior, the
  reviewer corrected the initial token-spelling rule. Rust capital-`Self`
  denotes the current implementing type and should be treated as a semantic
  type reference. This differs from Java `this` and Rust lowercase `self`,
  which are value receivers and remain excluded from type references.
- Fresh Bifrost 0.8.4 CLI outcome: `scan_usages_by_location` returned the three
  literal `Service` usages but omitted both capital-`Self` references.
- Fresh Bifrost 0.8.4 LSP outcome: it returned both required capital-`Self`
  references and also returned lowercase `self`, which is an unexpected type
  reference under the revised contract.
- Follow-up: filed `BrokkAi/bifrost#882` to distinguish Rust capital-`Self`
  type references from lowercase `self` receivers across usage surfaces.

### rust-method-call

- Source: `fixtures/rust/baseline/src/service.rs` and `src/lib.rs`.
- Authored definition: `execute` in the concrete `Service` implementation.
- Required usage: `service.execute(" Grace ")` in `run_demo`.
- Ground-truth decision: **correct**
- Operation decision: **definition**
- Reviewer rationale: the receiver is concretely a `Service`, so navigation
  reaches `Service::execute`; no method-family or receiver tokens belong in the
  reference result.
- Retained outcome: no case-specific raw analyzer result remains in the
  repository, so no post-review outcome is asserted here.

### rust-field-write

- Previous ID: `rust-field-access-and-constant`; renamed because the case does
  not contain a constant lookup.
- Source: `fixtures/rust/baseline/src/service.rs`.
- Authored declaration: `last` in `pub last: String`.
- Required usage: the `last` field write in `self.last = value.to_string()`.
- Ground-truth decision: **correct**
- Operation decision: **declaration**
- Reviewer rationale: the field has no implementation body, so navigation from
  its write should reach the field declarator. Lowercase `self` is neither a
  field reference nor an owning-type reference.
- Retained outcome: no case-specific raw analyzer result remains in the
  repository, so no post-review outcome is asserted here.

### rust-explicit-local-and-parameter-type-lookup

- Source: `fixtures/rust/baseline/src/service.rs`.
- Authored type lookups:
  - `local_label` at the `formatter.render(local_label)` call resolves to
    `AuditLabel`.
  - `event` in `event.label` resolves to `AuditEvent`.
- Ground-truth decision: **correct**
- Operation decision: **type definition** for both lookups.
- Reviewer rationale: these are explicit variable/parameter type questions,
  not ordinary declaration or definition navigation. No `formatter` lookup is
  authored in this focused case.
- Retained outcome: no case-specific raw analyzer result remains in the
  repository, so no post-review outcome is asserted here.

### rust-imported-type-and-member-type-lookup

- Source: `fixtures/rust/baseline/src/lib.rs` and `src/service.rs`.
- Authored type lookups:
  - The `formatter` argument resolves through its explicit imported annotation
    to `AuditFormatter`.
  - The `label` field access resolves through `event: AuditEvent` and the
    field's declaration to `AuditLabel`.
- Ground-truth decision: **correct**
- Operation decision: **type definition** for both lookups.
- Reviewer rationale: both questions ask for the statically declared type of an
  expression; the re-export does not change the underlying type definition.
- Retained outcome: no case-specific raw analyzer result remains in the
  repository, so no post-review outcome is asserted here.

The first independent human review of every case currently in
`rust-baseline.yaml` is complete. Its document metadata remains
`legacy_unattributed` until a second independent reviewer completes the
promotion requirement; this log preserves the first review meanwhile.

## rust-precision.yaml

### rust-ufcs-trait-method-through-barrel

- Source: `fixtures/rust/precision/src/service.rs`, `src/facade.rs`, and
  `src/lib.rs`.
- Authored declaration: `run` in the `Worker` trait.
- Required usages: both `Worker::run(...)` UFCS call tokens, including the call
  whose receiver uses `LocalAlias`.
- Excluded location: the concrete overriding `Local::run` definition.
- Ground-truth decision: **correct**
- Operation decision: **declaration** from both call tokens to the trait method.
- Reviewer rationale: resolving ordinary navigation to the public `Worker`
  trait contract is appropriate; it should not guess a runtime implementation.
  Finding the concrete override belongs to an explicit implementation
  operation.
- Harness follow-up: added reverse declaration lookups for both call tokens.
- Outcome revealed after review: Bifrost satisfied the retained authored call
  set. rust-analyzer returned both calls plus the queried trait declaration;
  the declaration token is not itself a usage when `includeDeclaration` is
  false.

### rust-barrel-trait-static-qualifier

- Source: `fixtures/rust/precision/src/service.rs`, `src/facade.rs`, and
  `src/lib.rs`.
- Authored declaration: `Worker` in `pub trait Worker`.
- Required usages: `Worker` in `impl Worker for Local` and both `Worker::run`
  qualifiers.
- Optional bindings: the `Worker` re-exports in `facade.rs` and `lib.rs`.
- Ground-truth decision: **correct**
- Operation decision: **declaration** from both UFCS qualifiers to the trait.
- Reviewer rationale: like the preceding method case, the qualifiers refer to
  the public trait contract; re-export bindings may appear or be omitted.
- Harness follow-up: added reverse declaration lookups for both qualifiers.
- Outcome revealed after review: Bifrost satisfied the retained required usage
  set. The retained rust-analyzer comparison differed only by returning the
  re-export bindings, which `bindings_optional` now accepts.

The first independent human review of every case currently in
`rust-precision.yaml` is complete. Its document metadata remains
`legacy_unattributed` until a second independent reviewer completes the
promotion requirement; this log preserves the first review meanwhile.

## rust-sibling-associated-type.yaml

### rust-sibling-module-type-segment-definition

- Previous ID: `rust-sibling-module-associated-type-segment-definition`;
  renamed because `AppState` is a type qualifier for an associated function,
  not an associated type.
- Source: `fixtures/rust/sibling-associated-type/src/state.rs` and
  `src/main.rs`.
- Authored query: `AppState` in `AppState::with_environment()`.
- Authored target: `AppState` in the unit-struct definition.
- Ground-truth decision: **correct**
- Operation decision: **definition**
- Reviewer rationale: the unit struct fully defines the type. Navigation must
  not land on the `impl AppState` token or `with_environment`.
- Retained outcome: no case-specific raw analyzer result remains in the
  repository, so no post-review outcome is asserted here.

The first independent human review of every case currently in
`rust-sibling-associated-type.yaml` is complete. Its document metadata remains
`legacy_unattributed` until a second independent reviewer completes the
promotion requirement; this log preserves the first review meanwhile.

## java-baseline.yaml

### java-service-class-construction

- Source: `fixtures/java/baseline/src/main/java/example/Service.java` and
  `src/test/java/example/ServiceTest.java`.
- Authored declaration: `Service` in `public class Service`.
- Required usages: both outer qualifiers in `Service.Repository`, the variable
  type, the constructor call, and the `Service.DEFAULT_PREFIX` qualifier.
- Excluded locations: the class declaration and constructor-definition token.
- Ground-truth decision: **correct**
- Operation decision: **definition** from `new Service(...)` to the constructor
  body.
- Reviewer rationale: the five expected tokens are semantic uses of the outer
  class in type, construction, nested-type qualification, and static-member
  contexts. A member definition is not itself a class usage on this surface.
- Outcome revealed after review: JDT LS matched the authored expectation
  exactly. Bifrost missed the two outer `Service` qualifiers on
  `Service.Repository`.
- Follow-up: filed `BrokkAi/bifrost#895` for the missing outer-type qualifier
  usages.

### java-service-method-call

- Source: `fixtures/java/baseline/src/main/java/example/Service.java` and
  `src/test/java/example/ServiceTest.java`.
- Authored definition: the concrete `Service.execute` method body.
- Required usage: `service.execute(" Ada ")`.
- Ground-truth decision: **correct**
- Operation decision: **definition**
- Reviewer rationale: this is an easy dynamic-dispatch case because the
  receiver is concretely `Service` and only one implementation exists.
- Retained outcome: no case-specific raw analyzer result remains in the
  repository, so no post-review outcome is asserted here.

### java-static-field-access

- Source: `fixtures/java/baseline/src/main/java/example/Service.java` and
  `src/test/java/example/ServiceTest.java`.
- Authored declaration: `DEFAULT_PREFIX` in the static-final field declarator.
- Required usages: the unqualified read inside `Service.execute` and the
  qualified read in `Service.DEFAULT_PREFIX`.
- Ground-truth decision: **correct**
- Operation decision: **declaration**
- Reviewer rationale: the initializer does not turn the field into a callable
  implementation body. Navigation reaches the declarator, while `Service` is a
  separate class qualifier usage.
- Retained outcome: no case-specific raw analyzer result remains in the
  repository, so no post-review outcome is asserted here.

### java-nested-class-constructor

- Source: `fixtures/java/baseline/src/main/java/example/Service.java` and
  `src/test/java/example/ServiceTest.java`.
- Authored definition: `Repository` in the nested static class definition.
- Required usages: the field type, constructor-parameter type, test variable
  type, and nested-class construction token.
- Ground-truth decision: **correct**
- Operation decision: **definition** from the construction to the class because
  its default constructor is implicit.
- Reviewer rationale: all four tokens semantically use the nested class. The
  outer `Service` qualifiers are scored separately against the outer class.
- Outcome revealed after review: JDT LS matched the authored expectation
  exactly. Bifrost missed the `Repository` field and constructor-parameter type
  usages.
- Issue candidate: extend Java nested-class inverse usage to retain field and
  parameter type occurrences, separately from `BrokkAi/bifrost#895`'s outer
  qualifier work.

The first independent human review of every case currently in
`java-baseline.yaml` is complete. Its document metadata remains
`legacy_unattributed` until a second independent reviewer completes the
promotion requirement; this log preserves the first review meanwhile.

## java-precision.yaml

### java-lambda-body-member-call

- Source: `fixtures/java/precision/src/main/java/precision/Precision.java`.
- Authored definition: the concrete `Worker.execute` method body.
- Required usage: `execute` in the inline `new Worker().execute()` lambda body.
- Ground-truth decision: **correct**
- Operation decision: **definition**
- Reviewer rationale: the lambda boundary does not change the concretely
  constructed receiver; `task.run()` is unrelated.
- Harness follow-up: added the reverse definition lookup from the call.
- Retained outcome: no case-specific raw analyzer result remains in the
  repository, so no post-review outcome is asserted here.

### java-record-type-reference

- Source: `fixtures/java/precision/src/main/java/precision/Precision.java`.
- Authored definition: `Entry` in the record definition.
- Required usages: the local type annotation and `new Entry(...)` construction
  token.
- Ground-truth decision: **correct**
- Operation decision: **definition** from the construction to the record
  definition because the canonical constructor is implicit.
- Reviewer rationale: the record token fully defines the type and its implicit
  canonical constructor; the definition token itself is not a usage.
- Harness follow-up: added the reverse definition lookup from the construction.
- Retained outcome: no case-specific raw analyzer result remains in the
  repository, so no post-review outcome is asserted here.

### java-static-qualified-method-call

- Source: `fixtures/java/precision/src/main/java/precision/Helpers.java` and
  `Precision.java`.
- Authored definition: the concrete static `Helpers.log` method body.
- Required usage: `log` in `Helpers.log()`.
- Ground-truth decision: **correct**
- Operation decision: **definition**
- Reviewer rationale: the member token is a static method usage; `Helpers` is a
  separate type qualifier rather than another method reference.
- Harness follow-up: added the reverse definition lookup from the call.
- Retained outcome: no case-specific raw analyzer result remains in the
  repository, so no post-review outcome is asserted here.

The first independent human review of every case currently in
`java-precision.yaml` is complete. Its document metadata remains
`legacy_unattributed` until a second independent reviewer completes the
promotion requirement; this log preserves the first review meanwhile.

## java-lsp-parity.yaml

### java-parity-static-import-method-call

- Source: `fixtures/java/lsp-parity/src/main/java/example/parity/Names.java`,
  `ConsoleHandler.java`, and `StaticImportUser.java`.
- Authored definition: the concrete static `Names.normalize` method body.
- Required usages: the qualified and statically imported calls.
- Optional binding: the static-import declaration.
- Ground-truth decision: **correct**
- Operation decision: **definition**
- Reviewer rationale: both call forms name the same static method; the import
  binding may be present or omitted without changing the runtime usage set.
- Outcome revealed after review: Bifrost satisfied the authored calls. The
  retained JDT LS comparison additionally returned the static-import binding,
  which `bindings_optional` now accepts.

### java-parity-interface-receiver-method-call

- Source: `fixtures/java/lsp-parity/src/main/java/example/parity/Handler.java`
  and `Runner.java`.
- Authored declaration: `handle` in the `Handler` interface.
- Required usages: `handler.handle(...)` and `makeAnonymous().handle(...)`.
- Excluded locations: concrete and anonymous overriding method definitions, and
  the concrete `direct.handle(...)` call.
- Ground-truth decision: **correct after tightening**
- Operation decision: **declaration** from both interface-typed calls.
- Reviewer rationale: both expressions have the statically declared `Handler`
  contract. Runtime implementation may change, so ordinary navigation should
  conservatively link only the interface declaration rather than guess an
  implementation.
- Harness follow-up: promoted the anonymous-return call from unproven to a
  required interface reference, removed the concrete override allowance, and
  added declaration navigation for both calls.

### java-parity-concrete-implementation-method-call

- Source: `fixtures/java/lsp-parity/src/main/java/example/parity/ConsoleHandler.java`
  and `Runner.java`.
- Authored definition: the concrete overriding `ConsoleHandler.handle` body.
- Required usage: `direct.handle(...)`, whose receiver has concrete
  `ConsoleHandler` type.
- Excluded locations: the `Handler.handle` declaration and
  `makeAnonymous().handle(...)`.
- Ground-truth decision: **correct after tightening**
- Operation decision: **definition**
- Reviewer rationale: a concretely typed receiver resolves to the override when
  present. The anonymous-return expression has only the declared `Handler`
  return type and therefore remains linked conservatively to the interface.
- Harness follow-up: removed both the interface-declaration allowance and the
  anonymous-call conservative candidate from the concrete method case.
- Retained outcome: the old JDT LS comparison included the anonymous call for
  the concrete method. Under the revised contract, that is a false positive
  rather than an allowed broad match.

- Issue candidate: add an explicit implemented-member/override navigation
  operation instead of overloading ordinary declaration or definition
  navigation from a token that is already its own definition.

### java-parity-interface-local-type-lookup

- Source: `fixtures/java/lsp-parity/src/main/java/example/parity/Runner.java`
  and `Handler.java`.
- Expression: the local variable `handler`, initialized with
  `new ConsoleHandler()` but explicitly declared as `Handler`.
- Expected type: the `Handler` interface declaration.
- Ground-truth decision: **correct**
- Reviewer rationale: type-definition lookup follows the variable's declared
  static type. The concrete initializer does not change the type denoted by the
  declaration.

The first independent human review of every case currently in
`java-lsp-parity.yaml` is complete. Its document metadata remains
`legacy_unattributed` until a second independent reviewer completes the
promotion requirement; this log preserves the first review meanwhile.
