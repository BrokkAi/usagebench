"""Cover the bundle-backed docs render path.

These assertions exist because the path they cover had no test and shipped a
bug that only appeared when the publication workflow ran for real: the page
guard required the body to start with a heading, and every generated page
starts with the result generator's provenance comment instead.

The rendering helpers are pure and take the page body from disk, so they can be
driven with a realistic generated page without a checksum-valid bundle. Bundle
validation belongs to the publish itself and is deliberately not re-tested here.
"""

import importlib.util
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).parents[1] / "scripts/generate-docs-results.py"
SPEC = importlib.util.spec_from_file_location("generate_docs_results", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)

RELEASE = "v0.3.0"
REVISION = "bfebd62c2df6bae136cb3e48d3f2dd96703fc8ab"
BLOB = f"https://github.com/BrokkAi/usagebench/blob/{RELEASE}"

# Shaped after what `usagebench generate-results` actually writes: the
# provenance comment first, then an H1, tables, and links that are relative to
# the bundle root rather than to the documentation site.
GENERATED_RESULTS = """<!-- GENERATED FILE. DO NOT EDIT.
Snapshot: evaluation v0.3.0
Revision: bfebd62c2df6bae136cb3e48d3f2dd96703fc8ab
Generator: usagebench generate-results v0.3.1
Input reports:
- bifrost: 665bcc23
-->

# Evaluation-only results

> **Partition:** `evaluation` only.

## Per-profile denominators

| Language | Registered adapter |
|---|---|
| java | [`adapters/lsp/eclipse-jdtls.json`](../adapters/lsp/eclipse-jdtls.json) |

Manifest: [`evidence/freeze-manifest.json`](../evidence/freeze-manifest.json) (`2f930edb`).
"""

# The case comparison carries no H1; it opens at a second-level heading.
GENERATED_CASE_COMPARISON = """<!-- GENERATED FILE. DO NOT EDIT.
Snapshot: evaluation v0.3.0
-->

## Separating strict-contract cases

| Reference profile | Case |
|---|---|
| `rust-analyzer` | [`benchmarks/cases/rust.yaml`](../benchmarks/cases/rust.yaml) |
"""


def bundle(results=GENERATED_RESULTS, case_comparison=GENERATED_CASE_COMPARISON):
    """Write a bundle-shaped results directory and return its summary."""

    directory = tempfile.TemporaryDirectory()
    root = Path(directory.name)
    (root / "results").mkdir()
    (root / "results/results.md").write_text(results, encoding="utf-8")
    (root / "results/case-comparison.md").write_text(case_comparison, encoding="utf-8")
    summary = {"release": RELEASE, "revision": REVISION, "bundlePath": root}
    return directory, summary


def transported(page):
    """Return only the content carried over from the bundle.

    The provenance note this script prepends contains a deliberate `../`
    sibling link on deeper pages, so a whole-page scan for bundle-relative
    links would flag it. Only the transported body should be link-free.
    """

    return page.split(MODULE.GENERATED_MARKER, 1)[1]


