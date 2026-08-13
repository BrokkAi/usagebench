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
    def test_plan_is_exactly_six_candidate_language_pairs(self):
        self.assertEqual(len(MODULE.SHARDS), 6)
        self.assertEqual(set(MODULE.SHARDS.values()), {
            ("bifrost", "java"), ("bifrost", "rust"), ("bifrost", "cpp"),
            ("eclipse-jdtls", "java"), ("rust-analyzer", "rust"),
            ("apple-clangd-21", "cpp"),
        })

    def test_merge_rejects_invariant_mismatch(self):
        left = {"runner": {"name": "bifrost"}, "environment": {"analyzerExecutable": {}}}
        right = {"runner": {"name": "other"}, "environment": {"analyzerExecutable": {}}}
        with self.assertRaisesRegex(ValueError, "runner"):
            MODULE.merge_bifrost([left, right])

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
