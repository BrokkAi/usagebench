#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
manifest="$repo_root/containers/reference/v1/manifest.json"
schema="$repo_root/schema/reference-environment.schema.json"
dockerfile="$repo_root/containers/reference/v1/Dockerfile"
candidate_registry="$repo_root/adapters/candidates.json"

usage() {
  echo "usage: $0 RUNNER_ID USAGEBENCH_RELEASE [USAGEBENCH_REVISION]" >&2
  echo "set USAGEBENCH_REFERENCE_IMAGE_FORCE_REBUILD=1 to bypass all reuse" >&2
  echo "set USAGEBENCH_REFERENCE_IMAGE_PUBLISH=1 after an authenticated registry login" >&2
  exit 2
}

sha256_stream() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum | awk '{print $1}'
  else
    shasum -a 256 | awk '{print $1}'
  fi
}

for command_name in docker jq python3; do
  command -v "$command_name" >/dev/null 2>&1 || {
    echo "required command not found: $command_name" >&2
    exit 1
  }
done

runner_id="${1:-}"
usagebench_release="${2:-}"
requested_revision="${3:-}"
force_rebuild="${USAGEBENCH_REFERENCE_IMAGE_FORCE_REBUILD:-0}"
publish_image="${USAGEBENCH_REFERENCE_IMAGE_PUBLISH:-0}"
[[ -n "$runner_id" && "$usagebench_release" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || usage
[[ -z "$requested_revision" || "$requested_revision" =~ ^[0-9a-f]{40}$ ]] || usage
[[ "$force_rebuild" =~ ^[01]$ && "$publish_image" =~ ^[01]$ ]] || usage
[[ "$force_rebuild" != "1" || "$publish_image" != "1" ]] || {
  echo "forced recipe validation must not publish or replace a registry tag" >&2
  exit 1
}
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
bifrost_revision="$(jq -r '.candidates[] | select(.id == "bifrost") | .revision' "$candidate_registry")"
gopls_version="$(jq -r '.candidates[] | select(.id == "gopls") | .requestedVersion' "$candidate_registry")"
gopls_checksum="$(jq -r '.candidates[] | select(.id == "gopls") | .moduleChecksum' "$candidate_registry")"
[[ "$bifrost_revision" == "$(jq -r '.runners.bifrost.analyzer.revision' "$manifest")" ]] || {
  echo "Bifrost candidate registry does not match the reference environment" >&2
  exit 1
}
[[ "$gopls_version" == "$(jq -r '.runners.gopls.analyzer.requestedVersion' "$manifest")" \
  && "$gopls_checksum" == "$(jq -r '.runners.gopls.analyzer.moduleChecksum' "$manifest")" ]] || {
  echo "gopls candidate registry does not match the reference environment" >&2
  exit 1
}

if [[ "$runner_id" == "bifrost" ]]; then
  analyzer_identity="$bifrost_revision"
else
  analyzer_identity="gopls@$gopls_version:$gopls_checksum"
fi

definition_digest="sha256:$(
  for definition_file in \
    "$manifest" \
    "$schema" \
    "$dockerfile" \
    "$candidate_registry" \
    "$repo_root/scripts/reference-image.sh" \
    "$repo_root/scripts/run-reference.sh"; do
    printf '%s\0' "${definition_file#$repo_root/}"
    cat "$definition_file"
  done | sha256_stream
)"

identity_digest="sha256:$({
  printf 'runnerId=%s\0' "$runner_id"
  printf 'usagebenchRelease=%s\0' "$usagebench_release"
  printf 'usagebenchRevision=%s\0' "$source_revision"
  printf 'environmentVersion=%s\0' "$environment_version"
  printf 'definitionDigest=%s\0' "$definition_digest"
  printf 'analyzerIdentity=%s\0' "$analyzer_identity"
  printf 'canonicalPlatform=%s\0' "$canonical_platform"
} | sha256_stream)"

