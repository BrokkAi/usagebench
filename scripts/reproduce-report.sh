#!/usr/bin/env bash
set -euo pipefail

script_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
  echo "usage: $0 PUBLISHED_REPORT [OUTPUT_REPORT]" >&2
  exit 2
}

for command_name in docker git jq; do
  command -v "$command_name" >/dev/null 2>&1 || {
    echo "required command not found: $command_name" >&2
    exit 1
  }
done

published_input="${1:-}"
output_input="${2:-reproduced-report.json}"
[[ -n "$published_input" && -f "$published_input" ]] || usage

published_report="$(cd "$(dirname "$published_input")" && pwd)/$(basename "$published_input")"
mkdir -p "$(dirname "$output_input")"
output_report="$(cd "$(dirname "$output_input")" && pwd)/$(basename "$output_input")"
[[ "$published_report" != "$output_report" ]] || {
  echo "output report must not overwrite the published report" >&2
  exit 1
}

release_tag="$(jq -er '.usagebenchRelease | select(type == "string")' "$published_report")"
release_revision="$(jq -er '.usagebenchRevision | select(type == "string")' "$published_report")"
environment_version="$(jq -er '.environment.referenceEnvironment.version | select(type == "string")' "$published_report")"
execution_mode="$(jq -er '.environment.executionMode | select(type == "string")' "$published_report")"
platform_scope="$(jq -er '.environment.platformScope | select(type == "string")' "$published_report")"
reported_definition_digest="$(jq -er '.environment.referenceEnvironment.definitionDigest | select(type == "string")' "$published_report")"
runner_id="$(jq -er '.runner.name | select(type == "string")' "$published_report")"
case_id="$(jq -r '.invocation.caseId // ""' "$published_report")"
include_unsupported="$(jq -r '.invocation.includeUnsupported' "$published_report")"

[[ "$release_tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || {
  echo "published report does not name a semantic UsageBench release" >&2
  exit 1
}
[[ "$release_revision" =~ ^[0-9a-f]{40}$ ]] || {
  echo "published report does not contain an exact UsageBench revision" >&2
  exit 1
}
[[ "$environment_version" == "1" ]] || {
  echo "unsupported reference-environment version: $environment_version" >&2
  exit 1
}
[[ "$execution_mode" == "container" && "$platform_scope" == "canonical_reference" ]] || {
  echo "only canonical container reports can be reproduced with this command" >&2
  exit 1
}
[[ "$runner_id" == "bifrost" || "$runner_id" == "gopls" ]] || {
  echo "unsupported reference runner: $runner_id" >&2
  exit 1
}
[[ "$include_unsupported" == "true" || "$include_unsupported" == "false" ]] || {
  echo "published report has an invalid includeUnsupported value" >&2
  exit 1
}

reported_case_files=()
while IFS= read -r case_file; do
  reported_case_files+=("$case_file")
done < <(jq -er '.caseFiles[] | select(type == "string")' "$published_report")
(( ${#reported_case_files[@]} > 0 )) || {
  echo "published report does not contain any case files" >&2
  exit 1
}

relative_case_files=()
for case_file in "${reported_case_files[@]}"; do
  [[ "$case_file" == /corpus/* ]] || {
    echo "published container case path is outside /corpus: $case_file" >&2
    exit 1
  }
  relative_case="${case_file#/corpus/}"
  [[ "$relative_case" != /* && "$relative_case" != *..* ]] || {
    echo "published report contains an unsafe case path: $case_file" >&2
    exit 1
  }
  relative_case_files+=("$relative_case")
done

if (( ${#relative_case_files[@]} == 1 )); then
  case_path="${relative_case_files[0]}"
else
  case_path="$(dirname "${relative_case_files[0]}")"
  for case_file in "${relative_case_files[@]:1}"; do
    while [[ "$case_file" != "$case_path" && "$case_file" != "$case_path/"* ]]; do
      next_path="$(dirname "$case_path")"
      [[ "$next_path" != "$case_path" ]] || {
        echo "could not derive a common case directory from the published report" >&2
        exit 1
      }
      case_path="$next_path"
    done
  done
fi

release_root="$script_root"
current_release=""
current_revision=""
if [[ -f "$release_root/.usagebench-release.json" ]]; then
  current_release="$(jq -r '.releaseTag // ""' "$release_root/.usagebench-release.json")"
  current_revision="$(jq -r '.revision // ""' "$release_root/.usagebench-release.json")"
elif git -C "$release_root" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  current_revision="$(git -C "$release_root" rev-parse HEAD)"
  current_release="$(git -C "$release_root" tag --points-at HEAD --list "$release_tag" | head -n 1)"
fi

reproduction_tmp=""
cleanup() {
  if [[ -n "$reproduction_tmp" && "$reproduction_tmp" == /tmp/* ]]; then
    rm -rf -- "$reproduction_tmp"
  fi
}
trap cleanup EXIT

if [[ "$current_release" != "$release_tag" || "$current_revision" != "$release_revision" ]]; then
  reproduction_tmp="$(mktemp -d /tmp/usagebench-reproduce.XXXXXX)"
  release_root="$reproduction_tmp/usagebench"
  git clone --quiet --depth 1 --branch "$release_tag" \
    https://github.com/BrokkAi/usagebench.git "$release_root"
  cloned_revision="$(git -C "$release_root" rev-parse HEAD)"
  [[ "$cloned_revision" == "$release_revision" ]] || {
    echo "release $release_tag resolved to $cloned_revision, expected $release_revision" >&2
    exit 1
  }
fi

[[ -x "$release_root/scripts/reference-image.sh" && -x "$release_root/scripts/run-reference.sh" ]] || {
  echo "release $release_tag does not contain reference-environment tooling" >&2
  exit 1
}
[[ -e "$release_root/$case_path" ]] || {
  echo "release $release_tag does not contain case selection $case_path" >&2
  exit 1
}

"$release_root/scripts/reference-image.sh" "$runner_id" "$release_tag"
image_metadata="$release_root/target/reference/${runner_id}.json"
built_definition_digest="$(jq -r '.definitionDigest' "$image_metadata")"
[[ "$built_definition_digest" == "$reported_definition_digest" ]] || {
  echo "reference definition mismatch: built $built_definition_digest, report records $reported_definition_digest" >&2
  exit 1
}

set +e
"$release_root/scripts/run-reference.sh" \
  "$runner_id" "$release_root" "$output_report" "$case_path" "$case_id" "$include_unsupported"
run_status=$?
set -e
[[ -f "$output_report" ]] || {
  echo "reference run exited with status $run_status without writing a report" >&2
  exit "$run_status"
}

image_reference="$(jq -r '.imageReference' "$image_metadata")"
canonical_platform="$(jq -r '.canonicalPlatform' "$image_metadata")"
published_dir="$(dirname "$published_report")"
published_name="$(basename "$published_report")"
output_dir="$(dirname "$output_report")"
output_name="$(basename "$output_report")"

docker run --rm \
  --platform "$canonical_platform" \
  --network none \
  --read-only \
  --user "$(id -u):$(id -g)" \
  --mount "type=bind,src=$published_dir,dst=/expected,readonly" \
  --mount "type=bind,src=$output_dir,dst=/actual,readonly" \
  "$image_reference" \
  compare-reports "/expected/$published_name" "/actual/$output_name"

echo "reproduced report: $output_report"
