#!/usr/bin/env bash
set -euo pipefail

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
  mkdir -p "$destination/results"
  cp "$generated_results_directory/results.md" "$generated_results_directory/case-comparison.md" "$destination/results/"
fi
