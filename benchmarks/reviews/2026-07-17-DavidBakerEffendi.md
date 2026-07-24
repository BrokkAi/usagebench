# Ground-truth review: DavidBakerEffendi

- Reviewer: `DavidBakerEffendi` (GitHub username)
- Started: 2026-07-17
- Completed: 2026-07-24
- Review stream: first independent human review
- Coverage: all 158 cases in the 35 current benchmark documents
- Corpus provenance: cases and expected locations were generated agentically
  from upstream LSP tests, existing analyzer tests, and observed edge cases.
- Procedure: inspect fixture source and the authored contract before revealing
  analyzer outcomes; then adjudicate any contract or adapter mismatch.
- Evidence standard: a mature, widely used language server is comparison
  evidence rather than automatic ground truth, but contradicting it requires
  stronger support than intuition alone. Recheck the language's declaration,
  definition, and usage semantics and seek a minimal reproduction or
  corroborating evidence before preserving the disagreement.

Every current document has completed this first review. Document-level
`groundTruth` metadata remains `legacy_unattributed` by design: a second
independent human review, preregistered selection, and an immutable freeze ID
are still required before promotion to the evaluation partition.

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

## csharp-baseline.yaml

### csharp-class-reference

- Source: `fixtures/csharp/baseline/src/Service.cs` and `src/Consumer.cs`.
- Authored declaration: `Service` in `public class Service`.
- Required usages: the explicit local type and the `Service` type named by
  `new Service(repository)`.
- Excluded location: the constructor-definition token in `public
  Service(Repository repository)`.
- Ground-truth decision: **correct after adding the construction type usage**
- Operation decision: **definition** from the explicit local type to the class
  body.
- Reviewer rationale: as in the reviewed Java case, the construction token has
  two compatible semantic roles: it names the class type and invokes the
  selected constructor. It therefore participates in both the class-reference
  and constructor-call cases, while the constructor definition remains a
  definition rather than an ordinary class usage.
- Outcome revealed after review: the official Roslyn language server from the
  macOS ARM64 C# extension `v2.140.9` returned both required class references
  and navigated from the explicit local type to the class body. The original
  one-location contract consequently reported the construction token as a
  proven extra before this correction.
- Harness observation: Roslyn's MSBuild host requires a local named pipe. When
  sandboxing denied that pipe, project loading failed and Roslyn fell back to
  separate `Miscellaneous Files` contexts, but the runner reported ordinary
  missing-reference and no-definition failures rather than a setup error. The
  valid result above came from a run in which the MSBuild host could create its
  pipe. A future runner guard should reject miscellaneous-file contexts for
  project-backed cases or otherwise surface project-load failure explicitly.
- Bifrost outcome at `a84d6df418e8975019007a60872e5788320ff54f`: no semantic
  result was obtained. Both the cached workspace and a fresh temporary work
  directory failed while publishing analyzer epochs with `attempt to write a
  readonly database`; this is recorded as an infrastructure error rather than
  evidence about the two reviewed class usages.

### csharp-constructor-call

- Source: `fixtures/csharp/baseline/src/Service.cs` and `src/Consumer.cs`.
- Authored definition: `Service` in `public Service(Repository repository)`.
- Required usage: `Service` in `new Service(repository)`.
- Excluded locations: the class declaration and the explicit local variable
  type, neither of which invokes the constructor.
- Ground-truth decision: **correct after making definition navigation
  explicit**
- Operation decision: **definition** from the construction token to the
  matching constructor body.
- Reviewer rationale: the construction token has a constructor-invocation
  identity in addition to its class-reference identity from the preceding
  case. The parameter list statically selects the only constructor, while the
  constructor body is a definition rather than an ordinary usage.
- Outcome revealed after review: the official Roslyn language server returned
  only the required construction token and navigated from it to the exact
  constructor definition.

### csharp-method-call

- Source: `fixtures/csharp/baseline/src/Service.cs` and `src/Consumer.cs`.
- Authored definition: the `Service.Execute` method body.
- Required usage: `service.Execute(" Ada ")`.
- Ground-truth decision: **correct after making definition navigation
  explicit**
- Operation decision: **definition** from the call to the `Execute` body.
- Reviewer rationale: the receiver has the static type `Service`, and
  `Execute` is non-virtual because C# instance methods are non-virtual unless
  declared otherwise. The call therefore has one exact statically resolved
  method identity with no dynamic-dispatch ambiguity.
- Outcome revealed after review: the official Roslyn language server returned
  only the required `Execute` call and navigated from it to the exact method
  definition.

### csharp-repository-method-call

- Source: `fixtures/csharp/baseline/src/Service.cs`.
- Authored definition: the `Repository.Save` method body.
- Required usage: `repository.Save(name)` in `Service.Execute`.
- Ground-truth decision: **correct after making definition navigation
  explicit**
- Operation decision: **definition** from the call to the `Save` body.
- Reviewer rationale: the field receiver has the static type `Repository`, and
  `Save` is non-virtual. The call therefore has one exact statically resolved
  method identity with no dynamic-dispatch ambiguity.
- Outcome revealed after review: the official Roslyn language server returned
  only the required `Save` call and navigated from it to the exact method
  definition.

### csharp-property-access

- Source: `fixtures/csharp/baseline/src/Service.cs` and `src/Consumer.cs`.
- Authored property: `Last` in `public string Last { get; private set; }`.
- Required usages: the setter access in `Last = value.Trim()`, the getter
  access in `return Last`, and the qualified getter access in
  `repository.Last`.
- Excluded location: the property declaration token itself.
- Ground-truth decision: **correct after making definition navigation
  explicit**
- Operation decision: **definition** from `repository.Last` to the
  auto-property token.
- Reviewer rationale: the benchmark operates at property-symbol granularity,
  so getter reads and the setter write all reference `Repository.Last`. The
  auto-property has no separate authored accessor body; ordinary source
  definition navigation should therefore land on its declaration token.
- Outcome revealed after review: the official Roslyn language server returned
  exactly the setter and two getter usages. Definition navigation selected the
  four-character `Last` identifier in the auto-property declaration, not an
  accessor keyword or a synthetic backing-field location.

### csharp-constant-access

- Source: `fixtures/csharp/baseline/src/Service.cs` and `src/Consumer.cs`.
- Authored definition: `Prefix` in `public const string Prefix = "job"`.
- Required usages: `Defaults.Prefix` in `Service.Execute` and in
  `Consumer.Run`.
- Excluded location: the defining const declarator token.
- Ground-truth decision: **correct after making definition navigation
  explicit**
- Operation decision: **definition** from a qualified read to the `Prefix`
  identifier.
- Reviewer rationale: the initialized const declarator both declares and fully
  defines the C# constant. There is no separate authored body or external
  definition, so ordinary definition navigation should return that identifier
  while the two reads remain the complete usage set.
- Outcome revealed after review: the official Roslyn language server returned
  exactly the two qualified reads and navigated from the Consumer read to the
  six-character `Prefix` identifier in the const definition.

The first independent human review of every case currently in
`csharp-baseline.yaml` is complete. Its document metadata remains
`legacy_unattributed` until a second independent reviewer completes the
promotion requirement; this log preserves the first review meanwhile.

## csharp-precision.yaml

### csharp-attribute-shorthand-reference

- Source: `fixtures/csharp/precision/Precision.cs`.
- Authored definition: the class `TrackedAttribute`.
- Required usage: the shorter `Tracked` token in `[Tracked]`.
- Excluded location: the class definition token.
- Ground-truth decision: **correct, with reverse definition navigation added**
- Operation decision: **definition** from the seven-character `Tracked` token
  to the sixteen-character `TrackedAttribute` identifier.
- Reviewer rationale: C# attribute binding permits omission of the
  conventional `Attribute` suffix. Only `TrackedAttribute` is available here,
  so the shorter source token has one exact class identity. The benchmark must
  preserve the different authored token ranges rather than normalize their
  display text.
- Outcome revealed after review: the official Roslyn language server returned
  only the seven-character shorthand usage and navigated from it to the exact
  sixteen-character class-definition token.

### csharp-generic-extension-call

- Source: `fixtures/csharp/precision/Precision.cs`.
- Authored definition: the generic extension method `Extensions.Echo<T>`.
- Required usage: `Echo()` on the constructed `Registered` receiver.
- Excluded location: the method definition token.
- Ground-truth decision: **correct, with reverse definition navigation added**
- Operation decision: **definition** from the extension-call token to the
  generic method definition.
- Reviewer rationale: extension-method binding resolves the call to the static
  generic method with `T` inferred as `Registered`. There are no competing
  overloads, and the constructed generic method has no separate authored body,
  so the call retains the source definition's method identity.
- Outcome revealed after review: the official Roslyn language server returned
  only the `Echo()` extension call and navigated from it to the exact generic
  method-definition token.

### csharp-static-qualified-method-call

- Source: `fixtures/csharp/precision/Precision.cs`.
- Authored definition: the static method `Labels.Create`.
- Required usage: `Labels.Create()` in `Consumer.Label`.
- Excluded location: the method definition token.
- Ground-truth decision: **correct, with reverse definition navigation added**
- Operation decision: **definition** from the call to the expression-bodied
  method definition.
- Reviewer rationale: the class qualifier and exact method name make this a
  direct, unambiguous static call. The neighboring consumer method `Label`
  shares neither the symbol identity nor the complete token spelling and must
  not be conflated with `Labels.Create`.
- Outcome revealed after review: the official Roslyn language server returned
  only the class-qualified `Create()` call and navigated from it to the exact
  expression-bodied method definition.

The first independent human review of every case currently in
`csharp-precision.yaml` is complete. Its document metadata remains
`legacy_unattributed` until a second independent reviewer completes the
promotion requirement; this log preserves the first review meanwhile.

## csharp-lsp-parity.yaml

### csharp-parity-namespace-alias-constructor

- Source: `fixtures/csharp/lsp-parity/src/Handlers.cs` and `src/Consumers.cs`.
- Authored definition: the class `ConsoleHandler`.
- Required usages: `WorkerAlias` in the aliased construction and the three
  directly spelled `ConsoleHandler` construction tokens across `Consumers.cs`
  and `Polymorphism.cs`.
- Optional binding usage: the `ConsoleHandler` token on the alias directive's
  right-hand side, allowed by `bindings_optional`.
- Ground-truth decision: **correct after separating alias definition from type
  definition**
- Operation decisions: **definition** from the construction token to the
  `WorkerAlias` binding; **type definition** from the same token to the
  underlying `ConsoleHandler` class.
- Reviewer rationale: the aliased construction is a transitive usage of the
  underlying class, but ordinary definition navigation preserves the spelled
  source alias. The distinct type-definition operation exposes the canonical
  underlying type. This matches the reviewed C++ alias split despite the
  languages' different alias syntax.
- Roslyn tie-breaker: the official language server returned both required
  construction usages plus the optional alias-RHS binding. Definition
  navigation landed on the `WorkerAlias` binding, while a focused
  type-definition probe landed on the `ConsoleHandler` class.
- Expanded-fixture outcome: after adding the two new directly spelled
  constructions in `Polymorphism.cs`, Roslyn returned all four required class
  usages plus the optional alias-RHS binding and passed both navigation
  operations exactly.

### csharp-parity-interface-receiver-method-call

- Source: `fixtures/csharp/lsp-parity/src/Handlers.cs` and `src/Consumers.cs`.
- Authored declaration: `Handle` in the `IHandler` interface.
- Required usages: both `handler.Handle("Ada")` calls through `IHandler`-typed
  locals, including the new call whose factory can return either of two
  implementations.
- Excluded locations: the concrete `ConsoleHandler.Handle` definition and
  `concrete.Handle("Ben")` through a concrete receiver.
- Ground-truth decision: **correct after making declaration navigation
  explicit**
- Operation decision: **declaration** from the interface-typed call because
  the interface member has no body.
- Reviewer rationale: callers receive only the declared `IHandler` contract;
  the factory's current implementation is not a sound basis for assigning the
  call to `ConsoleHandler.Handle`. Static analysis should preserve the
  interface member identity rather than guess a runtime implementation.
- Type-lookup decision: the `handler` local in `PolymorphismConsumer` has the
  static type `IHandler` despite its two possible runtime implementations.
- Coverage expansion: `BufferHandler` and a runtime-dependent factory branch
  were added during review. The original call remains a one-implementation
  control; the new call has multiple possible runtime implementations but an
  unambiguous static identity as `IHandler.Handle`, paired with exact direct
  calls to both implementations.
- Outcome revealed after review: after the expansion, Roslyn returned both
  required interface-typed calls and the exact `IHandler` type definition, but
  also cascaded the interface reference query to all three concrete calls: two
  on `ConsoleHandler` and one on `BufferHandler`. Those calls statically bind
  to their respective concrete methods and remain false positives under this
  benchmark's symbol-identity contract. Roslyn does not advertise
  `textDocument/declaration`, so the reviewed reverse operation was separately
  reported unsupported.

### csharp-parity-concrete-implementation-method-call

- Source: `fixtures/csharp/lsp-parity/src/Handlers.cs`, `src/Consumers.cs`, and
  `src/Polymorphism.cs`.
- Authored definition: the concrete `ConsoleHandler.Handle` body.
- Required usages: `concrete.Handle("Ben")` and `console.Handle("Ben")` through
  `ConsoleHandler`-typed locals.
- Excluded locations: both interface-typed calls and the direct
  `BufferHandler.Handle` call.
- Ground-truth decision: **correct after adding the second concrete control**
- Operation decision: **definition** from each concrete call to the
  `ConsoleHandler.Handle` body.
- Reviewer rationale: both receivers have the exact static type
  `ConsoleHandler`, so both invocations bind to its concrete method. The
  interface contract and sibling implementation remain distinct symbol
  identities.
- Outcome revealed after review: Roslyn returned both required concrete calls
  and navigated each to the exact `ConsoleHandler.Handle` definition, but also
  cascaded the reference query upward to both interface-typed calls. Those two
  extras are retained as false-positive family noise under the reviewed exact
  symbol-identity contract.

### csharp-parity-buffer-implementation-method-call

- Source: `fixtures/csharp/lsp-parity/src/Polymorphism.cs`.
- Authored definition: the concrete `BufferHandler.Handle` body.
- Required usage: `buffer.Handle("Cal")` through a `BufferHandler`-typed local.
- Excluded locations: both interface-typed calls and both direct
  `ConsoleHandler` calls.
- Ground-truth decision: **correct as the second concrete implementation
  control**
- Operation decision: **definition** from the concrete call to the
  `BufferHandler.Handle` body.
- Reviewer rationale: the receiver's static type is exactly `BufferHandler`,
  so the invocation binds to that concrete method. The interface and sibling
  implementation remain separate symbol identities despite their
  implementation relationship.
- Outcome revealed after review: Roslyn returned the required BufferHandler
  call and exact definition target but also cascaded the reference query upward
  to both interface-typed calls. The two extras are retained as the same
  family-oriented editor-policy expansion, not allowed usages.

### csharp-parity-extension-method-call

- Source: `fixtures/csharp/lsp-parity/src/Handlers.cs` and `src/Consumers.cs`.
- Authored definition: the static extension method `HandlerExtensions.Tag`.
- Required usages: `Name.Tag()` and `handler.Handle("Ada").Tag()`.
- Excluded location: the extension-method definition token.
- Ground-truth decision: **correct after adding definition navigation from
  both calls**
- Operation decision: **definition** from either extension call to the static
  method body.
- Reviewer rationale: both receiver expressions have the static type `string`,
  so extension-method binding selects `HandlerExtensions.Tag` exactly. The
  interface dispatch that produces one receiver value does not make the
  subsequent extension call ambiguous.
