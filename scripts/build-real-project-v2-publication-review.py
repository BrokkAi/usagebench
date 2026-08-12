#!/usr/bin/env python3
"""Build retained run manifests and publication review evidence for real-project-v2."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Any


FREEZE_ID = "real-project-v2"
RUN_IDS = [f"{FREEZE_ID}-agent-panel-milestone-{number}-v3" for number in range(1, 7)]
PROVIDERS = [
    ("openai", "openai-gpt-5.6-sol", "openai-gpt-5.6-sol.json"),
    ("anthropic", "anthropic-claude-fable-5", "anthropic-claude-fable-5.json"),
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


def declaration_index(declarations: dict[str, Any]) -> dict[str, dict[str, Any]]:
    result = {}
    for profile in declarations["profiles"]:
        for repository in profile["selected"]:
            for selected in repository["declarationDraw"]["selected"]:
                result[selected["caseId"]] = {
                    "repository": repository["fullName"],
                    "commit": repository["source"]["commit"],
                    "language": profile["language"],
                    "selectionRank": selected["rank"],
                    "location": {"uri": selected["uri"], "range": selected["range"]},
                    "kind": selected["kind"],
                    "displayName": selected["displayName"],
                }
    return result


def symbol(declaration: dict[str, Any], location: dict[str, Any]) -> dict[str, Any]:
    return {
        "location": location,
        "kind": declaration["kind"],
        "displayName": declaration["displayName"],
    }


def normalized_record(raw: dict[str, Any], declaration: dict[str, Any]) -> dict[str, Any]:
    if raw["declaration"] != declaration["location"]:
        raise ValueError(f"declaration drift for {raw['caseId']}")
    required = [
        symbol(declaration, item["location"])
        for item in raw["locations"]
        if item["classification"] == "required"
    ]
    required.sort(
        key=lambda item: json.dumps(
            item["location"], sort_keys=True, separators=(",", ":")
        )
    )
    definition = raw.get("definitionUsage")
    return {
        "caseId": raw["caseId"],
        "decision": {"accept": "accepted", "replace": "replace", "abstain": "abstain"}[
            raw["decision"]
        ],
        "declaration": declaration,
        "expectedUsages": required,
        "definitionUsage": symbol(declaration, definition) if definition is not None else None,
    }


def build(repo_root: Path) -> dict[Path, bytes]:
    evaluation_root = repo_root / "benchmarks/evaluation" / FREEZE_ID
    runs_root = repo_root / "benchmarks/review-protocol/runs"
    declarations = load_json(evaluation_root / "declarations.json")
    declarations_by_case = declaration_index(declarations)
    protocol_commit = declarations["protocolCommit"]
    sources = load_json(evaluation_root / "sources.json")["sources"]
    outputs: dict[Path, bytes] = {}
    sessions_by_provider = {provider: {} for provider, _, _ in PROVIDERS}
    records_by_provider = {provider: {} for provider, _, _ in PROVIDERS}
    canonical_records: dict[str, dict[str, Any]] = {}
    human_links = []
    attestations = []
    adjudicator = ""
    latest_adjudication = ""

    for run_id in RUN_IDS:
        run_root = runs_root / run_id
        packet_manifest = load_json(run_root / "packet-manifest.json")
        comparison_relative = f"benchmarks/review-protocol/runs/{run_id}/comparison.json"
        human_relative = f"benchmarks/review-protocol/runs/{run_id}/human-adjudication.json"
        comparison = artifact_link(repo_root, comparison_relative)
        human_link = artifact_link(repo_root, human_relative)
        human = load_json(repo_root / human_relative)["adjudicator"]
        if adjudicator and human["identity"] != adjudicator:
            raise ValueError("milestones use different human adjudicators")
        adjudicator = human["identity"]
        attestations.append(human["attestation"])
        latest_adjudication = max(latest_adjudication, human["executedAt"])
        human_links.append(human_relative)
        case_ids = packet_manifest["milestone"]["caseIds"]
        packet_by_case = {packet["caseId"]: packet for packet in packet_manifest["packets"]}
        retained_sessions = []

        for case_id in case_ids:
            if case_id not in declarations_by_case:
                raise ValueError(f"packet case is absent from declaration lock: {case_id}")
            for provider, _, filename in PROVIDERS:
                raw_relative = f"benchmarks/review-protocol/runs/{run_id}/raw/{case_id}/{filename}"
                raw_link = artifact_link(repo_root, raw_relative)
                response = load_json(repo_root / raw_relative)
                if len(response["records"]) != 1 or response["records"][0]["caseId"] != case_id:
                    raise ValueError(f"raw response coverage mismatch for {case_id}")
                reviewer = response["reviewer"]
                if reviewer["provider"].strip().lower() != provider:
                    raise ValueError(f"raw response provider mismatch for {case_id}")
                retained_sessions.append(
                    {
                        "caseId": case_id,
                        "provider": reviewer["provider"],
                        "model": reviewer["model"],
                        "executionId": reviewer["executionId"],
                        "executedAt": reviewer["executedAt"],
                        "rawResponse": raw_link,
                    }
                )
                normalized = normalized_record(
                    response["records"][0], declarations_by_case[case_id]
                )
                records_by_provider[provider][case_id] = normalized
                sessions_by_provider[provider][case_id] = {
                    "caseId": case_id,
                    "participant": {
                        "kind": "agent",
                        "provider": reviewer["provider"],
                        "model": reviewer["model"],
                        "executionId": reviewer["executionId"],
                        "executedAt": reviewer["executedAt"],
                    },
                    "packet": {
                        "file": packet_by_case[case_id]["file"],
                        "sha256": packet_by_case[case_id]["sha256"],
                    },
                    "prompt": packet_manifest["prompt"],
                    "responseSchema": packet_manifest["responseSchema"],
                    "rawResponse": raw_link,
                }
            # Every human resolution selected the OpenAI required-location contract.
            canonical = dict(records_by_provider["openai"][case_id])
            canonical["decision"] = "accepted"
            canonical_records[case_id] = canonical

        executed = [session["executedAt"] for session in retained_sessions]
        run = {
            "schemaVersion": 1,
            "milestoneId": run_id,
            "startedAt": min(executed),
            "reviewCompletedAt": max(executed),
            "adjudicatedAt": human["executedAt"],
            "status": "completed",
            "protocol": packet_manifest["protocol"],
            "methodology": packet_manifest["methodology"],
            "prompt": packet_manifest["prompt"],
            "responseSchema": packet_manifest["responseSchema"],
            "comparison": comparison,
            "adjudication": human_link,
            "milestone": {
                "number": packet_manifest["milestone"]["number"],
                "partitionProgress": f"{packet_manifest['milestone']['number'] * 6}/36",
                "caseIds": case_ids,
            },
            "execution": {
                "sessionPolicy": "fresh_session_per_provider_per_case",
                "packetPolicy": "one_case_manifest_plus_complete_pinned_project",
                "blinding": [
                    "authored_expectations",
                    "prior_strata",
                    "analyzer_identity_and_output",
                    "other_reviewer_responses",
                    "prior_adjudication",
                    "git_history",
                ],
            },
            "packets": packet_manifest["packets"],
            "sources": packet_manifest["sources"],
            "sessions": retained_sessions,
        }
        outputs[run_root / "run.json"] = encode_json(run)

    selected_case_ids = [
        case_id
        for run_id in RUN_IDS
        for case_id in load_json(runs_root / run_id / "packet-manifest.json")["milestone"]["caseIds"]
    ]
    if set(selected_case_ids) != set(declarations_by_case) or len(selected_case_ids) != 36:
        raise ValueError("six milestones do not cover the 36 declaration slots exactly once")

    reviewer_links = []
    for provider, reviewer_id, _ in PROVIDERS:
        evidence = {
            "schemaVersion": 1,
            "reviewer": reviewer_id,
            "referencePolicy": "bindings_optional",
            "selectionAlgorithm": "real-project-v2-source-syntax-v1",
            "records": [records_by_provider[provider][case_id] for case_id in selected_case_ids],
        }
        relative = f"benchmarks/evaluation/{FREEZE_ID}/reviews/{reviewer_id}.json"
        content = encode_json(evidence)
        outputs[repo_root / relative] = content
        reviewer_links.append(
            {
                "id": reviewer_id,
                "file": relative,
                "sha256": sha256_bytes(content),
                "sessions": [sessions_by_provider[provider][case_id] for case_id in selected_case_ids],
            }
        )

    adjudication = {
        "schemaVersion": 1,
        "freezeId": FREEZE_ID,
        "protocolCommit": protocol_commit,
        "referencePolicy": "bindings_optional",
        "reviewBasis": "human_adjudicated_agent_panel",
        "sourceArtifacts": human_links,
        "summary": {
            "selectedCases": len(selected_case_ids),
            "accepted": len(selected_case_ids),
            "replaced": 0,
            "abstained": 0,
        },
        "records": [canonical_records[case_id] for case_id in selected_case_ids],
    }
    adjudication_relative = f"benchmarks/evaluation/{FREEZE_ID}/reviews/adjudication.json"
    adjudication_bytes = encode_json(adjudication)
    outputs[repo_root / adjudication_relative] = adjudication_bytes
    review = {
        "schemaVersion": 2,
        "freezeId": FREEZE_ID,
        "selection": artifact_link(repo_root, f"benchmarks/evaluation/{FREEZE_ID}/selection.json"),
        "reviewTier": "human_adjudicated_agent_panel",
        "reviewProtocol": artifact_link(
            repo_root, "benchmarks/review-protocol/blinded-agent-review-v3.json"
        ),
        "reviewers": reviewer_links,
        "adjudication": {
            "id": f"{FREEZE_ID}-human-adjudication",
            "file": adjudication_relative,
            "sha256": sha256_bytes(adjudication_bytes),
            "participant": {
                "kind": "human",
                "identity": adjudicator,
                "executedAt": latest_adjudication,
                "attestation": " ".join(attestations),
            },
        },
    }
    outputs[evaluation_root / "review.json"] = encode_json(review)
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
            names = ", ".join(str(path.relative_to(repo_root)) for path in stale)
            raise SystemExit(f"publication review artifacts are stale: {names}")
        return
    for path, content in outputs.items():
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content)


if __name__ == "__main__":
    main()
