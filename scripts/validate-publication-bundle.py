#!/usr/bin/env python3
"""Validate the immutable inputs used to publish UsageBench result pages.

This is intentionally a release-boundary check rather than a score generator.
The Rust generator remains the authority for Markdown contents; this script
checks that the bundle contains the exact evidence and provenance that the
generator claims to have consumed.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import re
import sys
import tarfile
from typing import Any


SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
REVISION_RE = re.compile(r"^[0-9a-f]{40}$")
TAG_RE = re.compile(r"^v[0-9]+\.[0-9]+\.[0-9]+$")
SNAPSHOT_RE = re.compile(
    r"^Snapshot: (development|evaluation|legacy_promoted) (v[0-9]+\.[0-9]+\.[0-9]+)$"
)
REVISION_HEADER_RE = re.compile(r"^Revision: ([0-9a-f]{40})$")
MANIFEST_HEADER_RE = re.compile(r"^Manifest SHA-256: ([0-9a-f]{64})$")
REPORT_HEADER_RE = re.compile(r"^- ([^:]+): ([0-9a-f]{64})$")


class ValidationError(ValueError):
    """A user-actionable publication validation failure."""


def _require_regular_file(path: pathlib.Path, label: str) -> None:
    if path.is_symlink() or not path.is_file():
        raise ValidationError(f"{label} is missing or not a regular file: {path}")


def _require_directory(path: pathlib.Path, label: str) -> None:
    if path.is_symlink() or not path.is_dir():
        raise ValidationError(f"{label} is missing or unsafe: {path}")


def _sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def _load_json(path: pathlib.Path, label: str) -> dict[str, Any]:
    _require_regular_file(path, label)
    try:
        value = json.loads(path.read_text())
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise ValidationError(f"could not parse {label}: {error}") from error
    if not isinstance(value, dict):
        raise ValidationError(f"{label} must contain a JSON object")
    return value


def validate_release_metadata(
    bundle: pathlib.Path, expected_tag: str, expected_revision: str
) -> dict[str, Any]:
    if not TAG_RE.fullmatch(expected_tag):
        raise ValidationError(f"release tag is not immutable semver: {expected_tag}")
    if not REVISION_RE.fullmatch(expected_revision):
        raise ValidationError(f"release revision is not a full commit: {expected_revision}")
    metadata = _load_json(bundle / ".usagebench-release.json", "release metadata")
    if set(metadata) != {"releaseTag", "releaseVersion", "revision"}:
        raise ValidationError("release metadata has an unexpected shape")
    expected = {
        "releaseTag": expected_tag,
        "releaseVersion": expected_tag[1:],
        "revision": expected_revision,
    }
    if metadata != expected:
        raise ValidationError(
            f"release metadata does not match requested identity: {metadata!r} != {expected!r}"
        )
    return metadata


def verify_corpus_hashes(bundle: pathlib.Path) -> None:
    manifest = bundle / ".usagebench-corpus-hashes.json"
    _require_regular_file(manifest, "staged corpus hash manifest")
    verifier = bundle / "scripts" / "corpus-hashes.py"
    _require_regular_file(verifier, "staged corpus hash verifier")
    import subprocess

    result = subprocess.run(
        [
            sys.executable,
            str(verifier),
            "verify",
            "--root",
            str(bundle),
            "--manifest",
            str(manifest),
        ],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        detail = (result.stderr or result.stdout).strip()
        raise ValidationError(f"staged corpus checksum verification failed: {detail}")


def _safe_relative(path: str) -> pathlib.PurePosixPath:
    relative = pathlib.PurePosixPath(path)
    if relative.is_absolute() or ".." in relative.parts or not path:
        raise ValidationError(f"checksum list contains an unsafe path: {path!r}")
    return relative


def verify_evidence_checksums(bundle: pathlib.Path) -> None:
    evidence = bundle / "evidence"
    _require_directory(evidence, "frozen evidence directory")
    sums_path = evidence / "SHA256SUMS"
    _require_regular_file(sums_path, "frozen evidence checksum list")

    expected: dict[str, str] = {}
    for line_number, line in enumerate(sums_path.read_text().splitlines(), start=1):
        match = re.fullmatch(r"([0-9a-f]{64})  (.+)", line)
        if match is None:
            raise ValidationError(
                f"frozen evidence checksum list has an invalid line {line_number}"
            )
        digest, relative_text = match.groups()
        relative = _safe_relative(relative_text)
        if relative_text in expected:
            raise ValidationError(f"frozen evidence checksum list repeats {relative_text}")
        if relative.parts[0] != relative_text or relative.suffix != ".json":
            raise ValidationError(
                "frozen evidence checksum entries must be simple JSON file names"
            )
        expected[relative_text] = digest

    actual_files = sorted(
        path.name
        for path in evidence.iterdir()
        if path.name != "SHA256SUMS" and path.is_file() and not path.is_symlink()
    )
    unsafe = [
        path.name
        for path in evidence.iterdir()
        if path.name != "SHA256SUMS" and (path.is_symlink() or not path.is_file())
    ]
    if unsafe:
        raise ValidationError(f"frozen evidence contains unsafe entries: {unsafe}")
    if actual_files != sorted(expected):
        raise ValidationError(
            f"frozen evidence file set differs from SHA256SUMS: {actual_files!r} != {sorted(expected)!r}"
        )
    for name, digest in expected.items():
        actual = _sha256(evidence / name)
        if actual != digest:
            raise ValidationError(
                f"frozen evidence checksum mismatch for {name}: {digest} != {actual}"
            )


def validate_freeze_partition(
    bundle: pathlib.Path, expected_tag: str, expected_revision: str
) -> dict[str, Any]:
    manifest_path = bundle / "evidence" / "freeze-manifest.json"
    manifest = _load_json(manifest_path, "freeze manifest")
    if manifest.get("schemaVersion") not in (4, 5):
        raise ValidationError("unsupported freeze manifest schema version")
    if manifest.get("version") != expected_tag or manifest.get("revision") != expected_revision:
        raise ValidationError("freeze manifest identity does not match release metadata")
    kind = manifest.get("snapshotKind")
    if kind not in {"development", "evaluation", "legacy_promoted"}:
        raise ValidationError(f"unsupported freeze snapshot kind: {kind!r}")
    evaluation_audit = manifest.get("evaluationAudit")
    legacy_audit = manifest.get("legacyPromotionAudit")
    if kind == "development" and (evaluation_audit is not None or legacy_audit is not None):
        raise ValidationError("development freeze cannot carry evaluation or legacy audit metadata")
    if kind == "evaluation" and (evaluation_audit is None or legacy_audit is not None):
        raise ValidationError("evaluation freeze must carry only prospective evaluation audit metadata")
    if kind == "legacy_promoted" and (legacy_audit is None or evaluation_audit is not None):
        raise ValidationError("legacy freeze must carry only retrospective promotion audit metadata")

    corpus = manifest.get("corpus")
    if not isinstance(corpus, list) or not corpus:
        raise ValidationError("freeze manifest corpus is empty")
    partitions = {entry.get("partition") for entry in corpus if isinstance(entry, dict)}
    expected_partition = "evaluation" if kind == "evaluation" else "development"
    if partitions != {expected_partition}:
        raise ValidationError(
            f"{kind} freeze mixes corpus partitions: {sorted(partitions)!r}"
        )
    candidates = manifest.get("candidates")
    reports = manifest.get("reports")
    if not isinstance(candidates, list) or not isinstance(reports, list):
        raise ValidationError("freeze manifest candidates/reports are not arrays")
    candidate_ids = [entry.get("id") for entry in candidates if isinstance(entry, dict)]
    if (
        len(candidate_ids) != len(candidates)
        or any(not isinstance(candidate_id, str) or not candidate_id for candidate_id in candidate_ids)
        or not candidate_ids
        or len(set(candidate_ids)) != len(candidate_ids)
    ):
        raise ValidationError("freeze manifest candidate IDs are missing or duplicated")
    for entry in reports:
        if not isinstance(entry, dict):
            raise ValidationError("freeze manifest report entries must be objects")
        if (
            not isinstance(entry.get("candidateId"), str)
            or not isinstance(entry.get("file"), str)
            or pathlib.PurePosixPath(entry["file"]).name != entry["file"]
            or not SHA256_RE.fullmatch(entry.get("sha256", ""))
        ):
            raise ValidationError("freeze manifest report identity or checksum is invalid")
    report_ids = [entry["candidateId"] for entry in reports]
    if report_ids != candidate_ids and set(report_ids) != set(candidate_ids):
        raise ValidationError("freeze manifest does not provide one report per candidate")
    return manifest


def _header_reports(lines: list[str]) -> dict[str, str]:
    try:
        start = lines.index("Input reports:") + 1
        end = lines.index("-->")
    except ValueError as error:
        raise ValidationError("generated result page is missing its provenance header") from error
    reports: dict[str, str] = {}
    for line in lines[start:end]:
        match = REPORT_HEADER_RE.fullmatch(line)
        if match is None:
            raise ValidationError(f"generated result page has an invalid report header: {line!r}")
        candidate, digest = match.groups()
        if candidate in reports:
            raise ValidationError(f"generated result page repeats report {candidate}")
        reports[candidate] = digest
    return reports


def validate_generated_page(
    path: pathlib.Path, manifest: dict[str, Any], manifest_digest: str
) -> None:
    _require_regular_file(path, "generated result page")
    lines = path.read_text().splitlines()
    if not lines or lines[0] != "<!-- GENERATED FILE. DO NOT EDIT.":
        raise ValidationError(f"result page is not marked as generated: {path}")
    snapshot_line = next((line for line in lines if line.startswith("Snapshot: ")), "")
    snapshot = SNAPSHOT_RE.fullmatch(snapshot_line)
    if snapshot is None or snapshot.group(1) != manifest["snapshotKind"] or snapshot.group(2) != manifest["version"]:
        raise ValidationError(f"generated result page snapshot identity is stale: {path}")
    revision_line = next((line for line in lines if line.startswith("Revision: ")), "")
    revision = REVISION_HEADER_RE.fullmatch(revision_line)
    if revision is None or revision.group(1) != manifest["revision"]:
        raise ValidationError(f"generated result page revision is stale: {path}")
    manifest_line = next((line for line in lines if line.startswith("Manifest SHA-256: ")), "")
    recorded = MANIFEST_HEADER_RE.fullmatch(manifest_line)
    if recorded is None or recorded.group(1) != manifest_digest:
        raise ValidationError(f"generated result page manifest digest is stale: {path}")
    expected_reports = {
        entry["candidateId"]: entry["sha256"] for entry in manifest["reports"]
    }
    if _header_reports(lines) != expected_reports:
        raise ValidationError(f"generated result page report provenance is stale: {path}")


def validate_generated_results(bundle: pathlib.Path, manifest: dict[str, Any]) -> None:
    results = bundle / "results"
    _require_directory(results, "generated results directory")
    required = [results / "results.md", results / "case-comparison.md"]
    for path in required:
        _require_regular_file(path, "generated result page")
    extras = [
        path
        for path in results.iterdir()
        if path.is_symlink() or not path.is_file()
    ]
    if extras:
        raise ValidationError(f"generated results contain unsafe entries: {extras}")
    manifest_digest = _sha256(bundle / "evidence" / "freeze-manifest.json")
    for path in sorted(results.glob("*.md")):
        validate_generated_page(path, manifest, manifest_digest)


def _validate_archive_members(archive: pathlib.Path, root_name: str) -> None:
    with tarfile.open(archive, "r:gz") as tar:
        names: set[str] = set()
        for member in tar.getmembers():
            name = member.name.rstrip("/")
            if not name or name in names:
                raise ValidationError(f"publication archive contains duplicate or empty member: {member.name}")
            names.add(name)
            relative = pathlib.PurePosixPath(name)
            if relative.is_absolute() or ".." in relative.parts or relative.parts[0] != root_name:
                raise ValidationError(f"publication archive contains an unsafe member: {member.name}")
            if member.issym() or member.islnk() or not (member.isfile() or member.isdir()):
                raise ValidationError(f"publication archive contains an unsafe member type: {member.name}")


def verify_archive(
    archive: pathlib.Path,
    checksum: pathlib.Path,
    expected_tag: str,
    expected_revision: str,
    extract_to: pathlib.Path,
) -> pathlib.Path:
    _require_regular_file(archive, "release archive")
    _require_regular_file(checksum, "release archive checksum")
    expected_name = archive.name
    lines = checksum.read_text().splitlines()
    matching = [line for line in lines if re.fullmatch(rf"[0-9a-f]{{64}}  {re.escape(expected_name)}", line)]
    if len(lines) != 1 or len(matching) != 1:
        raise ValidationError("release checksum file must contain exactly one checksum for its archive")
    expected_digest = matching[0].split()[0]
    actual_digest = _sha256(archive)
    if expected_digest != actual_digest:
        raise ValidationError(f"release archive checksum mismatch: {expected_digest} != {actual_digest}")
    root_name = f"usagebench-{expected_tag}"
    _validate_archive_members(archive, root_name)
    if extract_to.exists():
        raise ValidationError(f"archive extraction destination already exists: {extract_to}")
    extract_to.mkdir(parents=True)
    with tarfile.open(archive, "r:gz") as tar:
        tar.extractall(extract_to)
    bundle = extract_to / root_name
    validate_bundle(bundle, expected_tag, expected_revision)
    return bundle


def validate_bundle(bundle: pathlib.Path, expected_tag: str, expected_revision: str) -> None:
    _require_directory(bundle, "publication bundle")
    validate_release_metadata(bundle, expected_tag, expected_revision)
    verify_corpus_hashes(bundle)
    verify_evidence_checksums(bundle)
    manifest = validate_freeze_partition(bundle, expected_tag, expected_revision)
    validate_generated_results(bundle, manifest)


def main() -> int:
    parser = argparse.ArgumentParser()
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--bundle", type=pathlib.Path)
    source.add_argument("--archive", type=pathlib.Path)
    parser.add_argument("--checksum", type=pathlib.Path)
    parser.add_argument("--extract-to", type=pathlib.Path)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--revision", required=True)
    args = parser.parse_args()
    if args.archive is not None and (args.checksum is None or args.extract_to is None):
        parser.error("--archive requires --checksum and --extract-to")
    if args.bundle is not None and (args.checksum is not None or args.extract_to is not None):
        parser.error("--checksum/--extract-to are only valid with --archive")
    try:
        if args.archive is not None:
            bundle = verify_archive(
                args.archive,
                args.checksum,
                args.tag,
                args.revision,
                args.extract_to,
            )
        else:
            bundle = args.bundle.resolve()
            validate_bundle(bundle, args.tag, args.revision)
        print(f"validated immutable publication bundle {bundle}")
    except (OSError, ValidationError, tarfile.TarError, json.JSONDecodeError) as error:
        print(f"publication bundle validation failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
