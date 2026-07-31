#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: $0 SOURCE_ROOT CANDIDATE_REGISTRY CANDIDATES CORPUS_ROOT CASE_PATH OUTPUT_DIRECTORY REVISION [NATIVE_EVIDENCE_DIRECTORY]" >&2
  exit 2
}

source_root="${1:-}"
candidate_registry="${2:-}"
candidate_input="${3:-}"
corpus_root="${4:-}"
case_path="${5:-}"
output_directory="${6:-}"
revision="${7:-}"
native_evidence_directory="${8:-}"
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
evidence_paths=()

native_candidates_json="$(
  for candidate_id in "${candidate_ids[@]}"; do
    jq -r --arg id "$candidate_id" \
      '.candidates[] | select(.id == $id and .reproductionClass == "native_two_host") | .id' \
      "$candidate_registry"
  done | jq -Rsc 'split("\n") | map(select(length > 0))'
)"
if [[ "$(jq -r 'length' <<< "$native_candidates_json")" -gt 0 ]]; then
  [[ -d "$native_evidence_directory" && -f "$native_evidence_directory/collection.json" && ! -L "$native_evidence_directory/collection.json" ]] || {
    echo "native candidates require collection metadata from the reproduction workflow" >&2
    exit 1
  }
  jq -e \
    --arg revision "$revision" \
    --arg snapshotKind "$SNAPSHOT_KIND" \
    --arg releaseTag "$SNAPSHOT_VERSION" \
    --arg casePath "$case_path" \
    --argjson candidates "$native_candidates_json" \
    '.schemaVersion == 1
      and .revision == $revision
      and .snapshotKind == $snapshotKind
      and .releaseTag == $releaseTag
      and .casePath == $casePath
      and (if $snapshotKind == "evaluation"
           then .candidates == $candidates
           else ($candidates - .candidates | length) == 0
           end)' \
    "$native_evidence_directory/collection.json" >/dev/null || {
      echo "native evidence collection does not match this revision, scope, release, case path, or candidate set" >&2
      exit 1
    }
  cp "$native_evidence_directory/collection.json" "$output_directory/native-collection.json"
fi

stage_native_file() {
  local file="$1"
  [[ "$file" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]] || {
    echo "native evidence names an unsafe file: $file" >&2
    exit 1
  }
  [[ -f "$native_evidence_directory/$file" && ! -L "$native_evidence_directory/$file" ]] || {
    echo "native evidence file is missing or is a symbolic link: $file" >&2
    exit 1
  }
  cp "$native_evidence_directory/$file" "$output_directory/$file"
}

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
  reproduction_class="$(jq -r '.reproductionClass' <<< "$candidate")"
  evidence="$output_directory/$candidate_id-evidence.json"
  if [[ "$reproduction_class" == "canonical" ]]; then
    reference_runner="$(jq -r '.referenceRunner // empty' <<< "$candidate")"
    [[ "$reference_runner" =~ ^[a-z0-9][a-z0-9-]*$ ]] || {
      echo "canonical candidate $candidate_id has no protected reference runner" >&2
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
    report_sha="$(sha256sum "$output_directory/$candidate_id.json" | awk '{print $1}')"
    environment_version="$(jq -er '.environment.referenceEnvironment.version' "$output_directory/$candidate_id.json")"
    definition_digest="$(jq -er '.environment.referenceEnvironment.definitionDigest' "$output_directory/$candidate_id.json")"
    jq -n \
      --arg candidateId "$candidate_id" \
      --arg report "$candidate_id.json" \
      --arg reportSha "$report_sha" \
      --arg referenceRunner "$reference_runner" \
      --arg environmentVersion "$environment_version" \
      --arg definitionDigest "$definition_digest" \
      '{schemaVersion: 1, candidateId: $candidateId,
        primaryReport: {file: $report, sha256: $reportSha},
        class: "canonical", referenceRunner: $referenceRunner,
        environmentVersion: $environmentVersion, definitionDigest: $definitionDigest}' \
      > "$evidence"
    report_paths+=("$output_directory/$candidate_id.json")
  elif [[ "$reproduction_class" == "native_two_host" ]]; then
    [[ -d "$native_evidence_directory" ]] || {
      echo "native candidate $candidate_id requires a staged native evidence directory" >&2
      exit 1
    }
    source_evidence="$native_evidence_directory/$candidate_id-evidence.json"
    [[ -f "$source_evidence" && ! -L "$source_evidence" ]] || {
      echo "native evidence not found: $source_evidence" >&2
      exit 1
    }
    cp "$source_evidence" "$evidence"
    [[ "$(jq -er '.candidateId' "$evidence")" == "$candidate_id" && "$(jq -er '.class' "$evidence")" == "native_two_host" ]] || {
      echo "native evidence identity does not match candidate $candidate_id" >&2
      exit 1
    }
    primary_file="$(jq -er '.primaryReport.file' "$evidence")"
    corroborating_file="$(jq -er '.corroboratingReport.file' "$evidence")"
    diff_file="$(jq -r '.comparison.diff.file // empty' "$evidence")"
    [[ "$primary_file" != "$corroborating_file" && "$primary_file" != "$candidate_id-evidence.json" && "$corroborating_file" != "$candidate_id-evidence.json" ]] || {
      echo "native evidence for $candidate_id reuses an artifact file name" >&2
      exit 1
    }
    stage_native_file "$primary_file"
    stage_native_file "$corroborating_file"
    if [[ -n "$diff_file" ]]; then
      [[ "$diff_file" != "$primary_file" && "$diff_file" != "$corroborating_file" && "$diff_file" != "$candidate_id-evidence.json" ]] || {
        echo "native evidence for $candidate_id reuses its diff file name" >&2
        exit 1
      }
      stage_native_file "$diff_file"
    fi
    report_paths+=("$output_directory/$primary_file")
  else
    echo "unsupported reproduction class for $candidate_id: $reproduction_class" >&2
    exit 1
  fi
  evidence_paths+=("$evidence")
done

report_args=()
evidence_args=()
for index in "${!candidate_ids[@]}"; do
  report_args+=(--report "${report_paths[$index]}")
  evidence_args+=(--evidence "${evidence_paths[$index]}")
done
freeze_args=(
  freeze-manifest
  --snapshot-kind "$SNAPSHOT_KIND" \
  --version "$SNAPSHOT_VERSION" \
  --revision "$revision" \
  --candidates-file "$candidate_registry" \
  --candidates "$candidate_input" \
  "${report_args[@]}" \
  "${evidence_args[@]}"
  --output "$output_directory/freeze-manifest.json"
)
if [[ "$SNAPSHOT_KIND" == "evaluation" ]]; then
  freeze_args+=(--evaluation-corpus "$corpus_root/$case_path")
fi
cargo run --locked --manifest-path "$source_root/Cargo.toml" -- "${freeze_args[@]}"
