---
title: Overview
description: What UsageBench measures and why the comparison is deliberately cautious.
---

UsageBench evaluates source usage and navigation behavior on small, reviewed
fixtures. A case starts from a declaration location, lists expected usage
locations, and may probe whether those usages navigate back to the intended
declaration or type.

The immediate product goal is Bifrost conformance. Mature language servers are
used as a strong baseline because their behavior is familiar to developers and
grounded in language tooling. The human-reviewed source contract remains the
tie-breaker: Bifrost should match the LSP where that behavior is semantically
sound, while retaining justified precision improvements or additional static
coverage. Each accepted decision becomes a recurring regression test.

All 158 current cases have completed a first human review. They remain a
development corpus—not an independently reviewed evaluation set—and the
published aggregate predates the corrected contracts. The
[human ground-truth audit](../ground-truth-review/) explains that boundary.

The benchmark is analyzer-neutral. Cases do not contain Bifrost symbol IDs or
LSP-specific response shapes. Runners translate each tool's public interface
into a shared report containing exact locations, missing locations, unexpected
locations, navigation targets, capability levels, and diagnostics.

That neutral contract also leaves room for future competitors. Adding another
analyzer should require a runner and a versioned environment, not rewritten
expectations or a tool-specific scoring exception.

## What the benchmark can establish

- A runner returned or omitted a specific reviewed source location.
- A navigation request reached or missed a specific declaration or type.
- A difference was reproducible for a named analyzer release and fixture.
- An extra location is an import/re-export policy difference or remains an
  unexplained precision difference.

## What it cannot establish by itself

- That a language server is generally “wrong.” Its editor contract may group
  declarations, constructors, implementations, or aliases differently.
- That a miss was caused by flow insensitivity, object insensitivity, or another
  particular approximation. That requires an isolating minimal pair.
- That one architecture is faster or more scalable. Correctness fixtures do not
  measure indexing time, warm-query latency, or peak memory.
- That every real-world program construct is represented by the current corpus.

The [comparison methodology](../methodology/) defines the evidence threshold for
stronger claims. The [result snapshot](../results/) preserves the historical
Bifrost-versus-LSP run pending a synchronized rerun, and each language page
explains the reviewed case-level deltas.
