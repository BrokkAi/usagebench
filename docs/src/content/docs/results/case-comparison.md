---
title: Case comparison
description: Case-level disagreements between Bifrost and each measured language server.
---

This page lists every non-exact case plus exact controls needed to interpret the
macro and CommonJS comparisons. “Bifrost pass” means the pinned Bifrost run
satisfied the authored contract; “Bifrost gap” means a documented expected
failure. LSP near misses are policy-only binding/export extras. An LSP hard
result means contract disagreement, not an automatic defect verdict.

> This is the case-level audit for the legacy 2026-07-16 development run. Its
> labels preserve that run's former line-level and policy-near-miss semantics;
> they are not schema-v2 evaluation outcomes.

## C++

| Case | Bifrost | clangd | Observed distinction |
|---|---|---|---|
| `cpp-function-call` | Gap | Exact | Human review separated the out-of-line definition from ordinary usages. Clangd returns only the call and navigates exactly to the body; Bifrost returns the definition as an extra usage and returns both header and body for definition navigation. |
| `cpp-method-call` | Gap | Exact | Human review separated the out-of-line method definition from the concrete call. Clangd is exact; Bifrost returns the definition as an extra usage and returns both header and body for definition navigation. |
| `cpp-out-of-line-member-call` | Gap | Exact | Human review excluded the out-of-line body and retained only the concrete call, comment/string exclusions, and exact body navigation. Clangd is exact; Bifrost returns the body as an extra usage and returns header/body `multiple_targets`. |
| `cpp-overload-string-call-is-narrow` | Gap | Exact | Human review requires only the string call and exact `select(const char*)` body. Clangd is exact. Bifrost preserves the forward overload usage but returns its body as an extra and broadens reverse navigation across both overload declarations and bodies. |
| `cpp-field-access` | Unsupported | Exact | Human review requires declaration navigation from the field read. Clangd is exact; Bifrost does not expose declaration navigation separately from definition lookup, so the case is a capability boundary rather than a semantic failure. |
| `cpp-constant-access` | Unsupported | Exact | Human review requires declaration navigation to the inline constant binding. Clangd is exact; Bifrost exposes the same declaration-navigation capability boundary as the field case. |
| `cpp-parity-overload-string-function-call` | Gap | Exact | Human review retained only the string call and requires exact definition navigation to `format(const std::string&)`. Clangd is exact. Bifrost preserves overload precision but returns the body as an extra usage and returns the string declaration/body as `multiple_targets`. |
| `cpp-class-reference` | Legacy | Hard | Human review added the construction expression as a required class usage. Clangd finds it but also expands the class query to constructor declaration/definition tokens; the C++ language and Clang AST classify those as constructor declarations rather than ordinary class usages. Bifrost requires a rerun against the corrected expectation. |
| `cpp-constructor-call` | Legacy | Exact | Human review confirmed that the functional-construction token names the class type while causing a constructor call. Clangd returns the reviewed class definition exactly; Bifrost requires a rerun because its retained pass used the rejected constructor-declaration target. |
| `cpp-parity-using-alias-constructor` | Gap | Hard | Human review split source-level alias identity from underlying type identity: declaration navigation reaches `HandlerAlias`, while type-definition navigation reaches `ConsoleHandler`. Clangd passes both operations but omits three required transitive/class-qualifier references and adds the constructor declaration. Bifrost finds all four usages at line precision, but declaration navigation is unsupported and C++ type lookup is not implemented. |
| `cpp-parity-virtual-base-method-call` | Unsupported | Exact | Human review made declaration navigation explicit for the pure-virtual base member. Clangd returns only the base-reference call and navigates exactly to `BaseHandler::handle`. Bifrost finds the required usage at line precision but has no distinct declaration-navigation operation. |
| `cpp-parity-concrete-override-method-call` | Error | Hard | Human review retained only the concrete-receiver call and requires definition navigation to the unique out-of-line override body. Clangd finds the call but adds the base declaration and base-typed call, then navigates only to the override header declaration. Bifrost hit a repeatable read-only analyzer-store error; one partial reverse result returned header/body `multiple_targets`, so no complete semantic result is claimed. |
| `cpp-parity-template-function-call` | Position-unverified | Exact | Human review made definition navigation to the authored template body explicit. Clangd is exact. Bifrost finds the specialization call and resolves it to the template body, but its C++ results are line-level rather than token-level. |
| `cpp-parity-direct-function-call-control` | Position-unverified | Exact | Human review made definition navigation explicit. Clangd is exact. Bifrost finds the direct call and inline definition without missing or extra lines, but returns line-level rather than token-level C++ locations. |
| `cpp-parity-function-like-macro-expanded-call` | Gap | Exact | Human review confirmed the visible macro argument as the stable usage/navigation token, corroborated exactly by clangd. Bifrost finds that argument in the forward direction at line precision but cannot navigate it back to the inline function definition. |
| `cpp-parity-compile-commands-unsupported` | Unsupported | Exact (opt-in) | The case now contains a guarded call and declaration lookup. `clangd-configured.json` defines `ENABLE_PARITY_FEATURE` and passes exactly with `--include-unsupported`; ordinary clangd fails the same expectations while the branch is inactive. Default Bifrost and cross-tool reports retain the unsupported boundary. |