- Outcome revealed after review: Roslyn returned exactly both `Tag()` calls and
  navigated each to the precise `HandlerExtensions.Tag` definition token.

### csharp-parity-partial-property-access

- Source: `fixtures/csharp/lsp-parity/src/Handlers.cs` and `src/Consumers.cs`.
- Authored property: `EventRecord.Name` in the `Handlers.cs` partial part.
- Required usages: the constructor assignment, the unqualified read from the
  other partial part, and the external `record.Name` read.
- Excluded location: the property definition token.
- Ground-truth decision: **correct after adding definition navigation from all
  three usages**
- Operation decision: **definition** from each access to the `Name` property
  identifier.
- Reviewer rationale: the constructor may initialize the getter-only
  auto-property, and both partial class declarations form one `EventRecord`
  type. All three source tokens therefore share the same property identity;
  the cross-file unqualified read is the key regression and the constructor
  assignment provides a same-file control.
- Outcome revealed after review: Roslyn returned exactly all three property
  usages and navigated each to the precise `Name` identifier in `Handlers.cs`.

### csharp-source-generator-partial-method-call

- Source: `fixtures/csharp/source-generator/`.
- Environment decision: move the source-generator scenario out of the shared
  parity project and make it a real two-project compiler setup. The consumer
  project builds the checked-in generator project as an analyzer; the
  generator emits the implementation half of `GeneratedName` for the authored
  partial declaration.
- Required usage: the authored `GeneratedName()` call in `Read`.
- Ground-truth decision: **correct and scoreable for forward references**
- Reviewer rationale: the call is a stable file-backed usage of the authored
  partial method declaration. The generated implementation is the method's
  definition counterpart, not another ordinary usage.
- Methodology boundary: navigation to the implementation in Roslyn's virtual
  generated document remains unscored until UsageBench can express a stable
  generated-source URI. This does not prevent the authored declaration-to-call
  relationship from being a normal planned case now.
- Build verification: a clean temporary fixture copy builds with .NET SDK
  8.0.418 with zero warnings and zero errors. This removes the `CS8795`
  compile error that previously affected every case sharing the C# parity
  fixture.
- Outcome revealed after review: the official language server loaded the
  custom project outside the restricted sandbox and returned exactly the
  authored `GeneratedName()` call from a reference query on the partial
  declaration.
- Bifrost outcome: Bifrost 0.8.5 at
  `a84d6df418e8975019007a60872e5788320ff54f` returned no usage for the same
  declaration. The planned case therefore records a concrete file-backed
  parity gap without requiring Bifrost to materialize or navigate Roslyn's
  virtual generated document.

## go-baseline.yaml

### go-package-function-call

- Source: `fixtures/go/baseline/src/example/service.go` and
  `src/example/service_test.go`.
- Authored definition: the package-level function `NewService`.
- Required usage: `NewService(repository)` in `ExampleService`.
- Excluded locations: the function definition token and `Service` in the
  returned composite literal, which is a separate type usage.
- Ground-truth decision: **correct**
- Operation decision: **definition** from the call to the package-level
  function body.
- Reviewer rationale: Go has no constructor declaration here; `NewService` is
  an ordinary package-level function with one exact call. Its body makes the
  authored target an executable definition rather than a declaration-only
  contract.
- Outcome revealed after review: gopls 0.23.0 returned exactly the one
  `NewService(repository)` usage and navigated from it to the precise
  `NewService` definition token.

### go-value-receiver-method-call

- Source: `fixtures/go/baseline/src/example/service.go` and
  `src/example/service_test.go`.
- Authored definition: `Execute` on the `Service` value receiver.
- Required usage: `service.Execute("Ada")` through a `Service` value.
- Excluded locations: the method definition token and the `Service` receiver
  type, which is a separate type usage.
- Ground-truth decision: **correct**
- Operation decision: **definition** from the call to the value-receiver
  method body.
- Reviewer rationale: the receiver has the exact static type `Service` and
  there is no competing method or dispatch ambiguity. The method has an
  executable body, so definition navigation is the appropriate operation.
- Outcome revealed after review: gopls 0.23.0 returned exactly the one
  `service.Execute("Ada")` usage and navigated from it to the precise
  `Service.Execute` definition token.

### go-pointer-receiver-method-call

- Source: `fixtures/go/baseline/src/example/service.go` and
  `src/example/service_test.go`.
- Authored definition: `Save` on the `*MemoryRepository` pointer receiver.
- Required usage: `repository.Save("Grace")` through an exact
  `*MemoryRepository` local.
- Unproven usage: `s.repository.Save(name)` through the `Repository` interface.
- Ground-truth decision: **correct with the interface call retained only as an
  unproven implementation candidate**
- Operation decision: **definition** from the direct concrete call to the
  pointer-receiver method body.
- Reviewer rationale: the direct receiver has exact concrete identity. The
  interface-typed call binds statically to `Repository.Save` and may dispatch
  to another implementation, so it cannot be a proven concrete usage; it is
  nevertheless a sound conservative candidate for the implementation family.
- gopls tie-breaker: an exact-only concrete probe failed because gopls 0.23.0
  returned both calls. The original two-tier contract passed. A reciprocal
  interface probe also passed: gopls treated the interface call as proven and
  the direct concrete call as an unproven family candidate.
- Navigation evidence: the direct call navigated to
  `(*MemoryRepository).Save`, while the interface call navigated to the
  `Repository.Save` declaration. gopls therefore expands reference queries
  across the method family while preserving distinct static navigation
  identities.

### go-struct-field-access

- Source: `fixtures/go/baseline/src/example/service.go` and
  `src/example/service_test.go`.
- Authored declaration: `Last` in the `MemoryRepository` struct.
- Required usages: the `m.Last` write in `Save` and the `repository.Last` read
  in `ExampleService`.
- Excluded location: the field declaration token itself.
- Ground-truth decision: **correct**
- Operation decision: **declaration** from the qualified read to the struct
  field declaration.
- Reviewer rationale: both receivers have exact `MemoryRepository` identity,
  and reads and writes both use the field. The field has no executable body or
  separate authored definition target.
- Outcome revealed after review: gopls 0.23.0 returned exactly the write and
  read usages. It does not advertise `textDocument/declaration`, so the reverse
  operation is correctly reported unsupported rather than falling back to
  `textDocument/definition`.

### go-package-constant-access

- Source: `fixtures/go/baseline/src/example/service.go` and
  `src/example/service_test.go`.
- Authored declaration: the initialized package constant `DefaultPrefix`.
- Required usages: the reads in `Service.Execute` and `ExampleService`.
- Excluded location: the constant declaration token itself.
- Ground-truth decision: **correct after separating package-variable coverage**
- Operation decision: **declaration** from the test read to the initialized
  constant binding.
- Reviewer rationale: both source tokens are reads of the same package
  constant. The initialized binding has no executable body or separate
  authored definition target.
- Outcome revealed after review: gopls 0.23.0 returned exactly both constant
  reads. It does not advertise `textDocument/declaration`, so reverse
  navigation is reported unsupported without falling back to definition.

### go-package-variable-access

- Source: `fixtures/go/baseline/src/example/service.go` and
  `src/example/service_test.go`.
- Authored declaration: the initialized package variable `DefaultRepository`.
- Required usage: the read in `ExampleService`.
- Excluded locations: the variable declaration token and `MemoryRepository` in
  its initializer, which is a separate type/construction usage.
- Ground-truth decision: **added because the former combined case did not
  actually score its advertised package-variable coverage**
- Operation decision: **declaration** from the read to the initialized package
  variable binding.
- Fixture correction: changed the initializer from `MemoryRepository{}` to
  `&MemoryRepository{}`. Only `*MemoryRepository` implements `Repository`
  because `Save` has a pointer receiver; the original fixture failed to
  compile.
- Reviewer rationale: the variable has one exact package-level read and no
  separate executable definition body. The variable and concrete type in its
  initializer retain distinct symbol identities.
- Outcome revealed after review: the corrected fixture passes `go test`.
  gopls 0.23.0 returned exactly the one `DefaultRepository` read and reported
  the separate declaration lookup unsupported, as expected.

The first independent human review of every case currently in
`go-baseline.yaml` is complete. Its document metadata remains
`legacy_unattributed` until a second independent reviewer completes the
promotion requirement; this log preserves the first review meanwhile.

## go-precision.yaml

### go-dot-import-concrete-receiver-call

- Source: `fixtures/go/precision/cmd/app/main.go` and
  `worker/worker.go`.
- Authored definition: `Record` on the concrete `Worker` receiver.
- Required usages: `worker.Record()` and `paired.Record()` through exact
  `Worker` values.
- Unproven usage: `recorder.Record()` through the `Recorder` interface.
- Ground-truth decision: **correct**
- Operation decision: **definition** from both concrete calls to the
  `Worker.Record` method body.
- Reviewer rationale: `NewWorker` returns `Worker`, and tuple assignment does
  not weaken the inferred type of its second binding. The dot import affects
  how package symbols enter scope, not the method identities selected by the
  receivers. The interface call keeps its distinct static identity while
  remaining a conservative implementation-family candidate.
- Outcome revealed after review: the fixture passes `go test`, and gopls
  0.23.0 returned both concrete calls plus the interface-family candidate with
  no extras or omissions. Definition navigation from each concrete call
  reached the exact `Worker.Record` method body.

### go-interface-receiver-method-call

- Source: `fixtures/go/precision/cmd/app/main.go` and
  `worker/worker.go`.
- Authored declaration: the bodyless `Record` member on `Recorder`.
- Required usage: `recorder.Record()` through the `Recorder`-typed local.
- Unproven usages: the two direct `Worker.Record` calls as implementation-family
  candidates.
- Ground-truth decision: **correct after making the reciprocal family
  candidates explicit**
- Operation decision: **declaration** from the interface call to the bodyless
  interface member.
- Reviewer rationale: assigning a `Worker` to the interface local does not
  change the call's static identity, and the local remains assignable to other
  implementations. The concrete calls are not proven interface-member usages,
  but are sound conservative method-family candidates.
- Outcome revealed after review: gopls 0.23.0 returned the interface call plus
  both concrete family candidates with no extras or omissions. It does not
  advertise `textDocument/declaration`, so reverse navigation is reported
  unsupported without falling back to definition.

The first independent human review of every case currently in
`go-precision.yaml` is complete. Its document metadata remains
`legacy_unattributed` until a second independent reviewer completes the
promotion requirement; this log preserves the first review meanwhile.

## go-lsp-parity.yaml

### go-parity-cross-package-import-alias-function-call

- Source: `fixtures/go/lsp-parity/pkg/service/service.go` and
  `cmd/app/main.go`.
- Authored definition: the package-level function `service.NewWorker`.
- Required usage: `NewWorker` in `svc.NewWorker()`.
- Excluded locations: the `svc` package alias and the `Worker`/`AuditLog` type
  tokens in the function body.
- Ground-truth decision: **correct**
- Operation decision: **definition** from the imported selector call to the
  package-level function body.
- Reviewer rationale: Go constructor naming does not create a distinct
  constructor symbol; `NewWorker` is an ordinary function. The import alias
  changes only how the package qualifier is spelled at the call site.
- Outcome revealed after review: the fixture passes `go test`, and gopls
  0.23.0 returned exactly the `svc.NewWorker()` selector usage and navigated
  from it to the precise function definition token.

### go-parity-embedded-promoted-method-call

- Source: `fixtures/go/lsp-parity/pkg/service/service.go` and
  `cmd/app/main.go`.
- Authored definition: `Record` on the embedded `*AuditLog` receiver.
- Required usages: `w.Record("run")` inside `Worker.Run` and
  `worker.Record("start")` in `main`.
- Excluded locations: the method definition token and `AuditLog` in the
  embedded field, which is a separate type/field identity.
- Ground-truth decision: **correct**
- Operation decision: **definition** from both promoted calls to the
  `(*AuditLog).Record` body.
- Reviewer rationale: both receivers have exact `*Worker` type, and the single
  embedded `*AuditLog` creates an unambiguous promotion path. Go does not
  synthesize a separate authored `Worker.Record` declaration or body.
- Outcome revealed after review: gopls 0.23.0 returned exactly both promoted
  calls and navigated from each to the precise `(*AuditLog).Record` definition
  token.

### go-parity-embedded-promoted-field-access

- Source: `fixtures/go/lsp-parity/pkg/service/service.go` and
  `cmd/app/main.go`.
- Authored declaration: `Last` on `AuditLog`.
- Required usages: the direct write and read through `*AuditLog`, plus the
  promoted `worker.Last` read through `*Worker`.
- Excluded locations: the field declaration token and `AuditLog` in the
  embedded field, which is a separate type/field identity.
- Ground-truth decision: **correct**
- Operation decision: **declaration** from the promoted read to `Last` in the
  `AuditLog` struct.
- Reviewer rationale: the single embedded `*AuditLog` creates an unambiguous
  promotion path. Go does not synthesize a separate authored `Worker.Last`
  field, and the field has no executable body or separate definition target.
- Outcome revealed after review: gopls 0.23.0 returned exactly all three field
  usages. It does not advertise `textDocument/declaration`, so the reverse
  operation is reported unsupported without falling back to definition.

### go-parity-build-tag-unsupported

- Source: `fixtures/go/lsp-parity/pkg/service/integration.go`.
- Authored definition: the package-level function `IntegrationOnly`, guarded by
  `//go:build integration`.
- Usage site: none; the current fixture contains no caller under the integration
  build tag.
- Boundary decision: **retain unsupported**.
- Reviewer rationale: an empty reference expectation would not distinguish a
  correctly loaded build-tag configuration from a runner that silently omitted
  the file. The benchmark contract also has no case-level setting for
  `-tags=integration`, so the environment required to score the declaration is
  not currently expressed.
- Promotion requirements: add a caller guarded by the same build tag; add an
  explicit benchmark-level `-tags=integration` setting; require the reference
  and definition-navigation result; and verify that every scored runner loaded
  that build configuration.

The first independent human review of every case currently in
`go-lsp-parity.yaml` is complete. Its document metadata remains
`legacy_unattributed` until a second independent reviewer completes the
promotion requirement; this log preserves the first review meanwhile.

## Go post-classification analyzer comparison

The analyzer comparison was run only after every Go case above had been
classified. Bifrost was built from exact `origin/master` commit
`4b1dd6456fe0d7ee6786ede958240f0deba4fd8f`.

- `go-baseline.yaml`: every declaration-to-usage set matched. The three
  definition lookups passed. The field, constant, and variable cases were
  reported unsupported solely because Bifrost does not expose declaration
  navigation separately from definition navigation.
- `go-lsp-parity.yaml`: the import-alias function and embedded promoted-method
  cases passed exactly. The promoted-field reference set matched, with only
  declaration navigation unsupported. The build-tag case remained unsupported
  by the authored contract.
- `go-dot-import-concrete-receiver-call`: the full two-tier reference family
  matched, but both concrete definition lookups failed. Bifrost incorrectly
  diagnosed the `Record` selector as shadowed by a local Go binding, whereas
  gopls navigated both selectors to `Worker.Record`.
- `go-interface-receiver-method-call`: Bifrost found the required static
  interface call but omitted both conservative concrete implementation-family
  candidates that gopls returns. Declaration navigation was independently
  unsupported.

The two precision failures are retained as candidate Bifrost issue requests;
they do not change the reviewed source-location contract.

## javascript-baseline.yaml

### js-commonjs-exported-member-usage

- Source: `fixtures/javascript/baseline/src/commonjs-request.js` and
  `src/commonjs-consumer.js`.
- Authored declaration: the `accepts` property in `exports.accepts`.
- Required usages: both `request.accepts(...)` call selectors.
- Excluded locations: the `request` local/import binding and the inner
  `accepts` name in `function accepts`, which is a distinct function-expression
  self-binding.
