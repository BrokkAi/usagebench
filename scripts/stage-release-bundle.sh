#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: $0 SOURCE_ROOT DESTINATION RELEASE_TAG REVISION" >&2
  exit 2
}

source_root="${1:-}"
destination="${2:-}"
release_tag="${3:-}"
revision="${4:-}"
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
