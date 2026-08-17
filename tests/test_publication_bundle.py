import importlib.util
import json
import pathlib
import tempfile
import unittest


SCRIPT = pathlib.Path(__file__).parents[1] / "scripts/validate-publication-bundle.py"
SPEC = importlib.util.spec_from_file_location("validate_publication_bundle", SCRIPT)
VALIDATOR = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(VALIDATOR)


REVISION = "0123456789abcdef0123456789abcdef01234567"
TAG = "v0.4.0"


def manifest(kind="evaluation"):
    return {
        "schemaVersion": 5,
        "snapshotKind": kind,
        "version": TAG,
        "revision": REVISION,
        "candidates": [{"id": "bifrost"}],
        "reports": [{"candidateId": "bifrost", "sha256": "a" * 64}],
        "corpus": [{"partition": "evaluation" if kind == "evaluation" else "development"}],
        "evaluationAudit": {} if kind == "evaluation" else None,
        "legacyPromotionAudit": None,
    }


class PublicationBundleTests(unittest.TestCase):
    def test_snapshot_partition_rejects_mixed_provenance(self):
        with self.assertRaisesRegex(
            VALIDATOR.ValidationError, "only prospective evaluation audit"
        ):
            value = manifest("evaluation")
            value["legacyPromotionAudit"] = {}
            with tempfile.TemporaryDirectory() as directory:
                root = pathlib.Path(directory)
                (root / "evidence").mkdir()
                (root / "evidence/freeze-manifest.json").write_text(json.dumps(value))
                VALIDATOR.validate_freeze_partition(root, TAG, REVISION)

    def test_snapshot_partition_rejects_evaluation_documents_in_development(self):
        value = manifest("development")
        value["corpus"][0]["partition"] = "evaluation"
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            (root / "evidence").mkdir()
            (root / "evidence/freeze-manifest.json").write_text(
                json.dumps(value)
            )
            with self.assertRaisesRegex(VALIDATOR.ValidationError, "mixes corpus partitions"):
                VALIDATOR.validate_freeze_partition(root, TAG, REVISION)

    def test_generated_page_binds_manifest_and_report_digests(self):
        value = manifest()
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            manifest_path = root / "freeze-manifest.json"
            manifest_path.write_text(json.dumps(value))
            digest = VALIDATOR._sha256(manifest_path)
            page = root / "results.md"
            page.write_text(
                "\n".join(
                    [
                        "<!-- GENERATED FILE. DO NOT EDIT.",
                        "Snapshot: evaluation v0.4.0",
                        f"Revision: {REVISION}",
                        f"Manifest SHA-256: {digest}",
                        "Generator: usagebench generate-results v0.4.0",
                        "Input reports:",
                        f"- bifrost: {'a' * 64}",
                        "-->",
                        "",
                        "# Generated result",
                    ]
                )
            )
            VALIDATOR.validate_generated_page(page, value, digest)
            page.write_text(page.read_text().replace("a" * 64, "b" * 64))
            with self.assertRaisesRegex(VALIDATOR.ValidationError, "report provenance"):
                VALIDATOR.validate_generated_page(page, value, digest)


if __name__ == "__main__":
    unittest.main()