tag_template="$(jq -r '.localTagTemplate' "$manifest")"
image_reference="${tag_template//\{usagebenchRelease\}/$usagebench_release}"
image_reference="${image_reference//\{environmentVersion\}/$environment_version}"
image_reference="${image_reference//\{runnerId\}/$runner_id}"
[[ "$image_reference" != *'{'* && "$image_reference" != *'}'* ]] || {
  echo "reference image tag template contains an unknown placeholder" >&2
  exit 1
}
registry="$(jq -er '.registry' "$manifest")"
identity_tag="$runner_id-${identity_digest#sha256:}"
registry_tag_reference="$registry:$identity_tag"
metadata_dir="$repo_root/target/reference"
mkdir -p "$metadata_dir"
buildkit_metadata="$metadata_dir/${runner_id}.buildkit.json"
metadata="$metadata_dir/${runner_id}.json"
resolution_started_ns="$(python3 -c 'import time; print(time.time_ns())')"
image_resolution_ms=0
image_construction_ms=""

finish_resolution_timing() {
  local finished_ns
  finished_ns="$(python3 -c 'import time; print(time.time_ns())')"
  image_resolution_ms="$(( (finished_ns - resolution_started_ns) / 1000000 ))"
  echo "phase timing: canonical image resolution $image_resolution_ms ms" >&2
  if [[ -n "$image_construction_ms" ]]; then
    echo "phase timing: canonical image construction $image_construction_ms ms" >&2
  fi
}

verify_label() {
  local image="$1"
  local label="$2"
  local expected="$3"
  local actual
  actual="$(docker image inspect --format "{{ index .Config.Labels \"$label\" }}" "$image")"
  [[ "$actual" == "$expected" ]] || {
    echo "reference image label $label is $actual, expected $expected" >&2
    return 1
  }
}

verify_loaded_image() {
  local image="$1"
  local actual_platform
  actual_platform="$(docker image inspect --format '{{.Os}}/{{.Architecture}}' "$image")"
  [[ "$actual_platform" == "$canonical_platform" ]] || {
    echo "reference image platform is $actual_platform, expected $canonical_platform" >&2
    return 1
  }
  verify_label "$image" ai.brokk.usagebench.release "$usagebench_release"
  verify_label "$image" org.opencontainers.image.revision "$source_revision"
  verify_label "$image" ai.brokk.usagebench.runner.id "$runner_id"
  verify_label "$image" ai.brokk.usagebench.environment.version "$environment_version"
  verify_label "$image" ai.brokk.usagebench.environment.definition-digest "$definition_digest"
  verify_label "$image" ai.brokk.usagebench.environment.identity-digest "$identity_digest"
  verify_label "$image" ai.brokk.usagebench.analyzer.identity "$analyzer_identity"
  verify_label "$image" ai.brokk.usagebench.canonical-platform "$canonical_platform"
  verified_image_digest="$(docker image inspect --format '{{.Id}}' "$image")"
  [[ "$verified_image_digest" =~ ^sha256:[0-9a-f]{64}$ ]] || {
    echo "loaded image does not have a sha256 image ID" >&2
    return 1
  }
}

resolve_registry_digest() {
  local reference="$1"
  local resolved
  set +e
  resolved="$(docker buildx imagetools inspect "$reference" --format '{{.Manifest.Digest}}' 2>/dev/null)"
  local status=$?
  set -e
  if [[ "$status" -ne 0 || ! "$resolved" =~ ^sha256:[0-9a-f]{64}$ ]]; then
    return 1
  fi
  printf '%s\n' "$resolved"
}

write_metadata() {
  local recorded_reference="$1"
  local image_digest="$2"
  local registry_digest="$3"
  local buildkit_digest="$4"
  local reuse_status="$5"
  jq -n \
    --arg runnerId "$runner_id" \
    --arg usagebenchRelease "$usagebench_release" \
    --arg usagebenchRevision "$source_revision" \
    --arg environmentVersion "$environment_version" \
    --arg canonicalPlatform "$canonical_platform" \
    --arg definitionDigest "$definition_digest" \
    --arg identityDigest "$identity_digest" \
    --arg analyzerIdentity "$analyzer_identity" \
    --arg imageReference "$recorded_reference" \
    --arg imageDigest "$image_digest" \
    --arg registryTag "$registry_tag_reference" \
    --arg registryDigest "$registry_digest" \
    --arg buildkitDigest "$buildkit_digest" \
    --arg reuseStatus "$reuse_status" \
    --argjson imageResolutionMs "$image_resolution_ms" \
    --arg imageConstructionMs "$image_construction_ms" \
    '{
      runnerId: $runnerId,
      usagebenchRelease: $usagebenchRelease,
      usagebenchRevision: $usagebenchRevision,
      environmentVersion: $environmentVersion,
      canonicalPlatform: $canonicalPlatform,
      definitionDigest: $definitionDigest,
      identityDigest: $identityDigest,
      analyzerIdentity: $analyzerIdentity,
      imageReference: $imageReference,
      imageDigest: $imageDigest,
      registryTag: $registryTag,
      registryDigest: (if $registryDigest == "" then null else $registryDigest end),
      buildkitDigest: (if $buildkitDigest == "" then null else $buildkitDigest end),
      reuseStatus: $reuseStatus,
      imageResolutionMs: $imageResolutionMs,
      imageConstructionMs: (if $imageConstructionMs == "" then null else ($imageConstructionMs | tonumber) end)
    }' > "$metadata"
  cat "$metadata"
}

