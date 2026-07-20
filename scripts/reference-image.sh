#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
manifest="$repo_root/containers/reference/v1/manifest.json"
schema="$repo_root/schema/reference-environment.schema.json"
dockerfile="$repo_root/containers/reference/v1/Dockerfile"

usage() {
  echo "usage: $0 RUNNER_ID USAGEBENCH_RELEASE" >&2
  exit 2
}

sha256_stream() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum | awk '{print $1}'
  else
    shasum -a 256 | awk '{print $1}'
  fi
}

for command_name in docker jq; do
  command -v "$command_name" >/dev/null 2>&1 || {
    echo "required command not found: $command_name" >&2
    exit 1
  }
done

runner_id="${1:-}"
usagebench_release="${2:-}"
[[ -n "$runner_id" && "$usagebench_release" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || usage
jq -e --arg runner "$runner_id" '.runners[$runner]' "$manifest" >/dev/null || {
  echo "unknown reference runner: $runner_id" >&2
  exit 1
}

environment_version="$(jq -r '.environmentVersion' "$manifest")"
canonical_platform="$(jq -r '.canonicalPlatform' "$manifest")"
target="$(jq -r --arg runner "$runner_id" '.runners[$runner].target' "$manifest")"
rust_base="$(jq -r --arg runner "$runner_id" '.runners[$runner].baseImages.harnessBuilder | .reference + "@" + .digest' "$manifest")"
bifrost_base="$(jq -r '.runners.bifrost.baseImages.analyzerBuilder | .reference + "@" + .digest' "$manifest")"
runtime_base="$(jq -r --arg runner "$runner_id" '.runners[$runner].baseImages.runtime | .reference + "@" + .digest' "$manifest")"
go_base="$(jq -r '.runners.gopls.baseImages.analyzerBuilder | .reference + "@" + .digest' "$manifest")"
bifrost_revision="$(jq -r '.runners.bifrost.analyzer.revision' "$manifest")"
gopls_version="$(jq -r '.runners.gopls.analyzer.requestedVersion' "$manifest")"
gopls_checksum="$(jq -r '.runners.gopls.analyzer.moduleChecksum' "$manifest")"

definition_digest="sha256:$(
  for definition_file in \
    "$manifest" \
    "$schema" \
    "$dockerfile" \
    "$repo_root/scripts/reference-image.sh" \
    "$repo_root/scripts/run-reference.sh"; do
    printf '%s\0' "${definition_file#$repo_root/}"
    cat "$definition_file"
  done | sha256_stream
)"

image_reference="usagebench-reference:${usagebench_release}-env${environment_version}-${runner_id}"
metadata_dir="$repo_root/target/reference"
mkdir -p "$metadata_dir"
buildkit_metadata="$metadata_dir/${runner_id}.buildkit.json"

docker buildx build \
  --platform "$canonical_platform" \
  --provenance=false \
  --load \
  --target "$target" \
  --file "$dockerfile" \
  --tag "$image_reference" \
  --metadata-file "$buildkit_metadata" \
  --build-arg "RUST_BASE=$rust_base" \
  --build-arg "BIFROST_BASE=$bifrost_base" \
  --build-arg "GO_BASE=$go_base" \
  --build-arg "RUNTIME_BASE=$runtime_base" \
  --build-arg "USAGEBENCH_RELEASE=$usagebench_release" \
  --build-arg "ENVIRONMENT_VERSION=$environment_version" \
  --build-arg "DEFINITION_DIGEST=$definition_digest" \
  --build-arg "BIFROST_REVISION=$bifrost_revision" \
  --build-arg "GOPLS_VERSION=$gopls_version" \
  --build-arg "GOPLS_MODULE_CHECKSUM=$gopls_checksum" \
  "$repo_root"

image_digest="$(jq -r '."containerimage.digest" // empty' "$buildkit_metadata")"
if [[ -z "$image_digest" ]]; then
  image_digest="$(docker image inspect --format '{{.Id}}' "$image_reference")"
fi

metadata="$metadata_dir/${runner_id}.json"
jq -n \
  --arg runnerId "$runner_id" \
  --arg usagebenchRelease "$usagebench_release" \
  --arg environmentVersion "$environment_version" \
  --arg canonicalPlatform "$canonical_platform" \
  --arg definitionDigest "$definition_digest" \
  --arg imageReference "$image_reference" \
  --arg imageDigest "$image_digest" \
  '{
    runnerId: $runnerId,
    usagebenchRelease: $usagebenchRelease,
    environmentVersion: $environmentVersion,
    canonicalPlatform: $canonicalPlatform,
    definitionDigest: $definitionDigest,
    imageReference: $imageReference,
    imageDigest: $imageDigest
  }' > "$metadata"

cat "$metadata"
