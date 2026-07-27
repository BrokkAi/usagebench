//! Immutable benchmark snapshot evidence.
//!
//! The candidate registry deliberately separates release identity from an LSP
//! profile's launch details. A profile can evolve its runner wiring, while a
//! freeze names the exact candidate release (and source revision where one is
//! available) that produced its report.

use crate::{
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

pub const FREEZE_MANIFEST_SCHEMA_VERSION: u32 = 1;

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
    reference_runner: Option<String>,
    eligible_for_freeze: bool,
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
    if options.candidate_ids.len() != options.report_paths.len() {
        bail!(
            "expected one --report for each selected candidate: {} candidate(s), {} report(s)",
            options.candidate_ids.len(),
            options.report_paths.len()
        );
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
    let mut documents = Vec::new();
    let mut contract: Option<ScoringContract> = None;

    for (candidate_id, report_path) in options.candidate_ids.iter().zip(&options.report_paths) {
        if !selected_ids.insert(candidate_id.clone()) {
            bail!("candidate {candidate_id} was selected more than once");
        }
        let candidate = known_candidates
            .get(candidate_id)
            .with_context(|| format!("unknown candidate {candidate_id}"))?;
        if !candidate.eligible_for_freeze {
            bail!(
                "candidate {} is not eligible for automated freeze{}",
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
        validate_report(candidate, &report, &options.revision)?;

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
    if options.snapshot_kind == SnapshotKind::Evaluation {
        validate_evaluation_corpus(&corpus)?;
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
    let registry: CandidateRegistry = serde_json::from_slice(
        &fs::read(path).with_context(|| format!("read candidate registry {}", path.display()))?,
    )
    .with_context(|| format!("parse candidate registry {}", path.display()))?;
    if registry.schema_version != 1 {
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
        if candidate.eligible_for_freeze && candidate.ineligible_reason.is_some() {
            bail!(
                "candidate {} cannot be eligible and have an ineligible reason",
                candidate.id
            );
        }
        if candidate.eligible_for_freeze
            && candidate
                .reference_runner
                .as_deref()
                .unwrap_or("")
                .is_empty()
        {
            bail!(
                "eligible candidate {} requires a protected reference runner",
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

fn validate_report(candidate: &Candidate, report: &RunReport, revision: &str) -> Result<()> {
    if report.usagebench_revision != revision {
        bail!(
            "candidate {} report was produced by UsageBench {}, expected {}",
            candidate.id,
            report.usagebench_revision,
            revision
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
            if report.runner.requested_version != candidate.requested_version
                || report.runner.source != candidate.source
            {
                bail!(
                    "candidate {} report identity does not match registry",
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
    fn development_manifest_freezes_report_identity_and_digest() {
        let tempdir = tempdir().unwrap();
        let registry = tempdir.path().join("candidates.json");
        std::fs::write(
            &registry,
            r#"{
              "schemaVersion": 1,
              "candidates": [{
                "id": "bifrost",
                "runner": "bifrost",
                "name": "Bifrost",
                "requestedVersion": "v0.8.8",
                "source": "https://github.com/BrokkAi/bifrost",
                "revision": "0123456789abcdef0123456789abcdef01234567",
                "referenceRunner": "bifrost",
                "eligibleForFreeze": true
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
        })
        .unwrap();

        assert_eq!(manifest.candidates[0].id, "bifrost");
        assert_eq!(manifest.reports[0].sha256.len(), 64);
        assert_eq!(manifest.corpus[0].partition, CorpusPartition::Development);
    }

    #[test]
    fn evaluation_manifest_rejects_development_documents() {
        let tempdir = tempdir().unwrap();
        let registry = tempdir.path().join("candidates.json");
        std::fs::write(
            &registry,
            r#"{
              "schemaVersion": 1,
              "candidates": [{
                "id": "bifrost",
                "runner": "bifrost",
                "name": "Bifrost",
                "requestedVersion": "v0.8.8",
                "source": "https://github.com/BrokkAi/bifrost",
                "revision": "0123456789abcdef0123456789abcdef01234567",
                "referenceRunner": "bifrost",
                "eligibleForFreeze": true
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
        })
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("evaluation snapshot requires promoted evaluation metadata"));
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
                let profile: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(root.join(profile)).unwrap()).unwrap();
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
                "executionMode": "native",
                "platformScope": "host_specific",
                "analyzerExecutable": {"command": "bifrost"},
                "toolchains": {}
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
}
