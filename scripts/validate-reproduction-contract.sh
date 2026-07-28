#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
registry="$repo_root/adapters/candidates.json"
reference_manifest="$repo_root/containers/reference/v1/manifest.json"
workflow="$repo_root/.github/workflows/reference-environments.yml"

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

grep -q 'name: native-reproduction-evidence' "$repo_root/.github/workflows/native-reproduction.yml"
grep -q 'path == ".github/workflows/native-reproduction.yml"' "$repo_root/.github/workflows/freeze.yml"
grep -q 'Historical identity limitation' "$repo_root/docs/src/content/docs/results/index.md"

while IFS= read -r candidate_id; do
  grep -Eq "^[[:space:]]*- $candidate_id[[:space:]]*$" \
    "$repo_root/.github/workflows/native-reproduction.yml" || {
    echo "native candidate $candidate_id lacks a two-host collection row" >&2
    exit 1
  }
done < <(
  jq -r '.candidates[]
    | select(.advertised and .reproductionClass == "native_two_host")
    | .id' "$registry"
)

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
