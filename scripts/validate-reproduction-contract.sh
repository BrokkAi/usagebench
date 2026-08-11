#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
registry="$repo_root/adapters/candidates.json"
reference_manifest="$repo_root/containers/reference/v1/manifest.json"
workflow="$repo_root/.github/workflows/reference-environments.yml"
native_workflow="$repo_root/.github/workflows/native-reproduction.yml"
scope_resolver="$repo_root/scripts/resolve-freeze-scope.sh"

python3 "$repo_root/scripts/build-real-project-v1-publication-review.py" --check

jq -e '.schemaVersion == 2' "$registry" >/dev/null

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
    | select(.advertised and .reproductionClass == "canonical")
    | [.id, .referenceRunner] | @tsv' "$registry"
)

jq -e '
  ([.candidates[] | select(.advertised and .reproductionClass == "native_two_host")] | length == 9)
  and ([.candidates[] | select(
    .advertised
    and .id == "apple-clangd-21"
    and .resolvedVersionPrefix == "Apple clangd 21.0.0"
  )] | length == 1)
  and ([.candidates[] | select(.id == "clangd" and (.advertised | not))] | length == 1)
' "$registry" >/dev/null

grep -q 'name: native-reproduction-evidence' "$native_workflow"
grep -q 'path == ".github/workflows/native-reproduction.yml"' "$repo_root/.github/workflows/freeze.yml"
grep -q 'Historical identity limitation' "$repo_root/docs/src/content/docs/results/index.md"

development_scope="$(bash "$scope_resolver" development native)"
evaluation_scope="$(bash "$scope_resolver" evaluation native)"
evaluation_all_scope="$(bash "$scope_resolver" evaluation all)"
development_native_candidates="$(jq -c '.candidates' <<< "$development_scope")"
evaluation_native_candidates="$(jq -c '.candidates' <<< "$evaluation_scope")"
[[ "$(jq -r 'length' <<< "$development_native_candidates")" -eq 9 ]] || {
  echo "development native candidate scope must contain all nine advertised profiles" >&2
  exit 1
}
[[ "$evaluation_native_candidates" == '["pyright","typescript-language-server"]' ]] || {
  echo "evaluation native candidate scope does not match the protocol targets" >&2
  exit 1
}
[[ "$(jq -r '.casePath' <<< "$evaluation_scope")" == "benchmarks/cases/evaluation/real-project-v1" ]] || {
  echo "evaluation case scope does not match the registered release slice" >&2
  exit 1
}
[[ "$(jq -c '.candidates' <<< "$evaluation_all_scope")" == '["bifrost","gopls","pyright","typescript-language-server"]' ]] || {
  echo "evaluation full candidate scope does not match Bifrost plus protocol targets" >&2
  exit 1
}
grep -Fq 'candidate: ${{ fromJSON(needs.contract.outputs.candidates) }}' "$native_workflow" || {
  echo "native workflow does not consume the validated dynamic candidate scope" >&2
  exit 1
}
grep -Fq 'scripts/resolve-freeze-scope.sh "$SNAPSHOT_KIND" native' "$native_workflow" || {
  echo "native workflow does not consume the shared scope resolver" >&2
  exit 1
}
grep -Fq 'scripts/resolve-freeze-scope.sh evaluation all' "$repo_root/.github/workflows/freeze.yml" || {
  echo "freeze workflow does not consume the shared scope resolver" >&2
  exit 1
}
grep -Fq 'rm -rf -- "$RUNNER_TEMP/release-corpus/benchmarks/cases/evaluation"' "$native_workflow" || {
  echo "native development collection does not exclude the evaluation partition" >&2
  exit 1
}
awk '
  /- name: Stage release-shaped corpus/ { in_stage = 1; next }
  in_stage && /SNAPSHOT_KIND: \$\{\{ inputs.snapshot_kind \}\}/ { found = 1 }
  in_stage && /run: \|/ { exit(found ? 0 : 1) }
  END { if (!in_stage || !found) exit 1 }
' "$native_workflow" || {
  echo "native corpus staging does not receive the selected snapshot kind" >&2
  exit 1
}
grep -Fq 'rm -rf -- "$RUNNER_TEMP/freeze-corpus/benchmarks/cases/evaluation"' \
  "$repo_root/.github/workflows/freeze.yml" || {
  echo "development freeze does not exclude the evaluation partition" >&2
  exit 1
}
grep -Fq -- '--manifest "$RUNNER_TEMP/freeze-corpus/evidence/freeze-manifest.json"' \
  "$repo_root/.github/workflows/freeze.yml" || {
  echo "result generation does not validate against the staged release root" >&2
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
