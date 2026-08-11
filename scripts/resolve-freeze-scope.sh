#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
snapshot_kind="${1:-}"
registry="$repo_root/adapters/candidates.json"
protocol="$repo_root/benchmarks/evaluation/real-project-v1/protocol.json"

case "$snapshot_kind" in
  development)
    jq -c '{
      casePath: "benchmarks/cases",
      candidates: [.candidates[] | select(.advertised) | .id]
    }' "$registry"
    ;;
  evaluation)
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
      --arg case_path "benchmarks/cases/evaluation/real-project-v1" \
      --argjson candidates "$candidates" \
      '{casePath: $case_path, candidates: $candidates}'
    ;;
  *)
    echo "usage: $0 {development|evaluation}" >&2
    exit 2
    ;;
esac
