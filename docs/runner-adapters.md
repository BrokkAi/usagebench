# Runner adapters

UsageBench cases are analyzer-neutral, but every analysis tool exposes a
different query and release surface. Runner adapters own that translation.
They prepare an exact tool version, query a fixture source root, and normalize
the result to UsageBench's source-location report model.

## Contract

An adapter must record:

- the tool name and requested version or revision;
- the resolved immutable version or commit actually executed;
- the tool repository or distribution source;
- support for declaration-to-usage, declaration navigation, definition
  navigation, and type lookup;
- any location-recovery step that is not part of the tool's native output;
- unsupported and abstained operations separately from empty successful
  results.

Adapters live under `src/runners/`. Shared report and scoring code belongs in
`src/runners/mod.rs`; tool setup, protocol details, and response normalization
belong in one module per tool. A release-specific adapter may reject versions
whose public response shape it does not understand. It must not silently parse
an unverified version as if it were compatible.

## Bifrost

The Bifrost adapter uses its public MCP tools. `search_symbols` maps authored
declaration locations to a Bifrost selector, `scan_usages_by_location` provides
usage locations, and the location-based definition and type tools implement
definition and type reverse probes. Bifrost does not currently expose a
distinct declaration-navigation tool, so explicit `operation: declaration`
lookups are reported as unsupported rather than falling back to definition. A
git revision is resolved before execution and recorded in the report. When the
document permits bindings, the adapter requests
`include_bindings: true`; compatible Bifrost releases may include imports and
re-exports. Releases that return only path/line locations are reported as
`position_unverified` until they expose exact token ranges. Hits explicitly
labeled `override_declaration` count only when the authored case expects or
allows that declaration; other override declarations are excluded from
ordinary usage scoring and recorded as `override_declarations_excluded` in
`rawStatuses`. Unlabeled supersets remain failures.

The original `usagebench::bifrost_runner` module remains as a compatibility
re-export while the implementation lives in `usagebench::runners::bifrost`.

## Scope

UsageBench compares Bifrost with language servers on the LSP-shaped task of
finding symbol references and navigating those references back to declarations
and types. Tools centered on a different analysis contract do not belong behind
private recovery hooks here. Call-graph resolution, taint-flow analysis, and
similar capabilities should be evaluated by focused sibling suites such as a
future `callbench` or `taintbench`.

## Generic LSP adapter

The LSP adapter is data-driven: JSON profiles under `adapters/lsp/` select the
benchmark languages, requested release, stdio command, language IDs,
initialization/configuration values, and minimal project files needed to make a
fixture a real workspace. One profile can cover more than one corpus language,
as typescript-language-server does for JavaScript and TypeScript.

For every matching benchmark document the adapter:

1. copies the fixture into an isolated workspace;
2. writes only missing profile bootstrap files such as `go.mod`, `Cargo.toml`,
   or a project file;
3. performs LSP initialization, answers bidirectional server requests, and
   opens all matching source documents;
4. queries `textDocument/references` with `includeDeclaration: false`; for
   navigation it uses `textDocument/declaration` when explicitly enabled and
   advertised, otherwise `textDocument/definition`, without unioning the two;
   and queries `textDocument/typeDefinition` when advertised;
5. keeps processing bidirectional server requests while the workspace settles;
6. normalizes `Location` and `LocationLink` responses to repository-relative,
   one-based UsageBench report locations.

The adapter does not silently discard LSP results. Import bindings, re-export
bindings, export metadata, declarations, implementations, aliases, and
generated locations remain in `actual`. Every extra location is also recorded
in `extraUsages` with a classification, disposition, and rationale.

Scoring requires exact start and end ranges. A path/line-only result is
`position_unverified`, not a pass. The document's `referencePolicy` decides
whether classified binding/export locations are excluded, optional, or
required. Optional binding extras remain visible but do not prevent `passed`;
any other superset is a hard failure.

Definition and type-definition navigation passes only when the response has one
location and its complete range exactly matches the authored target. An
expected target hidden among alternates is a failure. Missing required
locations, partial responses, and protocol failures remain failures or errors.

Profiles record both a requested version and the server's `serverInfo.version`
when available. Servers that omit it are reported as `not reported`; the
adapter never substitutes the requested value. A command override is likewise
visible through this requested/resolved split. Operations a server does not
advertise are reported as unsupported, not as an empty successful result.

Run a profile with:

```bash
cargo run -- run-lsp benchmarks/cases \
  --profile adapters/lsp/rust-analyzer.json \
  --output benchmark-output/rust-analyzer.json
```

