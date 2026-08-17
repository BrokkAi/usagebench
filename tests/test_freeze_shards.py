import importlib.util
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

    def test_v030_registry_preserves_historical_bifrost_identity(self):
        active = json.loads((ROOT / "adapters/candidates.json").read_text())
        frozen = json.loads(
            (ROOT / "benchmarks/evaluation/real-project-v2/candidates-v0.3.0.json").read_text()
        )
        active_bifrost = next(item for item in active["candidates"] if item["id"] == "bifrost")
        frozen_bifrost = next(item for item in frozen["candidates"] if item["id"] == "bifrost")
        self.assertEqual(
            active_bifrost["requestedVersion"], "v0.10.1"
        )
        self.assertEqual(
            active_bifrost["revision"], "511adaa2733067bb1b7809ab79e06ec0e3d2a146"
        )
        self.assertEqual(frozen_bifrost["requestedVersion"], "v0.10.1")
        self.assertEqual(frozen_bifrost["revision"], "511adaa2733067bb1b7809ab79e06ec0e3d2a146")

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
