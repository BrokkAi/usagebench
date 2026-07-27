#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: $0 SOURCE_ROOT CANDIDATE_REGISTRY CANDIDATES CORPUS_ROOT OUTPUT_DIRECTORY REVISION" >&2
  exit 2
}

source_root="${1:-}"
candidate_registry="${2:-}"
candidate_input="${3:-}"
corpus_root="${4:-}"
output_directory="${5:-}"
revision="${6:-}"
[[ -d "$source_root" && -f "$candidate_registry" && -n "$candidate_input" && -f "$corpus_root/.usagebench-release.json" ]] || usage
[[ "$revision" =~ ^[0-9a-f]{40}$ ]] || usage

mkdir -p "$output_directory"
IFS=',' read -r -a candidate_ids <<< "$candidate_input"
[[ "${#candidate_ids[@]}" -gt 0 ]] || usage

seen_ids=','
for candidate_id in "${candidate_ids[@]}"; do
  [[ "$candidate_id" =~ ^[a-z0-9][a-z0-9-]*$ ]] || {
    echo "invalid candidate ID: $candidate_id" >&2
    exit 1
  }
  [[ "$seen_ids" != *",$candidate_id,"* ]] || {
    echo "candidate selected more than once: $candidate_id" >&2
    exit 1
  }
  seen_ids+="$candidate_id,"

  candidate="$(jq --arg id "$candidate_id" -ce '.candidates[] | select(.id == $id)' "$candidate_registry")" || {
    echo "unknown candidate: $candidate_id" >&2
    exit 1
  }
  eligible="$(jq -r '.eligibleForFreeze' <<< "$candidate")"
  [[ "$eligible" == "true" ]] || {
    reason="$(jq -r '.ineligibleReason // "no protected execution contract"' <<< "$candidate")"
    echo "candidate $candidate_id is not eligible for automated freeze: $reason" >&2
    exit 1
  }

  reference_runner="$(jq -r '.referenceRunner // empty' <<< "$candidate")"
  [[ "$reference_runner" =~ ^[a-z0-9][a-z0-9-]*$ ]] || {
    echo "candidate $candidate_id has no protected reference runner" >&2
    exit 1
  }
  set +e
  "$source_root/scripts/reference-image.sh" "$reference_runner" "$SNAPSHOT_VERSION" "$revision"
  "$source_root/scripts/run-reference.sh" \
    "$reference_runner" "$corpus_root" "$output_directory/$candidate_id.json"
  status=$?
  set -e
  [[ -f "$output_directory/$candidate_id.json" ]] || {
    echo "candidate $candidate_id did not write a report" >&2
    exit 1
  }
  if [[ "$status" -ne 0 ]]; then
    echo "candidate $candidate_id completed with non-exact benchmark results; preserving its report" >&2
  fi
done

report_args=()
for candidate_id in "${candidate_ids[@]}"; do
  report_args+=(--report "$output_directory/$candidate_id.json")
done
cargo run --locked --manifest-path "$source_root/Cargo.toml" -- \
  freeze-manifest \
  --snapshot-kind "$SNAPSHOT_KIND" \
  --version "$SNAPSHOT_VERSION" \
  --revision "$revision" \
  --candidates-file "$candidate_registry" \
  --candidates "$candidate_input" \
  "${report_args[@]}" \
  --output "$output_directory/freeze-manifest.json"
