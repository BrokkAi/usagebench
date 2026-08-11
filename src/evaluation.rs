//! Evidence contracts for prospectively selected real-project evaluation slices.
//!
//! Evaluation case YAML remains the runner input. These manifests bind that
//! YAML to the protocol, independent review record, and materialized source
//! identities that were fixed before analyzer outcomes are inspected.

use crate::runners::RunReport;
use crate::{
    benchmark_source_path, is_exact_git_commit, BenchmarkDocument, CorpusPartition,
    GroundTruthReview, GroundTruthReviewStatus, Location, NavigationOperation, Source, SymbolKind,
    SymbolLocation,
};
use anyhow::{anyhow, bail, Context, Result};
use flate2::read::GzDecoder;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
};
use url::Url;

const EVALUATION_PROTOCOL_SCHEMA: &str = include_str!("../schema/evaluation-protocol.schema.json");
const EVALUATION_POPULATION_SCHEMA: &str =
    include_str!("../schema/evaluation-population.schema.json");
const EVALUATION_SELECTION_SCHEMA: &str =
    include_str!("../schema/evaluation-selection.schema.json");
const EVALUATION_REVIEW_SCHEMA: &str = include_str!("../schema/evaluation-review.schema.json");
const SOURCE_MATERIALIZATION_SCHEMA: &str =
    include_str!("../schema/source-materialization.schema.json");
const MAX_COMPRESSED_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_EXPANDED_ARCHIVE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_ARCHIVE_FILE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 200_000;
const MAX_PAX_HEADER_BYTES: u64 = 1024;
const MAX_TAR_STREAM_BYTES: u64 =
    MAX_EXPANDED_ARCHIVE_BYTES + (MAX_ARCHIVE_ENTRIES as u64 * 1024) + 1024 * 1024;

#[derive(Debug, Default)]
pub(crate) struct EvaluationEvidenceCache {
    validated_reviews: BTreeMap<ReviewValidationKey, ValidatedReviewEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ReviewValidationKey {
    selection_file: String,
    selection_sha256: String,
    review_file: String,
    review_sha256: String,
    source_lock_file: String,
    source_lock_sha256: String,
}

#[derive(Debug, Clone)]
struct ValidatedReviewEvidence {
    ground_truth: GroundTruthReview,
    reviewer_records: Vec<Vec<EvidenceRecord>>,
    adjudication_records: Vec<EvidenceRecord>,
}

struct BoundedReader<R> {
    inner: R,
    remaining: u64,
}

struct ArchiveTreeEntry {
    path: String,
    mode: &'static str,
}

impl<R: Read> BoundedReader<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            remaining: MAX_TAR_STREAM_BYTES,
        }
    }
}