- Ground-truth decision: **correct**.
- Operation decision: **declaration** from the imported member selector to the
  exported property assignment.
- Reviewer rationale: the CommonJS consumer selects the exported property. It
  should not be silently conflated with the function expression's private
  lexical name, even though that function is the value currently assigned to
  the property.
- Syntax verification: both fixture files pass `node --check`. The duplicated
  quote shown during the interactive review was a presentation typo and was
  never present in the checked-in fixture.
- Outcome revealed after review: TypeScript Language Server 5.3.0 returned
  exactly both `request.accepts` usages but does not advertise
  `textDocument/declaration`. A non-scored `textDocument/definition` probe
  navigated to the inner `function accepts` token instead. The server therefore
  exposes the same property-versus-function distinction through its available
  navigation behavior, while lacking the operation required by this contract.

### js-named-export-import-function

- Source: `fixtures/javascript/baseline/src/components.js` and `src/app.js`.
- Authored definition: the exported function declaration `formatName`.
- Required usages: the local call from `Greeter.greet` and the imported call in
  `app.js`.
- Optional binding: the `formatName` named import specifier under
  `bindings_optional`.
- Ground-truth decision: **correct**.
- Operation decision: **definition** from the imported call to the authored
  function body.
- Reviewer rationale: both calls select the same function identity. Exporting
  and importing the function adds a binding edge but does not create a second
  function definition.
- Outcome revealed after review: both ES-module files pass `node --check`, and
  TypeScript Language Server 5.3.0 returned exactly both calls and navigated
  from the imported call to the precise exported function definition token.

### js-class-construction

- Source: `fixtures/javascript/baseline/src/components.js` and `src/app.js`.
- Authored definition: the exported class `Greeter`.
- Required usages: `new Greeter(...)` in `createGreeter` and in `app.js`.
- Optional binding: the `Greeter` named import specifier.
- Ground-truth decision: **correct**.
- Usage-kind decision: **constructor** for both `new` expressions. JavaScript
  has no separately named constructor token at either call site.
- Operation decision: **definition** from the external construction to the
  `Greeter` class definition, not the `constructor` keyword.
- Reviewer rationale: ordinary JavaScript functions may be both callable and
  constructable, but `new` still invokes the distinct construct semantics. A
  `class` such as `Greeter` is constructable and cannot be invoked normally
  without `new`, so these sites are unambiguously constructions while their
  source token still names the class.
- Outcome revealed after review: TypeScript Language Server 5.3.0 returned
  exactly both construction usages. Definition navigation returned both the
  canonical `Greeter` class token and the explicit `constructor` member, so the
  original strict singleton lookup scored `multiple_targets`. After migration
  to cross-kind allowed targets, the result is position-unverified because the
  constructor destination spans its enclosing body rather than the authored
  token. Bifrost 0.8.9 returns the exact canonical target.
- Alternate-target policy: the constructor is a reasonable secondary target
  because control flow from `new Greeter(...)` eventually enters that body.
  The class definition remains canonical because the cursor token names the
  lexical `Greeter` binding. The later TypeScript audit added cross-kind
  `allowedExtraTargets`, so this reviewed constructor alternate is now encoded
  without weakening the canonical class requirement.

### js-method-call

- Source: `fixtures/javascript/baseline/src/components.js` and `src/app.js`.
- Authored definition: the `Greeter.greet` method body.
- Required usages: `greeter.greet(user)` and
  `direct.greet({ name: label })`.
- Excluded locations: the receiver bindings, construction tokens, and
  properties used inside the method.
- Ground-truth decision: **correct**.
- Operation decision: **definition** from either call to the `greet` method
  body.
- Reviewer rationale: `direct` is explicitly constructed as a `Greeter`, while
  `greeter` comes from `createGreeter`, whose direct return expression is
  `new Greeter(...)`. Neither path introduces a competing receiver type, so
  both calls resolve conservatively and unambiguously to the same method.
- Outcome revealed after review: both files pass `node --check`, and TypeScript
  Language Server 5.3.0 returned exactly both calls and navigated from the
  selected call to the precise `greet` method definition token.

### js-class-property-access

- Source: `fixtures/javascript/baseline/src/components.js`.
- Authored declaration: `title` in the constructor assignment's
  `this.title`.
- Required usage: the `this.title` read inside `Greeter.greet`.
- Excluded locations: the bare `title` constructor parameter and its
  right-hand-side read, which share a separate local-binding identity.
- Ground-truth decision: **correct**.
- Operation decision: **declaration** from the property read to the left-hand
  property token in the constructor assignment.
- Reviewer rationale: in this JavaScript source shape the constructor write
  introduces the inferred `Greeter` instance property. Treating that token as
  both the declaration and an ordinary usage would double count it; the read
  in `greet` is the sole use of that property.
- Outcome revealed after review: TypeScript Language Server 5.3.0 returned
  exactly the property read but does not advertise
  `textDocument/declaration`. A non-scored definition probe navigated exactly
  to the same left-hand `this.title` property token.

The first independent human review of every case currently in
`javascript-baseline.yaml` is complete. Its document metadata remains
`legacy_unattributed` until a second independent reviewer completes the
promotion requirement; this log preserves the first review meanwhile.

## javascript-precision.yaml

### js-commonjs-barrel-class-construction

- Source: `fixtures/javascript/precision/src/lib.js`, `src/barrel.js`, and
  `src/app.js`.
- Authored definition: `class Client` in `lib.js`.
- Required usages: `new Client()` in `create`, the `Client` runtime read in
  `module.exports = { Client, create }`, and downstream `new Client()` reached
  through the CommonJS barrel.
- Optional binding: `Client` in the destructuring `require` binding in
  `app.js`.
- Ground-truth decision: **correct**.
- Usage-kind decision: **class** for all three locations. The construction
  tokens also participate in constructor-call semantics, but this query tracks
  class identity and includes an export read that is not a construction.
- Operation decision: **definition** from the downstream construction to
  `class Client`.
- Reviewer rationale: the CommonJS barrel forwards the same runtime export
  object, so it does not create a new class identity. There is no explicit
  constructor body in this fixture, so navigation has no secondary constructor
  implementation target.
- Outcome revealed after review: all three files pass `node --check`.
  TypeScript Language Server 5.3.0 returned the factory construction and
  runtime export read but missed downstream `new Client()` in `app.js` when
  querying references from the class definition. Reverse definition navigation
  from that same downstream token still reached `class Client` exactly. This is
  retained as a directional CommonJS-barrel reference gap, not evidence against
  the reviewed class identity.

### js-commonjs-barrel-member-call

- Source: `fixtures/javascript/precision/src/lib.js`, `src/barrel.js`, and
  `src/app.js`.
- Authored definition: the `Client.request` method body.
- Required usages: `.request()` on the directly constructed imported `Client`
  and on the `Client` returned by `create()` through the barrel.
- Excluded locations: `Client`, `create`, `require`, and the receiver
  expressions, which are separate identities.
- Ground-truth decision: **correct**.
- Operation decision: **definition** from both calls to `Client.request`.
- Reviewer rationale: one receiver is constructed directly from the imported
  class, while the other follows the factory's direct `new Client()` return.
  No mutation or competing `request` implementation makes either path
  ambiguous. Scoring both lookups distinguishes binding propagation from
  factory-return inference.
- Outcome revealed after review: all three files pass `node --check`, and
  TypeScript Language Server 5.3.0 returned exactly both calls and navigated
  from each to the precise `Client.request` definition token.

The first independent human review of every case currently in
`javascript-precision.yaml` is complete. Its document metadata remains
`legacy_unattributed` until a second independent reviewer completes the
promotion requirement; this log preserves the first review meanwhile.

## javascript-lsp-parity.yaml

### js-parity-commonjs-destructured-function-call

- Source: `fixtures/javascript/lsp-parity/src/library.js` and
  `src/consumer.js`.
- Authored definition: the `buildTask` function body.
- Required usages: the right-hand runtime read in
  `exports.buildTask = buildTask` and the downstream `buildTask("direct")`
  call.
- Optional bindings: the left-hand CommonJS export property and the
  destructuring `require` token under `bindings_optional`.
- Ground-truth decision: **correct**.
- Operation decision: **definition** from the downstream call to the function
  body.
- Reviewer rationale: assigning the local function value to the CommonJS
  export and destructuring that exported property do not create a competing
  function implementation. The export-value read and call therefore preserve
  the same function identity.
- Outcome revealed after review: all fixture files pass `node --check`.
  TypeScript Language Server 5.3.0 found the export-value read but missed the
  downstream destructured call when querying references from the function
  definition. Definition navigation from that same missed call reached the
  precise `buildTask` definition token. This is a directional CommonJS binding
  propagation gap, not an ambiguous call target.

### js-parity-object-literal-method-call

- Source: `fixtures/javascript/lsp-parity/src/library.js` and
  `src/consumer.js`.
- Authored definition: the `formatTask` method in the `helpers` object literal.
- Required usages: `helpers.formatTask(this)` inside `Task.finish` and
  `helpers.formatTask(directTask)` in the consumer.
- Optional binding: `helpers` in the destructuring `require`.
- Excluded location: the export-side `helpers` token, which reads the
  containing object variable rather than its `formatTask` member.
- Ground-truth decision: **correct**.
- Operation decision: **definition** from the external call to the
  object-literal method body.
- Reviewer rationale: both receivers identify the same exported `helpers`
  object. No mutation, alternate object, or competing method implementation
  makes either call polymorphic or ambiguous.
- Outcome revealed after review: all fixture files pass `node --check`, and
  TypeScript Language Server 5.3.0 returned exactly both calls and navigated
  from the external call to the precise object-literal method definition.

### js-parity-computed-string-literal-method-call

- Source: `fixtures/javascript/lsp-parity/src/library.js` and
  `src/consumer.js`.
- Authored definition: the `Task.finish` method body.
- Required usages: the direct `constructed.finish()` call and the directly
  computed `constructed["finish"]()` call.
- Ground-truth decision: **added as a planned, position-sound computed-member
  control**.
- Operation decision: **definition** from both visible `finish` tokens to the
  method body.
- Reviewer rationale: the string literal is directly at the member-access site
  and names the selected method, unlike a separate value-flow initializer. The
  exact `Task` receiver and literal property name leave no dispatch ambiguity.
- Outcome revealed after review: TypeScript Language Server 5.3.0 returned both
  the dot and computed-literal calls and navigated each to `Task.finish`
  exactly. Bifrost at commit
  `4b1dd6456fe0d7ee6786ede958240f0deba4fd8f` found and navigated the dot call
  but missed `constructed["finish"]()` and returned no definition from its
  string-literal token.

### js-parity-computed-method-name-not-planned

- Source shape: an immutable `const methodName = "finish"` followed by
  `constructed[methodName]()`.
- Runtime target: unambiguously `Task.finish` in this fixture.
- Boundary decision: **retain not planned after removing the unsound reverse
  lookup**.
- Reviewer rationale: the token at the access site is `methodName`, a variable
  usage, while the `"finish"` initializer is only the value-flow source. Calling
  the initializer an ordinary method usage or navigating it directly to the
  method body conflates two source roles even though constant propagation can
  determine the runtime target.
- Promotion requirement: define a deliberate source-location representation
  for value-derived member selection. The direct string-literal access now has
  its own planned regression, so this boundary isolates only the indirection.

The first independent human review of every case currently in
`javascript-lsp-parity.yaml` is complete. Its document metadata remains
`legacy_unattributed` until a second independent reviewer completes the
promotion requirement; this log preserves the first review meanwhile.

## JavaScript post-classification analyzer comparison

The analyzer comparison was run only after every JavaScript case above had been
classified. TypeScript Language Server used the pinned 5.3.0 profile with
TypeScript 5.9.3. Bifrost was built from exact `origin/master` commit
`4b1dd6456fe0d7ee6786ede958240f0deba4fd8f`.

- `javascript-baseline.yaml`: Bifrost matched every reference set. The three
  definition cases passed; the CommonJS export-property and inferred-property
  cases were unsupported only because Bifrost has no distinct declaration
  operation. TypeScript LS produced the same reference results and declaration
  boundary. Its class-construction definition result included both the
  canonical class token and a reasonable explicit-constructor alternate. The
  later TypeScript audit made that reviewed cross-kind alternate expressible
  and migrated the JavaScript case.
- `js-commonjs-barrel-class-construction`: Bifrost passed the complete contract.
  TypeScript LS missed downstream `new Client()` in its class reference query
  but navigated from that same token to `class Client` exactly.
- `js-commonjs-barrel-member-call`: both analyzers returned both method calls.
  TypeScript LS navigated both exactly. Bifrost navigated the factory-returned
  call but returned no definition for `.request()` immediately following
  `new Client()` through the destructured barrel binding.
- `js-parity-commonjs-destructured-function-call`: Bifrost passed exactly.
  TypeScript LS found the export-value read but omitted the downstream call in
  its reference query, while reverse navigation from that call passed.
- `js-parity-object-literal-method-call`: both analyzers passed exactly.
- `js-parity-computed-string-literal-method-call`: TypeScript LS passed exactly.
  Bifrost found the direct dot call but missed the computed string-literal call
  and could not navigate its literal token to `Task.finish`.
- `js-parity-computed-method-name-not-planned`: remains unscored because neither
  the string initializer nor the access-site variable token is an honest
  ordinary method-usage position. No analyzer result is used to paper over that
  source-model boundary.

Initial Bifrost runs inside the restricted sandbox failed while publishing
analyzer epochs with `attempt to write a readonly database`. A sequential retry
failed identically. The semantic Bifrost results above come from fresh
unsandboxed work directories and contain no runner errors; the contaminated
missing-reference output from the sandboxed attempts was discarded.
## php-baseline.yaml

### php-class-construction

- Source: `fixtures/php/baseline/src/Service.php` and `src/Consumer.php`.
- Queried declaration: `Service` in the class definition.
- Required usage: `Service` in `new Service($repository)`, classified as a
  constructor call while also naming the class.
- Excluded locations: `Repository`, `$repository`, and `$service`, which are
  separate type and binding identities.
- Ground-truth decision: **correct**.
- Operation decision: **definition** from the construction token to the
  canonical `Service` class token.
- Alternate-target policy: `__construct` is the eventual control-flow target
  and would be a reasonable secondary result, but the visible cursor token
  names `Service`, not `__construct`. This follows the same canonical-target
  policy used for JavaScript class construction.
- Outcome revealed after review: Intelephense 1.18.5 returned exactly the
  construction usage. Definition navigation returned both the canonical
  `Service` class token and `__construct`, so the strict singleton lookup
  reported `multiple_targets`. This is the anticipated allowed-alternate
  schema limitation rather than a semantically wrong destination.
- Syntax-check boundary: the machine has no `php` CLI, so `php -l` was
  unavailable. The checked source is valid PHP by inspection and loaded
  successfully in the pinned Intelephense session.

### php-method-call

- Source: `fixtures/php/baseline/src/Service.php` and `src/Consumer.php`.
- Authored definition: the `Service::execute` method body.
- Required usage: `$service->execute(' Ada ')`.
- Excluded locations: `$service`, `Service`, `save`, and `PREFIX`, which are
  separate binding, type, method, and constant identities.
- Ground-truth decision: **correct**.
- Operation decision: **definition** from the call to the `execute` method
  body.
- Reviewer rationale: `$service` is initialized directly from
  `new Service(...)` and is not reassigned. There is no competing receiver type
  or override, so this is an exact concrete dispatch case.
- Outcome revealed after review: Intelephense 1.18.5 returned exactly the one
  call and navigated to the precise `Service::execute` definition token.

