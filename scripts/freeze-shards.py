#!/usr/bin/env python3
"""Create and verify cryptographically bound real-project-v2 freeze shards."""

import argparse
import hashlib
import json
import pathlib
import shutil
import sys

SHARDS = {
    "bifrost-java": ("bifrost", "java"),
    "bifrost-rust": ("bifrost", "rust"),
    "bifrost-cpp": ("bifrost", "cpp"),
    "eclipse-jdtls-java": ("eclipse-jdtls", "java"),
    "rust-analyzer-rust": ("rust-analyzer", "rust"),
    "apple-clangd-21-cpp": ("apple-clangd-21", "cpp"),
}
FROZEN_FILES = [
    "benchmarks/evaluation/real-project-v2/protocol.json",
    "benchmarks/evaluation/real-project-v2/population.json",
    "benchmarks/evaluation/real-project-v2/selection.json",
    "benchmarks/evaluation/real-project-v2/review.json",
    "benchmarks/evaluation/real-project-v2/sources.json",
    "benchmarks/evaluation/real-project-v2/declarations.json",
    "benchmarks/evaluation/real-project-v2/candidates-v0.3.0.json",
]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def expected_files(root, language):
    files = sorted((root / "benchmarks/cases/evaluation/real-project-v2").glob(f"{language}-*.yaml"))
    if len(files) != 4:
        raise ValueError(f"expected four {language} documents, found {len(files)}")
    return [str(path.relative_to(root)) for path in files]


def identity(root, version, revision, shard):
    if shard not in SHARDS:
        raise ValueError(f"unknown shard: {shard}")
    candidate, language = SHARDS[shard]
    hashes = {name: digest(root / name) for name in FROZEN_FILES}
    for name in expected_files(root, language):
        hashes[name] = digest(root / name)
    registry = json.loads(
        (root / "benchmarks/evaluation/real-project-v2/candidates-v0.3.0.json").read_text()
    )
    profile = next(item for item in registry["candidates"] if item["id"] == candidate)
    return {
        "schemaVersion": 1,
        "freezeId": "real-project-v2",
        "version": version,
        "revision": revision,
        "shard": shard,
        "candidateId": candidate,
        "language": language,
        "candidateIdentity": {
            key: profile[key]
            for key in ("requestedVersion", "revision", "profile", "profileSha256")
            if key in profile
        },
        "caseFiles": expected_files(root, language),
        "frozenInputSha256": hashes,
    }


def write_metadata(args):
    root = pathlib.Path(args.root).resolve()
    report = pathlib.Path(args.report).resolve()
    data = identity(root, args.version, args.revision, args.shard)
    parsed = json.loads(report.read_text())
    actual = sorted(parsed.get("caseFiles", []))
    if actual != data["caseFiles"]:
        raise ValueError(f"shard case coverage mismatch: expected {data['caseFiles']}, got {actual}")
    if not parsed.get("completed"):
        raise ValueError("shard report is incomplete")
    data["reportFile"] = "report.json"
    data["reportSha256"] = digest(report)
    output = pathlib.Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(report, output.parent / "report.json")
    output.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
    (output.parent / "SHA256SUMS").write_text(
        f"{digest(output)}  metadata.json\n{data['reportSha256']}  report.json\n"
    )


def add_numbers(left, right):
    if isinstance(left, dict) and isinstance(right, dict):
        if left.keys() != right.keys():
            raise ValueError("report total shapes differ")
        return {key: add_numbers(left[key], right[key]) for key in left}
    if isinstance(left, int) and isinstance(right, int):
        return left + right
    if left != right:
        raise ValueError("non-numeric report totals differ")
    return left


