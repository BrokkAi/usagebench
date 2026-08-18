import importlib.util
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).parents[1] / "scripts/generate-docs-evidence.py"
SPEC = importlib.util.spec_from_file_location("generate_docs_evidence", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class GeneratedScoreTests(unittest.TestCase):
    def test_transports_scores_from_generated_result_page(self):
        page = """<!-- GENERATED FILE. DO NOT EDIT. -->
## Required-destination comparison

| Reference profile | Shared | Bifrost found | Reference found |
|---|---:|---:|---:|
| gopls 0.23.0 | 12 | 8/12 (66.7%) | 9/12 (75.0%) |
| Pyright 1.1.411 | 12 | 8/12 (66.7%) | 11/12 (91.7%) |

## Strict contract conformance

| Reference profile | Shared | Both exact | Bifrost only | Reference only | Neither |
|---|---:|---:|---:|---:|---:|
| gopls 0.23.0 | 12 | 7 | 1 | 2 | 2 |
| Pyright 1.1.411 | 12 | 6 | 1 | 4 | 1 |
"""
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "results.md"
            path.write_text(page, encoding="utf-8")
            scores = MODULE._generated_scores(path)

        self.assertEqual(scores["strict"]["denominator"], 24)
        self.assertEqual(scores["strict"]["bifrostExact"], 15)
        self.assertEqual(scores["strict"]["referenceExact"], 19)
        self.assertEqual(scores["required"]["bifrostFound"], 16)
        self.assertEqual(scores["required"]["referenceFound"], 20)

    def test_rejects_pages_without_both_score_sections(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "results.md"
            path.write_text("# not a generated result\n", encoding="utf-8")
            with self.assertRaises(ValueError):
                MODULE._generated_scores(path)


if __name__ == "__main__":
    unittest.main()
