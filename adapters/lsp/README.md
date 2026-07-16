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

A non-zero exit after a completed run means one or more cases had a hard
failure; the JSON report is still written. A `near_miss` has complete references
and successful navigation, with only classified import bindings, re-export
bindings, or export metadata as extras. Every extra remains visible in `actual`
and is described in `extraUsages`. Any other superset is a hard failure.
Startup, protocol, and query failures are counted as runner errors separately
from incorrect results.

## Included profiles

| Profile | Corpus language(s) | Requested release | Default command source |
|---|---|---|---|
| `clangd.json` | C++ | 22.1.6 | installed `clangd` |
| `roslyn.json` | C# | vscode-csharp 2.140.9 | extracted official C# extension server |
| `csharp-ls.json` | C# | 0.26.0 | installed `csharp-ls` |
| `gopls.json` | Go | 0.23.0 | installed `gopls` |
| `eclipse-jdtls.json` | Java | 1.61.0-202607142124 | installed `jdtls` launcher |
| `typescript-language-server.json` | JavaScript, TypeScript | 5.3.0 + TypeScript 5.9.3 | exact packages through `npx` |
| `pyright.json` | Python | 1.1.411 | exact package through `npx` |
| `ruby-lsp.json` | Ruby | 0.26.10 | installed `ruby-lsp` |
| `metals.json` | Scala | 1.6.7 | exact artifact through Coursier `cs` |
| `intelephense.json` | PHP | 1.18.5 | exact package through `npx` |
| `rust-analyzer.json` | Rust | 2026-07-13 | installed `rust-analyzer` |

Representative exact installation commands are:

```bash
go install golang.org/x/tools/gopls@v0.23.0
gem install ruby-lsp -v 0.26.10
gem install debug -v 1.11.1
```

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
- `queryDeclaration` explicitly opts a profile into
  `textDocument/declaration` before the normal definition query when the server
  advertises it; this is enabled only where the release's behavior has been
  verified because some servers advertise the method without completing it;
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
