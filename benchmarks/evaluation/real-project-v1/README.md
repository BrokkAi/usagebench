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

The recorded population frame has a uniform 75,000-star minimum. This keeps
each profile below GitHub Search's 1,000-result window while retaining a
complete, source-only eligible population rather than selecting a ranked prefix.
Capture spaces GitHub REST requests by 800 ms so the complete snapshot remains
within the authenticated API budget.
If a transient request fails, capture resumes from its uncommitted
`population.partial.json` checkpoint; only the completed `population.json`
belongs in the immutable population commit.

## Selection procedure

1. Capture the GitHub API population in `population.json`, including the exact
   request URLs, retrieval timestamp, pagination, repository metadata, and the
   default-branch commit observed at capture time. Commit that file before
   drawing any repository.
2. Apply the recorded eligibility and exclusion checks. Preserve the complete
   ranked candidate list, including exclusions and reasons, in `selection.json`.

   ```bash
   cargo run -- capture-real-project-population
   git add benchmarks/evaluation/real-project-v1/population.json
   git commit -m "Capture real-project-v1 population"
   cargo run -- draw-real-project-selection --protocol-commit <protocol-introducing-commit>
   ```

   The draw command rejects a missing, uncommitted, or modified population
   snapshot. It only reads the frozen protocol and snapshot; it never calls an
   analyzer or language server.
3. Archive each selected exact Git commit with `git archive --format=tar.gz`;
   use an empty worktree-attribute view so export and EOL filters cannot alter
   the committed blob bytes. Then
   record the raw commit object, commit tree, archive-content tree, relative
   archive path, and SHA-256 in `sources.json`. Validation hashes the raw commit
   object, checks its tree pointer and the embedded archive commit, reconstructs
   the archive-content tree (including gitlinks), and requires both trees to be
   identical before reading source ranges from the bounded archive stream.
   These archives are tracked through Git LFS, not as ordinary Git blobs. A
   checkout that validates or runs this corpus must have Git LFS installed and
   materialize the objects (`git lfs pull`); CI checkouts that consume the
   corpus set `lfs: true`.
4. Author YAML documents under `benchmarks/cases/evaluation/real-project-v1/`
   with portable `benchmark://source/...` locations. Each document must point
   at `selection.json`, `review.json`, and `sources.json` through its corpus
   metadata.
5. Preserve the historical same-provider Codex derivations, then retain the
   fresh per-case OpenAI and Anthropic derivations and accountable human
   adjudications. `review.json` binds the normalized reviewer evidence, all 72
   raw sessions, case packets, prompt/schema, and canonical v3 protocol.
6. Run `cargo run -- validate-evaluation benchmarks/cases/evaluation/real-project-v1`
   before any Bifrost or reference-LSP execution.

## Evaluation freeze procedure

Materialize and verify the source and review boundary before running an
analyzer. The release order is:

1. Pull the Git LFS objects and verify the source lock, selection manifest,
   independent review records, and adjudication evidence.
2. Run
   `cargo run -- validate-evaluation benchmarks/cases/evaluation/real-project-v1`.
3. Verify the completed schema-v2 review with one accountable human adjudicator
   and blinded agents from OpenAI and Anthropic. The historical same-provider
   Codex evidence remains provenance, but is not the qualifying panel.
4. Run **Native two-host reproduction** at the exact release revision for only
   `pyright` and `typescript-language-server`, and retain its accepted evidence
   artifact.
5. Run **Freeze benchmark snapshot** with snapshot kind `evaluation`, corpus
   `benchmarks/cases/evaluation/real-project-v1`, candidates
   `bifrost,gopls,pyright,typescript-language-server`, and the native-evidence
   workflow run ID.

The freeze manifest binds `protocol.json`, `selection.json`, `review.json`, and
`sources.json` by digest and records repository and case denominators,
exclusions, and replacements. Generated tables must remain labeled as the
`evaluation` partition and must not be pooled with development cases.

The review evidence permits the evaluation freeze, whose resulting evidence
supports only descriptive per-profile comparisons for
these 12 source-only sampled repositories, 36 declarations, and the References
and Definition operations. It does not support language-wide or ecosystem-wide
estimates, cross-language rankings, causal defect claims, or latency, memory,
and cold-start claims. It does not turn the development fixture corpus into a
sampled evaluation set. The corpus and qualifying cross-provider review
evidence are checked in; analyzer results have not yet been frozen or
published.

## Declaration draw

`scripts/real_project_v1_candidates.py` records the complete eligible
declaration ranking before review. It is source-only and does not invoke an
analyzer or language server. The version-1 candidate universe consists of
module/package-level named functions, Go methods, and nominal types in the
profile language. A candidate must have a unique declaration name in the
eligible source frame, must not begin with `_`, and must have at least one
additional in-frame token occurrence. Tests, examples, benchmarks, generated
sources, dependency/vendor trees, TypeScript declaration files, and common
derived-output paths are excluded.

Every candidate records its portable URI, exact zero-based UTF-16 range,
symbol kind, source-token occurrence count, protocol digest, and rank. The
chosen three also retain all source-token occurrences for independent semantic
review. If either reviewer establishes that a choice violates the frozen
exclusions or lacks an unambiguous semantic use, the next unused rank in the
same repository is selected and the replacement is recorded before any
analyzer execution.