### php-repository-method-call

- Source: `fixtures/php/baseline/src/Service.php`.
- Authored definition: the `Repository::save` method body.
- Required usage: `$this->repository->save($name)` in `Service::execute`.
- Excluded locations: the promoted property, constructor parameter, receiver,
  and `Repository` type tokens.
- Ground-truth decision: **correct**.
- Operation decision: **definition** from the call to the `save` method body.
- Reviewer rationale: constructor property promotion declares
  `$this->repository` with the exact final `Repository` type. No interface,
  subclass, or alternate implementation makes dispatch ambiguous.
- Outcome revealed after review: Intelephense 1.18.5 returned exactly the one
  call and navigated to the precise `Repository::save` definition token.

### php-property-access

- Source: `fixtures/php/baseline/src/Service.php` and `src/Consumer.php`.
- Authored declaration: `$last` in `public string $last = ''`.
- Required usages: the `$this->last` write and read in `Repository::save`, plus
  the external `$repository->last` read in `Consumer::run`.
- Excluded location: the initialized property declarator itself.
- Ground-truth decision: **correct**.
- Operation decision: **declaration** from the external read to the property
  declarator.
- Reviewer rationale: all receivers have exact `Repository` identity, and
  reads and writes both use the same property. The initializer does not create
  a separate executable property-definition body or an extra ordinary usage.
- Outcome revealed after review: Intelephense 1.18.5 returned exactly all three
  property usages but does not advertise `textDocument/declaration`. A
  non-scored definition probe reached the same property declarator and ranged
  over the full `$last` token, including the sigil.
- Position-policy follow-up: PHP property declarations spell `$last`, while
  member accesses spell `->last`. The reviewer elected to align with
  Intelephense's complete-token range: the declaration selects `$last`, while
  the syntactically sigil-free member accesses select `last`. Bifrost may need
  a PHP-specific range adjustment if it currently returns only the declaration
  identifier subtoken.

### php-constant-access

- Source: `fixtures/php/baseline/src/Service.php` and `src/Consumer.php`.
- Authored declaration: `PREFIX` in `public const PREFIX = 'job'`.
- Required usages: the `Defaults::PREFIX` reads in `Service::execute` and
  `Consumer::run`.
- Excluded locations: both `Defaults` qualifiers, which are separate class
  usages.
- Ground-truth decision: **correct**.
- Operation decision: **declaration** from the consumer read to the constant
  declarator.
- Reviewer rationale: both source tokens read the same class constant. Its
  initializer supplies a value but not a separate executable definition body.
- Outcome revealed after review: Intelephense 1.18.5 returned exactly both
  constant reads but does not advertise `textDocument/declaration`. A
  non-scored definition probe navigated exactly to the same `PREFIX` token.

The first independent human review of every case currently in
`php-baseline.yaml` is complete. Its document metadata remains
`legacy_unattributed` until a second independent reviewer completes the
promotion requirement; this log preserves the first review meanwhile.

## php-precision.yaml

### php-interface-typed-receiver-call

- Source: `fixtures/php/precision/src/Contracts.php` and `src/Consumer.php`.
- Authored declaration: the bodyless `Notifier::send` interface member.
- Required usage: `$notifier->send(...)` through the `Notifier`-typed
  parameter.
- Removed location: `EmailNotifier::send`, which is an implementation
  definition rather than an ordinary usage site.
- Ground-truth decision: **correct after separating the implementation
  definition from usages**.
- Operation decision: **declaration** from the interface-typed call to
  `Notifier::send`.
- Reviewer rationale: `notify` may receive any `Notifier` implementation. The
  call therefore has exact static interface identity but no soundly determined
  concrete runtime body. Interface-to-implementation relationships belong in a
  distinct operation or benchmark category rather than the usage set.
- Outcome revealed after review: Intelephense 1.18.5 returned exactly the
  interface-typed call and did not include `EmailNotifier::send`. It does not
  advertise `textDocument/declaration`; a non-scored definition probe
  navigated exactly to the bodyless `Notifier::send` token rather than guessing
  the concrete implementation.

### php-function-import-call

- Source: `fixtures/php/precision/src/Support.php` and `src/Consumer.php`.
- Authored definition: the `Precision\format` function body.
- Required usage: the bare `format("hello")` call.
- Optional binding: `format` in `use function Precision\format` under
  `bindings_optional`.
- Ground-truth decision: **correct**.
- Operation decision: **definition** from the call to the function body.
- Reviewer rationale: the use-function import resolves the bare call to the
  single namespaced function without creating a second implementation.
  `send` and namespace components are separate identities.
- Outcome revealed after review: Intelephense 1.18.5 returned exactly the bare
  call and navigated to the precise `Precision\format` definition token. The
  import binding did not affect exact scoring under `bindings_optional`.

### php-static-qualified-method-call

- Source: `fixtures/php/precision/src/Labels.php` and `src/Consumer.php`.
- Authored definition: the static `Labels::create` method body.
- Required usage: `create` in `Labels::create()`.
- Excluded location: the `Labels` qualifier, which is a separate class usage.
- Ground-truth decision: **correct**.
- Operation decision: **definition** from the call to the `create` method body.
- Reviewer rationale: the class is final and the static target is explicit, so
  no receiver flow, subclass, or runtime dispatch makes the call ambiguous.
- Outcome revealed after review: Intelephense 1.18.5 returned exactly the
  static call and navigated to the precise `Labels::create` definition token.

The first independent human review of every case currently in
`php-precision.yaml` is complete. Its document metadata remains
`legacy_unattributed` until a second independent reviewer completes the
promotion requirement; this log preserves the first review meanwhile.

## php-lsp-parity.yaml

### php-parity-use-alias-static-method-call

- Source: `fixtures/php/lsp-parity/src/Service/EmailNotifier.php` and
  `src/Consumer.php`.
- Authored definition: the static `EmailNotifier::create` method body.
- Required usage: `create` in `Mailer::create()`.
- Optional binding and excluded qualifier: `Mailer` in the namespace import and
  call qualifier under `bindings_optional`.
- Ground-truth decision: **correct**.
- Operation decision: **definition** from the alias-qualified call to
  `EmailNotifier::create`.
- Reviewer rationale: the namespace alias changes only the class spelling at
  the call site. The static method target remains unique, with no receiver flow
  or dynamic dispatch.
- Outcome revealed after review: Intelephense 1.18.5 returned exactly the
  alias-qualified call and navigated to the precise
  `EmailNotifier::create` definition token.

### php-parity-trait-method-call

- Source: `fixtures/php/lsp-parity/src/Support/LogsEvents.php`,
  `src/Service/EmailNotifier.php`, and `src/Consumer.php`.
- Authored definition: the `LogsEvents::record` trait method body.
- Required usages: `$this->record(...)` inside `EmailNotifier::notify` and the
  external `$mailer->record(...)` call.
- Excluded location: `use LogsEvents`, which composes the trait but does not
  create another authored method token.
- Ground-truth decision: **correct**.
- Operation decision: **definition** from the external call to the trait
  method body.
- Reviewer rationale: `EmailNotifier` has one composed `record` implementation
  and no class override or competing trait. Both receiver paths therefore
  resolve unambiguously to the authored trait body.
- Outcome revealed after review: Intelephense 1.18.5 returned exactly both
  calls and navigated from the external call to the precise
  `LogsEvents::record` definition token.

### php-parity-interface-method-implementation

- Source: `fixtures/php/lsp-parity/src/Contracts/Notifier.php`,
  `src/Service/EmailNotifier.php`, and `src/Consumer.php`.
- Authored declaration: the bodyless `Notifier::notify` interface member.
- Removed location: `EmailNotifier::notify`, which is an implementation
  definition rather than an ordinary usage of the interface declaration.
- Conservative candidate: `$mailer->notify("hello")` is retained as an
  unproven interface-family usage, not a required usage.
- Ground-truth decision: **correct after separating interface relations from
  ordinary usages and splitting out the concrete call**.
- Operation decision: no ordinary definition/declaration lookup is scored for
  the interface case. Reverse implementation navigation is a distinct
  relationship.
- Reviewer rationale: `Mailer::create()` resolves to
  `EmailNotifier::create(): self`, whose body returns `new self()`. The
  consumer binding therefore has statically exact `EmailNotifier` identity;
  the call is concrete rather than genuinely interface-ambiguous. Keeping it
  as an unproven candidate acknowledges the interface family without treating
  the interface declaration as the exact dispatch target.
- Outcome revealed after review: Intelephense 1.18.5 returned the consumer call
  from the interface declaration but did not return the implementation
  definition. This is accepted through `expectedUnprovenUsages`: it documents
  Intelephense's deliberate interface-family reference expansion without
  reclassifying a statically concrete dispatch as an exact interface usage.

### php-parity-concrete-implementation-method-call

- Source: `fixtures/php/lsp-parity/src/Service/EmailNotifier.php` and
  `src/Consumer.php`.
- Authored definition: the concrete `EmailNotifier::notify` method body.
- Required usage: `$mailer->notify("hello")`.
- Excluded location: the bodyless `Notifier::notify` interface member, which
  is a related declaration rather than a usage of the concrete body.
- Ground-truth decision: **correct**.
- Operation decision: **definition** from the call to the concrete
  `EmailNotifier::notify` body.
- Reviewer rationale: the self-returning factory establishes exact concrete
  receiver identity, and the local binding is not reassigned. No alternate
  implementation or subclass can receive this call in the fixture.
- Outcome revealed after review: Intelephense 1.18.5 returned exactly the
  consumer call from the concrete implementation and navigated from that call
  to the precise `EmailNotifier::notify` method body. Thus its reference query
  exposes the call from both family members, while definition navigation still
  preserves the concrete target.

### php-parity-static-property-access

- Source: `fixtures/php/lsp-parity/src/Service/EmailNotifier.php` and
  `src/Consumer.php`.
- Authored declaration: `$sent` in `public static int $sent = 0`.
- Required usages: the `$sent` write in `self::$sent++` and read in
  `Mailer::$sent`.
- Excluded locations: `self` and `Mailer`, which are type or alias qualifiers
  rather than property usages.
- Ground-truth decision: **correct, with complete PHP property-token ranges**.
- Operation decision: **declaration** from the external read to the static
  property declarator.
- Reviewer rationale: the initializer supplies a value but does not create an
  executable property implementation body. All three source locations spell
  the token as `$sent`, so exact ranges include the sigil. This follows
  Intelephense's whole-token convention and may require a PHP-specific Bifrost
  range adjustment.
- Outcome revealed after review: Intelephense 1.18.5 returned both full `$sent`
  usage ranges. It does not advertise `textDocument/declaration`; an earlier
  definition probe reached the exact full `$sent` declarator.

### php-parity-magic-get-no-ordinary-usage

- Source: `fixtures/php/lsp-parity/src/Service/EmailNotifier.php` and
  `src/Consumer.php`.
- Authored definition: the `EmailNotifier::__get` magic method body.
- Runtime edge: `$mailer->dynamicName` will invoke `__get` because the receiver
  is exactly `EmailNotifier` and no declared `dynamicName` property exists.
- Required ordinary usages: none.
- Removed lookup: definition navigation from the `dynamicName` property token
  to the differently named `__get` method.
- Ground-truth decision: **correct after separating implicit runtime handling
  from ordinary symbol identity**.
- Operation decision: no ordinary definition/declaration lookup is scored.
  A future runtime-handler or call-target operation could represent the sound
  implicit edge separately.
- Reviewer rationale: the runtime target is statically recoverable, but
  `dynamicName` does not name `__get`. Treating it as an ordinary method usage
  would conflate control flow with token-level references and create a
  cross-name navigation exception.
- Outcome revealed after review: Intelephense 1.18.5 returned no references
  for `__get` and no definition from `dynamicName`, matching the ordinary
  symbol-identity contract.

The first independent human review of every case currently in
`php-lsp-parity.yaml` is complete. Its document metadata remains
`legacy_unattributed` until a second independent reviewer completes the
promotion requirement; this log preserves the first review meanwhile.

## PHP Bifrost comparison after human review

Bifrost 0.8.8 was run outside the restricted sandbox from exact source commit
`4b1dd6456fe0d7ee6786ede958240f0deba4fd8f` against all three reviewed PHP
documents.

- `php-baseline.yaml`: the class-construction and two method-call cases passed
  exactly. `php-constant-access` returned the exact reference set but is
  unsupported overall because Bifrost does not expose declaration navigation
  separately. `php-property-access` stopped at `symbol_resolution_failed`
  because the reviewed declaration cursor and display name now cover `$last`.
- `php-precision.yaml`: both definition-navigation cases passed exactly.
  `php-interface-typed-receiver-call` returned the exact interface reference
  set but is unsupported overall at the distinct declaration-navigation
  operation, matching Intelephense's advertised capability boundary.
- `php-lsp-parity.yaml`: the alias, trait, two-tier interface, concrete
  implementation, and magic-method negative-reference cases passed exactly.
  `php-parity-static-property-access` stopped at `symbol_resolution_failed`
  for the reviewed `$sent` declaration token; declaration navigation is also
  unsupported.

Direct CLI probes narrowed the two property failures. Bifrost resolves the
fully qualified declarations and returns all expected usages when the query is
moved from the leading `$` to the identifier subtoken. The instance-property
usage ranges are then exact because `->last` is sigil-free. The static-property
results find both usages but return only `sent`, omitting the syntactic `$` that
Intelephense includes for `self::$sent` and `Mailer::$sent`.

This is therefore not evidence that Bifrost lacks PHP property reference
analysis. It is a concrete PHP location-adapter/token-range gap: by-location
selection should accept a cursor on the full `$property` declaration token,
and static-property results should use the complete `$property` source range.
Distinct declaration navigation remains a separate cross-language capability
request. These are suitable follow-up Bifrost issue requests and do not weaken
the reviewed ground truth.

## python-baseline.yaml

### python-module-import

- Source: `fixtures/python/baseline/src/example/service.py`,
  `src/example/__init__.py`, and `tests/test_service.py`.
- Module definition: the zero-width file-start anchor of `service.py`, because
  Python has no textual `module example.service` declaration.
- Required ordinary usages: none.
- Optional import-path references: `service` in `.service` and
  `example.service`, recorded explicitly as allowed extras under
  `bindings_optional`.
- Ground-truth decision: **correct after explicitly recording both optional
  import references**.
- Operation decision: **definition** from the direct test import's `service`
  token to the `service.py` file anchor.
- Reference-query decision: use the direct test import's `service` token as a
  `referenceProbe` for token-based analyzers while retaining the zero-width
  file anchor as the canonical module definition.
- Reviewer rationale: imports are semantically genuine module references, but
  MCP consumers generally want concrete non-import usages and import results
  can be noisy. Keeping them optional preserves the cross-language policy;
  listing their locations still exposes which analyzers include or omit them.
  Navigation from an import token is independently useful and does not require
  forward reference enumeration to include imports.
- Outcome revealed after review: Pyright 1.1.411 navigated exactly from the
  direct `service` import-path token to the zero-width start of `service.py`.
  Before `referenceProbe` existed, the scored references operation was
  unsupported because the canonical module anchor had no source token. A
  diagnostic request from the direct import token returned the other `service`
  occurrence in `example/__init__.py` exactly, proving package discovery was
  working. The case now invokes that ordinary LSP interaction directly instead
  of penalizing Pyright for the synthetic anchor. In the scored rerun, Pyright
  returned the optional package initializer import and the exact module
  definition target, so the case passed. It did not repeat the probe occurrence
  itself in the reference response; the report preserves that inclusion detail
  without scoring it as a miss. Bifrost may continue using its semantic module
  selector.

### python-function-call-and-reexport