def merge_bifrost(reports):
    merged = reports[0]
    invariant = ["usagebenchVersion", "usagebenchRevision", "usagebenchRelease", "runner", "invocation", "bifrostCommit", "bifrostResolvedCommit"]
    expected_environment = json.loads(json.dumps(merged["environment"]))
    expected_environment.get("analyzerExecutable", {}).pop("resolvedPath", None)
    for report in reports[1:]:
        for key in invariant:
            if report.get(key) != merged.get(key):
                raise ValueError(f"Bifrost shard invariant differs: {key}")
        environment = json.loads(json.dumps(report["environment"]))
        environment.get("analyzerExecutable", {}).pop("resolvedPath", None)
        if environment != expected_environment:
            raise ValueError("Bifrost shard invariant differs: environment")
        merged["requestedCaseFiles"] += report["requestedCaseFiles"]
        merged["caseFiles"] += report["caseFiles"]
        merged["documents"] += report["documents"]
        merged["semanticPackRuns"] += report.get("semanticPackRuns", [])
        merged["requestedTotals"] = add_numbers(merged["requestedTotals"], report["requestedTotals"])
        merged["totals"] = add_numbers(merged["totals"], report["totals"])
        merged["startedAtUnixSeconds"] = min(merged["startedAtUnixSeconds"], report["startedAtUnixSeconds"])
        merged["finishedAtUnixSeconds"] = max(merged["finishedAtUnixSeconds"], report["finishedAtUnixSeconds"])
    merged["requestedCaseFiles"] = sorted(merged["requestedCaseFiles"])
    merged["caseFiles"] = sorted(merged["caseFiles"])
    merged["documents"].sort(key=lambda item: item["caseFile"])
    merged["environment"]["analyzerExecutable"].pop("resolvedPath", None)
    merged["bifrostRepo"] = "https://github.com/BrokkAi/bifrost"
    return merged


def aggregate(args):
    root = pathlib.Path(args.root).resolve()
    artifacts = pathlib.Path(args.artifacts).resolve()
    output = pathlib.Path(args.output).resolve()
    seen = {}
    for metadata_path in artifacts.rglob("metadata.json"):
        directory = metadata_path.parent
        sums = (directory / "SHA256SUMS").read_text().splitlines()
        expected_sums = {line.split()[1]: line.split()[0] for line in sums}
        for name in ("metadata.json", "report.json"):
            if expected_sums.get(name) != digest(directory / name):
                raise ValueError(f"checksum mismatch for {directory / name}")
        metadata = json.loads(metadata_path.read_text())
        shard = metadata.get("shard")
        if shard in seen:
            raise ValueError(f"duplicate shard artifact: {shard}")
        if metadata != {**identity(root, args.version, args.revision, shard), "reportFile": "report.json", "reportSha256": digest(directory / "report.json")}:
            raise ValueError(f"stale or mismatched shard metadata: {shard}")
        report = json.loads((directory / "report.json").read_text())
        if sorted(report.get("caseFiles", [])) != metadata["caseFiles"] or not report.get("completed"):
            raise ValueError(f"invalid report coverage or completion: {shard}")
        seen[shard] = report
    if set(seen) != set(SHARDS):
        raise ValueError(f"shard set mismatch: expected {sorted(SHARDS)}, got {sorted(seen)}")
    output.mkdir(parents=True, exist_ok=False)
    bifrost = merge_bifrost([seen[name] for name in ("bifrost-java", "bifrost-rust", "bifrost-cpp")])
    reports = {
        "bifrost": bifrost,
        "eclipse-jdtls": seen["eclipse-jdtls-java"],
        "rust-analyzer": seen["rust-analyzer-rust"],
        "apple-clangd-21": seen["apple-clangd-21-cpp"],
    }
    all_cases = sorted(sum((expected_files(root, language) for language in ("java", "rust", "cpp")), []))
    if sorted(bifrost["caseFiles"]) != all_cases:
        raise ValueError("merged Bifrost report does not exactly cover the frozen corpus")
    for candidate, report in reports.items():
        (output / f"{candidate}.json").write_text(json.dumps(report, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(required=True)
    metadata = sub.add_parser("metadata")
    metadata.add_argument("--root", default=".")
    metadata.add_argument("--version", required=True)
    metadata.add_argument("--revision", required=True)
    metadata.add_argument("--shard", required=True)
    metadata.add_argument("--report", required=True)
    metadata.add_argument("--output", required=True)
    metadata.set_defaults(func=write_metadata)
    combine = sub.add_parser("aggregate")
    combine.add_argument("--root", default=".")
    combine.add_argument("--version", required=True)
    combine.add_argument("--revision", required=True)
    combine.add_argument("--artifacts", required=True)
    combine.add_argument("--output", required=True)
    combine.set_defaults(func=aggregate)
    args = parser.parse_args()
    try:
        args.func(args)
    except (OSError, ValueError, KeyError, json.JSONDecodeError) as error:
        print(f"freeze shard validation failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
