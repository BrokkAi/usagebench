#!/usr/bin/env python3
"""Build the publication-qualified real-project-v1 review manifest.

The retained v3 runs are the source of truth for per-case agent provenance.
This script normalizes their raw responses into the reviewer evidence shape and
assembles the schema-v2 review manifest consumed by evaluation validation.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Any


RUN_IDS = [
    "real-project-v1-agent-panel-pilot-v3",
    "real-project-v1-agent-panel-milestone-1-v3",
    "real-project-v1-agent-panel-milestone-2-v3",
    "real-project-v1-agent-panel-milestone-3-v3",
    "real-project-v1-agent-panel-milestone-4-v3",
    "real-project-v1-agent-panel-milestone-5-v3",
]
PROVIDERS = [
    ("openai", "gpt-5.6-sol", "openai-gpt-5.6-sol"),
    ("anthropic", "claude-fable-5", "anthropic-claude-fable-5"),
]
HISTORICAL_SOURCE_ARTIFACTS = [
    "selection.issue90.final-candidates.json",
    "real-project-v1-reviewer-a-complete.json",
    "real-project-v1-reviewer-b-complete.json",
]


def load_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text())


def encode_json(value: Any) -> bytes:
    return (json.dumps(value, indent=2, ensure_ascii=False) + "\n").encode()


def sha256_bytes(content: bytes) -> str:
    return hashlib.sha256(content).hexdigest()


def artifact_link(repo_root: Path, relative_path: str) -> dict[str, str]:
    content = (repo_root / relative_path).read_bytes()
    return {"file": relative_path, "sha256": sha256_bytes(content)}


def symbol_for_location(
    final_record: dict[str, Any], location: dict[str, Any]
) -> dict[str, Any]:
    for usage in final_record["expectedUsages"]:
        if usage["location"] == location:
            return usage
    declaration = final_record["declaration"]
    return {
        "location": location,
        "kind": declaration["kind"],
        "displayName": declaration["displayName"],
    }


def normalized_record(
    raw_record: dict[str, Any], final_record: dict[str, Any]
) -> dict[str, Any]:
    if raw_record["declaration"] != final_record["declaration"]["location"]:
        raise ValueError(f"declaration drift for {raw_record['caseId']}")
    decision = {"accept": "accepted", "replace": "replace", "abstain": "abstain"}[
        raw_record["decision"]
    ]
    required = [
        symbol_for_location(final_record, item["location"])
        for item in raw_record["locations"]
        if item["classification"] == "required"
    ]
    definition = raw_record.get("definitionUsage")
    return {
        "caseId": raw_record["caseId"],
        "decision": decision,
        "declaration": final_record["declaration"],
        "expectedUsages": required,
        "definitionUsage": (
            symbol_for_location(final_record, definition) if definition is not None else None
        ),
    }


def build(repo_root: Path) -> dict[Path, bytes]:
    review_root = repo_root / "benchmarks/evaluation/real-project-v1"
    runs_root = repo_root / "benchmarks/review-protocol/runs"
    adjudication_path = review_root / "reviews/adjudication.json"
    adjudication = load_json(adjudication_path)
    final_records = {record["caseId"]: record for record in adjudication["records"]}

    runs = []
    for run_id in RUN_IDS:
        run_path = runs_root / run_id / "run.json"
        run = load_json(run_path)
        completed = run.get("status") == "completed" or (
            bool(run.get("completedAt")) and bool(run.get("adjudication"))
        )
        if not completed:
            raise ValueError(f"review run is not complete: {run_id}")
        runs.append(run)

    selected_case_ids = list(final_records)
    sessions_by_provider: dict[str, dict[str, dict[str, Any]]] = {
        provider: {} for provider, _, _ in PROVIDERS
    }
    records_by_provider: dict[str, dict[str, dict[str, Any]]] = {
        provider: {} for provider, _, _ in PROVIDERS
    }
    human_artifacts: list[str] = []
    human_attestations: list[str] = []
    latest_adjudication = ""
    adjudicator_identity = ""

    for run in runs:
        packet_by_case = {packet["caseId"]: packet for packet in run["packets"]}
        human_link = run["adjudication"]
        human_artifacts.append(human_link["file"])
        human = load_json(repo_root / human_link["file"])["adjudicator"]
        if adjudicator_identity and human["identity"] != adjudicator_identity:
            raise ValueError("agent-panel runs use different human adjudicators")
        adjudicator_identity = human["identity"]
        human_attestations.append(human["attestation"])
        latest_adjudication = max(latest_adjudication, human["executedAt"])

        for retained in run["sessions"]:
            case_id = retained["caseId"]
            provider = retained["provider"]
            if case_id in sessions_by_provider[provider]:
                raise ValueError(f"duplicate {provider} session for {case_id}")
            raw = load_json(repo_root / retained["rawResponse"]["file"])
            if len(raw["records"]) != 1 or raw["records"][0]["caseId"] != case_id:
                raise ValueError(f"raw response coverage mismatch for {case_id}")
            reviewer = raw["reviewer"]
            for field in ["provider", "model", "executionId"]:
                if reviewer[field] != retained[field]:
                    raise ValueError(f"retained session {field} mismatch for {case_id}")
            if retained.get("executedAt") not in (None, reviewer["executedAt"]):
                raise ValueError(f"retained session executedAt mismatch for {case_id}")
            sessions_by_provider[provider][case_id] = {
                "caseId": case_id,
                "participant": {
                    "kind": "agent",
                    "provider": provider,
                    "model": retained["model"],
                    "executionId": retained["executionId"],
                    "executedAt": reviewer["executedAt"],
                },
                "packet": {
                    "file": packet_by_case[case_id]["file"],
                    "sha256": packet_by_case[case_id]["sha256"],
                },
                "prompt": run["prompt"],
                "responseSchema": run["responseSchema"],
                "rawResponse": retained["rawResponse"],
            }
            records_by_provider[provider][case_id] = normalized_record(
                raw["records"][0], final_records[case_id]
            )

    expected = set(selected_case_ids)
    outputs: dict[Path, bytes] = {}
    reviewer_links: list[dict[str, Any]] = []
    for provider, model, reviewer_id in PROVIDERS:
        if set(sessions_by_provider[provider]) != expected:
            raise ValueError(f"{provider} sessions do not cover the selected cases")
        evidence = {
            "schemaVersion": 1,
            "reviewer": reviewer_id,
            "referencePolicy": "bindings_optional",
            "selectionAlgorithm": "real-project-v1-source-syntax-v1",
            "records": [records_by_provider[provider][case_id] for case_id in selected_case_ids],
        }
        evidence_relative = (
            f"benchmarks/evaluation/real-project-v1/reviews/{reviewer_id}.json"
        )
        evidence_bytes = encode_json(evidence)
        outputs[repo_root / evidence_relative] = evidence_bytes
        reviewer_links.append(
            {
                "id": reviewer_id,
                "file": evidence_relative,
                "sha256": sha256_bytes(evidence_bytes),
                "sessions": [
                    sessions_by_provider[provider][case_id]
                    for case_id in selected_case_ids
                ],
            }
        )
        models = {
            session["participant"]["model"]
            for session in sessions_by_provider[provider].values()
        }
        if models != {model}:
            raise ValueError(f"{provider} sessions do not use the expected model cohort")

    adjudication["sourceArtifacts"] = HISTORICAL_SOURCE_ARTIFACTS + human_artifacts
    adjudication_bytes = encode_json(adjudication)
    outputs[adjudication_path] = adjudication_bytes

    review_manifest = {
        "schemaVersion": 2,
        "freezeId": "real-project-v1",
        "selection": artifact_link(
            repo_root, "benchmarks/evaluation/real-project-v1/selection.json"
        ),
        "reviewTier": "human_adjudicated_agent_panel",
        "reviewProtocol": artifact_link(
            repo_root, "benchmarks/review-protocol/blinded-agent-review-v3.json"
        ),
        "reviewers": reviewer_links,
        "adjudication": {
            "id": "real-project-v1-human-adjudication",
            "file": "benchmarks/evaluation/real-project-v1/reviews/adjudication.json",
            "sha256": sha256_bytes(adjudication_bytes),
            "participant": {
                "kind": "human",
                "identity": adjudicator_identity,
                "executedAt": latest_adjudication,
                "attestation": " ".join(human_attestations),
            },
        },
    }
    outputs[review_root / "review.json"] = encode_json(review_manifest)
    return outputs


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--check", action="store_true", help="fail if checked-in artifacts differ"
    )
    args = parser.parse_args()
    repo_root = Path(__file__).resolve().parent.parent
    outputs = build(repo_root)
    stale = [path for path, content in outputs.items() if not path.is_file() or path.read_bytes() != content]
    if args.check:
        if stale:
            names = ", ".join(str(path.relative_to(repo_root)) for path in stale)
            raise SystemExit(f"publication review artifacts are stale: {names}")
        return
    for path, content in outputs.items():
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content)


if __name__ == "__main__":
    main()