- Source: `fixtures/python/baseline/src/example/service.py`,
  `src/example/__init__.py`, and `tests/test_service.py`.
- Authored definition: the `build_service` function body.
- Required usages: both `build_service()` calls in the test fixture.
- Optional binding and metadata locations: the package re-export import, the
  `__all__` string entry, and the test import binding, all recorded as allowed
  extras under `bindings_optional`.
- Ground-truth decision: **correct with all optional binding/export locations
  made explicit**.
- Operation decision: **definition** from the first call to the authored
  function body.
- Reviewer rationale: both calls execute the same unambiguous function. Import
  and export surfaces are semantically related but noisy for MCP consumers, so
  they remain observable without becoming required concrete usages.
- Outcome revealed after review: Pyright 1.1.411 returned both required calls,
  the package re-export binding, the `__all__` entry, and the test import
  binding. Definition navigation from the first call reached the exact
  `build_service` function token, so the reviewed case passed.

### python-class-instantiation

- Source: `fixtures/python/baseline/src/example/service.py`,
  `src/example/__init__.py`, and `tests/test_service.py`.
- Authored definition: the `Service` class.
- Required usage: `Service` in `Service(Repository())`, classified as a visible
  class-token usage that performs construction.
- Optional binding and metadata locations: the package re-export import,
  `__all__` entry, and test import binding.
- Excluded locations: `Repository` and implicit `__new__`/`__init__` control
  flow, which are separate symbol relationships.
- Ground-truth decision: **correct with optional binding/export locations made
  explicit**.
- Operation decision: **definition** from the construction token to
  `class Service`.
- Reviewer rationale: the source token names the class object. Runtime
  construction may invoke additional machinery, but ordinary definition
  navigation should preserve the visible class identity.
- Outcome revealed after review: Pyright 1.1.411 returned the required
  construction plus all three optional binding/export locations and navigated
  exactly from `Service(...)` to the `class Service` token.

### python-method-call

- Source: `fixtures/python/baseline/src/example/service.py` and
  `tests/test_service.py`.
- Authored definition: the `Service.execute` method body.
- Required usage: `execute` in `service.execute(" Ada ")`.
- Excluded locations: the later `"execute"` string, `method_name`, and
  `getattr`, which are separate token identities.
- Ground-truth decision: **correct**.
- Operation decision: **definition** from the direct call to
  `Service.execute`.
- Reviewer rationale: `build_service()` returns a concrete `Service`, and the
  local is not reassigned. The direct dispatch target is therefore exact; the
  string-mediated path belongs to a separate dynamic-runtime case.
- Outcome revealed after review: Pyright 1.1.411 returned only the required
  direct call and navigated exactly to the `Service.execute` definition. It did
  not conflate the later string-mediated path with ordinary method references.

### python-dynamic-getattr-not-planned

- Source: `fixtures/python/baseline/src/example/service.py` and
  `tests/test_service.py`.
- Authored definition: the `Service.execute` method body.
- Aspirational runtime-target location: the contents of the `"execute"` string
  assigned to the immutable local `method_name`.
- Ground-truth decision: **retain as not planned, not as an ordinary
  reference**.
- Aspirational operation decision: **definition** from the string provenance
  token to `Service.execute`.
- Reviewer rationale: ordinary Python reference semantics should not conflate
  string data with a method-name token. This particular runtime target is
  nevertheless statically recoverable because the receiver is an exact
  `Service`, the local string has one assignment, and the result of
  `getattr(service, method_name)` is called immediately. The string contents
  are the only authored range available to represent that inferred
  relationship, so the case remains useful as an explicitly unscored future
  constant-propagation category.
- Outcome revealed after review: Pyright 1.1.411 did not treat the string as a
  `Service.execute` reference and returned no definition from it. A references
  query for `Service.execute` returned the ordinary direct call from
  `python-method-call` instead. This cleanly separates standard LSP reference
  semantics from the aspirational runtime-target analysis and supports keeping
  the case `notPlanned`.

### python-attribute-access

- Source: `fixtures/python/baseline/src/example/service.py` and
  `tests/test_service.py`.
- Canonical declaration: `self.last = ""` in `Repository.__init__`.
- Required usages: the later `self.last = value` write in `Repository.save`
  and the `repository.last` read in the test.
- Ground-truth decision: **correct**.
- Operation decision: **declaration** from the test read to the first
  assignment.
- Allowed navigation alternate: the later `self.last = value` write may
  accompany the initializer target, but cannot replace it.
- Reviewer rationale: Python creates this instance attribute through
  assignment rather than a separate field declaration. The first assignment
  therefore provides the stable declaration anchor, while the later assignment
  remains a write usage. The test receiver comes directly from
  `Repository()`, so the attribute identity is unambiguous.
- Outcome revealed after review: Pyright 1.1.411 returned both assignment sites
  from both its Declaration and Definition endpoints. Bifrost 0.8.9 at
  `e9cf0ed0` returned only the canonical initializer from its Declaration
  endpoint. Both tools returned exactly the two required references. The
  reviewed `allowedExtraTargets` contract therefore accepts Pyright's
  conservative alternate without making it required, while preserving
  Bifrost's more precise canonical result. No Bifrost issue was filed because
  the current analyzer already exhibits the preferred behavior.

## python-precision.yaml

### python-barrel-inherited-member-call

- Source: `fixtures/python/precision/precision/services.py` and
  `consumer.py`.
- Authored definition: `Base.save`.
- Required usage: `client.save()` where `client` is explicitly annotated and
  initialized as `Child`.
- Excluded usage: `grandchild.save()`, because the fixture was strengthened so
  `Grandchild` overrides the inherited method.
- Ground-truth decision: **correct after adding the distinguishing override**.
- Operation decision: **definition** from `client.save()` to `Base.save`.
- Reviewer rationale: `Child` does not override `save`, so its exact runtime
  target remains `Base.save`. The package-barrel import does not weaken that
  relationship.
- Outcome revealed after review: Pyright 1.1.411 returned only
  `client.save()` as a `Base.save` reference and navigated it exactly to the
  base definition.

### python-barrel-overridden-member-call

- Source: `fixtures/python/precision/precision/services.py` and
  `consumer.py`.
- Authored definition: the new `Grandchild.save` override.
- Required usage: `grandchild.save()` where `grandchild` is explicitly
  annotated and initialized as `Grandchild`.
- Excluded usage: `client.save()`, which still dispatches to `Base.save`.
- Ground-truth decision: **correct**.
- Operation decision: **definition** from `grandchild.save()` to
  `Grandchild.save`.
- Reviewer rationale: the concrete receiver and explicit override make the
  dispatch target exact even though the class is imported through the package
  barrel.
- Outcome revealed after review: Pyright 1.1.411 returned only
  `grandchild.save()` as a `Grandchild.save` reference and navigated it exactly
  to the override definition.

### python-barrel-class-construction

- Source: `fixtures/python/precision/precision/services.py`,
  `precision/__init__.py`, and `consumer.py`.
- Authored definition: `class Child`.
- Required usages: `Child` as the `Grandchild` superclass, `Child` in the
  explicit `client` annotation, and `Child()` construction.
- Optional binding locations: the package re-export and consumer import.
- Ground-truth decision: **correct after adding the missing annotation and
  explicitly recording both optional bindings**.
- Operation decision: **definition** from `Child()` to `class Child`.
- Reviewer rationale: all three required tokens refer concretely to the same
  class. Python construction invokes the visible class object, so ordinary
  navigation preserves the class identity rather than inventing a separate
  constructor declaration.
- Outcome revealed after review: Pyright 1.1.411 returned all three required
  class usages, both optional binding locations, and navigated `Child()`
  exactly to the class declaration.

### python-multilevel-barrel-class-construction

- Source: `fixtures/python/precision/precision/services.py`,
  `precision/__init__.py`, and `consumer.py`.
- Authored definition: `class Grandchild`.
- Required usages: `Grandchild` in the explicit local annotation and
  `Grandchild()` construction.
- Optional binding locations: the package re-export and consumer import.
- Excluded relationships: the `Child` superclass token and
  `grandchild.save()` override call.
- Ground-truth decision: **correct after adding the missing annotation and
  explicitly recording both optional bindings**.
- Operation decision: **definition** from `Grandchild()` to
  `class Grandchild`.
- Reviewer rationale: the annotation and construction refer exactly to the
  concrete multilevel subclass. Its superclass and overridden method remain
  distinct symbol relationships.
- Outcome revealed after review: Pyright 1.1.411 returned both required class
  usages, both optional bindings, and navigated construction exactly to the
  `Grandchild` class declaration.

## python-lsp-parity.yaml

### python-parity-reexported-class-alias-classmethod

- Source: `fixtures/python/lsp-parity/src/shop/models.py`,
  `src/shop/__init__.py`, and `tests/test_models.py`.
- Authored definition: `class User`.
- Required usages: the quoted `"User"` return annotation plus both runtime
  `Account` qualifier tokens.
- Optional binding and metadata locations: `User` and `Account` in the package
  re-export, the `__all__` entry, and the test import binding.
- Ground-truth decision: **correct after adding the quoted annotation,
  explicitly recording the optional alias family, and documenting the required
  LSP interaction**.
- Operation decision: **definition** from runtime `Account` to `class User`.
- Reference-query decision: query both canonical `User` and the authored
  runtime `Account` `referenceProbe`, union the results, and count the probe
  itself as a known occurrence.
- Reviewer rationale: quoted forward annotations are standard Python type
  references. The runtime alias qualifiers name the same class object, while
  imports and export metadata remain optional under `bindings_optional`.
- Outcome revealed after review: Pyright 1.1.411 is occurrence-sensitive across
  this re-export alias. Find References from `User` returned the source import
  and quoted annotation; Find References from `Account` returned the alias
  binding, `__all__`, test import, and other runtime qualifier. Definition from
  `Account` reached `class User` exactly. The documented additive query
  interaction combines the two valid LSP views and passes without weakening
  the required semantic class-reference set. Bifrost may continue querying the
  canonical semantic declaration directly.

### python-parity-classmethod-call

- Source: `fixtures/python/lsp-parity/src/shop/models.py` and
  `tests/test_models.py`.
- Authored definition: `User.guest`.
- Required usage: `guest` in `Account.guest()`.
- Excluded relationships: the `Account` class qualifier, implicit `cls`
  parameter and call, and `@classmethod` decorator.
- Ground-truth decision: **correct**.
- Operation decision: **definition** from the call to `User.guest`.
- Reviewer rationale: the re-exported class alias does not change the
  classmethod's identity, and no other visible `guest` token participates in
  the method relationship.
- Outcome revealed after review: Pyright 1.1.411 returned only the required
  method call and navigated it exactly to `User.guest`.

### python-parity-staticmethod-call

- Source: `fixtures/python/lsp-parity/src/shop/models.py`,
  `src/shop/__init__.py`, and `tests/test_models.py`.
- Authored definition: `User.format_name`.
- Required usage: `format_name` in `Account.format_name("ada")`.
- Ground-truth decision: **correct**.
- Operation decision: **definition** from the call to `User.format_name`.
- Reviewer rationale: `Account` is a direct import alias for `User`, not an
  inherited subtype. The class qualifier, `@staticmethod` decorator, method
  parameter, and `name.title()` call are separate identities.
- Outcome revealed after review: Pyright 1.1.411 returned only the required
  staticmethod call and navigated it exactly to `User.format_name`.

### python-parity-property-getter-access

- Source: `fixtures/python/lsp-parity/src/shop/models.py` and
  `tests/test_models.py`.
- Authored definition: the decorated `User.normalized_name` getter.
- Required usage: `normalized_name` on the inferred `User` instance.
- Ground-truth decision: **correct**.
- Operation decision: **definition** from the property access to the decorated
  getter.
- Reviewer rationale: `Account.guest()` establishes the exact inferred receiver
  type. Property access invokes the getter at runtime while preserving a
  property token identity; the decorator, `self.name`, and `lower()` remain
  separate relationships.
- Outcome revealed after review: Pyright 1.1.411 returned only the required
  property access and navigated it exactly to `User.normalized_name`.

### python-parity-dunder-getattr-not-planned

- Source: `fixtures/python/lsp-parity/src/shop/models.py` and
  `tests/test_models.py`.
- Runtime fallback definition: `DynamicConfig.__getattr__`.
- Aspirational lookup source: the synthesized `theme` attribute token.
- Ground-truth decision: **retain as not planned, not as an ordinary
  reference**.
- Aspirational operation decision: **definition** from `config.theme` to
  `DynamicConfig.__getattr__`.
- Reviewer rationale: `theme` does not share the fallback handler's symbol
  identity, so ordinary reference scoring would be misleading. Runtime
  dispatch is nevertheless exact in this fixture because `config` has concrete
  `DynamicConfig` type and no declared `theme` member exists.
- Outcome revealed after review: Pyright 1.1.411 returned no Definition target
  from `config.theme`. This supports preserving the case as an explicitly
  unscored future dynamic-member navigation category.
## ruby-baseline.yaml

### ruby-require-relative-class-construction

- Source: `fixtures/ruby/baseline/lib/billing/invoice.rb` and
  `app/report.rb`.
- Authored definition: `Billing::Invoice`.
- Required usages: the `Invoice` qualifier in `Billing::Invoice.build`, the
  explicit `Invoice.new` self-construction, the
  `Billing::Invoice::DEFAULT_CURRENCY` qualifier, and the
  `Billing::Invoice.last_build` qualifier.
- Optional result: the class declaration itself, because both checked Ruby
  servers include it despite `includeDeclaration: false`.
- Excluded location: the lowercase filename inside `require_relative`, which
  is not a class token.
- Ground-truth decision: **correct after promoting both namespace/member
  qualifiers to required usages**.
- Operation decision: **definition** from the build qualifier to
  `class Invoice`.
- Reviewer rationale: all four visible `Invoice` tokens name the same exact
  class. `new` is a separate method token, while `Invoice.new` still contains
  an explicit class usage.
- Outcome revealed after review: Ruby LSP 0.26.10 and Solargraph 0.60.2 both
  found all four semantic references without an occurrence probe. Ruby LSP
  selected the full `Billing::Invoice` path for three qualified references but
  returned exact self-construction and Definition ranges. Solargraph returned
  all four reference tokens exactly but selected the enclosing class body for
  Definition. Both also included the class declaration. After treating
  enclosing ranges as `position_unverified` and explicitly allowing declaration
  inclusion, neither server has a semantic failure: Ruby LSP reports only the
  three broad qualifier ranges, while Solargraph reports only its broad
  Definition range.

### ruby-relative-nested-constant

- Source: `fixtures/ruby/baseline/lib/billing/money.rb` and
  `lib/billing/invoice.rb`.
- Authored definition: `Billing::Money::Currency`.
- Required usage: `Currency` in `Money::Currency.new`.
- Optional result: the class declaration itself, because both checked Ruby
  servers include it despite `includeDeclaration: false`.
- Ground-truth decision: **correct**.
- Operation decision: **definition** from the constant reference to
  `class Currency`.
- Reviewer rationale: lexical nesting places `Invoice` inside `Billing`, so
  `Money::Currency` resolves exactly to `Billing::Money::Currency`. `Money`,
  `new`, and the lowercase require filename are separate identities.
- Outcome revealed after review: neither Ruby LSP 0.26.10 nor Solargraph 0.60.2
  needed an occurrence probe. Both returned the semantic reference and class
  declaration. Ruby LSP selected the full `Money::Currency` reference and exact
  Definition token; Solargraph selected the exact reference token and enclosing
  class body for Definition. With declaration inclusion explicitly allowed,
  only the respective broad ranges remain `position_unverified`.

### ruby-superclass-reference

