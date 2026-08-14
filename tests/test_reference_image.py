import json
import os
import pathlib
import re
import shutil
import subprocess
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1]


MOCK_DOCKER = r'''#!/usr/bin/env python3
import json, os, pathlib, re, sys

state_path = pathlib.Path(os.environ["MOCK_DOCKER_STATE"])
state = json.loads(state_path.read_text()) if state_path.exists() else {"builds": 0, "images": {}}
args = sys.argv[1:]

def save():
    state_path.write_text(json.dumps(state))

def image_for(reference):
    key = state["images"].get(reference, reference)
    return state.get("image") if key == state.get("image", {}).get("id") else None

if args[:3] == ["buildx", "imagetools", "inspect"]:
    if state.get("registryDigest"):
        print(state["registryDigest"])
        raise SystemExit(0)
    raise SystemExit(1)
if args[:2] == ["buildx", "build"]:
    build_args, tags, metadata = {}, [], None
    index = 2
    while index < len(args):
        if args[index] == "--build-arg":
            key, value = args[index + 1].split("=", 1)
            build_args[key] = value
            index += 2
        elif args[index] == "--tag":
            tags.append(args[index + 1]); index += 2
        elif args[index] == "--metadata-file":
            metadata = pathlib.Path(args[index + 1]); index += 2
        else:
            index += 1
    runner = "bifrost" if any(tag.endswith("-bifrost") for tag in tags) else "gopls"
    image_id = "sha256:" + "b" * 64
    labels = {
        "ai.brokk.usagebench.release": build_args["USAGEBENCH_RELEASE"],
        "org.opencontainers.image.revision": build_args["USAGEBENCH_REVISION"],
        "ai.brokk.usagebench.runner.id": runner,
        "ai.brokk.usagebench.environment.version": build_args["ENVIRONMENT_VERSION"],
        "ai.brokk.usagebench.environment.definition-digest": build_args["DEFINITION_DIGEST"],
        "ai.brokk.usagebench.environment.identity-digest": build_args["IDENTITY_DIGEST"],
        "ai.brokk.usagebench.analyzer.identity": build_args["ANALYZER_IDENTITY"],
        "ai.brokk.usagebench.canonical-platform": "linux/amd64",
    }
    state["builds"] += 1
    state["image"] = {"id": image_id, "labels": labels}
    state["images"][image_id] = image_id
    for tag in tags:
        state["images"][tag] = image_id
    metadata.parent.mkdir(parents=True, exist_ok=True)
    metadata.write_text(json.dumps({"containerimage.digest": image_id}))
    save(); raise SystemExit(0)
if args[:2] == ["image", "inspect"]:
    reference = args[-1]
    image = image_for(reference)
    if image is None:
        raise SystemExit(1)
    if "--format" not in args:
        print("[]"); raise SystemExit(0)
    template = args[args.index("--format") + 1]
    if template == "{{.Id}}": print(image["id"])
    elif template == "{{.Os}}/{{.Architecture}}": print("linux/amd64")
    else:
        match = re.search(r'Labels "([^"]+)"', template)
        print(image["labels"].get(match.group(1), "") if match else "")
    raise SystemExit(0)
if args[0] == "tag":
    state["images"][args[2]] = state["images"].get(args[1], args[1]); save(); raise SystemExit(0)
if args[0] == "push":
    state["registryDigest"] = "sha256:" + "c" * 64
    state["registryImage"] = state["images"][args[1]]
    save(); raise SystemExit(0)
if args[0] == "pull":
    reference = args[-1]
    state["images"][reference] = state["registryImage"]
    save(); raise SystemExit(0)
raise SystemExit(f"unsupported mock docker command: {args}")
'''


class ReferenceImageTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.temp.name) / "release"
        for relative in (
            "scripts/reference-image.sh",
            "scripts/run-reference.sh",
            "containers/reference/v1/manifest.json",
            "containers/reference/v1/Dockerfile",
            "schema/reference-environment.schema.json",
            "adapters/candidates.json",
        ):
            destination = self.root / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / relative, destination)
        (self.root / "scripts/reference-image.sh").chmod(0o755)
        (self.root / ".usagebench-release.json").write_text(
            json.dumps({"releaseTag": "v0.3.0", "revision": "a" * 40})
        )
        self.bin = pathlib.Path(self.temp.name) / "bin"
        self.bin.mkdir()
        docker = self.bin / "docker"
        docker.write_text(MOCK_DOCKER)
        docker.chmod(0o755)
        self.state = pathlib.Path(self.temp.name) / "docker-state.json"
        self.env = {
            **os.environ,
            "PATH": f"{self.bin}:{os.environ['PATH']}",
            "MOCK_DOCKER_STATE": str(self.state),
        }

    def tearDown(self):
        self.temp.cleanup()

    def run_image(self, **extra_env):
        return subprocess.run(
            [self.root / "scripts/reference-image.sh", "bifrost", "v0.3.0", "a" * 40],
            env={**self.env, **extra_env},
            text=True,
            capture_output=True,
        )

    def test_publishes_restores_by_digest_and_forces_recipe_rebuild(self):
        cold = self.run_image(USAGEBENCH_REFERENCE_IMAGE_PUBLISH="1")
        self.assertEqual(cold.returncode, 0, cold.stderr)
        cold_metadata = json.loads(cold.stdout)
        self.assertEqual(cold_metadata["reuseStatus"], "built")
        self.assertEqual(cold_metadata["registryDigest"], "sha256:" + "c" * 64)
        self.assertIsNotNone(cold_metadata["imageConstructionMs"])

        shutil.rmtree(self.root / "target")
        local_warm = self.run_image()
        self.assertEqual(local_warm.returncode, 0, local_warm.stderr)
        self.assertEqual(json.loads(local_warm.stdout)["reuseStatus"], "local")
        self.assertIsNone(json.loads(local_warm.stdout)["imageConstructionMs"])
        self.assertEqual(json.loads(self.state.read_text())["builds"], 1)

        shutil.rmtree(self.root / "target")
        state = json.loads(self.state.read_text())
        state["images"] = {}
        self.state.write_text(json.dumps(state))
        warm = self.run_image()
        self.assertEqual(warm.returncode, 0, warm.stderr)
        warm_metadata = json.loads(warm.stdout)
        self.assertEqual(warm_metadata["reuseStatus"], "registry")
        self.assertIsNone(warm_metadata["imageConstructionMs"])
        self.assertEqual(warm_metadata["imageReference"], "ghcr.io/brokkai/usagebench-reference@sha256:" + "c" * 64)
        self.assertEqual(json.loads(self.state.read_text())["builds"], 1)

        forced = self.run_image(USAGEBENCH_REFERENCE_IMAGE_FORCE_REBUILD="1")
        self.assertEqual(forced.returncode, 0, forced.stderr)
        self.assertIn("forced canonical image rebuild", forced.stderr)
        self.assertEqual(json.loads(self.state.read_text())["builds"], 2)

    def test_rejects_force_and_publish_together(self):
        rejected = self.run_image(
            USAGEBENCH_REFERENCE_IMAGE_FORCE_REBUILD="1",
            USAGEBENCH_REFERENCE_IMAGE_PUBLISH="1",
        )
        self.assertNotEqual(rejected.returncode, 0)
        self.assertIn("must not publish", rejected.stderr)


if __name__ == "__main__":
    unittest.main()
