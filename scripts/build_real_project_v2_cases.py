#!/usr/bin/env python3
"""Build the frozen real-project-v2 benchmark case documents.

The source-only OpenAI response is the deterministic contract base.  Human
classification resolutions are then applied by exact source location.  The
script deliberately does not inspect or consume analyzer output.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


RUN_IDS = [
    f"real-project-v2-agent-panel-milestone-{number}-v3" for number in range(1, 7)
]


def load_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text())


def encode_yaml_compatible(value: Any) -> bytes:
    # JSON is a YAML 1.2 subset.  Using it here avoids an undeclared PyYAML
    # dependency while keeping generated documents deterministic.
    return (json.dumps(value, indent=2, ensure_ascii=False) + "\n").encode()


def location_key(location: dict[str, Any]) -> str:
    return json.dumps(location, sort_keys=True, separators=(",", ":"))


def declaration_metadata(declarations: dict[str, Any]) -> dict[str, dict[str, Any]]:
    wanted = {
        case_id
        for document in declarations["documents"]
        for case_id in document["caseIds"]
    }
    found: dict[str, dict[str, Any]] = {}

    def visit(value: Any) -> None:
        if isinstance(value, dict):
            case_id = value.get("caseId")
            if case_id in wanted and {"uri", "range", "kind", "displayName"} <= value.keys():
                found[case_id] = value
            for child in value.values():
                visit(child)
        elif isinstance(value, list):
            for child in value:
                visit(child)

    visit(declarations)
    missing = wanted - found.keys()
    if missing:
        raise ValueError(f"declaration metadata missing for: {sorted(missing)}")
    return found


def build(repo_root: Path) -> dict[Path, bytes]:
    evaluation = repo_root / "benchmarks/evaluation/real-project-v2"
    runs_root = repo_root / "benchmarks/review-protocol/runs"
    declarations = load_json(evaluation / "declarations.json")
    metadata = declaration_metadata(declarations)

    raw_by_case: dict[str, dict[str, Any]] = {}
    adjudication_by_case: dict[str, dict[str, Any]] = {}
    attestations: list[str] = []
    for run_id in RUN_IDS:
        run_root = runs_root / run_id
        human = load_json(run_root / "human-adjudication.json")
        attestations.append(human["adjudicator"]["attestation"])
        for record in human["records"]:
            case_id = record["caseId"]
            if case_id in adjudication_by_case:
                raise ValueError(f"duplicate adjudication for {case_id}")
            adjudication_by_case[case_id] = record
            raw_path = run_root / "raw" / case_id / "openai-gpt-5.6-sol.json"
            raw = load_json(raw_path)
            records = raw["records"]
            if len(records) != 1 or records[0]["caseId"] != case_id:
                raise ValueError(f"unexpected OpenAI response coverage for {case_id}")
            raw_by_case[case_id] = records[0]

    expected = set(metadata)
    if set(raw_by_case) != expected or set(adjudication_by_case) != expected:
        raise ValueError("the six adjudicated milestones do not cover all declarations")

    outputs: dict[Path, bytes] = {}
    for document in declarations["documents"]:
        cases = []
        for case_id in document["caseIds"]:
            raw = raw_by_case[case_id]
            meta = metadata[case_id]
            kind = "class" if meta["kind"] == "enum" else meta["kind"]
            declaration = {
                "location": {"uri": meta["uri"], "range": meta["range"]},
                "kind": kind,
                "displayName": meta["displayName"],
            }
            if raw["declaration"] != declaration["location"]:
                raise ValueError(f"review declaration drift for {case_id}")

            classifications = {
                location_key(item["location"]): (item["location"], item["classification"])
                for item in raw["locations"]
            }
            resolution = adjudication_by_case[case_id].get("classificationResolution")
            if resolution:
                for location in resolution["locations"]:
                    key = location_key(location)
                    if key not in classifications:
                        classifications[key] = (location, resolution["adjudicated"])
                    else:
                        classifications[key] = (location, resolution["adjudicated"])

            required = [
                {
                    "location": location,
                    "kind": kind,
                    "displayName": meta["displayName"],
                }
                for location, classification in classifications.values()
                if classification == "required"
            ]
            required.sort(key=lambda item: location_key(item["location"]))
            required_keys = {location_key(item["location"]) for item in required}
            definition = raw.get("definitionUsage")
            if definition is not None and location_key(definition) not in required_keys:
                definition = required[0]["location"] if required else None

            case: dict[str, Any] = {
                "id": case_id,
                "declaration": declaration,
                "expectedUsages": required,
            }
            if definition is not None:
                usage = next(
                    item for item in required if item["location"] == definition
                )
                case["usageLookups"] = [
                    {
                        "operation": "definition",
                        "usage": usage,
                        "expectedDeclaration": declaration,
                    }
                ]
            case["verification"] = {
                "method": "manual_inspection",
                "notes": (
                    "Independently reviewed from the pinned source archive by fresh "
                    "OpenAI and Anthropic agent sessions, then source-only human "
                    "adjudicated before analyzer execution. Exact required-location "
                    "classifications include the recorded human resolutions."
                ),
            }
            cases.append(case)

        case_document = {
            "schemaVersion": 2,
            "corpus": {
                "partition": "evaluation",
                "selection": "pre_registered",
                "freezeId": "real-project-v2",
                "selectionManifest": "benchmarks/evaluation/real-project-v2/selection.json",
                "reviewManifest": "benchmarks/evaluation/real-project-v2/review.json",
                "sourceLock": "benchmarks/evaluation/real-project-v2/sources.json",
            },
            "groundTruth": {
                "status": "human_adjudicated_agent_panel",
                "reviewers": ["openai-gpt-5.6-sol", "anthropic-claude-fable-5"],
            },
            "referencePolicy": "bindings_optional",
            "positionEncoding": "utf-16",
            "source": {
                "kind": "git",
                "repo": document["source"]["repo"],
                "commit": document["source"]["commit"],
            },
            "language": document["language"],
            "cases": cases,
        }
        outputs[repo_root / document["caseFile"]] = encode_yaml_compatible(case_document)
    return outputs


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    repo_root = Path(__file__).resolve().parent.parent
    outputs = build(repo_root)
    stale = [path for path, content in outputs.items() if not path.is_file() or path.read_bytes() != content]
    if args.check:
        if stale:
            paths = ", ".join(str(path.relative_to(repo_root)) for path in stale)
            raise SystemExit(f"real-project-v2 case documents are stale: {paths}")
        return
    for path, content in outputs.items():
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content)


if __name__ == "__main__":
    main()
