#!/usr/bin/env python3
"""Generate the machine-readable evidence map used by the documentation site.

This script consumes selection/review/source manifests and, when supplied,
checksum-verified immutable publication bundles. It does not run an analyzer,
re-score raw reports, or manufacture a result report. Published score rows are
transported only from the generated result page in a validated bundle and are
bound to that bundle's manifest and report hashes.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
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
RELEASE_TAG = re.compile(r"^v[0-9]+\.[0-9]+\.[0-9]+$")
REVISION = re.compile(r"^[0-9a-f]{40}$")
SHA256 = re.compile(r"^[0-9a-f]{64}$")


def read_json(relative: str) -> dict[str, Any]:
    return json.loads((ROOT / relative).read_text(encoding="utf-8"))


def digest(relative: str) -> str:
    return hashlib.sha256((ROOT / relative).read_bytes()).hexdigest()


def link(relative: str) -> dict[str, str]:
    return {"file": relative, "sha256": digest(relative)}


def _bundle_validator() -> Any:
    """Load the canonical immutable-bundle validator beside this script."""

    path = ROOT / "scripts/validate-publication-bundle.py"
    spec = importlib.util.spec_from_file_location("usagebench_publication_bundle", path)
    if spec is None or spec.loader is None:
        raise ValueError(f"could not load publication bundle validator: {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _bundle_json(path: Path, label: str) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise ValueError(f"could not read {label}: {error}") from error
    if not isinstance(value, dict):
        raise ValueError(f"{label} must contain a JSON object")
    return value


def _bundle_artifact(path: Path, relative: str, label: str) -> dict[str, str]:
    artifact = path / relative
    if not artifact.is_file() or artifact.is_symlink():
        raise ValueError(f"immutable bundle is missing {label}: {relative}")
    return {"file": relative, "sha256": hashlib.sha256(artifact.read_bytes()).hexdigest()}


def _generated_scores(path: Path) -> dict[str, Any]:
    """Extract score rows from the already-generated immutable result page.

    The Rust result generator is the score authority. This parser only
    transports its table values into the evidence map; it never evaluates raw
    reports or supplies fallback totals.
    """

    lines = path.read_text(encoding="utf-8").splitlines()
    required_start = "## Required-destination comparison"
    strict_start = "## Strict contract conformance"
    try:
        required_index = lines.index(required_start)
        strict_index = lines.index(strict_start)
    except ValueError as error:
        raise ValueError(f"generated results page lacks score sections: {path}") from error

    required_rows: list[dict[str, Any]] = []
    strict_rows: list[dict[str, Any]] = []
    required_pattern = re.compile(
        r"^\| ([^|]+) \| (\d+) \| (\d+)/(\d+) \([^)]*\) \| (\d+)/(\d+) \([^)]*\) \|$"
    )
    strict_pattern = re.compile(
        r"^\| ([^|]+) \| (\d+) \| (\d+) \| (\d+) \| (\d+) \| (\d+) \|$"
    )
    for line in lines[required_index + 1 : strict_index]:
        match = required_pattern.fullmatch(line)
        if match:
            name, shared, bifrost_found, denominator, reference_found, reference_denominator = match.groups()
            if denominator != reference_denominator:
                raise ValueError(f"generated required-score denominators disagree for {name}")
            required_rows.append(
                {
                    "reference": name.strip(),
                    "shared": int(shared),
                    "denominator": int(denominator),
                    "bifrostFound": int(bifrost_found),
                    "referenceFound": int(reference_found),
                }
            )
    for line in lines[strict_index + 1 :]:
        match = strict_pattern.fullmatch(line)
        if match:
            name, shared, both, bifrost_only, reference_only, neither = match.groups()
            strict_rows.append(
                {
                    "reference": name.strip(),
                    "shared": int(shared),
                    "both": int(both),
                    "bifrostOnly": int(bifrost_only),
                    "referenceOnly": int(reference_only),
                    "neither": int(neither),
                }
            )
    if not required_rows or not strict_rows or len(required_rows) != len(strict_rows):
        raise ValueError(f"generated result score rows are incomplete: {path}")
    if [row["reference"] for row in required_rows] != [row["reference"] for row in strict_rows]:
        raise ValueError(f"generated result score profiles disagree: {path}")
    strict_denominator = sum(row["shared"] for row in strict_rows)
    required_denominator = sum(row["shared"] for row in required_rows)
    return {
        "profiles": [
            {
                "reference": required["reference"],
                "strict": {
                    "shared": strict["shared"],
                    "bifrostExact": strict["both"] + strict["bifrostOnly"],
                    "referenceExact": strict["both"] + strict["referenceOnly"],
                },
                "required": {
                    "shared": required["shared"],
                    "bifrostFound": required["bifrostFound"],
                    "referenceFound": required["referenceFound"],
                },
            }
            for required, strict in zip(required_rows, strict_rows)
        ],
        "strict": {
            "denominator": strict_denominator,
            "bifrostExact": sum(row["both"] + row["bifrostOnly"] for row in strict_rows),
            "referenceExact": sum(row["both"] + row["referenceOnly"] for row in strict_rows),
        },
        "required": {
            "denominator": required_denominator,
            "bifrostFound": sum(row["bifrostFound"] for row in required_rows),
            "referenceFound": sum(row["referenceFound"] for row in required_rows),
        },
    }


def _verified_bundle(path: Path) -> dict[str, Any]:
    """Validate and summarize one extracted immutable publication bundle.

    The bundle validator is deliberately called here too, rather than relying
    on the docs workflow's earlier invocation. This keeps local generation and
    CI generation subject to the same checksum, partition, and generated-page
    contract.
    """

    path = path.resolve()
    metadata = _bundle_json(path / ".usagebench-release.json", "release metadata")
    tag = metadata.get("releaseTag")
    revision = metadata.get("revision")
    if not isinstance(tag, str) or not RELEASE_TAG.fullmatch(tag):
        raise ValueError(f"immutable bundle release tag is invalid: {tag!r}")
    if not isinstance(revision, str) or not REVISION.fullmatch(revision):
        raise ValueError(f"immutable bundle revision is invalid: {revision!r}")
    validator = _bundle_validator()
    try:
        validator.validate_bundle(path, tag, revision)
    except Exception as error:
        # v0.2.0 predates the corpus-hash sidecar introduced for v0.3.0.
        # Keep this compatibility path exact to the historical release and
        # retain all evidence/report/generated-page checksum checks. Never
        # broaden the current validator's acceptance rules for new bundles.
        historical_revision = "6ea6056fa6b3eb52a656a2b4a62c57956771de78"
        if tag != "v0.2.0" or revision != historical_revision:
            raise ValueError(f"immutable bundle {tag} failed validation: {error}") from error
        try:
            validator.validate_historical_v1_bundle(path, tag, revision)
        except Exception as historical_error:
            raise ValueError(
                f"historical v0.2.0 bundle failed its compatibility checks: {historical_error}"
            ) from historical_error

    manifest = _bundle_json(path / "evidence/freeze-manifest.json", "freeze manifest")
    snapshot_kind = manifest.get("snapshotKind")
    if snapshot_kind == "evaluation":
        audit = manifest.get("evaluationAudit")
        freeze_id = audit.get("freezeId") if isinstance(audit, dict) else None
        if freeze_id not in {"real-project-v1", "real-project-v2"}:
            raise ValueError(f"evaluation bundle has an unknown freeze ID: {freeze_id!r}")
        slice_id = "v1" if freeze_id == "real-project-v1" else "v2"
    elif snapshot_kind == "legacy_promoted":
        audit = manifest.get("legacyPromotionAudit")
        promotion_id = audit.get("promotionId") if isinstance(audit, dict) else None
        if promotion_id != "legacy-promotion-v1-balanced-core":
            raise ValueError(f"legacy bundle has an unknown promotion ID: {promotion_id!r}")
        slice_id = "legacy"
    else:
        raise ValueError(
            "docs publication accepts only evaluation or legacy_promoted bundles; "
            f"received {snapshot_kind!r}"
        )

    manifest_artifact = _bundle_artifact(
        path, "evidence/freeze-manifest.json", "freeze manifest"
    )
    reports: list[dict[str, str]] = []
    for report in manifest.get("reports", []):
        if not isinstance(report, dict):
            raise ValueError("freeze manifest report entry is not an object")
        candidate_id = report.get("candidateId")
        report_file = report.get("file")
        report_sha = report.get("sha256")
        if (
            not isinstance(candidate_id, str)
            or not isinstance(report_file, str)
            or not isinstance(report_sha, str)
            or not SHA256.fullmatch(report_sha)
        ):
            raise ValueError("freeze manifest contains invalid report provenance")
        actual = _bundle_artifact(path, f"evidence/{report_file}", "frozen report")
        if actual["sha256"] != report_sha:
            raise ValueError(
                f"frozen report checksum drift for {report_file}: "
                f"{report_sha} != {actual['sha256']}"
            )
        reports.append(
            {"candidateId": candidate_id, "file": f"evidence/{report_file}", "sha256": report_sha}
        )
    reports.sort(key=lambda report: report["candidateId"])
    generated = [
        _bundle_artifact(path, "results/results.md", "generated results page"),
        _bundle_artifact(path, "results/case-comparison.md", "generated case comparison"),
    ]
    return {
        "sliceId": slice_id,
        "release": tag,
        "revision": revision,
        "snapshotKind": snapshot_kind,
        "manifest": manifest_artifact,
        "reports": reports,
        "generatedResults": generated,
        "derivedScores": _generated_scores(path / "results/results.md"),
    }


def bundle_results(bundles: list[Path]) -> dict[str, dict[str, Any]]:
    """Return verified bundle summaries keyed by the fixed publication strata."""

    results: dict[str, dict[str, Any]] = {}
    for bundle in bundles:
        summary = _verified_bundle(bundle)
        slice_id = summary["sliceId"]
        if slice_id in results:
            raise ValueError(f"multiple immutable bundles supplied for {slice_id}")
        results[slice_id] = summary
    return results


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


def _result_for(
    slice_id: str, *, default_status: str, default_release: str | None, bundles: dict[str, dict[str, Any]]
) -> dict[str, Any]:
    summary = bundles.get(slice_id)
    if summary is None:
        return {
            "status": default_status,
            "release": default_release,
            "reportArtifact": None,
        }
    return {
        "status": "published_immutable_release",
        "release": summary["release"],
        "revision": summary["revision"],
        "snapshotKind": summary["snapshotKind"],
        "derivedScores": summary["derivedScores"],
        "reportArtifact": {
            "manifest": summary["manifest"],
            "reports": summary["reports"],
            "generatedResults": summary["generatedResults"],
        },
    }


def build_slice(
    freeze: str,
    *,
    result_status: str,
    release: str | None,
    bundles: dict[str, dict[str, Any]],
) -> dict[str, Any]:
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
        "result": _result_for(
            "v1" if freeze == "real-project-v1" else "v2",
            default_status=result_status,
            default_release=release,
            bundles=bundles,
        ),
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


def build_legacy(bundles: dict[str, dict[str, Any]]) -> dict[str, Any]:
    # The published legacy result is the authority for this slice, so the map
    # describes the manifest that snapshot was frozen under, not the one the
    # next freeze will use. Move this forward together with the legacy release
    # that supersedes it.
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
        "result": _result_for(
            "legacy",
            default_status="awaiting_checksum_verified_report",
            default_release=None,
            bundles=bundles,
        ),
        "inputs": [
            link(manifest_path),
            link(cohort_path),
            {
                "file": manifest["eligibilityPolicy"]["file"],
                "sha256": manifest["eligibilityPolicy"]["sha256"],
            },
        ],
    }


def build_data(bundles: dict[str, dict[str, Any]] | None = None) -> dict[str, Any]:
    bundles = bundles or {}
    development_count = len(development_case_ids())
    legacy = build_legacy(bundles)
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
                bundles=bundles,
            ),
            "v2": build_slice(
                "real-project-v2",
                result_status="awaiting_checksum_verified_report",
                release=None,
                bundles=bundles,
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
    v2_published = v2["result"]["status"] == "published_immutable_release"
    legacy_published = legacy["result"]["status"] == "published_immutable_release"
    v2_release = v2["result"]["release"]
    legacy_release = legacy["result"]["release"]
    v2_publication = (
        f"Immutable release `{v2_release}`; generated report provenance is recorded below"
        if v2_published
        else "Awaiting an immutable result report"
    )
    legacy_publication = (
        f"Immutable release `{legacy_release}`; generated report provenance is recorded below"
        if legacy_published
        else "Awaiting an immutable result report"
    )
    pending_sentence = (
        "The independent v2 slice and the retrospectively reviewed legacy core have "
        "frozen source/review boundaries, but this checkout does not contain "
        "checksum-verified analyzer result reports for either slice. They remain "
        "visibly pending rather than being scored or pooled."
        if not v2_published and not legacy_published
        else "Published result status is derived from the immutable bundles supplied to this generation run; no score is copied into this evidence map."
    )
    lines = [
        "---",
        "title: Evidence map",
        "description: Machine-derived UsageBench evidence boundaries and immutable publication status.",
        "---",
        "",
        "> **Generated evidence boundary.** This page is generated from the checked-in selection, review, source-lock, promotion, and cohort manifests. It does not run Bifrost and it does not copy provisional scores.",
        "",
        f"The shortest honest answer is currently the published `real-project-v1` result: its historical v0.2.0 page reports the measured Bifrost/reference comparison. {pending_sentence}",
        "",
        "## Evidence breadth",
        "",
        "| Slice | Frozen identity | Selection and review tier | Frozen denominator | Result publication |",
        "| --- | --- | --- | ---: | --- |",
        f"| Prospective v1 | `{v1['freezeId']}` · `{v1['result']['release']}` | `{v1['selectionRegime']}` · `{v1['reviewTier']}` | {v1['caseCount']} cases (12 per profile) | Historical release; [current v1 result](../) |",
        f"| Prospective v2 | `{v2['freezeId']}` · {v2_release or 'no result release yet'} | `{v2['selectionRegime']}` · `{v2['reviewTier']}` | {v2['caseCount']} cases (12 per profile) | {v2_publication} |",
        f"| Reviewed legacy core | `{legacy['promotionId']}` · {legacy_release or 'no result release yet'} | `{legacy['selectionRegime']}` · `{legacy['reviewTier']}` | {legacy['caseCount']} cases ({legacy['perLanguage']} × {legacy['languageCount']} languages) | {legacy_publication} |",
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
        "## Immutable report provenance",
        "",
        "The entries below are derived only from checksum-verified release bundles. The generated result pages remain the score authority; this index records their exact release, snapshot, manifest, and report identities without retyping score totals.",
        "",
        "| Slice | Release | Revision | Freeze manifest SHA-256 | Report artifacts |",
        "| --- | --- | --- | --- | --- |",
    ]
    for name, slice_data in (("v1", v1), ("v2", v2), ("legacy", legacy)):
        result = slice_data["result"]
        artifact = result.get("reportArtifact")
        if not artifact:
            if name == "v1" and result.get("release"):
                lines.append(
                    f"| {name} | `{result['release']}` | — | — | historical release; bundle not supplied in this generation |"
                )
                continue
            lines.append(f"| {name} | — | — | — | pending immutable bundle |")
            continue
        report_links = ", ".join(
            f"`{report['candidateId']}` `{report['sha256']}`"
            for report in artifact["reports"]
        )
        lines.append(
            f"| {name} | `{result['release']}` | `{result['revision']}` | "
            f"`{artifact['manifest']['sha256']}` | {report_links} |"
        )
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
        "- Score tables must be generated from a checksum-verified immutable report artifact bound to the matching manifest. This page records published provenance when such a bundle is supplied and otherwise reports readiness, never guessed scores.",
        "",
        "Manifest provenance is machine-readable in `docs/src/data/evidence.json` and is checked in CI with `scripts/generate-docs-evidence.py --check`. See the [current v1 result](../), the [historical development result](../development-2026-07-24/), and the [human ground-truth audit](../../ground-truth-review/) for retained historical evidence.",
        "",
    ]
    return "\n".join(lines)


def write_or_check(
    check: bool, bundles: dict[str, dict[str, Any]] | None = None
) -> int:
    data = json.dumps(build_data(bundles), indent=2, sort_keys=True) + "\n"
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
    parser.add_argument(
        "--bundle",
        action="append",
        type=Path,
        default=[],
        help=(
            "extracted immutable publication bundle to consume; may be repeated "
            "once for v1, v2, and legacy"
        ),
    )
    args = parser.parse_args()
    try:
        return write_or_check(args.check, bundle_results(args.bundle))
    except (OSError, KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
        print(f"docs evidence generation failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
