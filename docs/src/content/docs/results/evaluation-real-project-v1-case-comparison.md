---
title: Historical v0.2.0 evaluation cases
description: Cases separating Bifrost from gopls, Pyright, and TypeScript language server in v0.2.0.
---

> **Evaluation partition only.** These cases come from the immutable
> [v0.2.0 release](https://github.com/BrokkAi/usagebench/releases/tag/v0.2.0)
> and are never pooled with development cases.

Exact means the tool returned the complete reviewed location set with exact
token ranges. A disagreement is a measured contract result, not automatically
a defect verdict; the reviewed source contract remains the reference.

## Strict-contract overview

| Reference profile | Shared | Both exact | Bifrost only | Reference only | Neither |
|---|---:|---:|---:|---:|---:|
| gopls 0.23.0 | 12 | 7 | 1 | 2 | 2 |
| Pyright 1.1.411 | 12 | 6 | 1 | 4 | 1 |
| TypeScript language server 5.3.0 | 12 | 5 | 5 | 2 | 0 |
| **Total** | **36** | **18** | **7** | **8** | **3** |

## Exact only for Bifrost

| Reference profile | Case file | Case |
|---|---|---|
| gopls | [`go-02.yaml`](https://github.com/BrokkAi/usagebench/blob/v0.2.0/benchmarks/cases/evaluation/real-project-v1/go-02.yaml) | `real-project-v1-go-02-2` |
| Pyright | [`python-03.yaml`](https://github.com/BrokkAi/usagebench/blob/v0.2.0/benchmarks/cases/evaluation/real-project-v1/python-03.yaml) | `real-project-v1-python-03-1` |
| TypeScript language server | [`typescript-01.yaml`](https://github.com/BrokkAi/usagebench/blob/v0.2.0/benchmarks/cases/evaluation/real-project-v1/typescript-01.yaml) | `real-project-v1-typescript-01-2` |
| TypeScript language server | [`typescript-02.yaml`](https://github.com/BrokkAi/usagebench/blob/v0.2.0/benchmarks/cases/evaluation/real-project-v1/typescript-02.yaml) | `real-project-v1-typescript-02-1` |
| TypeScript language server | [`typescript-04.yaml`](https://github.com/BrokkAi/usagebench/blob/v0.2.0/benchmarks/cases/evaluation/real-project-v1/typescript-04.yaml) | `real-project-v1-typescript-04-1` |
| TypeScript language server | [`typescript-04.yaml`](https://github.com/BrokkAi/usagebench/blob/v0.2.0/benchmarks/cases/evaluation/real-project-v1/typescript-04.yaml) | `real-project-v1-typescript-04-2` |
| TypeScript language server | [`typescript-04.yaml`](https://github.com/BrokkAi/usagebench/blob/v0.2.0/benchmarks/cases/evaluation/real-project-v1/typescript-04.yaml) | `real-project-v1-typescript-04-3` |

## Exact only for the reference server

These eight cases are the clearest current Bifrost parity backlog within this
evaluation slice.

| Reference profile | Case file | Case |
|---|---|---|
| gopls | [`go-01.yaml`](https://github.com/BrokkAi/usagebench/blob/v0.2.0/benchmarks/cases/evaluation/real-project-v1/go-01.yaml) | `real-project-v1-go-01-1` |
| gopls | [`go-02.yaml`](https://github.com/BrokkAi/usagebench/blob/v0.2.0/benchmarks/cases/evaluation/real-project-v1/go-02.yaml) | `real-project-v1-go-02-3` |
| Pyright | [`python-01.yaml`](https://github.com/BrokkAi/usagebench/blob/v0.2.0/benchmarks/cases/evaluation/real-project-v1/python-01.yaml) | `real-project-v1-python-01-1` |
| Pyright | [`python-03.yaml`](https://github.com/BrokkAi/usagebench/blob/v0.2.0/benchmarks/cases/evaluation/real-project-v1/python-03.yaml) | `real-project-v1-python-03-3` |
| Pyright | [`python-04.yaml`](https://github.com/BrokkAi/usagebench/blob/v0.2.0/benchmarks/cases/evaluation/real-project-v1/python-04.yaml) | `real-project-v1-python-04-2` |
| Pyright | [`python-04.yaml`](https://github.com/BrokkAi/usagebench/blob/v0.2.0/benchmarks/cases/evaluation/real-project-v1/python-04.yaml) | `real-project-v1-python-04-3` |
| TypeScript language server | [`typescript-01.yaml`](https://github.com/BrokkAi/usagebench/blob/v0.2.0/benchmarks/cases/evaluation/real-project-v1/typescript-01.yaml) | `real-project-v1-typescript-01-1` |
| TypeScript language server | [`typescript-02.yaml`](https://github.com/BrokkAi/usagebench/blob/v0.2.0/benchmarks/cases/evaluation/real-project-v1/typescript-02.yaml) | `real-project-v1-typescript-02-3` |

## Neither exact

Three cases are non-exact for both Bifrost and the corresponding reference
server. They remain visible in the raw reports and count in the denominator;
they are not silently dropped from the comparison.

The broader [historical development case comparison](../development-case-comparison/)
covers ten language-server profiles and 131 shared cases. It is useful for
regression history but has a different selection and review boundary.
