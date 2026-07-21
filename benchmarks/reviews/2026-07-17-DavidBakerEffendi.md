# Ground-truth review: DavidBakerEffendi

- Reviewer: `DavidBakerEffendi` (GitHub username)
- Started: 2026-07-17
- Review stream: first independent human review
- Corpus provenance: cases and expected locations were generated agentically
  from upstream LSP tests, existing analyzer tests, and observed edge cases.
- Procedure: inspect fixture source and the authored contract before revealing
  analyzer outcomes; then adjudicate any contract or adapter mismatch.
- Evidence standard: a mature, widely used language server is comparison
  evidence rather than automatic ground truth, but contradicting it requires
  stronger support than intuition alone. Recheck the language's declaration,
  definition, and usage semantics and seek a minimal reproduction or
  corroborating evidence before preserving the disagreement.

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

## cpp-baseline.yaml

### cpp-function-call

- Source: `fixtures/cpp/baseline/include/service.h`, `src/service.cpp`,
  and `src/main.cpp`.
- Authored declaration: the header declaration of `build_service`.
- Required usage: the call in `run_demo`.
- Excluded location: the out-of-line function definition.
- Ground-truth decision: **correct after separating usages from definitions**
- Operation decision: **definition** from the call to the unique body in
  `service.cpp`; an explicit declaration request would instead target the
  header declaration.
- Reviewer rationale: all three tokens share one function identity, but a C++
  function definition is itself a declaration/definition rather than a usage.
  Ordinary definition navigation from the statically resolved direct call
  should nevertheless reach that executable body.
- Correction history: the initial review treated the out-of-line definition as
  a usage. Apple clangd 21.0 omitted it with declarations excluded while still
  navigating from the call to the exact body. Re-examining the semantic
  categories showed that clangd's split was appropriate, so the definition was
  removed from `expectedUsages` rather than retained as an LSP disagreement.
- Outcome after correction: clangd passed the corrected case exactly. Bifrost
  returned the out-of-line definition as an extra usage and returned both the
  header and body for definition navigation, marked `multiple_targets` with
  `unproven_cpp_link_unit`; both disagree with the reviewed narrow contract.
- Issue candidate: separate C++ definitions from ordinary usages and let
  definition navigation prefer the unique body when the fixture establishes
  one, without weakening genuinely ambiguous multi-link-unit projects.

### cpp-class-reference

- Source: `fixtures/cpp/baseline/include/service.h` and `src/service.cpp`.
- Authored definition: the `class Service` body.
- Required usages: `Service` as the `build_service` return type in the header
  and implementation; the class qualifier in `Service::Service` and
  `Service::execute`; and the type named by `return Service(repository)`.
- Excluded locations: the constructor declaration token and the second
  `Service` in `Service::Service`, which is the constructor definition token.
- Ground-truth decision: **correct after adding the construction type usage**
- Operation decision: **definition** from a type usage to the `class Service`
  body.
- Reviewer rationale: the construction expression explicitly names the class
  type as well as invoking its constructor, so it participates in both the
  class-reference and constructor-call cases. Constructor declaration and
  definition tokens remain declarations/definitions rather than usages.
- Outcome revealed after review: Apple clangd 21.0 found all five required
  class usages but also returned the constructor declaration and constructor
  definition tokens. The C++ draft's `[class.ctor]` section states that these
  declarators declare a constructor and that constructors do not have names;
  Clang's AST likewise represents both as `CXXConstructorDecl`, distinct from
  the `CXXRecordDecl` for `Service`. The extras are therefore retained as a
  documented constructor-family policy expansion rather than accepted as
  ordinary class usages.
- Bifrost status: the prior result predates the added construction type usage.
  A current-master rerun did not complete within the interactive review window,
  so no updated Bifrost outcome is asserted yet.

### cpp-constructor-call

- Source: `fixtures/cpp/baseline/include/service.h` and `src/service.cpp`.
- Authored declaration: `Service(Repository&)` in the class body.
- Required usage: `Service(repository)` in `build_service`.
- Excluded location: the second `Service` in the out-of-line
  `Service::Service` definition.
- Ground-truth decision: **correct after C++ token-identity correction**
- Operation decision: **definition** to the `class Service` body.
- Reviewer rationale: the construction expression statically and uniquely
  causes the `Repository&` constructor to be called, so it is a constructor
  usage in the forward reference graph. However, C++ constructors have no
  names: the `Service` source token in functional construction names the class
  type. Ordinary navigation therefore reaches the class definition; reaching
  the invoked constructor body would require a distinct call-target operation.
