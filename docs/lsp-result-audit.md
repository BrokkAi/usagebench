# LSP result audit

This audit accompanies the 2026-07-16 comparison in
[`runner-adapters.md`](runner-adapters.md). It distinguishes policy differences
from correctness differences instead of treating every reference superset as a
near miss.

This is a historical audit of the former line-level scorer. It must not be read
as a schema-v2 evaluation result; its policy-near-miss labels are retained only
to explain the legacy snapshot.

## Classification rules

- **Allowed policy extra:** an import binding, re-export binding, or export
  metadata location returned by `textDocument/references`. These locations are
  preserved in both `actual` and `extraUsages`; they produce `near_miss` only
  when required references and navigation also pass.
- **FP-like difference:** any other unexpected location, including a declaration
  returned despite `includeDeclaration: false`, same-name symbol expansion, or
  an implementation-family result. It remains a hard failure.
- **FN-like difference:** an expected reference or navigation target that is
  absent. It remains a hard failure.
- **Corpus error:** inspection shows that the source contains a real usage that
  the authored expectation omitted. The expectation is corrected and the LSP
  is not penalized.

“FP-like” and “FN-like” describe disagreement with UsageBench's task contract;
they do not claim that the server's broader editor semantics are defective.

## Findings by server

| Server | Allowed policy extras | Remaining FP-like differences | Remaining FN-like or navigation differences |
|---|---|---|---|
| clangd | None in the measured cases | Human review confirmed that `cpp-class-reference` includes constructor declaration/definition tokens even though the C++ language and Clang AST classify them separately; the using-alias case likewise includes the constructor declaration. The reviewed concrete-override query broadens to the pure-virtual base declaration and base-typed call. | `cpp-function-call`, `cpp-constructor-call`, `cpp-method-call`, `cpp-out-of-line-member-call`, both reviewed overload cases, the reviewed virtual-base call, explicit template-function call, direct inline-function control, and macro-expanded function call are exact after human review corrected their usage/navigation contracts. In the reviewed using-alias case, declaration navigation to the alias and type-definition navigation to the underlying class are both exact, but references omit the two out-of-line class qualifiers and the construction through the alias. Definition navigation from the concrete override call stops at the header declaration rather than its out-of-line body. |
| gopls | None | `go-interface-receiver-method-call` includes two concrete-receiver calls while querying the interface method | None |
| rust-analyzer | Re-export bindings in two cases | The reviewed `rust-struct-construction` ground truth now requires capital-`Self`; rerun to isolate any remaining receiver/declaration-like extras. `rust-ufcs-trait-method-through-barrel` includes the trait declaration. | Module navigation opens `workflow.rs` rather than returning the authored `mod workflow` declaration |
| TypeScript language server | Import and re-export bindings in nine cases | None | CommonJS destructuring and a CommonJS barrel omit the expected call/construction reference |
| Pyright | Import bindings, re-export bindings, and `__all__` metadata in four cases | The re-exported class-alias case returns one original-symbol location outside the allowed binding policy | The same alias case omits two alias-site references; the zero-width module selector is unsupported because it has no source token |
| Intelephense | One imported-function binding | None | Interface implementation and interface-typed receiver cases do not connect the expected implementation/call; one reverse lookup also misses |
| Ruby LSP | None | Most complete supersets were declaration sites returned despite `includeDeclaration: false`, with additional same-name/cross-symbol expansion in singleton and prepend cases | Missing references cover mixins, constants, class variables, autoload, generated attribute readers, aliases, and other dynamic dispatch cases |
| Eclipse JDT LS | One static-import binding | `java-parity-concrete-implementation-method-call` includes a call on an anonymous implementation when querying the concrete method | None after correcting two Java cases whose expectations omitted real type usages |
| Roslyn | None | Interface and concrete method queries include one related implementation-family call each | Namespace-alias constructor navigation resolves to the alias binding rather than the underlying class declaration |
| Metals | Import-alias bindings in two cases | None | Trait implementation references and the synthetic companion `apply` call are absent |

## Corpus corrections discovered during the audit

- `java-service-class-construction` omitted two real `Service` qualifiers in
  `Service.Repository`.
- `java-nested-class-constructor` omitted the `Repository` field and constructor
  parameter type usages.
- `ruby-require-relative-class-construction` omitted the real `Invoice`
  self-construction in `Invoice.build`.

These corrections are analyzer-neutral source facts. They are included in the
ground truth even if another runner subsequently exposes a new gap against
them.

The configured C++ parity case is intentionally separated from the default
comparison: `clangd-configured.json` activates `ENABLE_PARITY_FEATURE` and
passes the guarded usage/declaration regression with `--include-unsupported`,
while ordinary clangd fails the same assertions with the branch inactive.
Default cross-tool reports continue to classify the case as unsupported rather
than counting the opt-in regression result in their totals.
