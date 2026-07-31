#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
snapshot_kind="${1:-}"
candidate_scope="${2:-}"
registry="$repo_root/adapters/candidates.json"
protocol="$repo_root/benchmarks/evaluation/real-project-v1/protocol.json"

case "$snapshot_kind:$candidate_scope" in
  development:native)
    jq -c '{
      casePath: "benchmarks/cases",
      candidates: [.candidates[] | select(.advertised and .reproductionClass == "native_two_host") | .id]
    }' "$registry"
    ;;
  evaluation:all|evaluation:native)
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
    if [[ "$candidate_scope" == "all" ]]; then
      candidates="$(jq -cn --argjson targets "$target_candidates" '["bifrost"] + $targets')"
    else
      candidates="$(jq -cn \
        --argjson targets "$target_candidates" \
        --slurpfile registry "$registry" \
        '[ $targets[] as $id
           | $registry[0].candidates[]
           | select(.id == $id and .advertised and .reproductionClass == "native_two_host")
           | .id ]')"
    fi
    jq -cn \
      --arg case_path "benchmarks/cases/evaluation/real-project-v1" \
      --argjson candidates "$candidates" \
      '{casePath: $case_path, candidates: $candidates}'
    ;;
  *)
    echo "usage: $0 {development|evaluation} {native|all}" >&2
    exit 2
    ;;
esac
