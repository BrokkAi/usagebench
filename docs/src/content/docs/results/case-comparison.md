---
title: Case comparison
description: Cases that separate Bifrost from the reference language servers in the synchronized 24 July 2026 run.
---

This page expands the **131 cases scoreable by both sides** in the synchronized
24 July development run. Exact means complete token ranges, no unallowed extras,
and the one reviewed navigation target. A language-server disagreement is a
contract result, not an automatic defect verdict.

Import, re-export, and export-metadata bindings are optional. They remain
visible in reports but do not make an otherwise exact case non-exact.

## Shared-case overview

| Language | Shared | Both exact | Bifrost only | LSP only | Neither |
|---|---:|---:|---:|---:|---:|
| C++ | 15 | 11 | 1 | 1 | 2 |
| C# | 16 | 11 | 3 | 2 | 0 |
| Go | 6 | 5 | 0 | 1 | 0 |
| Java | 11 | 3 | 8 | 0 | 0 |
| JavaScript | 8 | 3 | 3 | 2 | 0 |
| TypeScript | 9 | 8 | 1 | 0 | 0 |
| PHP | 10 | 9 | 1 | 0 | 0 |
| Python | 13 | 10 | 0 | 3 | 0 |
| Ruby | 16 | 5 | 10 | 0 | 1 |
| Rust | 15 | 11 | 3 | 1 | 0 |
| Scala | 12 | 8 | 2 | 1 | 1 |
| **Total** | **131** | **84** | **32** | **11** | **4** |

## Exact only for Bifrost

These 32 cases satisfy the reviewed contract in Bifrost but not in the
reference LSP. Nine LSP results are `position_unverified`; the remaining 23 are
hard disagreements.

| Language | Separating cases | Current distinction |
|---|---|---|
| C++ | `cpp-parity-concrete-override-method-call` | clangd expands the implementation family beyond the concrete receiver. |
| C# | `csharp-parity-interface-receiver-method-call`, `csharp-parity-concrete-implementation-method-call`, `csharp-parity-buffer-implementation-method-call` | Roslyn returns related interface/implementation-family calls beyond the reviewed static identity. |
| Java | `java-service-method-call`, `java-static-field-access`, `java-nested-class-constructor`, `java-parity-static-import-method-call`, `java-parity-interface-receiver-method-call`, `java-parity-concrete-implementation-method-call`, `java-lambda-body-member-call`, `java-static-qualified-method-call` | JDT LS has five imprecise target ranges and three hard declaration/family differences; Bifrost is exact on all eleven Java cases. |
| JavaScript | `js-class-construction`, `js-parity-commonjs-destructured-function-call`, `js-commonjs-barrel-class-construction` | TypeScript LS broadens constructor navigation in one case and omits two CommonJS edges. |
| TypeScript | `ts-default-class-import-and-construction` | TypeScript LS reaches the expected class plus an enclosing constructor-body range, so strict singleton navigation is position-unverified. |
| PHP | `php-class-construction` | Intelephense returns both the class and explicit constructor; Bifrost returns the reviewed class target alone. |
| Ruby | `ruby-relative-nested-constant`, `ruby-include-instance-mixin`, `ruby-prepend-method-precedence`, `ruby-top-level-implicit-self-method-call`, `ruby-singleton-method-dispatch`, `ruby-class-variable-access`, `ruby-parity-autoload-constant-definition`, `ruby-parity-attr-reader-method-call`, `ruby-parity-alias-method-call`, `ruby-factory-return-member-call` | Ruby LSP spans range, declaration-inclusion, mixin, alias, generated-reader, singleton, and factory-return boundaries. No single approximation label explains the group. |
| Rust | `rust-parity-associated-type-definition-no-movement`, `rust-parity-associated-type-use-definition`, `rust-ufcs-trait-method-through-barrel` | Bifrost keeps associated-type owner identity and the UFCS trait member narrower than rust-analyzer. |
| Scala | `scala-class-construction`, `scala-object-apply-call` | Bifrost separates class/companion identity and connects the visible application to the authored `apply` member. |

## Exact only for the language server

These 11 cases are the clearest current Bifrost parity backlog.

| Language | Separating cases | Current Bifrost gap |
|---|---|---|
| C++ | `cpp-parity-function-like-macro-expanded-call` | Does not navigate the visible macro argument to the expanded function definition. |
| C# | `csharp-parity-namespace-alias-constructor`, `csharp-parity-extension-method-call` | Navigates the alias to the namespace surface and does not resolve the extension receiver. |
| Go | `go-dot-import-concrete-receiver-call` | Misclassifies both imported concrete member selectors as shadowed local bindings. |
| JavaScript | `js-parity-computed-string-literal-method-call`, `js-commonjs-barrel-member-call` | Misses the computed string-literal call and the member immediately following `new Client()` through a destructured barrel. |
| Python | `python-module-import`, `python-parity-reexported-class-alias-classmethod`, `python-barrel-inherited-member-call` | Misses module declaration navigation, one alias-site usage, and a narrow inherited-member contract. |
| Rust | `rust-parity-macro-generated-function-reference` | Does not expand the declarative macro to recover the generated declaration and call. |
| Scala | `scala-parity-case-class-generated-construction-and-copy` | Handles construction but cannot resolve the generated `copy` receiver. |

## Exact for neither

| Language | Case | Current distinction |
|---|---|---|
| C++ | `cpp-class-reference` | Bifrost misses a required class usage; clangd adds constructor-family tokens. |
| C++ | `cpp-parity-using-alias-constructor` | Bifrost navigates declaration identity to the underlying class and lacks C++ type lookup; clangd misses the alias usage and adds constructor-family locations. |
| Ruby | `ruby-require-relative-class-construction` | Bifrost misses the class self-construction; Ruby LSP reaches the expected lines but not exact token ranges. |
| Scala | `scala-parity-trait-method-implementation` | Bifrost over-expands to a concrete call while Metals omits the reviewed implementation edge. |

## Capability boundary

Twenty-three cases are unsupported by the corresponding LSP profile because
the authored operation is not advertised: 6 Go, 5 JavaScript/TypeScript, 4 PHP,
4 Ruby, 3 Scala, and 1 C++. Bifrost exactly satisfies 17 of them, is non-exact
on 4, and shares the unsupported boundary on 2 configured-build cases.

Four runtime-driven cases are not planned for either side: one JavaScript, two
Python, and one Ruby case.

## What to isolate next

The highest-value additions are:

1. More compiler-generated and configured-project controls where LSPs should
   lead: macros, source generators, SDK symbols, conditional compilation, and
   synthetic members.
2. Interface-family minimal pairs with multiple implementations and receiver
   contexts, separating intentional symbol-family grouping from receiver
   sensitivity.
3. Alias and barrel chains with direct, one-hop, and two-hop controls.
4. Future competitor runners evaluated against these same frozen source
   contracts, without tool-specific scoring exceptions.
