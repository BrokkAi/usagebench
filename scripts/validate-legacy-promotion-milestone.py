#!/usr/bin/env python3
"""Validate one hash-bound legacy-promotion agent-review milestone."""

from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path


LANGUAGES = {
    "cpp", "csharp", "go", "java", "javascript", "php", "python", "ruby",
    "rust", "scala", "typescript",
}
PROVIDERS = {"openai": "gpt-5.6-sol", "anthropic": "claude-fable-5"}
LOCATION_CLASSES = {"required", "optional", "unproven", "excluded"}


def fail(message: str) -> None:
    raise SystemExit(message)


def load_json(path: Path) -> dict:
    try:
        value = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as error:
        fail(f"cannot read JSON {path}: {error}")
    if not isinstance(value, dict):
        fail(f"expected JSON object: {path}")
    return value


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def linked_path(root: Path, link: dict, label: str) -> Path:
    if set(link) != {"file", "sha256"}:
        fail(f"{label} must contain only file and sha256")
    relative = Path(link["file"])
    if relative.is_absolute() or ".." in relative.parts:
        fail(f"unsafe {label} path: {relative}")
    path = (root / relative).resolve()
    if root not in path.parents:
        fail(f"{label} resolves outside repository: {relative}")
    if sha256(path) != link["sha256"]:
        fail(f"{label} hash mismatch: {relative}")
    return path


def location_key(location: dict | None) -> tuple | None:
    if location is None:
        return None
    if set(location) != {"uri", "range"} or not location["uri"].startswith("benchmark://source/"):
        fail("invalid review location")
    span = location["range"]
    if set(span) != {"start", "end"}:
        fail("invalid review range")
    values = []
    for endpoint in ("start", "end"):
        position = span[endpoint]
        if set(position) != {"line", "character"}:
            fail("invalid review position")
        if not all(isinstance(position[key], int) and position[key] >= 0 for key in position):
            fail("invalid review position values")
        values.extend((position["line"], position["character"]))
    return (location["uri"], *values)


def validate_response(path: Path, provider: str, model: str, execution_id: str, case_id: str, profile: str) -> dict:
    value = load_json(path)
    if set(value) != {"schemaVersion", "reviewer", "records"} or value["schemaVersion"] != 1:
        fail(f"invalid response envelope: {path}")
    reviewer = value["reviewer"]
    if set(reviewer) != {"provider", "model", "executionId", "executedAt"}:
        fail(f"invalid reviewer metadata: {path}")
    if (reviewer["provider"], reviewer["model"], reviewer["executionId"]) != (provider, model, execution_id):
        fail(f"reviewer provenance mismatch: {path}")
    if not re.fullmatch(r"\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ", reviewer["executedAt"]):
        fail(f"invalid reviewer timestamp: {path}")
    if not isinstance(value["records"], list) or len(value["records"]) != 1:
        fail(f"response must contain exactly one record: {path}")
    record = value["records"][0]
    if record.get("caseId") != case_id:
        fail(f"invalid response record: {path}")
    if profile == "navigation_v1":
        required = {"caseId", "queries", "inspectedPaths", "rationale"}
        if set(record) != required or not record["queries"]:
            fail(f"invalid navigation response record: {path}")
        for query in record["queries"]:
            expected = {"queryId", "operation", "query", "decision", "confidence", "targets", "ambiguities", "rationale"}
            if not expected.issubset(query) or set(query) - (expected | {"resolvedIdentity"}):
                fail(f"invalid navigation query: {path}")
            if query["operation"] not in {"declaration", "definition", "type_definition"}:
                fail(f"invalid navigation operation: {path}")
            location_key(query["query"])
            for target in query["targets"]:
                location_key(target)
            if query["decision"] not in {"accept", "replace", "abstain"} or query["confidence"] not in {"high", "medium", "low"}:
                fail(f"invalid navigation decision/confidence: {path}")
        return record
    required = {"caseId", "decision", "confidence", "declaration", "locations", "definitionUsage", "ambiguities", "inspectedPaths", "rationale"}
    if set(record) != required:
        fail(f"invalid response record: {path}")
    if record["decision"] not in {"accept", "replace", "abstain"} or record["confidence"] not in {"high", "medium", "low"}:
        fail(f"invalid response decision/confidence: {path}")
    location_key(record["declaration"])
    location_key(record["definitionUsage"])
    for reviewed in record["locations"]:
        if set(reviewed) != {"location", "classification", "rationale"} or reviewed["classification"] not in LOCATION_CLASSES:
            fail(f"invalid reviewed location: {path}")
        location_key(reviewed["location"])
    if not isinstance(record["ambiguities"], list) or not isinstance(record["inspectedPaths"], list) or not record["inspectedPaths"]:
        fail(f"invalid review audit fields: {path}")
    return record


