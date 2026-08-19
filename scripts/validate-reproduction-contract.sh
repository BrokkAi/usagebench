#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
registry="$repo_root/adapters/candidates.json"
reference_manifest="$repo_root/containers/reference/v1/manifest.json"
workflow="$repo_root/.github/workflows/reference-environments.yml"
reference_ci="$repo_root/scripts/reference-environment-ci.sh"
scope_resolver="$repo_root/scripts/resolve-freeze-scope.sh"
freeze_workflow="$repo_root/.github/workflows/freeze.yml"
docs_workflow="$repo_root/.github/workflows/docs.yml"
v030_registry="$repo_root/benchmarks/evaluation/real-project-v2/candidates-v0.3.0.json"
publication_validator="$repo_root/scripts/validate-publication-bundle.py"

[[ -f "$publication_validator" ]] || {
  echo "publication bundle validator is missing" >&2
  exit 1
}
grep -Fq 'v1_release_tag:' "$docs_workflow" && grep -Fq 'v2_release_tag:' "$docs_workflow" || {
  echo "docs publication must require immutable v1 and v2 release tags" >&2
  exit 1
}
grep -Fq 'gh release download "$tag"' "$docs_workflow" || {
  echo "docs publication does not download release-bound bundles" >&2
  exit 1
}
grep -Fq 'scripts/validate-publication-bundle.py' "$docs_workflow" || {
  echo "docs publication does not verify the immutable publication bundle" >&2
  exit 1
}
grep -Fq -- '--historical-v1' "$docs_workflow" || {
  echo "docs publication does not preserve the exact v0.2.0 compatibility path" >&2
  exit 1
}
grep -Fq -- '--output-directory "$bundle/results" --check' "$docs_workflow" || {
  echo "docs publication does not reject generated result drift" >&2
  exit 1
}
grep -Fq 'scripts/validate-publication-bundle.py' "$freeze_workflow" || {
  echo "freeze workflow does not verify generated publication provenance" >&2
  exit 1
}
grep -Fq 'actions/cache/restore@55cc8345863c7cc4c66a329aec7e433d2d1c52a9 # v6.1.0 (Node 24)' "$freeze_workflow" && \
grep -Fq 'actions/cache/save@55cc8345863c7cc4c66a329aec7e433d2d1c52a9 # v6.1.0 (Node 24)' "$freeze_workflow" || {
  echo "native legacy freeze does not use the Node 24 cache provider" >&2
  exit 1
}

python3 "$repo_root/scripts/build-real-project-v1-publication-review.py" --check
python3 "$repo_root/scripts/build-real-project-v2-publication-review.py" --check
python3 -m unittest "$repo_root/tests/test_publication_bundle.py"
cargo run --locked -- validate-legacy-promotion "$repo_root/benchmarks/promotion/legacy-v1/manifest.json"

jq -e '.schemaVersion == 3' "$registry" >/dev/null
jq -e '.schemaVersion == 3' "$v030_registry" >/dev/null
jq -e '
  [.candidates[] | select(.id == "bifrost")]
  == [{
    "id": "bifrost",
    "runner": "bifrost",
    "name": "Bifrost",
    "requestedVersion": "v0.10.2",
    "source": "https://github.com/BrokkAi/bifrost",
    "revision": "d1a7c0cc1cf58d0c0789476ad42a92318bb8da49",
    "advertised": true,
    "runtimeNetworking": "disabled",
    "projectHydration": "fixture sources are staged in the released corpus"
  }]
' "$v030_registry" >/dev/null
jq -e '
  [.candidates[] | select(
    .id == "bifrost"
    and .requestedVersion == "v0.10.1"
    and .revision == "511adaa2733067bb1b7809ab79e06ec0e3d2a146"
  )] | length == 1
' "$registry" >/dev/null

while IFS=$'\t' read -r candidate_id reference_runner; do
  [[ -n "$reference_runner" ]] || {
    echo "canonical candidate $candidate_id lacks referenceRunner" >&2
    exit 1
  }
  jq -e --arg runner "$reference_runner" '.runners[$runner]' "$reference_manifest" >/dev/null || {
    echo "canonical candidate $candidate_id references unknown environment $reference_runner" >&2
    exit 1
  }
  grep -Eq "^[[:space:]]*- runner: $reference_runner[[:space:]]*$" "$workflow" || {
    echo "canonical candidate $candidate_id lacks a reference-environment smoke row" >&2
    exit 1
  }
done < <(
  jq -r '.candidates[]
    | select(.advertised and .referenceRunner != null)
    | [.id, .referenceRunner] | @tsv' "$registry"
)

jq -e '
  ([.candidates[] | select(
    .advertised
    and .id == "apple-clangd-21"
    and .resolvedVersionPrefix == "Apple clangd version 21.0.0"
  )] | length == 1)
  and ([.candidates[] | select(.id == "clangd" and (.advertised | not))] | length == 1)
' "$registry" >/dev/null

