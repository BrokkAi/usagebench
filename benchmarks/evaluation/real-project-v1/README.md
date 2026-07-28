# Real-project evaluation v1 protocol

This directory is the preregistration boundary for the first real-project
slice. `protocol.json` was written before repositories, commits, declarations,
ground-truth locations, Bifrost results, or reference-language-server results
were inspected.

The initial slice has three source-language/reference-profile strata:

| Language | Profile | Repositories | Declarations per repository |
| --- | --- | ---: | ---: |
| Go | `gopls` | 4 | 3 |
| Python | `pyright` | 4 | 3 |
| TypeScript | `typescript-language-server` | 4 | 3 |

That produces 36 planned declarations. A repository or declaration can be
replaced only through the deterministic, source-only procedure in the protocol;
an analyzer result is never a reason to select, exclude, replace, or alter a
case.

## Selection procedure

1. Capture the GitHub API population in `population.json`, including the exact
   request URLs, retrieval timestamp, pagination, repository metadata, and the
   default-branch commit observed at capture time. Commit that file before
   drawing any repository.
2. Apply the recorded eligibility and exclusion checks. Preserve the complete
   ranked candidate list, including exclusions and reasons, in `selection.json`.
3. Archive each selected exact Git commit with `git archive`; record the commit,
   tree, relative archive path, and SHA-256 in `sources.json`.
4. Author YAML documents under `benchmarks/cases/evaluation/real-project-v1/`
   with portable `benchmark://source/...` locations. Each document must point
   at `selection.json`, `review.json`, and `sources.json` through its corpus
   metadata.
5. Have two reviewers independently derive the expected locations from the
   source archive, record their signed review artifacts, adjudicate differences,
   and hash those records in `review.json`.
6. Run `cargo run -- validate-evaluation benchmarks/cases/evaluation/real-project-v1`
   before any Bifrost or reference-LSP execution. Only then can the ordinary
   freeze and reporting workflows run.

The resulting evidence permits conclusions only within the scope stated in the
protocol. It does not turn the development fixture corpus into a sampled
evaluation set.
