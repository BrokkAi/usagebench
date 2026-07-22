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