- Source: `fixtures/ruby/baseline/lib/billing/record.rb` and
  `lib/billing/invoice.rb`.
- Authored definition: `Billing::Record`.
- Required usage: `Record` in `class Invoice < Record`.
- Optional result: the class declaration itself.
- Ground-truth decision: **correct**.
- Operation decision: **definition** from the superclass token to
  `class Record`.
- Reviewer rationale: lexical constant lookup resolves the unqualified
  superclass exactly inside `Billing`. The require filename and inherited
  member tokens are separate identities.
- Outcome revealed after review: Ruby LSP 0.26.10 and Solargraph 0.60.2 both
  returned the exact superclass reference plus the declaration, so no probe is
  required. Ruby LSP navigated to the exact class token. Solargraph selected
  the enclosing class body for Definition, leaving only a
  `position_unverified` range difference after declaration inclusion was
  explicitly allowed.

### ruby-include-instance-mixin

- Source: `fixtures/ruby/baseline/lib/billing/auditable.rb`,
  `lib/billing/invoice.rb`, and `app/report.rb`.
- Authored definition: `Billing::Auditable#audit`.
- Required usages: the direct `invoice.audit` call and the literal
  `invoice.public_send(:audit)` runtime-call target.
- Optional result: the method declaration itself.
- Ground-truth decision: **correct, deliberately exceeding ordinary Ruby LSP
  reference scope for the sound reflective call**.
- Operation decision: **definition** from the direct call to
  `Auditable#audit`; no ordinary reverse navigation is required from the symbol
  literal.
- Reviewer rationale: `Invoice` includes `Auditable` without an override, the
  receiver is exact, and `:audit` is a literal passed directly to
  `public_send`. The runtime target is therefore unique without mutable-string
  propagation. `public_send` and the `Auditable` module token remain separate
  identities.
- Outcome revealed after review: Ruby LSP 0.26.10 and Solargraph 0.60.2 both
  return the direct call plus method declaration but omit `:audit`. Querying
  from the symbol does not expose a hidden reference family: Ruby LSP
  associates the position with `public_send`, while Solargraph returns none.
  Ruby LSP navigates the direct call exactly; Solargraph selects the enclosing
  method range. Declaration inclusion is explicitly allowed, leaving the
  reflective literal as a genuine semantic false negative for both LSPs and an
  opportunity for Bifrost to demonstrate stronger sound call analysis.

### ruby-prepend-method-precedence

- Source: `fixtures/ruby/baseline/lib/billing/formatting.rb`,
  `lib/billing/invoice.rb`, and `app/report.rb`.
- Authored definition: `Billing::Formatting#total_label`.
- Required usage: `invoice.total_label`.
- Optional result: the queried `Formatting#total_label` declaration itself.
- Excluded result: the shadowed `Invoice#total_label` implementation.
- Ground-truth decision: **correct**.
- Operation decision: **definition** from the call to the prepended module
  method.
- Reviewer rationale: `prepend` deterministically places `Formatting` before
  `Invoice` in Ruby method lookup order. The class implementation is therefore
  neither an ambiguous alternate target nor a usage of the prepended method.
- Outcome revealed after review: both Ruby LSP 0.26.10 and Solargraph 0.60.2
  return the exact call and queried module declaration. Both navigate the call
  to `Formatting#total_label`; Solargraph uses an enclosing method range. Ruby
  LSP additionally returns the shadowed `Invoice#total_label` declaration,
  while Solargraph does not. After allowing only queried-declaration inclusion,
  the Ruby LSP result retains one genuine method-family extra and Solargraph
  retains only its `position_unverified` Definition range.

### ruby-extend-and-include-method-lookup

- Source: `fixtures/ruby/baseline/lib/billing/findable.rb`,
  `lib/billing/user.rb`, `lib/billing/legacy_user.rb`, and `app/report.rb`.
- Authored definition: `Billing::Findable#find`.
- Required usages: `Billing::User.find(42)` and
  `Billing::LegacyUser.new.find(7)`.
- Optional result: the queried `Findable#find` declaration itself.
- Ground-truth decision: **correct**.
- Operation decision: **definition** from both calls to `Findable#find`.
- Reviewer rationale: `extend Findable` installs the authored body as a
  singleton-class method on `User`, while `include Findable` installs the same
  body as an instance method on `LegacyUser`. Both receivers therefore have the
  same unique authored definition. `Findable` and `new` are separate symbol
  identities.
- Outcome revealed after review: Ruby LSP 0.26.10 and Solargraph 0.60.2 both
  return both exact calls plus the method declaration, so declaration inclusion
  is explicitly allowed. Both navigate both calls to the shared
  `Findable#find` body. Ruby LSP selects the exact method token; Solargraph
  selects the enclosing method range for both, leaving only
  `position_unverified` range differences.

### ruby-class-constant-access

- Source: `fixtures/ruby/baseline/lib/billing/invoice.rb` and
  `app/report.rb`.
- Authored declaration: `DEFAULT_CURRENCY = Money::Currency.new("USD")`.
- Required usage: `DEFAULT_CURRENCY` in
  `Billing::Invoice::DEFAULT_CURRENCY`.
- Optional result: the constant assignment token itself.
- Ground-truth decision: **correct**.
- Operation decision: **declaration** from the qualified read to the
  left-hand constant binding.
- Reviewer rationale: the assignment introduces a constant binding rather than
  an executable body. `Billing`, `Invoice`, `Money`, `Currency`, and `new` are
  separate symbol identities.
- Outcome revealed after review: neither Ruby LSP 0.26.10 nor Solargraph 0.60.2
  advertises `textDocument/declaration`, so the semantic Declaration operation
  is unsupported by both adapters. Both practical Definition probes reach the
  assignment with enclosing ranges. Solargraph finds the read plus assignment
  from the declaration; Ruby LSP finds nothing from the assignment in a cold
  query but finds the qualified read plus assignment through the scored
  read-origin probe after indexing settles. This is occurrence-sensitive
  reference discovery, not evidence against the authored usage. The assignment
  is explicitly allowed as declaration inclusion.

### ruby-top-level-implicit-self-method-call

- Source: `fixtures/ruby/baseline/app/report.rb`.
- Authored definition: top-level `def normalize_total(value)`.
- Required usage: the bare `normalize_total(19)` call inside
  `InvoiceReport#render`.
- Optional result: the queried method declaration itself.
- Ground-truth decision: **correct after reclassifying and renaming the case
  from a function to a method**.
- Operation decision: **definition** from the call to the authored method body.
- Reviewer rationale: a Ruby top-level `def` installs a private instance method
  on `Object`. The bare call has the implicit receiver `self`; while `render`
  executes, that is the `InvoiceReport` instance, whose ancestor chain reaches
  `Object`. The later textual position of the method body does not make the
  target ambiguous.
- Outcome revealed after review: Ruby LSP 0.26.10 and Solargraph 0.60.2 both
  return the exact call plus declaration. Solargraph navigates to the enclosing
  authored method body. Ruby LSP returns no Definition, leaving a genuine
  navigation false negative for the implicit-self Object-ancestor edge.

### ruby-singleton-method-dispatch

- Source: `fixtures/ruby/baseline/lib/billing/user.rb`,
  `lib/billing/invoice.rb`, and `app/report.rb`.
- Authored definition: `Billing::User.build`.
- Required usage: `build` in `Billing::User.build`.
- Optional result: the queried `User.build` declaration itself.
- Excluded locations: the same-spelled `Billing::Invoice.build` call and
  `Invoice.build` declaration.
- Ground-truth decision: **correct**.
- Operation decision: **definition** to `def self.build` on `User`.
- Reviewer rationale: the explicit class receiver makes this dispatch
  unambiguous. `Invoice.build` is a distinct singleton method, while `User`
  tokens and `new` are separate class and method identities.
- Outcome revealed after review: Ruby LSP 0.26.10 and Solargraph 0.60.2 both
  return the exact `User.build` call plus queried declaration and both navigate
  to `User.build`; Solargraph selects the enclosing method body. Ruby LSP also
  returns the distinct `Invoice.build` call and declaration. Solargraph does
  not, confirming that those two locations are method-family noise rather than
  necessary conservatism.

### ruby-dynamic-public-send

- Source: `fixtures/ruby/baseline/app/report.rb` and
  `lib/billing/auditable.rb`.
- Reviewed operation: Definition navigation from the literal `:audit` in
  `invoice.public_send(:audit)` to `Auditable#audit`.
- Ground-truth decision: **retain as navigation-only and not planned**.
- Corpus cleanup: removed the duplicate declaration-to-references contract;
  the planned `ruby-include-instance-mixin` case already requires both the
  direct call and reflective symbol usage.
- Reviewer rationale: the exact receiver and literal method name make the
  reflective reference edge sound, but clicking a symbol literal to request
  ordinary Definition is a stronger, separate editor operation.
- Outcome revealed after review: Ruby LSP 0.26.10 associates the symbol
  position with `public_send`, while Solargraph 0.60.2 returns no target.
  Navigation from the symbol literal therefore remains explicitly aspirational
  and unscored rather than weakening the planned reference case.

### ruby-script-level-constant-access

- Source: `fixtures/ruby/baseline/app/report.rb`.
- Authored declaration: top-level `SCRIPT_LIMIT = 100`.
- Required usage: `SCRIPT_LIMIT` inside `normalize_total`.
- Optional result: the constant assignment token itself.
- Ground-truth decision: **correct**.
- Operation decision: **declaration** from the read to the left-hand constant
  binding.
- Reviewer rationale: the top-level assignment introduces the constant on
  `Object`, and the top-level method's lexical context resolves its bare read
  to that binding. The assignment executes before the method is invoked in the
  intended loaded program.
- Outcome revealed after review: neither Ruby LSP 0.26.10 nor Solargraph 0.60.2
  advertises LSP Declaration. Solargraph returns the exact read plus assignment
  and its practical Definition lookup reaches an enclosing assignment range.
  With the scored read-origin probe and settled indexing, Ruby LSP also returns
  the required read plus assignment. Declaration remains unsupported by both
  adapters rather than becoming a semantic reference failure.

### ruby-instance-field-access

- Source: `fixtures/ruby/baseline/lib/billing/invoice.rb`.
- Authored declaration: `@status = "draft"` in `Invoice#initialize`.
- Required usage: the `@status` read in `Invoice#status`.
- Optional result: the assignment occurrence itself.
- Ground-truth decision: **correct**.
- Operation decision: **declaration** from the read to the assignment token.
- Reviewer rationale: both occurrences have implicit `self` inside `Invoice`
  instance methods and identify the same source-level member slot. Different
  runtime instances do not create distinct static field identities.
- Outcome revealed after review: neither Ruby LSP 0.26.10 nor Solargraph 0.60.2
  advertises LSP Declaration. Solargraph returns both exact occurrences and
  its practical Definition lookup selects the full assignment. A cold Ruby LSP
  query from the assignment exposed `NonExistingNamespaceError` for
  `Billing::Invoice`; the read origin returned both exact occurrences. With the
  scored read-origin probe and settled indexing, both origins complete and the
  reference family passes. The cold error remains initialization-sensitivity
  evidence, not evidence against the authored field identity.

### ruby-class-variable-access

- Source: `fixtures/ruby/baseline/lib/billing/invoice.rb`.
- Authored declaration: `@@sequence = 0` in `Invoice`.
- Required usage: the single `@@sequence` occurrence in `@@sequence += 1`.
- Optional result: the initializer occurrence itself.
- Ground-truth decision: **correct**.
- Operation decision: **declaration** to the initial assignment.
- Reviewer rationale: the compound assignment is one source occurrence that
  reads and writes the same class-hierarchy slot. It requires a pre-existing
  value and therefore cannot supply the declaration for its own read. There is
  no competing declaration or subclass in the fixture.
- Outcome revealed after review: neither Ruby LSP 0.26.10 nor Solargraph 0.60.2
  advertises LSP Declaration. Solargraph finds both exact occurrences when
  queried from the mutation, while practical Definition returns both the
  initializer and mutation. Ruby LSP finds no usage from the initializer; from
  the mutation it falls back to the enclosing `build` method family and returns
  unrelated `build` locations. Those results remain genuine reference noise,
  not allowed class-variable targets.

### ruby-singleton-field-access

- Source: `fixtures/ruby/baseline/lib/billing/invoice.rb` and
  `app/report.rb`.
- Authored declaration: class-body `@last_build = nil`.
- Required usages: the write in `Invoice.build` and read in
  `Invoice.last_build`.
- Optional result: the class-body initializer itself.
- Ground-truth decision: **correct under the fixture's exact receivers**.
- Operation decision: **declaration** from the read to the initial class-body
  assignment.
- Reviewer rationale: the class body and both singleton methods execute with
  `self` equal to the `Invoice` class object for the shown calls, so all three
  occurrences identify the same singleton-state slot. `@status` belongs to
  `Invoice` instances and is excluded. A future inherited call on a subclass
  could use subclass state, but no such receiver exists in this closed fixture.
- Outcome revealed after review: neither Ruby LSP 0.26.10 nor Solargraph 0.60.2
  advertises LSP Declaration. Solargraph finds all three exact occurrences from
  the read and practical Definition selects the full initializer assignment;
  it finds no usages from the initializer origin. Cold Ruby LSP queries exposed
  `NonExistingNamespaceError` for `Billing::Invoice::<Class:Invoice>`, but the
  scored read-origin probe returns all three occurrences once indexing settles.
  The reference family therefore passes while Declaration remains unsupported;
  the cold failure records initialization sensitivity rather than contradicting
  the authored singleton-field relation.

### ruby-factory-return-member-call

- Source: `fixtures/ruby/precision/lib/precision/base.rb`,
  `lib/precision/factory.rb`, and `app/run.rb`.
- Authored definition: `Precision::Base#execute`.
- Required usages: `service.execute` after `Precision.build` and
  `second.execute` after `Precision::Base.build`.
- Optional result: the queried method declaration itself.
- Ground-truth decision: **correct**.
- Operation decision: **definition** from both calls to `Base#execute`.
- Reviewer rationale: `Precision.build` explicitly returns `Base.new`;
  `Base.build` executes with `self == Base` and its bare `new` also returns a
  `Base`. Both locals are assigned once with no competing writes, so the return
  flow is exact in the closed fixture.
- Outcome revealed after review: Ruby LSP 0.26.10 and Solargraph 0.60.2 both
  find the two exact calls plus declaration. Solargraph navigates both calls to
  an enclosing `Base#execute` range. With settled full-fixture indexing, Ruby
  LSP returns the correct `Base#execute` target plus numerous unrelated RubyGems
  `execute` methods. The expected target is present, but the extra method-family
  targets remain genuine navigation noise.

### ruby-lexical-factory-constant

- Source: `fixtures/ruby/precision/lib/precision/base.rb`,
  `lib/precision/factory.rb`, and `app/run.rb`.
- Authored definition: `Precision::Base`.
- Required usages: lexical `Base` in `Precision.build` and qualified `Base` in
  `Precision::Base.build`.
- Optional result: the queried class declaration itself.
- Ground-truth decision: **correct**.
- Operation decision: **definition** from both usages to `class Base`.
- Reviewer rationale: the unqualified token is lexically inside
  `module Precision`; the qualified token names the same class explicitly.
  `require_relative "base"`, `new`, and `build` are separate binding or method
  identities.
- Outcome revealed after review: Solargraph 0.60.2 returns both exact usages
  plus declaration and navigates both to the enclosing `Precision::Base` class
  body. In an isolated cold run, Ruby LSP 0.26.10 omitted the qualified usage,
  sent lexical `Base` to the standard-library `Random::Base` RBS entry, and
  returned no Definition for `Precision::Base`. In the complete Ruby run,
  after the preceding factory-return case indexed the same fixture, it returned
  both usages and both correct definitions, with only a broad qualified
  reference range. The scored qualified-use probe and five-second profile
  settle make the complete result reproducible. The cold result is recorded as
  initialization/order sensitivity rather than a stable semantic failure.

