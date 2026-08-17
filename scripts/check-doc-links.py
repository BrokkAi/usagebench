#!/usr/bin/env python3
"""Check relative links between Astro/Starlight content pages."""

from __future__ import annotations

import posixpath
import re
import sys
from pathlib import Path
from urllib.parse import urlsplit


ROOT = Path(__file__).resolve().parents[1]
DOCS = ROOT / "docs/src/content/docs"
LINK = re.compile(r"\[[^\]]+\]\(([^)\s]+)(?:\s+[^)]*)?\)")


def route_for(path: Path) -> str:
    relative = path.relative_to(DOCS).with_suffix("")
    if relative.name == "index":
        relative = relative.parent
    value = "/" + relative.as_posix().strip("/")
    return (value or "/") + "/"


def routes() -> set[str]:
    result: set[str] = set()
    for path in DOCS.rglob("*.md"):
        result.add(route_for(path))
    for path in DOCS.rglob("*.mdx"):
        result.add(route_for(path))
    return result


def resolve(source: Path, href: str) -> str | None:
    href = href.strip("<>")
    parsed = urlsplit(href)
    if parsed.scheme or parsed.netloc or href.startswith("#"):
        return None
    target = parsed.path
    if not target:
        return None
    # Markdown links are resolved from the rendered page URL. Starlight gives
    # both an index page and a content page a trailing-slash route, so that
    # route itself is the relative-link base (not its parent filesystem path).
    source_dir = route_for(source).strip("/")
    if target.startswith("/"):
        route = posixpath.normpath(target)
    else:
        route = posixpath.normpath(posixpath.join("/", source_dir, target))
    if not route.endswith("/"):
        route += "/"
    return route


def main() -> int:
    known = routes()
    missing: list[str] = []
    for path in sorted((*DOCS.rglob("*.md"), *DOCS.rglob("*.mdx"))):
        for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            for match in LINK.finditer(line):
                route = resolve(path, match.group(1))
                if route is not None and route not in known:
                    missing.append(f"{path.relative_to(ROOT)}:{line_number}: {match.group(1)} -> {route}")
    if missing:
        print("broken internal documentation links:", file=sys.stderr)
        print("\n".join(missing), file=sys.stderr)
        return 1
    print(f"checked {len(known)} documentation routes")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
