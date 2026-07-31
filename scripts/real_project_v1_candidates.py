#!/usr/bin/env python3
"""Deterministically enumerate the real-project-v1 declaration draw.

This is intentionally a source-only tool. It reads extracted, pinned source
archives and never invokes Bifrost or a language server.
"""

from __future__ import annotations

import argparse
import ast
import hashlib
import json
import re
from collections import Counter
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


PREFIX = b"usagebench-real-project-v1\0"
ALGORITHM = "real-project-v1-source-syntax-v1"
EXCLUDED_PARTS = {
    ".git",
    "benchmark",
    "benchmarks",
    "build",
    "dist",
    "example",
    "examples",
    "fixtures",
    "generated",
    "node_modules",
    "test",
    "tests",
    "third_party",
    "vendor",
}


@dataclass(frozen=True)
class Candidate:
    path: str
    line: int
    start: int
    end: int
    name: str
    kind: str

    @property
    def uri(self) -> str:
        return f"benchmark://source/{self.path}"

    @property
    def range_text(self) -> str:
        return f"{self.line}:{self.start}-{self.line}:{self.end}"


def utf16_column(text: str) -> int:
    return len(text.encode("utf-16-le")) // 2


def source_files(root: Path, language: str) -> list[Path]:
    extensions = {
        "go": {".go"},
        "python": {".py"},
        "typescript": {".ts", ".tsx"},
    }[language]
    result = []
    for path in root.rglob("*"):
        if not path.is_file() or path.suffix not in extensions:
            continue
        relative = path.relative_to(root)
        lowered = {part.lower() for part in relative.parts}
        if lowered & EXCLUDED_PARTS:
            continue
        name = path.name.lower()
        if (
            name.endswith("_test.go")
            or name.startswith("test_")
            or name.endswith("_test.py")
            or re.search(r"(?:^|[._-])(?:test|spec)(?:[._-]|$)", name)
            or name.endswith(".d.ts")
            or ".generated." in name
            or name.startswith("generated_")
        ):
            continue
        result.append(path)
    return sorted(result, key=lambda item: item.relative_to(root).as_posix())


def candidate_from_match(root: Path, path: Path, line_index: int, line: str, match: re.Match[str], kind: str) -> Candidate:
    name = match.group("name")
    prefix = line[: match.start("name")]
    start = utf16_column(prefix)
    return Candidate(
        path=path.relative_to(root).as_posix(),
        line=line_index,
        start=start,
        end=start + utf16_column(name),
        name=name,
        kind=kind,
    )


def go_candidates(root: Path, files: Iterable[Path]) -> list[Candidate]:
    function = re.compile(
        r"^func\s+(?:(?P<receiver>\([^)]*\))\s*)?(?P<name>[A-Za-z_][A-Za-z0-9_]*)\s*(?:\[[^]]+\]\s*)?\("
    )
    type_decl = re.compile(r"^type\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)\s+(?:\[[^]]+\]\s*)?(?:struct|interface)\b")
    result = []
    for path in files:
        for line_index, line in enumerate(path.read_text(encoding="utf-8").splitlines()):
            match = function.match(line)
            if match:
                result.append(candidate_from_match(root, path, line_index, line, match, "method" if match.group("receiver") else "function"))
                continue
            match = type_decl.match(line)
            if match:
                result.append(candidate_from_match(root, path, line_index, line, match, "interface" if "interface" in line[match.end("name") :] else "class"))
    return result


def python_candidates(root: Path, files: Iterable[Path]) -> list[Candidate]:
    result = []
    for path in files:
        text = path.read_text(encoding="utf-8")
        try:
            module = ast.parse(text, filename=str(path))
        except SyntaxError:
            continue
        lines = text.splitlines()
        for node in module.body:
            if not isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
                continue
            line_index = node.lineno - 1
            line = lines[line_index]
            match = re.search(rf"\b(?P<name>{re.escape(node.name)})\b", line)
            if match:
                kind = "class" if isinstance(node, ast.ClassDef) else "function"
                result.append(candidate_from_match(root, path, line_index, line, match, kind))
    return result


def typescript_candidates(root: Path, files: Iterable[Path]) -> list[Candidate]:
    declaration = re.compile(
        r"^(?:export\s+)?(?:default\s+)?(?:declare\s+)?(?:async\s+)?"
        r"(?P<kind>function|class|interface|enum|type)\s+(?P<name>[A-Za-z_$][A-Za-z0-9_$]*)\b"
    )
    result = []
    kind_map = {"type": "type", "interface": "interface", "enum": "enum", "class": "class", "function": "function"}
    for path in files:
        for line_index, line in enumerate(path.read_text(encoding="utf-8").splitlines()):
            match = declaration.match(line)
            if match:
                result.append(candidate_from_match(root, path, line_index, line, match, kind_map[match.group("kind")]))
    return result


def occurrence_map(root: Path, files: Iterable[Path], names: set[str]) -> dict[str, list[dict[str, object]]]:
    token = re.compile(r"[A-Za-z_$][A-Za-z0-9_$]*")
    result: dict[str, list[dict[str, object]]] = {name: [] for name in names}
    for path in files:
        relative = path.relative_to(root).as_posix()
        for line_index, line in enumerate(path.read_text(encoding="utf-8").splitlines()):
            for match in token.finditer(line):
                name = match.group(0)
                if name not in names:
                    continue
                start = utf16_column(line[: match.start()])
                result[name].append(
                    {
                        "uri": f"benchmark://source/{relative}",
                        "range": {
                            "start": {"line": line_index, "character": start},
                            "end": {
                                "line": line_index,
                                "character": start + utf16_column(name),
                            },
                        },
                    }
                )
    return result


