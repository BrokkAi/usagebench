#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
manifest="$repo_root/containers/reference/v1/manifest.json"

usage() {
  echo "usage: $0 RUNNER_ID CORPUS_ROOT OUTPUT_FILE [CASE_PATH] [CASE_ID] [INCLUDE_UNSUPPORTED]" >&2
  exit 2
}

runner_id="${1:-}"
corpus_root="${2:-}"
output_file="${3:-}"
case_path="${4:-benchmarks/cases}"
case_id="${5:-}"
include_unsupported="${6:-false}"
[[ -n "$runner_id" && -n "$corpus_root" && -n "$output_file" ]] || usage
[[ "$include_unsupported" == "true" || "$include_unsupported" == "false" ]] || usage
[[ "$case_path" != /* && "$case_path" != *..* ]] || {
  echo "case path must be relative to the corpus root" >&2
  exit 1
}

corpus_root="$(cd "$corpus_root" && pwd)"
[[ -f "$corpus_root/.usagebench-release.json" ]] || {
  echo "reference runs require a released corpus containing .usagebench-release.json" >&2
  exit 1
}
[[ -e "$corpus_root/$case_path" ]] || {
  echo "case path does not exist in corpus: $case_path" >&2
  exit 1
}

output_dir="$(cd "$(dirname "$output_file")" && pwd)"
output_name="$(basename "$output_file")"
[[ "$output_name" != */* && "$output_name" != "." && "$output_name" != ".." ]] || usage

corpus_release="$(jq -er '.releaseTag | select(type == "string")' "$corpus_root/.usagebench-release.json")"
corpus_revision="$(jq -er '.revision | select(type == "string")' "$corpus_root/.usagebench-release.json")"
[[ "$corpus_release" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ && "$corpus_revision" =~ ^[0-9a-f]{40}$ ]] || {
  echo "released corpus metadata is invalid" >&2
  exit 1
}

metadata="$repo_root/target/reference/${runner_id}.json"
[[ -f "$metadata" ]] || {
  echo "build the $runner_id reference image first with scripts/reference-image.sh" >&2
  exit 1
}

image_reference="$(jq -r '.imageReference' "$metadata")"
image_digest="$(jq -r '.imageDigest' "$metadata")"
image_release="$(jq -r '.usagebenchRelease' "$metadata")"
image_revision="$(jq -r '.usagebenchRevision' "$metadata")"
environment_version="$(jq -r '.environmentVersion' "$metadata")"
canonical_platform="$(jq -r '.canonicalPlatform' "$metadata")"
definition_digest="$(jq -r '.definitionDigest' "$metadata")"
[[ "$image_release" == "$corpus_release" && "$image_revision" == "$corpus_revision" ]] || {
  echo "reference image metadata is for $image_release at $image_revision, not corpus $corpus_release at $corpus_revision" >&2
  exit 1
}

loaded_image_id="$(docker image inspect --format '{{.Id}}' "$image_reference")"
[[ "$loaded_image_id" == "$image_digest" ]] || {
  echo "local tag $image_reference no longer identifies the image recorded in $metadata" >&2
  exit 1
}
docker image inspect "$loaded_image_id" | jq -e \
  --arg runner "$runner_id" \
  --arg release "$corpus_release" \
  --arg revision "$corpus_revision" \
  --arg environmentVersion "$environment_version" \
  --arg definitionDigest "$definition_digest" \
  '.[0]
   | .Os == "linux"
     and .Architecture == "amd64"
     and .Config.User == "65532:65532"
     and .Config.Labels["ai.brokk.usagebench.runner.id"] == $runner
     and .Config.Labels["ai.brokk.usagebench.release"] == $release
     and .Config.Labels["org.opencontainers.image.revision"] == $revision
     and .Config.Labels["ai.brokk.usagebench.environment.version"] == $environmentVersion
     and .Config.Labels["ai.brokk.usagebench.environment.definition-digest"] == $definitionDigest' \
  >/dev/null || {
    echo "loaded image labels or platform do not match the reference metadata" >&2
    exit 1
  }