historical_results="$repo_root/docs/src/content/docs/results/development-2026-07-24.md"
historical_v1_results="$repo_root/docs/src/content/docs/results/evaluation-real-project-v1.md"
current_results="$repo_root/docs/src/content/docs/results/index.md"
grep -q 'Historical identity limitation' "$historical_results"
grep -q 'candidate evidence' "$historical_results"
# The v0.2.0 evaluation keeps its bounded claim on its own historical page.
grep -q 'Evaluation evidence' "$historical_v1_results"
grep -q 'UsageBench v0.2.0' "$historical_v1_results"
# The current result page is rendered from a checksum-verified bundle, so it
# carries a provenance boundary rather than a hand-written claim.
grep -qE 'Immutable evidence|No published result' "$current_results"

development_scope="$(bash "$scope_resolver" development)"
evaluation_v1_scope="$(bash "$scope_resolver" evaluation real-project-v1)"
evaluation_scope="$(bash "$scope_resolver" evaluation real-project-v2)"
legacy_scope="$(bash "$scope_resolver" legacy-promoted)"
development_candidates="$(jq -c '.candidates' <<< "$development_scope")"
[[ "$(jq -r 'length' <<< "$development_candidates")" -eq 11 ]] || {
  echo "development candidate scope must contain all eleven advertised candidates" >&2
  exit 1
}
[[ "$(jq -r '.casePath' <<< "$evaluation_scope")" == "benchmarks/cases/evaluation/real-project-v2" ]] || {
  echo "evaluation v2 case scope does not match the registered release slice" >&2
  exit 1
}
[[ "$(jq -r '.casePath' <<< "$evaluation_v1_scope")" == "benchmarks/cases/evaluation/real-project-v1" ]] || {
  echo "evaluation v1 case scope no longer resolves" >&2
  exit 1
}
[[ "$(jq -c '.candidates' <<< "$evaluation_v1_scope")" == '["bifrost","gopls","pyright","typescript-language-server"]' ]] || {
  echo "evaluation v1 candidate scope no longer matches its protocol" >&2
  exit 1
}
[[ "$(jq -c '.candidates' <<< "$evaluation_scope")" == '["bifrost","eclipse-jdtls","rust-analyzer","apple-clangd-21"]' ]] || {
  echo "evaluation v2 candidate scope does not match Bifrost plus protocol targets" >&2
  exit 1
}
[[ "$(jq -r '.casePath' <<< "$legacy_scope")" == "benchmarks/cases" ]] || {
  echo "legacy-promoted case scope does not use the development case corpus" >&2
  exit 1
}
[[ "$(jq -r '.promotionManifest' <<< "$legacy_scope")" == "benchmarks/promotion/legacy-v1/manifest.json" ]] || {
  echo "legacy-promoted scope is not bound to the v1 promotion manifest" >&2
  exit 1
}
[[ "$(jq -r '[.documents[].cases[] | select(.membership == "balanced_core")] | length' benchmarks/promotion/legacy-v1/manifest.json)" == "110" ]] || {
  echo "legacy promotion manifest does not contain exactly 110 balanced-core cases" >&2
  exit 1
}
[[ "$(jq -r '[.documents[].cases[] | select(.membership != "balanced_core")] | length' benchmarks/promotion/legacy-v1/manifest.json)" == "0" ]] || {
  echo "legacy freeze manifest contains overflow or control cases" >&2
  exit 1
}
grep -Fq 'scripts/stage-legacy-promotion-corpus.py' "$freeze_workflow" || {
  echo "legacy freeze does not create a filtered execution corpus" >&2
  exit 1
}
grep -Fq 'FREEZE_EXECUTION_CORPUS_ROOT' "$repo_root/scripts/run-freeze-candidates.sh" || {
  echo "legacy candidate execution is not separated from source-bound validation" >&2
  exit 1
}
grep -Fq -- '--promotion-manifest' "$repo_root/scripts/run-freeze-candidates.sh" || {
  echo "legacy candidate freeze does not bind the promotion manifest" >&2
  exit 1
}
grep -Fq 'scripts/resolve-freeze-scope.sh evaluation' "$freeze_workflow" || {
  echo "freeze workflow does not consume the shared scope resolver" >&2
  exit 1
}
grep -Fq 'resolved_version_prefix="$(jq -er --arg candidate "$CANDIDATE"' "$freeze_workflow" || {
  echo "freeze workflow does not read the selected candidate version prefix" >&2
  exit 1
}
grep -Fq '[[ "$actual_version" == "$resolved_version_prefix"* ]]' "$freeze_workflow" || {
  echo "freeze workflow does not compare the candidate version prefix literally" >&2
  exit 1
}
grep -Fq "default: 'macos-26'" "$freeze_workflow" || {
  echo "evaluation freeze does not default to the GitHub-hosted macOS 26 runner" >&2
  exit 1
}
grep -Fq '^usagebench-ephemeral-macos-arm64-[0-9a-f]{32}$' "$freeze_workflow" || {
  echo "evaluation freeze does not constrain one-job repository runner labels" >&2
  exit 1
}
[[ "$(grep -c 'shard: bifrost-' "$freeze_workflow")" -eq 3 ]] || {
  echo "evaluation freeze must contain three language-bounded Bifrost shards" >&2
  exit 1
}
for shard in eclipse-jdtls-java rust-analyzer-rust apple-clangd-21-cpp; do
  grep -Fq "shard: $shard" "$freeze_workflow" || {
    echo "evaluation freeze lacks shard $shard" >&2
    exit 1
  }
