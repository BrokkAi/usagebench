#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
manifest="$repo_root/containers/reference/v1/manifest.json"
schema="$repo_root/schema/reference-environment.schema.json"
dockerfile="$repo_root/containers/reference/v1/Dockerfile"

usage() {
  echo "usage: $0 RUNNER_ID USAGEBENCH_RELEASE [USAGEBENCH_REVISION]" >&2
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
requested_revision="${3:-}"
[[ -n "$runner_id" && "$usagebench_release" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || usage
[[ -z "$requested_revision" || "$requested_revision" =~ ^[0-9a-f]{40}$ ]] || usage
jq -e --arg runner "$runner_id" '.runners[$runner]' "$manifest" >/dev/null || {
  echo "unknown reference runner: $runner_id" >&2
  exit 1
}

if [[ -f "$repo_root/.usagebench-release.json" ]]; then
  source_release="$(jq -er '.releaseTag | select(type == "string")' "$repo_root/.usagebench-release.json")"
  source_revision="$(jq -er '.revision | select(type == "string")' "$repo_root/.usagebench-release.json")"
  [[ "$source_release" == "$usagebench_release" ]] || {
    echo "release bundle identifies $source_release, not $usagebench_release" >&2
    exit 1
  }
else
  git -C "$repo_root" rev-parse --is-inside-work-tree >/dev/null 2>&1 || {
    echo "build source has neither release metadata nor Git provenance" >&2
    exit 1
  }
  source_revision="$(git -C "$repo_root" rev-parse HEAD)"
  [[ -z "$(git -C "$repo_root" status --porcelain --untracked-files=normal)" ]] || {
    echo "refusing to build a reference image from a dirty worktree" >&2
    exit 1
  }
  if [[ -z "$requested_revision" ]]; then
    [[ "$(git -C "$repo_root" tag --points-at HEAD --list "$usagebench_release" | head -n 1)" == "$usagebench_release" ]] || {
      echo "worktree HEAD is not tagged $usagebench_release; pass the expected revision for a CI build" >&2
      exit 1
    }
  fi
fi
[[ "$source_revision" =~ ^[0-9a-f]{40}$ ]] || {
  echo "build source does not identify an exact UsageBench revision" >&2
  exit 1
}
if [[ -n "$requested_revision" && "$source_revision" != "$requested_revision" ]]; then
  echo "build source revision $source_revision does not match requested $requested_revision" >&2
  exit 1
fi

environment_version="$(jq -r '.environmentVersion' "$manifest")"
canonical_platform="$(jq -r '.canonicalPlatform' "$manifest")"
frontend="$(jq -r '.buildFrontend | .reference + "@" + .digest' "$manifest")"
read -r dockerfile_syntax < "$dockerfile"
[[ "$dockerfile_syntax" == "# syntax=$frontend" ]] || {
  echo "Dockerfile frontend does not match the reference-environment manifest" >&2
  exit 1
}
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

tag_template="$(jq -r '.localTagTemplate' "$manifest")"
image_reference="${tag_template//\{usagebenchRelease\}/$usagebench_release}"
image_reference="${image_reference//\{environmentVersion\}/$environment_version}"
image_reference="${image_reference//\{runnerId\}/$runner_id}"
[[ "$image_reference" != *'{'* && "$image_reference" != *'}'* ]] || {
  echo "reference image tag template contains an unknown placeholder" >&2
  exit 1
}
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
  --build-arg "USAGEBENCH_REVISION=$source_revision" \
  --build-arg "ENVIRONMENT_VERSION=$environment_version" \
  --build-arg "DEFINITION_DIGEST=$definition_digest" \
  --build-arg "BIFROST_REVISION=$bifrost_revision" \
  --build-arg "GOPLS_VERSION=$gopls_version" \
  --build-arg "GOPLS_MODULE_CHECKSUM=$gopls_checksum" \
  "$repo_root"

buildkit_digest="$(jq -r '."containerimage.digest" // empty' "$buildkit_metadata")"
image_digest="$(docker image inspect --format '{{.Id}}' "$image_reference")"
[[ "$image_digest" =~ ^sha256:[0-9a-f]{64}$ ]] || {
  echo "loaded image does not have a sha256 image ID" >&2
  exit 1
}

metadata="$metadata_dir/${runner_id}.json"
jq -n \
  --arg runnerId "$runner_id" \
  --arg usagebenchRelease "$usagebench_release" \
  --arg usagebenchRevision "$source_revision" \
  --arg environmentVersion "$environment_version" \
  --arg canonicalPlatform "$canonical_platform" \
  --arg definitionDigest "$definition_digest" \
  --arg imageReference "$image_reference" \
  --arg imageDigest "$image_digest" \
  --arg buildkitDigest "$buildkit_digest" \
  '{
    runnerId: $runnerId,
    usagebenchRelease: $usagebenchRelease,
    usagebenchRevision: $usagebenchRevision,
    environmentVersion: $environmentVersion,
    canonicalPlatform: $canonicalPlatform,
    definitionDigest: $definitionDigest,
    imageReference: $imageReference,
    imageDigest: $imageDigest,
    buildkitDigest: (if $buildkitDigest == "" then null else $buildkitDigest end)
  }' > "$metadata"

cat "$metadata"