impl<R: Read> Read for BoundedReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.remaining == 0 {
            let mut extra = [0_u8; 1];
            return match self.inner.read(&mut extra)? {
                0 => Ok(0),
                _ => Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "materialized source archive exceeds the tar-stream limit",
                )),
            };
        }
        let length = buffer.len().min(self.remaining as usize);
        let read = self.inner.read(&mut buffer[..length])?;
        self.remaining -= read as u64;
        Ok(read)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EvaluationProtocol {
    schema_version: u32,
    freeze_id: String,
    target_profiles: Vec<TargetProfile>,
    sampling: EvaluationSampling,
    claim_scope: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EvaluationSampling {
    repositories_per_profile: usize,
    declarations_per_repository: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TargetProfile {
    language: String,
    candidate_id: String,
    profile: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ArtifactLink {
    file: String,
    sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EvaluationSelection {
    schema_version: u32,
    freeze_id: String,
    protocol: ArtifactLink,
    protocol_commit: String,
    profiles: Vec<SelectionProfile>,
    #[serde(default)]
    replacements: Vec<SelectionReplacement>,
    documents: Vec<SelectedDocument>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SelectionProfile {
    language: String,
    candidate_id: String,
    ranked: Vec<RankedRepository>,
    selected: Vec<PlannedRepository>,
}

#[derive(Debug, Clone, Deserialize)]
struct RankedRepository {
    eligibility: RepositoryEligibility,
}

#[derive(Debug, Clone, Deserialize)]
struct RepositoryEligibility {
    eligible: bool,
    #[serde(default)]
    reasons: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SelectionReplacement {
    language: String,
    candidate_id: String,
    status: String,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlannedRepository {
    full_name: String,
    source: GitSource,
    case_file: String,
    case_ids: Vec<String>,
    declaration_draw: DeclarationDraw,
}

#[derive(Debug, Clone, Deserialize)]
struct DeclarationDraw {
    selected: Vec<SelectedDeclaration>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SelectedDeclaration {
    case_id: String,
    rank: usize,
    uri: Url,
    range: crate::Range,
    kind: SymbolKind,
    display_name: String,
}

impl SelectedDeclaration {
    fn symbol(&self) -> SymbolLocation {
        SymbolLocation {
            location: Location {
                uri: self.uri.clone(),
                range: self.range.clone(),
            },
            kind: self.kind.clone(),
            display_name: self.display_name.clone(),
            disambiguation: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SelectedDocument {
    case_file: String,
    language: String,
    candidate_id: String,
    source: GitSource,
    case_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct GitSource {
    repo: Url,
    commit: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EvaluationReview {
    schema_version: u32,
    freeze_id: String,
    selection: ArtifactLink,
    #[serde(default)]
    review_tier: Option<ReviewTier>,
    #[serde(default)]
    review_protocol: Option<ArtifactLink>,
    reviewers: Vec<ReviewArtifact>,
    adjudication: ReviewArtifact,
}

const CANONICAL_AGENT_REVIEW_PROTOCOL: &str =
    "benchmarks/review-protocol/blinded-agent-review-v3.json";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentReviewProtocol {
    schema_version: u32,
    methodology: ArtifactLink,
    prompt: ArtifactLink,
    response_schema: ArtifactLink,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ReviewTier {
    AgentReviewed,
    HumanAdjudicatedAgentPanel,
    IndependentlyHumanReviewed,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ParticipantKind {
    Agent,
    Human,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReviewParticipant {
    kind: ParticipantKind,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    execution_id: Option<String>,
    #[serde(default)]
    executed_at: Option<String>,
    #[serde(default)]
    identity: Option<String>,
    #[serde(default)]
    attestation: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReviewArtifact {
    id: String,
    file: String,
    sha256: String,
    #[serde(default)]
    participant: Option<ReviewParticipant>,
    #[serde(default)]
    prompt: Option<ArtifactLink>,
    #[serde(default)]
    response_schema: Option<ArtifactLink>,
    #[serde(default)]
    raw_response: Option<ArtifactLink>,
    #[serde(default)]
    sessions: Vec<AgentReviewSession>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentReviewSession {
    case_id: String,
    participant: ReviewParticipant,
    packet: ArtifactLink,
    prompt: ArtifactLink,
    response_schema: ArtifactLink,
    raw_response: ArtifactLink,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentCasePacket {
    schema_version: u32,
    case_id: String,
    language: String,
    reference_policy: String,
    position_encoding: String,
    source: AgentCasePacketSource,
    declaration: Location,
    display_name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentCasePacketSource {
    root: String,
    repo: Url,
    commit: String,
    archive_sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReviewerEvidence {
    schema_version: u32,
    reviewer: String,
    reference_policy: String,
    selection_algorithm: String,
    records: Vec<EvidenceRecord>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdjudicationEvidence {
    schema_version: u32,
    freeze_id: String,
    protocol_commit: String,
    records: Vec<EvidenceRecord>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EvidenceRecord {
    case_id: String,
    decision: String,
    declaration: EvidenceDeclaration,
    expected_usages: Vec<SymbolLocation>,
    definition_usage: Option<SymbolLocation>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentResponse {
    schema_version: u32,
    reviewer: AgentResponseReviewer,
    records: Vec<AgentResponseRecord>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentResponseReviewer {
    provider: String,
    model: String,
    execution_id: String,
    executed_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentResponseRecord {
    case_id: String,
    decision: String,
    confidence: String,
    declaration: Location,
    locations: Vec<AgentReviewedLocation>,
    definition_usage: Option<Location>,
    ambiguities: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentReviewedLocation {
    location: Location,
    classification: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EvidenceDeclaration {
    repository: String,
    commit: String,
    language: String,
    selection_rank: usize,
    location: Location,
    kind: SymbolKind,
    display_name: String,
}

impl EvidenceDeclaration {
    fn symbol(&self) -> SymbolLocation {
        SymbolLocation {
            location: self.location.clone(),
            kind: self.kind.clone(),
            display_name: self.display_name.clone(),
            disambiguation: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceMaterialization {
    schema_version: u32,
    freeze_id: String,
    selection: ArtifactLink,
    sources: Vec<MaterializedSource>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct MaterializedSource {
    repo: Url,
    commit: String,
    commit_object: String,
    tree: String,
    archive_tree: String,
    archive: String,
    sha256: String,
    #[serde(default)]
    gitlinks: Vec<Gitlink>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct Gitlink {
    path: String,
    commit: String,
}

/// Content-addressed evidence used to publish an evaluation snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationArtifactLink {
    pub file: String,
    pub sha256: String,
}

/// A protocol target retained in the public release audit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationTargetProfile {
    pub language: String,
    pub candidate_id: String,
    pub profile: String,
}

/// Content-addressed independent-review evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationReviewArtifact {
    pub id: String,
    pub file: String,
    pub sha256: String,
    pub participant_kind: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub execution_id: Option<String>,
    pub executed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sessions: Vec<EvaluationReviewSession>,
}

/// One independently executed, hash-bound agent review session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationReviewSession {
    pub case_id: String,
    pub provider: String,
    pub model: String,
    pub execution_id: String,
    pub executed_at: String,
    pub packet: EvaluationArtifactLink,
    pub prompt: EvaluationArtifactLink,
    pub response_schema: EvaluationArtifactLink,
    pub raw_response: EvaluationArtifactLink,
}

/// The evidence manifests bound into an evaluation release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationAuditArtifacts {
    pub protocol: EvaluationArtifactLink,
    pub selection: EvaluationArtifactLink,
    pub review: EvaluationArtifactLink,
    pub source_lock: EvaluationArtifactLink,
}

/// A counted reason included in the source-only selection audit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationAuditCount {
    pub reason: String,
    pub count: usize,
}

/// Per-profile source-only selection and replacement summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationSelectionAudit {
    pub language: String,
    pub candidate_id: String,
    pub ranked_repositories: usize,
    pub selected_repositories: usize,
    pub excluded_repositories: usize,
    pub exclusion_reasons: Vec<EvaluationAuditCount>,
    pub replacements: usize,
    pub replacement_reasons: Vec<EvaluationAuditCount>,
}

/// Validated publication metadata for one frozen evaluation corpus.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationReleaseAudit {
    pub freeze_id: String,
    pub claim_scope: String,
    pub target_profiles: Vec<EvaluationTargetProfile>,
    pub artifacts: EvaluationAuditArtifacts,
    pub review_tier: ReviewTierAudit,
    pub review_protocol: EvaluationArtifactLink,
    pub reviewers: Vec<EvaluationReviewArtifact>,
    pub adjudication: EvaluationReviewArtifact,
    pub source_count: usize,
    pub case_files: Vec<String>,
    pub case_ids_by_file: BTreeMap<String, Vec<String>>,
    pub case_count: usize,
    pub selection: Vec<EvaluationSelectionAudit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewTierAudit {
    HumanAdjudicatedAgentPanel,
    IndependentlyHumanReviewed,
}

pub(crate) fn validate_report_against_release_audit(
    report: &RunReport,
    audit: &EvaluationReleaseAudit,
    candidate_id: &str,
) -> Result<()> {
    let expected_files = audit.case_files.iter().cloned().collect::<BTreeSet<_>>();
    let report_files = report.case_files.iter().cloned().collect::<BTreeSet<_>>();
    if report.case_files.len() != audit.case_files.len() || report_files != expected_files {
        bail!(
            "evaluation report for {candidate_id} must exactly cover the explicit evaluation corpus"
        );
    }

    let document_files = report
        .documents
        .iter()
        .map(|document| document.case_file.clone())
        .collect::<BTreeSet<_>>();
    let case_ids_by_file = report
        .documents
        .iter()
        .map(|document| {
            (
                document.case_file.clone(),
                document
                    .cases
                    .iter()
                    .map(|case| case.id.clone())
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let case_count = report
        .documents
        .iter()
        .map(|document| document.cases.len())
        .sum::<usize>();
    if report.documents.len() != audit.case_files.len()
        || document_files != expected_files
        || case_ids_by_file != audit.case_ids_by_file
        || case_count != audit.case_count
        || report
            .documents
            .iter()
            .any(|document| document.corpus_partition != CorpusPartition::Evaluation)
    {
        bail!(
            "evaluation report for {candidate_id} contains missing, duplicate, substituted, or development cases"
        );
    }
    Ok(())
}

/// Validate the evidence linked from one evaluation document.
///
/// This checks metadata, review records, and the archived source bytes. The
/// source archive is part of the frozen evaluation artifact, so runner
/// execution never needs to resolve a mutable Git ref or clone from the
/// network.
pub fn validate_document_evidence(
    document: &BenchmarkDocument,
    case_file: &Path,
    repo_root: &Path,
) -> Result<()> {
    validate_document_evidence_cached(
        document,
        case_file,
        repo_root,
        &mut EvaluationEvidenceCache::default(),
    )
}

pub(crate) fn validate_document_evidence_cached(
    document: &BenchmarkDocument,
    case_file: &Path,
    repo_root: &Path,
    cache: &mut EvaluationEvidenceCache,
) -> Result<()> {
    if document.corpus.partition != CorpusPartition::Evaluation {
        return Ok(());
    }

    let freeze_id = required_metadata(&document.corpus.freeze_id, "freezeId")?;
    let selection_file =
        required_metadata(&document.corpus.selection_manifest, "selectionManifest")?;
    let selection_path = evidence_path(repo_root, selection_file)?;
    let review_path = evidence_path(
        repo_root,
        required_metadata(&document.corpus.review_manifest, "reviewManifest")?,
    )?;
    let source_lock_path = evidence_path(
        repo_root,
        required_metadata(&document.corpus.source_lock, "sourceLock")?,
    )?;

    let (selection, selection_bytes) = load_checked::<EvaluationSelection>(
        &selection_path,
        EVALUATION_SELECTION_SCHEMA,
        "evaluation selection manifest",
    )?;
    validate_schema_version(selection.schema_version, "evaluation selection manifest")?;
    require_same("selection freezeId", &selection.freeze_id, freeze_id)?;

    let protocol_path = evidence_path(repo_root, &selection.protocol.file)?;
    let (protocol, protocol_bytes) = load_checked::<EvaluationProtocol>(
        &protocol_path,
        EVALUATION_PROTOCOL_SCHEMA,
        "evaluation protocol",
    )?;
    validate_protocol_schema_version(protocol.schema_version, "evaluation protocol")?;
    require_same("protocol freezeId", &protocol.freeze_id, freeze_id)?;
    validate_link(
        &selection.protocol,
        &protocol_path,
        &protocol_bytes,
        "evaluation selection protocol",
    )?;
    validate_profiles(&protocol.target_profiles)?;

    let case_file = repo_relative(case_file, repo_root)?;
    let selected = selection
        .documents
        .iter()
        .filter(|selected| selected.case_file == case_file)
        .collect::<Vec<_>>();
    let [selected] = selected.as_slice() else {
        bail!("evaluation selection does not contain exactly one entry for {case_file}");
    };
    if selected.language != document.language {
        bail!(
            "evaluation selection language {} does not match document language {} for {case_file}",
            selected.language,
            document.language
        );
    }
    if !protocol.target_profiles.iter().any(|profile| {
        profile.language == selected.language && profile.candidate_id == selected.candidate_id
    }) {
        bail!(
            "evaluation selection candidate {} is not registered for language {} in the protocol",
            selected.candidate_id,
            selected.language
        );
    }
    validate_document_source(document, &selected.source)?;
    validate_case_ids(document, &selected.case_ids, &case_file)?;
    validate_document_draw(document, &selection, &case_file)?;

    let (review, review_bytes) = load_checked::<EvaluationReview>(
        &review_path,
        EVALUATION_REVIEW_SCHEMA,
        "evaluation review manifest",
    )?;
    validate_protocol_schema_version(review.schema_version, "evaluation review manifest")?;
    require_same("review freezeId", &review.freeze_id, freeze_id)?;
    require_same(
        "evaluation review selection file",
        &review.selection.file,
        selection_file,
    )?;
    validate_link(
        &review.selection,
        &selection_path,
        &selection_bytes,
        "evaluation review selection",
    )?;
    let (source_lock, source_lock_bytes) = load_checked::<SourceMaterialization>(
        &source_lock_path,
        SOURCE_MATERIALIZATION_SCHEMA,
        "evaluation source lock",
    )?;
    validate_schema_version(source_lock.schema_version, "evaluation source lock")?;
    require_same("source lock freezeId", &source_lock.freeze_id, freeze_id)?;
    require_same(
        "evaluation source lock selection file",
        &source_lock.selection.file,
        selection_file,
    )?;
    validate_link(
        &source_lock.selection,
        &selection_path,
        &selection_bytes,
        "evaluation source lock selection",
    )?;
    let review_key = ReviewValidationKey {
        selection_file: selection_file.to_string(),
        selection_sha256: sha256(&selection_bytes),
        review_file: required_metadata(&document.corpus.review_manifest, "reviewManifest")?
            .to_string(),
        review_sha256: sha256(&review_bytes),
        source_lock_file: required_metadata(&document.corpus.source_lock, "sourceLock")?
            .to_string(),
        source_lock_sha256: sha256(&source_lock_bytes),
    };
    if !cache.validated_reviews.contains_key(&review_key) {
        let validated = validate_reviewers(
            document,
            &selection,
            &review,
            &source_lock.sources,
            repo_root,
        )?;
        cache
            .validated_reviews
            .insert(review_key.clone(), validated);
    }
    let validated = cache
        .validated_reviews
        .get(&review_key)
        .expect("validated review was inserted");
    if validated.ground_truth != document.ground_truth {
        bail!("evaluation documents sharing review evidence must use the same groundTruth");
    }
    validate_document_review_evidence(document, validated)?;
    validate_source_lock(&source_lock.sources, &selected.source, repo_root)?;
    validate_archive_ranges(document, &source_lock.sources, &selected.source, repo_root)?;

    Ok(())
}

/// Validate a path containing only promoted evaluation documents.
pub fn validate_path(path: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
    let path = path.as_ref();
    let files = crate::validate_path(path)?;
    let repo_root = crate::find_repo_root_for_path(path)?;
    let mut actual_documents = BTreeMap::<String, BTreeSet<String>>::new();
    let mut selection_manifest = None::<String>;
    for file in &files {
        let yaml = fs::read_to_string(file).with_context(|| format!("read {}", file.display()))?;
        let document: BenchmarkDocument = serde_yaml::from_str(&yaml)
            .with_context(|| format!("deserialize benchmark document {}", file.display()))?;
        if document.corpus.partition != CorpusPartition::Evaluation {
            bail!("{} is not an evaluation document", file.display());
        }
        let manifest = required_metadata(&document.corpus.selection_manifest, "selectionManifest")?;
        if let Some(expected) = &selection_manifest {
            require_same("evaluation corpus selection manifest", manifest, expected)?;
        } else {
            selection_manifest = Some(manifest.to_string());
        }
        let relative = repo_relative(file, &repo_root)?;
        let case_ids = document
            .cases
            .iter()
            .map(|case| case.id.clone())
            .collect::<BTreeSet<_>>();
        if case_ids.len() != document.cases.len()
            || actual_documents
                .insert(relative.clone(), case_ids)
                .is_some()
        {
            bail!("evaluation corpus contains duplicate cases or document {relative}");
        }
    }

    if path.is_dir() {
        let selection_path = evidence_path(
            &repo_root,
            selection_manifest
                .as_deref()
                .context("evaluation corpus has no selection manifest")?,
        )?;
        let (selection, _) = load_checked::<EvaluationSelection>(
            &selection_path,
            EVALUATION_SELECTION_SCHEMA,
            "evaluation selection manifest",
        )?;
        let protocol_path = evidence_path(&repo_root, &selection.protocol.file)?;
        let (protocol, protocol_bytes) = load_checked::<EvaluationProtocol>(
            &protocol_path,
            EVALUATION_PROTOCOL_SCHEMA,
            "evaluation protocol",
        )?;
        validate_link(
            &selection.protocol,
            &protocol_path,
            &protocol_bytes,
            "evaluation selection protocol",
        )?;
        validate_selection_coverage(&actual_documents, &selection, &protocol)?;
    }
    Ok(files)
}

/// Validate a complete evaluation corpus and build its publication audit.
///
/// This deliberately starts with [`validate_path`], so the summary can only be
/// constructed after the protocol, selection, review, adjudication, source
/// lock, archived sources, and authored documents have passed their existing
/// semantic checks.
pub fn build_release_audit(path: impl AsRef<Path>) -> Result<EvaluationReleaseAudit> {
    let path = path.as_ref();
    if !path.is_dir() {
        bail!(
            "evaluation release corpus must be a directory: {}",
            path.display()
        );
    }
    let files = validate_path(path).context("validate evaluation release corpus")?;
    let repo_root = crate::find_repo_root_for_path(path)?;
    let mut case_files = Vec::with_capacity(files.len());
    let mut case_ids_by_file = BTreeMap::new();
    let mut case_count = 0;
    let mut selection_file = None::<String>;
    let mut review_file = None::<String>;
    let mut source_lock_file = None::<String>;
    for file in &files {
        let yaml = fs::read_to_string(file).with_context(|| format!("read {}", file.display()))?;
        let document: BenchmarkDocument = serde_yaml::from_str(&yaml)
            .with_context(|| format!("deserialize benchmark document {}", file.display()))?;
        require_common_metadata(
            &mut selection_file,
            required_metadata(&document.corpus.selection_manifest, "selectionManifest")?,
            "selection manifest",
        )?;
        require_common_metadata(
            &mut review_file,
            required_metadata(&document.corpus.review_manifest, "reviewManifest")?,
            "review manifest",
        )?;
        require_common_metadata(
            &mut source_lock_file,
            required_metadata(&document.corpus.source_lock, "sourceLock")?,
            "source lock",
        )?;
        let case_file = repo_relative(file, &repo_root)?;
        let case_ids = document
            .cases
            .iter()
            .map(|case| case.id.clone())
            .collect::<Vec<_>>();
        if case_ids_by_file
            .insert(case_file.clone(), case_ids)
            .is_some()
        {
            bail!("evaluation corpus contains duplicate case file {case_file}");
        }
        case_files.push(case_file);
        case_count += document.cases.len();
    }
    case_files.sort();

    let selection_file = selection_file.context("evaluation corpus has no selection manifest")?;
    let selection_path = evidence_path(&repo_root, &selection_file)?;
    let (selection, selection_bytes) = load_checked::<EvaluationSelection>(
        &selection_path,
        EVALUATION_SELECTION_SCHEMA,
        "evaluation selection manifest",
    )?;
    let protocol_path = evidence_path(&repo_root, &selection.protocol.file)?;
    let (protocol, protocol_bytes) = load_checked::<EvaluationProtocol>(
        &protocol_path,
        EVALUATION_PROTOCOL_SCHEMA,
        "evaluation protocol",
    )?;
    validate_link(
        &selection.protocol,
        &protocol_path,
        &protocol_bytes,
        "evaluation selection protocol",
    )?;

    let review_file = review_file.context("evaluation corpus has no review manifest")?;
    let review_path = evidence_path(&repo_root, &review_file)?;
    let (review, review_bytes) = load_checked::<EvaluationReview>(
        &review_path,
        EVALUATION_REVIEW_SCHEMA,
        "evaluation review manifest",
    )?;
    if review.schema_version != 2 {
        bail!("evaluation publication requires schema-v2 review evidence");
    }
    let review_tier = match review.review_tier {
        Some(ReviewTier::HumanAdjudicatedAgentPanel) => {
            ReviewTierAudit::HumanAdjudicatedAgentPanel
        }
        Some(ReviewTier::IndependentlyHumanReviewed) => {
            ReviewTierAudit::IndependentlyHumanReviewed
        }
        _ => bail!(
            "evaluation publication requires human_adjudicated_agent_panel or independently_human_reviewed evidence"
        ),
    };
    let review_protocol = review
        .review_protocol
        .clone()
        .context("evaluation publication requires canonical review protocol evidence")?;
    let source_lock_file = source_lock_file.context("evaluation corpus has no source lock")?;
    let source_lock_path = evidence_path(&repo_root, &source_lock_file)?;
    let (source_lock, source_lock_bytes) = load_checked::<SourceMaterialization>(
        &source_lock_path,
        SOURCE_MATERIALIZATION_SCHEMA,
        "evaluation source lock",
    )?;

    let selection_audit = selection
        .profiles
        .iter()
        .map(|profile| {
            let excluded = profile
                .ranked
                .iter()
                .filter(|repository| !repository.eligibility.eligible)
                .collect::<Vec<_>>();
            let replacement_reasons = selection
                .replacements
                .iter()
                .filter(|replacement| {
                    replacement.language == profile.language
                        && replacement.candidate_id == profile.candidate_id
                        && replacement.status == "replaced"
                })
                .filter_map(|replacement| replacement.reason.as_deref());
            EvaluationSelectionAudit {
                language: profile.language.clone(),
                candidate_id: profile.candidate_id.clone(),
                ranked_repositories: profile.ranked.len(),
                selected_repositories: profile.selected.len(),
                excluded_repositories: excluded.len(),
                exclusion_reasons: counted_reasons(excluded.iter().flat_map(|repository| {
                    repository.eligibility.reasons.iter().map(String::as_str)
                })),
                replacements: selection
                    .replacements
                    .iter()
                    .filter(|replacement| {
                        replacement.language == profile.language
                            && replacement.candidate_id == profile.candidate_id
                            && replacement.status == "replaced"
                    })
                    .count(),
                replacement_reasons: counted_reasons(replacement_reasons),
            }
        })
        .collect();

    Ok(EvaluationReleaseAudit {
        freeze_id: protocol.freeze_id,
        claim_scope: protocol.claim_scope,
        target_profiles: protocol
            .target_profiles
            .into_iter()
            .map(|profile| EvaluationTargetProfile {
                language: profile.language,
                candidate_id: profile.candidate_id,
                profile: profile.profile,
            })
            .collect(),
        artifacts: EvaluationAuditArtifacts {
            protocol: EvaluationArtifactLink {
                file: selection.protocol.file,
                sha256: selection.protocol.sha256,
            },
            selection: artifact_link(selection_file, &selection_bytes),
            review: artifact_link(review_file, &review_bytes),
            source_lock: artifact_link(source_lock_file, &source_lock_bytes),
        },
        review_tier,
        review_protocol: EvaluationArtifactLink {
            file: review_protocol.file,
            sha256: review_protocol.sha256,
        },
        reviewers: review
            .reviewers
            .into_iter()
            .map(review_artifact)
            .collect::<Result<Vec<_>>>()?,
        adjudication: review_artifact(review.adjudication)?,
        source_count: source_lock.sources.len(),
        case_files,
        case_ids_by_file,
        case_count,
        selection: selection_audit,
    })
}

fn require_common_metadata(slot: &mut Option<String>, actual: &str, kind: &str) -> Result<()> {
    if let Some(expected) = slot {
        require_same(&format!("evaluation corpus {kind}"), actual, expected)
    } else {
        *slot = Some(actual.to_string());
        Ok(())
    }
}

fn artifact_link(file: String, bytes: &[u8]) -> EvaluationArtifactLink {
    EvaluationArtifactLink {
        file,
        sha256: sha256(bytes),
    }
}

fn review_artifact(artifact: ReviewArtifact) -> Result<EvaluationReviewArtifact> {
    let participant = artifact.participant;
    let sessions = artifact
        .sessions
        .into_iter()
        .map(review_session)
        .collect::<Result<Vec<_>>>()?;
    Ok(EvaluationReviewArtifact {
        id: artifact.id,
        file: artifact.file,
        sha256: artifact.sha256,
        participant_kind: participant
            .as_ref()
            .map(|participant| match participant.kind {
                ParticipantKind::Agent => "agent".to_string(),
                ParticipantKind::Human => "human".to_string(),
            }),
        provider: participant
            .as_ref()
            .and_then(|participant| participant.provider.clone()),
        model: participant
            .as_ref()
            .and_then(|participant| participant.model.clone()),
        execution_id: participant
            .as_ref()
            .and_then(|participant| participant.execution_id.clone()),
        executed_at: participant
            .as_ref()
            .and_then(|participant| participant.executed_at.clone()),
        sessions,
    })
}

fn review_session(session: AgentReviewSession) -> Result<EvaluationReviewSession> {
    if session.participant.kind != ParticipantKind::Agent {
        bail!("release audit session participant must be an agent");
    }
    let provider = nonempty(
        session.participant.provider.as_deref(),
        "release audit session provider",
    )?
    .to_string();
    let model = nonempty(
        session.participant.model.as_deref(),
        "release audit session model",
    )?
    .to_string();
    let execution_id = nonempty(
        session.participant.execution_id.as_deref(),
        "release audit session executionId",
    )?
    .to_string();
    let executed_at = nonempty(
        session.participant.executed_at.as_deref(),
        "release audit session executedAt",
    )?
    .to_string();
    Ok(EvaluationReviewSession {
        case_id: session.case_id,
        provider,
        model,
        execution_id,
        executed_at,
        packet: linked_audit_artifact(session.packet),
        prompt: linked_audit_artifact(session.prompt),
        response_schema: linked_audit_artifact(session.response_schema),
        raw_response: linked_audit_artifact(session.raw_response),
    })
}

fn linked_audit_artifact(link: ArtifactLink) -> EvaluationArtifactLink {
    EvaluationArtifactLink {
        file: link.file,
        sha256: link.sha256,
    }
}

fn counted_reasons<'a>(reasons: impl Iterator<Item = &'a str>) -> Vec<EvaluationAuditCount> {
    let mut counts = BTreeMap::<String, usize>::new();
    for reason in reasons {
        *counts.entry(reason.to_string()).or_default() += 1;
    }
    counts
        .into_iter()
        .map(|(reason, count)| EvaluationAuditCount { reason, count })
        .collect()
}

fn validate_selection_coverage(
    actual_documents: &BTreeMap<String, BTreeSet<String>>,
    selection: &EvaluationSelection,
    protocol: &EvaluationProtocol,
) -> Result<()> {
    let expected_documents = selection
        .documents
        .iter()
        .map(|document| {
            (
                document.case_file.clone(),
                document.case_ids.iter().cloned().collect::<BTreeSet<_>>(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    if expected_documents.len() != selection.documents.len() {
        bail!("evaluation selection contains duplicate document paths");
    }
    let expected_case_ids = selection
        .documents
        .iter()
        .flat_map(|document| document.case_ids.iter())
        .collect::<BTreeSet<_>>();
    if expected_case_ids.len()
        != selection
            .documents
            .iter()
            .map(|document| document.case_ids.len())
            .sum::<usize>()
    {
        bail!("evaluation selection contains duplicate case IDs across documents");
    }
    let actual_case_ids = actual_documents
        .values()
        .flat_map(|case_ids| case_ids.iter())
        .collect::<BTreeSet<_>>();
    if actual_case_ids.len() != actual_documents.values().map(BTreeSet::len).sum::<usize>() {
        bail!("evaluation corpus contains duplicate case IDs across documents");
    }
    if actual_documents != &expected_documents {
        bail!("evaluation corpus documents and case IDs do not exactly cover the selection");
    }
    if selection.profiles.len() != protocol.target_profiles.len() {
        bail!("evaluation repository draw does not cover every target profile");
    }
    let mut profile_identities = BTreeSet::new();
    let mut planned_documents = BTreeMap::new();
    for profile in &selection.profiles {
        if !protocol.target_profiles.iter().any(|target| {
            target.language == profile.language && target.candidate_id == profile.candidate_id
        }) || !profile_identities.insert((&profile.language, &profile.candidate_id))
        {
            bail!("evaluation repository draw contains an unknown or duplicate target profile");
        }
        if profile.selected.len() != protocol.sampling.repositories_per_profile {
            bail!("evaluation repository draw has the wrong repository count for a profile");
        }
        for repository in &profile.selected {
            let selected_document = selection
                .documents
                .iter()
                .find(|document| document.case_file == repository.case_file)
                .context("repository draw is missing its selected document")?;
            if selected_document.language != profile.language
                || selected_document.candidate_id != profile.candidate_id
                || selected_document.source != repository.source
                || selected_document.case_ids != repository.case_ids
            {
                bail!("evaluation selected document identity does not match its repository draw");
            }
            let case_ids = repository.case_ids.iter().cloned().collect::<BTreeSet<_>>();
            if case_ids.len() != repository.case_ids.len()
                || case_ids.len() != protocol.sampling.declarations_per_repository
                || planned_documents
                    .insert(repository.case_file.clone(), case_ids.clone())
                    .is_some()
            {
                bail!("evaluation repository draw contains duplicate documents or case IDs");
            }
            let declaration_case_ids = repository
                .declaration_draw
                .selected
                .iter()
                .map(|declaration| declaration.case_id.clone())
                .collect::<BTreeSet<_>>();
            if declaration_case_ids.len() != repository.declaration_draw.selected.len()
                || declaration_case_ids != case_ids
            {
                bail!("evaluation declaration draw does not cover its planned case IDs");
            }
        }
    }
    if planned_documents != expected_documents {
        bail!("evaluation documents do not exactly cover the repository draw");
    }
    Ok(())
}

/// Extract the locked source archive for an evaluation document into a runner
/// workspace. Non-evaluation documents retain their existing source handling.
pub fn materialized_source_root(
    document: &BenchmarkDocument,
    repo_root: &Path,
    work_dir: &Path,
) -> Result<Option<PathBuf>> {
    if document.corpus.partition != CorpusPartition::Evaluation {
        return Ok(None);
    }

    let source_lock_path = evidence_path(
        repo_root,
        required_metadata(&document.corpus.source_lock, "sourceLock")?,
    )?;
    let (source_lock, _) = load_checked::<SourceMaterialization>(
        &source_lock_path,
        SOURCE_MATERIALIZATION_SCHEMA,
        "evaluation source lock",
    )?;
    let Source::Git { repo, commit } = &document.source else {
        bail!("evaluation document must use a git source");
    };
    let matching = source_lock
        .sources
        .iter()
        .filter(|source| source.repo == *repo && source.commit == *commit)
        .collect::<Vec<_>>();
    let [source] = matching.as_slice() else {
        bail!("source lock does not contain exactly one entry for the evaluation document source");
    };
    let selected = GitSource {
        repo: repo.clone(),
        commit: commit.clone(),
    };
    validate_source_lock(&source_lock.sources, &selected, repo_root)?;
    let archive_path = evidence_path(repo_root, &source.archive)?;
    let sources_directory = work_dir.join("sources");
    fs::create_dir_all(&sources_directory).with_context(|| {
        format!(
            "create materialized source cache {}",
            sources_directory.display()
        )
    })?;
    let temporary = tempfile::Builder::new()
        .prefix(&format!("{}-", source.sha256))
        .tempdir_in(&sources_directory)
        .context("create temporary materialized source root")?;
    let _ = extract_materialized_archive(&archive_path, temporary.path(), commit)?;
    let destination = temporary.keep();
    destination
        .canonicalize()
        .with_context(|| format!("canonicalize {}", destination.display()))
        .map(Some)
}

fn extract_materialized_archive(
    archive_path: &Path,
    destination: &Path,
    expected_commit: &str,
) -> Result<Vec<ArchiveTreeEntry>> {
    let file = fs::File::open(archive_path).with_context(|| {
        format!(
            "open materialized source archive {}",
            archive_path.display()
        )
    })?;
    let mut archive = tar::Archive::new(BoundedReader::new(GzDecoder::new(file)));
    let mut entry_count = 0_usize;
    let mut expanded_bytes = 0_u64;
    let mut seen_paths = BTreeSet::new();
    let mut tree_entries = Vec::new();
    for entry in archive
        .entries()
        .with_context(|| format!("read entries from {}", archive_path.display()))?
        .raw(true)
    {
        let mut entry =
            entry.with_context(|| format!("read entry from {}", archive_path.display()))?;
        entry_count += 1;
        if entry_count > MAX_ARCHIVE_ENTRIES {
            bail!("materialized source archive contains too many entries");
        }
        let entry_type = entry.header().entry_type();
        if entry_type.is_pax_global_extensions() {
            validate_global_pax_header(&mut entry, expected_commit)?;
            continue;
        }
        if entry_type.is_pax_local_extensions()
            || entry_type.is_gnu_longname()
            || entry_type.is_gnu_longlink()
        {
            bail!("materialized source archive contains unsupported extension metadata");
        }
        let path = entry.path().context("decode materialized source path")?;
        validate_archive_path(&path)?;
        let path = path
            .to_str()
            .context("materialized source path is not valid UTF-8")?
            .to_string();
        if !(entry_type.is_file() || entry_type.is_dir() || entry_type.is_symlink()) {
            bail!("materialized source archive contains an unsupported entry type");
        }
        let size = if entry_type.is_symlink() {
            entry
                .link_name()?
                .context("archived symlink has no target")?
                .as_os_str()
                .len() as u64
        } else {
            entry.size()
        };
        if size > MAX_ARCHIVE_FILE_BYTES {
            bail!("materialized source archive entry exceeds the file-size limit");
        }
        expanded_bytes = expanded_bytes
            .checked_add(size)
            .context("materialized source archive expanded-size overflow")?;
        if expanded_bytes > MAX_EXPANDED_ARCHIVE_BYTES {
            bail!("materialized source archive exceeds the expanded-size limit");
        }
        if !entry
            .unpack_in(destination)
            .context("extract validated materialized source entry")?
        {
            bail!("materialized source archive entry escaped the destination");
        }
        if !entry_type.is_dir() {
            if !seen_paths.insert(path.clone()) {
                bail!("materialized source archive contains duplicate path {path}");
            }
            tree_entries.push(ArchiveTreeEntry {
                path,
                mode: if entry_type.is_symlink() {
                    "120000"
                } else if entry.header().mode().unwrap_or(0) & 0o111 != 0 {
                    "100755"
                } else {
                    "100644"
                },
            });
        }
    }
    Ok(tree_entries)
}

fn load_checked<T: DeserializeOwned>(
    path: &Path,
    schema_source: &str,
    kind: &str,
) -> Result<(T, Vec<u8>)> {
    let bytes = fs::read(path).with_context(|| format!("read {kind} {}", path.display()))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("parse {kind} {}", path.display()))?;
    let schema: serde_json::Value = serde_json::from_str(schema_source)
        .with_context(|| format!("parse bundled {kind} schema"))?;
    let compiled = jsonschema::JSONSchema::compile(&schema)
        .map_err(|error| anyhow!("compile bundled {kind} schema: {error}"))?;
    if let Err(errors) = compiled.validate(&value) {
        let messages = errors
            .map(|error| format!("{}: {error}", error.instance_path))
            .collect::<Vec<_>>()
            .join("\n");
        bail!("{} failed schema validation:\n{messages}", path.display());
    }
    let parsed = serde_json::from_value(value)
        .with_context(|| format!("deserialize {kind} {}", path.display()))?;
    Ok((parsed, bytes))
}

pub(crate) fn load_evaluation_protocol_checked<T: DeserializeOwned>(
    path: &Path,
) -> Result<(T, Vec<u8>)> {
    load_checked(path, EVALUATION_PROTOCOL_SCHEMA, "evaluation protocol")
}

pub(crate) fn load_evaluation_population_checked<T: DeserializeOwned>(
    path: &Path,
) -> Result<(T, Vec<u8>)> {
    load_checked(path, EVALUATION_POPULATION_SCHEMA, "evaluation population")
}

#[cfg(test)]
pub(crate) fn load_evaluation_selection_checked<T: DeserializeOwned>(
    path: &Path,
) -> Result<(T, Vec<u8>)> {
    load_checked(path, EVALUATION_SELECTION_SCHEMA, "evaluation selection")
}

fn required_metadata<'a>(value: &'a Option<String>, field: &str) -> Result<&'a str> {
    value
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("evaluation corpus documents require a non-empty {field}"))
}

fn evidence_path(repo_root: &Path, value: &str) -> Result<PathBuf> {
    Ok(repo_root.join(safe_repo_relative_path(value, "evaluation evidence path")?))
}

pub(crate) fn safe_repo_relative_path(value: &str, kind: &str) -> Result<PathBuf> {
    let path = Path::new(value);
    validate_safe_repo_relative_path(path, kind)?;
    Ok(path.to_path_buf())
}

fn validate_safe_repo_relative_path(path: &Path, kind: &str) -> Result<()> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!(
            "{kind} must be a safe repository-relative path: {}",
            path.display()
        );
    }
    Ok(())
}

fn repo_relative(path: &Path, repo_root: &Path) -> Result<String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .context("read current directory")?
            .join(path)
    };
    let relative = absolute.strip_prefix(repo_root).with_context(|| {
        format!(
            "case file {} is not under repository root {}",
            absolute.display(),
            repo_root.display()
        )
    })?;
    relative
        .to_str()
        .map(|path| path.replace(std::path::MAIN_SEPARATOR, "/"))
        .ok_or_else(|| anyhow!("case file {} is not valid UTF-8", absolute.display()))
}

fn validate_schema_version(version: u32, kind: &str) -> Result<()> {
    if version != 1 {
        bail!("{kind} schemaVersion must be 1");
    }
    Ok(())
}

fn validate_protocol_schema_version(version: u32, kind: &str) -> Result<()> {
    if !matches!(version, 1 | 2) {
        bail!("{kind} schemaVersion must be 1 or 2");
    }
    Ok(())
}

fn require_same(field: &str, actual: &str, expected: &str) -> Result<()> {
    if actual != expected {
        bail!("{field} {actual} does not match {expected}");
    }
    Ok(())
}

fn validate_link(link: &ArtifactLink, path: &Path, bytes: &[u8], kind: &str) -> Result<()> {
    if !is_hex_digest(&link.sha256) {
        bail!("{kind} has an invalid sha256");
    }
    let actual = sha256(bytes);
    if link.sha256 != actual {
        bail!("{kind} sha256 does not match {}", path.display());
    }
    Ok(())
}

fn validate_profiles(profiles: &[TargetProfile]) -> Result<()> {
    let mut identities = BTreeSet::new();
    for profile in profiles {
        if profile.language.trim().is_empty()
            || profile.candidate_id.trim().is_empty()
            || profile.profile.trim().is_empty()
        {
            bail!("evaluation protocol target profiles must be non-empty");
        }
        if !identities.insert((profile.language.as_str(), profile.candidate_id.as_str())) {
            bail!("evaluation protocol contains a duplicate language/candidate profile");
        }
    }
    Ok(())
}

fn validate_document_source(document: &BenchmarkDocument, selected: &GitSource) -> Result<()> {
    if !is_exact_git_commit(&selected.commit) {
        bail!("evaluation selection source commit must be an exact 40-character lowercase hexadecimal ID");
    }
    match &document.source {
        Source::Git { repo, commit } if repo == &selected.repo && commit == &selected.commit => {
            Ok(())
        }
        Source::Git { .. } => bail!("evaluation document git source does not match the selection"),
        Source::Fixture { .. } => bail!("evaluation document must use a git source"),
    }
}

fn validate_case_ids(
    document: &BenchmarkDocument,
    selected: &[String],
    case_file: &str,
) -> Result<()> {
    let actual = document
        .cases
        .iter()
        .map(|case| case.id.as_str())
        .collect::<BTreeSet<_>>();
    let expected = selected.iter().map(String::as_str).collect::<BTreeSet<_>>();
    if actual.len() != document.cases.len() {
        bail!("evaluation document {case_file} contains duplicate case IDs");
    }
    if expected.len() != selected.len() {
        bail!("evaluation selection contains duplicate case IDs for {case_file}");
    }
    if actual != expected {
        bail!("evaluation document case IDs do not match the selection for {case_file}");
    }
    Ok(())
}

fn validate_document_draw(
    document: &BenchmarkDocument,
    selection: &EvaluationSelection,
    case_file: &str,
) -> Result<()> {
    let planned = selection
        .profiles
        .iter()
        .flat_map(|profile| &profile.selected)
        .filter(|repository| repository.case_file == case_file)
        .collect::<Vec<_>>();
    let [planned] = planned.as_slice() else {
        bail!("evaluation repository draw does not contain exactly one entry for {case_file}");
    };
    validate_document_source(document, &planned.source)?;
    for case in &document.cases {
        let selected = planned
            .declaration_draw
            .selected
            .iter()
            .filter(|declaration| declaration.case_id == case.id)
            .collect::<Vec<_>>();
        let [selected] = selected.as_slice() else {
            bail!(
                "evaluation declaration draw does not contain exactly one slot for {}",
                case.id
            );
        };
        if selected.rank == 0 || case.declaration.as_ref() != Some(&selected.symbol()) {
            bail!(
                "evaluation case {} does not match its declaration draw",
                case.id
            );
        }
    }
    Ok(())
}

fn validate_reviewers(
    document: &BenchmarkDocument,
    selection: &EvaluationSelection,
    review: &EvaluationReview,
    sources: &[MaterializedSource],
    repo_root: &Path,
) -> Result<ValidatedReviewEvidence> {
    validate_review_contract(document, review, repo_root)?;
    let compiled_agent_schema = if review.schema_version == 2
        && matches!(
            review.review_tier,
            Some(ReviewTier::AgentReviewed | ReviewTier::HumanAdjudicatedAgentPanel)
        ) {
        let reviewer = review
            .reviewers
            .first()
            .context("agent review evidence requires at least one reviewer")?;
        let schema_link = reviewer
            .response_schema
            .as_ref()
            .or_else(|| {
                reviewer
                    .sessions
                    .first()
                    .map(|session| &session.response_schema)
            })
            .context("agent review evidence requires a response schema")?;
        Some(compile_agent_response_schema(schema_link, repo_root)?)
    } else {
        None
    };
    let expected = document
        .ground_truth
        .reviewers
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let actual = review
        .reviewers
        .iter()
        .map(|reviewer| reviewer.id.as_str())
        .collect::<BTreeSet<_>>();
    if actual.len() != review.reviewers.len() || actual != expected {
        bail!("evaluation review evidence reviewers do not match groundTruth reviewers");
    }
    let selected_case_ids = selection
        .documents
        .iter()
        .flat_map(|document| document.case_ids.iter().cloned())
        .collect::<BTreeSet<_>>();
    let mut reviewer_records = Vec::new();
    let mut agent_responses = Vec::new();
    let mut providers_by_case = selected_case_ids
        .iter()
        .map(|case_id| (case_id.clone(), BTreeSet::<String>::new()))
        .collect::<BTreeMap<_, _>>();
    for reviewer in &review.reviewers {
        let evidence =
            load_review_artifact::<ReviewerEvidence>(reviewer, repo_root, "reviewer evidence")?;
        validate_schema_version(evidence.schema_version, "reviewer evidence")?;
        require_same(
            "reviewer evidence identity",
            &evidence.reviewer,
            &reviewer.id,
        )?;
        require_same(
            "reviewer evidence reference policy",
            &evidence.reference_policy,
            "bindings_optional",
        )?;
        require_same(
            "reviewer evidence selection algorithm",
            &evidence.selection_algorithm,
            "real-project-v1-source-syntax-v1",
        )?;
        validate_reviewer_evidence_records(
            &evidence.records,
            &selected_case_ids,
            "reviewer evidence",
        )?;
        for record in &evidence.records {
            validate_evidence_identity(record, selection, "reviewer evidence")?;
        }
        if review.schema_version == 2
            && matches!(
                review.review_tier,
                Some(ReviewTier::AgentReviewed | ReviewTier::HumanAdjudicatedAgentPanel)
            )
        {
            if reviewer.sessions.is_empty() {
                let participant = reviewer
                    .participant
                    .as_ref()
                    .context("missing participant")?;
                let provider = participant
                    .provider
                    .as_deref()
                    .expect("validated legacy agent participant has a provider")
                    .trim()
                    .to_ascii_lowercase();
                for providers in providers_by_case.values_mut() {
                    providers.insert(provider.clone());
                }
                let raw = validate_agent_response(
                    compiled_agent_schema
                        .as_ref()
                        .expect("agent review tiers compile the response schema"),
                    reviewer
                        .raw_response
                        .as_ref()
                        .context("missing raw response")?,
                    participant,
                    repo_root,
                )?;
                validate_raw_response_matches_evidence(&raw, &evidence, &selected_case_ids)?;
                agent_responses.push(raw);
            } else {
                validate_session_coverage(reviewer, &selected_case_ids)?;
                for session in &reviewer.sessions {
                    let raw = validate_agent_response(
                        compiled_agent_schema
                            .as_ref()
                            .expect("agent review tiers compile the response schema"),
                        &session.raw_response,
                        &session.participant,
                        repo_root,
                    )?;
                    let expected_case_ids = BTreeSet::from([session.case_id.clone()]);
                    validate_raw_response_matches_evidence(&raw, &evidence, &expected_case_ids)?;
                    let normalized = evidence
                        .records
                        .iter()
                        .find(|record| record.case_id == session.case_id)
                        .expect("reviewer evidence coverage was validated");
                    validate_agent_case_packet(session, normalized, selection, sources, repo_root)?;
                    let provider = session
                        .participant
                        .provider
                        .as_deref()
                        .expect("validated agent session has a provider")
                        .trim()
                        .to_ascii_lowercase();
                    providers_by_case
                        .get_mut(&session.case_id)
                        .expect("session coverage was validated")
                        .insert(provider);
                    agent_responses.push(raw);
                }
            }
        }
        reviewer_records.push(evidence.records);
    }
    if review.schema_version == 2
        && matches!(
            review.review_tier,
            Some(ReviewTier::AgentReviewed | ReviewTier::HumanAdjudicatedAgentPanel)
        )
    {
        validate_per_case_provider_diversity(&providers_by_case)?;
    }
    let adjudication = load_review_artifact::<AdjudicationEvidence>(
        &review.adjudication,
        repo_root,
        "adjudication evidence",
    )?;
    validate_schema_version(adjudication.schema_version, "adjudication evidence")?;
    require_same(
        "adjudication freezeId",
        &adjudication.freeze_id,
        &selection.freeze_id,
    )?;
    require_same(
        "adjudication protocol commit",
        &adjudication.protocol_commit,
        &selection.protocol_commit,
    )?;
    validate_final_evidence_records(
        &adjudication.records,
        &selected_case_ids,
        "adjudication evidence",
    )?;
    for record in &adjudication.records {
        validate_evidence_identity(record, selection, "adjudication evidence")?;
    }
    if review.schema_version == 2 {
        validate_panel_escalations(&agent_responses, &adjudication.records, &selected_case_ids)?;
    }

    Ok(ValidatedReviewEvidence {
        ground_truth: document.ground_truth.clone(),
        reviewer_records,
        adjudication_records: adjudication.records,
    })
}

fn validate_document_review_evidence(
    document: &BenchmarkDocument,
    validated: &ValidatedReviewEvidence,
) -> Result<()> {
    for case in &document.cases {
        let adjudicated = validated
            .adjudication_records
            .iter()
            .find(|record| record.case_id == case.id)
            .with_context(|| format!("adjudication is missing case {}", case.id))?;
        let declaration = case
            .declaration
            .as_ref()
            .with_context(|| format!("evaluation case {} is missing a declaration", case.id))?;
        if adjudicated.declaration.symbol() != *declaration
            || adjudicated.expected_usages != case.expected_usages
        {
            bail!(
                "evaluation case {} does not match adjudicated evidence",
                case.id
            );
        }
        let [lookup] = case.usage_lookups.as_slice() else {
            bail!(
                "evaluation case {} must contain one definition lookup",
                case.id
            );
        };
        if lookup.operation != NavigationOperation::Definition
            || lookup.expected_declaration != *declaration
            || adjudicated.definition_usage.as_ref() != Some(&lookup.usage)
        {
            bail!(
                "evaluation case {} definition lookup does not match adjudication",
                case.id
            );
        }
        for records in &validated.reviewer_records {
            let reviewed = records
                .iter()
                .find(|record| record.case_id == case.id)
                .with_context(|| format!("reviewer evidence is missing case {}", case.id))?;
            if reviewed.declaration.symbol() != *declaration {
                bail!(
                    "reviewer evidence declaration does not match case {}",
                    case.id
                );
            }
        }
    }
    Ok(())
}

fn validate_review_contract(
    document: &BenchmarkDocument,
    review: &EvaluationReview,
    repo_root: &Path,
) -> Result<()> {
    if !review.adjudication.sessions.is_empty() {
        bail!("adjudication evidence cannot contain agent review sessions");
    }
    match review.schema_version {
        1 => {
            if review
                .reviewers
                .iter()
                .any(|reviewer| !reviewer.sessions.is_empty())
            {
                bail!("legacy schema-v1 review evidence cannot contain sessions");
            }
            if !matches!(
                document.ground_truth.status,
                GroundTruthReviewStatus::AgentReviewed
                    | GroundTruthReviewStatus::IndependentlyReviewed
            ) {
                bail!("legacy schema-v1 review evidence requires a compatible review status");
            }
        }
        2 => {
            let tier = review
                .review_tier
                .context("schema-v2 review evidence requires reviewTier")?;
            let expected_status = match tier {
                ReviewTier::AgentReviewed => GroundTruthReviewStatus::AgentReviewed,
                ReviewTier::HumanAdjudicatedAgentPanel => {
                    GroundTruthReviewStatus::HumanAdjudicatedAgentPanel
                }
                ReviewTier::IndependentlyHumanReviewed => {
                    GroundTruthReviewStatus::IndependentlyReviewed
                }
            };
            if document.ground_truth.status != expected_status {
                bail!("reviewTier does not match groundTruth status");
            }

            let review_protocol_link = review
                .review_protocol
                .as_ref()
                .context("schema-v2 review evidence requires reviewProtocol")?;
            validate_linked_artifact(review_protocol_link, repo_root, "review protocol")?;
            let canonical_agent_protocol = if matches!(
                tier,
                ReviewTier::AgentReviewed | ReviewTier::HumanAdjudicatedAgentPanel
            ) {
                require_same(
                    "canonical agent review protocol path",
                    &review_protocol_link.file,
                    CANONICAL_AGENT_REVIEW_PROTOCOL,
                )?;
                let protocol_path = evidence_path(repo_root, &review_protocol_link.file)?;
                let protocol: AgentReviewProtocol =
                    serde_json::from_slice(&fs::read(&protocol_path).with_context(|| {
                        format!("read review protocol {}", protocol_path.display())
                    })?)
                    .with_context(|| {
                        format!("parse review protocol {}", protocol_path.display())
                    })?;
                if protocol.schema_version != 3 {
                    bail!("unsupported canonical agent review protocol schema version");
                }
                validate_linked_artifact(
                    &protocol.methodology,
                    repo_root,
                    "canonical agent methodology",
                )?;
                validate_linked_artifact(&protocol.prompt, repo_root, "canonical agent prompt")?;
                validate_linked_artifact(
                    &protocol.response_schema,
                    repo_root,
                    "canonical agent response schema",
                )?;
                Some(protocol)
            } else {
                None
            };

            let mut providers = BTreeSet::new();
            let mut execution_identities = BTreeSet::new();
            let mut session_mode = None;
            for reviewer in &review.reviewers {
                match tier {
                    ReviewTier::AgentReviewed | ReviewTier::HumanAdjudicatedAgentPanel => {
                        let canonical = canonical_agent_protocol
                            .as_ref()
                            .expect("agent review tiers loaded the canonical protocol");
                        let uses_sessions = !reviewer.sessions.is_empty();
                        if let Some(expected) = session_mode {
                            if expected != uses_sessions {
                                bail!(
                                    "agent review panels cannot mix singleton and per-case session evidence"
                                );
                            }
                        } else {
                            session_mode = Some(uses_sessions);
                        }
                        if uses_sessions {
                            if reviewer.participant.is_some()
                                || reviewer.prompt.is_some()
                                || reviewer.response_schema.is_some()
                                || reviewer.raw_response.is_some()
                            {
                                bail!(
                                    "per-case session reviewers cannot also declare singleton provenance"
                                );
                            }
                            let mut reviewer_cohort = None::<(String, String)>;
                            for session in &reviewer.sessions {
                                let (provider, execution_id) = validate_agent_submission(
                                    &session.participant,
                                    &session.prompt,
                                    &session.response_schema,
                                    &session.raw_response,
                                    canonical,
                                    repo_root,
                                )?;
                                let model = session
                                    .participant
                                    .model
                                    .as_deref()
                                    .expect("validated agent session has a model")
                                    .trim()
                                    .to_string();
                                let cohort = (provider.clone(), model);
                                if let Some(expected) = &reviewer_cohort {
                                    if expected != &cohort {
                                        bail!(
                                            "agent reviewer {} sessions must use one provider and model cohort",
                                            reviewer.id
                                        );
                                    }
                                } else {
                                    reviewer_cohort = Some(cohort);
                                }
                                validate_linked_artifact(
                                    &session.packet,
                                    repo_root,
                                    "agent case packet",
                                )?;
                                if !execution_identities.insert((provider.clone(), execution_id)) {
                                    bail!(
                                        "agent review sessions contain a duplicate provider executionId"
                                    );
                                }
                                providers.insert(provider);
                            }
                        } else {
                            let participant = reviewer.participant.as_ref().context(
                                "schema-v2 singleton reviewer requires participant provenance",
                            )?;
                            let prompt = reviewer
                                .prompt
                                .as_ref()
                                .context("schema-v2 singleton reviewer requires agent prompt")?;
                            let response_schema = reviewer.response_schema.as_ref().context(
                                "schema-v2 singleton reviewer requires agent response schema",
                            )?;
                            let raw_response = reviewer.raw_response.as_ref().context(
                                "schema-v2 singleton reviewer requires raw agent response",
                            )?;
                            let (provider, execution_id) = validate_agent_submission(
                                participant,
                                prompt,
                                response_schema,
                                raw_response,
                                canonical,
                                repo_root,
                            )?;
                            if !execution_identities.insert((provider.clone(), execution_id)) {
                                bail!("agent reviewers contain a duplicate provider executionId");
                            }
                            providers.insert(provider);
                        }
                    }
                    ReviewTier::IndependentlyHumanReviewed => {
                        let participant = reviewer
                            .participant
                            .as_ref()
                            .context("schema-v2 human reviewer requires participant provenance")?;
                        if participant.kind != ParticipantKind::Human {
                            bail!("independently human-reviewed evidence requires human reviewers");
                        }
                        if !reviewer.sessions.is_empty() {
                            bail!("independently human-reviewed evidence cannot contain sessions");
                        }
                    }
                }
            }

            let adjudicator = review
                .adjudication
                .participant
                .as_ref()
                .context("schema-v2 adjudication requires participant provenance")?;
            if adjudicator.kind != ParticipantKind::Human {
                bail!("adjudication must identify an accountable human");
            }
            nonempty(
                adjudicator.identity.as_deref(),
                "human adjudicator identity",
            )?;
            nonempty(
                adjudicator.attestation.as_deref(),
                "human adjudicator attestation",
            )?;
            if matches!(
                tier,
                ReviewTier::AgentReviewed | ReviewTier::HumanAdjudicatedAgentPanel
            ) && providers.len() < 2
            {
                bail!("agent review panels require at least two agent providers");
            }
        }
        version => bail!("unsupported evaluation review schema version {version}"),
    }
    Ok(())
}

fn validate_session_coverage(
    reviewer: &ReviewArtifact,
    selected_case_ids: &BTreeSet<String>,
) -> Result<()> {
    let session_case_ids = reviewer
        .sessions
        .iter()
        .map(|session| session.case_id.clone())
        .collect::<BTreeSet<_>>();
    if session_case_ids.len() != reviewer.sessions.len() || &session_case_ids != selected_case_ids {
        bail!(
            "agent reviewer {} sessions must cover each selected case exactly once",
            reviewer.id
        );
    }
    Ok(())
}

fn validate_per_case_provider_diversity(
    providers_by_case: &BTreeMap<String, BTreeSet<String>>,
) -> Result<()> {
    for (case_id, providers) in providers_by_case {
        if providers.len() < 2 {
            bail!("agent review panel case {case_id} requires at least two agent providers");
        }
    }
    Ok(())
}

fn nonempty<'a>(value: Option<&'a str>, kind: &str) -> Result<&'a str> {
    value
        .filter(|value| !value.trim().is_empty())
        .with_context(|| format!("{kind} must be non-empty"))
}

fn validate_agent_submission(
    participant: &ReviewParticipant,
    prompt: &ArtifactLink,
    response_schema: &ArtifactLink,
    raw_response: &ArtifactLink,
    canonical: &AgentReviewProtocol,
    repo_root: &Path,
) -> Result<(String, String)> {
    if participant.kind != ParticipantKind::Agent {
        bail!("agent review tiers require agent participants");
    }
    let provider = nonempty(participant.provider.as_deref(), "agent reviewer provider")?
        .trim()
        .to_ascii_lowercase();
    nonempty(participant.model.as_deref(), "agent reviewer model")?;
    let execution_id = nonempty(
        participant.execution_id.as_deref(),
        "agent reviewer executionId",
    )?
    .trim()
    .to_string();
    nonempty(
        participant.executed_at.as_deref(),
        "agent reviewer executedAt",
    )?;
    if prompt != &canonical.prompt || response_schema != &canonical.response_schema {
        bail!("agent reviewers must use the canonical prompt and response schema");
    }
    for (link, kind) in [
        (prompt, "agent prompt"),
        (response_schema, "agent response schema"),
        (raw_response, "raw agent response"),
    ] {
        validate_linked_artifact(link, repo_root, kind)?;
    }
    Ok((provider, execution_id))
}

fn validate_linked_artifact(link: &ArtifactLink, repo_root: &Path, kind: &str) -> Result<()> {
    let path = evidence_path(repo_root, &link.file)?;
    let bytes = fs::read(&path).with_context(|| format!("read {kind} {}", path.display()))?;
    validate_link(link, &path, &bytes, kind)
}

fn load_linked_json<T: DeserializeOwned>(
    link: &ArtifactLink,
    repo_root: &Path,
    kind: &str,
) -> Result<T> {
    let path = evidence_path(repo_root, &link.file)?;
    let bytes = fs::read(&path).with_context(|| format!("read {kind} {}", path.display()))?;
    validate_link(link, &path, &bytes, kind)?;
    serde_json::from_slice(&bytes).with_context(|| format!("parse {kind} {}", path.display()))
}

fn compile_agent_response_schema(
    schema_link: &ArtifactLink,
    repo_root: &Path,
) -> Result<jsonschema::JSONSchema> {
    let schema_path = evidence_path(repo_root, &schema_link.file)?;
    let schema: serde_json::Value =
        load_linked_json(schema_link, repo_root, "agent response schema")?;
    jsonschema::JSONSchema::compile(&schema).map_err(|error| {
        anyhow!(
            "compile agent response schema {}: {error}",
            schema_path.display()
        )
    })
}

fn validate_agent_response(
    compiled_schema: &jsonschema::JSONSchema,
    response_link: &ArtifactLink,
    participant: &ReviewParticipant,
    repo_root: &Path,
) -> Result<AgentResponse> {
    let response: serde_json::Value =
        load_linked_json(response_link, repo_root, "raw agent response")?;
    if let Err(errors) = compiled_schema.validate(&response) {
        let details = errors
            .map(|error| error.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        bail!("raw agent response failed schema validation: {details}");
    }
    let typed: AgentResponse =
        serde_json::from_value(response).context("deserialize validated raw agent response")?;
    if typed.schema_version != 1 {
        bail!("unsupported raw agent response schema version");
    }
    require_same(
        "raw agent response provider",
        typed.reviewer.provider.trim(),
        participant.provider.as_deref().unwrap_or_default().trim(),
    )?;
    require_same(
        "raw agent response model",
        typed.reviewer.model.trim(),
        participant.model.as_deref().unwrap_or_default().trim(),
    )?;
    require_same(
        "raw agent response executionId",
        typed.reviewer.execution_id.trim(),
        participant
            .execution_id
            .as_deref()
            .unwrap_or_default()
            .trim(),
    )?;
    require_same(
        "raw agent response executedAt",
        typed.reviewer.executed_at.trim(),
        participant
            .executed_at
            .as_deref()
            .unwrap_or_default()
            .trim(),
    )?;
    Ok(typed)
}

fn validate_raw_response_matches_evidence(
    raw: &AgentResponse,
    evidence: &ReviewerEvidence,
    selected_case_ids: &BTreeSet<String>,
) -> Result<()> {
    let raw_case_ids = raw
        .records
        .iter()
        .map(|record| record.case_id.clone())
        .collect::<BTreeSet<_>>();
    if raw_case_ids.len() != raw.records.len() || &raw_case_ids != selected_case_ids {
        bail!("raw agent response must contain each selected case exactly once");
    }
    for raw_record in &raw.records {
        let normalized = evidence
            .records
            .iter()
            .find(|record| record.case_id == raw_record.case_id)
            .expect("normalized reviewer evidence coverage was validated");
        if normalized_agent_decision(&raw_record.decision)? != normalized.decision
            || raw_record.declaration != normalized.declaration.location
            || raw_record.definition_usage
                != normalized
                    .definition_usage
                    .as_ref()
                    .map(|usage| usage.location.clone())
        {
            bail!(
                "raw agent response does not match normalized evidence for case {}",
                normalized.case_id
            );
        }
        let raw_required = raw_record
            .locations
            .iter()
            .filter(|location| location.classification == "required")
            .map(|location| location.location.clone())
            .collect::<Vec<_>>();
        let normalized_required = normalized
            .expected_usages
            .iter()
            .map(|usage| usage.location.clone())
            .collect::<Vec<_>>();
        if raw_required.len() != normalized_required.len()
            || !raw_required
                .iter()
                .all(|location| normalized_required.contains(location))
        {
            bail!(
                "raw agent response required locations do not match normalized evidence for case {}",
                normalized.case_id
            );
        }
        if raw_required.contains(&raw_record.declaration) {
            bail!(
                "raw agent response includes the selected declaration as a usage for case {}",
                normalized.case_id
            );
        }
        if raw_required
            .iter()
            .enumerate()
            .any(|(index, location)| raw_required[index + 1..].contains(location))
        {
            bail!(
                "raw agent response contains duplicate required locations for case {}",
                normalized.case_id
            );
        }
        let deterministic_definition = raw_required.iter().min_by_key(|location| {
            (
                location.uri.as_str(),
                location.range.start.line,
                location.range.start.character,
            )
        });
        if raw_record.definition_usage.as_ref() != deterministic_definition {
            bail!(
                "raw agent response does not use the deterministic definition target for case {}",
                normalized.case_id
            );
        }
    }
    Ok(())
}

fn normalized_agent_decision(decision: &str) -> Result<&str> {
    match decision {
        "accept" => Ok("accepted"),
        "replace" => Ok("replace"),
        "abstain" => Ok("abstain"),
        other => bail!("unsupported raw agent decision {other}"),
    }
}

fn validate_agent_case_packet(
    session: &AgentReviewSession,
    normalized: &EvidenceRecord,
    selection: &EvaluationSelection,
    sources: &[MaterializedSource],
    repo_root: &Path,
) -> Result<()> {
    let packet: AgentCasePacket =
        load_linked_json(&session.packet, repo_root, "agent case packet")?;
    if packet.schema_version != 2 {
        bail!("unsupported agent case packet schema version");
    }
    require_same(
        "agent case packet caseId",
        &packet.case_id,
        &session.case_id,
    )?;
    require_same(
        "agent case packet referencePolicy",
        &packet.reference_policy,
        "bindings_optional",
    )?;
    require_same(
        "agent case packet positionEncoding",
        &packet.position_encoding,
        "utf-16",
    )?;
    require_same(
        "agent case packet source root",
        &packet.source.root,
        "source",
    )?;
    if packet.declaration != normalized.declaration.location {
        bail!(
            "agent case packet declaration does not match normalized evidence for case {}",
            session.case_id
        );
    }
    let selected = selection
        .documents
        .iter()
        .filter(|document| document.case_ids.contains(&session.case_id))
        .collect::<Vec<_>>();
    let [selected] = selected.as_slice() else {
        bail!(
            "agent case packet case {} does not map to exactly one selected document",
            session.case_id
        );
    };
    if packet.language != selected.language
        || packet.language != normalized.declaration.language
        || packet.display_name != normalized.declaration.display_name
    {
        bail!(
            "agent case packet language or display name does not match the selection for case {}",
            session.case_id
        );
    }
    if packet.source.repo != selected.source.repo || packet.source.commit != selected.source.commit
    {
        bail!(
            "agent case packet source does not match the selection for case {}",
            session.case_id
        );
    }
    let source = sources
        .iter()
        .find(|source| source.repo == packet.source.repo && source.commit == packet.source.commit)
        .with_context(|| {
            format!(
                "agent case packet source is absent from the source lock for case {}",
                session.case_id
            )
        })?;
    if !is_hex_digest(&packet.source.archive_sha256)
        || packet.source.archive_sha256 != source.sha256
    {
        bail!(
            "agent case packet archive digest does not match the source lock for case {}",
            session.case_id
        );
    }
    Ok(())
}

fn validate_panel_escalations(
    responses: &[AgentResponse],
    adjudication: &[EvidenceRecord],
    selected_case_ids: &BTreeSet<String>,
) -> Result<()> {
    for case_id in selected_case_ids {
        let records = responses
            .iter()
            .filter_map(|response| {
                response
                    .records
                    .iter()
                    .find(|record| &record.case_id == case_id)
            })
            .collect::<Vec<_>>();
        let first = records.first().context("agent panel has no responses")?;
        let required = |record: &AgentResponseRecord| {
            record
                .locations
                .iter()
                .filter(|location| location.classification == "required")
                .map(|location| location.location.clone())
                .collect::<Vec<_>>()
        };
        let first_required = required(first);
        let exact_agreement = records.iter().all(|record| {
            let record_required = required(record);
            record.decision == first.decision
                && record.declaration == first.declaration
                && record_required.len() == first_required.len()
                && record_required
                    .iter()
                    .all(|location| first_required.contains(location))
                && record.definition_usage == first.definition_usage
        });
        let requires_escalation = !exact_agreement
            || records.iter().any(|record| {
                record.decision != "accept"
                    || record.confidence != "high"
                    || !record.ambiguities.is_empty()
            });
        let human_recorded = adjudication.iter().any(|record| &record.case_id == case_id);
        if (requires_escalation || !responses.is_empty()) && !human_recorded {
            bail!("case {case_id} requires accountable human adjudication");
        }
    }
    Ok(())
}

fn load_review_artifact<T: DeserializeOwned>(
    artifact: &ReviewArtifact,
    repo_root: &Path,
    kind: &str,
) -> Result<T> {
    let path = evidence_path(repo_root, &artifact.file)?;
    let bytes = fs::read(&path).with_context(|| format!("read {kind} {}", path.display()))?;
    if !is_hex_digest(&artifact.sha256) || sha256(&bytes) != artifact.sha256 {
        bail!("{kind} sha256 does not match {}", path.display());
    }
    serde_json::from_slice(&bytes).with_context(|| format!("parse {kind} {}", path.display()))
}

fn validate_reviewer_evidence_records(
    records: &[EvidenceRecord],
    selected_case_ids: &BTreeSet<String>,
    kind: &str,
) -> Result<()> {
    validate_evidence_record_coverage(records, selected_case_ids, kind)?;
    for record in records {
        if !matches!(record.decision.as_str(), "accepted" | "replace" | "abstain") {
            bail!("{kind} contains an unsupported reviewer decision");
        }
        if record.decision == "accepted" && record.definition_usage.is_none() {
            bail!("{kind} case {} has no definition usage", record.case_id);
        }
        if let Some(definition_usage) = &record.definition_usage {
            if !record.expected_usages.contains(definition_usage) {
                bail!("{kind} definition usage is not an expected usage");
            }
        }
        validate_evidence_record_declaration(record, kind)?;
    }
    Ok(())
}

fn validate_final_evidence_records(
    records: &[EvidenceRecord],
    selected_case_ids: &BTreeSet<String>,
    kind: &str,
) -> Result<()> {
    validate_evidence_record_coverage(records, selected_case_ids, kind)?;
    for record in records {
        if record.decision != "accepted" {
            bail!("{kind} contains a non-accepted final decision");
        }
        let definition_usage = record
            .definition_usage
            .as_ref()
            .with_context(|| format!("{kind} case {} has no definition usage", record.case_id))?;
        if !record.expected_usages.contains(definition_usage) {
            bail!("{kind} definition usage is not an expected usage");
        }
        validate_evidence_record_declaration(record, kind)?;
    }
    Ok(())
}

fn validate_evidence_record_coverage(
    records: &[EvidenceRecord],
    selected_case_ids: &BTreeSet<String>,
    kind: &str,
) -> Result<()> {
    let case_ids = records
        .iter()
        .map(|record| record.case_id.clone())
        .collect::<BTreeSet<_>>();
    if case_ids.len() != records.len() || &case_ids != selected_case_ids {
        bail!("{kind} case IDs do not exactly cover the selection");
    }
    Ok(())
}

fn validate_evidence_record_declaration(record: &EvidenceRecord, kind: &str) -> Result<()> {
    if !is_exact_git_commit(&record.declaration.commit)
        || record.declaration.repository.trim().is_empty()
        || record.declaration.language.trim().is_empty()
        || record.declaration.selection_rank == 0
    {
        bail!("{kind} contains an invalid declaration identity");
    }
    Ok(())
}

fn validate_evidence_identity(
    record: &EvidenceRecord,
    selection: &EvaluationSelection,
    kind: &str,
) -> Result<()> {
    let matches = selection
        .profiles
        .iter()
        .flat_map(|profile| {
            profile.selected.iter().flat_map(move |repository| {
                repository
                    .declaration_draw
                    .selected
                    .iter()
                    .filter(move |declaration| declaration.case_id == record.case_id)
                    .map(move |declaration| (profile, repository, declaration))
            })
        })
        .collect::<Vec<_>>();
    let [(profile, repository, selected)] = matches.as_slice() else {
        bail!("{kind} case identity does not resolve to exactly one declaration slot");
    };
    if record.declaration.repository != repository.full_name
        || record.declaration.commit != repository.source.commit
        || record.declaration.language != profile.language
        || record.declaration.selection_rank != selected.rank
        || record.declaration.symbol() != selected.symbol()
    {
        bail!(
            "{kind} case {} does not match selection identity",
            record.case_id
        );
    }
    Ok(())
}

fn validate_source_lock(
    sources: &[MaterializedSource],
    selected: &GitSource,
    repo_root: &Path,
) -> Result<()> {
    let matching = sources
        .iter()
        .filter(|source| source.repo == selected.repo && source.commit == selected.commit)
        .collect::<Vec<_>>();
    let [source] = matching.as_slice() else {
        bail!("source lock does not contain exactly one entry for the selected git source");
    };
    if !is_exact_git_commit(&source.tree)
        || source.archive_tree != source.tree
        || !is_hex_digest(&source.sha256)
        || safe_repo_relative_path(&source.archive, "materialized source archive").is_err()
    {
        bail!("source lock contains an invalid materialized source entry");
    }
    let archive_path = evidence_path(repo_root, &source.archive)?;
    let archive_size = fs::metadata(&archive_path)
        .with_context(|| format!("read metadata for {}", archive_path.display()))?
        .len();
    if archive_size > MAX_COMPRESSED_ARCHIVE_BYTES {
        bail!(
            "materialized source archive {} exceeds the compressed-size limit",
            archive_path.display()
        );
    }
    let archive = fs::read(&archive_path).with_context(|| {
        format!(
            "read materialized source archive {}",
            archive_path.display()
        )
    })?;
    if sha256(&archive) != source.sha256 {
        bail!(
            "materialized source archive sha256 does not match {}",
            archive_path.display()
        );
    }
    validate_commit_object(source)?;
    Ok(())
}

fn validate_commit_object(source: &MaterializedSource) -> Result<()> {
    let bytes = decode_hex(&source.commit_object).context("decode source lock commit object")?;
    let mut child = Command::new("git")
        .args(["hash-object", "-t", "commit", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("start Git commit-object verification")?;
    child
        .stdin
        .take()
        .context("open Git commit-object verification input")?
        .write_all(&bytes)?;
    let output = child
        .wait_with_output()
        .context("wait for Git commit-object verification")?;
    if !output.status.success() {
        bail!("Git commit-object verification failed");
    }
    require_same(
        "materialized source commit object",
        String::from_utf8(output.stdout)?.trim(),
        &source.commit,
    )?;
    let header = bytes
        .split(|byte| *byte == b'\n')
        .next()
        .context("source lock commit object is empty")?;
    require_same(
        "materialized source commit tree",
        std::str::from_utf8(header)?
            .strip_prefix("tree ")
            .unwrap_or(""),
        &source.tree,
    )
}

fn validate_archive_ranges(
    document: &BenchmarkDocument,
    sources: &[MaterializedSource],
    selected: &GitSource,
    repo_root: &Path,
) -> Result<()> {
    let source = sources
        .iter()
        .find(|source| source.repo == selected.repo && source.commit == selected.commit)
        .context("selected source is missing from source lock")?;
    let archive_path = evidence_path(repo_root, &source.archive)?;
    let mut locations = Vec::<&SymbolLocation>::new();
    for case in &document.cases {
        locations.extend(
            case.symbol_locations()
                .into_iter()
                .map(|(_, location)| location),
        );
    }

    let extracted = tempfile::tempdir().context("create archive extraction directory")?;
    let tree_entries =
        extract_materialized_archive(&archive_path, extracted.path(), &source.commit)?;
    let repository = tempfile::tempdir().context("create archive tree verification repository")?;
    git_status(repository.path(), &["init", "--bare", "--quiet"])?;
    let mut fast_import = Command::new("git")
        .args(["fast-import", "--quiet"])
        .current_dir(repository.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("start Git archive-tree reconstruction")?;
    let mut importer = fast_import
        .stdin
        .take()
        .context("open Git archive-tree reconstruction input")?;
    importer.write_all(
        b"feature done\ncommit refs/heads/archive\ncommitter UsageBench <usagebench@example.invalid> 0 +0000\ndata 0\n\n",
    )?;
    for entry in tree_entries {
        let path = extracted.path().join(&entry.path);
        let bytes = if entry.mode == "120000" {
            fs::read_link(&path)?
                .to_str()
                .context("archived symlink target is not valid UTF-8")?
                .as_bytes()
                .to_vec()
        } else {
            fs::read(&path)
                .with_context(|| format!("read extracted archive entry {}", entry.path))?
        };
        fast_import_inline(&mut importer, entry.mode, &entry.path, &bytes)?;
    }
    for gitlink in &source.gitlinks {
        if !is_exact_git_commit(&gitlink.commit) {
            bail!("source lock contains an invalid gitlink commit");
        }
        let path = Path::new(&gitlink.path);
        validate_archive_path(path)?;
        writeln!(
            importer,
            "M 160000 {} {}",
            gitlink.commit,
            serde_json::to_string(&gitlink.path)?
        )?;
    }
    importer.write_all(b"\ndone\n")?;
    drop(importer);
    let output = fast_import
        .wait_with_output()
        .context("wait for Git archive-tree reconstruction")?;
    if !output.status.success() {
        bail!(
            "reconstruct materialized source Git tree: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let actual_tree = git_stdout(
        repository.path(),
        &["rev-parse", "refs/heads/archive^{tree}"],
    )?;
    require_same(
        "materialized source archive content tree",
        &actual_tree,
        &source.archive_tree,
    )?;

    for location in locations {
        let relative = benchmark_source_path(&location.location.uri)?;
        let relative = relative
            .to_str()
            .context("evaluation source path is not valid UTF-8")?
            .to_string();
        let source_path = extracted.path().join(&relative);
        let metadata = fs::symlink_metadata(&source_path).with_context(|| {
            format!(
                "location uri {} maps to missing archived source file",
                location.location.uri
            )
        })?;
        if !metadata.file_type().is_file() {
            bail!(
                "location uri {} does not map to a regular archived source file",
                location.location.uri
            );
        }
        let text = fs::read_to_string(&source_path)
            .with_context(|| format!("decode referenced archived source file {relative}"))?;
        location
            .location
            .range
            .validate_with_source_text(&text, document.position_encoding)
            .with_context(|| format!("range for {} in archived source", location.location.uri))?;
        if !location.location.range.is_zero_width() {
            let selected_text = location
                .location
                .range
                .text_from_source(&text, document.position_encoding)?;
            if selected_text != location.display_name {
                bail!(
                    "range for {} does not select displayName {:?}",
                    location.location.uri,
                    location.display_name
                );
            }
        }
    }
    Ok(())
}

fn validate_archive_path(path: &Path) -> Result<()> {
    if validate_safe_repo_relative_path(path, "materialized source archive entry").is_err()
        || path.as_os_str().to_string_lossy().starts_with('-')
        || path.components().next().is_some_and(
            |component| matches!(component, Component::Normal(value) if value == ".git"),
        )
    {
        bail!("materialized source archive contains an unsafe path");
    }
    Ok(())
}

fn validate_global_pax_header<R: Read>(
    entry: &mut tar::Entry<'_, R>,
    expected_commit: &str,
) -> Result<()> {
    if entry.size() > MAX_PAX_HEADER_BYTES {
        bail!("materialized source archive global PAX header is too large");
    }
    let extensions = entry
        .pax_extensions()
        .context("read materialized source global PAX header")?
        .context("materialized source global PAX header is empty")?
        .collect::<std::io::Result<Vec<_>>>()?;
    let [extension] = extensions.as_slice() else {
        bail!("materialized source archive has unexpected global PAX metadata");
    };
    if extension.key()? != "comment" || extension.value()? != expected_commit {
        bail!("materialized source archive has unexpected global PAX metadata");
    }
    Ok(())
}

fn fast_import_inline(
    importer: &mut impl Write,
    mode: &str,
    path: &str,
    bytes: &[u8],
) -> Result<()> {
    writeln!(importer, "M {mode} inline {}", serde_json::to_string(path)?)?;
    writeln!(importer, "data {}", bytes.len())?;
    importer.write_all(bytes)?;
    importer.write_all(b"\n")?;
    Ok(())
}

fn git_status(path: &Path, args: &[&str]) -> Result<()> {
    let status = Command::new("git")
        .args(args)
        .current_dir(path)
        .status()
        .with_context(|| format!("run git {}", args.join(" ")))?;
    if !status.success() {
        bail!("git {} failed", args.join(" "));
    }
    Ok(())
}

fn git_stdout(path: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(path)
        .output()
        .with_context(|| format!("run git {}", args.join(" ")))?;
    if !output.status.success() {
        bail!("git {} failed", args.join(" "));
    }
    String::from_utf8(output.stdout)
        .context("decode git output")
        .map(|value| value.trim().to_string())
}

fn is_hex_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn decode_hex(value: &str) -> Result<Vec<u8>> {
    if value.len() & 1 == 1 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("invalid hexadecimal value");
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair)?;
            u8::from_str_radix(text, 16).context("decode hexadecimal byte")
        })
        .collect()
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BenchmarkCase, CorpusMetadata, CorpusSelection, GroundTruthReview, Location, Position,
        PositionEncoding, Range, ReferencePolicy, SymbolKind, UsageLookup,
    };
    use serde_json::json;
    use tempfile::tempdir;

    const TREE: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn checked_in_real_project_protocols_are_valid() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        for (freeze_id, schema_version) in [("real-project-v1", 1), ("real-project-v2", 2)] {
            let path = root.join(format!("benchmarks/evaluation/{freeze_id}/protocol.json"));
            let (protocol, _) = load_checked::<EvaluationProtocol>(
                &path,
                EVALUATION_PROTOCOL_SCHEMA,
                "checked-in evaluation protocol",
            )
            .unwrap();

            validate_protocol_schema_version(
                protocol.schema_version,
                "checked-in evaluation protocol",
            )
            .unwrap();
            assert_eq!(protocol.schema_version, schema_version);
            assert_eq!(protocol.freeze_id, freeze_id);
            validate_profiles(&protocol.target_profiles).unwrap();
            for profile in protocol.target_profiles {
                assert!(root.join(profile.profile).is_file());
            }
        }
    }

    #[test]
    fn retained_v3_agent_panel_artifacts_are_hash_bound_and_schema_valid() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let protocol_path = root.join(CANONICAL_AGENT_REVIEW_PROTOCOL);
        let protocol: AgentReviewProtocol =
            serde_json::from_slice(&fs::read(protocol_path).unwrap()).unwrap();
        assert_eq!(protocol.schema_version, 3);
        validate_linked_artifact(&protocol.methodology, root, "canonical methodology").unwrap();
        validate_linked_artifact(&protocol.prompt, root, "canonical prompt").unwrap();
        validate_linked_artifact(&protocol.response_schema, root, "canonical schema").unwrap();

        let schema: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join(&protocol.response_schema.file)).unwrap())
                .unwrap();
        let compiled = jsonschema::JSONSchema::compile(&schema).unwrap();
        for run_file in [
            "benchmarks/review-protocol/runs/real-project-v1-agent-panel-pilot-v3/run.json",
            "benchmarks/review-protocol/runs/real-project-v1-agent-panel-milestone-1-v3/run.json",
            "benchmarks/review-protocol/runs/real-project-v1-agent-panel-milestone-2-v3/run.json",
            "benchmarks/review-protocol/runs/real-project-v1-agent-panel-milestone-3-v3/run.json",
            "benchmarks/review-protocol/runs/real-project-v1-agent-panel-milestone-4-v3/run.json",
            "benchmarks/review-protocol/runs/real-project-v1-agent-panel-milestone-5-v3/run.json",
        ] {
            let run: serde_json::Value =
                serde_json::from_slice(&fs::read(root.join(run_file)).unwrap()).unwrap();

            for field in ["protocol", "methodology", "prompt", "responseSchema"] {
                let link: ArtifactLink = serde_json::from_value(run[field].clone()).unwrap();
                validate_linked_artifact(&link, root, "retained protocol artifact").unwrap();
            }
            if !run["comparison"].is_null() {
                let comparison: ArtifactLink =
                    serde_json::from_value(run["comparison"].clone()).unwrap();
                validate_linked_artifact(&comparison, root, "retained comparison").unwrap();
            }

            if let Some(sources) = run["sources"].as_array() {
                for source in sources {
                    let link = ArtifactLink {
                        file: source["archive"].as_str().unwrap().to_string(),
                        sha256: source["sha256"].as_str().unwrap().to_string(),
                    };
                    validate_linked_artifact(&link, root, "retained source archive").unwrap();
                }
            }

            let packets = run["packets"].as_array().unwrap();
            assert_eq!(packets.len(), 6);
            for packet in packets {
                let link: ArtifactLink = serde_json::from_value(packet.clone()).unwrap();
                validate_linked_artifact(&link, root, "retained case packet").unwrap();
            }

            if !run["adjudication"].is_null() {
                let adjudication: ArtifactLink =
                    serde_json::from_value(run["adjudication"].clone()).unwrap();
                validate_linked_artifact(&adjudication, root, "retained human adjudication")
                    .unwrap();
            }

            let sessions = run["sessions"].as_array().unwrap();
            assert_eq!(sessions.len(), 12);
            let mut coverage = BTreeMap::<String, usize>::new();
            for session in sessions {
                let link: ArtifactLink =
                    serde_json::from_value(session["rawResponse"].clone()).unwrap();
                validate_linked_artifact(&link, root, "retained raw response").unwrap();
                let response: serde_json::Value =
                    serde_json::from_slice(&fs::read(root.join(&link.file)).unwrap()).unwrap();
                assert!(compiled.is_valid(&response));
                let typed: AgentResponse = serde_json::from_value(response).unwrap();
                assert_eq!(typed.records.len(), 1);
                assert_eq!(typed.reviewer.provider, session["provider"]);
                assert_eq!(typed.reviewer.model, session["model"]);
                assert_eq!(typed.reviewer.execution_id, session["executionId"]);
                if let Some(executed_at) = session["executedAt"].as_str() {
                    assert_eq!(typed.reviewer.executed_at, executed_at);
                }
                assert_eq!(typed.records[0].case_id, session["caseId"]);
                *coverage
                    .entry(typed.records[0].case_id.clone())
                    .or_default() += 1;
            }
            assert_eq!(coverage.len(), 6);
            assert!(coverage.values().all(|count| *count == 2));
        }
    }

    #[test]
    fn legacy_real_project_review_cannot_be_published() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let error = build_release_audit(root.join("benchmarks/cases/evaluation/real-project-v1"))
            .unwrap_err();
        assert!(format!("{error:#}").contains("requires schema-v2 review evidence"));
    }

    #[test]
    fn counted_audit_reasons_are_stable_and_aggregated() {
        let counts = counted_reasons(["too large", "missing marker", "too large"].into_iter());
        assert_eq!(
            counts,
            vec![
                EvaluationAuditCount {
                    reason: "missing marker".to_string(),
                    count: 1,
                },
                EvaluationAuditCount {
                    reason: "too large".to_string(),
                    count: 2,
                },
            ]
        );
    }

    #[test]
    fn evaluation_directory_must_cover_every_selected_document_and_case() {
        let commit = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let selection = EvaluationSelection {
            schema_version: 1,
            freeze_id: "real-project-v1".to_string(),
            protocol: ArtifactLink {
                file: "protocol.json".to_string(),
                sha256: "b".repeat(64),
            },
            protocol_commit: commit.to_string(),
            profiles: Vec::new(),
            replacements: Vec::new(),
            documents: vec![SelectedDocument {
                case_file: "benchmarks/cases/evaluation/real-project-v1/go-01.yaml".to_string(),
                language: "go".to_string(),
                candidate_id: "gopls".to_string(),
                source: GitSource {
                    repo: Url::parse("https://github.com/example/project").unwrap(),
                    commit: commit.to_string(),
                },
                case_ids: vec!["case-1".to_string(), "case-2".to_string()],
            }],
        };
        let actual = BTreeMap::from([(
            "benchmarks/cases/evaluation/real-project-v1/go-01.yaml".to_string(),
            BTreeSet::from(["case-1".to_string()]),
        )]);
        let protocol = EvaluationProtocol {
            schema_version: 1,
            freeze_id: "real-project-v1".to_string(),
            target_profiles: vec![TargetProfile {
                language: "go".to_string(),
                candidate_id: "gopls".to_string(),
                profile: "adapters/lsp/gopls.json".to_string(),
            }],
            sampling: EvaluationSampling {
                repositories_per_profile: 1,
                declarations_per_repository: 2,
            },
            claim_scope: "descriptive test scope".to_string(),
        };

        let error = validate_selection_coverage(&actual, &selection, &protocol).unwrap_err();
        assert!(format!("{error:#}").contains("do not exactly cover the selection"));
    }

    #[test]
    fn matching_evaluation_evidence_is_accepted() {
        let temp = tempdir().unwrap();
        let root = temp.path();
        let archive_input = root.join("archive-input");
        fs::create_dir_all(&archive_input).unwrap();
        git(&archive_input, &["init"]);
        git(
            &archive_input,
            &["config", "user.email", "usagebench@example.test"],
        );
        git(&archive_input, &["config", "user.name", "UsageBench test"]);
        git(&archive_input, &["config", "commit.gpgSign", "false"]);
        fs::write(archive_input.join("source.txt"), "archived archived").unwrap();
        git(&archive_input, &["add", "source.txt"]);
        git(&archive_input, &["commit", "-m", "archive source"]);
        let commit = git_stdout(&archive_input, &["rev-parse", "HEAD"]);
        let tree = git_stdout(&archive_input, &["rev-parse", "HEAD^{tree}"]);
        let commit_object = Command::new("git")
            .args(["cat-file", "commit", &commit])
            .current_dir(&archive_input)
            .output()
            .unwrap()
            .stdout
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let protocol_path = root.join("protocol.json");
        write_json(
            &protocol_path,
            json!({
                "schemaVersion": 1,
                "freezeId": "real-project-v1",
                "targetProfiles": [
                    {"language": "go", "candidateId": "gopls", "profile": "adapters/lsp/gopls.json"},
                    {"language": "python", "candidateId": "pyright", "profile": "adapters/lsp/pyright.json"}
                ],
                "population": {"snapshot": "population.json", "eligibility": "documented", "exclusions": "documented", "minimumStars": 100},
                "sampling": {"seedDerivation": "protocol commit", "repositoriesPerProfile": 4, "declarationsPerRepository": 3, "replacementRule": "documented"},
                "operations": ["references", "definition"],
                "claimScope": "the sampled repositories"
            }),
        );
        let selection_path = root.join("selection.json");
        write_json(
            &selection_path,
            json!({
                "schemaVersion": 1,
                "freezeId": "real-project-v1",
                "protocol": artifact_link("protocol.json", &protocol_path),
                "population": {"file": "population.json", "sha256": "0000000000000000000000000000000000000000000000000000000000000000"},
                "protocolCommit": commit,
                "profiles": [{
                    "language": "go",
                    "candidateId": "gopls",
                    "ranked": [],
                    "selected": [{
                        "fullName": "example/project",
                        "source": {"repo": "https://github.com/example/project.git", "commit": commit},
                        "caseFile": "cases/example.yaml",
                        "caseIds": ["selected-call"],
                        "declarationDraw": {"selected": [{
                            "caseId": "selected-call",
                            "rank": 1,
                            "uri": "benchmark://source/source.txt",
                            "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 8}},
                            "kind": "variable",
                            "displayName": "archived"
                        }]}
                    }]
                }],
                "replacements": [],
                "documents": [{
                    "caseFile": "cases/example.yaml",
                    "language": "go",
                    "candidateId": "gopls",
                    "source": {"repo": "https://github.com/example/project.git", "commit": commit},
                    "caseIds": ["selected-call"]
                }]
            }),
        );
        let reviewer_a = root.join("review-a.json");
        let reviewer_b = root.join("review-b.json");
        let adjudication = root.join("adjudication.json");
        write_evidence_artifacts(root, &commit, &commit);
        let review_path = root.join("review.json");
        write_json(
            &review_path,
            json!({
                "schemaVersion": 1,
                "freezeId": "real-project-v1",
                "selection": artifact_link("selection.json", &selection_path),
                "reviewers": [
                    review_artifact("alice", "review-a.json", &reviewer_a),
                    review_artifact("bob", "review-b.json", &reviewer_b)
                ],
                "adjudication": review_artifact("adjudication", "adjudication.json", &adjudication)
            }),
        );
        let source_lock_path = root.join("sources.json");
        let archive = root.join("sources/example-project.tar.gz");
        fs::create_dir_all(archive.parent().unwrap()).unwrap();
        let status = Command::new("git")
            .arg("archive")
            .arg("--format=tar.gz")
            .arg("--output")
            .arg(&archive)
            .arg("HEAD")
            .current_dir(&archive_input)
            .status()
            .unwrap();
        assert!(status.success());
        write_json(
            &source_lock_path,
            json!({
                "schemaVersion": 1,
                "freezeId": "real-project-v1",
                "selection": artifact_link("selection.json", &selection_path),
                "sources": [{
                    "repo": "https://github.com/example/project.git",
                    "commit": commit,
                    "commitObject": commit_object,
                    "tree": tree,
                    "archiveTree": tree,
                    "archive": "sources/example-project.tar.gz",
                    "sha256": sha256(&fs::read(&archive).unwrap())
                }]
            }),
        );
        let case_file = root.join("cases/example.yaml");
        fs::create_dir_all(case_file.parent().unwrap()).unwrap();
        fs::write(&case_file, "placeholder").unwrap();
        let document = document(&commit);

        validate_document_evidence(&document, &case_file, root).unwrap();
        let extracted = materialized_source_root(&document, root, &root.join("work"))
            .unwrap()
            .unwrap();
        assert_eq!(
            fs::read_to_string(extracted.join("source.txt")).unwrap(),
            "archived archived"
        );
    }

    #[test]
    fn review_schema_accepts_per_case_sessions_and_rejects_mixed_provenance() {
        let schema: serde_json::Value = serde_json::from_str(EVALUATION_REVIEW_SCHEMA).unwrap();
        let compiled = jsonschema::JSONSchema::compile(&schema).unwrap();
        let link = json!({"file": "evidence.json", "sha256": "a".repeat(64)});
        let participant = |provider: &str, execution_id: &str| {
            json!({
                "kind": "agent",
                "provider": provider,
                "model": format!("{provider}-model"),
                "executionId": execution_id,
                "executedAt": "2026-08-07T00:00:00Z"
            })
        };
        let session = |provider: &str, execution_id: &str| {
            json!({
                "caseId": "selected-call",
                "participant": participant(provider, execution_id),
                "packet": link.clone(),
                "prompt": link.clone(),
                "responseSchema": link.clone(),
                "rawResponse": link.clone()
            })
        };
        let reviewer = |id: &str, provider: &str, execution_id: &str| {
            json!({
                "id": id,
                "file": "reviewer.json",
                "sha256": "b".repeat(64),
                "sessions": [session(provider, execution_id)]
            })
        };
        let mut manifest = json!({
            "schemaVersion": 2,
            "freezeId": "real-project-v1",
            "selection": link.clone(),
            "reviewTier": "human_adjudicated_agent_panel",
            "reviewProtocol": link.clone(),
            "reviewers": [
                reviewer("openai-reviewer", "openai", "openai-1"),
                reviewer("anthropic-reviewer", "anthropic", "anthropic-1")
            ],
            "adjudication": {
                "id": "human-adjudication",
                "file": "adjudication.json",
                "sha256": "c".repeat(64),
                "participant": {
                    "kind": "human",
                    "identity": "human@example.test",
                    "attestation": "I reviewed every case."
                }
            }
        });
        assert!(compiled.is_valid(&manifest));

        let mut mixed_panel = manifest.clone();
        mixed_panel["reviewers"][0] = json!({
            "id": "openai-reviewer",
            "file": "reviewer.json",
            "sha256": "b".repeat(64),
            "participant": participant("openai", "openai-batch"),
            "prompt": link.clone(),
            "responseSchema": link.clone(),
            "rawResponse": link.clone()
        });
        assert!(!compiled.is_valid(&mixed_panel));

        let mut invalid_adjudication = manifest.clone();
        invalid_adjudication["adjudication"]["sessions"] =
            json!([session("openai", "adjudication-session")]);
        assert!(!compiled.is_valid(&invalid_adjudication));

        manifest["reviewers"][0]["participant"] = participant("openai", "openai-batch");
        assert!(!compiled.is_valid(&manifest));
    }

    #[test]
    fn raw_agent_decisions_map_to_provisional_reviewer_evidence() {
        let declaration = Location {
            uri: Url::parse("benchmark://source/source.txt").unwrap(),
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 8,
                },
            },
        };
        let usage = Location {
            uri: declaration.uri.clone(),
            range: Range {
                start: Position {
                    line: 0,
                    character: 9,
                },
                end: Position {
                    line: 0,
                    character: 17,
                },
            },
        };
        let normalized_location = SymbolLocation {
            location: usage.clone(),
            kind: SymbolKind::Variable,
            display_name: "archived".to_string(),
            disambiguation: None,
        };
        let evidence_record = |decision: &str, include_usage: bool| EvidenceRecord {
            case_id: "selected-call".to_string(),
            decision: decision.to_string(),
            declaration: EvidenceDeclaration {
                repository: "example/project".to_string(),
                commit: "a".repeat(40),
                language: "go".to_string(),
                selection_rank: 1,
                location: declaration.clone(),
                kind: SymbolKind::Variable,
                display_name: "archived".to_string(),
            },
            expected_usages: if include_usage {
                vec![normalized_location.clone()]
            } else {
                Vec::new()
            },
            definition_usage: include_usage.then(|| normalized_location.clone()),
        };
        let raw_response = |decision: &str, include_usage: bool| AgentResponse {
            schema_version: 1,
            reviewer: AgentResponseReviewer {
                provider: "openai".to_string(),
                model: "model".to_string(),
                execution_id: "execution".to_string(),
                executed_at: "2026-08-07T00:00:00Z".to_string(),
            },
            records: vec![AgentResponseRecord {
                case_id: "selected-call".to_string(),
                decision: decision.to_string(),
                confidence: "high".to_string(),
                declaration: declaration.clone(),
                locations: if include_usage {
                    vec![AgentReviewedLocation {
                        location: usage.clone(),
                        classification: "required".to_string(),
                    }]
                } else {
                    Vec::new()
                },
                definition_usage: include_usage.then(|| usage.clone()),
                ambiguities: Vec::new(),
            }],
        };
        let case_ids = BTreeSet::from(["selected-call".to_string()]);
        for (raw_decision, normalized_decision, include_usage) in [
            ("accept", "accepted", true),
            ("replace", "replace", true),
            ("abstain", "abstain", false),
        ] {
            let evidence = ReviewerEvidence {
                schema_version: 1,
                reviewer: "reviewer".to_string(),
                reference_policy: "bindings_optional".to_string(),
                selection_algorithm: "real-project-v1-source-syntax-v1".to_string(),
                records: vec![evidence_record(normalized_decision, include_usage)],
            };
            validate_reviewer_evidence_records(&evidence.records, &case_ids, "reviewer evidence")
                .unwrap();
            validate_raw_response_matches_evidence(
                &raw_response(raw_decision, include_usage),
                &evidence,
                &case_ids,
            )
            .unwrap();
        }

        let provisional = vec![evidence_record("replace", true)];
        let error =
            validate_final_evidence_records(&provisional, &case_ids, "adjudication").unwrap_err();
        assert!(format!("{error:#}").contains("non-accepted final decision"));
    }

    #[test]
    fn agent_case_packet_is_bound_to_selection_and_source_lock() {
        let temp = tempdir().unwrap();
        let root = temp.path();
        let commit = "a".repeat(40);
        let archive_sha256 = "d".repeat(64);
        let declaration = Location {
            uri: Url::parse("benchmark://source/source.txt").unwrap(),
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 8,
                },
            },
        };
        let packet_path = root.join("packet.json");
        let write_packet = |archive_sha256: &str| {
            write_json(
                &packet_path,
                json!({
                    "schemaVersion": 2,
                    "caseId": "selected-call",
                    "language": "go",
                    "referencePolicy": "bindings_optional",
                    "positionEncoding": "utf-16",
                    "source": {
                        "root": "source",
                        "repo": "https://github.com/example/project.git",
                        "commit": commit.clone(),
                        "archiveSha256": archive_sha256
                    },
                    "declaration": declaration.clone(),
                    "displayName": "archived"
                }),
            );
        };
        write_packet(&archive_sha256);
        let packet_link = || ArtifactLink {
            file: "packet.json".to_string(),
            sha256: sha256(&fs::read(&packet_path).unwrap()),
        };
        let placeholder = ArtifactLink {
            file: "unused".to_string(),
            sha256: "f".repeat(64),
        };
        let session = |packet: ArtifactLink| AgentReviewSession {
            case_id: "selected-call".to_string(),
            participant: ReviewParticipant {
                kind: ParticipantKind::Agent,
                provider: Some("openai".to_string()),
                model: Some("model".to_string()),
                execution_id: Some("execution".to_string()),
                executed_at: Some("2026-08-07T00:00:00Z".to_string()),
                identity: None,
                attestation: None,
            },
            packet,
            prompt: placeholder.clone(),
            response_schema: placeholder.clone(),
            raw_response: placeholder.clone(),
        };
        let selection = EvaluationSelection {
            schema_version: 1,
            freeze_id: "real-project-v1".to_string(),
            protocol: placeholder.clone(),
            protocol_commit: "b".repeat(40),
            profiles: Vec::new(),
            replacements: Vec::new(),
            documents: vec![SelectedDocument {
                case_file: "cases/example.yaml".to_string(),
                language: "go".to_string(),
                candidate_id: "gopls".to_string(),
                source: GitSource {
                    repo: Url::parse("https://github.com/example/project.git").unwrap(),
                    commit: commit.clone(),
                },
                case_ids: vec!["selected-call".to_string()],
            }],
        };
        let sources = vec![MaterializedSource {
            repo: Url::parse("https://github.com/example/project.git").unwrap(),
            commit: commit.clone(),
            commit_object: String::new(),
            tree: "c".repeat(40),
            archive_tree: "c".repeat(40),
            archive: "sources/example.tar.gz".to_string(),
            sha256: archive_sha256.clone(),
            gitlinks: Vec::new(),
        }];
        let normalized = EvidenceRecord {
            case_id: "selected-call".to_string(),
            decision: "accepted".to_string(),
            declaration: EvidenceDeclaration {
                repository: "example/project".to_string(),
                commit: commit.clone(),
                language: "go".to_string(),
                selection_rank: 1,
                location: declaration.clone(),
                kind: SymbolKind::Variable,
                display_name: "archived".to_string(),
            },
            expected_usages: Vec::new(),
            definition_usage: None,
        };
        validate_agent_case_packet(
            &session(packet_link()),
            &normalized,
            &selection,
            &sources,
            root,
        )
        .unwrap();

        write_packet(&"e".repeat(64));
        let error = validate_agent_case_packet(
            &session(packet_link()),
            &normalized,
            &selection,
            &sources,
            root,
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("does not match the source lock"));
    }

    #[test]
    fn agent_panels_require_cross_provider_and_stable_reviewer_cohorts() {
        let temp = tempdir().unwrap();
        let root = temp.path();
        let protocol_dir = root.join("benchmarks/review-protocol");
        fs::create_dir_all(&protocol_dir).unwrap();
        fs::write(
            protocol_dir.join("per-case-methodology-v3.md"),
            "methodology",
        )
        .unwrap();
        fs::write(protocol_dir.join("blinded-agent-prompt-v1.md"), "prompt").unwrap();
        write_json(
            &protocol_dir.join("agent-response-v1.schema.json"),
            json!({
                "type": "object",
                "required": ["reviewer", "records"],
                "properties": {
                    "reviewer": {
                        "type": "object",
                        "required": ["provider", "model", "executionId", "executedAt"],
                        "properties": {
                            "provider": {"type": "string"},
                            "model": {"type": "string"},
                            "executionId": {"type": "string"},
                            "executedAt": {"type": "string"}
                        }
                    },
                    "records": {"type": "array"}
                }
            }),
        );
        let prompt_link = ArtifactLink {
            file: "benchmarks/review-protocol/blinded-agent-prompt-v1.md".to_string(),
            sha256: sha256(&fs::read(protocol_dir.join("blinded-agent-prompt-v1.md")).unwrap()),
        };
        let methodology_link = ArtifactLink {
            file: "benchmarks/review-protocol/per-case-methodology-v3.md".to_string(),
            sha256: sha256(&fs::read(protocol_dir.join("per-case-methodology-v3.md")).unwrap()),
        };
        let response_schema_link = ArtifactLink {
            file: "benchmarks/review-protocol/agent-response-v1.schema.json".to_string(),
            sha256: sha256(&fs::read(protocol_dir.join("agent-response-v1.schema.json")).unwrap()),
        };
        write_json(
            &protocol_dir.join("blinded-agent-review-v3.json"),
            json!({"schemaVersion": 3, "methodology": methodology_link, "prompt": prompt_link, "responseSchema": response_schema_link}),
        );
        write_json(
            &root.join("openai.json"),
            json!({"schemaVersion": 1, "reviewer": {"provider": "openai", "model": "openai-model", "executionId": "run-agent-a", "executedAt": "2026-08-06T00:00:00Z"}, "records": []}),
        );
        write_json(
            &root.join("second.json"),
            json!({"schemaVersion": 1, "reviewer": {"provider": "openai", "model": "openai-model", "executionId": "run-agent-b", "executedAt": "2026-08-06T00:00:00Z"}, "records": []}),
        );
        let linked = |file: &str| ArtifactLink {
            file: file.to_string(),
            sha256: sha256(&fs::read(root.join(file)).unwrap()),
        };
        let reviewer = |id: &str, provider: &str, raw: &str| ReviewArtifact {
            id: id.to_string(),
            file: raw.to_string(),
            sha256: sha256(&fs::read(root.join(raw)).unwrap()),
            participant: Some(ReviewParticipant {
                kind: ParticipantKind::Agent,
                provider: Some(provider.to_string()),
                model: Some(format!("{provider}-model")),
                execution_id: Some(format!("run-{id}")),
                executed_at: Some("2026-08-06T00:00:00Z".to_string()),
                identity: None,
                attestation: None,
            }),
            prompt: Some(prompt_link.clone()),
            response_schema: Some(response_schema_link.clone()),
            raw_response: Some(linked(raw)),
            sessions: Vec::new(),
        };
        let adjudication = ReviewArtifact {
            id: "human-adjudication".to_string(),
            file: "openai.json".to_string(),
            sha256: sha256(&fs::read(root.join("openai.json")).unwrap()),
            participant: Some(ReviewParticipant {
                kind: ParticipantKind::Human,
                provider: None,
                model: None,
                execution_id: None,
                executed_at: None,
                identity: Some("human@example.test".to_string()),
                attestation: Some("I reviewed and accept every adjudicated case.".to_string()),
            }),
            prompt: None,
            response_schema: None,
            raw_response: None,
            sessions: Vec::new(),
        };
        let mut document = document(&"a".repeat(40));
        document.ground_truth.status = GroundTruthReviewStatus::HumanAdjudicatedAgentPanel;
        document.ground_truth.reviewers = vec!["agent-a".to_string(), "agent-b".to_string()];
        let mut review = EvaluationReview {
            schema_version: 2,
            freeze_id: "real-project-v1".to_string(),
            selection: response_schema_link.clone(),
            review_tier: Some(ReviewTier::HumanAdjudicatedAgentPanel),
            review_protocol: Some(ArtifactLink {
                file: CANONICAL_AGENT_REVIEW_PROTOCOL.to_string(),
                sha256: sha256(
                    &fs::read(protocol_dir.join("blinded-agent-review-v3.json")).unwrap(),
                ),
            }),
            reviewers: vec![
                reviewer("agent-a", "openai", "openai.json"),
                reviewer("agent-b", "openai", "second.json"),
            ],
            adjudication,
        };

        let error = validate_review_contract(&document, &review, root).unwrap_err();
        assert!(format!("{error:#}").contains("at least two agent providers"));

        review.review_tier = Some(ReviewTier::AgentReviewed);
        document.ground_truth.status = GroundTruthReviewStatus::AgentReviewed;
        let error = validate_review_contract(&document, &review, root).unwrap_err();
        assert!(format!("{error:#}").contains("at least two agent providers"));

        write_json(
            &root.join("second.json"),
            json!({"schemaVersion": 1, "reviewer": {"provider": "anthropic", "model": "anthropic-model", "executionId": "run-agent-b", "executedAt": "2026-08-06T00:00:00Z"}, "records": []}),
        );
        review.reviewers[1].participant.as_mut().unwrap().provider = Some("anthropic".to_string());
        review.reviewers[1].participant.as_mut().unwrap().model =
            Some("anthropic-model".to_string());
        review.reviewers[1].sha256 = sha256(&fs::read(root.join("second.json")).unwrap());
        review.reviewers[1].raw_response.as_mut().unwrap().sha256 =
            sha256(&fs::read(root.join("second.json")).unwrap());
        validate_review_contract(&document, &review, root).unwrap();

        write_json(
            &root.join("packet.json"),
            json!({"caseId": "selected-call"}),
        );
        let packet_link = linked("packet.json");
        let convert_to_session = |reviewer: &mut ReviewArtifact| {
            reviewer.sessions = vec![AgentReviewSession {
                case_id: "selected-call".to_string(),
                participant: reviewer.participant.take().unwrap(),
                packet: packet_link.clone(),
                prompt: reviewer.prompt.take().unwrap(),
                response_schema: reviewer.response_schema.take().unwrap(),
                raw_response: reviewer.raw_response.take().unwrap(),
            }];
        };
        convert_to_session(&mut review.reviewers[0]);
        let error = validate_review_contract(&document, &review, root).unwrap_err();
        assert!(format!("{error:#}").contains("cannot mix singleton and per-case session"));

        convert_to_session(&mut review.reviewers[1]);
        validate_review_contract(&document, &review, root).unwrap();
        let mut mixed_cohort = review.clone();
        let mut switched_session = mixed_cohort.reviewers[0].sessions[0].clone();
        switched_session.case_id = "second-case".to_string();
        switched_session.participant.provider = Some("anthropic".to_string());
        switched_session.participant.model = Some("anthropic-model".to_string());
        switched_session.participant.execution_id = Some("cohort-switch".to_string());
        switched_session.raw_response = linked("second.json");
        mixed_cohort.reviewers[0].sessions.push(switched_session);
        let error = validate_review_contract(&document, &mixed_cohort, root).unwrap_err();
        assert!(format!("{error:#}").contains("one provider and model cohort"));

        review.adjudication.sessions = review.reviewers[0].sessions.clone();
        let error = validate_review_contract(&document, &review, root).unwrap_err();
        assert!(format!("{error:#}").contains("adjudication evidence cannot contain"));
        review.adjudication.sessions.clear();
        let published = super::review_artifact(review.reviewers[0].clone()).unwrap();
        assert_eq!(published.sessions.len(), 1);
        assert_eq!(published.sessions[0].case_id, "selected-call");
        assert_eq!(published.sessions[0].packet.sha256, packet_link.sha256);
        let serialized = serde_json::to_value(&published).unwrap();
        assert_eq!(serialized["sessions"][0]["caseId"], "selected-call");
        let mut invalid_audit = review.reviewers[0].clone();
        invalid_audit.sessions[0].participant.provider = None;
        let error = super::review_artifact(invalid_audit).unwrap_err();
        assert!(format!("{error:#}").contains("release audit session provider"));

        let selected_case_ids = BTreeSet::from(["selected-call".to_string()]);
        validate_session_coverage(&review.reviewers[0], &selected_case_ids).unwrap();
        let duplicate = review.reviewers[0].sessions[0].clone();
        review.reviewers[0].sessions.push(duplicate);
        let error =
            validate_session_coverage(&review.reviewers[0], &selected_case_ids).unwrap_err();
        assert!(format!("{error:#}").contains("cover each selected case exactly once"));
    }

    #[test]
    fn provider_diversity_is_required_for_every_case() {
        let providers_by_case = BTreeMap::from([
            (
                "case-a".to_string(),
                BTreeSet::from(["openai".to_string(), "anthropic".to_string()]),
            ),
            ("case-b".to_string(), BTreeSet::from(["openai".to_string()])),
        ]);
        let error = validate_per_case_provider_diversity(&providers_by_case).unwrap_err();
        assert!(format!("{error:#}")
            .contains("agent review panel case case-b requires at least two agent providers"));
    }

    #[test]
    fn selection_source_mismatch_is_rejected() {
        let temp = tempdir().unwrap();
        let root = temp.path();
        let protocol_path = root.join("protocol.json");
        write_json(
            &protocol_path,
            json!({
                "schemaVersion": 1,
                "freezeId": "real-project-v1",
                "targetProfiles": [
                    {"language": "go", "candidateId": "gopls", "profile": "adapters/lsp/gopls.json"},
                    {"language": "python", "candidateId": "pyright", "profile": "adapters/lsp/pyright.json"}
                ],
                "population": {"snapshot": "population.json", "eligibility": "documented", "exclusions": "documented", "minimumStars": 100},
                "sampling": {"seedDerivation": "protocol commit", "repositoriesPerProfile": 4, "declarationsPerRepository": 3, "replacementRule": "documented"},
                "operations": ["references"],
                "claimScope": "the sampled repositories"
            }),
        );
        let selection_path = root.join("selection.json");
        write_json(
            &selection_path,
            json!({
                "schemaVersion": 1,
                "freezeId": "real-project-v1",
                "protocol": artifact_link("protocol.json", &protocol_path),
                "population": {"file": "population.json", "sha256": "0000000000000000000000000000000000000000000000000000000000000000"},
                "protocolCommit": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "profiles": [{
                    "language": "go",
                    "candidateId": "gopls",
                    "ranked": [],
                    "selected": [{
                        "fullName": "example/project",
                        "source": {"repo": "https://github.com/example/project.git", "commit": "dddddddddddddddddddddddddddddddddddddddd"},
                        "caseFile": "cases/example.yaml",
                        "caseIds": ["selected-call"],
                        "declarationDraw": {"selected": [{
                            "caseId": "selected-call",
                            "rank": 1,
                            "uri": "benchmark://source/source.txt",
                            "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 8}},
                            "kind": "variable",
                            "displayName": "archived"
                        }]}
                    }]
                }],
                "replacements": [],
                "documents": [{
                    "caseFile": "cases/example.yaml",
                    "language": "go",
                    "candidateId": "gopls",
                    "source": {"repo": "https://github.com/example/project.git", "commit": "dddddddddddddddddddddddddddddddddddddddd"},
                    "caseIds": ["selected-call"]
                }]
            }),
        );
        let reviewer_a = root.join("review-a.json");
        let reviewer_b = root.join("review-b.json");
        let adjudication = root.join("adjudication.json");
        fs::write(&reviewer_a, "a").unwrap();
        fs::write(&reviewer_b, "b").unwrap();
        fs::write(&adjudication, "adjudicated").unwrap();
        let review_path = root.join("review.json");
        write_json(
            &review_path,
            json!({
                "schemaVersion": 1,
                "freezeId": "real-project-v1",
                "selection": artifact_link("selection.json", &selection_path),
                "reviewers": [review_artifact("alice", "review-a.json", &reviewer_a), review_artifact("bob", "review-b.json", &reviewer_b)],
                "adjudication": review_artifact("adjudication", "adjudication.json", &adjudication)
            }),
        );
        let source_lock_path = root.join("sources.json");
        write_json(
            &source_lock_path,
            json!({
                "schemaVersion": 1,
                "freezeId": "real-project-v1",
                "selection": artifact_link("selection.json", &selection_path),
                "sources": [{
                    "repo": "https://github.com/example/project.git",
                    "commit": "dddddddddddddddddddddddddddddddddddddddd",
                    "commitObject": "00",
                    "tree": TREE,
                    "archiveTree": TREE,
                    "archive": "sources/example-project.tar",
                    "sha256": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
                }]
            }),
        );
        let case_file = root.join("cases/example.yaml");
        fs::create_dir_all(case_file.parent().unwrap()).unwrap();
        fs::write(&case_file, "placeholder").unwrap();

        let error = validate_document_evidence(
            &document("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            &case_file,
            root,
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("does not match the selection"));
    }

    fn document(commit: &str) -> BenchmarkDocument {
        let declaration = SymbolLocation {
            location: Location {
                uri: Url::parse("benchmark://source/source.txt").unwrap(),
                range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 0,
                        character: 8,
                    },
                },
            },
            kind: SymbolKind::Variable,
            display_name: "archived".to_string(),
            disambiguation: None,
        };
        let usage = SymbolLocation {
            location: Location {
                uri: Url::parse("benchmark://source/source.txt").unwrap(),
                range: Range {
                    start: Position {
                        line: 0,
                        character: 9,
                    },
                    end: Position {
                        line: 0,
                        character: 17,
                    },
                },
            },
            kind: SymbolKind::Variable,
            display_name: "archived".to_string(),
            disambiguation: None,
        };
        BenchmarkDocument {
            schema_version: 2,
            position_encoding: PositionEncoding::Utf16,
            source: Source::Git {
                repo: Url::parse("https://github.com/example/project.git").unwrap(),
                commit: commit.to_string(),
            },
            language: "go".to_string(),
            corpus: CorpusMetadata {
                partition: CorpusPartition::Evaluation,
                selection: CorpusSelection::PreRegistered,
                freeze_id: Some("real-project-v1".to_string()),
                selection_manifest: Some("selection.json".to_string()),
                review_manifest: Some("review.json".to_string()),
                source_lock: Some("sources.json".to_string()),
            },
            ground_truth: GroundTruthReview {
                status: GroundTruthReviewStatus::IndependentlyReviewed,
                reviewers: vec!["alice".to_string(), "bob".to_string()],
            },
            reference_policy: ReferencePolicy::BindingsOptional,
            semantic_packs: None,
            cases: vec![BenchmarkCase {
                id: "selected-call".to_string(),
                workspace_semantic_models: Vec::new(),
                declaration: Some(declaration.clone()),
                reference_probe: None,
                expected_usages: vec![usage.clone()],
                expected_unproven_usages: Vec::new(),
                allowed_extra_usages: Vec::new(),
                allowed_unproven_usages: Vec::new(),
                usage_lookups: vec![UsageLookup {
                    operation: NavigationOperation::Definition,
                    compatible_operations: Vec::new(),
                    expect_no_movement: false,
                    usage,
                    expected_declaration: declaration,
                    allowed_extra_targets: Vec::new(),
                }],
                type_lookups: Vec::new(),
                expected_failure: None,
                not_planned: None,
                unsupported: None,
                verification: None,
            }],
        }
    }

    fn write_evidence_artifacts(root: &Path, commit: &str, protocol_commit: &str) {
        let declaration = json!({
            "repository": "example/project",
            "commit": commit,
            "language": "go",
            "selectionRank": 1,
            "location": {
                "uri": "benchmark://source/source.txt",
                "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 8}}
            },
            "kind": "variable",
            "displayName": "archived"
        });
        let usage = json!({
            "location": {
                "uri": "benchmark://source/source.txt",
                "range": {"start": {"line": 0, "character": 9}, "end": {"line": 0, "character": 17}}
            },
            "kind": "variable",
            "displayName": "archived"
        });
        let record = json!({
            "caseId": "selected-call",
            "decision": "accepted",
            "declaration": declaration,
            "expectedUsages": [usage.clone()],
            "definitionUsage": usage
        });
        for (file, reviewer) in [("review-a.json", "alice"), ("review-b.json", "bob")] {
            write_json(
                &root.join(file),
                json!({
                    "schemaVersion": 1,
                    "reviewer": reviewer,
                    "referencePolicy": "bindings_optional",
                    "selectionAlgorithm": "real-project-v1-source-syntax-v1",
                    "records": [record.clone()]
                }),
            );
        }
        write_json(
            &root.join("adjudication.json"),
            json!({
                "schemaVersion": 1,
                "freezeId": "real-project-v1",
                "protocolCommit": protocol_commit,
                "records": [record]
            }),
        );
    }

    fn artifact_link(file: &str, path: &Path) -> serde_json::Value {
        json!({"file": file, "sha256": sha256(&fs::read(path).unwrap())})
    }

    fn review_artifact(id: &str, file: &str, path: &Path) -> serde_json::Value {
        json!({"id": id, "file": file, "sha256": sha256(&fs::read(path).unwrap())})
    }

    fn write_json(path: &Path, value: serde_json::Value) {
        fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    }

    fn git(path: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    }

    fn git_stdout(path: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(path)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?} failed");
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    }
}
