//! Immutable benchmark snapshot evidence.
//!
//! The candidate registry deliberately separates release identity from an LSP
//! profile's launch details. A profile can evolve its runner wiring, while a
//! freeze names the exact candidate release (and source revision where one is
//! available) that produced its report.

use crate::{
    evaluation::{validate_report_against_release_audit, EvaluationReleaseAudit},
    reproduction::{parse_evidence, validate_evidence, CandidateEvidenceLink, ReproductionClass},
    runners::{DocumentRunReport, RunReport, RunnerMetadata},
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
};

pub const FREEZE_MANIFEST_SCHEMA_VERSION: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotKind {
    Development,
    Evaluation,
}

impl std::fmt::Display for SnapshotKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Development => "development",
            Self::Evaluation => "evaluation",
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
    reproduction_class: Option<ReproductionClass>,
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
    pub evidence_paths: Vec<PathBuf>,
    pub evaluation_corpus: Option<PathBuf>,
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
    pub candidate_evidence: Vec<CandidateEvidenceLink>,
    pub corpus: Vec<ManifestDocument>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evaluation_audit: Option<EvaluationReleaseAudit>,
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
    pub reproduction_class: ReproductionClass,
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
    validate_release_tag(&options.version)?;
    validate_commit(&options.revision)?;
    if options.candidate_ids.is_empty() {
        bail!("at least one --candidates value is required");
    }
    let evaluation_audit = match options.snapshot_kind {
        SnapshotKind::Development => None,
        SnapshotKind::Evaluation => {
            let corpus = options.evaluation_corpus.as_deref().context(
                "evaluation snapshots require --evaluation-corpus pointing to the promoted corpus",
            )?;
            Some(crate::evaluation::build_release_audit(corpus)?)
        }
    };
    if options.candidate_ids.len() != options.report_paths.len() {
        bail!(
            "expected one --report for each selected candidate: {} candidate(s), {} report(s)",
            options.candidate_ids.len(),
            options.report_paths.len()
        );
    }
    if options.candidate_ids.len() != options.evidence_paths.len() {
        bail!(
            "expected one --evidence for each selected candidate: {} candidate(s), {} evidence file(s)",
            options.candidate_ids.len(),
            options.evidence_paths.len()
        );
    }

    let mut evidence_by_candidate = HashMap::new();
    for evidence_path in &options.evidence_paths {
        let evidence_bytes = fs::read(evidence_path)
            .with_context(|| format!("read reproduction evidence {}", evidence_path.display()))?;
        let evidence = parse_evidence(&evidence_bytes, evidence_path)?;
        if evidence_by_candidate
            .insert(evidence.candidate_id.clone(), evidence_path.clone())
            .is_some()
        {
            bail!(
                "candidate {} has duplicate reproduction evidence",
                evidence.candidate_id
            );
        }
    }

    let registry = load_registry(&options.candidates_file)?;
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
    let mut candidate_evidence = Vec::new();
    let mut documents = Vec::new();
    let mut contract: Option<ScoringContract> = None;

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
        let reproduction_class = candidate
            .reproduction_class
            .context("advertised candidate lacks a reproduction class")?;
        let evidence_path = evidence_by_candidate
            .get(candidate_id)
            .with_context(|| format!("candidate {candidate_id} lacks reproduction evidence"))?;
        let validated = validate_evidence(
            evidence_path,
            candidate_id,
            reproduction_class,
            candidate.reference_runner.as_deref(),
            &candidate.requested_version,
            candidate.profile_sha256.as_deref(),
            report_path,
            &report,
        )?;
        if !validated.accepted {
            bail!("candidate {candidate_id} reproduction evidence is not semantically equivalent");
        }
        candidate_evidence.push(validated.link);

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

    documents.sort_by(|left, right| left.case_file.cmp(&right.case_file));
    let corpus = documents;
    if corpus.is_empty() {
        bail!("selected reports did not execute any benchmark documents");
    }
    if options.snapshot_kind == SnapshotKind::Development {
        validate_development_corpus(&corpus)?;
    } else {
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

    Ok(FreezeManifest {
        schema_version: FREEZE_MANIFEST_SCHEMA_VERSION,
        snapshot_kind: options.snapshot_kind,
        version: options.version,
        revision: options.revision,
        scoring_contract: contract
            .context("selected reports did not provide a scoring contract")?,
        candidates,
        reports,
        candidate_evidence,
        corpus,
        evaluation_audit,
    })
}

