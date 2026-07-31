//! Deterministic public-result pages derived from frozen benchmark evidence.
//!
//! This module deliberately reads the release manifest and its raw reports
//! directly. It never accepts copied totals as input: a generated page is only
//! as trustworthy as the immutable report bytes whose digests it verifies.

use crate::{
    evaluation::{
        safe_repo_relative_path, validate_report_against_release_audit, EvaluationReleaseAudit,
    },
    freeze::{FreezeManifest, ManifestCandidate, FREEZE_MANIFEST_SCHEMA_VERSION},
    reproduction::validate_evidence,
    runners::{CaseRunReport, CaseStatus, LocationMetrics, RequiredDestinationStatus, RunReport},
};
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

mod location_metrics;
use location_metrics::{
    metric_rates, ratio, MetricAverageSet, MetricFormat, MetricRates, ProfileLocationMetrics,
    METRIC_DESCRIPTORS,
};

const RESULTS_FILE: &str = "results.md";
const CASE_COMPARISON_FILE: &str = "case-comparison.md";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedResultPages {
    pub results: String,
    pub case_comparison: String,
}

/// Generate public Markdown fragments from a `freeze-manifest.json` and the
/// report JSON files beside it. Every manifest checksum is verified first.
pub fn generate_result_pages(manifest_path: &Path) -> Result<GeneratedResultPages> {
    let snapshot = load_snapshot(manifest_path)?;
    let bifrost = snapshot.bifrost()?;
    let references = snapshot.references()?;
    if references.is_empty() {
        bail!("snapshot contains no LSP reference reports");
    }
    ensure_location_metrics_available(&snapshot)?;

    let comparisons = references
        .iter()
        .map(|reference| compare_reports(bifrost, reference))
        .collect::<Result<Vec<_>>>()?;

    Ok(GeneratedResultPages {
        results: render_results(&snapshot, &comparisons)?,
        case_comparison: render_case_comparison(&snapshot, &comparisons),
    })
}

fn ensure_location_metrics_available(snapshot: &Snapshot) -> Result<()> {
    for (candidate_id, loaded) in &snapshot.reports {
        if loaded.report.totals.location_metrics.is_none() {
            bail!(
                "location metrics are unavailable for candidate {} from UsageBench {}; regenerate the report with UsageBench >=0.2.0",
                candidate_id,
                loaded.report.usagebench_version,
            );
        }
    }
    Ok(())
}