### ruby-parity-autoload-constant-definition

- Source: `fixtures/ruby/lsp-parity/lib/shop/product.rb`,
  `lib/shop/discount.rb`, and `app/catalog.rb`.
- Authored definition: `Shop::Discount`.
- Required concrete usage: `Discount` in `Shop::Discount.default`.
- Optional binding: `:Discount` in Ruby's built-in `autoload`.
- Excluded token: the `"shop/discount"` path string.
- Ground-truth decision: **correct after making the autoload symbol an optional
  binding rather than a required concrete usage**.
- Operation decision: **definition** from both the qualified class token and
  recognized autoload binding to `class Discount`.
- Reviewer rationale: `autoload` declaratively registers a lazy constant name;
  under `bindings_optional` it is import-like metadata. Navigation remains
  useful and sound because the exact indexed declaration exists in this
  fixture. The case uses no Zeitwerk configuration or application lockfile.
- Outcome revealed after review: Ruby LSP 0.26.10 navigates both tokens exactly
  to `class Discount`, confirming its built-in autoload handling. Its reference
  range for `Shop::Discount` covers the full qualified path. Solargraph 0.60.2
  navigates the qualified token to the enclosing class body but sends the
  autoload symbol only to the start of `discount.rb`, leaving a genuine
  file-level rather than concrete-definition target.

### ruby-parity-attr-reader-method-call

- Source: `fixtures/ruby/lsp-parity/lib/shop/product.rb` and
  `app/catalog.rb`.
- Authored declaration: generated method token `:name` in
  `attr_reader :name`.
- Required usages: source symbol `:name` in `alias_method :label, :name` and
  direct call `product.name`.
- Excluded reference result: the `attr_reader` macro method token or expression;
  it is not the declared `:name` token.
- Excluded identities: alias declaration `:label`, calls to `label`, and field
  `@name`.
- Ground-truth decision: **correct**.
- Operation decision: **declaration** from `product.name` to the `attr_reader`
  symbol because no authored method body exists.
- Reviewer rationale: `attr_reader` declares the generated reader;
  `alias_method`'s second symbol names that exact source method, while its first
  symbol declares a distinct alias scored separately.
- Outcome revealed after review: neither Ruby LSP 0.26.10 nor Solargraph 0.60.2
  advertises LSP Declaration. Practical Definition from `product.name` reaches
  the exact symbol in Ruby LSP and full `attr_reader` expression in Solargraph.
  Both omit the alias source from references queried at the scored call-site
  probe. Querying the declaration origin instead produces the `attr_reader`
  macro token or expression rather than the declared `:name` token, so it
  remains unexpected. The alias source remains a genuine semantic false
  negative for both.

### ruby-parity-singleton-class-method-definition

- Source: `fixtures/ruby/lsp-parity/lib/shop/product.rb` and
  `app/catalog.rb`.
- Authored definition: `Product.from_sku` inside `class << self`.
- Required usage: `from_sku` in `Shop::Product.from_sku("sku-1")`.
- Optional result: the queried method declaration itself.
- Ground-truth decision: **correct**.
- Operation decision: **definition** to the authored method body.
- Reviewer rationale: `class << self` opens `Product`'s singleton class, making
  the nested `def` the exact target of the qualified constant-receiver call.
- Outcome revealed after review: Ruby LSP 0.26.10 and Solargraph 0.60.2 both
  return the exact call plus declaration and both navigate to `from_sku`. Ruby
  LSP selects the exact method token; Solargraph selects the enclosing method
  body.

### ruby-parity-alias-method-call

- Source: `fixtures/ruby/lsp-parity/lib/shop/product.rb` and
  `app/catalog.rb`.
- Authored declaration: `:label` in `alias_method :label, :name`.
- Required usages: bare `label` inside `summary` and receiver call
  `product.label`.
- Excluded reference result: the `alias_method` macro method token or
  expression; it is not the declared `:label` token.
- Excluded identity: source symbol `:name`, scored as a usage of the generated
  reader in the attr-reader case.
- Ground-truth decision: **correct**.
- Operation decision: **declaration** from `product.label` to `:label`.
- Reviewer rationale: `alias_method` declares a distinct alias identity even
  though it copies the source method implementation. A separate
  Definition-style operation could follow the implementation chain, but the
  source-level declaration of `label` is the first target.
- Outcome revealed after review: neither Ruby LSP 0.26.10 nor Solargraph 0.60.2
  advertises LSP Declaration. From `product.label`, both return both exact
  calls and practical Definition reaches the alias declaration rather than
  `name`; Ruby LSP selects `:label` including its colon and Solargraph the full
  `alias_method` expression. Queries from the declaration origin return the
  macro token or expression rather than calls, while the scored call-site probe
  returns both expected calls. Reference discovery is occurrence-sensitive,
  and the macro result remains unexpected.

### ruby-parity-module-function-call

- Source: `fixtures/ruby/lsp-parity/lib/shop/pricing.rb` and
  `app/catalog.rb`.
- Authored definition: `Shop::Pricing.tax_rate`.
- Required usage: `tax_rate` in `Shop::Pricing.tax_rate("EU")`.
- Optional result: the queried method declaration itself.
- Ground-truth decision: **correct**.
- Operation decision: **definition** to `def tax_rate`.
- Reviewer rationale: argument-free `module_function` applies to the following
  definition, retaining a private instance-method form and exposing a
  singleton-method copy on `Pricing` that shares the authored body.
- Outcome revealed after review: Ruby LSP 0.26.10 and Solargraph 0.60.2 both
  return the exact call plus declaration and navigate to the shared body. Ruby
  LSP selects the exact method token; Solargraph selects the enclosing method
  range.

## Ruby LSP calibration summary

- Ruby LSP required a five-second post-initialization settle for reproducible
  fixture indexing. At 750 ms, repeated full runs intermittently returned empty
  reference and Definition results for otherwise supported cases.
- The settled Ruby LSP run reports 5 passes, 3 position-unverified outcomes,
  8 semantic failures, 4 unsupported cases, and 1 deliberately not-planned
  case across the 21 reviewed Ruby cases.
- Solargraph reports 11 position-unverified outcomes, 3 semantic failures,
  6 unsupported cases, and 1 deliberately not-planned case. Its broad enclosing
  ranges account for the absence of exact-position passes.
- Scored `referenceProbe` origins preserve occurrence-sensitive LSP behavior.
  When one reference origin fails and another succeeds, the successful union is
  scored while the failed origin remains visible in raw status; a case errors
  only when every origin fails.

## scala-baseline.yaml

### scala-class-construction

- Source: `fixtures/scala/baseline/src/main/scala/example/Service.scala` and
  `Consumer.scala`.
- Authored definition: `class Service(repository: Repository)`.
- Required usages: the `Service` return type of the companion `build` method
  and the constructor token in `new Service(repository)`.
- Excluded identity: `Service` in `Service.build(repository)`, which resolves
  to the companion object in Scala's term namespace rather than the class.
- Ground-truth decision: **correct**.
- Operation decision: **definition** from the constructor occurrence to
  `class Service`, which defines the primary constructor.
- Reviewer rationale: both tokens in the `build` body/signature name the class;
  the constructor occurrence is additionally classified as `constructor`.
  The same-spelled companion object is a related but distinct declaration.
- Outcome revealed after review: Metals 1.6.7 returns both required class
  occurrences and navigates the constructor exactly, but also includes the
  companion-object qualifier in `Service.build`. That extra remains genuine
  class-reference noise. Bifrost 0.8.9 returns the exact authored usage family
  and definition target.

### scala-companion-method-call

- Source: `fixtures/scala/baseline/src/main/scala/example/Service.scala` and
  `Consumer.scala`.
- Authored definition: `def build` on the `Service` companion object.
- Required usage: `build` in `Service.build(repository)`.
- Excluded identity: the `Service` qualifier, which is a usage of the companion
  object rather than the method.
- Ground-truth decision: **correct after renaming and reclassifying the case
  from function to method**.
- Operation decision: **definition** to the authored `def build`.
- Reviewer rationale: Scala `def` introduces a method. A function is a value
  implementing `FunctionN`, such as a lambda assigned to a `val`; no such value
  exists here.
- Outcome revealed after review: Metals 1.6.7 and Bifrost 0.8.9 both return the
  exact `build` call and definition target.

### scala-method-call

- Source: `fixtures/scala/baseline/src/main/scala/example/Service.scala` and
  `Consumer.scala`.
- Authored definition: `Service#execute`.
- Required usage: `execute` in `service.execute(" Ada ")`.
- Ground-truth decision: **correct**.
- Operation decision: **definition** to the authored `def execute`.
- Reviewer rationale: the immutable local is initialized by `Service.build`,
  whose return type is `Service` and whose implementation constructs exactly
  `new Service`. The fixture contains no subclass, override, reassignment, or
  competing `execute` declaration.
- Outcome revealed after review: Bifrost 0.8.9 returns the exact reference and
  definition target. Metals 1.6.7 returns the exact reference family but no
  Definition target in an isolated run; the complete Scala run passes the same
  case after earlier baseline queries. This is query-order-sensitive analysis,
  not ground-truth ambiguity.

### scala-var-field-access

- Source: `fixtures/scala/baseline/src/main/scala/example/Service.scala` and
  `Consumer.scala`.
- Authored declaration: `var last: String = ""` on `Repository`.
- Required usages: the write and read inside `save`, plus the external
  `repository.last` read.
- Ground-truth decision: **correct after renaming the misleading
  `scala-field-and-val-access` case**.
- Operation decision: **declaration** from the external read to the `last`
  token in `var last`.
- Reviewer rationale: Scala generates getter and setter members for a `var`,
  but these source occurrences are accessor operations on one mutable property
  family. The fixture contains no `val` access, so the former case name was
  inaccurate.
- Compilation verification: the pinned Metals profile successfully runs
  `sbt 1.11.7 compile` with Scala 3.7.3 before semantic queries.
- Outcome revealed after review: Bifrost 0.8.9 returns the exact property
  family and declaration target. Metals 1.6.7 returns the exact reference
  family but does not advertise LSP Declaration, so the semantic navigation
  operation is unsupported rather than replaced with Definition.

### scala-object-val-access

- Source: `fixtures/scala/baseline/src/main/scala/example/Service.scala` and
  `Consumer.scala`.
- Authored declaration: `val Prefix = "job"` on `object Defaults`.
- Required usages: `Defaults.Prefix` in `Service#execute` and in
  `Consumer.run`.
- Excluded identities: the `Defaults` qualifiers, which reference the singleton
  object rather than the `Prefix` member.
- Ground-truth decision: **correct**.
- Operation decision: **declaration** to the `Prefix` token in `val Prefix`.
- Reviewer rationale: `Prefix` is an immutable stable object member. Scala may
  implement access through a generated accessor, but the source-level binding
  is the `val` declaration rather than a callable definition body.
- Outcome revealed after review: Bifrost 0.8.9 returns the exact reads and
  declaration target. Metals 1.6.7 returns the exact reference family but does
  not advertise LSP Declaration, so navigation remains unsupported rather than
  being weakened to Definition.

## scala-precision.yaml

### scala-renamed-import-object-method-call

- Source: `fixtures/scala/precision/src/main/scala/precision/Precision.scala`
  and `Consumer.scala`.
- Authored definition: `Tools.choose`.
- Required usage: the aliased concrete call `select("value")`.
- Optional bindings: both names in `import Tools.{choose => select}` under the
  document's `bindings_optional` policy.
- Ground-truth decision: **correct after renaming the case from companion to
  object and reclassifying the symbol from function to method**.
- Operation decision: **definition** from the aliased call to `def choose`.
- Reviewer rationale: importing a method without its receiver makes the call
  syntactically function-like, but the referenced declaration remains the
  method member of singleton `object Tools`. There is no companion class or
  `FunctionN` value in the fixture.
- Compilation verification: the pinned Metals profile successfully runs
  `sbt 1.11.7 compile` with Scala 3.7.3 before semantic queries.
- Outcome revealed after review: Bifrost 0.8.9 returns the exact aliased call
  and definition target. Metals 1.6.7 returns neither the call from Find
  References nor a Definition target from `select`, despite compiling the
  fixture. These are renamed-import semantic misses, not evidence that the
  valid alias is disconnected from `Tools.choose`.

### scala-imported-extension-method-call

- Source: `fixtures/scala/precision/src/main/scala/precision/Precision.scala`
  and `Consumer.scala`.
- Authored definition: the `decorate` extension method on `String`.
- Required usage: `decorate` in `"value".decorate`.
- Optional binding: the wildcard `import Extensions.*`; it contains no separate
  `decorate` token.
- Ground-truth decision: **correct**.
- Operation decision: **definition** to the authored extension
  `def decorate`.
- Reviewer rationale: the statically known `String` receiver exactly matches
  the extension receiver type, and the fixture contains no competing extension
  or ordinary `decorate` member.
- Outcome revealed after review: Bifrost 0.8.9 returns the exact extension call
  and definition target. Metals 1.6.7 passes in isolation but misses both the
  reference and Definition in the complete Scala run after the renamed-import
  query, exposing query-order-sensitive LSP behavior.

### scala-object-apply-call

- Source: `fixtures/scala/precision/src/main/scala/precision/Precision.scala`
  and `Consumer.scala`.
- Authored definition: `Maker.apply`.
- Required usage: the visible `Maker` token in `Maker("value")`, which
  implicitly invokes `apply`.
- Ground-truth decision: **correct after renaming the case from companion to
  object and reclassifying the symbol from function to method**.
- Operation decision: **reference-only**.
- Reviewer rationale: the implicit `apply` edge is exact, but the only visible
  token simultaneously names `object Maker`. Ordinary navigation from that
  token naturally targets the object, so the benchmark should not manufacture
  a reverse Definition expectation to an unspelled `apply` token.
- Outcome revealed after review: Bifrost 0.8.9 returns the implicit apply
  reference exactly. Metals 1.6.7 does not connect `def apply` to the visible
  `Maker` call token. This is a synthetic-call reference miss rather than a
  reason to remove the exact compiler edge.

## scala-lsp-parity.yaml

### scala-parity-trait-method-implementation

- Source: `fixtures/scala/lsp-parity/src/main/scala/example/Workflow.scala`.
- Authored definition: abstract `Renderer#render`.
- Required usages: the overriding `ConsoleRenderer#render` declaration and the
  call through `Workflow`'s `Renderer`-typed constructor parameter.
- Excluded identity: the final direct call through the immutable,
  concrete-typed `renderer` value.
- Ground-truth decision: **correct after removing the statically concrete call
  from the trait usage family**.
- Operation decision: **definition** from the interface-typed call to the
  abstract trait method.
- Reviewer rationale: the override declaration explicitly relates itself to
  the trait member, and the `Renderer`-typed call dispatches through that
  contract. The separately scored concrete receiver has no genuine static
  ambiguity and should not be absorbed into the trait family.
- Outcome revealed after review: Metals 1.6.7 returns the exact interface-typed
  call and navigation target but omits the overriding declaration from
  ordinary References; the complete parity run also returns the separately
  scored concrete call as an extra. Bifrost 0.8.9 includes the override and
  navigates exactly, but likewise returns the concrete call as trait-family
  noise.

### scala-parity-concrete-override-method-call

- Source: `fixtures/scala/lsp-parity/src/main/scala/example/Workflow.scala`.
- Authored definition: overriding `ConsoleRenderer#render`.
- Required usage: `render` in the final direct call through the imported
  `renderer` value.