## C#

| Case | Bifrost | Roslyn | Observed distinction |
|---|---|---|---|
| `csharp-parity-interface-receiver-method-call` | Pass | Hard | Roslyn includes a related implementation-family call. |
| `csharp-parity-concrete-implementation-method-call` | Pass | Hard | Roslyn includes a related interface/implementation-family call. |
| `csharp-parity-namespace-alias-constructor` | Pass | Hard | Reverse navigation resolves to the namespace alias binding rather than the underlying class declaration. |

## Go

| Case | Bifrost | gopls | Observed distinction |
|---|---|---|---|
| `go-pointer-receiver-method-call` | Exact | Exact | The reviewed contract requires the exact concrete call and records the interface-typed call as an unproven implementation-family candidate. gopls expands references across that family while preserving distinct concrete and interface navigation targets; Bifrost satisfies the same two-tier contract. |
| `go-dot-import-concrete-receiver-call` | Gap | Exact | Both analyzers return the reviewed concrete calls plus the conservative interface-family candidate. gopls navigates each concrete selector to `Worker.Record`; Bifrost instead rejects both definition lookups with an incorrect local-binding-shadow diagnosis. |
| `go-interface-receiver-method-call` | Gap | Exact | The required usage keeps its static `Recorder.Record` identity, while both concrete calls are conservative implementation-family candidates. gopls returns the complete two-tier family. Bifrost finds the required interface call but omits both candidates. |
| Field, constant, and variable declaration cases | Unsupported | Unsupported | Both analyzers return the reviewed reference sets. Neither exposes Go declaration navigation as a distinct operation, and the harness deliberately does not substitute definition navigation. |

## Java

| Case | Bifrost | JDT LS | Observed distinction |
|---|---|---|---|
| `java-service-class-construction` | Gap | Exact | Bifrost misses two `Service` qualifiers in `Service.Repository`. |
| `java-nested-class-constructor` | Gap | Exact | Bifrost misses the nested `Repository` field and constructor-parameter type usages. |
| `java-parity-static-import-method-call` | Pass | Near | JDT LS additionally returns the static-import binding. |
| `java-parity-concrete-implementation-method-call` | Pass | Hard | Policy difference: JDT LS expands the concrete-method query across the implementation family and includes an anonymous `Handler` call. The reviewed ground truth keeps concrete method identity narrow, so this is documented rather than accepted as an allowed extra. |

## JavaScript and TypeScript

| Case group | Bifrost | TypeScript LS | Observed distinction |
|---|---|---|---|
| Nine ES import/re-export cases | Pass | Near | Required references agree; TypeScript LS also returns binding/export locations. |
| `js-parity-commonjs-destructured-function-call` | Pass | Hard | TypeScript LS omits the destructured CommonJS function call. |
| `js-commonjs-barrel-class-construction` | Pass | Hard | TypeScript LS omits the construction reached through the CommonJS barrel. |
| `js-commonjs-barrel-member-call` | Exact | Exact | Both analyzers satisfy the barrel-member control, isolating the other TypeScript LS CommonJS disagreements to different binding shapes. |

