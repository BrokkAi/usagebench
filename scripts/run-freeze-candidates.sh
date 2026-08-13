#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: $0 SOURCE_ROOT CANDIDATE_REGISTRY CANDIDATES CORPUS_ROOT CASE_PATH OUTPUT_DIRECTORY REVISION" >&2
  exit 2
}

source_root="${1:-}"
candidate_registry="${2:-}"
candidate_input="${3:-}"
corpus_root="${4:-}"
case_path="${5:-}"
output_directory="${6:-}"
revision="${7:-}"
[[ -d "$source_root" && -f "$candidate_registry" && -n "$candidate_input" && -f "$corpus_root/.usagebench-release.json" ]] || usage
[[ "$revision" =~ ^[0-9a-f]{40}$ ]] || usage
[[ -n "$case_path" && "$case_path" != /* && "$case_path" != *//* && "$case_path" =~ ^[A-Za-z0-9._/-]+$ ]] || {
  echo "case path must be a safe path relative to the corpus root" >&2
  exit 1
}
IFS='/' read -r -a case_components <<< "$case_path"
for component in "${case_components[@]}"; do
  [[ -n "$component" && "$component" != "." && "$component" != ".." ]] || {
    echo "case path must not contain empty, dot, or parent components" >&2
    exit 1
  }
done
corpus_root="$(cd "$corpus_root" && pwd -P)"
[[ -d "$corpus_root/$case_path" ]] || {
  echo "case path is not a directory in the corpus: $case_path" >&2
  exit 1
}
case_directory="$(cd "$corpus_root/$case_path" && pwd -P)"
[[ "$case_directory" == "$corpus_root/"* ]] || {
  echo "case path resolves outside the corpus root: $case_path" >&2
  exit 1
}

mkdir -p "$output_directory"
IFS=',' read -r -a candidate_ids <<< "$candidate_input"
[[ "${#candidate_ids[@]}" -gt 0 ]] || usage

seen_ids=','
report_paths=()

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
  advertised="$(jq -r '.advertised' <<< "$candidate")"
  [[ "$advertised" == "true" ]] || {
    reason="$(jq -r '.ineligibleReason // "not advertised"' <<< "$candidate")"
    echo "candidate $candidate_id is not advertised for public results: $reason" >&2
    exit 1
  }
  reference_runner="$(jq -r '.referenceRunner // empty' <<< "$candidate")"
  if [[ "$candidate_id" == "bifrost" && -n "${NATIVE_BIFROST_REPO:-}" ]]; then
    bifrost_revision="$(jq -er '.revision' <<< "$candidate")"
    set +e
    bifrost_args=(
      run-bifrost "$case_directory"
      --bifrost-repo "$NATIVE_BIFROST_REPO"
      --bifrost-commit "$bifrost_revision"
      --work-dir "$output_directory/$candidate_id-work"
      --output "$output_directory/$candidate_id.json"
    )
    if [[ -n "${FREEZE_SHARD_LANGUAGE:-}" ]]; then
      bifrost_args+=(--language "$FREEZE_SHARD_LANGUAGE")
    fi
    cargo run --locked --manifest-path "$corpus_root/Cargo.toml" -- "${bifrost_args[@]}"
    status=$?
    set -e
    [[ -f "$output_directory/$candidate_id.json" ]] || {
      echo "candidate $candidate_id did not write a report" >&2
      exit 1
    }
    if [[ "$status" -ne 0 ]]; then
      echo "candidate $candidate_id completed with non-exact benchmark results; preserving its report" >&2
    fi
    report_paths+=("$output_directory/$candidate_id.json")
  elif [[ -n "$reference_runner" ]]; then
    [[ "$reference_runner" =~ ^[a-z0-9][a-z0-9-]*$ ]] || {
      echo "candidate $candidate_id has an invalid reference runner" >&2
      exit 1
    }
    "$source_root/scripts/reference-image.sh" "$reference_runner" "$SNAPSHOT_VERSION" "$revision"
    set +e
    "$source_root/scripts/run-reference.sh" \
      "$reference_runner" "$corpus_root" "$output_directory/$candidate_id.json" "$case_path"
    status=$?
    set -e
    [[ -f "$output_directory/$candidate_id.json" ]] || {
      echo "candidate $candidate_id did not write a report" >&2
      exit 1
    }
    if [[ "$status" -ne 0 ]]; then
      echo "candidate $candidate_id completed with non-exact benchmark results; preserving its report" >&2
    fi
    report_paths+=("$output_directory/$candidate_id.json")
  else
    [[ "$(jq -r '.runner' <<< "$candidate")" == "lsp" ]] || {
      echo "candidate $candidate_id has no executable release runner" >&2
      exit 1
    }
    profile="$(jq -er '.profile' <<< "$candidate")"
    [[ "$profile" =~ ^adapters/lsp/[A-Za-z0-9._-]+\.json$ && -f "$corpus_root/$profile" ]] || {
      echo "candidate $candidate_id has an unsafe or missing LSP profile" >&2
      exit 1
    }
    set +e
    cargo run --locked --manifest-path "$corpus_root/Cargo.toml" -- \
      run-lsp "$corpus_root/$case_path" \
      --profile "$corpus_root/$profile" \
      --work-dir "$output_directory/$candidate_id-work" \
      --output "$output_directory/$candidate_id.json"
    status=$?
    set -e
    [[ -f "$output_directory/$candidate_id.json" ]] || {
      echo "candidate $candidate_id did not write a report" >&2
      exit 1
    }
    if [[ "$status" -ne 0 ]]; then
      echo "candidate $candidate_id completed with non-exact benchmark results; preserving its report" >&2
    fi
    report_paths+=("$output_directory/$candidate_id.json")
  fi
done

if [[ "${FREEZE_SHARD_ONLY:-0}" == "1" ]]; then
  exit 0
fi

report_args=()
for index in "${!candidate_ids[@]}"; do
  report_args+=(--report "${report_paths[$index]}")
done
freeze_args=(
  freeze-manifest
  --snapshot-kind "$SNAPSHOT_KIND" \
  --version "$SNAPSHOT_VERSION" \
  --revision "$revision" \
  --candidates-file "$candidate_registry" \
  --candidates "$candidate_input" \
  "${report_args[@]}" \
  --output "$output_directory/freeze-manifest.json"
)
if [[ "$SNAPSHOT_KIND" == "evaluation" ]]; then
  freeze_args+=(--evaluation-corpus "$corpus_root/$case_path")
fi
cargo run --locked --manifest-path "$source_root/Cargo.toml" -- "${freeze_args[@]}"
