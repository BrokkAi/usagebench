#!/usr/bin/env python3
"""Create and verify checksum-bound staged UsageBench corpus manifests."""

import argparse
import hashlib
import json
import pathlib
import re
import sys
import time


INCLUDED_DIRECTORIES = (
    "adapters",
    "benchmarks",
    "containers",
    "fixtures",
    "schema",
    "scripts",
    "src",
)
INCLUDED_FILES = (
    ".dockerignore",
    ".usagebench-release.json",
    "ARTIFACT.md",
    "CITATION.cff",
    "Cargo.lock",
    "Cargo.toml",
    "LICENSE.md",
    "README.md",
    "RELEASES.md",
)


def digest_bytes(data):
    return hashlib.sha256(data).hexdigest()


def digest_file(path):
    hasher = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            hasher.update(chunk)
    return hasher.hexdigest()


def corpus_files(root):
    files = []
    for name in INCLUDED_FILES:
        path = root / name
        if not path.is_file() or path.is_symlink():
            raise ValueError(f"staged corpus input is missing or not a regular file: {name}")
        files.append(path)
    for name in INCLUDED_DIRECTORIES:
        directory = root / name
        if not directory.is_dir() or directory.is_symlink():
            raise ValueError(f"staged corpus input directory is missing or unsafe: {name}")
        for path in directory.rglob("*"):
            if path.is_symlink():
                raise ValueError(
                    f"staged corpus input contains a symlink: {path.relative_to(root)}"
                )
            if path.is_file():
                files.append(path)
    return sorted(files, key=lambda path: path.relative_to(root).as_posix())


def root_digest(document):
    identity = {
        "schemaVersion": document["schemaVersion"],
        "usagebenchRelease": document["usagebenchRelease"],
        "usagebenchRevision": document["usagebenchRevision"],
        "files": document["files"],
    }
    encoded = json.dumps(identity, sort_keys=True, separators=(",", ":")).encode()
    return digest_bytes(encoded)


def create(args):
    root = pathlib.Path(args.root).resolve()
    output = pathlib.Path(args.output).resolve()
    metadata = json.loads((root / ".usagebench-release.json").read_text())
    started = time.monotonic_ns()
    files = []
    for path in corpus_files(root):
        relative = path.relative_to(root).as_posix()
        before = path.stat()
        sha256 = digest_file(path)
        after = path.stat()
        if (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns) != (
            after.st_dev,
            after.st_ino,
            after.st_size,
            after.st_mtime_ns,
        ):
            raise ValueError(f"staged corpus input changed while hashing: {relative}")
        files.append({"path": relative, "sha256": sha256, "size": after.st_size})
    hashing_ms = (time.monotonic_ns() - started) // 1_000_000
    document = {
        "schemaVersion": 1,
        "usagebenchRelease": metadata["releaseTag"],
        "usagebenchRevision": metadata["revision"],
        "files": files,
    }
    document["rootDigest"] = f"sha256:{root_digest(document)}"
    output.write_text(json.dumps(document, indent=2, sort_keys=True) + "\n")

    timings = {
        "schemaVersion": 1,
        "releaseStagingMs": args.release_staging_ms,
        "corpusHashingMs": hashing_ms,
        "hashedFiles": len(files),
    }
    timings_output = pathlib.Path(args.timings_output).resolve()
    timings_output.write_text(json.dumps(timings, indent=2, sort_keys=True) + "\n")
    print(f"phase timing: release staging {args.release_staging_ms} ms", file=sys.stderr)
    print(
        f"phase timing: corpus hashing {hashing_ms} ms ({len(files)} files)",
        file=sys.stderr,
    )


def load_verified(root, manifest_path):
    document = json.loads(manifest_path.read_text())
    if set(document) != {
        "schemaVersion",
        "usagebenchRelease",
        "usagebenchRevision",
        "files",
        "rootDigest",
    }:
        raise ValueError("staged corpus hash manifest has an unexpected shape")
    if document.get("schemaVersion") != 1:
        raise ValueError("unsupported staged corpus hash schema")
    metadata = json.loads((root / ".usagebench-release.json").read_text())
    if (
        document.get("usagebenchRelease") != metadata.get("releaseTag")
        or document.get("usagebenchRevision") != metadata.get("revision")
    ):
        raise ValueError("staged corpus hash identity does not match release metadata")
    if document.get("rootDigest") != f"sha256:{root_digest(document)}":
        raise ValueError("staged corpus root digest does not match its entries")
    expected = {}
    for entry in document.get("files", []):
        if (
            set(entry) != {"path", "sha256", "size"}
            or not isinstance(entry["path"], str)
            or pathlib.PurePosixPath(entry["path"]).is_absolute()
            or ".." in pathlib.PurePosixPath(entry["path"]).parts
            or not re.fullmatch(r"[0-9a-f]{64}", entry["sha256"])
            or not isinstance(entry["size"], int)
            or entry["size"] < 0
        ):
            raise ValueError("staged corpus hash manifest contains an invalid file entry")
        expected[entry["path"]] = (entry["sha256"], entry["size"])
    if len(expected) != len(document.get("files", [])):
        raise ValueError("staged corpus hash manifest contains duplicate paths")
    actual_paths = [path.relative_to(root).as_posix() for path in corpus_files(root)]
    if sorted(expected) != actual_paths:
        raise ValueError("staged corpus file set does not match its hash manifest")
    for relative in actual_paths:
        path = root / relative
        expected_sha256, expected_size = expected[relative]
        if path.stat().st_size != expected_size or digest_file(path) != expected_sha256:
            raise ValueError(f"staged corpus checksum mismatch: {relative}")
    return document


def verify(args):
    started = time.monotonic_ns()
    root = pathlib.Path(args.root).resolve()
    document = load_verified(root, pathlib.Path(args.manifest).resolve())
    elapsed_ms = (time.monotonic_ns() - started) // 1_000_000
    print(
        f"phase timing: staged corpus verification {elapsed_ms} ms "
        f"({len(document['files'])} files, {document['rootDigest']})",
        file=sys.stderr,
    )


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(required=True)
    create_parser = sub.add_parser("create")
    create_parser.add_argument("--root", required=True)
    create_parser.add_argument("--output", required=True)
    create_parser.add_argument("--timings-output", required=True)
    create_parser.add_argument("--release-staging-ms", required=True, type=int)
    create_parser.set_defaults(func=create)
    verify_parser = sub.add_parser("verify")
    verify_parser.add_argument("--root", required=True)
    verify_parser.add_argument("--manifest", required=True)
    verify_parser.set_defaults(func=verify)
    args = parser.parse_args()
    try:
        args.func(args)
    except (OSError, ValueError, KeyError, json.JSONDecodeError) as error:
        print(f"staged corpus hash validation failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