- Ground-truth decision: **added as a separate exact concrete-dispatch case**.
- Operation decision: **definition** to the overriding `def render`.
- Reviewer rationale: the immutable value is initialized by
  `ConsoleRenderer.default`, whose declared return type and implementation are
  exactly `ConsoleRenderer`. No competing assignment or implementation exists
  in the fixture.
- Outcome revealed after review: Bifrost 0.8.9 returns the exact concrete call
  and overriding definition target. Metals 1.6.7 passes in isolation, while
  the complete parity run also returns the interface-typed trait call as an
  extra, exposing query-order-sensitive family expansion.

### scala-parity-import-alias-companion-method

- Source: `fixtures/scala/lsp-parity/src/main/scala/example/Workflow.scala`.
- Authored definition: parameterless `ConsoleRenderer.default` on the
  companion object.
- Required usages: both concrete `renderer` expressions after the renamed
  import.
- Optional bindings: `default` and `renderer` inside
  `import ConsoleRenderer.{default => renderer}`.
- Ground-truth decision: **correct**.
- Operation decision: **definition** from the alias expression to
  `def default`.
- Reviewer rationale: each bare alias expression evaluates the imported
  parameterless method. `render` is a separate method usage, while the import
  clause is binding metadata under `bindings_optional`.
- Outcome revealed after review: Metals 1.6.7 and Bifrost 0.8.9 both return the
  two exact alias expressions and definition target.

### scala-parity-extension-method-call

- Source: `fixtures/scala/lsp-parity/src/main/scala/example/Workflow.scala`.
- Authored definition: the `slug` extension method on `String`.
- Required usage: `slug` in `"Hello World".slug`.
- Optional binding: `import Syntax.*`; it contains no separate `slug` token.
- Ground-truth decision: **correct**.
- Operation decision: **definition** to the authored extension `def slug`.
- Reviewer rationale: the statically known `String` receiver exactly matches
  the extension receiver type, and no competing extension or ordinary `slug`
  member exists.
- Outcome revealed after review: Metals 1.6.7 and Bifrost 0.8.9 both return the
  exact extension call and definition target.

### scala-parity-workflow-method-call

- Source: `fixtures/scala/lsp-parity/src/main/scala/example/Workflow.scala`.
- Authored definition: `Workflow#run`.
- Required usage: `run` in `workflow.run("Hello World")`.
- Ground-truth decision: **correct**.
- Operation decision: **definition** to the authored `def run`.
- Reviewer rationale: the immutable object `val` is initialized directly with
  `new Workflow`; the fixture contains no subclass, reassignment, override, or
  competing `run` method.
- Outcome revealed after review: Metals 1.6.7 and Bifrost 0.8.9 both return the
  exact call and definition target.

### scala-parity-case-class-generated-construction-and-copy

- Source: `fixtures/scala/lsp-parity/src/main/scala/example/Workflow.scala`.
- Authored provenance: `case class RenderRequest(value: String)`.
- Required usage: the generated-`apply` construction
  `RenderRequest("Hello World")`.
- Ground-truth decision: **replaced the empty not-planned synthetic placeholder
  with planned concrete coverage**.
- Operation decisions: **definition** from the construction token and from
  `copy` to the originating case-class declaration.
- Reviewer rationale: generated `apply` and `copy` lack authored method tokens,
  but their source provenance is unambiguously the case-class declaration.
  This follows the same source-origin policy used for generated JavaScript and
  TypeScript behavior rather than making Scala uniquely unscoreable.
- Outcome revealed after review: Metals 1.6.7 returns the exact construction
  reference and navigates both construction and `copy` to the case-class
  declaration. Bifrost 0.8.9 returns the construction edge and navigation
  exactly but cannot resolve the generated `copy` receiver.

### scala-parity-case-class-generated-component-access

- Source: `fixtures/scala/lsp-parity/src/main/scala/example/Workflow.scala`.
- Authored declaration: the case-class component parameter `value`.
- Required usages: the named `value` argument to generated `copy` and the
  generated accessor in `copied.value`.
- Ground-truth decision: **added as planned generated-component coverage**.
- Operation decision: **declaration** from the accessor to the authored
  case-class parameter.
- Reviewer rationale: the constructor parameter is the authored source origin
  for both the immutable accessor and generated `copy` parameter name.
- Outcome revealed after review: Metals 1.6.7 returns both exact component
  usages; Declaration remains unsupported because it is not advertised.
  Bifrost 0.8.9 returns neither generated usage and cannot resolve the accessor
  receiver for Declaration. Together with `copy`, this points to a general
  synthesized-member model rather than a Scala-specific exception.

## Scala analyzer calibration summary

- All three Scala fixture roots compile successfully with sbt 1.11.7 and Scala
  3.7.3 before Metals starts semantic queries.
- The complete Metals 1.6.7 run reports 6 passes, 6 semantic failures, and 3
  unsupported Declaration cases across 15 reviewed cases.
- The complete Bifrost 0.8.9 comparison reports 12 passes and 3 semantic
  failures. Its failures are trait-family over-expansion plus generated
  `copy`/component receiver and usage recovery.
- Metals confirms the planned case-class provenance contract exactly:
  generated construction and `copy` Definition target the case-class
  declaration, while the authored component parameter finds both the generated
  `copy` argument and accessor.
- Several Metals results depend on query order despite the configured
  60-second settle: `Service#execute` improves in the aggregate, while the
  precision extension and override-family boundaries regress. These outcomes
  are recorded as analyzer/request-order behavior rather than used to redefine
  the human-reviewed contract.

## typescript-baseline.yaml

### ts-named-export-import-function

- Source: `fixtures/typescript/baseline/src/components.tsx` and `app.tsx`.
- Authored definition: exported function `formatName`.
- Required usages: the call inside `Greeter#greet` and the imported call in
  `app.tsx`.
- Optional binding: `formatName` in the app import clause.
- Ground-truth decision: **correct**.
- Operation decision: **definition** to the authored function body.
- Reviewer rationale: inline `export` modifies the declaration rather than
  creating a separate re-export usage. Both executable calls resolve exactly
  to the same function.
- Compilation verification: pinned TypeScript 5.9.3 accepts the complete
  baseline fixture with `tsc --noEmit`.
- Outcome revealed after review: TypeScript Language Server 5.3.0 and Bifrost
  0.8.9 both return the exact two calls and definition target.

### ts-default-class-import-and-construction

- Source: `fixtures/typescript/baseline/src/components.tsx` and `app.tsx`.
- Authored definition: default-exported class `Greeter` with an explicit
  constructor.
- Required usages: both `Greeter` tokens in `new` expressions.
- Optional binding: the default-import token in `app.tsx`.
- Ground-truth decision: **correct**.
- Operation decision: **definition** to the class declaration, with the
  explicit `constructor` member allowed as an additional target.
- Reviewer rationale: the visible token names the class binding, making the
  class declaration canonical. The constructor body is also a useful
  control-flow destination but cannot replace the canonical source target.
- Methodology adjustment: `allowedExtraTargets` now permits an explicitly
  reviewed related target with a different symbol kind, allowing class and
  constructor destinations to coexist without misclassifying either token.
- Outcome revealed after review: Bifrost 0.8.9 returns the exact constructions
  and canonical class target. TypeScript Language Server 5.3.0 returns those
  constructions plus the optional import binding and both navigation targets.
  Its constructor target spans the enclosing body rather than the authored
  token, making the otherwise correct semantic result position-unverified.

### ts-class-method-call

- Source: `fixtures/typescript/baseline/src/components.tsx` and `app.tsx`.
- Authored definition: `Greeter#greet`.
- Required usages: the calls through directly constructed immutable locals in
  `WelcomeCard` and `app.tsx`.
- Ground-truth decision: **correct**.
- Operation decision: **definition** to the authored method body.
- Reviewer rationale: the fixture contains no subclass, override, reassignment,
  or competing `greet` member, so both receivers have exact `Greeter` identity.
- Outcome revealed after review: TypeScript Language Server 5.3.0 and Bifrost
  0.8.9 both return the exact two calls and definition target.

### ts-object-property-access

- Source: `fixtures/typescript/baseline/src/components.tsx` and `app.tsx`.
- Authored declaration: the `name` property signature in the `User` type
  literal.
- Required usages: the `user.name` read in `formatName` and the `name` key in
  the contextually typed object literal.
- Ground-truth decision: **correct**.
- Operation decision: **declaration** to the property signature.
- Reviewer rationale: the explicit `User` annotation gives the object literal
  an exact contextual type, so its key and the later read refer to the same
  structural property. The type-only property has no executable definition.
- Outcome revealed after review: Bifrost 0.8.9 passes the declaration lookup
  exactly. TypeScript Language Server 5.3.0 does not advertise
  `textDocument/declaration`, so the canonical operation is unsupported; a
  separate ordinary definition probe targets the exact property signature.

### ts-tsx-component-reference

- Source: `fixtures/typescript/baseline/src/components.tsx` and `app.tsx`.
- Authored definition: function component `WelcomeCard`.
- Required usage: the `WelcomeCard` token in the self-closing JSX element.
- Optional binding: `WelcomeCard` in the app import clause.
- Ground-truth decision: **correct**.
- Operation decision: **definition** to the authored function body.
- Reviewer rationale: the JSX tag is an executable component reference, and
  the self-closing syntax has no separate closing-tag occurrence.
- Outcome revealed after review: TypeScript Language Server 5.3.0 and Bifrost
  0.8.9 both return the exact JSX usage and definition target.

### ts-ky-style-implementation-static-create

- Source: `fixtures/typescript/baseline/src/http.ts` and `app.tsx`.
- Authored definition: implementation class `Ky`.
- Required usages: the three `Ky` qualifiers on static `create` calls in the
  callable factory, assigned `get` implementation, and direct app call.
- Optional binding: `Ky` in the app import clause.
- Ground-truth decision: **correct**.
- Operation decision: **definition** to the authored class body.
- Reviewer rationale: this case measures the class-symbol family. The `create`
  member tokens are distinct static-method usages and do not belong in this
  class-anchored case.
- Outcome revealed after review: TypeScript Language Server 5.3.0 and Bifrost
  0.8.9 both return the exact three class usages and definition target.

## typescript-precision.yaml

### ts-chained-barrel-function-call

- Source: `fixtures/typescript/precision/src/api.ts`, `barrel.ts`, `app.ts`, and
  `shadow.ts`.
- Authored definition: exported function `createWidget` in `api.ts`.
- Required usage: the executable call imported through the barrel in `app.ts`.
- Optional bindings: the barrel re-export plus import-clause tokens.
- Required exclusion: the same-spelled parameter and its call in `shadow.ts`,
  which belong to the local parameter symbol.
- Ground-truth decision: **correct**.
- Operation decision: **definition** through the barrel to the authored
  function body.
- Compilation verification: pinned TypeScript 5.9.3 accepts the complete
  precision fixture with `tsc --noEmit`.
- Outcome revealed after review: TypeScript Language Server 5.3.0 and Bifrost
  0.8.9 both preserve the function identity through the barrel, exclude the
  shadowed call, and return the exact definition target.

### ts-type-annotation-through-barrel

- Source: `fixtures/typescript/precision/src/api.ts`, `barrel.ts`, and `app.ts`.
- Authored declaration: interface `Widget` in `api.ts`.
- Required usages: the `Widget` return annotation in `api.ts` and variable
  annotation imported through the barrel in `app.ts`.
- Optional bindings: the barrel re-export and app import-clause tokens.
- Ground-truth decision: **correct**.
- Operation decision: **declaration** through the barrel to the authored
  interface declaration.
- Reviewer rationale: both explicit type tokens share exact interface identity.
  The structurally compatible object literal has no `Widget` token and is not a
  usage.
- Outcome revealed after review: Bifrost 0.8.9 passes the declaration lookup
  exactly. TypeScript Language Server 5.3.0 does not advertise
  `textDocument/declaration`, so that operation is unsupported; a separate
  ordinary definition probe traverses the barrel to the exact interface
  declaration.

## typescript-lsp-parity.yaml

### ts-parity-default-function-import-call

- Source: `fixtures/typescript/lsp-parity/src/api.ts` and `app.ts`.
- Authored definition: default-exported function `createClient`.
- Required usage: the executable app call.
- Optional binding: the default-import token.
- Ground-truth decision: **correct**.
- Operation decision: **definition** to the authored function body.
- Reviewer rationale: inline `export default` modifies the declaration rather
  than adding a separate usage.
- Compilation verification: pinned TypeScript 5.9.3 accepts the complete
  LSP-parity fixture with `tsc --noEmit`.
- Outcome revealed after review: TypeScript Language Server 5.3.0 and Bifrost
  0.8.9 both return the exact call and definition target.

### ts-parity-static-method-call

- Source: `fixtures/typescript/lsp-parity/src/api.ts` and `app.ts`.
- Authored definition: static method `ApiClient.create`.
- Required usages: the `create` member calls in `createClient` and `app.ts`.
- Ground-truth decision: **correct**.
- Operation decision: **definition** to the authored static method body.
- Reviewer rationale: static dispatch gives both member tokens exact identity.
  The `ApiClient` qualifiers belong to the class-symbol family, while
  `new ApiClient` invokes the constructor rather than `create`.
- Outcome revealed after review: TypeScript Language Server 5.3.0 and Bifrost
  0.8.9 both return the exact two static-method usages and definition target.

### ts-parity-type-only-interface-import

- Source: `fixtures/typescript/lsp-parity/src/api.ts` and `app.ts`.
- Query expression: the local variable identifier `user` in its declaration.
- Expected type: the authored `User` interface in `api.ts`.
- Ground-truth decision: **correct**.
- Reviewer rationale: the explicit `User` annotation makes the recovered type
  exact without depending on inference from `fetchUser`. The type-only import
  binding is routing metadata rather than the final type target.
- Outcome revealed after review: TypeScript Language Server 5.3.0 and Bifrost
  0.8.9 both recover the exact authored interface target.

### ts-parity-interface-property-access

- Source: `fixtures/typescript/lsp-parity/src/api.ts` and `app.ts`.
- Authored declaration: the `name` property signature in interface `User`.
- Required usages: the contextually typed object-literal key in `fetchUser`,
  the read in `formatUser`, and the app member access.
- Required exclusion: the same-spelled local variable declaration in
  `const name`.
- Ground-truth decision: **correct**.
- Operation decision: **declaration** to the property signature.
- Reviewer rationale: the explicit `User` return type makes the object-literal
  key exact, while the local variable and member token have distinct symbol
  identities.
- Outcome revealed after review: Bifrost 0.8.9 passes the declaration lookup
  exactly. TypeScript Language Server 5.3.0 does not advertise
  `textDocument/declaration`, so that operation is unsupported; a separate
  ordinary definition probe reaches the exact property signature.

## TypeScript analyzer calibration summary

The first independent human review of every case currently in
`typescript-baseline.yaml`, `typescript-precision.yaml`, and
`typescript-lsp-parity.yaml` is complete.

- All three fixture roots compile successfully with pinned TypeScript 5.9.3
  before semantic comparisons.
- Bifrost 0.8.9 passes all 12 reviewed cases exactly.
- TypeScript Language Server 5.3.0 passes eight cases exactly. The default-class
  construction case is position-unverified because its useful constructor
  alternate spans the enclosing body rather than the authored token.
- The three bodyless type/property cases deliberately require Declaration.
  TypeScript Language Server does not advertise that operation, so they are
  unsupported rather than semantic failures; separate ordinary Definition
  probes reach the exact authored declarations.
- Import and re-export bindings remain optional metadata throughout. The
  reviewed contracts require concrete calls, constructions, type references,
  and property accesses while retaining analyzer-returned bindings for audit.
