---
title: Reproduce the comparison
description: Validate cases and run Bifrost or a pinned LSP profile against the same corpus.
---

## Validate the corpus

```bash
cargo test
cargo run -- validate benchmarks/cases
```

## Run Bifrost

Pin the Bifrost revision so the report records both the requested ref and the
resolved commit. The report also records the exact UsageBench source revision
and its release tag when the checkout is clean and tagged:

```bash
cargo run -- run-bifrost benchmarks/cases \
  --bifrost-repo ../bifrost \
  --bifrost-commit origin/master \
  --output benchmark-output/bifrost.json
```

## Run a language server

Profiles under `adapters/lsp/` pin the requested release and describe workspace
hydration, language IDs, readiness, and protocol extensions:

```bash
cargo run -- run-lsp benchmarks/cases \
  --profile adapters/lsp/rust-analyzer.json \
  --output benchmark-output/rust-analyzer.json
```

Use `--server-command` when the pinned executable is not on `PATH`. The report
retains the server-reported release separately; a local fallback must not
masquerade as the requested version.

## Cite a release

Use the repository root `CITATION.cff` for the latest citation metadata and pin
the `usagebenchRelease` value from the JSON report. Benchmark release tags are
separate from the Rust CLI version and YAML `schemaVersion`; the policy is in
`RELEASES.md`.

No DOI is claimed until an archival service assigns one. A real
version-specific DOI will be added to the citation metadata and release notes
when available.

## Preserve the evidence envelope

When publishing a comparison, retain:

- `usagebenchRevision` and `usagebenchRelease` from the report;
- requested and resolved analyzer versions;
- operating system and architecture;
- workspace bootstrap and settle configuration;
- the complete JSON reports, including diagnostics and capability fields; and
- whether unsupported and not-planned cases were included in a denominator.

The current site snapshot was captured on macOS arm64 on 2026-07-16. Its
Bifrost report resolved `origin/master` to
`4051809aea27b59accb2180a29a6ef2b365f1613`.