toolchains="$(jq -c --arg runner "$runner_id" '.runners[$runner].toolchains' "$manifest")"
environment_descriptor="$(jq -cn \
  --arg version "$environment_version" \
  --arg definitionDigest "$definition_digest" \
  --arg canonicalPlatform "$canonical_platform" \
  --arg imageReference "$image_reference" \
  --arg imageDigest "$image_digest" \
  --arg usagebenchRelease "$corpus_release" \
  --arg usagebenchRevision "$corpus_revision" \
  --arg runnerId "$runner_id" \
  --argjson toolchains "$toolchains" \
  '{
    version: $version,
    definitionDigest: $definitionDigest,
    canonicalPlatform: $canonicalPlatform,
    imageReference: $imageReference,
    imageDigest: $imageDigest,
    usagebenchRelease: $usagebenchRelease,
    usagebenchRevision: $usagebenchRevision,
    runnerId: $runnerId,
    toolchains: $toolchains
  }')"

container_uid="$(id -u)"
container_gid="$(id -g)"
if [[ "$container_uid" == "0" ]]; then
  container_uid=65532
  container_gid=65532
fi

output_staging="$(mktemp -d /tmp/usagebench-reference-output.XXXXXX)"
output_tmp=""
cleanup() {
  if [[ -n "$output_tmp" ]]; then
    rm -f -- "$output_tmp"
  fi
  if [[ "$output_staging" == /tmp/usagebench-reference-output.* ]]; then
    rm -rf -- "$output_staging"
  fi
}
trap cleanup EXIT
chmod 0777 "$output_staging"

docker_args=(
  run --rm
  --platform "$canonical_platform"
  --network none
  --read-only
  --user "$container_uid:$container_gid"
  --tmpfs /tmp:rw,noexec,nosuid,size=256m,mode=1777
  --tmpfs /work:rw,nosuid,size=2g,mode=1777
  --mount "type=bind,src=$corpus_root,dst=/corpus,readonly"
  --mount "type=bind,src=$output_staging,dst=/output"
  --env "USAGEBENCH_REFERENCE_ENVIRONMENT=$environment_descriptor"
  "$loaded_image_id"
)

if [[ "$runner_id" == "bifrost" ]]; then
  bifrost_revision="$(jq -r '.runners.bifrost.analyzer.revision' "$manifest")"
  command_args=(
    run-bifrost "/corpus/$case_path" \
    --bifrost-binary /usr/local/bin/bifrost \
    --bifrost-resolved-commit "$bifrost_revision" \
    --work-dir /work \
    --output /output/report.json
  )
  if [[ -n "$case_id" ]]; then
    command_args+=(--case-id "$case_id")
  fi
  if [[ "$include_unsupported" == "true" ]]; then
    command_args+=(--include-unsupported)
  fi
  set +e
  docker "${docker_args[@]}" "${command_args[@]}"
  run_status=$?
  set -e
elif [[ "$runner_id" == "gopls" ]]; then
  command_args=(
    run-lsp "/corpus/$case_path"
    --profile /corpus/adapters/lsp/gopls.json
    --server-command /usr/local/bin/gopls
    --work-dir /work
    --output /output/report.json
  )
  if [[ -n "$case_id" ]]; then
    command_args+=(--case-id "$case_id")
  fi
  if [[ "$include_unsupported" == "true" ]]; then
    command_args+=(--include-unsupported)
  fi
  set +e
  docker "${docker_args[@]}" "${command_args[@]}"
  run_status=$?
  set -e
else
  echo "unknown reference runner: $runner_id" >&2
  exit 1
fi

[[ -f "$output_staging/report.json" && ! -L "$output_staging/report.json" ]] || {
  echo "reference run exited with status $run_status without writing a regular report" >&2
  if [[ "$run_status" == "0" ]]; then
    exit 1
  fi
  exit "$run_status"
}
output_tmp="$(mktemp "$output_dir/.usagebench-report.XXXXXX")"
cp -- "$output_staging/report.json" "$output_tmp"
mv -f -- "$output_tmp" "$output_dir/$output_name"
output_tmp=""
exit "$run_status"