if [[ "$force_rebuild" != "1" && -f "$metadata" ]] && jq -e \
  --arg runnerId "$runner_id" \
  --arg usagebenchRelease "$usagebench_release" \
  --arg usagebenchRevision "$source_revision" \
  --arg environmentVersion "$environment_version" \
  --arg canonicalPlatform "$canonical_platform" \
  --arg definitionDigest "$definition_digest" \
  --arg identityDigest "$identity_digest" \
  --arg analyzerIdentity "$analyzer_identity" \
  '.runnerId == $runnerId
   and .usagebenchRelease == $usagebenchRelease
   and .usagebenchRevision == $usagebenchRevision
   and .environmentVersion == $environmentVersion
   and .canonicalPlatform == $canonicalPlatform
   and .definitionDigest == $definitionDigest
   and .identityDigest == $identityDigest
   and .analyzerIdentity == $analyzerIdentity
   and (.imageDigest | test("^sha256:[0-9a-f]{64}$"))' "$metadata" >/dev/null; then
  cached_image_digest="$(jq -r .imageDigest "$metadata")"
  if docker image inspect "$cached_image_digest" >/dev/null 2>&1 \
    && verify_loaded_image "$cached_image_digest"; then
    cached_reference="$(jq -r .imageReference "$metadata")"
    cached_registry_digest="$(jq -r '.registryDigest // empty' "$metadata")"
    cached_buildkit_digest="$(jq -r '.buildkitDigest // empty' "$metadata")"
    if [[ "$publish_image" == "1" ]]; then
      if ! cached_registry_digest="$(resolve_registry_digest "$registry_tag_reference")"; then
        docker tag "$cached_image_digest" "$registry_tag_reference"
        docker push "$registry_tag_reference" >/dev/null
        cached_registry_digest="$(resolve_registry_digest "$registry_tag_reference")" || {
          echo "published cached image did not resolve to an immutable registry digest" >&2
          exit 1
        }
      fi
      cached_reference="$registry@$cached_registry_digest"
    fi
    echo "reusing verified local canonical image $cached_image_digest" >&2
    finish_resolution_timing
    write_metadata "$cached_reference" "$verified_image_digest" "$cached_registry_digest" "$cached_buildkit_digest" local
    exit 0
  fi
  echo "local canonical image cache failed verification; checking the immutable registry image" >&2
fi

