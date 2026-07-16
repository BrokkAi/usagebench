---
title: Case comparison
description: Case-level disagreements between Bifrost and each measured language server.
---

This page lists non-exact cases. “Bifrost pass” means the pinned Bifrost run
satisfied the authored contract; “Bifrost gap” means a documented expected
failure. LSP near misses are policy-only binding/export extras. An LSP hard
result means contract disagreement, not an automatic defect verdict.

## C++

| Case | Bifrost | clangd | Observed distinction |
|---|---|---|---|
| `cpp-function-call`, `cpp-method-call`, `cpp-out-of-line-member-call`, `cpp-parity-overload-string-function-call`, `cpp-overload-string-call-is-narrow` | Pass | Hard | clangd omits the expected out-of-line definition from references with declarations excluded. |
| `cpp-class-reference` | Pass | Hard | clangd includes constructor-family locations in the class reference set. |
| `cpp-constructor-call` | Pass | Hard | Reverse navigation returns the related class surface rather than the authored constructor declaration. |
| `cpp-parity-using-alias-constructor` | Pass | Hard | Missing reference plus alias/class navigation difference. |
| `cpp-parity-concrete-override-method-call` | Pass | Hard | Missing expected call and extra related declaration/implementation locations. |
| `cpp-parity-compile-commands-unsupported` | Unsupported | Unsupported | Deliberately outside the current portable corpus boundary. |

## C#

| Case | Bifrost | Roslyn | Observed distinction |
|---|---|---|---|
| `csharp-generic-extension-call` | Gap | Exact | Bifrost retains the object-created receiver edge only as unproven; Roslyn returns the expected reference. |
| `csharp-parity-interface-receiver-method-call` | Pass | Hard | Roslyn includes a related implementation-family call. |
| `csharp-parity-concrete-implementation-method-call` | Pass | Hard | Roslyn includes a related interface/implementation-family call. |
| `csharp-parity-namespace-alias-constructor` | Pass | Hard | Reverse navigation resolves to the namespace alias binding rather than the underlying class declaration. |

## Go

| Case | Bifrost | gopls | Observed distinction |
|---|---|---|---|
| `go-interface-receiver-method-call` | Gap | Hard | Both broaden the interface method to two concrete-receiver calls; Bifrost records those candidates as unproven while gopls returns ordinary reference locations. |

## Java

| Case | Bifrost | JDT LS | Observed distinction |
|---|---|---|---|
| `java-service-class-construction` | Gap | Exact | Bifrost misses two `Service` qualifiers in `Service.Repository`. |
| `java-nested-class-constructor` | Gap | Exact | Bifrost misses the nested `Repository` field and constructor-parameter type usages. |
| `java-parity-static-import-method-call` | Pass | Near | JDT LS additionally returns the static-import binding. |
| `java-parity-concrete-implementation-method-call` | Pass | Hard | JDT LS includes a call on an anonymous implementation when querying the concrete method. |

## JavaScript and TypeScript

| Case group | Bifrost | TypeScript LS | Observed distinction |
|---|---|---|---|
| Nine ES import/re-export cases | Pass | Near | Required references agree; TypeScript LS also returns binding/export locations. |
| `js-parity-commonjs-destructured-function-call` | Pass | Hard | TypeScript LS omits the destructured CommonJS function call. |
| `js-commonjs-barrel-class-construction` | Pass | Hard | TypeScript LS omits the construction reached through the CommonJS barrel. |
| `js-commonjs-barrel-member-call` | Gap | Exact | Bifrost retains the factory-result member call only as unproven; TypeScript LS returns the expected reference. |

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
| `ruby-factory-return-member-call` | Gap | Hard | Bifrost returns the factory-result call only as unproven; Ruby LSP misses the expected call and returns an extra declaration-like location. |
| `ruby-require-relative-class-construction` | Gap | Hard | Bifrost misses the class's self-construction; Ruby LSP finds it but also returns the class declaration, so neither is exact. |
| `ruby-singleton-field-access` | Exact | Exact | The only planned exact agreement among the Ruby cases. |

## Rust

| Case | Bifrost | rust-analyzer | Observed distinction |
|---|---|---|---|
| `rust-function-call-and-reexport` | Pass | Near | rust-analyzer additionally returns the re-export binding. |
| `rust-barrel-trait-static-qualifier` | Gap | Near | rust-analyzer finds every required qualifier plus re-export bindings; Bifrost misses the chained re-export qualifiers. |
| `rust-ufcs-trait-method-through-barrel` | Gap | Hard | rust-analyzer finds both required calls but also returns the trait declaration; Bifrost misses the calls. |
| `rust-struct-construction` | Pass | Hard | rust-analyzer adds a re-export plus `Self`/declaration-like locations; only the re-export is policy-allowed. |
| `rust-parity-module-declaration-definition` | Pass | Hard | rust-analyzer navigates to the module file start rather than the authored `mod workflow` declaration. |

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
4. Macro/generated equivalents of an otherwise identical direct declaration,
   so compiler expansion support is represented rather than merely discussed.
