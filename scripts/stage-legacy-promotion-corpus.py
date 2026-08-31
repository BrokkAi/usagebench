#!/usr/bin/env python3
"""Stage an execution-only view of the reviewed legacy balanced core.

The release corpus remains byte-for-byte intact so the promotion manifest can
verify its historical source hashes.  This helper copies that corpus to a
temporary execution root and filters each legacy case document to the exact
balanced-core IDs named by the promotion manifest.  Reports therefore retain
their canonical ``benchmarks/cases/...`` paths while overflow and controls can
never enter a legacy run.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
from pathlib import Path


CASE_ITEM = re.compile(r"^  - id: ([A-Za-z0-9][A-Za-z0-9_-]*)\s*$", re.MULTILINE)
EXPECTED_CASE_COUNT = 110
CASE_KEY = re.compile(r"^    (?=\S)", re.MULTILINE)
EXPECTED_FAILURE_KEY = "    expectedFailure:"


def fail(message: str) -> None:
    raise SystemExit(message)


def balanced_case_ids(
    manifest_path: Path,
) -> tuple[dict[str, set[str]], dict[str, str], set[str]]:
    try:
        manifest = json.loads(manifest_path.read_text())
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        fail(f"cannot read legacy promotion manifest: {error}")
    documents = manifest.get("documents")
    if not isinstance(documents, list) or not documents:
        fail("legacy promotion manifest has no documents")

    by_file: dict[str, set[str]] = {}
    source_hashes: dict[str, str] = {}
    retired: set[str] = set()
    all_ids: set[str] = set()
    for document in documents:
        if not isinstance(document, dict):
            fail("legacy promotion manifest document is not an object")
        case_file = document.get("caseFile")
        source_sha256 = document.get("sourceSha256")
        cases = document.get("cases")
        if (
            not isinstance(case_file, str)
            or not isinstance(source_sha256, str)
            or not re.fullmatch(r"[0-9a-f]{64}", source_sha256)
            or not isinstance(cases, list)
        ):
            fail("legacy promotion manifest document has an invalid shape")
        relative_case_file = Path(case_file)
        if (
            relative_case_file.is_absolute()
            or ".." in relative_case_file.parts
            or relative_case_file.parts[:2] != ("benchmarks", "cases")
            or relative_case_file.suffix != ".yaml"
        ):
            fail(f"legacy promotion case file is outside benchmarks/cases: {case_file}")
        selected: set[str] = set()
        for case in cases:
            if not isinstance(case, dict) or not isinstance(case.get("id"), str):
                fail(f"legacy promotion case has an invalid shape: {case_file}")
            if case.get("membership") != "balanced_core":
                fail(
                    "legacy execution requires a manifest containing only "
                    f"balanced_core cases: {case_file}/{case.get('id')}"
                )
            case_id = case["id"]
            if case_id in all_ids:
                fail(f"legacy promotion case ID is duplicated: {case_id}")
            selected.add(case_id)
            all_ids.add(case_id)
            if "retiredExpectedFailure" in case:
                retired.add(case_id)
        if not selected:
            fail(f"legacy promotion document has no balanced_core cases: {case_file}")
        if case_file in by_file:
            fail(f"legacy promotion document is duplicated: {case_file}")
        by_file[case_file] = selected
        source_hashes[case_file] = source_sha256

    if len(all_ids) != EXPECTED_CASE_COUNT:
        fail(
            "legacy promotion execution requires exactly "
            f"{EXPECTED_CASE_COUNT} balanced_core cases, got {len(all_ids)}"
        )
    return by_file, source_hashes, retired


def retire_expected_failure(block: str, case_id: str) -> str:
    """Drop the ``expectedFailure`` mapping from one staged case block.

    The historical document is content-addressed and is never edited.  A
    promotion manifest that retires the annotation removes it from this
    execution-only copy, so the case is scored as an ordinary pass instead of
    reporting ``improved`` against an expectation it no longer fails.
    """

    keys = [match.start() for match in CASE_KEY.finditer(block)]
    starts = [start for start in keys if block.startswith(EXPECTED_FAILURE_KEY, start)]
    if len(starts) != 1:
        fail(
            f"case {case_id} does not author exactly one expectedFailure to retire: "
            f"found {len(starts)}"
        )
    start = starts[0]
    following = [key for key in keys if key > start]
    end = following[0] if following else len(block)
    retired = block[:start] + block[end:]
    if EXPECTED_FAILURE_KEY in retired:
        fail(f"retiring the expectedFailure for {case_id} left the annotation behind")
    if CASE_ITEM.search(block[start:end]):
        fail(f"retiring the expectedFailure for {case_id} would remove another case")
    return retired


def filter_document(
    source: Path, destination: Path, expected_ids: set[str], retired_ids: set[str]
) -> None:
    text = source.read_text()
    matches = list(CASE_ITEM.finditer(text))
    if not matches:
        fail(f"legacy case document contains no case list: {source}")
    actual_ids = [match.group(1) for match in matches]
    if len(actual_ids) != len(set(actual_ids)):
        fail(f"legacy case document contains duplicate case IDs: {source}")
    if not expected_ids.issubset(actual_ids):
        fail(
            f"legacy promotion IDs do not match source document {source}: "
            f"expected at least {sorted(expected_ids)}, got {sorted(actual_ids)}"
        )
    header = text[: matches[0].start()]
    blocks = []
    for index, match in enumerate(matches):
        case_id = match.group(1)
        if case_id not in expected_ids:
            continue
        end = matches[index + 1].start() if index + 1 < len(matches) else len(text)
        block = text[match.start() : end]
        if case_id in retired_ids:
            block = retire_expected_failure(block, case_id)
        blocks.append(block)
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(header + "".join(blocks))
    staged_ids = [match.group(1) for match in CASE_ITEM.finditer(destination.read_text())]
    if staged_ids != [case_id for case_id in actual_ids if case_id in expected_ids]:
        fail(f"filtered legacy document did not retain exactly the selected IDs: {destination}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-root", required=True, type=Path)
    parser.add_argument("--destination", required=True, type=Path)
    parser.add_argument("--promotion-manifest", required=True, type=Path)
    args = parser.parse_args()

    source_root = args.source_root.resolve()
    destination = args.destination.resolve()
    manifest = args.promotion_manifest.resolve()
    if not source_root.is_dir() or source_root.is_symlink():
        fail(f"source corpus root is missing or unsafe: {source_root}")
    try:
        destination.relative_to(source_root)
    except ValueError:
        pass
    else:
        fail("legacy execution destination must be outside the source corpus root")
    if destination.exists():
        if destination.is_symlink() or not destination.is_dir():
            fail(f"legacy execution destination is unsafe: {destination}")
        if any(destination.iterdir()):
            fail(f"legacy execution destination is not empty: {destination}")
    else:
        destination.parent.mkdir(parents=True, exist_ok=True)
    if not manifest.is_file() or manifest.is_symlink():
        fail(f"promotion manifest is missing or unsafe: {manifest}")
    try:
        relative_manifest = manifest.relative_to(source_root)
    except ValueError:
        fail("promotion manifest must be inside the source corpus root")
    # Corrections to the promotion tier are append-only, so a superseding
    # manifest lives beside its predecessor under a new legacy-vN directory.
    # The rail stays: execution is bound to a promotion manifest, never to an
    # arbitrary path.
    if not re.fullmatch(
        r"benchmarks/promotion/legacy-v[0-9]+/manifest\.json", relative_manifest.as_posix()
    ):
        fail("legacy execution is bound to benchmarks/promotion/legacy-vN/manifest.json")

    selected, source_hashes, retired = balanced_case_ids(manifest)
    shutil.copytree(source_root, destination, symlinks=False, dirs_exist_ok=True)
    case_root = destination / "benchmarks/cases"
    selected_files = set(selected)
    for path in case_root.rglob("*.yaml"):
        relative = path.relative_to(destination).as_posix()
        if relative not in selected_files:
            if path.is_symlink():
                fail(f"execution corpus contains an unsafe case symlink: {relative}")
            path.unlink()
    for case_file, case_ids in selected.items():
        source = source_root / case_file
        target = destination / case_file
        if not source.is_file() or source.is_symlink():
            fail(f"legacy promotion source document is missing or unsafe: {case_file}")
        source_digest = hashlib.sha256(source.read_bytes()).hexdigest()
        if source_digest != source_hashes[case_file]:
            fail(f"legacy promotion source hash does not match manifest: {case_file}")
        filter_document(source, target, case_ids, retired)
    staged_files = {
        path.relative_to(destination).as_posix()
        for path in case_root.rglob("*.yaml")
    }
    if staged_files != selected_files:
        fail(
            "legacy execution corpus case-file set drift: "
            f"expected {sorted(selected_files)}, got {sorted(staged_files)}"
        )
    print(
        f"staged legacy balanced_core execution corpus: {len(selected)} documents, "
        f"{EXPECTED_CASE_COUNT} cases"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