## PHP

| Case | Bifrost | Intelephense | Observed distinction |
|---|---|---|---|
| `php-function-import-call` | Pass | Near | Intelephense additionally returns the imported-function binding. |
| `php-parity-interface-method-implementation` | Pass | Hard | The expected implementation reference and one reverse lookup are absent. |
| `php-interface-typed-receiver-call` | Pass | Hard | The interface-typed receiver call is absent. |

## Python

| Case group | Bifrost | Pyright | Observed distinction |
|---|---|---|---|
| Four import/barrel cases | Pass | Near | Required uses agree; Pyright additionally returns imports, re-exports, or `__all__` metadata. |
| `python-parity-reexported-class-alias-classmethod` | Pass | Hard | Pyright omits two alias-site references and returns one original-symbol location outside the binding policy. |
| `python-module-import` | Pass | Unsupported | The authored zero-width module selector has no LSP cursor token; Bifrost resolves it through symbol selection. |

## Ruby

| Case group | Bifrost | Ruby LSP | Observed distinction |
|---|---|---|---|
| Constants, mixins, singleton methods, aliases, autoload, `attr_reader`, class variables, factory/lexical constants | Mostly pass | Hard | Ruby LSP either omits expected dynamic-language edges or returns declaration/same-name locations outside the contract. The individual result does not isolate one approximation mechanism. |
| `ruby-factory-return-member-call` | Pass | Hard | Bifrost satisfies the factory-result call contract; Ruby LSP misses the expected call and returns an extra declaration-like location. |
| `ruby-require-relative-class-construction` | Gap | Hard | Bifrost misses the class's self-construction; Ruby LSP finds it but also returns the class declaration, so neither is exact. |
| `ruby-singleton-field-access` | Exact | Exact | The only planned exact agreement among the Ruby cases. |

## Rust

| Case | Bifrost | rust-analyzer | Observed distinction |
|---|---|---|---|
| `rust-function-call-and-reexport` | Pass | Near | rust-analyzer additionally returns the re-export binding. |
| `rust-barrel-trait-static-qualifier` | Pass | Near | Bifrost finds the required qualifiers; rust-analyzer finds them plus re-export bindings. |
| `rust-ufcs-trait-method-through-barrel` | Pass | Hard | Bifrost satisfies the authored calls; rust-analyzer finds both calls but also returns the trait declaration. |
| `rust-struct-construction` | Legacy | Legacy | Human review now requires capital-`Self` type references and excludes lowercase `self`; rerun both analyzers against the corrected ground truth. |
| `rust-parity-module-declaration-definition` | Pass | Hard | rust-analyzer navigates to the module file start rather than the authored `mod workflow` declaration. |
| `rust-parity-direct-function-reference` | Exact | Exact | The direct declaration and call establish the non-macro control for the paired generated-function case. |
| `rust-parity-macro-generated-function-reference` | Gap | Exact | rust-analyzer resolves the generated declaration anchor and call exactly; Bifrost does not expand the declarative macro and misses both the usage and reverse definition. |

## Scala

| Case | Bifrost | Metals | Observed distinction |
|---|---|---|---|
| Two renamed/import-alias companion cases | Pass | Near | Metals additionally returns import-alias bindings. |
| `scala-parity-trait-method-implementation` | Pass | Hard | Metals omits the two expected implementation references. |
| `scala-companion-apply-call` | Pass | Hard | Metals omits the expected synthetic companion `apply` call reference. |

## What to isolate next

The highest-value minimal pairs are:

1. Interface-family grouping with two unrelated implementations and receiver
   contexts, to distinguish intentional symbol-family semantics from receiver
   or object insensitivity.
2. Ordered and branched factory assignments, to test whether CommonJS/Ruby
   misses are flow-sensitive, name-resolution, or return-summary boundaries.
3. Alias chains with and without re-export hops, to isolate binding identity
   from canonical declaration identity.
4. Proc-macro, derive-generated, synthetic-member, and configured-project
   equivalents using the same direct-versus-generated pairing now present for
   Rust declarative macros and C++ function-like macros.