- Correction history: the initial review proposed navigating to the
  out-of-line constructor body. Apple clangd 21.0 instead returned the exact
  class definition while satisfying the constructor usage set. The C++
  constructor rules explain this split, so the class target was adopted rather
  than treating clangd's result as a navigation failure.
- Outcome after correction: clangd passed both the constructor usage set and
  the class-definition navigation target exactly. The retained Bifrost result
  predates this corrected target and requires a rerun.

### cpp-method-call

- Source: `fixtures/cpp/baseline/include/service.h`, `src/service.cpp`, and
  `src/main.cpp`.
- Authored declaration: `Service::execute` in the class body.
- Required usage: `service.execute(...)` in `run_demo`.
- Excluded location: the out-of-line `Service::execute` definition token.
- Ground-truth decision: **correct after separating usages from definitions**
- Operation decision: **definition** from the call to the out-of-line method
  body; explicit declaration navigation would target the header.
- Reviewer rationale: `service` has the statically inferred concrete type
  `Service`, and `execute` is non-virtual, so the call resolves uniquely. The
  out-of-line body is a definition rather than an ordinary method usage.
- Outcome revealed after review: clangd passed the corrected usage and
  definition-navigation contract exactly. Bifrost at
  `a84d6df418e8975019007a60872e5788320ff54f` returned the out-of-line method
  definition as an extra usage and returned both header and body for definition
  navigation, marked `multiple_targets` with `unproven_cpp_link_unit`.
- Issue candidate: same C++ declaration/definition separation and unique-body
  navigation gap recorded for `cpp-function-call`.

### cpp-field-access

- Source: `fixtures/cpp/baseline/include/service.h`, `src/service.cpp`, and
  `src/main.cpp`.
- Authored declaration: `Repository::last` in the class body.
- Required usages: the unqualified write and read in `Repository::save`, plus
  the qualified `repository.last` read in `run_demo`.
- Ground-truth decision: **correct**
- Operation decision: **declaration** from the qualified read to the field
  declaration.
- Reviewer rationale: reads and writes both use the field. The unqualified
  tokens resolve through the implicit `this`, while the qualified token has the
  concrete static receiver type `Repository`; there is no separate executable
  definition target.
- Outcome revealed after review: clangd passed the complete usage set and
  declaration target exactly. Bifrost at
  `a84d6df418e8975019007a60872e5788320ff54f` marked the case unsupported
  because it does not expose declaration navigation separately from
  `get_definitions_by_location`; no semantic Bifrost pass/fail is asserted.
- Issue candidate: expose language-adapter declaration navigation without
  silently falling back to definition navigation.

### cpp-constant-access

- Source: `fixtures/cpp/baseline/include/service.h`, `src/service.cpp`, and
  `src/main.cpp`.
- Authored binding: the inline `constexpr` variable `DefaultPrefix` in the
  header.
- Required usages: the reads in `Service::execute` and `run_demo`.
- Ground-truth decision: **correct**
- Operation decision: **declaration** to the header binding.
- Reviewer rationale: the inline variable is declared and defined at one
  source token and has no separate executable body. Both reads are usages;
  declaration navigation names the binding directly without depending on a
  definition fallback that happens to reach the same token.
- Outcome revealed after review: clangd passed both reads and the declaration
  target exactly. Bifrost at
  `a84d6df418e8975019007a60872e5788320ff54f` marked the case unsupported
  because it does not expose declaration navigation separately from
  `get_definitions_by_location`, the same capability boundary as
  `cpp-field-access`.

The first independent human review of every case currently in
`cpp-baseline.yaml` is complete. Its document metadata remains
`legacy_unattributed` until a second independent reviewer completes the
promotion requirement; this log preserves the first review meanwhile.

## cpp-precision.yaml

### cpp-out-of-line-member-call

- Source: `fixtures/cpp/precision/include/worker.h`, `src/worker.cpp`, and
  `src/consumer.cpp`.
- Authored declaration: `Worker::execute` in the class body.
- Required usage: `worker.execute()` through the concrete `Worker` receiver.
- Excluded locations: the out-of-line method definition, the commented-out
  call, and the same text inside a string literal.
- Ground-truth decision: **correct after separating usages from definitions**
- Operation decision: **definition** from the call to the unique out-of-line
  body; explicit declaration navigation would target the header.
- Reviewer rationale: the receiver is statically `precision::Worker` and the
  method is non-virtual, so the executable definition is unambiguous. Comments
  and string contents are not semantic usages.
- Outcome revealed after review: clangd passed the corrected call-only usage
  set and exact definition target. Bifrost at
  `a84d6df418e8975019007a60872e5788320ff54f` returned the out-of-line body as
  an extra usage and returned header plus body for definition navigation,
  marked `multiple_targets` with `unproven_cpp_link_unit`.
