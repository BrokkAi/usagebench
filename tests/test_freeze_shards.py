import importlib.util
import hashlib
import json
import pathlib
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("freeze_shards", ROOT / "scripts/freeze-shards.py")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class FreezeShardTests(unittest.TestCase):
    def test_staged_corpus_hashes_verify_exact_file_set_and_content(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            for name in ("adapters", "benchmarks", "containers", "fixtures", "schema", "scripts", "src"):
                (root / name).mkdir()
                (root / name / "input.txt").write_text(name)
            for name in (
                ".dockerignore", "ARTIFACT.md", "CITATION.cff", "Cargo.lock", "Cargo.toml",
                "LICENSE.md", "README.md", "RELEASES.md",
            ):
                (root / name).write_text(name)
            (root / ".usagebench-release.json").write_text(json.dumps({
                "releaseTag": "v0.3.0", "revision": "a" * 40,
            }))
            manifest = root / ".usagebench-corpus-hashes.json"
            timings = root / ".usagebench-stage-timings.json"
            command = [
                ROOT / "scripts/corpus-hashes.py", "create", "--root", root,
                "--output", manifest, "--timings-output", timings,
                "--release-staging-ms", "7",
            ]
            created = subprocess.run(command, text=True, capture_output=True)
            self.assertEqual(created.returncode, 0, created.stderr)
            verified = subprocess.run([
                ROOT / "scripts/corpus-hashes.py", "verify", "--root", root,
                "--manifest", manifest,
            ], text=True, capture_output=True)
            self.assertEqual(verified.returncode, 0, verified.stderr)
            (root / "benchmarks/input.txt").write_text("changed")
            rejected = subprocess.run([
                ROOT / "scripts/corpus-hashes.py", "verify", "--root", root,
                "--manifest", manifest,
            ], text=True, capture_output=True)
            self.assertNotEqual(rejected.returncode, 0)
            self.assertIn("checksum mismatch", rejected.stderr)

    def test_v030_registry_uses_public_native_bifrost_identity(self):
        active = json.loads((ROOT / "adapters/candidates.json").read_text())
        frozen = json.loads(
            (ROOT / "benchmarks/evaluation/real-project-v2/candidates-v0.3.0.json").read_text()
        )
        active_bifrost = next(item for item in active["candidates"] if item["id"] == "bifrost")
        frozen_bifrost = next(item for item in frozen["candidates"] if item["id"] == "bifrost")
        self.assertEqual(
            active_bifrost["requestedVersion"], "v0.10.6"
        )
        self.assertEqual(
            active_bifrost["revision"], "6624e883dc8b8268dd4c454c06f8eebea173308e"
        )
        self.assertEqual(frozen_bifrost["requestedVersion"], "v0.10.2")
        self.assertEqual(
            frozen_bifrost["source"], "https://github.com/BrokkAi/bifrost"
        )
        self.assertEqual(
            frozen_bifrost["revision"], "d1a7c0cc1cf58d0c0789476ad42a92318bb8da49"
        )
        self.assertNotIn("referenceRunner", frozen_bifrost)

    def test_apple_clangd_registries_use_exact_reported_banner_prefix(self):
        active = json.loads((ROOT / "adapters/candidates.json").read_text())
        frozen = json.loads(
            (ROOT / "benchmarks/evaluation/real-project-v2/candidates-v0.3.0.json").read_text()
        )
        for registry in (active, frozen):
            apple_clangd = next(
                item for item in registry["candidates"] if item["id"] == "apple-clangd-21"
            )
            self.assertEqual(
                apple_clangd["resolvedVersionPrefix"], "Apple clangd version 21.0.0"
            )

    def test_freeze_setup_reads_apple_clangd_prefix_from_selected_registry(self):
        workflow = (ROOT / ".github/workflows/freeze.yml").read_text()
        self.assertIn(
            'resolved_version_prefix="$(jq -er --arg candidate "$CANDIDATE"', workflow
        )
        self.assertIn('[[ "$actual_version" == "$resolved_version_prefix"* ]]', workflow)
        self.assertNotIn("grep -F 'Apple clangd version 21.0.0'", workflow)

    def test_freeze_setup_verifies_public_bifrost_tag_identity(self):
        workflow = (ROOT / ".github/workflows/freeze.yml").read_text()
        self.assertIn("public_bifrost_source='https://github.com/BrokkAi/bifrost'", workflow)
        self.assertIn('refs/tags/$bifrost_requested_version^{}', workflow)
        self.assertIn('[[ "$bifrost_tag_revision" == "$bifrost_revision" ]]', workflow)

    def test_legacy_native_shards_publish_semantic_misses_but_reject_runner_errors(self):
        workflow = (ROOT / ".github/workflows/freeze.yml").read_text()
        self.assertIn(
            '.requestedTotals.authoredCases == $expected_cases and .totals.errors == 0',
            workflow,
        )
        self.assertNotIn('.totals.cases == $expected_cases', workflow)

    def test_roslyn_native_shard_provisions_the_runtime_its_build_requires(self):
        workflow = (ROOT / ".github/workflows/freeze.yml").read_text()
        # The framework comes from the server's own runtimeconfig. An earlier
        # revision asserted net8.0 from a comment and pinned the .NET 8 keg,
        # which passed its own gate and then failed at launch because
        # vscode-csharp 2.140.9 targets net10.0 with rollForward: Major.
        self.assertIn(
            'required_framework="$(jq -er \'.runtimeOptions.framework.version\' '
            '"$roslyn_runtimeconfig")"',
            workflow,
        )
        self.assertIn('required_major="${required_framework%%.*}"', workflow)
        self.assertIn('dotnet_root="$(brew --prefix "$dotnet_formula")/libexec"', workflow)
        self.assertIn('printf \'DOTNET_ROOT=%s\\n\' "$dotnet_root" >> "$GITHUB_ENV"', workflow)
        # The gate reads the runtime list rather than `dotnet --version`, which
        # reports the SDK and moves with roll-forward.
        self.assertIn(
            '"$dotnet_bin" --list-runtimes | grep -q '
            '"^Microsoft.NETCore.App ${required_major}\\."',
            workflow,
        )
        self.assertIn('[[ "$resolved_dotnet" == "$dotnet_bin" ]]', workflow)
        self.assertNotIn('[[ "$dotnet_version" == 8.* ]]', workflow)
        # No restated framework version may survive in the Roslyn provisioning.
        self.assertNotIn('brew --prefix dotnet@8', workflow)
        self.assertNotIn('^Microsoft.NETCore.App 8\\.', workflow)

    def test_native_toolchain_probe_checks_the_roslyn_dotnet_contract(self):
        probe = (ROOT / ".github/workflows/native-toolchain-probe.yml").read_text()
        self.assertIn("runs-on: macos-26", probe)
        # The probe exists to answer the questions the freeze log could not:
        # whether the keg provides a muxer, and whether the prepend wins.
        self.assertIn('prefix="$(brew --prefix "$dotnet_formula" 2>&1)"', probe)
        self.assertIn("command -v dotnet before prepend", probe)
        self.assertIn("command -v dotnet after prepend", probe)
        # The probe must derive the framework the same way the freeze does, or
        # it rehearses a contract the freeze no longer depends on.
        self.assertIn(
            '"$dotnet_bin" --list-runtimes | grep -q '
            '"^Microsoft.NETCore.App ${required_major}\\."',
            probe,
        )
        self.assertIn('[[ "$resolved_dotnet" == "$dotnet_bin" ]]', probe)
        self.assertNotIn('brew --prefix dotnet@8', probe)

    def test_ruby_lsp_native_shard_provisions_ruby_three_four_keg(self):
        workflow = (ROOT / ".github/workflows/freeze.yml").read_text()
        self.assertIn('brew list --formula ruby@3.4 >/dev/null 2>&1 || brew install ruby@3.4', workflow)
        self.assertIn('ruby_prefix="$(brew --prefix ruby@3.4)"', workflow)
        self.assertIn('ruby_gem_bin="$(ruby -rrubygems -e \'print Gem.bindir\')"', workflow)
        self.assertIn('[[ "$ruby_version" =~ ^3\\.4\\.[0-9]+$ ]]', workflow)
        self.assertNotIn('brew list ruby >/dev/null', workflow)
        self.assertNotIn('ruby\\ 3\\.[4-9]', workflow)

    def test_release_bundle_stages_every_artifact_a_promotion_manifest_binds(self):
        """A bundle must contain what its manifests hash-bind.

        The v0.3.1 freeze reached aggregation and failed on `canonicalize
        promotion artifact docs/legacy-promotion-selection-policy.md`: the
        legacy manifest binds an eligibility policy under docs/, and the
        staging step copies benchmarks, fixtures, adapters, schema, src,
        containers, and scripts but not docs.
        """

        staging = (ROOT / "scripts/stage-release-bundle.sh").read_text()
        # Derived from the manifests rather than a hardcoded docs/ path, so a
        # newly bound artifact is staged without editing this script.
        self.assertIn(
            "jq -r '.. | objects | select(has(\"file\")) | .file' \"$promotion_manifest\"",
            staging,
        )
        self.assertIn('cp -- "$source_root/$bound_artifact" "$destination/$bound_artifact"', staging)
        # All of docs/ would drag in node_modules and build output.
        self.assertNotIn('"$source_root/docs" \\', staging)

        promotion_root = ROOT / "benchmarks/promotion"
        staged_prefixes = (
            "benchmarks/", "fixtures/", "adapters/", "schema/", "src/",
            "containers/", "scripts/",
        )
        bound = set()

        def walk(node):
            if isinstance(node, dict):
                value = node.get("file")
                if isinstance(value, str):
                    bound.add(value)
                for child in node.values():
                    walk(child)
            elif isinstance(node, list):
                for child in node:
                    walk(child)

        for manifest in sorted(promotion_root.rglob("*.json")):
            walk(json.loads(manifest.read_text()))
        outside = sorted(p for p in bound if not p.startswith(staged_prefixes))
        # Every such artifact must exist, or the freeze fails at aggregation.
        for relative in outside:
            self.assertTrue((ROOT / relative).is_file(), f"bound artifact missing: {relative}")
        self.assertIn("docs/legacy-promotion-selection-policy.md", outside)

    def test_legacy_scope_is_bound_to_the_110_case_manifest(self):
        scope = subprocess.run(
            [ROOT / "scripts/resolve-freeze-scope.sh", "legacy-promoted"],
            text=True,
            capture_output=True,
        )
        self.assertEqual(scope.returncode, 0, scope.stderr)
        value = json.loads(scope.stdout)
        self.assertEqual(value["casePath"], "benchmarks/cases")
        self.assertEqual(
            value["promotionManifest"],
            "benchmarks/promotion/legacy-v2/manifest.json",
        )
        self.assertEqual(
            value["candidates"],
            [
                item["id"]
                for item in json.loads((ROOT / "adapters/candidates.json").read_text())["candidates"]
                if item["advertised"]
            ],
        )

    def test_legacy_execution_staging_excludes_unselected_documents_and_cases(self):
        with tempfile.TemporaryDirectory() as directory:
            source = pathlib.Path(directory) / "source"
            destination = pathlib.Path(directory) / "execution"
            manifest_path = source / "benchmarks/promotion/legacy-v2/manifest.json"
            manifest_path.parent.mkdir(parents=True)
            case_root = source / "benchmarks/cases"
            case_root.mkdir(parents=True)
            documents = []
            expected_ids = []
            cursor = 0
            for index in range(30):
                case_file = f"benchmarks/cases/legacy-{index:02d}.yaml"
                count = 4 if index < 20 else 3
                ids = [f"legacy-case-{number:03d}" for number in range(cursor, cursor + count)]
                cursor += count
                expected_ids.extend(ids)
                source_path = source / case_file
                source_path.write_text(
                    "schemaVersion: 2\ncorpus:\n  partition: development\n  selection: analyzer_informed\n"
                    "groundTruth:\n  status: legacy_unattributed\n  reviewers: []\n"
                    "referencePolicy: bindings_optional\npositionEncoding: utf-16\n"
                    "source:\n  kind: fixture\n  path: fixtures/test\nlanguage: rust\ncases:\n"
                    + "".join(f"  - id: {case_id}\n    declaration: {{}}\n" for case_id in ids)
                    + "  - id: overflow-case\n    declaration: {}\n"
                )
                documents.append(
                    {
                        "caseFile": case_file,
                        "sourceSha256": hashlib.sha256(source_path.read_bytes()).hexdigest(),
                        "language": "rust",
                        "cases": [
                            {"id": case_id, "membership": "balanced_core"} for case_id in ids
                        ],
                    }
                )
            self.assertEqual(len(expected_ids), 110)
            manifest_path.write_text(
                json.dumps(
                    {
                        "promotionId": "test",
                        "documents": documents,
                    }
                )
            )
            result = subprocess.run(
                [
                    ROOT / "scripts/stage-legacy-promotion-corpus.py",
                    "--source-root",
                    source,
                    "--destination",
                    destination,
                    "--promotion-manifest",
                    manifest_path,
                ],
                text=True,
                capture_output=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            staged_files = sorted((destination / "benchmarks/cases").glob("*.yaml"))
            self.assertEqual(len(staged_files), 30)
            staged_ids = [
                line.removeprefix("  - id: ")
                for path in staged_files
                for line in path.read_text().splitlines()
                if line.startswith("  - id: ")
            ]
            self.assertEqual(staged_ids, expected_ids)

    def test_legacy_execution_staging_retires_the_annotations_the_manifest_names(self):
        annotated = (
            "  - id: legacy-case-000\n"
            "    expectedFailure:\n"
            "      reason: stale expectation\n"
            "    declaration: {}\n"
            "  - id: legacy-case-001\n"
            "    expectedFailure:\n"
            "      reason: still failing\n"
            "    declaration: {}\n"
        )
        staged = self._stage_two_case_document(
            annotated, retired={"legacy-case-000"}
        )
        # The retired annotation is gone; the one the manifest does not name
        # survives, along with every other key on the retired case.
        self.assertNotIn("stale expectation", staged)
        self.assertIn("still failing", staged)
        self.assertEqual(staged.count("expectedFailure:"), 1)
        self.assertEqual(staged.count("    declaration: {}"), 2)
        self.assertIn("  - id: legacy-case-000\n    declaration: {}", staged)

    def test_legacy_execution_staging_refuses_to_retire_an_absent_annotation(self):
        with self.assertRaises(AssertionError):
            self._stage_two_case_document(
                "  - id: legacy-case-000\n    declaration: {}\n"
                "  - id: legacy-case-001\n    declaration: {}\n",
                retired={"legacy-case-000"},
            )

    def _stage_two_case_document(self, cases: str, retired: set) -> str:
        """Stage a one-document, two-case manifest and return the staged YAML."""

        with tempfile.TemporaryDirectory() as directory:
            source = pathlib.Path(directory) / "source"
            destination = pathlib.Path(directory) / "execution"
            manifest_path = source / "benchmarks/promotion/legacy-v2/manifest.json"
            manifest_path.parent.mkdir(parents=True)
            case_root = source / "benchmarks/cases"
            case_root.mkdir(parents=True)
            documents = []
            cursor = 0
            for index in range(30):
                case_file = f"benchmarks/cases/legacy-{index:02d}.yaml"
                source_path = source / case_file
                if index == 0:
                    ids = ["legacy-case-000", "legacy-case-001"]
                    body = cases
                    cursor = 2
                else:
                    count = 4 if index < 22 else 3
                    ids = [f"legacy-case-{number:03d}" for number in range(cursor, cursor + count)]
                    cursor += count
                    body = "".join(
                        f"  - id: {case_id}\n    declaration: {{}}\n" for case_id in ids
                    )
                source_path.write_text(
                    "schemaVersion: 2\ncorpus:\n  partition: development\n  selection: analyzer_informed\n"
                    "groundTruth:\n  status: legacy_unattributed\n  reviewers: []\n"
                    "referencePolicy: bindings_optional\npositionEncoding: utf-16\n"
                    "source:\n  kind: fixture\n  path: fixtures/test\nlanguage: rust\ncases:\n"
                    + body
                )
                documents.append(
                    {
                        "caseFile": case_file,
                        "sourceSha256": hashlib.sha256(source_path.read_bytes()).hexdigest(),
                        "language": "rust",
                        "cases": [
                            dict(
                                {"id": case_id, "membership": "balanced_core"},
                                **(
                                    {"retiredExpectedFailure": {"supersededReason": "x"}}
                                    if case_id in retired
                                    else {}
                                ),
                            )
                            for case_id in ids
                        ],
                    }
                )
            self.assertEqual(cursor, 110)
            manifest_path.write_text(json.dumps({"promotionId": "test", "documents": documents}))
            result = subprocess.run(
                [
                    ROOT / "scripts/stage-legacy-promotion-corpus.py",
                    "--source-root",
                    source,
                    "--destination",
                    destination,
                    "--promotion-manifest",
                    manifest_path,
                ],
                text=True,
                capture_output=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            return (destination / "benchmarks/cases/legacy-00.yaml").read_text()

    def test_bifrost_reference_cache_stays_off_read_only_corpus(self):
        runner = (ROOT / "scripts/run-reference.sh").read_text()
        self.assertIn('--mount "type=bind,src=$corpus_root,dst=/corpus,readonly"', runner)
        self.assertIn('docker_args+=(--env "BIFROST_CACHE_DIR=/work/bifrost-cache")', runner)
        self.assertIn('docker "${docker_args[@]}" "$loaded_image_id" "${command_args[@]}"', runner)

    def test_plan_is_exactly_six_candidate_language_pairs(self):
        self.assertEqual(len(MODULE.SHARDS), 6)
        self.assertEqual(set(MODULE.SHARDS.values()), {
            ("bifrost", "java"), ("bifrost", "rust"), ("bifrost", "cpp"),
            ("eclipse-jdtls", "java"), ("rust-analyzer", "rust"),
            ("apple-clangd-21", "cpp"),
        })

    def test_shard_identity_reuses_checksum_bound_staged_hashes(self):
        paths = MODULE.FROZEN_FILES + MODULE.expected_files(ROOT, "java")
        hashes = {path: f"{index:064x}" for index, path in enumerate(paths, 1)}
        corpus = ({"rootDigest": "sha256:" + "f" * 64}, hashes)
        identity = MODULE.identity(ROOT, "v0.3.0", "a" * 40, "bifrost-java", corpus)
        self.assertEqual(identity["stagedCorpusSha256"], "sha256:" + "f" * 64)
        self.assertEqual(identity["frozenInputSha256"], hashes)

    def test_merge_rejects_invariant_mismatch(self):
        left = {"runner": {"name": "bifrost"}, "environment": {"analyzerExecutable": {}}}
        right = {"runner": {"name": "other"}, "environment": {"analyzerExecutable": {}}}
        with self.assertRaisesRegex(ValueError, "runner"):
            MODULE.merge_bifrost([left, right])

    def test_merge_accepts_reports_omitting_empty_semantic_pack_runs(self):
        def report(case_file, semantic_pack_runs=None):
            result = {
                "usagebenchVersion": "0.3.0",
                "usagebenchRevision": "a" * 40,
                "usagebenchRelease": "v0.3.0",
                "runner": {"name": "bifrost"},
                "invocation": {},
                "bifrostCommit": "b" * 40,
                "bifrostResolvedCommit": "b" * 40,
                "environment": {"analyzerExecutable": {}},
                "requestedCaseFiles": [case_file],
                "caseFiles": [case_file],
                "documents": [{"caseFile": case_file}],
                "requestedTotals": {"cases": 1},
                "totals": {"cases": 1},
                "startedAtUnixSeconds": 2,
                "finishedAtUnixSeconds": 3,
            }
            if semantic_pack_runs is not None:
                result["semanticPackRuns"] = semantic_pack_runs
            return result

        merged = MODULE.merge_bifrost([
            report("java-01.yaml"),
            report("rust-01.yaml", [{"caseFile": "rust-01.yaml"}]),
        ])

        self.assertEqual(merged["semanticPackRuns"], [{"caseFile": "rust-01.yaml"}])
        self.assertEqual(merged["requestedCaseFiles"], ["java-01.yaml", "rust-01.yaml"])
        self.assertEqual(merged["caseFiles"], ["java-01.yaml", "rust-01.yaml"])
        self.assertEqual(merged["totals"], {"cases": 2})

    def test_aggregate_rejects_partial_artifact_set(self):
        with tempfile.TemporaryDirectory() as artifacts, tempfile.TemporaryDirectory() as output_parent:
            args = type("Args", (), {
                "root": str(ROOT), "artifacts": artifacts,
                "output": str(pathlib.Path(output_parent) / "reports"),
                "version": "v0.3.0", "revision": "a" * 40,
            })()
            with self.assertRaisesRegex(ValueError, "shard set mismatch"):
                MODULE.aggregate(args)

    def test_launcher_rejects_broad_label_and_is_bounded(self):
        launcher = ROOT / "scripts/usagebench-ephemeral-runner"
        rejected = subprocess.run([launcher, "batch", "6", "self-hosted"], text=True, capture_output=True)
        self.assertEqual(rejected.returncode, 2)
        label = "usagebench-ephemeral-macos-arm64-" + "a" * 32
        planned = subprocess.run(
            [launcher, "batch", "6", label],
            env={"USAGEBENCH_RUNNER_TEST_MODE": "1"}, text=True, capture_output=True,
        )
        self.assertEqual(planned.returncode, 0, planned.stderr)
        self.assertEqual(planned.stdout.count("would register ephemeral runner"), 6)


if __name__ == "__main__":
    unittest.main()
