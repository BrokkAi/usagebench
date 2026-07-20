# LSP runner profiles

These profiles adapt standard language-server protocol operations to the
UsageBench report contract. They cover every language currently present in the
curated corpus.

## Run a profile

Put the profile's executable on `PATH`, then run:

```bash
cargo run -- run-lsp benchmarks/cases \
  --profile adapters/lsp/gopls.json \
  --output benchmark-output/gopls-v0.23.0.json
```

Useful options:

- `--server-command /absolute/path/to/server` replaces the first command item
  but preserves the profile's arguments;
- `--case-id CASE_ID` runs one matching case while debugging;
- `--include-unsupported` runs cases carrying corpus-level unsupported markers;
- `--keep-worktrees` retains isolated workspaces under `target/usagebench/lsp`.

A non-zero exit after a completed run means one or more cases failed or could
not be position-verified; the JSON report is still written. Exact scoring
requires complete token ranges. Under `bindings_optional`, classified import
bindings, re-export bindings, and export metadata remain visible in `actual`
and `extraUsages` without preventing a pass. Any other superset is a hard
failure.
Startup, protocol, and query failures are counted as runner errors separately
from incorrect results.

## Included profiles

| Profile | Corpus language(s) | Requested release | Default command source |
|---|---|---|---|
| `clangd.json` | C++ | 22.1.6 | installed `clangd` |
| `ccls.json` | C++ | 0.20250815.1 | installed `ccls` |
| `roslyn.json` | C# | vscode-csharp 2.140.9 | extracted official C# extension server |
| `csharp-ls.json` | C# | 0.26.0 | installed `csharp-ls` |
| `gopls.json` | Go | 0.23.0 | installed `gopls` |
| `eclipse-jdtls.json` | Java | 1.61.0-202607142124 | installed `jdtls` launcher |
| `typescript-language-server.json` | JavaScript, TypeScript | 5.3.0 + TypeScript 5.9.3 | exact packages through `npx` |
| `pyright.json` | Python | 1.1.411 | exact package through `npx` |
| `ruby-lsp.json` | Ruby | 0.26.10 | installed `ruby-lsp` |
| `solargraph.json` | Ruby | 0.60.2 | installed `solargraph` |
| `metals.json` | Scala | 1.6.7 | exact artifact through Coursier `cs` |
| `intelephense.json` | PHP | 1.18.5 | exact package through `npx` |
| `phpactor.json` | PHP | 2026.06.25.0 | installed `phpactor` |
| `rust-analyzer.json` | Rust | 2026-07-13 | installed `rust-analyzer` |

Representative exact installation commands are:

```bash
go install golang.org/x/tools/gopls@v0.23.0
gem install ruby-lsp -v 0.26.10
gem install debug -v 1.11.1
gem install solargraph -v 0.60.2
```

The ccls, Solargraph, and Phpactor profiles are reproducible candidate adapters,
not measured rows in the legacy snapshot. They were added because they are
distinct implementations for C++, Ruby, and PHP with documented stdio LSP and
reference/navigation support. Phpactor should be installed from its pinned
release artifact; ccls currently requires a platform package or source build.
Basedpyright was considered for Python but not added as a second implementation:
it is explicitly a Pyright fork and would mostly measure the same navigation
engine rather than independent corroboration.

### Second-implementation coverage

| Corpus language | Independent second profile | Status |
|---|---|---|
| C++ | ccls | Added, unmeasured |
| C# | csharp-ls | Existing, installation blocker documented |
| PHP | Phpactor | Added, unmeasured |
| Ruby | Solargraph | Added, unmeasured |
| Python | None accepted | Basedpyright is a Pyright fork |
| JavaScript, TypeScript | None accepted | vtsls and similar servers wrap the TypeScript engine rather than independently corroborating it |
| Go, Java, Rust, Scala | None accepted in this pass | No candidate was both independently implemented and reproducibly verified for this harness yet |

This matrix is deliberately about implementation diversity, not profile count.
A wrapper around the same semantic engine can still be useful for editor
integration, but it is weak evidence against a methodology or harness bug.

The npm profiles need Node.js and fetch their pinned packages themselves.
Metals needs Coursier's `cs` launcher and performs a real
[SBT compile/import](https://scalameta.org/metals/docs/build-tools/sbt/).
clangd, rust-analyzer, Eclipse JDT LS,
and the official Roslyn server should be downloaded from the release URL in
their profile and placed on `PATH`; the Roslyn executable is under `.roslyn/`
in the official C# extension archive.

At the time of the 2026-07-15 probe, `dotnet tool install csharp-ls --version
0.26.0` failed because the published package lacked
`DotnetToolSettings.xml`. The profile documents the intended invocation, but
the measured C# row uses the official Roslyn server. The runner always records
the requested and server-reported versions independently so a system fallback
cannot silently masquerade as the requested release.

## Profile contract

Profiles are JSON objects with these core fields:

- `id`, `name`, `languages`, `requestedVersion`, and `source` identify the run;
- `command` is the stdio server command and arguments;
- `fileExtensions` and `languageIds` determine which fixture files are opened;
- `initializationOptions` and `configuration` are sent through LSP;
- `clientCapabilities`, `postInitializeNotifications`, and
  `projectContextRequest` describe profile-specific protocol extensions;
- `acceptFirstActionRequests` lets an isolated benchmark accept a server's
  first setup action, such as Metals' documented “Import build” prompt;
- `queryDeclaration` explicitly selects `textDocument/declaration` instead of
  `textDocument/definition` when the server advertises it; the two response sets
  are never unioned;
- `environment` supports `{workspace}` and `{runDir}` substitutions;
- `workspaceFiles` supplies missing project bootstrap files without replacing
  fixture-owned files;
- `prepareCommand` and `prepareTimeoutMilliseconds` hydrate or restore a
  workspace before the server starts;
- `readinessNotification` and `readinessTimeoutMilliseconds` wait for an
  explicit project-loaded signal when the server provides one;
- `settleMilliseconds` and `requestTimeoutMilliseconds` tune server startup;
- `generateCompileCommands` creates a minimal C/C++ compilation database.

During the settle window the runner continues answering bidirectional server
requests instead of sleeping; this is necessary for interactive build-import
flows. Each matching benchmark document gets a fresh server and isolated
workspace. Profiles are therefore comparable for correctness, but the current
report does not measure warm-start or request latency. See
[`docs/runner-adapters.md`](../../docs/runner-adapters.md) for the measured
correctness table and the exact scoring semantics.
