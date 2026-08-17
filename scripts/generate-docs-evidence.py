#!/usr/bin/env python3
"""Generate the machine-readable evidence map used by the documentation site.

This script intentionally consumes selection/review/source manifests only. It
does not run an analyzer and it does not manufacture a result report. A
published score may be added to a slice only by a later report generator that
binds a checksum-verified immutable report to the same manifest.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
OUTPUTS = (
    ROOT / "docs/src/data/evidence.json",
    ROOT / "docs/src/content/docs/results/evidence.md",
)
CASE_ID = re.compile(r"^  - id:\s*([A-Za-z0-9_.-]+)\s*$")


def read_json(relative: str) -> dict[str, Any]:
    return json.loads((ROOT / relative).read_text(encoding="utf-8"))


def digest(relative: str) -> str:
    return hashlib.sha256((ROOT / relative).read_bytes()).hexdigest()


def link(relative: str) -> dict[str, str]:
    return {"file": relative, "sha256": digest(relative)}


def selected_profiles(freeze: str) -> list[dict[str, Any]]:
    selection = read_json(f"benchmarks/evaluation/{freeze}/selection.json")
    protocol = read_json(f"benchmarks/evaluation/{freeze}/protocol.json")
    target_profiles = {
        profile["language"]: profile for profile in protocol["targetProfiles"]
    }
    profiles = []
    seen_cases: set[str] = set()
    for profile in selection["profiles"]:
        selected = profile["selected"]
        case_ids = [case_id for repository in selected for case_id in repository["caseIds"]]
        if len(selected) != 4 or len(case_ids) != 12:
            raise ValueError(
                f"{freeze}/{profile['language']} must contain 4 repositories and 12 cases"
            )
        if seen_cases.intersection(case_ids):
            raise ValueError(f"duplicate case ID in {freeze}: {profile['language']}")
        seen_cases.update(case_ids)
        target = target_profiles.get(profile["language"])
        if not target or target["candidateId"] != profile["candidateId"]:
            raise ValueError(f"{freeze}/{profile['language']} target profile drift")
        profiles.append(
            {
                "language": profile["language"],
                "candidateId": profile["candidateId"],
                "repositories": len(selected),
                "caseCount": len(case_ids),
            }
        )
    if len(profiles) != len(target_profiles):
        raise ValueError(f"{freeze} profile denominator drift")
    return profiles


def build_slice(freeze: str, *, result_status: str, release: str | None) -> dict[str, Any]:
    protocol_path = f"benchmarks/evaluation/{freeze}/protocol.json"
    population_path = f"benchmarks/evaluation/{freeze}/population.json"
    selection_path = f"benchmarks/evaluation/{freeze}/selection.json"
    review_path = f"benchmarks/evaluation/{freeze}/review.json"
    sources_path = f"benchmarks/evaluation/{freeze}/sources.json"
    protocol = read_json(protocol_path)
    selection = read_json(selection_path)
    review = read_json(review_path)
    profiles = selected_profiles(freeze)
    if protocol["freezeId"] != freeze or selection["freezeId"] != freeze:
        raise ValueError(f"{freeze} freeze identity drift")
    if review["freezeId"] != freeze:
        raise ValueError(f"{freeze} review identity drift")
    if freeze == "real-project-v2":
        reporting = protocol.get("reporting")
        if reporting != {
            "separateSliceDenominators": True,
            "combinedAggregation": "stratified-only",
        }:
            raise ValueError("real-project-v2 reporting boundary drift")
    return {
        "freezeId": freeze,
        "selectionRegime": "prospective_pre_registered",
        "reviewTier": review["reviewTier"],
        "caseCount": sum(profile["caseCount"] for profile in profiles),
        "profiles": profiles,
        "claimScope": protocol["claimScope"],
        "result": {
            "status": result_status,
            "release": release,
            "reportArtifact": None,
        },
        "inputs": [
            link(protocol_path),
            link(population_path),
            link(selection_path),
            link(review_path),
            link(sources_path),
        ],
    }


def development_case_ids() -> list[str]:
    ids: list[str] = []
    for path in sorted((ROOT / "benchmarks/cases").glob("*.yaml")):
        for line in path.read_text(encoding="utf-8").splitlines():
            match = CASE_ID.match(line)
            if match:
                ids.append(match.group(1))
    if len(ids) != len(set(ids)):
        raise ValueError("duplicate case ID in development corpus")
    return ids


def build_legacy() -> dict[str, Any]:
    manifest_path = "benchmarks/promotion/legacy-v1/manifest.json"
    cohort_path = "benchmarks/promotion/legacy-v1/cohort.json"
    manifest = read_json(manifest_path)
    cohort = read_json(cohort_path)
    decisions: dict[str, int] = {}
    for row in cohort["inventory"]:
        decision = row["decision"]
        decisions[decision] = decisions.get(decision, 0) + 1
    core = sum(len(document["cases"]) for document in manifest["documents"])
    expected_core = manifest["balancePolicy"]["languageCount"] * manifest["balancePolicy"]["balancedCorePerLanguage"]
    if core != expected_core or decisions.get("balanced_core") != core:
        raise ValueError("legacy balanced-core denominator drift")
    controls = decisions.get("control_not_planned", 0) + decisions.get("control_unsupported", 0)
    overflow = decisions.get("overflow", 0)
    inventory_count = len(cohort["inventory"])
    if core + controls + overflow != inventory_count:
        raise ValueError("legacy inventory partition does not sum")
    language_counts = {
        language: sum(
            len(document["cases"])
            for document in manifest["documents"]
            if document["language"] == language
        )
        for language in sorted(manifest["balancePolicy"]["eligibleCounts"])
    }
    if set(language_counts) != set(manifest["balancePolicy"]["eligibleCounts"]):
        raise ValueError("legacy language denominator drift")
    if set(language_counts.values()) != {manifest["balancePolicy"]["balancedCorePerLanguage"]}:
        raise ValueError("legacy per-language denominator drift")
    return {
        "promotionId": manifest["promotionId"],
        "selectionRegime": manifest["selectionProvenance"],
        "reviewTier": manifest["reviewTier"],
        "selectionBasis": manifest["selectionBasis"],
        "analyzerOutcomeUse": manifest["analyzerOutcomeUse"],
        "caseCount": core,
        "languageCount": manifest["balancePolicy"]["languageCount"],
        "perLanguage": manifest["balancePolicy"]["balancedCorePerLanguage"],
        "denominators": language_counts,
        "inventoryCases": inventory_count,
        "overflowCases": overflow,
        "controlCases": controls,
        "claimScope": manifest["claimScope"],
        "result": {
            "status": "awaiting_checksum_verified_report",
            "release": None,
            "reportArtifact": None,
        },
        "inputs": [
            link(manifest_path),
            link(cohort_path),
            {
                "file": manifest["eligibilityPolicy"]["file"],
                "sha256": manifest["eligibilityPolicy"]["sha256"],
            },
        ],
    }


def build_data() -> dict[str, Any]:
    development_count = len(development_case_ids())
    legacy = build_legacy()
    remaining = development_count - legacy["caseCount"]
    if remaining < 0:
        raise ValueError("legacy core exceeds development corpus")
    return {
        "schemaVersion": 1,
        "generatedBy": "scripts/generate-docs-evidence.py",
        "slices": {
            "v1": build_slice(
                "real-project-v1",
                result_status="published_historical_release",
                release="v0.2.0",
            ),
            "v2": build_slice(
                "real-project-v2",
                result_status="awaiting_checksum_verified_report",
                release=None,
            ),
            "legacy": legacy,
        },
        "development": {
            "caseCount": development_count,
            "legacyInventoryCases": legacy["inventoryCases"],
            "balancedCoreCases": legacy["caseCount"],
            "overflowCases": legacy["overflowCases"],
            "controlCases": legacy["controlCases"],
            "remainingCases": remaining,
            "semanticPackCases": development_count - legacy["inventoryCases"],
        },
        "guardrails": [
            "Each prospective slice keeps its own profile and language denominator.",
            "The reviewed legacy core is retrospectively selected and never becomes preregistered.",
            "Controls, overflow, and development cases are not correctness denominators.",
            "No score is published without a checksum-verified immutable result report.",
        ],
    }


def markdown(data: dict[str, Any]) -> str:
    v1 = data["slices"]["v1"]
    v2 = data["slices"]["v2"]
    legacy = data["slices"]["legacy"]
    development = data["development"]
    lines = [
        "---",
        "title: Evidence map",
        "description: Machine-derived UsageBench evidence boundaries and immutable publication status.",
        "---",
        "",
        "> **Generated evidence boundary.** This page is generated from the checked-in selection, review, source-lock, promotion, and cohort manifests. It does not run Bifrost and it does not copy provisional scores.",
        "",
        "The shortest honest answer is currently the published `real-project-v1` result: its historical v0.2.0 page reports the measured Bifrost/reference comparison. The independent v2 slice and the retrospectively reviewed legacy core have frozen source/review boundaries, but this checkout does not contain checksum-verified analyzer result reports for either slice. They remain visibly pending rather than being scored or pooled.",
        "",
        "## Evidence breadth",
        "",
        "| Slice | Frozen identity | Selection and review tier | Frozen denominator | Result publication |",
        "| --- | --- | --- | ---: | --- |",
        f"| Prospective v1 | `{v1['freezeId']}` · `{v1['result']['release']}` | `{v1['selectionRegime']}` · `{v1['reviewTier']}` | {v1['caseCount']} cases (12 per profile) | Historical release; [current v1 result](../) |",
        f"| Prospective v2 | `{v2['freezeId']}` · no result release yet | `{v2['selectionRegime']}` · `{v2['reviewTier']}` | {v2['caseCount']} cases (12 per profile) | Awaiting an immutable result report |",
        f"| Reviewed legacy core | `{legacy['promotionId']}` | `{legacy['selectionRegime']}` · `{legacy['reviewTier']}` | {legacy['caseCount']} cases ({legacy['perLanguage']} × {legacy['languageCount']} languages) | Awaiting an immutable result report |",
        "",
        "The denominators above are not interchangeable. In particular, v1 and v2 are prospective source-only selections, while the legacy core is a separately frozen retrospective promotion of analyzer-informed development cases. A later report may present a documented stratified aggregate, but it may not flatten these trust tiers into one accuracy score.",
        "",
        "## Prospective profile denominators",
        "",
        "Each profile remains visible before any aggregate. The numbers here are selected-case counts, not analyzer outcomes.",
        "",
        "| Slice | Language | Candidate/reference profile | Repositories | Cases |",
        "| --- | --- | --- | ---: | ---: |",
    ]
    for name, slice_data in (("v1", v1), ("v2", v2)):
        for profile in slice_data["profiles"]:
            lines.append(
                f"| {name} | {profile['language']} | `{profile['candidateId']}` | {profile['repositories']} | {profile['caseCount']} |"
            )
    lines += [
        "",
        "## Reviewed legacy boundaries",
        "",
        f"The immutable promotion is `{legacy['promotionId']}`. Its balanced core is **{legacy['caseCount']} cases**, with **{legacy['overflowCases']} overflow** candidates and **{legacy['controlCases']} controls** kept outside the correctness denominator. The source-only legacy inventory contains {legacy['inventoryCases']} cases; the remaining development corpus also contains {development['semanticPackCases']} semantic-pack cases that were never part of that inventory.",
        "",
        "| Language | Balanced-core cases |",
        "| --- | ---: |",
    ]
    for language, count in legacy["denominators"].items():
        lines.append(f"| {language} | {count} |")
    lines += [
        "",
        "## Remaining development evidence",
        "",
        f"The checked-in development corpus contains **{development['caseCount']} cases**. The reviewed legacy core accounts for {development['balancedCoreCases']}; the {development['remainingCases']} cases outside that core comprise {development['overflowCases']} frozen overflow candidates, {development['controlCases']} unsupported/not-planned controls, and {development['semanticPackCases']} semantic-pack cases. This remainder is retained for regression and diagnosis; it is not silently added to v1, v2, or the legacy denominator.",
        "",
        "## Publication safeguards",
        "",
        "- Prospective v1 and v2 keep separate profile/language denominators; v2 permits only documented stratified aggregation.",
        "- The legacy manifest records `retrospectively_selected`, `legacy_promoted`, `source_only`, and `analyzerOutcomeUse: forbidden`; re-review cannot make the source contract preregistered.",
        "- Controls and overflow remain explicit partitions and cannot enter the balanced-core score.",
        "- Score tables must be generated from a checksum-verified immutable report artifact bound to the matching manifest. This page therefore reports readiness, not guessed scores, for v2 and legacy.",
        "",
        "Manifest provenance is machine-readable in `docs/src/data/evidence.json` and is checked in CI with `scripts/generate-docs-evidence.py --check`. See the [current v1 result](../), the [historical development result](../development-2026-07-24/), and the [human ground-truth audit](../../ground-truth-review/) for retained historical evidence.",
        "",
    ]
    return "\n".join(lines)


def write_or_check(check: bool) -> int:
    data = json.dumps(build_data(), indent=2, sort_keys=True) + "\n"
    rendered = markdown(json.loads(data))
    expected = {OUTPUTS[0]: data, OUTPUTS[1]: rendered}
    drift = []
    for path, content in expected.items():
        actual = path.read_text(encoding="utf-8") if path.exists() else None
        if actual != content:
            drift.append(str(path.relative_to(ROOT)))
        if not check:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding="utf-8")
    if drift and check:
        print("generated docs evidence is stale:", ", ".join(drift), file=sys.stderr)
        print("run scripts/generate-docs-evidence.py", file=sys.stderr)
        return 1
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="fail if generated docs files drift")
    args = parser.parse_args()
    try:
        return write_or_check(args.check)
    except (OSError, KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
        print(f"docs evidence generation failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
