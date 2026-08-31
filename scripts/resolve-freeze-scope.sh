#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
snapshot_kind="${1:-}"
evaluation_freeze="${2:-real-project-v2}"
registry="$repo_root/adapters/candidates.json"
[[ "$evaluation_freeze" =~ ^real-project-v[0-9]+$ ]] || {
  echo "evaluation freeze must match real-project-vN" >&2
  exit 2
}
protocol="$repo_root/benchmarks/evaluation/$evaluation_freeze/protocol.json"

case "$snapshot_kind" in
  development)
    jq -c '{
      casePath: "benchmarks/cases",
      candidates: [.candidates[] | select(.advertised) | .id]
    }' "$registry"
    ;;
  evaluation)
    [[ -f "$protocol" ]] || {
      echo "unknown evaluation freeze: $evaluation_freeze" >&2
      exit 1
    }
    target_candidates="$(jq -ce '[.targetProfiles[].candidateId]' "$protocol")"
    advertised_candidates="$(jq -ce '[.candidates[] | select(.advertised) | .id]' "$registry")"
    missing_candidates="$(jq -cn \
      --argjson targets "$target_candidates" \
      --argjson advertised "$advertised_candidates" \
      '$targets - $advertised')"
    [[ "$missing_candidates" == "[]" ]] || {
      echo "evaluation protocol contains unavailable candidates: $missing_candidates" >&2
      exit 1
    }
    candidates="$(jq -cn --argjson targets "$target_candidates" '["bifrost"] + $targets')"
    jq -cn \
      --arg case_path "benchmarks/cases/evaluation/$evaluation_freeze" \
      --argjson candidates "$candidates" \
      '{casePath: $case_path, candidates: $candidates}'
    ;;
  legacy-promoted)
    promotion_manifest="benchmarks/promotion/legacy-v2/manifest.json"
    promotion_path="$repo_root/$promotion_manifest"
    [[ -f "$promotion_path" ]] || {
      echo "legacy promotion manifest is missing: $promotion_manifest" >&2
      exit 1
    }
    balanced_core_count="$(jq -er '[.documents[].cases[] | select(.membership == "balanced_core")] | length' "$promotion_path")"
    [[ "$balanced_core_count" == "110" ]] || {
      echo "legacy promotion manifest must contain exactly 110 balanced_core cases" >&2
      exit 1
    }
    non_core_count="$(jq -er '[.documents[].cases[] | select(.membership != "balanced_core")] | length' "$promotion_path")"
    [[ "$non_core_count" == "0" ]] || {
      echo "legacy promotion execution manifest must exclude overflow and controls" >&2
      exit 1
    }
    candidates="$(jq -ce '[.candidates[] | select(.advertised) | .id]' "$registry")"
    jq -cn \
      --arg case_path "benchmarks/cases" \
      --arg promotion_manifest "$promotion_manifest" \
      --argjson candidates "$candidates" \
      '{casePath: $case_path, candidates: $candidates, promotionManifest: $promotion_manifest}'
    ;;
  *)
    echo "usage: $0 {development|evaluation|legacy-promoted} [real-project-vN]" >&2
    exit 2
    ;;
esac
