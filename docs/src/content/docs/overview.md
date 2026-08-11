---
title: Overview
description: What UsageBench measures and why the comparison is deliberately cautious.
---

UsageBench evaluates source usage and navigation behavior on small, reviewed
fixtures. A case starts from a declaration location, lists expected usage
locations, and may probe whether those usages navigate back to the intended
declaration or type.

The immediate product goal is Bifrost conformance. Established language servers
provide strong comparison evidence because their behavior is familiar to
developers and grounded in language tooling. They are not automatic ground
truth. The human-reviewed source contract remains the tie-breaker: Bifrost
should match the reference server where that behavior is semantically sound,
while retaining justified precision improvements or additional static
coverage. Each accepted decision becomes a recurring regression test.

## Different consumers, shared expectations

Language servers primarily serve a developer working through an editor. Their
broader contract includes completion, diagnostics, refactoring, and responsive
navigation in a configured workspace.

Bifrost serves repository code analysis and navigation as a machine-readable
substrate. Coding agents and static-analysis tool developers depend on stable
symbol identities, exact source locations, and queryable relationships to
understand and transform code safely. For these consumers, navigation is a
central analysis interface rather than one editor feature among many.

UsageBench deliberately measures the overlapping usage and navigation surface.
Reference-server comparison tests whether an agent loses navigation behavior
already available through established language tooling. Bifrost-specific wins
are reported only where they satisfy the reviewed contract; the benchmark does
not claim that Bifrost replaces an LSP's complete development-tooling surface.

UsageBench now publishes two deliberately separate evidence partitions. The
36-case `real-project-v1` slice is preregistered, independently reviewed,
source-locked, and published as immutable evaluation release
[`v0.2.0`](https://github.com/BrokkAi/usagebench/releases/tag/v0.2.0). The 158
fixture cases have completed one human review but remain an analyzer-informed
development corpus. Their broader 24 July run is retained as historical
regression evidence and is never pooled with the evaluation result. The
[human ground-truth audit](../ground-truth-review/) explains the distinct trust
boundaries.

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
stronger claims. The [current result](../results/) reports the frozen v0.2.0
evaluation, while the language pages explain case-level deltas from the broader
historical development run.