/// Write generated fragments, or fail when the checked-in files are stale.
pub fn write_result_pages(
    manifest_path: &Path,
    output_directory: &Path,
    check: bool,
) -> Result<()> {
    let pages = generate_result_pages(manifest_path)?;
    let outputs = [
        (output_directory.join(RESULTS_FILE), pages.results),
        (
            output_directory.join(CASE_COMPARISON_FILE),
            pages.case_comparison,
        ),
    ];

    for (path, contents) in outputs {
        if check {
            let existing = fs::read_to_string(&path)
                .with_context(|| format!("read generated result page {}", path.display()))?;
            if existing != contents {
                bail!(
                    "generated result page is stale: {} (run generate-results without --check)",
                    path.display()
                );
            }
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        fs::write(&path, contents).with_context(|| format!("write {}", path.display()))?;
    }
    Ok(())
}

struct Snapshot {
    manifest: FreezeManifest,
    manifest_checksum: String,
    reports: BTreeMap<String, LoadedReport>,
}

struct LoadedReport {
    candidate: ManifestCandidate,
    report: RunReport,
    checksum: String,
}

impl Snapshot {
    fn bifrost(&self) -> Result<&LoadedReport> {
        let mut matching = self
            .reports
            .values()
            .filter(|loaded| loaded.candidate.runner == "bifrost");
        let bifrost = matching
            .next()
            .context("snapshot does not contain a Bifrost report")?;
        if matching.next().is_some() {
            bail!("snapshot contains more than one Bifrost report");
        }
        Ok(bifrost)
    }

    fn references(&self) -> Result<Vec<&LoadedReport>> {
        let references = self
            .reports
            .values()
            .filter(|loaded| loaded.candidate.runner == "lsp")
            .collect::<Vec<_>>();
        if self
            .reports
            .values()
            .any(|loaded| loaded.candidate.runner != "bifrost" && loaded.candidate.runner != "lsp")
        {
            bail!("snapshot contains a candidate with an unsupported runner kind");
        }
        Ok(references)
    }
}

fn load_snapshot(manifest_path: &Path) -> Result<Snapshot> {
    let manifest_bytes = fs::read(manifest_path)
        .with_context(|| format!("read freeze manifest {}", manifest_path.display()))?;
    let manifest: FreezeManifest = serde_json::from_slice(&manifest_bytes)
        .with_context(|| format!("parse freeze manifest {}", manifest_path.display()))?;
    if manifest.schema_version != FREEZE_MANIFEST_SCHEMA_VERSION {
        bail!(
            "unsupported freeze manifest schema version {}",
            manifest.schema_version
        );
    }
    validate_snapshot_audit(manifest_path, &manifest)?;
    let evidence_directory = manifest_path
        .parent()
        .context("freeze manifest has no parent directory")?;

    let candidates = manifest
        .candidates
        .iter()
        .map(|candidate| (candidate.id.clone(), candidate.clone()))
        .collect::<BTreeMap<_, _>>();
    if candidates.len() != manifest.candidates.len() {
        bail!("freeze manifest contains duplicate candidate IDs");
    }
    let evidence_links = manifest
        .candidate_evidence
        .iter()
        .map(|evidence| (evidence.candidate_id.clone(), evidence))
        .collect::<BTreeMap<_, _>>();
    if evidence_links.len() != manifest.candidate_evidence.len()
        || evidence_links.len() != manifest.candidates.len()
    {
        bail!("freeze manifest must contain one unique evidence link per candidate");
    }

    let mut reports = BTreeMap::new();
    let mut report_files = BTreeSet::new();
    for entry in &manifest.reports {
        let candidate = candidates
            .get(&entry.candidate_id)
            .with_context(|| {
                format!(
                    "manifest report references unknown candidate {}",
                    entry.candidate_id
                )
            })?
            .clone();
        let report_file = safe_report_file_name(&entry.file)?;
        if !report_files.insert(report_file.clone()) {
            bail!("freeze manifest reuses report file {}", entry.file);
        }
        let report_path = evidence_directory.join(report_file);
        let bytes = fs::read(&report_path)
            .with_context(|| format!("read frozen report {}", report_path.display()))?;
        let checksum = hex_digest(&bytes);
        if checksum != entry.sha256 {
            bail!(
                "checksum mismatch for {}: manifest {}, actual {}",
                entry.file,
                entry.sha256,
                checksum
            );
        }
        let report: RunReport = serde_json::from_slice(&bytes)
            .with_context(|| format!("parse frozen report {}", report_path.display()))?;
        if report.usagebench_revision != manifest.revision {
            bail!(
                "report {} was produced by {}, expected snapshot revision {}",
                entry.file,
                report.usagebench_revision,
                manifest.revision
            );
        }
        if report.usagebench_release.as_deref() != Some(manifest.version.as_str()) {
            bail!(
                "report {} release does not match snapshot {}",
                entry.file,
                manifest.version
            );
        }
        validate_candidate_report(&candidate, &report)?;
        validate_report_location_metrics(&candidate.id, &report)?;
        let evidence_link = evidence_links.get(&entry.candidate_id).with_context(|| {
            format!(
                "candidate {} lacks reproduction evidence",
                entry.candidate_id
            )
        })?;
        if evidence_link.class != candidate.reproduction_class {
            bail!(
                "candidate {} evidence class does not match manifest",
                entry.candidate_id
            );
        }
        let evidence_file = safe_report_file_name(&evidence_link.file)?;
        let evidence_path = evidence_directory.join(evidence_file);
        let evidence_bytes = fs::read(&evidence_path)
            .with_context(|| format!("read reproduction evidence {}", evidence_path.display()))?;
        if hex_digest(&evidence_bytes) != evidence_link.sha256 {
            bail!(
                "checksum mismatch for reproduction evidence {}",
                evidence_link.file
            );
        }
        let validated = validate_evidence(
            &evidence_path,
            &entry.candidate_id,
            candidate.reproduction_class,
            candidate.reference_runner.as_deref(),
            &candidate.requested_version,
            candidate.profile_sha256.as_deref(),
            &report_path,
            &report,
        )?;
        if !validated.accepted {
            bail!(
                "candidate {} reproduction evidence is not accepted",
                entry.candidate_id
            );
        }
        if report.runner != entry.runner
            || report.environment != entry.environment
            || report.totals != entry.totals
        {
            bail!(
                "manifest metadata does not match frozen report {}",
                entry.file
            );
        }
        if reports
            .insert(
                entry.candidate_id.clone(),
                LoadedReport {
                    candidate,
                    report,
                    checksum,
                },
            )
            .is_some()
        {
            bail!("freeze manifest contains duplicate report candidate IDs");
        }
    }
    if reports.len() != manifest.candidates.len() {
        bail!("freeze manifest does not contain one report for every candidate");
    }
    validate_snapshot_partition(&manifest, &reports)?;
    Ok(Snapshot {
        manifest,
        manifest_checksum: hex_digest(&manifest_bytes),
        reports,
    })
}

fn validate_snapshot_audit(manifest_path: &Path, manifest: &FreezeManifest) -> Result<()> {
    match (manifest.snapshot_kind, manifest.evaluation_audit.as_ref()) {
        (crate::freeze::SnapshotKind::Development, None) => Ok(()),
        (crate::freeze::SnapshotKind::Development, Some(_)) => {
            bail!("development snapshot must not contain an evaluation audit")
        }
        (crate::freeze::SnapshotKind::Evaluation, None) => {
            bail!("evaluation snapshot is missing its evaluation audit")
        }
        (crate::freeze::SnapshotKind::Evaluation, Some(audit)) => {
            let first_case = audit
                .case_files
                .first()
                .context("evaluation audit contains no case files")?;
            let case_path = safe_repo_relative_path(first_case, "evaluation case file")?;
            let corpus_relative = case_path
                .parent()
                .context("evaluation audit case file has no parent directory")?;
            if audit.case_files.iter().any(|file| {
                safe_repo_relative_path(file, "evaluation case file")
                    .ok()
                    .and_then(|path| path.parent().map(Path::to_path_buf))
                    .as_deref()
                    != Some(corpus_relative)
            }) {
                bail!("evaluation audit case files must share one corpus directory");
            }
            let release_root = crate::find_repo_root_for_path(manifest_path)
                .context("locate released UsageBench tree for evaluation audit verification")?;
            let rebuilt =
                crate::evaluation::build_release_audit(release_root.join(corpus_relative))
                    .context("verify evaluation audit evidence")?;
            validate_rebuilt_audit(audit, &rebuilt)
        }
    }
}

fn validate_rebuilt_audit(
    recorded: &EvaluationReleaseAudit,
    rebuilt: &EvaluationReleaseAudit,
) -> Result<()> {
    if recorded != rebuilt {
        bail!("evaluation audit does not match the hash-verified release evidence");
    }
    Ok(())
}

fn validate_snapshot_partition(
    manifest: &FreezeManifest,
    reports: &BTreeMap<String, LoadedReport>,
) -> Result<()> {
    if manifest.snapshot_kind != crate::freeze::SnapshotKind::Evaluation {
        return Ok(());
    }
    let audit = manifest
        .evaluation_audit
        .as_ref()
        .expect("evaluation audit presence was validated");
    let expected_files = audit.case_files.iter().collect::<BTreeSet<_>>();
    let expected_candidates = std::iter::once("bifrost")
        .chain(
            audit
                .target_profiles
                .iter()
                .map(|profile| profile.candidate_id.as_str()),
        )
        .collect::<BTreeSet<_>>();
    let actual_candidates = manifest
        .candidates
        .iter()
        .map(|candidate| candidate.id.as_str())
        .collect::<BTreeSet<_>>();
    if actual_candidates != expected_candidates || reports.keys().len() != expected_candidates.len()
    {
        bail!("evaluation snapshot candidates do not match its registered target profiles");
    }
    let manifest_files = manifest
        .corpus
        .iter()
        .map(|document| &document.case_file)
        .collect::<BTreeSet<_>>();
    if manifest.corpus.len() != expected_files.len()
        || manifest_files != expected_files
        || manifest
            .corpus
            .iter()
            .any(|document| document.partition != crate::CorpusPartition::Evaluation)
    {
        bail!("evaluation snapshot corpus does not exactly match its evaluation audit");
    }
    for (candidate_id, loaded) in reports {
        validate_report_against_release_audit(&loaded.report, audit, candidate_id)?;
    }
    Ok(())
}

fn validate_candidate_report(candidate: &ManifestCandidate, report: &RunReport) -> Result<()> {
    match candidate.runner.as_str() {
        "bifrost" => {
            if report.runner.name != "bifrost" {
                bail!("candidate {} requires a Bifrost report", candidate.id);
            }
            let revision = candidate
                .revision
                .as_deref()
                .context("Bifrost manifest candidates require a pinned revision")?;
            if report.bifrost_resolved_commit.as_deref() != Some(revision) {
                bail!(
                    "Bifrost report does not match the pinned revision for candidate {}",
                    candidate.id
                );
            }
        }
        "lsp" => {
            let profile = candidate
                .profile
                .as_deref()
                .context("LSP manifest candidates require a profile")?;
            let profile_id = Path::new(profile)
                .file_stem()
                .and_then(|name| name.to_str())
                .context("LSP manifest profile has no file stem")?;
            if report.invocation.profile.as_deref() != Some(profile_id)
                || report.invocation.profile_sha256 != candidate.profile_sha256
                || report.runner.requested_version != candidate.requested_version
                || report.runner.source != candidate.source
                || candidate
                    .resolved_version_prefix
                    .as_deref()
                    .is_some_and(|prefix| !report.runner.resolved_version.starts_with(prefix))
            {
                bail!(
                    "LSP report identity does not match manifest candidate {}",
                    candidate.id
                );
            }
        }
        _ => bail!(
            "manifest candidate {} has an unsupported runner kind",
            candidate.id
        ),
    }
    Ok(())
}

fn validate_report_location_metrics(candidate_id: &str, report: &RunReport) -> Result<()> {
    let Some(totals) = &report.totals.location_metrics else {
        if report
            .documents
            .iter()
            .flat_map(|document| &document.cases)
            .any(|case| case.location_metrics.is_some())
        {
            bail!("candidate {candidate_id} mixes case-level location metrics with legacy totals");
        }
        return Ok(());
    };
    totals
        .validate()
        .with_context(|| format!("candidate {candidate_id} has invalid total location metrics"))?;

    let mut merged = LocationMetrics::default();
    for document in &report.documents {
        for case in &document.cases {
            let metrics = case.location_metrics.as_ref().with_context(|| {
                format!(
                    "candidate {candidate_id} lacks location metrics for {} / {}",
                    document.case_file, case.id
                )
            })?;
            metrics.validate().with_context(|| {
                format!(
                    "candidate {candidate_id} has invalid location metrics for {} / {}",
                    document.case_file, case.id
                )
            })?;
            if metrics.cases == 0 {
                if *metrics != LocationMetrics::default() {
                    bail!(
                        "candidate {candidate_id} has non-zero ineligible location metrics for {} / {}",
                        document.case_file,
                        case.id
                    );
                }
            } else {
                let exact_set_evidence = metrics.range_quality.exact_token
                    == metrics.required_locations
                    && metrics.returned_locations.required == metrics.true_positives
                    && metrics.returned_locations.policy_allowed == 0
                    && metrics.false_positives == 0;
                if metrics.cases != 1
                    || metrics.exact_set_cases
                        != usize::from(metrics.exact_set_queries == metrics.queries)
                    || metrics.exact_set_cases != usize::from(exact_set_evidence)
                {
                    bail!(
                        "candidate {candidate_id} has inconsistent case-level exact-set metrics for {} / {}",
                        document.case_file,
                        case.id
                    );
                }
            }
            merged.checked_merge(metrics).with_context(|| {
                format!(
                    "candidate {candidate_id} location metrics overflow while merging {} / {}",
                    document.case_file, case.id
                )
            })?;
        }
    }
    if merged != *totals {
        bail!("candidate {candidate_id} total location metrics do not match merged case metrics");
    }
    Ok(())
}

fn safe_report_file_name(file: &str) -> Result<PathBuf> {
    let path = Path::new(file);
    if path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
    {
        bail!("report file must be a simple file name: {file}");
    }
    Ok(path.to_path_buf())
}

#[derive(Debug)]
struct Comparison {
    reference_id: String,
    reference_name: String,
    reference_version: String,
    strict: OutcomeTotals,
    required: OutcomeTotals,
    bifrost_locations: ProfileLocationMetrics,
    reference_locations: ProfileLocationMetrics,
    bifrost_only: Vec<CaseKey>,
    reference_only: Vec<CaseKey>,
}

#[derive(Debug, Default, Clone, Copy)]
struct OutcomeTotals {
    shared: usize,
    both: usize,
    bifrost_only: usize,
    reference_only: usize,
    neither: usize,
}

impl OutcomeTotals {
    fn add(&mut self, bifrost: bool, reference: bool) {
        self.shared += 1;
        match (bifrost, reference) {
            (true, true) => self.both += 1,
            (true, false) => self.bifrost_only += 1,
            (false, true) => self.reference_only += 1,
            (false, false) => self.neither += 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CaseKey {
    case_file: String,
    language: String,
    id: String,
}

fn compare_reports(bifrost: &LoadedReport, reference: &LoadedReport) -> Result<Comparison> {
    let bifrost_cases = report_cases(&bifrost.report)?;
    let reference_cases = report_cases(&reference.report)?;
    let mut strict = OutcomeTotals::default();
    let mut required = OutcomeTotals::default();
    let mut bifrost_locations = ProfileLocationMetrics::default();
    let mut reference_locations = ProfileLocationMetrics::default();
    let mut bifrost_only = Vec::new();
    let mut reference_only = Vec::new();

    for (key, bifrost_case) in &bifrost_cases {
        let Some(reference_case) = reference_cases.get(key) else {
            continue;
        };
        if strict_scoreable(bifrost_case) && strict_scoreable(reference_case) {
            let bifrost_exact = strict_exact(bifrost_case);
            let reference_exact = strict_exact(reference_case);
            strict.add(bifrost_exact, reference_exact);
            if bifrost_exact && !reference_exact {
                bifrost_only.push(key.clone());
            } else if !bifrost_exact && reference_exact {
                reference_only.push(key.clone());
            }
        }
        if let (Some(bifrost_status), Some(reference_status)) = (
            bifrost_case.required_destination_status,
            reference_case.required_destination_status,
        ) {
            if required_scoreable(bifrost_status) && required_scoreable(reference_status) {
                required.add(
                    bifrost_status == RequiredDestinationStatus::Found,
                    reference_status == RequiredDestinationStatus::Found,
                );
            }
        }
        if let (Some(bifrost_metrics), Some(reference_metrics)) = (
            bifrost_case.location_metrics.as_ref(),
            reference_case.location_metrics.as_ref(),
        ) {
            if bifrost_metrics.cases == 1 && reference_metrics.cases == 1 {
                bifrost_locations.add_case(bifrost_metrics)?;
                reference_locations.add_case(reference_metrics)?;
            }
        }
    }

    if strict.shared == 0 {
        bail!(
            "Bifrost and {} do not share any strict-scoreable cases",
            reference.candidate.id
        );
    }
    if bifrost_locations.cases == 0 {
        bail!(
            "Bifrost and {} do not share any location-metric cases",
            reference.candidate.id
        );
    }
    Ok(Comparison {
        reference_id: reference.candidate.id.clone(),
        reference_name: reference.candidate.name.clone(),
        reference_version: reference.candidate.requested_version.clone(),
        strict,
        required,
        bifrost_locations,
        reference_locations,
        bifrost_only,
        reference_only,
    })
}

fn report_cases(report: &RunReport) -> Result<BTreeMap<CaseKey, &CaseRunReport>> {
    let mut cases = BTreeMap::new();
    for document in &report.documents {
        for case in &document.cases {
            let key = CaseKey {
                case_file: document.case_file.clone(),
                language: document.language.clone(),
                id: case.id.clone(),
            };
            if cases.insert(key.clone(), case).is_some() {
                bail!(
                    "report contains duplicate case {} / {} / {}",
                    key.case_file,
                    key.language,
                    key.id
                );
            }
        }
    }
    Ok(cases)
}

fn strict_scoreable(case: &CaseRunReport) -> bool {
    !matches!(
        case.status,
        CaseStatus::NotPlanned | CaseStatus::Unsupported | CaseStatus::Skipped | CaseStatus::Error
    )
}

fn strict_exact(case: &CaseRunReport) -> bool {
    matches!(case.status, CaseStatus::Passed | CaseStatus::Improved)
}

fn required_scoreable(status: RequiredDestinationStatus) -> bool {
    matches!(
        status,
        RequiredDestinationStatus::Found | RequiredDestinationStatus::Missing
    )
}

fn render_results(snapshot: &Snapshot, comparisons: &[Comparison]) -> Result<String> {
    let mut output = provenance_header(snapshot);
    render_evaluation_audit(&mut output, snapshot)?;
    output.push_str("## Snapshot inputs\n\n");
    output.push_str(
        "| Candidate | Runner | Requested version | Profile | Environment | Reproduction | Report SHA-256 |\n",
    );
    output.push_str("|---|---|---|---|---|---|---|\n");
    for (candidate_id, loaded) in &snapshot.reports {
        let environment = &loaded.report.environment;
        let evidence = snapshot
            .manifest
            .candidate_evidence
            .iter()
            .find(|evidence| evidence.candidate_id == *candidate_id)
            .expect("snapshot validation requires candidate evidence");
        output.push_str(&format!(
            "| {} | {} | {} | {} | {}/{:?}/{:?}/{:?} | [{}](../evidence/{}) | `{}` |\n",
            candidate_id,
            loaded.report.runner.name,
            loaded.report.runner.requested_version,
            loaded.candidate.profile.as_deref().unwrap_or("—"),
            environment.operating_system,
            environment.architecture,
            environment.execution_mode,
            environment.platform_scope,
            evidence.class,
            evidence.file,
            loaded.checksum,
        ));
    }
    output.push('\n');
    output.push_str("## Required-destination comparison\n\n");
    output.push_str("| Reference profile | Shared | Bifrost found | Reference found |\n");
    output.push_str("|---|---:|---:|---:|\n");
    for comparison in comparisons {
        output.push_str(&format!(
            "| {} {} | {} | {}/{} ({}) | {}/{} ({}) |\n",
            comparison.reference_name,
            comparison.reference_version,
            comparison.required.shared,
            comparison.required.bifrost_only + comparison.required.both,
            comparison.required.shared,
            percentage(
                comparison.required.bifrost_only + comparison.required.both,
                comparison.required.shared
            ),
            comparison.required.reference_only + comparison.required.both,
            comparison.required.shared,
            percentage(
                comparison.required.reference_only + comparison.required.both,
                comparison.required.shared
            ),
        ));
    }
    if snapshot.manifest.snapshot_kind == crate::freeze::SnapshotKind::Development {
        let required = aggregate(comparisons, |comparison| comparison.required);
        output.push_str(&format!(
            "| **Pooled** | **{}** | **{}/{} ({})** | **{}/{} ({})** |\n",
            required.shared,
            required.bifrost_only + required.both,
            required.shared,
            percentage(required.bifrost_only + required.both, required.shared),
            required.reference_only + required.both,
            required.shared,
            percentage(required.reference_only + required.both, required.shared),
        ));
    }
    output.push('\n');

    render_location_comparison(
        &mut output,
        comparisons,
        snapshot.manifest.snapshot_kind == crate::freeze::SnapshotKind::Development,
    )?;

    output.push_str("## Strict contract conformance\n\n");
    output.push_str(
        "| Reference profile | Shared | Both exact | Bifrost only | Reference only | Neither |\n",
    );
    output.push_str("|---|---:|---:|---:|---:|---:|\n");
    for comparison in comparisons {
        let strict = comparison.strict;
        output.push_str(&format!(
            "| {} {} | {} | {} | {} | {} | {} |\n",
            comparison.reference_name,
            comparison.reference_version,
            strict.shared,
            strict.both,
            strict.bifrost_only,
            strict.reference_only,
            strict.neither,
        ));
    }
    if snapshot.manifest.snapshot_kind == crate::freeze::SnapshotKind::Development {
        let strict = aggregate(comparisons, |comparison| comparison.strict);
        output.push_str(&format!(
            "| **Pooled** | **{}** | **{}** | **{}** | **{}** | **{}** |\n",
            strict.shared, strict.both, strict.bifrost_only, strict.reference_only, strict.neither
        ));
    }
    output.push('\n');

    if snapshot.manifest.snapshot_kind == crate::freeze::SnapshotKind::Evaluation {
        return Ok(output);
    }

    output.push_str("## Strict sensitivity\n\n");
    output.push_str(
        "| Excluded reference profile | Bifrost exact | Reference exact | Difference |\n",
    );
    output.push_str("|---|---:|---:|---:|\n");
    for excluded in comparisons {
        let retained = aggregate(
            &comparisons
                .iter()
                .filter(|comparison| comparison.reference_id != excluded.reference_id)
                .collect::<Vec<_>>(),
            |comparison| comparison.strict,
        );
        if retained.shared == 0 {
            continue;
        }
        let bifrost_exact = retained.bifrost_only + retained.both;
        let reference_exact = retained.reference_only + retained.both;
        output.push_str(&format!(
            "| {} | {}/{} ({}) | {}/{} ({}) | {} pp |\n",
            excluded.reference_id,
            bifrost_exact,
            retained.shared,
            percentage(bifrost_exact, retained.shared),
            reference_exact,
            retained.shared,
            percentage(reference_exact, retained.shared),
            percentage_points(bifrost_exact, reference_exact, retained.shared),
        ));
    }
    Ok(output)
}

fn render_evaluation_audit(output: &mut String, snapshot: &Snapshot) -> Result<()> {
    let Some(audit) = snapshot.manifest.evaluation_audit.as_ref() else {
        return Ok(());
    };
    output.push_str("# Evaluation-only results\n\n");
    output.push_str("> **Partition:** `evaluation` only. These tables do not include or pool development cases.\n\n");
    output.push_str("## Evaluation scope and audit\n\n");
    output.push_str(&format!("- Freeze ID: `{}`\n", audit.freeze_id));
    output.push_str(&format!("- Bounded claim: {}\n", audit.claim_scope));
    output.push_str(&format!(
        "- Audited corpus: {} repositories and {} cases\n\n",
        audit.source_count, audit.case_count
    ));

    output.push_str("### Per-profile denominators\n\n");
    output.push_str("| Language | Reference profile | Registered adapter | Repositories | Cases | Population exclusions | Source-review replacements |\n");
    output.push_str("|---|---|---|---:|---:|---|---|\n");
    let bifrost = snapshot.bifrost()?;
    for selection in &audit.selection {
        let cases = bifrost
            .report
            .documents
            .iter()
            .filter(|document| document.language == selection.language)
            .map(|document| document.cases.len())
            .sum::<usize>();
        let profile = audit
            .target_profiles
            .iter()
            .find(|profile| {
                profile.language == selection.language
                    && profile.candidate_id == selection.candidate_id
            })
            .context("evaluation selection lacks a registered target profile")?;
        output.push_str(&format!(
            "| {} | `{}` | [`{}`](../{}) | {} | {} | {} | {} |\n",
            selection.language,
            selection.candidate_id,
            profile.profile,
            profile.profile,
            selection.selected_repositories,
            cases,
            render_audit_counts(
                selection.excluded_repositories,
                &selection.exclusion_reasons
            ),
            render_audit_counts(selection.replacements, &selection.replacement_reasons),
        ));
    }
    output.push('\n');

    output.push_str("### Hash-bound artifact provenance\n\n");
    output.push_str("| Artifact | File | SHA-256 |\n|---|---|---|\n");
    let artifacts = [
        ("Protocol", &audit.artifacts.protocol),
        ("Selection", &audit.artifacts.selection),
        ("Independent review", &audit.artifacts.review),
        ("Source lock", &audit.artifacts.source_lock),
    ];
    for (label, artifact) in artifacts {
        output.push_str(&format!(
            "| {label} | [`{}`](../{}) | `{}` |\n",
            artifact.file, artifact.file, artifact.sha256
        ));
    }
    for reviewer in &audit.reviewers {
        output.push_str(&format!(
            "| Reviewer `{}` | [`{}`](../{}) | `{}` |\n",
            reviewer.id, reviewer.file, reviewer.file, reviewer.sha256
        ));
    }
    output.push_str(&format!(
        "| Adjudication `{}` | [`{}`](../{}) | `{}` |\n\n",
        audit.adjudication.id,
        audit.adjudication.file,
        audit.adjudication.file,
        audit.adjudication.sha256
    ));
    Ok(())
}

fn render_audit_counts(total: usize, counts: &[crate::evaluation::EvaluationAuditCount]) -> String {
    if total == 0 {
        return "none".to_string();
    }
    let reasons = counts
        .iter()
        .map(|entry| format!("{} × {}", markdown_table_text(&entry.reason), entry.count))
        .collect::<Vec<_>>()
        .join("; ");
    if reasons.is_empty() {
        total.to_string()
    } else {
        format!("{total} ({reasons})")
    }
}

fn markdown_table_text(value: &str) -> String {
    value.replace('|', "\\|").replace(['\r', '\n'], " ")
}

fn render_location_comparison(
    output: &mut String,
    comparisons: &[Comparison],
    include_aggregate_views: bool,
) -> Result<()> {
    output.push_str("## Location-level precision and recall\n\n");
    output.push_str(
        "TP, FP, and FN are reported without true negatives. Strict precision counts every extra result; policy-adjusted precision excludes authored and policy-allowed extras.\n\n",
    );
    output.push_str("| Reference profile | Analyzer | Cases | TP | FP | FN");
    for descriptor in METRIC_DESCRIPTORS {
        output.push_str(&format!(" | {}", descriptor.label));
    }
    output.push_str(" |\n");
    output.push_str("|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n");
    for comparison in comparisons {
        let profile = format!(
            "{} {}",
            comparison.reference_name, comparison.reference_version
        );
        push_metric_row(output, &profile, "Bifrost", &comparison.bifrost_locations);
        push_metric_row(
            output,
            &profile,
            &comparison.reference_name,
            &comparison.reference_locations,
        );
    }
    if !include_aggregate_views {
        output.push('\n');
        return Ok(());
    }

    let mut pooled_bifrost = ProfileLocationMetrics::default();
    let mut pooled_reference = ProfileLocationMetrics::default();
    let mut equal_profile_bifrost = MetricAverageSet::default();
    let mut equal_profile_reference = MetricAverageSet::default();
    for comparison in comparisons {
        pooled_bifrost.merge(&comparison.bifrost_locations)?;
        pooled_reference.merge(&comparison.reference_locations)?;
        equal_profile_bifrost.add(metric_rates(&comparison.bifrost_locations.micro));
        equal_profile_reference.add(metric_rates(&comparison.reference_locations.micro));
    }
    push_metric_row(output, "**Pooled micro**", "**Bifrost**", &pooled_bifrost);
    push_metric_row(
        output,
        "**Pooled micro**",
        "**Reference**",
        &pooled_reference,
    );
    output.push('\n');

    output.push_str("### Macro and equal-profile views\n\n");
    output.push_str("| Aggregation | Analyzer");
    for descriptor in METRIC_DESCRIPTORS {
        output.push_str(&format!(" | {}", descriptor.label));
    }
    output.push_str(" |\n");
    output.push_str("|---|---|---:|---:|---:|---:|---:|---:|\n");
    push_rate_row(
        output,
        "Case macro",
        "Bifrost",
        pooled_bifrost.case_macro.rates(),
    );
    push_rate_row(
        output,
        "Case macro",
        "Reference",
        pooled_reference.case_macro.rates(),
    );
    push_rate_row(
        output,
        "Equal profile",
        "Bifrost",
        equal_profile_bifrost.rates(),
    );
    push_rate_row(
        output,
        "Equal profile",
        "Reference",
        equal_profile_reference.rates(),
    );
    output.push('\n');

    output.push_str("### Pooled range quality\n\n");
    output.push_str(
        "| Analyzer | Exact token | Containing | Line-only | Wrong location | Missing |\n",
    );
    output.push_str("|---|---:|---:|---:|---:|---:|\n");
    push_range_row(output, "Bifrost", &pooled_bifrost.micro);
    push_range_row(output, "Reference", &pooled_reference.micro);
    output.push('\n');
    Ok(())
}

fn push_metric_row(
    output: &mut String,
    profile: &str,
    analyzer: &str,
    metrics: &ProfileLocationMetrics,
) {
    let rates = metric_rates(&metrics.micro);
    output.push_str(&format!(
        "| {profile} | {analyzer} | {} | {} | {} | {}",
        metrics.cases,
        metrics.micro.true_positives,
        metrics.micro.false_positives,
        metrics.micro.false_negatives,
    ));
    push_rate_cells(output, rates);
    output.push_str(" |\n");
}

fn push_rate_row(output: &mut String, aggregation: &str, analyzer: &str, rates: MetricRates) {
    output.push_str(&format!("| {aggregation} | {analyzer}"));
    push_rate_cells(output, rates);
    output.push_str(" |\n");
}

fn push_rate_cells(output: &mut String, rates: MetricRates) {
    for descriptor in METRIC_DESCRIPTORS {
        let rate = rates.get(descriptor.kind);
        let rendered = match descriptor.format {
            MetricFormat::Percentage => rate_percentage(rate),
            MetricFormat::Decimal => rate_decimal(rate),
        };
        output.push_str(&format!(" | {rendered}"));
    }
}

fn push_range_row(output: &mut String, analyzer: &str, metrics: &LocationMetrics) {
    output.push_str(&format!(
        "| {analyzer} | {} | {} | {} | {} | {} |\n",
        metrics.range_quality.exact_token,
        metrics.range_quality.containing,
        metrics.range_quality.line_only,
        metrics.range_quality.wrong_location,
        metrics.range_quality.missing,
    ));
}

fn render_case_comparison(snapshot: &Snapshot, comparisons: &[Comparison]) -> String {
    let mut output = provenance_header(snapshot);
    output.push_str("## Separating strict-contract cases\n\n");
    output.push_str("### Exact only for Bifrost\n\n");
    output.push_str("| Reference profile | Case file | Language | Case |\n|---|---|---|---|\n");
    for comparison in comparisons {
        for case in &comparison.bifrost_only {
            output.push_str(&format!(
                "| {} | `{}` | {} | `{}` |\n",
                comparison.reference_id, case.case_file, case.language, case.id
            ));
        }
    }
    output.push_str("\n### Exact only for the reference server\n\n");
    output.push_str("| Reference profile | Case file | Language | Case |\n|---|---|---|---|\n");
    for comparison in comparisons {
        for case in &comparison.reference_only {
            output.push_str(&format!(
                "| {} | `{}` | {} | `{}` |\n",
                comparison.reference_id, case.case_file, case.language, case.id
            ));
        }
    }
    output
}

fn provenance_header(snapshot: &Snapshot) -> String {
    let mut output = format!(
        "<!-- GENERATED FILE. DO NOT EDIT.\nSnapshot: {} {}\nRevision: {}\nManifest SHA-256: {}\nGenerator: usagebench generate-results v{}\nInput reports:\n",
        snapshot.manifest.snapshot_kind,
        snapshot.manifest.version,
        snapshot.manifest.revision,
        snapshot.manifest_checksum,
        env!("CARGO_PKG_VERSION"),
    );
    for (candidate_id, report) in &snapshot.reports {
        output.push_str(&format!("- {}: {}\n", candidate_id, report.checksum));
    }
    output.push_str("-->\n\n");
    output
}

fn aggregate<T>(comparisons: &[T], select: impl Fn(&T) -> OutcomeTotals) -> OutcomeTotals {
    comparisons
        .iter()
        .fold(OutcomeTotals::default(), |mut total, comparison| {
            let next = select(comparison);
            total.shared += next.shared;
            total.both += next.both;
            total.bifrost_only += next.bifrost_only;
            total.reference_only += next.reference_only;
            total.neither += next.neither;
            total
        })
}

fn percentage(numerator: usize, denominator: usize) -> String {
    rate_percentage(ratio(numerator, denominator))
}

fn rate_percentage(rate: Option<f64>) -> String {
    rate.map(|rate| format!("{:.1}%", rate * 100.0))
        .unwrap_or_else(|| "n/a".to_string())
}

fn rate_decimal(rate: Option<f64>) -> String {
    rate.map(|rate| format!("{rate:.2}"))
        .unwrap_or_else(|| "n/a".to_string())
}

fn percentage_points(left: usize, right: usize, denominator: usize) -> String {
    format!(
        "{:+.1}",
        (left as f64 - right as f64) * 100.0 / denominator as f64
    )
}

fn hex_digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        evaluation::{
            EvaluationArtifactLink, EvaluationAuditArtifacts, EvaluationReleaseAudit,
            EvaluationReviewArtifact, EvaluationSelectionAudit, EvaluationTargetProfile,
        },
        freeze::{
            FreezeManifest, ManifestCandidate, ManifestDocument, ManifestReport, ScoringContract,
            SnapshotKind,
        },
        reproduction::{CandidateEvidenceLink, ReproductionClass},
        runners::{
            ContainerProvenance, ExecutableProvenance, ExecutionEnvironment, ExecutionMode,
            PlatformScope, RangeQualityTotals, ReferenceEnvironmentProvenance,
            RequiredDestinationTotals, ReturnedLocationTotals, RunInvocation, RunTotals,
            RunnerMetadata,
        },
        CorpusPartition, CorpusSelection, GroundTruthReviewStatus, ReferencePolicy,
    };
    use tempfile::tempdir;

    const REVISION: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn metric_rates_separate_policy_extras_from_false_positives() {
        let metrics = LocationMetrics {
            queries: 1,
            successful_queries: 1,
            required_locations: 2,
            true_positives: 2,
            false_positives: 2,
            successful_query_extras: 3,
            returned_locations: ReturnedLocationTotals {
                required: 2,
                policy_allowed: 1,
                related_unallowed: 1,
                unrelated: 1,
            },
            range_quality: RangeQualityTotals {
                exact_token: 1,
                containing: 1,
                wrong_location: 2,
                ..RangeQualityTotals::default()
            },
            ..LocationMetrics::default()
        };

        let rates = metric_rates(&metrics);

        assert_eq!(rates.destination_recall, Some(1.0));
        assert_eq!(rates.exact_token_recall, Some(0.5));
        assert_eq!(rates.strict_precision, Some(0.4));
        assert_eq!(rates.policy_adjusted_precision, Some(0.5));
        assert_eq!(rates.extra_result_burden, Some(3.0));
    }

    #[test]
    fn exact_set_rate_requires_every_query_in_the_case_to_be_exact() {
        let metrics = LocationMetrics {
            cases: 1,
            exact_set_cases: 0,
            queries: 2,
            exact_set_queries: 1,
            ..LocationMetrics::default()
        };

        assert_eq!(metric_rates(&metrics).exact_set_rate, Some(0.0));
    }

    #[test]
    fn case_macro_weights_cases_equally_instead_of_pooling_locations() {
        let mut profile = ProfileLocationMetrics::default();
        profile
            .add_case(&LocationMetrics {
                cases: 1,
                queries: 1,
                required_locations: 10,
                true_positives: 9,
                false_negatives: 1,
                returned_locations: ReturnedLocationTotals {
                    required: 9,
                    ..ReturnedLocationTotals::default()
                },
                range_quality: RangeQualityTotals {
                    exact_token: 9,
                    missing: 1,
                    ..RangeQualityTotals::default()
                },
                ..LocationMetrics::default()
            })
            .unwrap();
        profile
            .add_case(&LocationMetrics {
                cases: 1,
                queries: 1,
                required_locations: 1,
                false_negatives: 1,
                range_quality: RangeQualityTotals {
                    missing: 1,
                    ..RangeQualityTotals::default()
                },
                ..LocationMetrics::default()
            })
            .unwrap();

        assert_eq!(
            metric_rates(&profile.micro).destination_recall,
            Some(9.0 / 11.0)
        );
        assert_eq!(profile.case_macro.rates().destination_recall, Some(0.45));
    }

    #[test]
    fn generates_strict_and_compatibility_aware_pages_from_verified_reports() {
        let tempdir = tempdir().unwrap();
        let bifrost = sample_report(
            "bifrost",
            vec![
                (
                    "shared-exact",
                    CaseStatus::Passed,
                    RequiredDestinationStatus::Found,
                ),
                (
                    "bifrost-strict",
                    CaseStatus::Passed,
                    RequiredDestinationStatus::Found,
                ),
                (
                    "reference-strict",
                    CaseStatus::Failed,
                    RequiredDestinationStatus::Found,
                ),
                (
                    "neither",
                    CaseStatus::Failed,
                    RequiredDestinationStatus::Missing,
                ),
            ],
        );
        let reference = sample_report(
            "gopls",
            vec![
                (
                    "shared-exact",
                    CaseStatus::Passed,
                    RequiredDestinationStatus::Found,
                ),
                (
                    "bifrost-strict",
                    CaseStatus::PositionUnverified,
                    RequiredDestinationStatus::Found,
                ),
                (
                    "reference-strict",
                    CaseStatus::Passed,
                    RequiredDestinationStatus::Found,
                ),
                (
                    "neither",
                    CaseStatus::Failed,
                    RequiredDestinationStatus::Found,
                ),
            ],
        );
        let manifest = write_snapshot(tempdir.path(), bifrost, reference);
        let pages = generate_result_pages(&manifest).unwrap();

        assert!(pages
            .results
            .contains("| gopls 1.0.0 | 4 | 3/4 (75.0%) | 4/4 (100.0%) |"));
        assert!(pages
            .results
            .contains("| gopls 1.0.0 | 4 | 1 | 1 | 1 | 1 |"));
        assert!(pages
            .results
            .contains("## Location-level precision and recall"));
        assert!(pages.results.contains(
            "| gopls 1.0.0 | Bifrost | 4 | 3 | 0 | 1 | 75.0% | 50.0% | 100.0% | 100.0% | 50.0% | 0.00 |"
        ));
        assert!(pages.results.contains(
            "| gopls 1.0.0 | gopls | 4 | 4 | 0 | 0 | 100.0% | 50.0% | 100.0% | 100.0% | 50.0% | 0.00 |"
        ));
        assert!(pages.results.contains("| Case macro | Bifrost |"));
        assert!(pages.results.contains("| Equal profile | Reference |"));
        assert!(pages.results.contains("### Pooled range quality"));
        assert!(pages.case_comparison.contains("`bifrost-strict`"));
        assert!(pages.case_comparison.contains("`reference-strict`"));
        assert!(pages
            .results
            .contains("[canonical](../evidence/gopls-evidence.json)"));
        assert!(!pages.results.contains("/host/specific/root"));
        assert!(!pages.results.contains("/nonportable/path"));

        let generated = tempdir.path().join("generated");
        write_result_pages(&manifest, &generated, false).unwrap();
        write_result_pages(&manifest, &generated, true).unwrap();
    }

    #[test]
    fn rejects_legacy_reports_when_location_tables_are_requested() {
        let tempdir = tempdir().unwrap();
        let mut bifrost = sample_report(
            "bifrost",
            vec![("case", CaseStatus::Passed, RequiredDestinationStatus::Found)],
        );
        bifrost.usagebench_version = "0.1.0".to_string();
        bifrost.totals.location_metrics = None;
        for document in &mut bifrost.documents {
            for case in &mut document.cases {
                case.location_metrics = None;
            }
        }
        let manifest = write_snapshot(
            tempdir.path(),
            bifrost,
            sample_report(
                "gopls",
                vec![("case", CaseStatus::Passed, RequiredDestinationStatus::Found)],
            ),
        );

        let error = generate_result_pages(&manifest).unwrap_err();

        assert!(error.to_string().contains("candidate bifrost"));
        assert!(error.to_string().contains("UsageBench 0.1.0"));
        assert!(error.to_string().contains("UsageBench >=0.2.0"));
    }

    #[test]
    fn rejects_internally_inconsistent_location_metrics() {
        let tempdir = tempdir().unwrap();
        let mut bifrost = sample_report(
            "bifrost",
            vec![("case", CaseStatus::Passed, RequiredDestinationStatus::Found)],
        );
        bifrost
            .totals
            .location_metrics
            .as_mut()
            .unwrap()
            .false_positives = 1;
        let manifest = write_snapshot(
            tempdir.path(),
            bifrost,
            sample_report(
                "gopls",
                vec![("case", CaseStatus::Passed, RequiredDestinationStatus::Found)],
            ),
        );

        let error = generate_result_pages(&manifest).unwrap_err();

        assert!(error
            .to_string()
            .contains("candidate bifrost has invalid total location metrics"));
    }

    #[test]
    fn rejects_missing_case_level_location_metrics() {
        let mut report = sample_report(
            "bifrost",
            vec![("case", CaseStatus::Passed, RequiredDestinationStatus::Found)],
        );
        report.documents[0].cases[0].location_metrics = None;

        let error = validate_report_location_metrics("bifrost", &report).unwrap_err();

        assert!(error.to_string().contains("lacks location metrics"));
    }

    #[test]
    fn rejects_exact_set_claim_without_exact_range_evidence() {
        let mut report = sample_report(
            "bifrost",
            vec![("case", CaseStatus::Passed, RequiredDestinationStatus::Found)],
        );
        let metrics = report.documents[0].cases[0]
            .location_metrics
            .as_mut()
            .unwrap();
        metrics.range_quality.exact_token = 0;
        metrics.range_quality.containing = 1;
        report.totals.location_metrics = Some(metrics.clone());

        let error = validate_report_location_metrics("bifrost", &report).unwrap_err();

        assert!(error
            .to_string()
            .contains("inconsistent case-level exact-set"));
    }

    #[test]
    fn rejects_overflow_while_merging_case_metrics() {
        let mut report = sample_report(
            "bifrost",
            vec![
                (
                    "first",
                    CaseStatus::Passed,
                    RequiredDestinationStatus::Found,
                ),
                (
                    "second",
                    CaseStatus::Passed,
                    RequiredDestinationStatus::Found,
                ),
            ],
        );
        let huge = LocationMetrics {
            cases: 1,
            exact_set_cases: 1,
            queries: 1,
            successful_queries: 1,
            exact_set_queries: 1,
            required_locations: usize::MAX,
            true_positives: usize::MAX,
            returned_locations: ReturnedLocationTotals {
                required: usize::MAX,
                ..ReturnedLocationTotals::default()
            },
            range_quality: RangeQualityTotals {
                exact_token: usize::MAX,
                ..RangeQualityTotals::default()
            },
            ..LocationMetrics::default()
        };
        for case in &mut report.documents[0].cases {
            case.location_metrics = Some(huge.clone());
        }
        report.totals.location_metrics = Some(LocationMetrics::default());

        let error = validate_report_location_metrics("bifrost", &report).unwrap_err();

        assert!(error.to_string().contains("overflow while merging"));
    }

    #[test]
    fn rejects_a_tampered_report_before_generating_pages() {
        let tempdir = tempdir().unwrap();
        let manifest = write_snapshot(
            tempdir.path(),
            sample_report(
                "bifrost",
                vec![("case", CaseStatus::Passed, RequiredDestinationStatus::Found)],
            ),
            sample_report(
                "gopls",
                vec![("case", CaseStatus::Passed, RequiredDestinationStatus::Found)],
            ),
        );
        fs::write(tempdir.path().join("gopls.json"), b"{}\n").unwrap();

        let error = generate_result_pages(&manifest).unwrap_err();
        assert!(error.to_string().contains("checksum mismatch"));
    }

    #[test]
    fn rejects_a_report_from_a_different_release() {
        let tempdir = tempdir().unwrap();
        let manifest_path = write_snapshot(
            tempdir.path(),
            sample_report(
                "bifrost",
                vec![("case", CaseStatus::Passed, RequiredDestinationStatus::Found)],
            ),
            sample_report(
                "gopls",
                vec![("case", CaseStatus::Passed, RequiredDestinationStatus::Found)],
            ),
        );
        let report_path = tempdir.path().join("gopls.json");
        let mut report: RunReport =
            serde_json::from_slice(&fs::read(&report_path).unwrap()).unwrap();
        report.usagebench_release = Some("v9.9.9".to_string());
        let report_bytes = serde_json::to_vec_pretty(&report).unwrap();
        fs::write(&report_path, &report_bytes).unwrap();
        let mut manifest: FreezeManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest
            .reports
            .iter_mut()
            .find(|entry| entry.candidate_id == "gopls")
            .unwrap()
            .sha256 = hex_digest(&report_bytes);
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let error = generate_result_pages(&manifest_path).unwrap_err();
        assert!(error.to_string().contains("release does not match"));
    }

    #[test]
    fn rejects_a_report_relabelled_as_a_different_candidate() {
        let error = validate_candidate_report(
            &candidate("bifrost", "bifrost"),
            &sample_report(
                "gopls",
                vec![("case", CaseStatus::Passed, RequiredDestinationStatus::Found)],
            ),
        )
        .unwrap_err();
        assert!(error.to_string().contains("requires a Bifrost report"));
    }

    #[test]
    fn rejects_manifest_report_file_reuse() {
        let tempdir = tempdir().unwrap();
        let manifest_path = write_snapshot(
            tempdir.path(),
            sample_report(
                "bifrost",
                vec![("case", CaseStatus::Passed, RequiredDestinationStatus::Found)],
            ),
            sample_report(
                "gopls",
                vec![("case", CaseStatus::Passed, RequiredDestinationStatus::Found)],
            ),
        );
        let mut manifest: FreezeManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest.reports[1].file = "bifrost.json".to_string();
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let error = generate_result_pages(&manifest_path).unwrap_err();
        assert!(error.to_string().contains("reuses report file"));
    }

    #[test]
    fn rejects_missing_reproduction_evidence() {
        let tempdir = tempdir().unwrap();
        let manifest_path = write_snapshot(
            tempdir.path(),
            sample_report(
                "bifrost",
                vec![("case", CaseStatus::Passed, RequiredDestinationStatus::Found)],
            ),
            sample_report(
                "gopls",
                vec![("case", CaseStatus::Passed, RequiredDestinationStatus::Found)],
            ),
        );
        let mut manifest: FreezeManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest.candidate_evidence.pop();
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let error = generate_result_pages(&manifest_path).unwrap_err();
        assert!(error.to_string().contains("one unique evidence link"));
    }

    #[test]
    fn rejects_tampered_reproduction_evidence() {
        let tempdir = tempdir().unwrap();
        let manifest_path = write_snapshot(
            tempdir.path(),
            sample_report(
                "bifrost",
                vec![("case", CaseStatus::Passed, RequiredDestinationStatus::Found)],
            ),
            sample_report(
                "gopls",
                vec![("case", CaseStatus::Passed, RequiredDestinationStatus::Found)],
            ),
        );
        fs::write(tempdir.path().join("gopls-evidence.json"), b"{}\n").unwrap();

        let error = generate_result_pages(&manifest_path).unwrap_err();
        assert!(error
            .to_string()
            .contains("checksum mismatch for reproduction evidence"));
    }

    #[test]
    fn rejects_evaluation_snapshot_without_audit() {
        let tempdir = tempdir().unwrap();
        let manifest_path = write_snapshot(
            tempdir.path(),
            sample_report(
                "bifrost",
                vec![("case", CaseStatus::Passed, RequiredDestinationStatus::Found)],
            ),
            sample_report(
                "gopls",
                vec![("case", CaseStatus::Passed, RequiredDestinationStatus::Found)],
            ),
        );
        let mut manifest: FreezeManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest.snapshot_kind = SnapshotKind::Evaluation;
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let error = generate_result_pages(&manifest_path).unwrap_err();
        assert!(error.to_string().contains("missing its evaluation audit"));
    }

    #[test]
    fn rejects_tampered_evaluation_audit_summary() {
        let expected = sample_evaluation_audit();
        let mut recorded = expected.clone();
        recorded.claim_scope = "broader claim inserted after freeze".to_string();

        let error = validate_rebuilt_audit(&recorded, &expected).unwrap_err();
        assert!(error
            .to_string()
            .contains("does not match the hash-verified release evidence"));
    }

    #[test]
    fn rejects_same_count_evaluation_case_id_substitution() {
        let mut audit = sample_evaluation_audit();
        audit.target_profiles = vec![EvaluationTargetProfile {
            language: "go".to_string(),
            candidate_id: "gopls".to_string(),
            profile: "adapters/lsp/gopls.json".to_string(),
        }];
        let mut bifrost = sample_report(
            "bifrost",
            vec![("case", CaseStatus::Passed, RequiredDestinationStatus::Found)],
        );
        let mut gopls = sample_report(
            "gopls",
            vec![(
                "substituted-case",
                CaseStatus::Passed,
                RequiredDestinationStatus::Found,
            )],
        );
        for report in [&mut bifrost, &mut gopls] {
            report.case_files = audit.case_files.clone();
            report.documents[0].case_file = audit.case_files[0].clone();
            report.documents[0].corpus_partition = CorpusPartition::Evaluation;
            report.totals.development_cases = 0;
            report.totals.evaluation_cases = 1;
        }
        let manifest = FreezeManifest {
            schema_version: FREEZE_MANIFEST_SCHEMA_VERSION,
            snapshot_kind: SnapshotKind::Evaluation,
            version: "v0.2.0".to_string(),
            revision: REVISION.to_string(),
            scoring_contract: ScoringContract {
                benchmark_case_schema_version: 2,
                report_schema_version: 1,
                include_unsupported: false,
                include_definition_lookups: true,
            },
            candidates: vec![candidate("bifrost", "bifrost"), candidate("gopls", "lsp")],
            reports: Vec::new(),
            candidate_evidence: Vec::new(),
            corpus: vec![ManifestDocument {
                case_file: audit.case_files[0].clone(),
                language: "go".to_string(),
                partition: CorpusPartition::Evaluation,
                selection: CorpusSelection::PreRegistered,
                ground_truth_status: GroundTruthReviewStatus::IndependentlyReviewed,
            }],
            evaluation_audit: Some(audit),
        };
        let reports = BTreeMap::from([
            (
                "bifrost".to_string(),
                LoadedReport {
                    candidate: candidate("bifrost", "bifrost"),
                    report: bifrost,
                    checksum: "a".repeat(64),
                },
            ),
            (
                "gopls".to_string(),
                LoadedReport {
                    candidate: candidate("gopls", "lsp"),
                    report: gopls,
                    checksum: "b".repeat(64),
                },
            ),
        ]);

        let error = validate_snapshot_partition(&manifest, &reports).unwrap_err();

        assert!(error.to_string().contains("substituted"));
    }

    #[test]
    fn audit_counts_render_explicit_none() {
        assert_eq!(render_audit_counts(0, &[]), "none");
    }

    #[test]
    fn renders_evaluation_partition_scope_and_denominators() {
        let bifrost_report = sample_report(
            "bifrost",
            vec![("case", CaseStatus::Passed, RequiredDestinationStatus::Found)],
        );
        let bifrost_candidate = candidate("bifrost", "bifrost");
        let mut manifest = FreezeManifest {
            schema_version: FREEZE_MANIFEST_SCHEMA_VERSION,
            snapshot_kind: SnapshotKind::Evaluation,
            version: "v0.2.0".to_string(),
            revision: REVISION.to_string(),
            scoring_contract: ScoringContract {
                benchmark_case_schema_version: 2,
                report_schema_version: 1,
                include_unsupported: false,
                include_definition_lookups: true,
            },
            candidates: vec![bifrost_candidate.clone(), candidate("gopls", "lsp")],
            reports: Vec::new(),
            candidate_evidence: Vec::new(),
            corpus: Vec::new(),
            evaluation_audit: Some(sample_evaluation_audit()),
        };
        manifest.evaluation_audit.as_mut().unwrap().target_profiles =
            vec![EvaluationTargetProfile {
                language: "go".to_string(),
                candidate_id: "gopls".to_string(),
                profile: "adapters/lsp/gopls.json".to_string(),
            }];
        manifest.evaluation_audit.as_mut().unwrap().selection = vec![EvaluationSelectionAudit {
            language: "go".to_string(),
            candidate_id: "gopls".to_string(),
            ranked_repositories: 1,
            selected_repositories: 1,
            excluded_repositories: 0,
            exclusion_reasons: Vec::new(),
            replacements: 0,
            replacement_reasons: Vec::new(),
        }];
        let snapshot = Snapshot {
            manifest,
            manifest_checksum: "d".repeat(64),
            reports: BTreeMap::from([(
                "bifrost".to_string(),
                LoadedReport {
                    candidate: bifrost_candidate,
                    report: bifrost_report,
                    checksum: "e".repeat(64),
                },
            )]),
        };
        let mut output = String::new();

        render_evaluation_audit(&mut output, &snapshot).unwrap();

        assert!(output.contains("# Evaluation-only results"));
        assert!(output.contains("Freeze ID: `sample-v1`"));
        assert!(output.contains("| go | `gopls` |"));
        assert!(output.contains("| 1 | 1 | none | none |"));
    }

    #[test]
    fn distinguishes_duplicate_case_ids_from_different_documents() {
        let tempdir = tempdir().unwrap();
        let mut bifrost = sample_report(
            "bifrost",
            vec![(
                "same-id",
                CaseStatus::Passed,
                RequiredDestinationStatus::Found,
            )],
        );
        let mut reference = sample_report(
            "gopls",
            vec![(
                "same-id",
                CaseStatus::Passed,
                RequiredDestinationStatus::Found,
            )],
        );
        duplicate_document(&mut bifrost);
        duplicate_document(&mut reference);
        for document in &mut reference.documents {
            document.cases[0].status = CaseStatus::PositionUnverified;
        }
        reference.totals.passed = 0;
        reference.totals.failed = 2;
        let manifest = write_snapshot(tempdir.path(), bifrost, reference);

        let pages = generate_result_pages(&manifest).unwrap();
        assert!(pages
            .results
            .contains("| gopls 1.0.0 | 2 | 2/2 (100.0%) | 2/2 (100.0%) |"));
        assert!(pages.case_comparison.contains("second.yaml"));
    }

    fn duplicate_document(report: &mut RunReport) {
        let mut document = report.documents[0].clone();
        let location_metrics = document.cases[0].location_metrics.clone();
        document.case_file = "benchmarks/cases/second.yaml".to_string();
        report.case_files.push(document.case_file.clone());
        report.documents.push(document);
        report.totals.documents = 2;
        report.totals.cases = 2;
        report.totals.development_cases = 2;
        report.totals.passed = 2;
        report.totals.required_destinations.scoreable_cases = 2;
        report.totals.required_destinations.found = 2;
        if let (Some(total), Some(case)) = (
            report.totals.location_metrics.as_mut(),
            location_metrics.as_ref(),
        ) {
            total.merge(case);
        }
    }

    fn write_snapshot(directory: &Path, bifrost: RunReport, reference: RunReport) -> PathBuf {
        let bifrost_bytes = serde_json::to_vec_pretty(&bifrost).unwrap();
        let reference_bytes = serde_json::to_vec_pretty(&reference).unwrap();
        fs::write(directory.join("bifrost.json"), &bifrost_bytes).unwrap();
        fs::write(directory.join("gopls.json"), &reference_bytes).unwrap();
        let bifrost_evidence = write_canonical_evidence(directory, "bifrost", &bifrost_bytes);
        let gopls_evidence = write_canonical_evidence(directory, "gopls", &reference_bytes);
        let manifest = FreezeManifest {
            schema_version: FREEZE_MANIFEST_SCHEMA_VERSION,
            snapshot_kind: SnapshotKind::Development,
            version: "v0.2.0".to_string(),
            revision: REVISION.to_string(),
            scoring_contract: ScoringContract {
                benchmark_case_schema_version: 2,
                report_schema_version: 1,
                include_unsupported: false,
                include_definition_lookups: true,
            },
            candidates: vec![candidate("bifrost", "bifrost"), candidate("gopls", "lsp")],
            reports: vec![
                manifest_report("bifrost", "bifrost.json", &bifrost_bytes, &bifrost),
                manifest_report("gopls", "gopls.json", &reference_bytes, &reference),
            ],
            candidate_evidence: vec![bifrost_evidence, gopls_evidence],
            corpus: vec![ManifestDocument {
                case_file: "benchmarks/cases/sample.yaml".to_string(),
                language: "go".to_string(),
                partition: CorpusPartition::Development,
                selection: CorpusSelection::AnalyzerInformed,
                ground_truth_status: GroundTruthReviewStatus::LegacyUnattributed,
            }],
            evaluation_audit: None,
        };
        let path = directory.join("freeze-manifest.json");
        fs::write(&path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
        path
    }

    fn sample_evaluation_audit() -> EvaluationReleaseAudit {
        let artifact = EvaluationArtifactLink {
            file: "benchmarks/evaluation/sample.json".to_string(),
            sha256: "a".repeat(64),
        };
        EvaluationReleaseAudit {
            freeze_id: "sample-v1".to_string(),
            claim_scope: "bounded descriptive comparison".to_string(),
            target_profiles: Vec::new(),
            artifacts: EvaluationAuditArtifacts {
                protocol: artifact.clone(),
                selection: artifact.clone(),
                review: artifact.clone(),
                source_lock: artifact,
            },
            reviewers: vec![EvaluationReviewArtifact {
                id: "reviewer-a".to_string(),
                file: "benchmarks/evaluation/reviewer-a.json".to_string(),
                sha256: "b".repeat(64),
            }],
            adjudication: EvaluationReviewArtifact {
                id: "adjudication".to_string(),
                file: "benchmarks/evaluation/adjudication.json".to_string(),
                sha256: "c".repeat(64),
            },
            source_count: 1,
            case_files: vec!["benchmarks/cases/evaluation/sample.yaml".to_string()],
            case_ids_by_file: BTreeMap::from([(
                "benchmarks/cases/evaluation/sample.yaml".to_string(),
                vec!["case".to_string()],
            )]),
            case_count: 1,
            selection: Vec::new(),
        }
    }

    fn candidate(id: &str, runner: &str) -> ManifestCandidate {
        ManifestCandidate {
            id: id.to_string(),
            runner: runner.to_string(),
            name: id.to_string(),
            requested_version: "1.0.0".to_string(),
            source: "https://example.test".to_string(),
            revision: (runner == "bifrost").then(|| REVISION.to_string()),
            module_checksum: None,
            profile: (runner == "lsp").then(|| format!("adapters/lsp/{id}.json")),
            profile_sha256: None,
            resolved_version_prefix: None,
            reference_runner: Some(id.to_string()),
            advertised: true,
            reproduction_class: ReproductionClass::Canonical,
            runtime_networking: "disabled".to_string(),
            project_hydration: "fixture".to_string(),
        }
    }

    fn write_canonical_evidence(
        directory: &Path,
        id: &str,
        report_bytes: &[u8],
    ) -> CandidateEvidenceLink {
        let file = format!("{id}-evidence.json");
        let bytes = serde_json::to_vec_pretty(&serde_json::json!({
            "schemaVersion": 1,
            "candidateId": id,
            "primaryReport": {"file": format!("{id}.json"), "sha256": hex_digest(report_bytes)},
            "class": "canonical",
            "referenceRunner": id,
            "environmentVersion": "1",
            "definitionDigest": format!("sha256:{}", "c".repeat(64))
        }))
        .unwrap();
        fs::write(directory.join(&file), &bytes).unwrap();
        CandidateEvidenceLink {
            candidate_id: id.to_string(),
            class: ReproductionClass::Canonical,
            file,
            sha256: hex_digest(&bytes),
        }
    }

    fn manifest_report(id: &str, file: &str, bytes: &[u8], report: &RunReport) -> ManifestReport {
        ManifestReport {
            candidate_id: id.to_string(),
            file: file.to_string(),
            sha256: hex_digest(bytes),
            runner: report.runner.clone(),
            environment: report.environment.clone(),
            case_files: report.case_files.clone(),
            totals: report.totals.clone(),
        }
    }

    fn sample_report(
        runner: &str,
        cases: Vec<(&str, CaseStatus, RequiredDestinationStatus)>,
    ) -> RunReport {
        let exact = cases
            .iter()
            .filter(|(_, status, required)| {
                matches!(status, CaseStatus::Passed | CaseStatus::Improved)
                    && *required == RequiredDestinationStatus::Found
            })
            .count();
        let found = cases
            .iter()
            .filter(|(_, _, status)| *status == RequiredDestinationStatus::Found)
            .count();
        RunReport {
            usagebench_version: "0.2.0".to_string(),
            usagebench_revision: REVISION.to_string(),
            usagebench_release: Some("v0.2.0".to_string()),
            runner: RunnerMetadata {
                name: runner.to_string(),
                requested_version: "1.0.0".to_string(),
                resolved_version: "1.0.0".to_string(),
                source: "https://example.test".to_string(),
                adapter_version: "0.2.0".to_string(),
                capabilities: Vec::new(),
            },
            invocation: RunInvocation {
                include_unsupported: false,
                include_definition_lookups: true,
                profile: (runner != "bifrost").then(|| runner.to_string()),
                profile_sha256: None,
                case_id: None,
            },
            environment: ExecutionEnvironment {
                operating_system: "linux".to_string(),
                architecture: "x86_64".to_string(),
                execution_mode: ExecutionMode::Container,
                platform_scope: PlatformScope::CanonicalReference,
                reference_environment: Some(ReferenceEnvironmentProvenance {
                    version: "1".to_string(),
                    definition_digest: format!("sha256:{}", "c".repeat(64)),
                    canonical_platform: "linux/amd64".to_string(),
                }),
                container: Some(ContainerProvenance {
                    image_reference: format!("usagebench-reference:v0.2.0-env1-{runner}"),
                    image_digest: format!("sha256:{}", "d".repeat(64)),
                }),
                analyzer_executable: ExecutableProvenance {
                    command: runner.to_string(),
                    resolved_path: Some("/nonportable/path".to_string()),
                    sha256: Some("e".repeat(64)),
                },
                toolchains: BTreeMap::from([("rustc".to_string(), "rustc 1.97.0".to_string())]),
            },
            bifrost_repo: None,
            bifrost_commit: None,
            bifrost_resolved_commit: (runner == "bifrost").then(|| REVISION.to_string()),
            started_at_unix_seconds: 1,
            finished_at_unix_seconds: 2,
            case_files: vec!["benchmarks/cases/sample.yaml".to_string()],
            totals: RunTotals {
                documents: 1,
                cases: cases.len(),
                development_cases: cases.len(),
                passed: exact,
                failed: cases.len() - exact,
                required_destinations: RequiredDestinationTotals {
                    scoreable_cases: cases.len(),
                    found,
                    missing: cases.len() - found,
                    ..RequiredDestinationTotals::default()
                },
                location_metrics: Some(LocationMetrics {
                    cases: cases.len(),
                    exact_set_cases: exact,
                    queries: cases.len(),
                    successful_queries: found,
                    exact_set_queries: exact,
                    required_locations: cases.len(),
                    true_positives: found,
                    false_positives: 0,
                    false_negatives: cases.len() - found,
                    successful_query_extras: 0,
                    returned_locations: ReturnedLocationTotals {
                        required: found,
                        ..ReturnedLocationTotals::default()
                    },
                    range_quality: RangeQualityTotals {
                        exact_token: exact,
                        containing: found - exact,
                        missing: cases.len() - found,
                        ..RangeQualityTotals::default()
                    },
                }),
                ..RunTotals::default()
            },
            documents: vec![crate::runners::DocumentRunReport {
                case_file: "benchmarks/cases/sample.yaml".to_string(),
                language: "go".to_string(),
                source_root: "/host/specific/root".to_string(),
                corpus_partition: CorpusPartition::Development,
                corpus_selection: CorpusSelection::AnalyzerInformed,
                ground_truth_status: GroundTruthReviewStatus::LegacyUnattributed,
                reference_policy: ReferencePolicy::BindingsOptional,
                cases: cases
                    .into_iter()
                    .map(|(id, status, required_destination_status)| {
                        let found = required_destination_status == RequiredDestinationStatus::Found;
                        let exact =
                            found && matches!(status, CaseStatus::Passed | CaseStatus::Improved);
                        CaseRunReport {
                            id: id.to_string(),
                            status,
                            required_destination_status: Some(required_destination_status),
                            location_metrics: Some(LocationMetrics {
                                cases: 1,
                                exact_set_cases: usize::from(exact),
                                queries: 1,
                                successful_queries: usize::from(found),
                                exact_set_queries: usize::from(exact),
                                required_locations: 1,
                                true_positives: usize::from(found),
                                false_positives: 0,
                                false_negatives: usize::from(!found),
                                successful_query_extras: 0,
                                returned_locations: ReturnedLocationTotals {
                                    required: usize::from(found),
                                    ..ReturnedLocationTotals::default()
                                },
                                range_quality: RangeQualityTotals {
                                    exact_token: usize::from(exact),
                                    containing: usize::from(found && !exact),
                                    missing: usize::from(!found),
                                    ..RangeQualityTotals::default()
                                },
                            }),
                            expected_failure_reason: None,
                            not_planned_reason: None,
                            unsupported_reason: None,
                            declaration_to_usages: None,
                            usage_to_declaration: Vec::new(),
                            compatible_usage_to_declaration: Vec::new(),
                            type_lookups: Vec::new(),
                            diagnostics: Vec::new(),
                        }
                    })
                    .collect(),
            }],
        }
    }
}