- Issue candidate: same C++ declaration/definition separation and unique-body
  navigation gap as the reviewed baseline function and method cases.

### cpp-overload-string-call-is-narrow

- Source: `fixtures/cpp/precision/include/worker.h`, `src/worker.cpp`, and
  `src/consumer.cpp`.
- Authored declaration: the `select(const char*)` overload in the header.
- Required usage: `precision::select("name")`.
- Excluded locations: both overload definitions and the separate `select(int)`
  declaration identity.
- Ground-truth decision: **correct after separating usages from definitions**
- Operation decision: **definition** to the exact `select(const char*)` body.
- Reviewer rationale: the string literal decays to `const char*`, while the
  `int` overload is not viable. Both forward references and reverse navigation
  must preserve the selected overload identity rather than accepting a
  same-name function.
- Outcome revealed after review: clangd passed the narrow usage and exact
  overload-definition target. Bifrost at
  `a84d6df418e8975019007a60872e5788320ff54f` preserved the selected overload
  for forward usages but returned its body as an extra usage; reverse
  definition lookup returned both declarations and both bodies as
  `multiple_targets` under `unproven_cpp_link_unit`.
- Issue candidate: preserve C++ overload signatures in reverse definition
  lookup while separately resolving header declarations to out-of-line bodies.

The first independent human review of every case currently in
`cpp-precision.yaml` is complete. Its document metadata remains
`legacy_unattributed` until a second independent reviewer completes the
promotion requirement; this log preserves the first review meanwhile.

## cpp-lsp-parity.yaml

### cpp-parity-using-alias-constructor

- Source: `fixtures/cpp/lsp-parity/include/parity.h`, `src/parity.cpp`, and
  `src/main.cpp`.
- Authored declaration: the `class ConsoleHandler` body.
- Required usages: the `ConsoleHandler` alias RHS, the class qualifiers in the
  out-of-line constructor and method definitions, and the `HandlerAlias`
  construction expression.
- Ground-truth decision: **correct after separating alias and type navigation**
- Operation decisions: **declaration** from the spelled `HandlerAlias`
  construction token to the `using HandlerAlias = ...` binding; **type
  definition** from the same token to the underlying `class ConsoleHandler`.