if [[ "$force_rebuild" != "1" ]] && docker image inspect "$image_reference" >/dev/null 2>&1 \
  && verify_loaded_image "$image_reference"; then
  local_registry_digest=""
  local_reference="$image_reference"
  if [[ "$publish_image" == "1" ]]; then
    docker tag "$verified_image_digest" "$registry_tag_reference"
    docker push "$registry_tag_reference" >/dev/null
    local_registry_digest="$(resolve_registry_digest "$registry_tag_reference")" || {
      echo "published local image did not resolve to an immutable registry digest" >&2
      exit 1
    }
    local_reference="$registry@$local_registry_digest"
  fi
  echo "reusing verified local canonical image $verified_image_digest" >&2
  finish_resolution_timing
  write_metadata "$local_reference" "$verified_image_digest" "$local_registry_digest" "" local
  exit 0
fi

if [[ "$force_rebuild" != "1" ]] && registry_digest="$(resolve_registry_digest "$registry_tag_reference")"; then
  immutable_reference="$registry@$registry_digest"
  docker pull --platform "$canonical_platform" "$immutable_reference" >/dev/null
  verify_loaded_image "$immutable_reference"
  echo "restored verified canonical image $immutable_reference" >&2
  finish_resolution_timing
  write_metadata "$immutable_reference" "$verified_image_digest" "$registry_digest" "" registry
  exit 0
fi

if [[ "$force_rebuild" == "1" ]]; then
  echo "forced canonical image rebuild; bypassing local and registry reuse" >&2
else
  echo "no verified canonical image exists for $identity_digest; building it" >&2
fi

build_tags=(--tag "$image_reference")
if [[ "$publish_image" == "1" ]]; then
  build_tags+=(--tag "$registry_tag_reference")
fi
construction_started_ns="$(python3 -c 'import time; print(time.time_ns())')"
docker buildx build \
  --platform "$canonical_platform" \
  --provenance=false \
  --load \
  --target "$target" \
  --file "$dockerfile" \
  "${build_tags[@]}" \
  --metadata-file "$buildkit_metadata" \
  --build-arg "RUST_BASE=$rust_base" \
  --build-arg "BIFROST_BASE=$bifrost_base" \
  --build-arg "GO_BASE=$go_base" \
  --build-arg "RUNTIME_BASE=$runtime_base" \
  --build-arg "USAGEBENCH_RELEASE=$usagebench_release" \
  --build-arg "USAGEBENCH_REVISION=$source_revision" \
  --build-arg "ENVIRONMENT_VERSION=$environment_version" \
  --build-arg "DEFINITION_DIGEST=$definition_digest" \
  --build-arg "IDENTITY_DIGEST=$identity_digest" \
  --build-arg "ANALYZER_IDENTITY=$analyzer_identity" \
  --build-arg "BIFROST_REVISION=$bifrost_revision" \
  --build-arg "GOPLS_VERSION=$gopls_version" \
  --build-arg "GOPLS_MODULE_CHECKSUM=$gopls_checksum" \
  "$repo_root"
construction_finished_ns="$(python3 -c 'import time; print(time.time_ns())')"
image_construction_ms="$(( (construction_finished_ns - construction_started_ns) / 1000000 ))"

buildkit_digest="$(jq -r '."containerimage.digest" // empty' "$buildkit_metadata")"
verify_loaded_image "$image_reference"
recorded_reference="$image_reference"
registry_digest=""
if [[ "$publish_image" == "1" ]]; then
  docker push "$registry_tag_reference" >/dev/null
  registry_digest="$(resolve_registry_digest "$registry_tag_reference")" || {
    echo "published image did not resolve to an immutable registry digest" >&2
    exit 1
  }
  recorded_reference="$registry@$registry_digest"
  echo "published canonical image $recorded_reference" >&2
fi
finish_resolution_timing
write_metadata "$recorded_reference" "$verified_image_digest" "$registry_digest" "$buildkit_digest" built