`--case-id` narrows diagnosis to one case. `--include-unsupported` opts into
cases carrying corpus-level unsupported markers; it is intentionally off for
the comparison below.

### Legacy popular-LSP comparison

The following end-to-end runs were captured on macOS arm64 on 2026-07-16 with
the former line-level scorer and former binding-policy near-miss category. They
are retained as a historical diagnostic snapshot, not a schema-v2 evaluation
result. Re-run every profile before publishing a hardened aggregate. The legacy
runs were captured after workspace hydration and active bidirectional request handling were enabled.
“Near miss” is limited to the allowed binding/export-policy extras above;
declarations, same-name symbols, hierarchy expansion, and other supersets stay
in hard failure. “Planned” is `exact + near miss + hard failure`; not-planned
and unsupported cases are displayed separately. All ten servers completed with
zero runner errors.

| Corpus language(s) | Server | Requested release | Server-reported release | Exact | Near miss | Hard failure | Not planned | Unsupported | Errors |
|---|---|---|---|---:|---:|---:|---:|---:|---:|
| C++ | clangd | 22.1.6 | Apple clangd 21.0.0 | 6 | 0 | 9 | 0 | 1 | 0 |
| Go | gopls | 0.23.0 | v0.23.0 | 9 | 0 | 1 | 0 | 1 | 0 |
| Rust | rust-analyzer | 2026-07-13 | 0.3.2971-standalone | 9 | 2 | 3 | 0 | 0 | 0 |
| JavaScript, TypeScript | typescript-language-server | 5.3.0 + TypeScript 5.9.3 | not reported | 10 | 9 | 2 | 1 | 0 | 0 |
| Python | Pyright | 1.1.411 | not reported | 6 | 4 | 1 | 2 | 1 | 0 |
| PHP | Intelephense | 1.18.5 | not reported | 9 | 1 | 2 | 1 | 0 | 0 |
| Ruby | Ruby LSP | 0.26.10 | 0.26.10 | 1 | 0 | 19 | 1 | 0 | 0 |
| Java | Eclipse JDT LS | 1.61.0-202607142124 | 1.61.0-SNAPSHOT | 9 | 1 | 1 | 0 | 0 | 0 |
| C# | Roslyn language server | vscode-csharp 2.140.9 | not reported | 11 | 0 | 3 | 1 | 0 | 0 |
| Scala | Metals | 1.6.7 | 1.6.7 | 8 | 2 | 2 | 1 | 0 | 0 |
| **Total** | **10 measured servers** |  |  | **78** | **19** | **43** | **7** | **3** | **0** |

The clangd row deliberately records the actual system server used: the profile
requested upstream 22.1.6, but this machine resolved Apple clangd 21.0.0. It
should not be read as a 22.1.6 result. The unmeasured `csharp-ls` 0.26.0 profile
is also included for reproducibility, but its published NuGet tool package
could not be installed because it lacked `DotnetToolSettings.xml`; Roslyn is
the measured C# implementation.

The strongest exact-or-policy-allowed coverage is JDT LS at 10/11, gopls at
9/10, TypeScript at 19/21, Metals at 10/12, and Pyright at 10/11. These are not
generic editor-quality rankings: the hard-failure bucket includes distinct
choices such as implementation-family expansion as well as missing results.
The case-level [LSP result audit](lsp-result-audit.md) records the observed cause
of each remaining FP-like or FN-like difference.

Roslyn's profile now restores the project, sends the official `project/open`
notification, waits for `workspace/projectInitializationComplete`, advertises
the `_vs_projectContext` capability, queries the default project context, and
attaches it to navigation requests. With the full project load it resolves 11
cases exactly; the remaining three are alias or implementation-family semantic
differences. JDT LS also exposed omissions in two authored Java cases, and Ruby
LSP exposed one in Ruby; those source-level usages are now
part of the corpus instead of being counted against the servers. These are
fixture-specific protocol results rather than a general ranking of editor
quality. Installation and exact reproduction details are in
`adapters/lsp/README.md`.

## Adding another adapter

1. Pin and verify one release before accepting a version range.
2. Prefer a documented CLI, MCP, LSP, or export contract over private storage.
3. Normalize paths relative to the benchmark source root and locations to
   one-based report lines.
4. Preserve confidence or ambiguity as proven/unproven/partial output instead
   of forcing a single answer.
5. Add parser tests using captured public responses; live installation and
   indexing tests should be opt-in when they require network access.
6. Document missing operations. An honest reproducible blocker is a benchmark
   result; fabricated glue is not.
