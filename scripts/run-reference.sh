#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
manifest="$repo_root/containers/reference/v1/manifest.json"

usage() {
  echo "usage: $0 RUNNER_ID CORPUS_ROOT OUTPUT_FILE [CASE_PATH] [CASE_ID]" >&2
  exit 2
}

runner_id="${1:-}"
corpus_root="${2:-}"
output_file="${3:-}"
case_path="${4:-benchmarks/cases}"
case_id="${5:-}"
[[ -n "$runner_id" && -n "$corpus_root" && -n "$output_file" ]] || usage
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

metadata="$repo_root/target/reference/${runner_id}.json"
[[ -f "$metadata" ]] || {
  echo "build the $runner_id reference image first with scripts/reference-image.sh" >&2
  exit 1
}

image_reference="$(jq -r '.imageReference' "$metadata")"
image_digest="$(jq -r '.imageDigest' "$metadata")"
environment_version="$(jq -r '.environmentVersion' "$metadata")"
canonical_platform="$(jq -r '.canonicalPlatform' "$metadata")"
definition_digest="$(jq -r '.definitionDigest' "$metadata")"
toolchains="$(jq -c --arg runner "$runner_id" '.runners[$runner].toolchains' "$manifest")"
environment_descriptor="$(jq -cn \
  --arg version "$environment_version" \
  --arg definitionDigest "$definition_digest" \
  --arg canonicalPlatform "$canonical_platform" \
  --arg imageReference "$image_reference" \
  --arg imageDigest "$image_digest" \
  --argjson toolchains "$toolchains" \
  '{
    version: $version,
    definitionDigest: $definitionDigest,
    canonicalPlatform: $canonicalPlatform,
    imageReference: $imageReference,
    imageDigest: $imageDigest,
    toolchains: $toolchains
  }')"

docker_args=(
  run --rm
  --platform "$canonical_platform"
  --network none
  --read-only
  --user "$(id -u):$(id -g)"
  --tmpfs /tmp:rw,noexec,nosuid,size=256m,mode=1777
  --tmpfs /work:rw,nosuid,size=2g,mode=1777
  --mount "type=bind,src=$corpus_root,dst=/corpus,readonly"
  --mount "type=bind,src=$output_dir,dst=/output"
  --env "USAGEBENCH_REFERENCE_ENVIRONMENT=$environment_descriptor"
  "$image_reference"
)

if [[ "$runner_id" == "bifrost" ]]; then
  bifrost_revision="$(jq -r '.runners.bifrost.analyzer.revision' "$manifest")"
  command_args=(
    run-bifrost "/corpus/$case_path" \
    --bifrost-binary /usr/local/bin/bifrost \
    --bifrost-resolved-commit "$bifrost_revision" \
    --work-dir /work \
    --output "/output/$output_name"
  )
  if [[ -n "$case_id" ]]; then
    command_args+=(--case-id "$case_id")
  fi
  docker "${docker_args[@]}" "${command_args[@]}"
elif [[ "$runner_id" == "gopls" ]]; then
  command_args=(
    run-lsp "/corpus/$case_path"
    --profile /corpus/adapters/lsp/gopls.json
    --server-command /usr/local/bin/gopls
    --work-dir /work
    --output "/output/$output_name"
  )
  if [[ -n "$case_id" ]]; then
    command_args+=(--case-id "$case_id")
  fi
  docker "${docker_args[@]}" "${command_args[@]}"
else
  echo "unknown reference runner: $runner_id" >&2
  exit 1
fi