done
grep -Fq 'permissions:' "$freeze_workflow"
grep -Fq 'python3 scripts/freeze-shards.py aggregate' "$freeze_workflow" || {
  echo "freeze workflow does not verify shard identity and coverage before aggregation" >&2
  exit 1
}
grep -Fq 'benchmarks/evaluation/real-project-v2/candidates-v0.3.0.json' "$freeze_workflow" || {
  echo "v0.3.0 evaluation freeze is not bound to its historical candidate registry" >&2
  exit 1
}
grep -Fq "public_bifrost_source='https://github.com/BrokkAi/bifrost'" "$freeze_workflow" || {
  echo "freeze workflow does not pin Bifrost to the public repository" >&2
  exit 1
}
grep -Fq 'refs/tags/$bifrost_requested_version^{}' "$freeze_workflow" || {
  echo "freeze workflow does not resolve the requested public Bifrost tag" >&2
  exit 1
}
grep -Fq '[[ "$bifrost_tag_revision" == "$bifrost_revision" ]]' "$freeze_workflow" || {
  echo "freeze workflow does not compare the public Bifrost tag to the pinned revision" >&2
  exit 1
}
grep -Fq -- '--manifest "$RUNNER_TEMP/freeze-corpus/evidence/freeze-manifest.json"' \
  "$freeze_workflow" || {
  echo "result generation does not validate against the staged release root" >&2
  exit 1
}
grep -Fq -- '--manifest-path "$runner_corpus_root/Cargo.toml"' \
  "$repo_root/scripts/run-freeze-candidates.sh" || {
  echo "native LSP candidates are not executed from the staged release corpus" >&2
  exit 1
}
grep -Fq -- '--mount "type=bind,src=$corpus_root,dst=/corpus,readonly"' \
  "$repo_root/scripts/run-reference.sh" || {
  echo "reference execution no longer protects the released corpus with a read-only mount" >&2
  exit 1
}
grep -Fq 'BIFROST_CACHE_DIR=/work/bifrost-cache' "$repo_root/scripts/run-reference.sh" || {
  echo "Bifrost reference execution does not relocate generated cache state to writable tmpfs" >&2
  exit 1
}
jq -e '
  .distribution == "checksum_addressed_registry"
  and .registry == "ghcr.io/brokkai/usagebench-reference"
' "$reference_manifest" >/dev/null
grep -Fq 'USAGEBENCH_REFERENCE_IMAGE_FORCE_REBUILD' "$repo_root/scripts/reference-image.sh" || {
  echo "reference image tooling lacks a forced recipe rebuild path" >&2
  exit 1
}
for identity_label in \
  ai.brokk.usagebench.environment.identity-digest \
  ai.brokk.usagebench.environment.definition-digest \
  ai.brokk.usagebench.analyzer.identity \
  ai.brokk.usagebench.canonical-platform; do
  grep -Fq "$identity_label" "$repo_root/scripts/reference-image.sh" || {
    echo "reference image reuse does not verify $identity_label" >&2
    exit 1
  }
done
grep -Fq 'docker login ghcr.io --username "$GITHUB_ACTOR" --password-stdin' "$workflow" || {
  echo "trusted reference workflow does not authenticate checksum-addressed publication" >&2
  exit 1
}
grep -Fq 'unset USAGEBENCH_REFERENCE_IMAGE_FORCE_REBUILD' "$reference_ci" || {
  echo "reference smoke does not restore the ordinary reuse path after forced construction" >&2
  exit 1
}
grep -Fq 'unset USAGEBENCH_REFERENCE_IMAGE_PUBLISH' "$reference_ci" || {
  echo "reference smoke leaks trusted publication into reproduction" >&2
  exit 1
}
grep -Fq 'scripts/corpus-hashes.py verify' "$freeze_workflow" || {
  echo "freeze workflow does not verify the exact staged corpus" >&2
  exit 1
}

while IFS=$'\t' read -r candidate_id profile expected_sha256; do
  actual_sha256="$(shasum -a 256 "$repo_root/$profile" | awk '{print $1}')"
  [[ "$actual_sha256" == "$expected_sha256" ]] || {
    echo "advertised LSP profile checksum drift for $candidate_id" >&2
    exit 1
  }
done < <(
  jq -r '.candidates[]
    | select(.advertised and .runner == "lsp")
    | [.id, .profile, .profileSha256] | @tsv' "$registry"
)
