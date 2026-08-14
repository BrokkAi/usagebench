#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: $0 RUNNER_ID CASE_PATH CASE_ID RELEASE REVISION" >&2
  exit 2
}

runner_id="${1:-}"
case_path="${2:-}"
case_id="${3:-}"
release="${4:-}"
revision="${5:-}"
[[ "$runner_id" =~ ^[a-z0-9][a-z0-9-]*$ \
  && "$case_path" =~ ^[A-Za-z0-9._/-]+$ \
  && -n "$case_id" \
  && "$release" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ \
  && "$revision" =~ ^[0-9a-f]{40}$ ]] || usage

corpus="target/usagebench-$release"
scripts/stage-release-bundle.sh . "$corpus" "$release" "$revision"
scripts/reference-image.sh "$runner_id" "$release" "$revision"
# The workflow-level controls apply only to the explicit cold build or trusted
# publication above. Reproduction must exercise the ordinary verified reuse
# path against the image that operation produced.
unset USAGEBENCH_REFERENCE_IMAGE_FORCE_REBUILD
unset USAGEBENCH_REFERENCE_IMAGE_PUBLISH

image="usagebench-reference:$release-env1-$runner_id"
docker run --rm --network none --entrypoint /bin/sh "$image" -c '
  set -eu
  repository="$(mktemp -d)"
  git init --bare --quiet "$repository"
  printf "tree 4b825dc642cb6eb9a060e54bf8d69288fbee4904\nauthor UsageBench <usagebench@example.invalid> 0 +0000\ncommitter UsageBench <usagebench@example.invalid> 0 +0000\n\n" \
    | git hash-object -t commit --stdin >/dev/null
  printf "feature done\ncommit refs/heads/archive\ncommitter UsageBench <usagebench@example.invalid> 0 +0000\ndata 0\n\ndone\n" \
    | git --git-dir="$repository" fast-import --quiet
  git --git-dir="$repository" rev-parse "refs/heads/archive^{tree}" >/dev/null
'

mkdir -p target/reference-smoke
report="target/reference-smoke/$runner_id.json"
scripts/run-reference.sh "$runner_id" "$corpus" "$report" "$case_path" "$case_id"
jq -e \
  --arg runner "$runner_id" \
  --arg revision "$revision" \
  --arg release "$release" \
  '.usagebenchRelease == $release
   and .usagebenchRevision == $revision
   and .runner.name == $runner
   and .environment.executionMode == "container"
   and .environment.platformScope == "canonical_reference"
   and .environment.referenceEnvironment.version == "1"
   and .environment.referenceEnvironment.canonicalPlatform == "linux/amd64"
   and (.environment.referenceEnvironment.definitionDigest | test("^sha256:[0-9a-f]{64}$"))
   and (.environment.container.imageDigest | test("^sha256:[0-9a-f]{64}$"))
   and (.environment.analyzerExecutable.sha256 | test("^[0-9a-f]{64}$"))
   and .totals.passed == 1
   and .totals.failed == 0
   and .totals.errors == 0' "$report"

chmod 0600 "$report"
"$corpus/scripts/reproduce-report.sh" "$report" "target/reference-smoke/$runner_id-reproduced.json"
