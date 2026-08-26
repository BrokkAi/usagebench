#!/usr/bin/env bash
set -euo pipefail

stage_started_ns="$(python3 -c 'import time; print(time.time_ns())')"

usage() {
  echo "usage: $0 SOURCE_ROOT DESTINATION RELEASE_TAG REVISION [FREEZE_EVIDENCE_DIRECTORY] [GENERATED_RESULTS_DIRECTORY]" >&2
  exit 2
}

source_root="${1:-}"
destination="${2:-}"
release_tag="${3:-}"
revision="${4:-}"
freeze_evidence_directory="${5:-}"
generated_results_directory="${6:-}"
[[ -d "$source_root" && -n "$destination" ]] || usage
[[ "$release_tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || usage
[[ "$revision" =~ ^[0-9a-f]{40}$ ]] || usage
[[ ! -e "$destination" ]] || {
  echo "release-bundle destination already exists: $destination" >&2
  exit 1
}

mkdir -p "$destination"
cp -R \
  "$source_root/benchmarks" \
  "$source_root/fixtures" \
  "$source_root/adapters" \
  "$source_root/schema" \
  "$source_root/src" \
  "$source_root/containers" \
  "$source_root/scripts" \
  "$destination/"
cp \
  "$source_root/Cargo.toml" \
  "$source_root/Cargo.lock" \
  "$source_root/README.md" \
  "$source_root/RELEASES.md" \
  "$source_root/ARTIFACT.md" \
  "$source_root/CITATION.cff" \
  "$source_root/LICENSE.md" \
  "$source_root/.dockerignore" \
  "$destination/"

# Promotion manifests hash-bind an eligibility policy that lives under docs/,
# which is otherwise outside the staged tree. A bundle has to contain every
# artifact its manifests bind or freeze-manifest cannot canonicalize them, so
# copy exactly those files. Not all of docs/: that also carries node_modules
# and build output, which bloats the bundle and gives the analyzer a large
# unrelated tree to index.
promotion_root="$source_root/benchmarks/promotion"
if [[ -d "$promotion_root" ]]; then
  bound_list="$(mktemp)"
  trap 'rm -f "$bound_list"' EXIT
  find "$promotion_root" -type f -name '*.json' -print | sort | while IFS= read -r promotion_manifest; do
    jq -r '.. | objects | select(has("file")) | .file' "$promotion_manifest"
  done | sort -u | grep -Ev '^(benchmarks|fixtures|adapters|schema|src|containers|scripts)/' > "$bound_list" || true
  while IFS= read -r bound_artifact; do
    [[ -n "$bound_artifact" ]] || continue
    [[ "$bound_artifact" != /* && "$bound_artifact" != *..* ]] || {
      echo "promotion manifest binds an unsafe artifact path: $bound_artifact" >&2
      exit 1
    }
    [[ -f "$source_root/$bound_artifact" ]] || {
      echo "promotion manifest binds a missing artifact: $bound_artifact" >&2
      exit 1
    }
    mkdir -p "$destination/$(dirname "$bound_artifact")"
    cp -- "$source_root/$bound_artifact" "$destination/$bound_artifact"
    echo "staged bound promotion artifact: $bound_artifact"
  done < "$bound_list"
fi

jq -n \
  --arg releaseTag "$release_tag" \
  --arg releaseVersion "${release_tag#v}" \
  --arg revision "$revision" \
  '{releaseTag: $releaseTag, releaseVersion: $releaseVersion, revision: $revision}' \
  > "$destination/.usagebench-release.json"

if [[ -n "$freeze_evidence_directory" ]]; then
  [[ -d "$freeze_evidence_directory" && -f "$freeze_evidence_directory/freeze-manifest.json" ]] || {
    echo "freeze evidence must contain freeze-manifest.json: $freeze_evidence_directory" >&2
    exit 1
  }
  mkdir -p "$destination/evidence"
  cp "$freeze_evidence_directory"/*.json "$destination/evidence/"
  (
    cd "$destination/evidence"
    shasum -a 256 *.json > SHA256SUMS
  )
fi

if [[ -n "$generated_results_directory" ]]; then
  [[ -d "$generated_results_directory" \
    && -f "$generated_results_directory/results.md" \
    && -f "$generated_results_directory/case-comparison.md" ]] || {
    echo "generated results must contain results.md and case-comparison.md: $generated_results_directory" >&2
    exit 1
  }
  if find "$generated_results_directory" -maxdepth 1 -type l -print -quit | grep -q .; then
    echo "generated results may not contain symlinks: $generated_results_directory" >&2
    exit 1
  fi
  generated_result_files=()
  while IFS= read -r generated_result_file; do
    generated_result_files+=("$generated_result_file")
  done < <(find "$generated_results_directory" -maxdepth 1 -type f -print | sort)
  [[ "${#generated_result_files[@]}" -gt 0 ]] || {
    echo "generated results directory is empty: $generated_results_directory" >&2
    exit 1
  }
  mkdir -p "$destination/results"
  cp -- "${generated_result_files[@]}" "$destination/results/"
fi

stage_finished_ns="$(python3 -c 'import time; print(time.time_ns())')"
release_staging_ms="$(( (stage_finished_ns - stage_started_ns) / 1000000 ))"
python3 "$source_root/scripts/corpus-hashes.py" create \
  --root "$destination" \
  --output "$destination/.usagebench-corpus-hashes.json" \
  --timings-output "$destination/.usagebench-stage-timings.json" \
  --release-staging-ms "$release_staging_ms"