class PublishedPageTests(unittest.TestCase):
    def render(self, source, results=GENERATED_RESULTS):
        directory, summary = bundle(results=results)
        self.addCleanup(directory.cleanup)
        return MODULE._published_page(summary, source, MODULE.PAGES[source])

    def test_frontmatter_leads_the_page(self):
        page = self.render("results.md")
        self.assertTrue(page.startswith('---\ntitle: "Current evaluation result"\n'))
        self.assertIn(f'description: "Immutable {RELEASE} results', page)

    def test_generated_heading_is_demoted_so_the_page_has_one_title(self):
        # Starlight renders the frontmatter title as the H1. Carrying the
        # generated H1 through would give the document two.
        page = self.render("results.md")
        self.assertNotIn("\n# ", page)
        self.assertIn("\n## Evaluation-only results\n", page)

    def test_generator_provenance_comment_is_preserved(self):
        page = self.render("results.md")
        self.assertIn(MODULE.GENERATED_MARKER, page)
        self.assertIn("Generator: usagebench generate-results", page)

    def test_bundle_relative_links_become_release_absolute(self):
        page = self.render("results.md")
        self.assertIn(f"]({BLOB}/adapters/lsp/eclipse-jdtls.json)", page)
        self.assertIn(f"]({BLOB}/evidence/freeze-manifest.json)", page)
        self.assertNotIn("](../", transported(page))

    def test_provenance_note_names_the_release_and_revision(self):
        page = self.render("results.md")
        self.assertIn(f"/releases/tag/{RELEASE})", page)
        self.assertIn(f"/tree/{REVISION})", page)

    def test_a_page_without_the_generator_preamble_is_rejected(self):
        # The bug this suite exists for: the guard previously required a
        # leading heading, which no generated page has.
        with self.assertRaises(ValueError) as raised:
            self.render("results.md", results="# Hand-written results\n")
        self.assertIn("provenance comment", str(raised.exception))

    def test_case_comparison_renders_without_a_top_level_heading(self):
        page = self.render("case-comparison.md")
        self.assertTrue(page.startswith('---\ntitle: "Evaluation case comparison"\n'))
        self.assertIn("\n## Separating strict-contract cases\n", page)
        self.assertNotIn("](../", transported(page))


class SiblingLinkDepthTests(unittest.TestCase):
    """Pages render at different depths, so sibling links differ."""

    def setUp(self):
        directory, self.summary = bundle()
        self.addCleanup(directory.cleanup)

    def test_index_links_to_siblings_without_climbing(self):
        # results.md renders at /results/, where evidence/ is one segment away.
        page = MODULE._published_page(self.summary, "results.md", MODULE.PAGES["results.md"])
        self.assertIn("[evidence map](evidence/)", page)

    def test_case_comparison_climbs_to_reach_siblings(self):
        # case-comparison.md renders at /results/case-comparison/, so a bare
        # evidence/ would resolve under it and 404.
        spec = MODULE.PAGES["case-comparison.md"]
        page = MODULE._published_page(self.summary, "case-comparison.md", spec)
        self.assertIn("[evidence map](../evidence/)", page)


class PendingPageTests(unittest.TestCase):
    def test_pending_page_shows_no_score_and_says_why(self):
        page = MODULE._pending_page(MODULE.PAGES["results.md"])
        self.assertTrue(page.startswith('---\ntitle: "Current evaluation result"\n'))
        self.assertIn("No published result", page)
        self.assertIn("[evidence map](evidence/)", page)

    def test_pending_case_comparison_climbs_to_reach_siblings(self):
        page = MODULE._pending_page(MODULE.PAGES["case-comparison.md"])
        self.assertIn("[evidence map](../evidence/)", page)


class LinkRewriteTests(unittest.TestCase):
    def test_only_bundle_relative_links_are_rewritten(self):
        body = (
            "[a](../adapters/x.json) [b](./sibling/) [c](https://example.com/y) "
            "[d](#anchor) [e](../evidence/z.json)"
        )
        rewritten = MODULE._absolute_links(body, RELEASE)
        self.assertIn(f"[a]({BLOB}/adapters/x.json)", rewritten)
        self.assertIn(f"[e]({BLOB}/evidence/z.json)", rewritten)
        self.assertIn("[b](./sibling/)", rewritten)
        self.assertIn("[c](https://example.com/y)", rewritten)
        self.assertIn("[d](#anchor)", rewritten)


class FrontmatterTests(unittest.TestCase):
    def test_quotes_are_refused_rather_than_emitted_unescaped(self):
        with self.assertRaises(ValueError):
            MODULE._frontmatter('a "quoted" title', "description")
        with self.assertRaises(ValueError):
            MODULE._frontmatter("title", 'a "quoted" description')


class SliceAcceptanceTests(unittest.TestCase):
    def test_no_bundle_yields_the_pending_placeholder(self):
        self.assertIsNone(MODULE.evaluation_summary(None))


if __name__ == "__main__":
    unittest.main()