def digest(protocol_commit: str, language: str, candidate_id: str, candidate: Candidate) -> str:
    value = b"".join(
        [
            PREFIX,
            protocol_commit.encode(),
            b"\0",
            language.encode(),
            b"\0",
            candidate_id.encode(),
            b"\0declaration\0",
            candidate.uri.encode(),
            b"\0",
            candidate.range_text.encode(),
        ]
    )
    return hashlib.sha256(value).hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--selection", type=Path, required=True)
    parser.add_argument("--sources-root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--replace-case", action="append", default=[])
    parser.add_argument(
        "--select-rank",
        action="append",
        default=[],
        metavar="CASE_ID=RANK",
    )
    args = parser.parse_args()

    selection = json.loads(args.selection.read_text())
    protocol_commit = selection["protocolCommit"]
    replacement_cases = set(args.replace_case)
    unknown_replacements = set(replacement_cases)
    selected_ranks = {}
    for value in args.select_rank:
        case_id, separator, rank = value.rpartition("=")
        if not separator or not case_id or not rank.isdigit() or int(rank) < 1:
            raise SystemExit(f"invalid --select-rank value: {value}")
        selected_ranks[case_id] = int(rank)
    unknown_selected_ranks = set(selected_ranks)
    documents = []
    for profile in selection["profiles"]:
        language = profile["language"]
        candidate_id = profile["candidateId"]
        for repository in profile["selected"]:
            slug = repository["fullName"].replace("/", "--")
            root = args.sources_root / slug
            files = source_files(root, language)
            if language == "go":
                raw = go_candidates(root, files)
            elif language == "python":
                raw = python_candidates(root, files)
            else:
                raw = typescript_candidates(root, files)

            name_counts = Counter(candidate.name for candidate in raw)
            sites_by_name = occurrence_map(root, files, set(name_counts))
            eligible = []
            for candidate in raw:
                if candidate.name.startswith("_") or name_counts[candidate.name] != 1:
                    continue
                sites = sites_by_name[candidate.name]
                if len(sites) < 2:
                    continue
                eligible.append((digest(protocol_commit, language, candidate_id, candidate), candidate, sites))
            eligible.sort(key=lambda item: item[0])
            if len(eligible) < len(repository["caseIds"]):
                raise SystemExit(f"{repository['fullName']} has only {len(eligible)} eligible declarations")

            ranked = []
            ranked_with_sites = []
            for rank, (value, candidate, sites) in enumerate(eligible, 1):
                record = {
                    "rank": rank,
                    "digest": value,
                    "uri": candidate.uri,
                    "range": {
                        "start": {"line": candidate.line, "character": candidate.start},
                        "end": {"line": candidate.line, "character": candidate.end},
                    },
                    "kind": candidate.kind,
                    "displayName": candidate.name,
                    "occurrenceCount": len(sites),
                }
                ranked.append(record)
                ranked_with_sites.append((record, sites))

            selected = []
            replacement_cursor = len(repository["caseIds"])
            for index, case_id in enumerate(repository["caseIds"]):
                source_index = index
                if case_id in selected_ranks:
                    unknown_selected_ranks.discard(case_id)
                    source_index = selected_ranks[case_id] - 1
                    selection["replacements"].append(
                        {
                            "language": language,
                            "candidateId": candidate_id,
                            "repository": repository["fullName"],
                            "caseId": case_id,
                            "status": "replaced",
                            "rule": "next recorded declaration candidate from the same pinned repository",
                            "replacedRank": index + 1,
                            "selectedRank": source_index + 1,
                            "reason": "independent source review rejected every earlier assigned candidate before analyzer execution",
                        }
                    )
                elif case_id in replacement_cases:
                    unknown_replacements.discard(case_id)
                    source_index = replacement_cursor
                    replacement_cursor += 1
                    selection["replacements"].append(
                        {
                            "language": language,
                            "candidateId": candidate_id,
                            "repository": repository["fullName"],
                            "caseId": case_id,
                            "status": "replaced",
                            "rule": "next recorded declaration candidate from the same pinned repository",
                            "replacedRank": index + 1,
                            "selectedRank": source_index + 1,
                            "reason": "independent source review rejected the initial declaration before analyzer execution",
                        }
                    )
                record, sites = ranked_with_sites[source_index]
                chosen = dict(record)
                chosen["caseId"] = case_id
                chosen["sourceOnlyOccurrences"] = sites
                selected.append(chosen)
            repository["declarationDraw"] = {
                "algorithm": ALGORITHM,
                "rules": {
                    "scope": "module/package-level named functions, Go methods, and nominal types",
                    "uniqueDeclarationName": True,
                    "minimumSourceTokenOccurrences": 2,
                    "excludeLeadingUnderscore": True,
                    "excludedPathParts": sorted(EXCLUDED_PARTS),
                    "excludedFileKinds": ["tests", "specs", "generated", "declaration-only TypeScript"],
                    "positionEncoding": "utf-16",
                },
                "eligibleCandidateCount": len(ranked),
                "ranked": ranked,
                "selected": selected,
            }
            documents.append(
                {
                    "caseFile": repository["caseFile"],
                    "language": language,
                    "candidateId": candidate_id,
                    "source": repository["source"],
                    "caseIds": repository["caseIds"],
                }
            )
    if unknown_replacements:
        raise SystemExit(
            "replacement case IDs were not present in the draw: "
            + ", ".join(sorted(unknown_replacements))
        )
    if unknown_selected_ranks:
        raise SystemExit(
            "selected-rank case IDs were not present in the draw: "
            + ", ".join(sorted(unknown_selected_ranks))
        )
    selection["documents"] = documents
    args.output.write_text(json.dumps(selection, indent=2) + "\n")


if __name__ == "__main__":
    main()