- Reviewer rationale: a C++ `using` alias is an immutable synonym for its
  underlying type, so constructing `HandlerAlias` is a transitive class usage.
  Ordinary declaration navigation preserves the source-level alias identity
  instead of skipping the binding, while the distinct type-definition
  operation exposes the canonical underlying record. The C++ draft's
  [`[dcl.typedef]`](https://eel.is/c%2B%2Bdraft/dcl.typedef) wording gives an
  alias-declaration the same semantics as a typedef, while
  [`[basic.pre]`](https://eel.is/c%2B%2Bdraft/basic.pre) distinguishes the alias
  entity from its underlying entity.
- Outcome revealed after review: Apple clangd 21.0 passed both navigation
  operations exactly. Its reference query found the alias RHS but omitted the
  two out-of-line class qualifiers and the construction through
  `HandlerAlias`; it also returned the constructor declaration token despite
  `includeDeclaration: false`. This is retained as a reference-identity and
  declaration-classification disagreement rather than used to collapse the
  two reviewed navigation operations.
- Bifrost outcome at `a84d6df418e8975019007a60872e5788320ff54f`: all four
  usages were returned at line precision. Declaration navigation was
  unsupported because Bifrost exposes no distinct declaration operation, and
  type lookup failed with `unsupported_language` because C++ type lookup is
  not implemented.
- Issue candidates: expose declaration navigation through the language adapter
  and implement C++ type lookup without losing alias-to-underlying-type
  resolution.

### cpp-parity-virtual-base-method-call

- Source: `fixtures/cpp/lsp-parity/include/parity.h` and `src/main.cpp`.
- Authored declaration: the pure-virtual `BaseHandler::handle` member.
- Required usage: `base.handle("Ada")` through the statically declared
  `BaseHandler&` receiver.
- Excluded location: `handler.handle("Ben")`, whose receiver has the concrete
  static type `ConsoleHandler` and therefore names the override.
- Ground-truth decision: **correct after making declaration navigation
  explicit**
- Operation decision: **declaration** from `base.handle` to the pure-virtual
  member in `BaseHandler`.
- Reviewer rationale: ordinary conservative navigation follows the receiver's
  static interface identity. It should not guess a runtime override merely
  because this particular reference was initialized from a `ConsoleHandler`.
  The fixture contains no executable definition of the pure-virtual base
  member, so a default or implicit definition operation would obscure the
  intended contract.
- Outcome revealed after review: Apple clangd 21.0 passed exactly, returning
  only `base.handle("Ada")` as the base-method usage and navigating to the
  pure-virtual declaration.
- Bifrost outcome at `a84d6df418e8975019007a60872e5788320ff54f`: the required
  usage was returned at line precision with no missing or extra line, but the
  case was correctly reported unsupported because Bifrost has no distinct
  declaration-navigation operation.
- Issue candidate: the same language-adapter declaration-navigation capability
  recorded for the reviewed field, constant, and alias cases.

### cpp-parity-concrete-override-method-call

- Source: `fixtures/cpp/lsp-parity/include/parity.h`, `src/parity.cpp`, and
  `src/main.cpp`.
- Authored declaration: `ConsoleHandler::handle` in the class body.
- Required usage: `handler.handle("Ben")` through the concrete
  `ConsoleHandler` receiver.
- Excluded locations: the out-of-line method body, which is a definition, and
  `base.handle("Ada")`, whose receiver has the static type `BaseHandler`.
- Ground-truth decision: **correct after separating the body from usages**
- Operation decision: **definition** from the concrete call to the unique
  out-of-line `ConsoleHandler::handle` body.
- Reviewer rationale: static receiver identity makes the concrete override
  unambiguous. Conversely, the base-reference call should retain the base
  method identity rather than being attributed to this override through
  runtime speculation.
- Outcome revealed after review: Apple clangd 21.0 found the required concrete
  call but also broadened the override query to the pure-virtual base
  declaration and the base-typed call. Definition navigation from the concrete
  call stopped at the override declaration in the header rather than reaching
  its out-of-line body. Clangd exposes virtual implementations as a distinct
  operation, which helps explain its family-oriented editor behavior but does
  not change this benchmark's explicit definition/body contract.
- Bifrost outcome at `a84d6df418e8975019007a60872e5788320ff54f`: no complete
  semantic result was obtained. Two attempts hit `attempt to write a readonly
  database` while retrying deferred analyzer-store writes. On the first
  attempt, reverse definition lookup did complete and returned the header and
  body as `multiple_targets` under `unproven_cpp_link_unit`; the forward usage
  query failed before producing a result.
- Issue candidates: keep concrete override references distinct from their base
  family by default; resolve a known concrete override declaration to its
  unique body; separately investigate the repeatable analyzer-store write
  failure if it persists outside this cached benchmark process.

### cpp-parity-overload-string-function-call

- Source: `fixtures/cpp/lsp-parity/include/parity.h`, `src/parity.cpp`, and
  `src/main.cpp`.
- Authored declaration: the `format(const std::string&)` overload.
- Required usage: `format(first)`, whose argument has type `std::string`.
- Excluded locations: both out-of-line definitions, the `format(int)`
  declaration identity, and `format(7)`.
- Ground-truth decision: **correct after separating usages from definitions**
- Operation decision: **definition** from the string call to the exact
  `format(const std::string&)` body.
- Reviewer rationale: overload resolution uniquely selects the string overload.
  The body is a definition rather than an ordinary reference, while reverse
  navigation must preserve the selected signature and must not merge the
  same-name integer overload.
- Outcome revealed after review: Apple clangd 21.0 passed exactly, returning
  only the string call and navigating to the exact string-overload body.
- Bifrost outcome at `a84d6df418e8975019007a60872e5788320ff54f`: forward
  analysis found the required call at line precision but also returned the
  string-overload body as an extra usage. Reverse definition lookup returned
  the string declaration and body as `multiple_targets` under
  `unproven_cpp_link_unit`; it did not merge the integer overload.
- Issue candidate: the same declaration/body separation and unique-body
  resolution gap as the reviewed precision overload case, while preserving
  the already-correct overload-signature discrimination.

### cpp-parity-template-function-call

- Source: `fixtures/cpp/lsp-parity/include/parity.h` and `src/main.cpp`.
- Authored definition: the `choose<T>` function template body.
- Required usage: the explicit `choose<std::string>(...)` specialization call.
- Ground-truth decision: **correct after making definition navigation
  explicit**
- Operation decision: **definition** from the specialization call to the
  authored function-template body.
- Reviewer rationale: the call instantiates and uses the function template.
  The implicit specialization has no separate authored source body, so normal
  source navigation returns the template definition rather than inventing a
  specialization location.
- Outcome revealed after review: Apple clangd 21.0 passed the usage and exact
  definition target.
- Bifrost outcome at `a84d6df418e8975019007a60872e5788320ff54f`: it found the
  specialization call and resolved it back to the authored function-template
  body. Both directions were reported `position_unverified` solely because the
  C++ analyzer returned line-level rather than token-level locations.
- Adjudication: this is supported Bifrost template behavior with a result-
  precision limitation, not the anticipated semantic template gap.

### cpp-parity-direct-function-call-control

- Source: `fixtures/cpp/lsp-parity/include/parity.h` and `src/main.cpp`.
- Authored definition: the inline `direct_label` function body.
- Required usage: `parity::direct_label("direct")`.
- Ground-truth decision: **correct after making definition navigation
  explicit**
- Operation decision: **definition** from the call to the inline function body.
- Reviewer rationale: this is an ordinary direct call with one source-backed
  definition. It deliberately provides the non-macro control for the paired
  `expanded_label` case, so differences in that pair can be attributed to the
  macro expansion rather than ordinary C++ function resolution.
- Outcome revealed after review: Apple clangd 21.0 passed the reference and
  exact definition target.
- Bifrost outcome at `a84d6df418e8975019007a60872e5788320ff54f`: it found the
  direct call and resolved the inline definition with no missing or extra line.
  Both directions were `position_unverified` because the C++ result contained
  line-level rather than token-level locations.
- Adjudication: ordinary direct inline-function resolution is supported; this
  is the clean control for the following macro-expansion case.

### cpp-parity-function-like-macro-expanded-call

- Source: `fixtures/cpp/lsp-parity/include/parity.h` and `src/main.cpp`.
- Authored definition: the inline `expanded_label` function body.
- Required usage: the file-backed `expanded_label` argument supplied to
  `PARITY_CALL`.
- Excluded token: the macro parameter name `function_name`, which is not an
  occurrence of the concrete `expanded_label` symbol.
- Ground-truth decision: **correct after making definition navigation
  explicit**
- Operation decision: **definition** from the macro-argument token to the
  inline `expanded_label` body.
- Reviewer rationale: preprocessing substitutes the visible macro-argument
  token into the call expression. That argument is the only stable authored
  source position for the call and does not require a synthetic document.
  Clangd is used as the mature-LSP corroborating baseline for preservation of
  this source-to-expansion identity.
- Outcome revealed after review: Apple clangd 21.0 passed exactly, preserving
  the macro-argument token as the function reference and navigating from it to
  the inline definition.
- Bifrost outcome at `a84d6df418e8975019007a60872e5788320ff54f`: forward
  analysis found the macro-argument usage at line precision, but definition
  navigation from the same token returned `no_definition` with
  `no_indexed_definition`. The expected-failure reason was narrowed after the
  run to describe this reverse-only semantic gap.
- Issue candidate: preserve the macro argument's expanded function identity in
  location-to-definition navigation, using the visible argument token as the
  source anchor.

### cpp-parity-compile-commands-unsupported

- Source: `fixtures/cpp/lsp-parity/include/parity.h` and `src/main.cpp`.
- Authored declaration: `configured_only` guarded by
  `ENABLE_PARITY_FEATURE`.
- Required configured usage: the guarded `parity::configured_only()` call.
- Ground-truth decision: **retain as an opt-in configured regression, not an
  empty cross-tool case**
- Operation decision: **declaration** from the guarded call because the fixture
  provides no function body.
- Reviewer rationale: the original empty expectation passed clangd vacuously
  while the feature define was absent. A meaningful regression must activate
  both visible source tokens using deliberate compile flags. Default cross-tool
  scoring still excludes the case until equivalent configuration is supplied
  per runner; `clangd-configured.json` enables the focused regression run.
- Outcome after correction: Apple clangd 21.0 passed the configured usage and
  declaration lookup exactly with `clangd-configured.json` and
  `--include-unsupported`. The retained workspace contained
  `-DENABLE_PARITY_FEATURE` in `compile_flags.txt` and no
  `compile_commands.json` that could override it.
- Negative control: the ordinary `clangd.json` profile, run against the same
  expected case with `--include-unsupported`, missed the guarded call and
  returned no declaration target. This proves the positive result depends on
  deliberate configuration rather than inactive-code text matching.
- Default comparison behavior: Bifrost without `--include-unsupported`
  reported the case unsupported and did not score it, preserving the intended
  separation between an LSP regression probe and cross-tool benchmark totals.

The first independent human review of every case currently in
`cpp-lsp-parity.yaml` is complete. Its document metadata remains
`legacy_unattributed` until a second independent reviewer completes the
promotion requirement; this log preserves the first review meanwhile.
