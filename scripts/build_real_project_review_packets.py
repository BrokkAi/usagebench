#!/usr/bin/env python3
"""Build blinded per-case review packets without creating execution evidence."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path


LANGUAGES = ("java", "rust", "cpp")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n")


def linked(path: Path, repo_root: Path) -> dict[str, str]:
    return {"file": path.relative_to(repo_root).as_posix(), "sha256": sha256(path)}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--declarations", type=Path, required=True)
    parser.add_argument("--sources", type=Path, required=True)
    parser.add_argument("--output-root", type=Path, required=True)
    parser.add_argument("--repo-root", type=Path, default=Path("."))
    args = parser.parse_args()

    repo_root = args.repo_root.resolve()
    declarations_path = args.declarations.resolve()
    sources_path = args.sources.resolve()
    declarations = json.loads(declarations_path.read_text())
    sources = json.loads(sources_path.read_text())
    if declarations["freezeId"] != "real-project-v2":
        raise SystemExit("review packet builder only accepts real-project-v2")
    if sources["freezeId"] != declarations["freezeId"]:
        raise SystemExit("source lock and declaration ranking freeze IDs differ")

    source_by_identity = {
        (entry["repo"], entry["commit"]): entry for entry in sources["sources"]
    }
    selected_by_language: dict[str, list[dict[str, object]]] = {}
    for profile in declarations["profiles"]:
        language = profile["language"]
        selected_by_language[language] = [
            {
                "repository": repository,
                "declaration": declaration,
            }
            for repository in profile["selected"]
            for declaration in repository["declarationDraw"]["selected"]
        ]

    if set(selected_by_language) != set(LANGUAGES):
        raise SystemExit("expected exactly the frozen Java, Rust, and C++ profiles")
    if any(len(selected_by_language[language]) != 12 for language in LANGUAGES):
        raise SystemExit("expected 12 selected declarations per language")

    protocol_files = {
        "protocol": repo_root / "benchmarks/review-protocol/blinded-agent-review-v3.json",
        "methodology": repo_root / "benchmarks/review-protocol/per-case-methodology-v3.md",
        "prompt": repo_root / "benchmarks/review-protocol/per-case-blinded-agent-prompt-v3.md",
        "responseSchema": repo_root / "benchmarks/review-protocol/agent-response-v1.schema.json",
    }

    for milestone_index in range(6):
        milestone_number = milestone_index + 1
        milestone_id = f"real-project-v2-agent-panel-milestone-{milestone_number}-v3"
        milestone_root = (args.output_root / milestone_id).resolve()
        selected = [
            item
            for language in LANGUAGES
            for item in selected_by_language[language][milestone_index * 2 : milestone_index * 2 + 2]
        ]
        packet_links = []
        source_links: dict[tuple[str, str], dict[str, object]] = {}
        for item in selected:
            repository = item["repository"]
            declaration = item["declaration"]
            source = repository["source"]
            source_lock = source_by_identity[(source["repo"], source["commit"])]
            packet = {
                "schemaVersion": 2,
                "caseId": declaration["caseId"],
                "language": next(
                    language
                    for language in LANGUAGES
                    if item in selected_by_language[language]
                ),
                "referencePolicy": "bindings_optional",
                "positionEncoding": "utf-16",
                "source": {
                    "root": "source",
                    "repo": source["repo"],
                    "commit": source["commit"],
                    "archiveSha256": source_lock["sha256"],
                },
                "declaration": {
                    "uri": declaration["uri"],
                    "range": declaration["range"],
                },
                "displayName": declaration["displayName"],
            }
            packet_path = milestone_root / "packets" / f"{declaration['caseId']}.json"
            write_json(packet_path, packet)
            packet_links.append({"caseId": declaration["caseId"], **linked(packet_path, repo_root)})
            source_links[(source["repo"], source["commit"])] = {
                key: source_lock[key]
                for key in ("repo", "commit", "tree", "archive", "sha256")
            }

        manifest = {
            "schemaVersion": 1,
            "milestoneId": milestone_id,
            "status": "awaiting_external_sessions",
            "milestone": {
                "number": milestone_number,
                "partitionProgress": f"{milestone_number * 6}/36 packets prepared",
                "caseIds": [link["caseId"] for link in packet_links],
                "balance": {language: 2 for language in LANGUAGES},
            },
            "executionRequired": {
                "sessionPolicy": "fresh_session_per_provider_per_case",
                "providers": ["openai", "anthropic"],
                "sessionCount": 12,
                "humanAdjudicationRequired": True,
                "note": "This manifest records prepared inputs only; it is not review, execution, or adjudication evidence.",
            },
            **{name: linked(path, repo_root) for name, path in protocol_files.items()},
            "declarationRanking": linked(declarations_path, repo_root),
            "sourceLock": linked(sources_path, repo_root),
            "packets": packet_links,
            "sources": list(source_links.values()),
        }
        write_json(milestone_root / "packet-manifest.json", manifest)


if __name__ == "__main__":
    main()