def required_locations(record: dict) -> list[tuple]:
    return sorted(location_key(item["location"]) for item in record["locations"] if item["classification"] == "required")


def navigation_fields(record: dict) -> dict:
    queries = record["queries"]
    return {
        "identity": [(q["queryId"], q["operation"], location_key(q["query"])) for q in queries],
        "decisions": [q["decision"] for q in queries],
        "targets": [sorted(location_key(target) for target in q["targets"]) for q in queries],
        "high": all(q["confidence"] == "high" for q in queries),
        "clear": all(not q["ambiguities"] for q in queries),
    }


def tree_sha256(root: Path) -> str:
    digest = hashlib.sha256()
    for path in sorted(root.rglob("*"), key=lambda item: item.relative_to(root).as_posix()):
        relative = path.relative_to(root).as_posix()
        if path.is_symlink():
            digest.update(f"link\0{relative}\0{path.readlink()}\0".encode())
        elif path.is_file():
            digest.update(f"file\0{relative}\0{sha256(path)}\0".encode())
        elif path.is_dir():
            digest.update(f"dir\0{relative}\0".encode())
    return digest.hexdigest()


def main() -> None:
    if len(sys.argv) != 2:
        fail("usage: validate-legacy-promotion-milestone.py <run.json>")
    run_path = Path(sys.argv[1]).resolve()
    root = Path(__file__).resolve().parents[1]
    run = load_json(run_path)
    if run.get("schemaVersion") != 1 or run.get("status") != "human_adjudicated":
        fail("milestone must be schema-v1 and human_adjudicated")
    if (run.get("selectionProvenance"), run.get("selectionBasis"), run.get("analyzerOutcomeUse")) != ("retrospectively_selected", "source_only", "forbidden"):
        fail("milestone violates retrospective source-only evidence boundary")
    for index, link in enumerate(run["protocolArtifacts"]):
        linked_path(root, link, f"protocol artifact {index}")
    cohort_link = next((link for link in run["protocolArtifacts"] if link["file"].endswith("/cohort.json")), None)
    if not cohort_link:
        fail("milestone does not bind the frozen cohort")
    cohort = load_json(linked_path(root, cohort_link, "cohort"))
    comparison = load_json(linked_path(root, run["comparison"], "comparison"))
    adjudication = load_json(linked_path(root, run["adjudication"], "adjudication"))
    if comparison.get("status") != "pending_human_adjudication" or adjudication.get("milestoneId") != run.get("milestoneId"):
        fail("comparison/adjudication lifecycle mismatch")
    cases = run.get("cases")
    if not isinstance(cases, list) or len(cases) != 11 or {case["language"] for case in cases} != LANGUAGES:
        fail("milestone must cover exactly one case in each legacy language")
    milestone_match = re.fullmatch(r"legacy-promotion-v1-milestone-(\d+)", run.get("milestoneId", ""))
    if not milestone_match:
        fail("invalid milestone identity")
    selection_order = int(milestone_match.group(1))
    if any(case.get("selectionOrder") != selection_order for case in cases):
        fail(f"milestone may contain only selectionOrder {selection_order} cases")
    comparison_by_id = {case["caseId"]: case for case in comparison["cases"]}
    adjudication_by_id = {case["caseId"]: case for case in adjudication["cases"]}
    if len(comparison_by_id) != 11 or len(adjudication_by_id) != 11:
        fail("comparison and adjudication must exactly cover 11 unique cases")
    seen_executions: set[tuple[str, str]] = set()
    for case in cases:
        case_id = case["caseId"]
        inventory = next((row for row in cohort["inventory"] if row["caseId"] == case_id), None)
        if not inventory or inventory["language"] != case["language"] or inventory["decision"] != "balanced_core" or inventory["selectionOrder"] != selection_order:
            fail(f"case is not the frozen milestone selection: {case_id}")
        packet_path = linked_path(root, case["packet"], f"packet {case_id}")
        packet = load_json(packet_path)
        profile = case.get("reviewProfile", "references_v3")
        allowed_packet_keys = ({"schemaVersion", "cohortId", "milestoneId", "caseId", "language", "referencePolicy", "positionEncoding", "source", "declaration", "displayName"}
                               if profile == "references_v3" else
                               {"schemaVersion", "cohortId", "milestoneId", "caseId", "language", "referencePolicy", "positionEncoding", "source", "reviewProfile", "queries"})
        if set(packet) != allowed_packet_keys or packet["caseId"] != case_id or packet["language"] != case["language"]:
            fail(f"packet shape/identity mismatch: {case_id}")
        source_document = root / inventory["document"]
        fixture_match = re.search(r"(?m)^  path: (fixtures/[^\n]+)$", source_document.read_text())
        if not fixture_match:
            fail(f"cannot resolve fixture source for {case_id}")
        actual_tree = tree_sha256(root / fixture_match.group(1))
        if actual_tree != packet["source"]["treeSha256"] or actual_tree != case["sourceTreeSha256"]:
            fail(f"source tree drift: {case_id}")
        records = {}
        if len(case["sessions"]) != 2:
            fail(f"case must have two sessions: {case_id}")
        for session in case["sessions"]:
            provider = session["provider"]
            if provider not in PROVIDERS or session["model"] != PROVIDERS[provider]:
                fail(f"invalid provider/model cohort: {case_id}")
            provenance = (provider, session["executionId"])
            if provenance in seen_executions:
                fail(f"reused provider/execution ID: {provenance}")
            seen_executions.add(provenance)
            response_field = "rawResponse" if provider == "openai" else "normalizedResponse"
            response_path = linked_path(root, session[response_field], f"review response {case_id}/{provider}")
            records[provider] = validate_response(response_path, provider, PROVIDERS[provider], session["executionId"], case_id, profile)
            if provider == "anthropic":
                if "rawResponse" in session or "providerEnvelope" in session:
                    fail(f"Anthropic session must distinguish rawProviderEnvelope from normalizedResponse: {case_id}")
                envelope = load_json(linked_path(root, session["rawProviderEnvelope"], f"raw provider envelope {case_id}"))
                if envelope.get("is_error") or envelope.get("session_id") != session["executionId"] or envelope.get("modelUsage", {}).get("claude-fable-5", {}).get("canonicalModel") != "claude-fable-5":
                    fail(f"Anthropic provider envelope mismatch: {case_id}")
        if profile == "navigation_v1":
            openai_navigation = navigation_fields(records["openai"])
            anthropic_navigation = navigation_fields(records["anthropic"])
            expected_fields = {
                "queryIdentity": openai_navigation["identity"] == anthropic_navigation["identity"],
                "decision": openai_navigation["decisions"] == anthropic_navigation["decisions"],
                "targets": openai_navigation["targets"] == anthropic_navigation["targets"],
                "highConfidence": openai_navigation["high"] and anthropic_navigation["high"],
                "noRequiredAmbiguity": openai_navigation["clear"] and anthropic_navigation["clear"],
            }
        else:
            expected_fields = {
                "decision": records["openai"]["decision"] == records["anthropic"]["decision"],
                "declaration": location_key(records["openai"]["declaration"]) == location_key(records["anthropic"]["declaration"]),
                "requiredLocations": required_locations(records["openai"]) == required_locations(records["anthropic"]),
                "definitionUsage": location_key(records["openai"]["definitionUsage"]) == location_key(records["anthropic"]["definitionUsage"]),
                "highConfidence": records["openai"]["confidence"] == records["anthropic"]["confidence"] == "high",
                "noRequiredAmbiguity": not records["openai"]["ambiguities"] and not records["anthropic"]["ambiguities"],
            }
        compared = comparison_by_id.get(case_id)
        if not compared or compared["fields"] != expected_fields or compared["primaryConsensus"] != all(expected_fields.values()):
            fail(f"raw-to-comparison drift: {case_id}")
        adjudicated = adjudication_by_id.get(case_id)
        if not adjudicated or adjudicated.get("adjudicated") != "accept" or not adjudicated.get("rationale"):
            fail(f"missing accountable adjudication: {case_id}")
    adjudicator = adjudication.get("adjudicator", {})
    if not all(adjudicator.get(field) for field in ("identity", "adjudicatedAt", "attestation")):
        fail("adjudication lacks accountable identity, timestamp, or attestation")
    print(f"validated {run['milestoneId']}: 11 cases, 22 fresh sessions, mechanical comparison, accountable adjudication")


if __name__ == "__main__":
    main()
