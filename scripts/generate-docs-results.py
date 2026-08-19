#!/usr/bin/env python3
"""Render the published result pages from a checksum-verified publication bundle.

The Rust result generator is the score authority. This script does not run an
analyzer, re-score a report, or compute a total. It transports the already
generated result pages out of a validated immutable bundle and into the docs
content collection, adding only the frontmatter Starlight requires and the
release-absolute form of the links those pages already carry.

Without a bundle it writes a readiness placeholder instead, so the checked-in
tree never claims a score that no verified bundle supports.
"""

from __future__ import annotations

import argparse
import importlib.util
import re
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
RESULTS_DIR = ROOT / "docs/src/content/docs/results"
REPOSITORY = "https://github.com/BrokkAi/usagebench"

# The generated pages link bundle-relative to files that also exist in the
# tagged tree, as [`path`](../path). On the site those must resolve to the
# immutable release rather than to a sibling docs page.
BUNDLE_RELATIVE_LINK = re.compile(r"\]\(\.\./([^)]+)\)")

# Every generated page opens with the Rust generator's provenance comment.
# Requiring it proves the content came from that generator rather than from
# anything a publication step might have substituted.
GENERATED_MARKER = "<!-- GENERATED FILE. DO NOT EDIT."

# Starlight renders the frontmatter title as the page's H1, and results.md
# carries its own. Demote so the document has exactly one. Safe as a plain
# regex because these pages are tables and prose with no fenced code.
TOP_LEVEL_HEADING = re.compile(r"^# ", re.MULTILINE)

PAGES = {
    "results.md": {
        "output": "index.md",
        "title": "Current evaluation result",
        # Rendered at /results/, so sibling pages are one segment away.
        "sibling_prefix": "",
    },
    "case-comparison.md": {
        "output": "case-comparison.md",
        "title": "Evaluation case comparison",
        # Rendered at /results/case-comparison/, so siblings need to climb.
        "sibling_prefix": "../",
    },
}

GENERATED_BY = "scripts/generate-docs-results.py"


def _evidence_module() -> Any:
    """Load the evidence generator for its bundle validation and identity rules.

    Reused rather than reimplemented so a bundle accepted for the evidence map
    and a bundle accepted for these pages are accepted on identical terms.
    """

    path = ROOT / "scripts/generate-docs-evidence.py"
    spec = importlib.util.spec_from_file_location("usagebench_docs_evidence", path)
    if spec is None or spec.loader is None:
        raise ValueError(f"could not load evidence generator: {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _frontmatter(title: str, description: str) -> str:
    if '"' in title or '"' in description:
        raise ValueError("frontmatter values must not contain double quotes")
    return f'---\ntitle: "{title}"\ndescription: "{description}"\n---\n\n'


def _absolute_links(body: str, tag: str) -> str:
    return BUNDLE_RELATIVE_LINK.sub(
        lambda match: f"]({REPOSITORY}/blob/{tag}/{match.group(1)})", body
    )


def _provenance_note(summary: dict[str, Any], sibling_prefix: str) -> str:
    tag = summary["release"]
    revision = summary["revision"]
    return (
        f"> **Immutable evidence.** This page is the result page generated inside\n"
        f"> the checksum-verified [{tag}]({REPOSITORY}/releases/tag/{tag}) publication\n"
        f"> bundle, at source revision [`{revision}`]({REPOSITORY}/tree/{revision}).\n"
        f"> It is reproduced here without retyping a score; the\n"
        f"> [evidence map]({sibling_prefix}evidence/) records its manifest and report checksums.\n\n"
    )


def _published_page(summary: dict[str, Any], source: str, spec: dict[str, str]) -> str:
    body = (summary["bundlePath"] / "results" / source).read_text(encoding="utf-8")
    if not body.startswith(GENERATED_MARKER):
        raise ValueError(
            f"generated {source} does not open with the result generator's "
            "provenance comment"
        )
    body = TOP_LEVEL_HEADING.sub("## ", body)
    tag = summary["release"]
    description = f"Immutable {tag} results for the reviewed evaluation slice."
    return (
        _frontmatter(spec["title"], description)
        + _provenance_note(summary, spec["sibling_prefix"])
        + _absolute_links(body, tag)
    )


def _pending_page(spec: dict[str, str]) -> str:
    description = "Awaiting a checksum-verified immutable publication bundle."
    return _frontmatter(spec["title"], description) + (
        "> **No published result.** This page is generated from a checksum-verified\n"
        "> immutable publication bundle. None was supplied to this generation run,\n"
        "> so no score is shown.\n\n"
        f"The [evidence map]({spec['sibling_prefix']}evidence/) records which slices have a verified\n"
        "release and which are still pending. Score tables never enter this\n"
        "repository by hand; they are transported from the generated result page\n"
        "inside a validated bundle when the documentation site is published.\n"
    )


def render(summary: dict[str, Any] | None) -> dict[Path, str]:
    pages: dict[Path, str] = {}
    for source, spec in PAGES.items():
        output = RESULTS_DIR / spec["output"]
        if summary is None:
            pages[output] = _pending_page(spec)
        else:
            pages[output] = _published_page(summary, source, spec)
    return pages


def evaluation_summary(bundle: Path | None) -> dict[str, Any] | None:
    if bundle is None:
        return None
    evidence = _evidence_module()
    summary = evidence._verified_bundle(bundle)
    if summary["sliceId"] not in {"v1", "v2"}:
        raise ValueError(
            "the current evaluation result page accepts only a prospective "
            f"evaluation bundle; received the {summary['sliceId']} slice"
        )
    summary["bundlePath"] = Path(bundle).resolve()
    return summary


def write_or_check(check: bool, summary: dict[str, Any] | None) -> int:
    pages = render(summary)
    drift = []
    for path, content in pages.items():
        if check:
            current = path.read_text(encoding="utf-8") if path.is_file() else None
            if current != content:
                drift.append(path)
        else:
            path.write_text(content, encoding="utf-8")
    if drift:
        for path in drift:
            print(
                f"{path.relative_to(ROOT)} is out of date; regenerate with {GENERATED_BY}",
                file=sys.stderr,
            )
        return 1
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="fail if generated docs files drift")
    parser.add_argument(
        "--bundle",
        type=Path,
        default=None,
        help=(
            "extracted immutable publication bundle to publish; omit to write "
            "the readiness placeholder"
        ),
    )
    args = parser.parse_args()
    try:
        summary = evaluation_summary(args.bundle)
    except ValueError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return write_or_check(args.check, summary)


if __name__ == "__main__":
    raise SystemExit(main())
