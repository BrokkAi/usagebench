#!/usr/bin/env python3
"""Create the content-addressed source lock for real-project-v1."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import tarfile
import tempfile
from pathlib import Path, PurePosixPath


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def archive_tree(path: Path, gitlinks: list[dict[str, str]]) -> str:
    with tempfile.TemporaryDirectory(prefix="usagebench-archive-tree-") as temporary:
        subprocess.run(["git", "init", "--bare", "--quiet", temporary], check=True)
        importer = subprocess.Popen(
            ["git", "fast-import", "--quiet"],
            cwd=temporary,
            stdin=subprocess.PIPE,
        )
        if importer.stdin is None:
            raise SystemExit("cannot open git fast-import input")
        stream = importer.stdin
        stream.write(
            b"feature done\ncommit refs/heads/archive\n"
            b"committer UsageBench <usagebench@example.invalid> 0 +0000\n"
            b"data 0\n\n"
        )
        with tarfile.open(path, "r:gz") as archive:
            for member in archive:
                relative = PurePosixPath(member.name)
                if relative.is_absolute() or ".." in relative.parts:
                    raise SystemExit(f"unsafe archive path {member.name}")
                if member.isdir() or member.name == "pax_global_header":
                    continue
                if member.isfile():
                    source = archive.extractfile(member)
                    if source is None:
                        raise SystemExit(f"cannot read {member.name} from {path}")
                    data = source.read()
                    mode = "100755" if member.mode & 0o111 else "100644"
                elif member.issym():
                    data = member.linkname.encode()
                    mode = "120000"
                else:
                    raise SystemExit(f"unsupported archive entry {member.name}")
                stream.write(f"M {mode} inline {json.dumps(member.name)}\n".encode())
                stream.write(f"data {len(data)}\n".encode())
                stream.write(data)
                stream.write(b"\n")
        for gitlink in gitlinks:
            stream.write(
                f"M 160000 {gitlink['commit']} {json.dumps(gitlink['path'])}\n".encode()
            )
        stream.write(b"\ndone\n")
        stream.close()
        if importer.wait() != 0:
            raise SystemExit("git fast-import failed while reconstructing archive tree")
        return subprocess.run(
            ["git", "-C", temporary, "rev-parse", "refs/heads/archive^{tree}"],
            check=True,
            text=True,
            capture_output=True,
        ).stdout.strip()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--selection", type=Path, required=True)
    parser.add_argument("--archives", type=Path, required=True)
    parser.add_argument("--checkouts", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    selection = json.loads(args.selection.read_text())
    sources = []
    seen = set()
    for profile in selection["profiles"]:
        for repository in profile["selected"]:
            source = repository["source"]
            identity = (source["repo"], source["commit"])
            if identity in seen:
                continue
            seen.add(identity)
            slug = repository["fullName"].replace("/", "--")
            checkout = args.checkouts / slug
            archive = args.archives / f"{slug}.tar.gz"
            archive.parent.mkdir(parents=True, exist_ok=True)
            git_directory = subprocess.run(
                ["git", "-C", str(checkout), "rev-parse", "--absolute-git-dir"],
                check=True,
                text=True,
                capture_output=True,
            ).stdout.strip()
            with tempfile.TemporaryDirectory(prefix="usagebench-no-attributes-") as work_tree:
                subprocess.run(
                    [
                        "git",
                        "-c",
                        "core.attributesFile=/dev/null",
                        f"--git-dir={git_directory}",
                        f"--work-tree={work_tree}",
                        "archive",
                        "--worktree-attributes",
                        "--format=tar.gz",
                        f"--output={archive}",
                        source["commit"],
                    ],
                    check=True,
                    env={**os.environ, "GIT_ATTR_NOSYSTEM": "1"},
                )
            tree = subprocess.run(
                ["git", "-C", str(checkout), "rev-parse", f"{source['commit']}^{{tree}}"],
                check=True,
                text=True,
                capture_output=True,
            ).stdout.strip()
            gitlink_lines = subprocess.run(
                ["git", "-C", str(checkout), "ls-tree", "-r", source["commit"]],
                check=True,
                text=True,
                capture_output=True,
            ).stdout.splitlines()
            gitlinks = []
            for line in gitlink_lines:
                metadata, separator, path = line.partition("\t")
                mode, kind, commit = metadata.split()
                if mode == "160000" and kind == "commit" and separator:
                    gitlinks.append({"path": path, "commit": commit})
            commit_object = subprocess.run(
                ["git", "-C", str(checkout), "cat-file", "commit", source["commit"]],
                check=True,
                capture_output=True,
            ).stdout.hex()
            exported_tree = archive_tree(archive, gitlinks)
            if exported_tree != tree:
                raise SystemExit(f"{archive} content tree does not match {source['commit']}")
            sources.append(
                {
                    "repo": source["repo"],
                    "commit": source["commit"],
                    "commitObject": commit_object,
                    "tree": tree,
                    "archiveTree": exported_tree,
                    "archive": archive.as_posix(),
                    "sha256": sha256(archive),
                    "gitlinks": gitlinks,
                }
            )

    manifest = {
        "schemaVersion": 1,
        "freezeId": selection["freezeId"],
        "selection": {
            "file": args.selection.as_posix(),
            "sha256": sha256(args.selection),
        },
        "sources": sources,
    }
    args.output.write_text(json.dumps(manifest, indent=2) + "\n")


if __name__ == "__main__":
    main()
