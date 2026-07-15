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

## Repowise v0.31.0 feasibility

The first researched peer release is
[`repowise-dev/repowise` v0.31.0](https://github.com/repowise-dev/repowise/releases/tag/v0.31.0),
resolved at commit `068c2808641aa8c865bc2396723bc6b07d076ada`.

Repowise advertises symbol-level call resolution and exposes caller/callee
relationships through `get_context(include=["callers", "callees"])`. Its
release implementation also extracts a one-based `CallSite.line` while parsing.
However, the graph builder collapses calls between the same caller and callee
to one edge without persisting the call-site line. The public MCP response then
returns the *caller declaration line*, not the call-site line:

- [`CallSite` retains the source line during parsing](https://github.com/repowise-dev/repowise/blob/v0.31.0/packages/core/src/repowise/core/ingestion/models.py#L215-L226)
- [the graph edge drops that line](https://github.com/repowise-dev/repowise/blob/v0.31.0/packages/core/src/repowise/core/ingestion/graph/_resolvers.py#L481-L507)
- [`get_context` returns the other symbol's definition line](https://github.com/repowise-dev/repowise/blob/v0.31.0/packages/server/src/repowise/server/mcp_server/tool_context/enrichment.py#L153-L169)

Consequences for UsageBench:

| Operation | v0.31.0 public surface | Adapter policy |
|---|---|---|
| Declaration to call sites | Caller symbols, capped at 50, with confidence | Recover candidate call tokens inside each caller body and disclose the recovery method |
| Non-call references | No exhaustive source-location reference query | Unsupported |
| Usage to declaration | Callees of an enclosing caller symbol | Supported only for call expressions that can be located unambiguously |
| Type lookup | No source-location type query | Unsupported |

This is a real capability boundary, not evidence that the internal call graph is
wrong. The benchmark must not substitute caller definition lines for usage
locations or infer reference precision from Repowise's code-health results.

An end-to-end probe on 2026-07-15 ran v0.31.0 against
`benchmarks/cases/rust-baseline.yaml`. The adapter completed with no runner
errors: one case passed, two produced ordinary benchmark failures, and three
were reported unsupported. Notably, the function-call/re-export case passed;
the failures exposed Repowise collapsing a Rust struct declaration into an
`impl` symbol location and omitting one method edge. These are preliminary
single-fixture observations, not a competitive conclusion.

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
