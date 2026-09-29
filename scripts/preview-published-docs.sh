#!/usr/bin/env bash
# Render the docs locally from the published immutable releases, as the Docs
# workflow does before it builds: download each release's bundle, verify it
# against its checksum and tag, then generate the evidence map and the result
# pages from it. Without this the site renders its "no published comparison"
# state, which is what the committed tree deliberately holds.
#
# The generated pages are written into the working tree. Run with --restore to
# put the committed, unpublished state back before committing.
#
# usage: scripts/preview-published-docs.sh [--restore]
# Release tags default to the ones the live site is built from; override with
# V1_RELEASE_TAG, V2_RELEASE_TAG and LEGACY_RELEASE_TAG (empty to omit legacy).
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

generated=(
  docs/src/content/docs/results/case-comparison.md
  docs/src/content/docs/results/evidence.md
  docs/src/content/docs/results/index.md
  docs/src/data/evidence.json
)

if [[ "${1:-}" == --restore ]]; then
  git checkout -- "${generated[@]}"
  echo "restored the committed, unpublished docs state"
  exit 0
fi

v1_tag="${V1_RELEASE_TAG-v0.2.0}"
v2_tag="${V2_RELEASE_TAG-v0.3.4}"
legacy_tag="${LEGACY_RELEASE_TAG-v0.3.1}"

cache="${XDG_CACHE_HOME:-$HOME/.cache}/usagebench/publication"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

git fetch --quiet --tags origin

# Prints the path of the verified, extracted bundle for one release.
fetch_bundle() {
  local slot="$1" tag="$2"
  local archive="usagebench-${tag}.tar.gz"
  local revision
  revision="$(git rev-parse --verify "refs/tags/$tag^{}")"
  mkdir -p "$cache/$tag"
  if [[ ! -f "$cache/$tag/$archive" || ! -f "$cache/$tag/$archive.sha256" ]]; then
    gh release download "$tag" --pattern "$archive" --pattern "$archive.sha256" \
      --dir "$cache/$tag" --clobber >&2
  fi
  local args=(--archive "$cache/$tag/$archive" --checksum "$cache/$tag/$archive.sha256"
    --extract-to "$work/$slot" --tag "$tag" --revision "$revision")
  [[ "$slot" == v1 ]] && args+=(--historical-v1)
  python3 scripts/validate-publication-bundle.py "${args[@]}" >&2
  echo "$work/$slot/usagebench-$tag"
}

bundles=(--bundle "$(fetch_bundle v1 "$v1_tag")")
v2_bundle="$(fetch_bundle v2 "$v2_tag")"
bundles+=(--bundle "$v2_bundle")
if [[ -n "$legacy_tag" ]]; then
  bundles+=(--bundle "$(fetch_bundle legacy "$legacy_tag")")
fi

python3 scripts/generate-docs-evidence.py "${bundles[@]}"
python3 scripts/generate-docs-results.py --bundle "$v2_bundle"

echo "docs now render ${v2_tag} (v2), ${v1_tag} (v1)${legacy_tag:+ and ${legacy_tag} (legacy)}."
echo "run $0 --restore before committing."