pub fn write_manifest(output: &Path, manifest: &FreezeManifest) -> Result<()> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let encoded = serde_json::to_vec_pretty(manifest).context("serialize freeze manifest")?;
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
    if registry.schema_version != 2 {
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
            && (candidate.reproduction_class.is_none()
                || candidate
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
                "advertised candidate {} requires reproductionClass, runtimeNetworking, and projectHydration",
                candidate.id
            );
        }
        if candidate.reproduction_class == Some(ReproductionClass::Canonical)
            && candidate
                .reference_runner
                .as_deref()
                .unwrap_or("")
                .is_empty()
        {
            bail!(
                "canonical candidate {} requires a reference runner",
                candidate.id
            );
        }
        if candidate.reproduction_class != Some(ReproductionClass::Canonical)
            && candidate.reference_runner.is_some()
        {
            bail!("only canonical candidates may name a reference runner");
        }
        if !candidate.advertised
            && (candidate.reproduction_class.is_some()
                || candidate.runtime_networking.is_some()
                || candidate.project_hydration.is_some())
        {
            bail!(
                "unadvertised candidate {} cannot declare reproduction evidence",
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
    Ok(())
}

fn validate_evaluation_corpus(corpus: &[ManifestDocument]) -> Result<()> {
    for document in corpus {
        if document.partition != CorpusPartition::Evaluation
            || document.selection != CorpusSelection::PreRegistered
            || document.ground_truth_status != GroundTruthReviewStatus::IndependentlyReviewed
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
        reproduction_class: candidate
            .reproduction_class
            .expect("advertised candidates were validated"),
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
            resolved_version_prefix: Some("Apple clangd 21.0.0".to_string()),
            reference_runner: None,
            advertised: true,
            reproduction_class: Some(ReproductionClass::NativeTwoHost),
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
    fn development_manifest_freezes_report_identity_and_digest() {
        let tempdir = tempdir().unwrap();
        let registry = tempdir.path().join("candidates.json");
        std::fs::write(
            &registry,
            r#"{
              "schemaVersion": 2,
              "candidates": [{
                "id": "bifrost",
                "runner": "bifrost",
                "name": "Bifrost",
                "requestedVersion": "v0.8.8",
                "source": "https://github.com/BrokkAi/bifrost",
                "revision": "0123456789abcdef0123456789abcdef01234567",
                "advertised": true,
                "reproductionClass": "canonical",
                "referenceRunner": "bifrost",
                "runtimeNetworking": "disabled",
                "projectHydration": "fixture"
              }]
            }"#,
        )
        .unwrap();
        let report = tempdir.path().join("bifrost.json");
        std::fs::write(&report, serde_json::to_vec(&sample_report()).unwrap()).unwrap();
        let evidence = write_canonical_evidence(tempdir.path(), &report);

        let manifest = create_manifest(FreezeManifestOptions {
            snapshot_kind: SnapshotKind::Development,
            version: "v0.2.0".to_string(),
            revision: "0123456789abcdef0123456789abcdef01234567".to_string(),
            candidates_file: registry,
            candidate_ids: vec!["bifrost".to_string()],
            report_paths: vec![report],
            evidence_paths: vec![evidence],
            evaluation_corpus: None,
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
              "schemaVersion": 2,
              "candidates": [{
                "id": "bifrost",
                "runner": "bifrost",
                "name": "Bifrost",
                "requestedVersion": "v0.8.8",
                "source": "https://github.com/BrokkAi/bifrost",
                "revision": "0123456789abcdef0123456789abcdef01234567",
                "advertised": true,
                "reproductionClass": "canonical",
                "referenceRunner": "bifrost",
                "runtimeNetworking": "disabled",
                "projectHydration": "fixture"
              }]
            }"#,
        )
        .unwrap();
        let report = tempdir.path().join("bifrost.json");
        std::fs::write(&report, serde_json::to_vec(&sample_report()).unwrap()).unwrap();
        let evidence = write_canonical_evidence(tempdir.path(), &report);

        let error = create_manifest(FreezeManifestOptions {
            snapshot_kind: SnapshotKind::Evaluation,
            version: "v0.2.0".to_string(),
            revision: "0123456789abcdef0123456789abcdef01234567".to_string(),
            candidates_file: registry,
            candidate_ids: vec!["bifrost".to_string()],
            report_paths: vec![report],
            evidence_paths: vec![evidence],
            evaluation_corpus: None,
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
            requested_version: "v0.8.8".to_string(),
            source: "https://github.com/BrokkAi/bifrost".to_string(),
            revision: Some("0123456789abcdef0123456789abcdef01234567".to_string()),
            module_checksum: None,
            profile: None,
            profile_sha256: None,
            resolved_version_prefix: None,
            reference_runner: Some("bifrost".to_string()),
            advertised: true,
            reproduction_class: Some(ReproductionClass::Canonical),
            runtime_networking: Some("disabled".to_string()),
            project_hydration: Some("fixture".to_string()),
            ineligible_reason: None,
        }
    }

    fn write_canonical_evidence(directory: &Path, report: &Path) -> PathBuf {
        let report_bytes = std::fs::read(report).unwrap();
        let evidence = directory.join("bifrost-evidence.json");
        std::fs::write(
            &evidence,
            serde_json::to_vec_pretty(&json!({
                "schemaVersion": 1,
                "candidateId": "bifrost",
                "primaryReport": {
                    "file": "bifrost.json",
                    "sha256": hex_digest(&report_bytes)
                },
                "class": "canonical",
                "referenceRunner": "bifrost",
                "environmentVersion": "1",
                "definitionDigest": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
            }))
            .unwrap(),
        )
        .unwrap();
        evidence
    }
}
