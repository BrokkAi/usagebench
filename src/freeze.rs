//! Immutable benchmark snapshot evidence.
//!
//! The candidate registry deliberately separates release identity from an LSP
//! profile's launch details. A profile can evolve its runner wiring, while a
//! freeze names the exact candidate release (and source revision where one is
//! available) that produced its report.

use crate::{
    evaluation::{validate_report_against_release_audit, EvaluationReleaseAudit},
    promotion::{build_promotion_audit, validate_report_against_promotion, LegacyPromotionAudit},
    runners::{DocumentRunReport, ExecutionMode, PlatformScope, RunReport, RunnerMetadata},
    CorpusPartition, CorpusSelection, GroundTruthReviewStatus,
};
use anyhow::{bail, Context, Result};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

pub const FREEZE_MANIFEST_SCHEMA_VERSION: u32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotKind {
    Development,
    Evaluation,
    LegacyPromoted,
}

impl std::fmt::Display for SnapshotKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Development => "development",
            Self::Evaluation => "evaluation",
            Self::LegacyPromoted => "legacy_promoted",
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CandidateRegistry {
    schema_version: u32,
    candidates: Vec<Candidate>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Candidate {
    id: String,
    runner: CandidateRunner,
    name: String,
    requested_version: String,
    source: String,
    #[serde(default)]
    revision: Option<String>,
    #[serde(default)]
    module_checksum: Option<String>,
    #[serde(default)]
    profile: Option<String>,
    #[serde(default)]
    profile_sha256: Option<String>,
    #[serde(default)]
    resolved_version_prefix: Option<String>,
    #[serde(default)]
    reference_runner: Option<String>,
    advertised: bool,
    #[serde(default)]
    runtime_networking: Option<String>,
    #[serde(default)]
    project_hydration: Option<String>,
    #[serde(default)]
    ineligible_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CandidateRunner {
    Bifrost,
    Lsp,
}

#[derive(Debug, Clone)]
pub struct FreezeManifestOptions {
    pub snapshot_kind: SnapshotKind,
    pub version: String,
    pub revision: String,
    pub candidates_file: PathBuf,
    pub candidate_ids: Vec<String>,
    pub report_paths: Vec<PathBuf>,
    pub evaluation_corpus: Option<PathBuf>,
    pub promotion_manifest: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FreezePhaseTimings {
    pub schema_version: u32,
    pub candidate_registry_validation_ms: Option<u64>,
    pub corpus_hashing_and_validation_ms: Option<u64>,
    pub report_validation_ms: Option<u64>,
    pub manifest_writing_ms: Option<u64>,
    pub total_ms: Option<u64>,
    pub completed: bool,
}

impl FreezePhaseTimings {
    pub fn new() -> Self {
        Self {
            schema_version: 1,
            ..Self::default()
        }
    }
}

struct PhaseTimer<'a> {
    started: Instant,
    destination: Option<&'a mut Option<u64>>,
    label: &'static str,
}

impl<'a> PhaseTimer<'a> {
    fn new(destination: Option<&'a mut Option<u64>>, label: &'static str) -> Self {
        Self {
            started: Instant::now(),
            destination,
            label,
        }
    }
}

impl Drop for PhaseTimer<'_> {
    fn drop(&mut self) {
        let elapsed = self.started.elapsed().as_millis() as u64;
        if let Some(destination) = self.destination.as_deref_mut() {
            *destination = Some(elapsed);
            eprintln!("phase timing: {} {} ms", self.label, elapsed);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FreezeManifest {
    pub schema_version: u32,
    pub snapshot_kind: SnapshotKind,
    pub version: String,
    pub revision: String,
    pub scoring_contract: ScoringContract,
    pub candidates: Vec<ManifestCandidate>,
    pub reports: Vec<ManifestReport>,
    pub corpus: Vec<ManifestDocument>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evaluation_audit: Option<EvaluationReleaseAudit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legacy_promotion_audit: Option<LegacyPromotionAudit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoringContract {
    pub benchmark_case_schema_version: u32,
    pub report_schema_version: u32,
    pub include_unsupported: bool,
    pub include_definition_lookups: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestCandidate {
    pub id: String,
    pub runner: String,
    pub name: String,
    pub requested_version: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_checksum: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_version_prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_runner: Option<String>,
    pub advertised: bool,
    pub runtime_networking: String,
    pub project_hydration: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestReport {
    pub candidate_id: String,
    pub file: String,
    pub sha256: String,
    pub runner: RunnerMetadata,
    pub environment: crate::runners::ExecutionEnvironment,
    pub case_files: Vec<String>,
    pub totals: crate::runners::RunTotals,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestDocument {
    pub case_file: String,
    pub language: String,
    pub partition: CorpusPartition,
    pub selection: CorpusSelection,
    pub ground_truth_status: GroundTruthReviewStatus,
}

pub fn create_manifest(options: FreezeManifestOptions) -> Result<FreezeManifest> {
    create_manifest_inner(options, None)
}

pub fn create_manifest_profiled(
    options: FreezeManifestOptions,
    timings: &mut FreezePhaseTimings,
) -> Result<FreezeManifest> {
    create_manifest_inner(options, Some(timings))
}

fn create_manifest_inner(
    options: FreezeManifestOptions,
    mut timings: Option<&mut FreezePhaseTimings>,
) -> Result<FreezeManifest> {
    validate_release_tag(&options.version)?;
    validate_commit(&options.revision)?;
    if options.candidate_ids.is_empty() {
        bail!("at least one --candidates value is required");
    }
    let corpus_timer = PhaseTimer::new(
        timings
            .as_deref_mut()
            .map(|timings| &mut timings.corpus_hashing_and_validation_ms),
        "corpus hashing and validation",
    );
    let (evaluation_audit, legacy_promotion_audit) = match options.snapshot_kind {
        SnapshotKind::Development => (None, None),
        SnapshotKind::Evaluation => {
            let corpus = options.evaluation_corpus.as_deref().context(
                "evaluation snapshots require --evaluation-corpus pointing to the promoted corpus",
            )?;
            (Some(crate::evaluation::build_release_audit(corpus)?), None)
        }
        SnapshotKind::LegacyPromoted => {
            let manifest = options
                .promotion_manifest
                .as_deref()
                .context("legacy-promoted snapshots require --promotion-manifest")?;
            (None, Some(build_promotion_audit(manifest)?))
        }
    };
    drop(corpus_timer);
    if options.candidate_ids.len() != options.report_paths.len() {
        bail!(
            "expected one --report for each selected candidate: {} candidate(s), {} report(s)",
            options.candidate_ids.len(),
            options.report_paths.len()
        );
    }
    let candidate_registry_timer = PhaseTimer::new(
        timings
            .as_deref_mut()
            .map(|timings| &mut timings.candidate_registry_validation_ms),
        "candidate registry validation",
    );
    let registry = load_registry(&options.candidates_file)?;
    drop(candidate_registry_timer);
    let mut known_candidates = HashMap::new();
    for candidate in registry.candidates {
        if known_candidates
            .insert(candidate.id.clone(), candidate)
            .is_some()
        {
            bail!("candidate registry contains a duplicate ID");
        }
    }

    let mut selected_ids = BTreeSet::new();
    let mut candidates = Vec::new();
    let mut reports = Vec::new();
    let mut documents = Vec::new();
    let mut contract: Option<ScoringContract> = None;

    let report_timer = PhaseTimer::new(
        timings
            .as_deref_mut()
            .map(|timings| &mut timings.report_validation_ms),
        "report validation",
    );
    for (candidate_id, report_path) in options.candidate_ids.iter().zip(&options.report_paths) {
        if !selected_ids.insert(candidate_id.clone()) {
            bail!("candidate {candidate_id} was selected more than once");
        }
        let candidate = known_candidates
            .get(candidate_id)
            .with_context(|| format!("unknown candidate {candidate_id}"))?;
        if !candidate.advertised {
            bail!(
                "candidate {} is not advertised for public results{}",
                candidate.id,
                candidate
                    .ineligible_reason
                    .as_deref()
                    .map(|reason| format!(": {reason}"))
                    .unwrap_or_default()
            );
        }

        let report_bytes = fs::read(report_path)
            .with_context(|| format!("read candidate report {}", report_path.display()))?;
        let report: RunReport = serde_json::from_slice(&report_bytes)
            .with_context(|| format!("parse candidate report {}", report_path.display()))?;
        validate_report(candidate, &report, &options.revision, &options.version)?;
        if let Some(audit) = &evaluation_audit {
            validate_report_against_release_audit(&report, audit, &candidate.id)?;
        }
        if let Some(audit) = &legacy_promotion_audit {
            validate_report_against_promotion(&report, audit)?;
        }
        let report_contract = ScoringContract {
            benchmark_case_schema_version: 2,
            report_schema_version: crate::runners::RUN_REPORT_SCHEMA_VERSION,
            include_unsupported: report.invocation.include_unsupported,
            include_definition_lookups: report.invocation.include_definition_lookups,
        };
        if let Some(expected_contract) = &contract {
            if expected_contract != &report_contract {
                bail!(
                    "candidate {} used a different scoring contract from the earlier report",
                    candidate.id
                );
            }
        } else {
            contract = Some(report_contract);
        }

        for document in report.documents.iter().map(manifest_document) {
            if !documents.contains(&document) {
                documents.push(document);
            }
        }
        reports.push(ManifestReport {
            candidate_id: candidate.id.clone(),
            file: report_file_name(report_path)?,
            sha256: hex_digest(&report_bytes),
            runner: report.runner,
            environment: report.environment,
            case_files: report.case_files,
            totals: report.totals,
        });
        candidates.push(manifest_candidate(candidate));
    }
    drop(report_timer);

    documents.sort_by(|left, right| left.case_file.cmp(&right.case_file));
    let corpus = documents;
    if corpus.is_empty() {
        bail!("selected reports did not execute any benchmark documents");
    }
    match options.snapshot_kind {
        SnapshotKind::Development => validate_development_corpus(&corpus)?,
        SnapshotKind::LegacyPromoted => {
            validate_development_corpus(&corpus)?;
            let audit = legacy_promotion_audit
                .as_ref()
                .expect("promotion audit was constructed");
            let actual = corpus
                .iter()
                .map(|d| d.case_file.as_str())
                .collect::<BTreeSet<_>>();
            let expected = audit
                .case_ids_by_file
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>();
            if actual != expected {
                bail!(
                    "legacy-promoted reports must contain exactly the promotion manifest documents"
                );
            }
        }
        SnapshotKind::Evaluation => {
            validate_evaluation_corpus(&corpus)?;
            let expected_candidates = std::iter::once("bifrost".to_string())
                .chain(
                    evaluation_audit
                        .as_ref()
                        .expect("evaluation audit was constructed")
                        .target_profiles
                        .iter()
                        .map(|profile| profile.candidate_id.clone()),
                )
                .collect::<BTreeSet<_>>();
            if selected_ids != expected_candidates {
                bail!(
                "evaluation snapshot candidates must exactly match Bifrost plus protocol targets: expected {:?}, got {:?}",
                expected_candidates,
                selected_ids
            );
            }
        }
    }

    Ok(FreezeManifest {
        schema_version: FREEZE_MANIFEST_SCHEMA_VERSION,
        snapshot_kind: options.snapshot_kind,
        version: options.version,
        revision: options.revision,
        scoring_contract: contract
            .context("selected reports did not provide a scoring contract")?,
        candidates,
        reports,
        corpus,
        evaluation_audit,
        legacy_promotion_audit,
    })
}

pub fn write_manifest(output: &Path, manifest: &FreezeManifest) -> Result<()> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let encoded = serde_json::to_vec_pretty(manifest).context("serialize freeze manifest")?;
    fs::write(output, encoded).with_context(|| format!("write {}", output.display()))
}

pub fn write_phase_timings(output: &Path, timings: &FreezePhaseTimings) -> Result<()> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let encoded = serde_json::to_vec_pretty(timings).context("serialize freeze phase timings")?;
    fs::write(output, encoded).with_context(|| format!("write {}", output.display()))
}

fn load_registry(path: &Path) -> Result<CandidateRegistry> {
    let bytes =
        fs::read(path).with_context(|| format!("read candidate registry {}", path.display()))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("parse candidate registry {}", path.display()))?;
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../schema/candidate-registry.schema.json"))
            .context("parse embedded candidate registry schema")?;
    let compiled = jsonschema::JSONSchema::compile(&schema)
        .map_err(|error| anyhow::anyhow!("compile candidate registry schema: {error}"))?;
    if let Err(errors) = compiled.validate(&value) {
        let messages = errors
            .take(8)
            .map(|error| error.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        bail!(
            "candidate registry {} violates its schema: {}",
            path.display(),
            messages
        );
    }
    let registry: CandidateRegistry = serde_json::from_value(value)
        .with_context(|| format!("decode candidate registry {}", path.display()))?;
    if registry.schema_version != 3 {
        bail!(
            "unsupported candidate registry schema version {}",
            registry.schema_version
        );
    }
    for candidate in &registry.candidates {
        if candidate.id.is_empty()
            || candidate.name.is_empty()
            || candidate.requested_version.is_empty()
            || candidate.source.is_empty()
        {
            bail!("candidate registry entries require ID, name, requestedVersion, and source");
        }
        if candidate.runner == CandidateRunner::Lsp
            && candidate.profile.as_deref().unwrap_or("").is_empty()
        {
            bail!("LSP candidate {} requires a profile", candidate.id);
        }
        if candidate.advertised && candidate.ineligible_reason.is_some() {
            bail!(
                "advertised candidate {} cannot have an ineligible reason",
                candidate.id
            );
        }
        if candidate.advertised
            && (candidate
                .runtime_networking
                .as_deref()
                .unwrap_or("")
                .is_empty()
                || candidate
                    .project_hydration
                    .as_deref()
                    .unwrap_or("")
                    .is_empty())
        {
            bail!(
                "advertised candidate {} requires runtimeNetworking and projectHydration",
                candidate.id
            );
        }
        if !candidate.advertised
            && (candidate.runtime_networking.is_some()
                || candidate.project_hydration.is_some()
                || candidate.reference_runner.is_some())
        {
            bail!(
                "unadvertised candidate {} cannot declare release execution metadata",
                candidate.id
            );
        }
        if candidate.runner == CandidateRunner::Lsp
            && candidate.revision.is_some()
            && candidate
                .module_checksum
                .as_deref()
                .unwrap_or("")
                .is_empty()
        {
            bail!(
                "LSP candidate {} records a revision without a verifiable module checksum",
                candidate.id
            );
        }
    }
    Ok(registry)
}

fn validate_report(
    candidate: &Candidate,
    report: &RunReport,
    revision: &str,
    version: &str,
) -> Result<()> {
    report.ensure_complete()?;
    if report.totals.documents == 0 || report.totals.cases == 0 {
        bail!(
            "candidate {} report did not execute any cases",
            candidate.id
        );
    }
    if report.totals.errors > 0 {
        bail!(
            "candidate {} report contains execution errors",
            candidate.id
        );
    }
    if report.usagebench_revision != revision {
        bail!(
            "candidate {} report was produced by UsageBench {}, expected {}",
            candidate.id,
            report.usagebench_revision,
            revision
        );
    }
    if report.usagebench_release.as_deref() != Some(version) {
        bail!(
            "candidate {} report release does not match snapshot {}",
            candidate.id,
            version
        );
    }
    match candidate.runner {
        CandidateRunner::Bifrost => {
            if report.runner.name != "bifrost" {
                bail!("candidate {} requires a Bifrost report", candidate.id);
            }
            let expected_revision = candidate
                .revision
                .as_deref()
                .context("Bifrost candidates require a pinned revision")?;
            if report.bifrost_resolved_commit.as_deref() != Some(expected_revision) {
                bail!(
                    "candidate {} resolved Bifrost revision does not match registry",
                    candidate.id
                );
            }
        }
        CandidateRunner::Lsp => {
            let profile = candidate.profile.as_deref().expect("validated profile");
            let profile_id = Path::new(profile)
                .file_stem()
                .and_then(|name| name.to_str())
                .context("candidate LSP profile has no file stem")?;
            if report.invocation.profile.as_deref() != Some(profile_id) {
                bail!(
                    "candidate {} report used a different LSP profile",
                    candidate.id
                );
            }
            if report.invocation.profile_sha256 != candidate.profile_sha256 {
                bail!(
                    "candidate {} report used a different LSP profile checksum",
                    candidate.id
                );
            }
            if report.runner.requested_version != candidate.requested_version
                || report.runner.source != candidate.source
            {
                bail!(
                    "candidate {} report identity does not match registry",
                    candidate.id
                );
            }
            if candidate
                .resolved_version_prefix
                .as_deref()
                .is_some_and(|prefix| !report.runner.resolved_version.starts_with(prefix))
            {
                bail!(
                    "candidate {} resolved implementation does not match registry",
                    candidate.id
                );
            }
        }
    }
    if candidate.reference_runner.is_some() {
        validate_canonical_environment(&candidate.id, report)?;
    }
    Ok(())
}

pub(crate) fn validate_canonical_environment(candidate_id: &str, report: &RunReport) -> Result<()> {
    let environment = &report.environment;
    if environment.execution_mode != ExecutionMode::Container
        || environment.platform_scope != PlatformScope::CanonicalReference
        || environment.operating_system != "linux"
        || environment.architecture != "x86_64"
    {
        bail!(
            "candidate {} reference report must use the canonical linux/amd64 container",
            candidate_id
        );
    }
    let reference = environment
        .reference_environment
        .as_ref()
        .context("canonical reference report lacks reference-environment provenance")?;
    if reference.version.trim().is_empty()
        || reference.canonical_platform != "linux/amd64"
        || !is_prefixed_sha256(&reference.definition_digest)
    {
        bail!(
            "candidate {} reference-environment provenance is incomplete",
            candidate_id
        );
    }
    let container = environment
        .container
        .as_ref()
        .context("canonical reference report lacks container provenance")?;
    if container.image_reference.trim().is_empty() || !is_prefixed_sha256(&container.image_digest) {
        bail!(
            "candidate {} container provenance is incomplete",
            candidate_id
        );
    }
    if container.image_reference.starts_with("ghcr.io/")
        && !container
            .image_reference
            .rsplit_once('@')
            .is_some_and(|(_, digest)| is_prefixed_sha256(digest))
    {
        bail!(
            "candidate {} registry image reference is not bound to an immutable digest",
            candidate_id
        );
    }
    if environment.analyzer_executable.command.trim().is_empty()
        || !environment
            .analyzer_executable
            .sha256
            .as_deref()
            .is_some_and(is_raw_sha256)
    {
        bail!(
            "candidate {} analyzer executable provenance is incomplete",
            candidate_id
        );
    }
    if environment.toolchains.is_empty()
        || environment
            .toolchains
            .iter()
            .any(|(name, version)| name.trim().is_empty() || version.trim().is_empty())
    {
        bail!(
            "candidate {} toolchain provenance is incomplete",
            candidate_id
        );
    }
    Ok(())
}

fn is_prefixed_sha256(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(is_raw_sha256)
}

fn is_raw_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_evaluation_corpus(corpus: &[ManifestDocument]) -> Result<()> {
    for document in corpus {
        if document.partition != CorpusPartition::Evaluation
            || document.selection != CorpusSelection::PreRegistered
            || !matches!(
                document.ground_truth_status,
                GroundTruthReviewStatus::HumanAdjudicatedAgentPanel
                    | GroundTruthReviewStatus::IndependentlyReviewed
            )
        {
            bail!(
                "evaluation snapshot requires promoted evaluation metadata; {} is not eligible",
                document.case_file
            );
        }
    }
    Ok(())
}

fn validate_development_corpus(corpus: &[ManifestDocument]) -> Result<()> {
    for document in corpus {
        if document.partition != CorpusPartition::Development {
            bail!(
                "development snapshot cannot include evaluation document {}",
                document.case_file
            );
        }
    }
    Ok(())
}

fn manifest_candidate(candidate: &Candidate) -> ManifestCandidate {
    ManifestCandidate {
        id: candidate.id.clone(),
        runner: match candidate.runner {
            CandidateRunner::Bifrost => "bifrost",
            CandidateRunner::Lsp => "lsp",
        }
        .to_string(),
        name: candidate.name.clone(),
        requested_version: candidate.requested_version.clone(),
        source: candidate.source.clone(),
        revision: candidate.revision.clone(),
        module_checksum: candidate.module_checksum.clone(),
        profile: candidate.profile.clone(),
        profile_sha256: candidate.profile_sha256.clone(),
        resolved_version_prefix: candidate.resolved_version_prefix.clone(),
        reference_runner: candidate.reference_runner.clone(),
        advertised: candidate.advertised,
        runtime_networking: candidate
            .runtime_networking
            .clone()
            .expect("advertised candidates were validated"),
        project_hydration: candidate
            .project_hydration
            .clone()
            .expect("advertised candidates were validated"),
    }
}

fn manifest_document(document: &DocumentRunReport) -> ManifestDocument {
    ManifestDocument {
        case_file: document.case_file.clone(),
        language: document.language.clone(),
        partition: document.corpus_partition,
        selection: document.corpus_selection,
        ground_truth_status: document.ground_truth_status,
    }
}

fn validate_release_tag(version: &str) -> Result<()> {
    let Some(version) = version.strip_prefix('v') else {
        bail!("snapshot version must match vMAJOR.MINOR.PATCH");
    };
    if version.split('.').count() != 3
        || version
            .split('.')
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        bail!("snapshot version must match vMAJOR.MINOR.PATCH");
    }
    Ok(())
}

fn validate_commit(revision: &str) -> Result<()> {
    if revision.len() != 40 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("snapshot revision must be an exact 40-character Git commit");
    }
    Ok(())
}

fn report_file_name(path: &Path) -> Result<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .with_context(|| format!("report path {} has no file name", path.display()))
}

fn hex_digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn validates_release_tag_shape() {
        assert!(validate_release_tag("v1.2.3").is_ok());
        assert!(validate_release_tag("1.2.3").is_err());
        assert!(validate_release_tag("v1.2").is_err());
    }

    #[test]
    fn validates_exact_commit_shape() {
        assert!(validate_commit("0123456789abcdef0123456789abcdef01234567").is_ok());
        assert!(validate_commit("01234567").is_err());
    }

    #[test]
    fn rejects_report_from_a_different_release() {
        let candidate = sample_bifrost_candidate();
        let mut report: RunReport = serde_json::from_value(sample_report()).unwrap();
        report.usagebench_release = Some("v0.1.0".to_string());

        let error = validate_report(
            &candidate,
            &report,
            "0123456789abcdef0123456789abcdef01234567",
            "v0.2.0",
        )
        .unwrap_err();
        assert!(error.to_string().contains("release does not match"));
    }

    #[test]
    fn rejects_wrong_resolved_lsp_implementation() {
        let candidate = Candidate {
            id: "apple-clangd-21".to_string(),
            runner: CandidateRunner::Lsp,
            name: "Apple clangd".to_string(),
            requested_version: "21.0.0".to_string(),
            source: "https://developer.apple.com/xcode/".to_string(),
            revision: None,
            module_checksum: None,
            profile: Some("adapters/lsp/apple-clangd-21.json".to_string()),
            profile_sha256: Some("f".repeat(64)),
            resolved_version_prefix: Some("Apple clangd version 21.0.0".to_string()),
            reference_runner: None,
            advertised: true,
            runtime_networking: Some("disabled".to_string()),
            project_hydration: Some("fixture".to_string()),
            ineligible_reason: None,
        };
        let mut report: RunReport = serde_json::from_value(sample_report()).unwrap();
        report.runner.name = "clangd".to_string();
        report.runner.requested_version = "21.0.0".to_string();
        report.runner.resolved_version = "clangd version 22.1.6".to_string();
        report.runner.source = "https://developer.apple.com/xcode/".to_string();
        report.invocation.profile = Some("apple-clangd-21".to_string());
        report.invocation.profile_sha256 = Some("f".repeat(64));

        let error = validate_report(
            &candidate,
            &report,
            "0123456789abcdef0123456789abcdef01234567",
            "v0.2.0",
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("resolved implementation does not match"));
    }

    #[test]
    fn accepts_exact_apple_clangd_21_resolved_banner() {
        let candidate = Candidate {
            id: "apple-clangd-21".to_string(),
            runner: CandidateRunner::Lsp,
            name: "Apple clangd".to_string(),
            requested_version: "21.0.0".to_string(),
            source: "https://developer.apple.com/xcode/".to_string(),
            revision: None,
            module_checksum: None,
            profile: Some("adapters/lsp/apple-clangd-21.json".to_string()),
            profile_sha256: Some("f".repeat(64)),
            resolved_version_prefix: Some("Apple clangd version 21.0.0".to_string()),
            reference_runner: None,
            advertised: true,
            runtime_networking: Some("disabled".to_string()),
            project_hydration: Some("fixture".to_string()),
            ineligible_reason: None,
        };
        let mut report: RunReport = serde_json::from_value(sample_report()).unwrap();
        report.runner.name = "clangd".to_string();
        report.runner.requested_version = candidate.requested_version.clone();
        report.runner.resolved_version =
            "Apple clangd version 21.0.0 (clang-2100.1.1.101) mac+xpc arm64-apple-darwin25.5.0"
                .to_string();
        report.runner.source = candidate.source.clone();
        report.invocation.profile = Some("apple-clangd-21".to_string());
        report.invocation.profile_sha256 = candidate.profile_sha256.clone();

        validate_report(
            &candidate,
            &report,
            "0123456789abcdef0123456789abcdef01234567",
            "v0.2.0",
        )
        .unwrap();
    }

    #[test]
    fn accepts_one_identity_checked_native_lsp_report() {
        let candidate = Candidate {
            id: "pyright".to_string(),
            runner: CandidateRunner::Lsp,
            name: "Pyright".to_string(),
            requested_version: "1.1.411".to_string(),
            source: "https://github.com/microsoft/pyright/releases/tag/1.1.411".to_string(),
            revision: None,
            module_checksum: None,
            profile: Some("adapters/lsp/pyright.json".to_string()),
            profile_sha256: Some("f".repeat(64)),
            resolved_version_prefix: None,
            reference_runner: None,
            advertised: true,
            runtime_networking: Some("required by npx".to_string()),
            project_hydration: Some("npx resolves Pyright".to_string()),
            ineligible_reason: None,
        };
        let mut report: RunReport = serde_json::from_value(sample_report()).unwrap();
        report.runner.name = "pyright".to_string();
        report.runner.requested_version = candidate.requested_version.clone();
        report.runner.resolved_version = "pyright 1.1.411".to_string();
        report.runner.source = candidate.source.clone();
        report.invocation.profile = Some("pyright".to_string());
        report.invocation.profile_sha256 = candidate.profile_sha256.clone();
        report.environment.execution_mode = ExecutionMode::Native;
        report.environment.platform_scope = PlatformScope::HostSpecific;
        report.environment.reference_environment = None;
        report.environment.container = None;

        validate_report(
            &candidate,
            &report,
            "0123456789abcdef0123456789abcdef01234567",
            "v0.2.0",
        )
        .unwrap();
    }

    #[test]
    fn accepts_identity_checked_native_bifrost_without_reference_runner() {
        let mut candidate = sample_bifrost_candidate();
        candidate.reference_runner = None;
        let mut report: RunReport = serde_json::from_value(sample_report()).unwrap();
        report.environment.execution_mode = ExecutionMode::Native;
        report.environment.platform_scope = PlatformScope::HostSpecific;
        report.environment.reference_environment = None;
        report.environment.container = None;

        validate_report(
            &candidate,
            &report,
            "0123456789abcdef0123456789abcdef01234567",
            "v0.2.0",
        )
        .unwrap();
    }

    #[test]
    fn canonical_candidate_still_requires_reference_environment_provenance() {
        let candidate = sample_bifrost_candidate();
        let mut report: RunReport = serde_json::from_value(sample_report()).unwrap();
        report.environment.execution_mode = ExecutionMode::Native;
        report.environment.platform_scope = PlatformScope::HostSpecific;
        report.environment.reference_environment = None;
        report.environment.container = None;

        let error = validate_report(
            &candidate,
            &report,
            "0123456789abcdef0123456789abcdef01234567",
            "v0.2.0",
        )
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("must use the canonical linux/amd64 container"));
    }

    #[test]
    fn canonical_registry_reference_requires_an_immutable_digest() {
        let mut report: RunReport = serde_json::from_value(sample_report()).unwrap();
        report
            .environment
            .container
            .as_mut()
            .unwrap()
            .image_reference = "ghcr.io/brokkai/usagebench-reference:bifrost-latest".to_string();

        let error = validate_canonical_environment("bifrost", &report).unwrap_err();

        assert!(error.to_string().contains("immutable digest"));
    }

    #[test]
    fn development_manifest_freezes_report_identity_and_digest() {
        let tempdir = tempdir().unwrap();
        let registry = tempdir.path().join("candidates.json");
        std::fs::write(
            &registry,
            r#"{
              "schemaVersion": 3,
              "candidates": [{
                "id": "bifrost",
                "runner": "bifrost",
                "name": "Bifrost",
                "requestedVersion": "v0.9.3",
                "source": "https://github.com/BrokkAi/bifrost",
                "revision": "0123456789abcdef0123456789abcdef01234567",
                "advertised": true,
                "referenceRunner": "bifrost",
                "runtimeNetworking": "disabled",
                "projectHydration": "fixture"
              }]
            }"#,
        )
        .unwrap();
        let report = tempdir.path().join("bifrost.json");
        std::fs::write(&report, serde_json::to_vec(&sample_report()).unwrap()).unwrap();
        let manifest = create_manifest(FreezeManifestOptions {
            snapshot_kind: SnapshotKind::Development,
            version: "v0.2.0".to_string(),
            revision: "0123456789abcdef0123456789abcdef01234567".to_string(),
            candidates_file: registry,
            candidate_ids: vec!["bifrost".to_string()],
            report_paths: vec![report],
            evaluation_corpus: None,
            promotion_manifest: None,
        })
        .unwrap();

        assert_eq!(manifest.candidates[0].id, "bifrost");
        assert_eq!(manifest.reports[0].sha256.len(), 64);
        assert_eq!(manifest.corpus[0].partition, CorpusPartition::Development);
        assert!(manifest.evaluation_audit.is_none());
        assert!(serde_json::to_value(&manifest).unwrap()["evaluationAudit"].is_null());
    }

    #[test]
    fn evaluation_manifest_rejects_development_documents() {
        let tempdir = tempdir().unwrap();
        let registry = tempdir.path().join("candidates.json");
        std::fs::write(
            &registry,
            r#"{
              "schemaVersion": 3,
              "candidates": [{
                "id": "bifrost",
                "runner": "bifrost",
                "name": "Bifrost",
                "requestedVersion": "v0.9.3",
                "source": "https://github.com/BrokkAi/bifrost",
                "revision": "0123456789abcdef0123456789abcdef01234567",
                "advertised": true,
                "referenceRunner": "bifrost",
                "runtimeNetworking": "disabled",
                "projectHydration": "fixture"
              }]
            }"#,
        )
        .unwrap();
        let report = tempdir.path().join("bifrost.json");
        std::fs::write(&report, serde_json::to_vec(&sample_report()).unwrap()).unwrap();
        let error = create_manifest(FreezeManifestOptions {
            snapshot_kind: SnapshotKind::Evaluation,
            version: "v0.2.0".to_string(),
            revision: "0123456789abcdef0123456789abcdef01234567".to_string(),
            candidates_file: registry,
            candidate_ids: vec!["bifrost".to_string()],
            report_paths: vec![report],
            evaluation_corpus: None,
            promotion_manifest: None,
        })
        .unwrap_err();

        assert!(error.to_string().contains("require --evaluation-corpus"));
    }

    #[test]
    fn development_manifest_rejects_evaluation_documents() {
        let error = validate_development_corpus(&[ManifestDocument {
            case_file: "benchmarks/cases/evaluation/real-project-v1/go-01.yaml".to_string(),
            language: "go".to_string(),
            partition: CorpusPartition::Evaluation,
            selection: CorpusSelection::PreRegistered,
            ground_truth_status: GroundTruthReviewStatus::IndependentlyReviewed,
        }])
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("development snapshot cannot include evaluation document"));
    }

    #[test]
    fn checked_in_registry_matches_profiles_and_reference_environment() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let registry = load_registry(&root.join("adapters/candidates.json")).unwrap();
        let reference_environment: serde_json::Value = serde_json::from_slice(
            &std::fs::read(root.join("containers/reference/v1/manifest.json")).unwrap(),
        )
        .unwrap();

        for candidate in registry.candidates {
            if let Some(profile) = &candidate.profile {
                let profile_bytes = std::fs::read(root.join(profile)).unwrap();
                let profile: serde_json::Value = serde_json::from_slice(&profile_bytes).unwrap();
                assert_eq!(
                    profile["requestedVersion"].as_str(),
                    Some(candidate.requested_version.as_str()),
                    "profile version drift for {}",
                    candidate.id
                );
                assert_eq!(
                    profile["source"].as_str(),
                    Some(candidate.source.as_str()),
                    "profile source drift for {}",
                    candidate.id
                );
                if let Some(expected_sha256) = candidate.profile_sha256.as_deref() {
                    assert_eq!(
                        hex_digest(&profile_bytes),
                        expected_sha256,
                        "profile checksum drift for {}",
                        candidate.id
                    );
                }
            }
            let Some(reference_runner) = candidate.reference_runner.as_deref() else {
                continue;
            };
            let analyzer = &reference_environment["runners"][reference_runner]["analyzer"];
            assert_eq!(
                analyzer["requestedVersion"].as_str(),
                Some(candidate.requested_version.as_str()),
                "reference version drift for {}",
                candidate.id
            );
            assert_eq!(
                analyzer["source"].as_str(),
                Some(candidate.source.as_str()),
                "reference source drift for {}",
                candidate.id
            );
            if let Some(revision) = candidate.revision.as_deref() {
                assert_eq!(
                    analyzer["revision"].as_str(),
                    Some(revision),
                    "reference revision drift for {}",
                    candidate.id
                );
            }
            if let Some(module_checksum) = candidate.module_checksum.as_deref() {
                assert_eq!(
                    analyzer["moduleChecksum"].as_str(),
                    Some(module_checksum),
                    "reference module checksum drift for {}",
                    candidate.id
                );
            }
        }
    }

    #[test]
    fn v030_evaluation_registry_uses_public_native_bifrost_identity() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let registry = load_registry(
            &root.join("benchmarks/evaluation/real-project-v2/candidates-v0.3.0.json"),
        )
        .unwrap();
        let bifrost = registry
            .candidates
            .iter()
            .find(|candidate| candidate.id == "bifrost")
            .unwrap();

        assert_eq!(bifrost.requested_version, "v0.10.2");
        assert_eq!(bifrost.source, "https://github.com/BrokkAi/bifrost");
        assert_eq!(
            bifrost.revision.as_deref(),
            Some("d1a7c0cc1cf58d0c0789476ad42a92318bb8da49")
        );
        assert!(bifrost.reference_runner.is_none());
        assert_eq!(
            registry
                .candidates
                .iter()
                .map(|candidate| candidate.id.as_str())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([
                "apple-clangd-21",
                "bifrost",
                "eclipse-jdtls",
                "rust-analyzer",
            ])
        );
    }

    fn sample_report() -> serde_json::Value {
        json!({
            "usagebenchVersion": "0.1.0",
            "usagebenchRevision": "0123456789abcdef0123456789abcdef01234567",
            "usagebenchRelease": "v0.2.0",
            "runner": {
                "name": "bifrost",
                "requestedVersion": "0123456789abcdef0123456789abcdef01234567",
                "resolvedVersion": "0123456789abcdef0123456789abcdef01234567",
                "source": "https://github.com/BrokkAi/bifrost",
                "adapterVersion": "0.1.0",
                "capabilities": []
            },
            "invocation": {
                "includeUnsupported": false,
                "includeDefinitionLookups": true
            },
            "environment": {
                "operatingSystem": "linux",
                "architecture": "x86_64",
                "executionMode": "container",
                "platformScope": "canonical_reference",
                "referenceEnvironment": {
                    "version": "1",
                    "definitionDigest": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
                    "canonicalPlatform": "linux/amd64"
                },
                "container": {
                    "imageReference": "usagebench-reference:v0.2.0-env1-bifrost",
                    "imageDigest": "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
                },
                "analyzerExecutable": {
                    "command": "bifrost",
                    "resolvedPath": "/usr/local/bin/bifrost",
                    "sha256": "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
                },
                "toolchains": {"rustc": "rustc 1.97.0"}
            },
            "bifrostResolvedCommit": "0123456789abcdef0123456789abcdef01234567",
            "startedAtUnixSeconds": 1,
            "finishedAtUnixSeconds": 2,
            "caseFiles": ["benchmarks/cases/sample.yaml"],
            "totals": {
                "documents": 1,
                "cases": 1,
                "developmentCases": 1,
                "evaluationCases": 0,
                "passed": 1,
                "nearMisses": 0,
                "positionUnverified": 0,
                "improved": 0,
                "failed": 0,
                "expectedFailures": 0,
                "notPlanned": 0,
                "unsupported": 0,
                "skipped": 0,
                "errors": 0,
                "requiredDestinations": {
                    "scoreableCases": 1,
                    "found": 1,
                    "missing": 0,
                    "notPlanned": 0,
                    "unsupported": 0,
                    "skipped": 0,
                    "errors": 0,
                    "unreported": 0
                }
            },
            "documents": [{
                "caseFile": "benchmarks/cases/sample.yaml",
                "language": "rust",
                "sourceRoot": "fixtures/sample",
                "corpusPartition": "development",
                "corpusSelection": "analyzer_informed",
                "groundTruthStatus": "legacy_unattributed",
                "referencePolicy": "bindings_optional",
                "cases": []
            }]
        })
    }

    fn sample_bifrost_candidate() -> Candidate {
        Candidate {
            id: "bifrost".to_string(),
            runner: CandidateRunner::Bifrost,
            name: "Bifrost".to_string(),
            requested_version: "v0.9.3".to_string(),
            source: "https://github.com/BrokkAi/bifrost".to_string(),
            revision: Some("0123456789abcdef0123456789abcdef01234567".to_string()),
            module_checksum: None,
            profile: None,
            profile_sha256: None,
            resolved_version_prefix: None,
            reference_runner: Some("bifrost".to_string()),
            advertised: true,
            runtime_networking: Some("disabled".to_string()),
            project_hydration: Some("fixture".to_string()),
            ineligible_reason: None,
        }
    }
}
