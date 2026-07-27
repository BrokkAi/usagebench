//! Deterministic public-result pages derived from frozen benchmark evidence.
//!
//! This module deliberately reads the release manifest and its raw reports
//! directly. It never accepts copied totals as input: a generated page is only
//! as trustworthy as the immutable report bytes whose digests it verifies.

use crate::{
    freeze::{FreezeManifest, ManifestCandidate, FREEZE_MANIFEST_SCHEMA_VERSION},
    runners::{CaseRunReport, CaseStatus, RequiredDestinationStatus, RunReport},
};
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
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

    let comparisons = references
        .iter()
        .map(|reference| compare_reports(bifrost, reference))
        .collect::<Result<Vec<_>>>()?;

    Ok(GeneratedResultPages {
        results: render_results(&snapshot, &comparisons),
        case_comparison: render_case_comparison(&snapshot, &comparisons),
    })
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
        validate_candidate_report(&candidate, &report)?;
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
    Ok(Snapshot {
        manifest,
        manifest_checksum: hex_digest(&manifest_bytes),
        reports,
    })
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
                || report.runner.requested_version != candidate.requested_version
                || report.runner.source != candidate.source
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
    }

    if strict.shared == 0 {
        bail!(
            "Bifrost and {} do not share any strict-scoreable cases",
            reference.candidate.id
        );
    }
    Ok(Comparison {
        reference_id: reference.candidate.id.clone(),
        reference_name: reference.candidate.name.clone(),
        reference_version: reference.candidate.requested_version.clone(),
        strict,
        required,
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

fn render_results(snapshot: &Snapshot, comparisons: &[Comparison]) -> String {
    let mut output = provenance_header(snapshot);
    output.push_str("## Snapshot inputs\n\n");
    output.push_str(
        "| Candidate | Runner | Requested version | Profile | Environment | Report SHA-256 |\n",
    );
    output.push_str("|---|---|---|---|---|---|\n");
    for (candidate_id, loaded) in &snapshot.reports {
        let environment = &loaded.report.environment;
        output.push_str(&format!(
            "| {} | {} | {} | {} | {}/{:?}/{:?}/{:?} | `{}` |\n",
            candidate_id,
            loaded.report.runner.name,
            loaded.report.runner.requested_version,
            loaded.candidate.profile.as_deref().unwrap_or("—"),
            environment.operating_system,
            environment.architecture,
            environment.execution_mode,
            environment.platform_scope,
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
    let required = aggregate(comparisons, |comparison| comparison.required);
    output.push_str(&format!(
        "| **Pooled** | **{}** | **{}/{} ({})** | **{}/{} ({})** |\n\n",
        required.shared,
        required.bifrost_only + required.both,
        required.shared,
        percentage(required.bifrost_only + required.both, required.shared),
        required.reference_only + required.both,
        required.shared,
        percentage(required.reference_only + required.both, required.shared),
    ));

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
    let strict = aggregate(comparisons, |comparison| comparison.strict);
    output.push_str(&format!(
        "| **Pooled** | **{}** | **{}** | **{}** | **{}** | **{}** |\n\n",
        strict.shared, strict.both, strict.bifrost_only, strict.reference_only, strict.neither
    ));

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
    output
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
    if denominator == 0 {
        "n/a".to_string()
    } else {
        format!("{:.1}%", numerator as f64 * 100.0 / denominator as f64)
    }
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
        freeze::{
            FreezeManifest, ManifestCandidate, ManifestDocument, ManifestReport, ScoringContract,
            SnapshotKind,
        },
        runners::{
            ExecutableProvenance, ExecutionEnvironment, ExecutionMode, PlatformScope,
            RequiredDestinationTotals, RunInvocation, RunTotals, RunnerMetadata,
        },
        CorpusPartition, CorpusSelection, GroundTruthReviewStatus, ReferencePolicy,
    };
    use tempfile::tempdir;

    const REVISION: &str = "0123456789abcdef0123456789abcdef01234567";

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
        assert!(pages.case_comparison.contains("`bifrost-strict`"));
        assert!(pages.case_comparison.contains("`reference-strict`"));
        assert!(!pages.results.contains("/host/specific/root"));
        assert!(!pages.results.contains("/nonportable/path"));

        let generated = tempdir.path().join("generated");
        write_result_pages(&manifest, &generated, false).unwrap();
        write_result_pages(&manifest, &generated, true).unwrap();
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
        document.case_file = "benchmarks/cases/second.yaml".to_string();
        report.case_files.push(document.case_file.clone());
        report.documents.push(document);
        report.totals.documents = 2;
        report.totals.cases = 2;
        report.totals.development_cases = 2;
        report.totals.passed = 2;
        report.totals.required_destinations.scoreable_cases = 2;
        report.totals.required_destinations.found = 2;
    }

    fn write_snapshot(directory: &Path, bifrost: RunReport, reference: RunReport) -> PathBuf {
        let bifrost_bytes = serde_json::to_vec_pretty(&bifrost).unwrap();
        let reference_bytes = serde_json::to_vec_pretty(&reference).unwrap();
        fs::write(directory.join("bifrost.json"), &bifrost_bytes).unwrap();
        fs::write(directory.join("gopls.json"), &reference_bytes).unwrap();
        let manifest = FreezeManifest {
            schema_version: FREEZE_MANIFEST_SCHEMA_VERSION,
            snapshot_kind: SnapshotKind::Development,
            version: "v0.1.0".to_string(),
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
            corpus: vec![ManifestDocument {
                case_file: "benchmarks/cases/sample.yaml".to_string(),
                language: "go".to_string(),
                partition: CorpusPartition::Development,
                selection: CorpusSelection::AnalyzerInformed,
                ground_truth_status: GroundTruthReviewStatus::LegacyUnattributed,
            }],
        };
        let path = directory.join("freeze-manifest.json");
        fs::write(&path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
        path
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
            .filter(|(_, status, _)| matches!(status, CaseStatus::Passed | CaseStatus::Improved))
            .count();
        let found = cases
            .iter()
            .filter(|(_, _, status)| *status == RequiredDestinationStatus::Found)
            .count();
        RunReport {
            usagebench_version: "0.1.0".to_string(),
            usagebench_revision: REVISION.to_string(),
            usagebench_release: Some("v0.1.0".to_string()),
            runner: RunnerMetadata {
                name: runner.to_string(),
                requested_version: "1.0.0".to_string(),
                resolved_version: "1.0.0".to_string(),
                source: "https://example.test".to_string(),
                adapter_version: "0.1.0".to_string(),
                capabilities: Vec::new(),
            },
            invocation: RunInvocation {
                include_unsupported: false,
                include_definition_lookups: true,
                profile: (runner != "bifrost").then(|| runner.to_string()),
                case_id: None,
            },
            environment: ExecutionEnvironment {
                operating_system: "linux".to_string(),
                architecture: "x86_64".to_string(),
                execution_mode: ExecutionMode::Container,
                platform_scope: PlatformScope::CanonicalReference,
                reference_environment: None,
                container: None,
                analyzer_executable: ExecutableProvenance {
                    command: runner.to_string(),
                    resolved_path: Some("/nonportable/path".to_string()),
                    sha256: None,
                },
                toolchains: BTreeMap::new(),
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
                    .map(|(id, status, required_destination_status)| CaseRunReport {
                        id: id.to_string(),
                        status,
                        required_destination_status: Some(required_destination_status),
                        expected_failure_reason: None,
                        not_planned_reason: None,
                        unsupported_reason: None,
                        declaration_to_usages: None,
                        usage_to_declaration: Vec::new(),
                        compatible_usage_to_declaration: Vec::new(),
                        type_lookups: Vec::new(),
                        diagnostics: Vec::new(),
                    })
                    .collect(),
            }],
        }
    }
}
