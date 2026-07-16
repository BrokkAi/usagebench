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
- support for declaration-to-usage, usage-to-declaration, and type lookup;
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
the reverse probes. A git revision is resolved before execution and recorded in
the report.

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
4. queries `textDocument/references` with `includeDeclaration: false`,
   `textDocument/definition`, and `textDocument/typeDefinition` when advertised;
5. keeps processing bidirectional server requests while the workspace settles;
6. normalizes `Location` and `LocationLink` responses to repository-relative,
   one-based UsageBench report locations.

The adapter does not silently reinterpret LSP results. Import bindings, aliases,
header declarations, implementations, and generated locations remain in the
report exactly as returned. Scoring exposes two successful-coverage tiers:

- `passed` is an exact reference match with successful reverse/type lookups;
- `near_miss` has every required reference and successful reverse/type lookups,
  but the server returns a complete superset of reference locations.

A near miss is not counted as an exact precision pass or a hard failure. This
keeps Bifrost's product decision to omit binding-only imports measurable without
punishing an LSP whose “find references” semantics include them. For
multi-target definition and type-definition responses, the lookup passes when
the expected target is among the returned locations; every alternate is still
recorded. Missing required locations, absent definitions, wrong targets,
partial responses, and protocol failures remain hard failures or errors.

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

### Popular LSP comparison

The following end-to-end runs were captured on macOS arm64 on 2026-07-15 after
workspace hydration and active bidirectional request handling were enabled.
“Planned” is `exact + near miss + hard failure`; not-planned and unsupported
cases are displayed separately. All ten servers completed with zero runner
errors.

| Corpus language(s) | Server | Requested release | Server-reported release | Exact | Near miss | Hard failure | Not planned | Unsupported | Errors |
|---|---|---|---|---:|---:|---:|---:|---:|---:|
| C++ | clangd | 22.1.6 | Apple clangd 21.0.0 | 4 | 1 | 8 | 0 | 1 | 0 |
| Go | gopls | 0.23.0 | v0.23.0 | 9 | 1 | 0 | 0 | 1 | 0 |
| Rust | rust-analyzer | 2026-07-13 | 0.3.2971-standalone | 6 | 4 | 2 | 1 | 0 | 0 |
| JavaScript, TypeScript | typescript-language-server | 5.3.0 + TypeScript 5.9.3 | not reported | 10 | 9 | 2 | 1 | 0 | 0 |
| Python | Pyright | 1.1.411 | not reported | 6 | 5 | 1 | 2 | 0 | 0 |
| PHP | Intelephense | 1.18.5 | not reported | 9 | 1 | 2 | 1 | 0 | 0 |
| Ruby | Ruby LSP | 0.26.10 | 0.26.10 | 1 | 11 | 8 | 1 | 0 | 0 |
| Java | Eclipse JDT LS | 1.61.0-202607142124 | 1.61.0-SNAPSHOT | 7 | 4 | 0 | 0 | 0 | 0 |
| C# | Roslyn language server | vscode-csharp 2.140.9 | not reported | 4 | 0 | 10 | 1 | 0 | 0 |
| Scala | Metals | 1.6.7 | 1.6.7 | 8 | 2 | 2 | 1 | 0 | 0 |
| **Total** | **10 measured servers** |  |  | **64** | **38** | **35** | **8** | **2** | **0** |

The clangd row deliberately records the actual system server used: the profile
requested upstream 22.1.6, but this machine resolved Apple clangd 21.0.0. It
should not be read as a 22.1.6 result. The unmeasured `csharp-ls` 0.26.0 profile
is also included for reproducibility, but its published NuGet tool package
could not be installed because it lacked `DotnetToolSettings.xml`; Roslyn is
the measured C# implementation.

The strongest compatible-coverage results were gopls at 10/10 and JDT LS at
11/11. TypeScript reached 19/21, Pyright 11/12, and Ruby LSP 12/20 once complete
supersets stopped being labeled hard failures. Metals improved from 2/12 to
8 exact + 2 near misses after the runner accepted its build-import prompt while
continuing to service server requests; its own earlier log had reported “no
build target found.”

Roslyn's profile now restores the project, sends the official `project/open`
notification, waits for `workspace/projectInitializationComplete`, advertises
the `_vs_projectContext` capability, queries the default project context, and
attaches it to navigation requests. The baseline fixture builds successfully,
but cross-file `Consumer.cs` lookups remain absent, so its 4/14 result did not
receive speculative credit. These are fixture-specific protocol results rather
than a general ranking of editor quality. Installation and exact reproduction
details are in `adapters/lsp/README.md`.

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
