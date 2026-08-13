#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
registry="$repo_root/adapters/candidates.json"
reference_manifest="$repo_root/containers/reference/v1/manifest.json"
workflow="$repo_root/.github/workflows/reference-environments.yml"
scope_resolver="$repo_root/scripts/resolve-freeze-scope.sh"
freeze_workflow="$repo_root/.github/workflows/freeze.yml"

python3 "$repo_root/scripts/build-real-project-v1-publication-review.py" --check
python3 "$repo_root/scripts/build-real-project-v2-publication-review.py" --check

jq -e '.schemaVersion == 3' "$registry" >/dev/null

while IFS=$'\t' read -r candidate_id reference_runner; do
  [[ -n "$reference_runner" ]] || {
    echo "canonical candidate $candidate_id lacks referenceRunner" >&2
    exit 1
  }
  jq -e --arg runner "$reference_runner" '.runners[$runner]' "$reference_manifest" >/dev/null || {
    echo "canonical candidate $candidate_id references unknown environment $reference_runner" >&2
    exit 1
  }
  grep -Eq "^[[:space:]]*- runner: $reference_runner[[:space:]]*$" "$workflow" || {
    echo "canonical candidate $candidate_id lacks a reference-environment smoke row" >&2
    exit 1
  }
done < <(
  jq -r '.candidates[]
    | select(.advertised and .referenceRunner != null)
    | [.id, .referenceRunner] | @tsv' "$registry"
)

jq -e '
  ([.candidates[] | select(
    .advertised
    and .id == "apple-clangd-21"
    and .resolvedVersionPrefix == "Apple clangd 21.0.0"
  )] | length == 1)
  and ([.candidates[] | select(.id == "clangd" and (.advertised | not))] | length == 1)
' "$registry" >/dev/null

historical_results="$repo_root/docs/src/content/docs/results/development-2026-07-24.md"
current_results="$repo_root/docs/src/content/docs/results/index.md"
grep -q 'Historical identity limitation' "$historical_results"
grep -q 'candidate evidence' "$historical_results"
grep -q 'Evaluation evidence' "$current_results"
grep -q 'UsageBench v0.2.0' "$current_results"

development_scope="$(bash "$scope_resolver" development)"
evaluation_v1_scope="$(bash "$scope_resolver" evaluation real-project-v1)"
evaluation_scope="$(bash "$scope_resolver" evaluation real-project-v2)"
development_candidates="$(jq -c '.candidates' <<< "$development_scope")"
[[ "$(jq -r 'length' <<< "$development_candidates")" -eq 11 ]] || {
  echo "development candidate scope must contain all eleven advertised candidates" >&2
  exit 1
}
[[ "$(jq -r '.casePath' <<< "$evaluation_scope")" == "benchmarks/cases/evaluation/real-project-v2" ]] || {
  echo "evaluation v2 case scope does not match the registered release slice" >&2
  exit 1
}
[[ "$(jq -r '.casePath' <<< "$evaluation_v1_scope")" == "benchmarks/cases/evaluation/real-project-v1" ]] || {
  echo "evaluation v1 case scope no longer resolves" >&2
  exit 1
}
[[ "$(jq -c '.candidates' <<< "$evaluation_v1_scope")" == '["bifrost","gopls","pyright","typescript-language-server"]' ]] || {
  echo "evaluation v1 candidate scope no longer matches its protocol" >&2
  exit 1
}
[[ "$(jq -c '.candidates' <<< "$evaluation_scope")" == '["bifrost","eclipse-jdtls","rust-analyzer","apple-clangd-21"]' ]] || {
  echo "evaluation v2 candidate scope does not match Bifrost plus protocol targets" >&2
  exit 1
}
grep -Fq 'scripts/resolve-freeze-scope.sh evaluation' "$freeze_workflow" || {
  echo "freeze workflow does not consume the shared scope resolver" >&2
  exit 1
}
grep -Fq "default: 'macos-26'" "$freeze_workflow" || {
  echo "evaluation freeze does not default to the GitHub-hosted macOS 26 runner" >&2
  exit 1
}
grep -Fq '^usagebench-ephemeral-macos-arm64-[0-9a-f]{32}$' "$freeze_workflow" || {
  echo "evaluation freeze does not constrain one-job repository runner labels" >&2
  exit 1
}
[[ "$(grep -c 'shard: bifrost-' "$freeze_workflow")" -eq 3 ]] || {
  echo "evaluation freeze must contain three language-bounded Bifrost shards" >&2
  exit 1
}
for shard in eclipse-jdtls-java rust-analyzer-rust apple-clangd-21-cpp; do
  grep -Fq "shard: $shard" "$freeze_workflow" || {
    echo "evaluation freeze lacks shard $shard" >&2
    exit 1
  }
done
grep -Fq 'permissions:' "$freeze_workflow"
grep -Fq 'python3 scripts/freeze-shards.py aggregate' "$freeze_workflow" || {
  echo "freeze workflow does not verify shard identity and coverage before aggregation" >&2
  exit 1
}
grep -Fq -- '--manifest "$RUNNER_TEMP/freeze-corpus/evidence/freeze-manifest.json"' \
  "$freeze_workflow" || {
  echo "result generation does not validate against the staged release root" >&2
  exit 1
}
grep -Fq -- '--manifest-path "$corpus_root/Cargo.toml"' \
  "$repo_root/scripts/run-freeze-candidates.sh" || {
  echo "native LSP candidates are not executed from the staged release corpus" >&2
  exit 1
}

while IFS=$'\t' read -r candidate_id profile expected_sha256; do
  actual_sha256="$(shasum -a 256 "$repo_root/$profile" | awk '{print $1}')"
  [[ "$actual_sha256" == "$expected_sha256" ]] || {
    echo "advertised LSP profile checksum drift for $candidate_id" >&2
    exit 1
  }
done < <(
  jq -r '.candidates[]
    | select(.advertised and .runner == "lsp")
    | [.id, .profile, .profileSha256] | @tsv' "$registry"
)
