//! Hash-bound, stratified publication metadata.
//!
//! A publication is a small index over immutable freeze manifests.  It is not
//! a second scoring implementation: all case counts and outcomes are derived
//! from the raw reports after each manifest and report checksum has been
//! verified.  The index keeps prospective evaluation slices, the
//! retrospectively reviewed legacy slice, and development regression evidence
//! as separate typed inputs so that a future renderer cannot silently pool
//! them.

use crate::{
    evaluation::{safe_repo_relative_path, validate_report_against_release_audit},
    freeze::{FreezeManifest, ManifestCandidate, ManifestReport, SnapshotKind},
    promotion::{
        case_memberships, promotion_case_keys, promotion_document_languages,
        validate_report_against_promotion, validate_report_against_promotion_scope,
        PromotionMembership,
    },
    runners::{CaseRunReport, CaseStatus, RequiredDestinationStatus, RunReport},
    CorpusPartition, CorpusSelection,
};
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const SCHEMA: &str = include_str!("../schema/stratified-publication.schema.json");
pub const STRATIFIED_PUBLICATION_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactLink {
    pub file: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationEvidenceTier {
    ProspectiveEvaluation,
    RetrospectiveReviewedLegacy,
    DevelopmentRegression,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionProvenance {
    PreRegistered,
    RetrospectivelySelected,
    AnalyzerInformed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregateMethod {
    None,
    EqualSlice,
    EqualLanguage,
    Micro,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AggregatePolicy {
    pub method: AggregateMethod,
    pub included_slices: Vec<String>,
    pub weighting_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublicationSliceInput {
    pub id: String,
    pub label: String,
    pub evidence_tier: PublicationEvidenceTier,
    pub selection_provenance: SelectionProvenance,
    pub snapshot_kind: SnapshotKind,
    pub snapshot: ArtifactLink,
    pub include_in_headline: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StratifiedPublicationManifest {
    pub schema_version: u32,
    pub publication_id: String,
    pub slices: Vec<PublicationSliceInput>,
    pub aggregate: AggregatePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StratifiedPublication {
    pub schema_version: u32,
    pub publication_id: String,
    pub slices: Vec<PublicationSliceReport>,
    pub aggregate: PublicationAggregate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublicationSliceReport {
    pub id: String,
    pub label: String,
    pub evidence_tier: PublicationEvidenceTier,
    pub selection_provenance: SelectionProvenance,
    pub snapshot_kind: SnapshotKind,
    pub snapshot_version: String,
    pub snapshot_revision: String,
    pub snapshot: ArtifactLink,
    pub scope: PublicationScope,
    pub candidates: Vec<PublicationCandidateReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublicationScope {
    pub total_reported_cases: usize,
    pub headline_cases: usize,
    pub balanced_core_cases: usize,
    pub overflow_cases: usize,
    pub control_cases: usize,
    pub excluded_from_headline_cases: usize,
    pub denominator_policy: String,
    pub languages: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublicationCandidateReport {
    pub candidate_id: String,
    pub name: String,
    pub runner: String,
    pub requested_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    pub report: ArtifactLink,
    pub languages: BTreeMap<String, PublicationLanguageReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublicationLanguageReport {
    pub headline_denominator: usize,
    pub strict_scoreable_cases: usize,
    pub strict_exact_cases: usize,
    pub required_scoreable_cases: usize,
    pub required_found_cases: usize,
    pub required_missing_cases: usize,
    pub status_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublicationAggregate {
    pub method: AggregateMethod,
    pub included_slices: Vec<String>,
    pub weighting_name: String,
    pub pooled_scores: bool,
}

#[derive(Debug, Clone)]
struct VerifiedSnapshot {
    manifest: FreezeManifest,
    manifest_path: PathBuf,
    manifest_sha256: String,
    reports: BTreeMap<String, VerifiedReport>,
    legacy_memberships: BTreeMap<CaseKey, PromotionMembership>,
}

#[derive(Debug, Clone)]
struct VerifiedReport {
    candidate: ManifestCandidate,
    manifest_report: ManifestReport,
    report: RunReport,
    sha256: String,
    path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CaseKey {
    file: String,
    id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeadlineMembership {
    Headline,
    Overflow,
    Control,
    Development,
}

pub fn validate_manifest(path: impl AsRef<Path>) -> Result<StratifiedPublicationManifest> {
    let path = path.as_ref();
    let (manifest, root) = load_publication_manifest(path)?;
    for input in &manifest.slices {
        let snapshot_path = root.join(safe_repo_relative_path(
            &input.snapshot.file,
            "publication snapshot",
        )?);
        load_verified_snapshot(&snapshot_path, input)?;
    }
    Ok(manifest)
}

pub fn generate(path: impl AsRef<Path>) -> Result<StratifiedPublication> {
    let path = path.as_ref();
    let (manifest, root) = load_publication_manifest(path)?;
    let mut slices = Vec::with_capacity(manifest.slices.len());
    for input in &manifest.slices {
        let snapshot_path = root.join(safe_repo_relative_path(
            &input.snapshot.file,
            "publication snapshot",
        )?);
        let snapshot = load_verified_snapshot(&snapshot_path, input)?;
        slices.push(render_slice(input, snapshot, &root)?);
    }

    Ok(StratifiedPublication {
        schema_version: STRATIFIED_PUBLICATION_SCHEMA_VERSION,
        publication_id: manifest.publication_id,
        slices,
        aggregate: PublicationAggregate {
            method: manifest.aggregate.method,
            included_slices: manifest.aggregate.included_slices,
            weighting_name: manifest.aggregate.weighting_name,
            // This module deliberately does not pool scores. A renderer may
            // add a named aggregate later, but it must consume this explicit
            // policy and preserve slice boundaries.
            pooled_scores: false,
        },
    })
}

pub fn write(path: impl AsRef<Path>, output: impl AsRef<Path>) -> Result<StratifiedPublication> {
    let report = generate(path)?;
    let output = output.as_ref();
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    fs::write(
        output,
        format!("{}\n", serde_json::to_string_pretty(&report)?),
    )
    .with_context(|| format!("write publication report {}", output.display()))?;
    Ok(report)
}

fn load_publication_manifest(path: &Path) -> Result<(StratifiedPublicationManifest, PathBuf)> {
    let root = crate::find_repo_root_for_path(path)?;
    let bytes =
        fs::read(path).with_context(|| format!("read publication manifest {}", path.display()))?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).context("parse stratified publication manifest JSON")?;
    let schema: serde_json::Value =
        serde_json::from_str(SCHEMA).context("parse bundled publication schema")?;
    let compiled = jsonschema::JSONSchema::compile(&schema)
        .map_err(|error| anyhow!("compile publication schema: {error}"))?;
    if let Err(errors) = compiled.validate(&value) {
        bail!(
            "publication manifest schema validation failed: {}",
            errors
                .map(|error| error.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
    let manifest: StratifiedPublicationManifest =
        serde_json::from_value(value).context("deserialize stratified publication manifest")?;
    validate_manifest_shape(&manifest)?;
    Ok((manifest, root))
}

fn validate_manifest_shape(manifest: &StratifiedPublicationManifest) -> Result<()> {
    if manifest.schema_version != STRATIFIED_PUBLICATION_SCHEMA_VERSION {
        bail!(
            "unsupported stratified publication schema version {}",
            manifest.schema_version
        );
    }
    if manifest.slices.is_empty() {
        bail!("publication must contain at least one slice");
    }
    let ids = manifest
        .slices
        .iter()
        .map(|slice| slice.id.as_str())
        .collect::<BTreeSet<_>>();
    if ids.len() != manifest.slices.len() {
        bail!("publication slices must have unique IDs");
    }
    let mut prospective = BTreeSet::new();
    let mut snapshot_links = BTreeSet::new();
    for slice in &manifest.slices {
        if !snapshot_links.insert((&slice.snapshot.file, &slice.snapshot.sha256)) {
            bail!("publication slices must link distinct immutable snapshots");
        }
        match (
            slice.evidence_tier,
            slice.selection_provenance,
            slice.snapshot_kind,
        ) {
            (
                PublicationEvidenceTier::ProspectiveEvaluation,
                SelectionProvenance::PreRegistered,
                SnapshotKind::Evaluation,
            ) => {
                if !matches!(slice.id.as_str(), "real-project-v1" | "real-project-v2") {
                    bail!(
                        "prospective evaluation slice {} must identify real-project-v1 or real-project-v2",
                        slice.id
                    );
                }
                prospective.insert(slice.id.as_str());
                if !slice.include_in_headline {
                    bail!(
                        "prospective evaluation slice {} cannot be hidden from the headline",
                        slice.id
                    );
                }
            }
            (
                PublicationEvidenceTier::RetrospectiveReviewedLegacy,
                SelectionProvenance::RetrospectivelySelected,
                SnapshotKind::LegacyPromoted,
            ) => {
                if slice.id != "legacy-reviewed" {
                    bail!("retrospective reviewed legacy slice must use ID legacy-reviewed");
                }
                if !slice.include_in_headline {
                    bail!(
                        "retrospective reviewed legacy evidence cannot be hidden from the headline"
                    );
                }
            }
            (
                PublicationEvidenceTier::DevelopmentRegression,
                SelectionProvenance::AnalyzerInformed,
                SnapshotKind::Development,
            ) => {
                if slice.include_in_headline {
                    bail!(
                        "development regression slice {} cannot enter the headline",
                        slice.id
                    );
                }
            }
            _ => bail!(
                "slice {} has incompatible evidence tier, selection provenance, and snapshot kind",
                slice.id
            ),
        }
    }
    if prospective.len() > 2 {
        bail!("publication cannot contain more than v1 and v2 prospective slices");
    }
    if manifest.aggregate.method == AggregateMethod::None {
        if !manifest.aggregate.included_slices.is_empty() {
            bail!("aggregate method none cannot name included slices");
        }
    } else {
        if manifest.aggregate.included_slices.is_empty() {
            bail!("named aggregate must list its included slices");
        }
        if manifest.aggregate.weighting_name.trim().is_empty() {
            bail!("named aggregate must describe its weighting");
        }
        if manifest
            .aggregate
            .included_slices
            .iter()
            .any(|id| !ids.contains(id.as_str()))
        {
            bail!("aggregate includes an unknown slice");
        }
        if manifest.aggregate.included_slices.len()
            != manifest
                .aggregate
                .included_slices
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
        {
            bail!("aggregate included slices must be unique");
        }
        if manifest.aggregate.included_slices.iter().any(|id| {
            manifest
                .slices
                .iter()
                .find(|slice| &slice.id == id)
                .is_some_and(|slice| {
                    slice.evidence_tier == PublicationEvidenceTier::DevelopmentRegression
                })
        }) {
            bail!("development regression evidence cannot enter a publication aggregate");
        }
    }
    Ok(())
}

fn load_verified_snapshot(path: &Path, input: &PublicationSliceInput) -> Result<VerifiedSnapshot> {
    let root = crate::find_repo_root_for_path(path)?;
    let manifest_bytes = validate_artifact(&root, &input.snapshot, "publication snapshot")?;
    let actual_manifest_sha = input.snapshot.sha256.clone();
    let path = checked_path(&root, path, "publication snapshot")?;
    let manifest: FreezeManifest = serde_json::from_slice(&manifest_bytes)
        .with_context(|| format!("parse freeze manifest {}", path.display()))?;
    if manifest.snapshot_kind != input.snapshot_kind {
        bail!(
            "publication slice {} expects {} snapshot but linked manifest is {}",
            input.id,
            input.snapshot_kind,
            manifest.snapshot_kind
        );
    }
    validate_snapshot_provenance(&manifest, input)?;
    let candidates = manifest
        .candidates
        .iter()
        .map(|candidate| (candidate.id.clone(), candidate.clone()))
        .collect::<BTreeMap<_, _>>();
    if candidates.len() != manifest.candidates.len() {
        bail!(
            "freeze manifest {} contains duplicate candidate IDs",
            path.display()
        );
    }
    if manifest.reports.len() != candidates.len() {
        bail!(
            "freeze manifest {} does not contain one report per candidate",
            path.display()
        );
    }
    let mut reports = BTreeMap::new();
    let mut report_files = BTreeSet::new();
    for manifest_report in &manifest.reports {
        let candidate = candidates
            .get(&manifest_report.candidate_id)
            .with_context(|| {
                format!(
                    "freeze report references unknown candidate {}",
                    manifest_report.candidate_id
                )
            })?;
        let report_path = checked_path(
            &root,
            &path
                .parent()
                .context("freeze manifest has no parent")?
                .join(safe_repo_relative_path(
                    &manifest_report.file,
                    "frozen report",
                )?),
            "frozen report",
        )?;
        if !report_files.insert(report_path.clone()) {
            bail!(
                "freeze manifest reuses report file {}",
                manifest_report.file
            );
        }
        let report_bytes = fs::read(&report_path)
            .with_context(|| format!("read frozen report {}", report_path.display()))?;
        let report_sha = sha256(&report_bytes);
        if report_sha != manifest_report.sha256 {
            bail!(
                "checksum mismatch for frozen report {}: manifest {}, actual {}",
                manifest_report.file,
                manifest_report.sha256,
                report_sha
            );
        }
        let report: RunReport = serde_json::from_slice(&report_bytes)
            .with_context(|| format!("parse frozen report {}", report_path.display()))?;
        report.ensure_complete()?;
        if report.totals.errors != 0 {
            bail!(
                "frozen report {} contains execution errors",
                manifest_report.file
            );
        }
        if report.usagebench_revision != manifest.revision
            || report.usagebench_release.as_deref() != Some(manifest.version.as_str())
        {
            bail!(
                "frozen report {} provenance does not match its snapshot",
                manifest_report.file
            );
        }
        if report.runner != manifest_report.runner
            || report.environment != manifest_report.environment
            || report.totals != manifest_report.totals
            || report.case_files != manifest_report.case_files
        {
            bail!(
                "freeze metadata does not match frozen report {}",
                manifest_report.file
            );
        }
        if reports
            .insert(
                manifest_report.candidate_id.clone(),
                VerifiedReport {
                    candidate: candidate.clone(),
                    manifest_report: manifest_report.clone(),
                    report,
                    sha256: report_sha,
                    path: report_path,
                },
            )
            .is_some()
        {
            bail!("freeze manifest contains duplicate report candidate IDs");
        }
    }
    validate_scope(&manifest, &reports, &path)?;

    if let Some(audit) = &manifest.evaluation_audit {
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
        let rebuilt = crate::evaluation::build_release_audit(root.join(corpus_relative))
            .context("rebuild prospective evaluation audit")?;
        if rebuilt != *audit {
            bail!("evaluation audit does not match hash-verified release evidence");
        }
        for (candidate_id, verified) in &reports {
            validate_report_against_release_audit(&verified.report, audit, candidate_id)?;
        }
    }

    let legacy_memberships = if manifest.snapshot_kind == SnapshotKind::LegacyPromoted {
        let audit = manifest
            .legacy_promotion_audit
            .as_ref()
            .context("legacy-promoted snapshot is missing its promotion audit")?;
        let root = crate::find_repo_root_for_path(&path)?;
        let promotion_path = root.join(safe_repo_relative_path(
            &audit.manifest.file,
            "promotion manifest",
        )?);
        let rebuilt = crate::promotion::build_promotion_audit(&promotion_path)
            .context("verify legacy promotion audit")?;
        if rebuilt != *audit {
            bail!("legacy promotion audit does not match hash-verified evidence");
        }
        case_memberships(&promotion_path)?
            .into_iter()
            .map(|(key, membership)| {
                (
                    CaseKey {
                        file: key.0,
                        id: key.1,
                    },
                    membership,
                )
            })
            .collect()
    } else {
        BTreeMap::new()
    };

    Ok(VerifiedSnapshot {
        manifest,
        manifest_path: path.to_path_buf(),
        manifest_sha256: actual_manifest_sha,
        reports,
        legacy_memberships,
    })
}

fn validate_scope(
    manifest: &FreezeManifest,
    reports: &BTreeMap<String, VerifiedReport>,
    manifest_path: &Path,
) -> Result<()> {
    if manifest.snapshot_kind == SnapshotKind::LegacyPromoted {
        return validate_legacy_scope(manifest, reports, manifest_path);
    }
    let expected_files = manifest
        .corpus
        .iter()
        .map(|document| document.case_file.as_str())
        .collect::<BTreeSet<_>>();
    let expected_documents = manifest
        .corpus
        .iter()
        .map(|document| {
            (
                document.case_file.as_str(),
                (
                    document.language.as_str(),
                    document.partition,
                    document.selection,
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    if expected_documents.len() != manifest.corpus.len() {
        bail!("freeze manifest contains duplicate corpus documents");
    }
    if expected_files.is_empty() {
        bail!("freeze manifest contains no corpus documents");
    }
    let mut expected: Option<BTreeMap<CaseKey, (String, CorpusPartition, CorpusSelection)>> = None;
    for verified in reports.values() {
        let actual_files = verified
            .report
            .documents
            .iter()
            .map(|document| document.case_file.as_str())
            .collect::<BTreeSet<_>>();
        if actual_files != expected_files {
            bail!(
                "report {} documents do not match the freeze manifest corpus",
                verified.manifest_report.file
            );
        }
        let mut actual = BTreeMap::new();
        for document in &verified.report.documents {
            let expected_document = expected_documents
                .get(document.case_file.as_str())
                .with_context(|| {
                    format!(
                        "report {} contains an unknown corpus document {}",
                        verified.manifest_report.file, document.case_file
                    )
                })?;
            if expected_document
                != &(
                    document.language.as_str(),
                    document.corpus_partition,
                    document.corpus_selection,
                )
            {
                bail!(
                    "report {} corpus metadata drifted for {}",
                    verified.manifest_report.file,
                    document.case_file
                );
            }
            if document.corpus_partition != manifest_partition(manifest.snapshot_kind) {
                bail!(
                    "report {} contains a case from the wrong corpus partition",
                    verified.manifest_report.file
                );
            }
            for case in &document.cases {
                let key = CaseKey {
                    file: document.case_file.clone(),
                    id: case.id.clone(),
                };
                if actual
                    .insert(
                        key,
                        (
                            document.language.clone(),
                            document.corpus_partition,
                            document.corpus_selection,
                        ),
                    )
                    .is_some()
                {
                    bail!(
                        "report {} contains duplicate case IDs",
                        verified.manifest_report.file
                    );
                }
            }
        }
        if actual.is_empty() {
            bail!("report {} contains no cases", verified.manifest_report.file);
        }
        if let Some(previous) = &expected {
            if previous != &actual {
                bail!("snapshot reports do not cover an identical case scope (denominator drift)");
            }
        } else {
            expected = Some(actual);
        }
    }
    Ok(())
}

fn validate_legacy_scope(
    manifest: &FreezeManifest,
    reports: &BTreeMap<String, VerifiedReport>,
    manifest_path: &Path,
) -> Result<()> {
    let audit = manifest
        .legacy_promotion_audit
        .as_ref()
        .context("legacy-promoted snapshot is missing its promotion audit")?;
    let root = crate::find_repo_root_for_path(manifest_path)?;
    let promotion_path = root.join(safe_repo_relative_path(
        &audit.manifest.file,
        "promotion manifest",
    )?);
    let document_languages = promotion_document_languages(&promotion_path)?;
    let expected_documents = manifest
        .corpus
        .iter()
        .map(|document| {
            (
                document.case_file.as_str(),
                (
                    document.language.as_str(),
                    document.partition,
                    document.selection,
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    if expected_documents.len() != manifest.corpus.len() {
        bail!("freeze manifest contains duplicate corpus documents");
    }
    let mut union = BTreeSet::new();
    for verified in reports.values() {
        let allowed = verified
            .candidate
            .languages
            .as_ref()
            .map(|languages| languages.iter().cloned().collect::<BTreeSet<_>>());
        let keys = if let Some(allowed) = allowed.as_ref() {
            validate_report_against_promotion_scope(
                &verified.report,
                audit,
                &document_languages,
                Some(allowed),
            )?
        } else {
            // Historical manifests predate serialized candidate language
            // scopes. Preserve their original full-corpus contract while new
            // legacy manifests use the strict scoped path above.
            validate_report_against_promotion(&verified.report, audit)?;
            verified
                .report
                .documents
                .iter()
                .flat_map(|document| {
                    document
                        .cases
                        .iter()
                        .map(|case| (document.case_file.clone(), case.id.clone()))
                })
                .collect()
        };
        for document in &verified.report.documents {
            let expected = expected_documents
                .get(document.case_file.as_str())
                .with_context(|| {
                    format!(
                        "report {} contains unknown corpus document {}",
                        verified.manifest_report.file, document.case_file
                    )
                })?;
            if expected
                != &(
                    document.language.as_str(),
                    document.corpus_partition,
                    document.corpus_selection,
                )
            {
                bail!(
                    "report {} corpus metadata drifted for {}",
                    verified.manifest_report.file,
                    document.case_file
                );
            }
        }
        union.extend(keys);
    }
    if expected_documents.len() != audit.case_ids_by_file.len()
        || expected_documents
            .keys()
            .any(|file| !audit.case_ids_by_file.contains_key(*file))
    {
        bail!("legacy freeze corpus documents drift from the promotion manifest");
    }
    if union != promotion_case_keys(audit) {
        bail!("legacy reports do not union to exactly the promotion manifest cases");
    }
    Ok(())
}

fn validate_snapshot_provenance(
    manifest: &FreezeManifest,
    input: &PublicationSliceInput,
) -> Result<()> {
    match input.evidence_tier {
        PublicationEvidenceTier::ProspectiveEvaluation => {
            if manifest.evaluation_audit.is_none() || manifest.legacy_promotion_audit.is_some() {
                bail!("prospective publication slice requires evaluation-only audit provenance");
            }
            if manifest.corpus.iter().any(|document| {
                document.partition != CorpusPartition::Evaluation
                    || document.selection != CorpusSelection::PreRegistered
            }) {
                bail!("prospective publication slice contains a non-preregistered document");
            }
        }
        PublicationEvidenceTier::RetrospectiveReviewedLegacy => {
            if manifest.legacy_promotion_audit.is_none() || manifest.evaluation_audit.is_some() {
                bail!("retrospective publication slice requires legacy-only audit provenance");
            }
            if manifest.corpus.iter().any(|document| {
                document.partition != CorpusPartition::Development
                    || document.selection != CorpusSelection::AnalyzerInformed
            }) {
                bail!("legacy publication slice contains a non-development analyzer-informed document");
            }
        }
        PublicationEvidenceTier::DevelopmentRegression => {
            if manifest.evaluation_audit.is_some() || manifest.legacy_promotion_audit.is_some() {
                bail!("development regression slice cannot carry evaluation or promotion audit provenance");
            }
            if manifest.corpus.iter().any(|document| {
                document.partition != CorpusPartition::Development
                    || document.selection != CorpusSelection::AnalyzerInformed
            }) {
                bail!("development regression slice contains a non-development analyzer-informed document");
            }
        }
    }
    Ok(())
}

fn manifest_partition(kind: SnapshotKind) -> CorpusPartition {
    match kind {
        SnapshotKind::Development | SnapshotKind::LegacyPromoted => CorpusPartition::Development,
        SnapshotKind::Evaluation => CorpusPartition::Evaluation,
    }
}

fn render_slice(
    input: &PublicationSliceInput,
    snapshot: VerifiedSnapshot,
    root: &Path,
) -> Result<PublicationSliceReport> {
    let mut all_cases = BTreeMap::<CaseKey, (String, &CaseRunReport)>::new();
    for verified in snapshot.reports.values() {
        for document in &verified.report.documents {
            for case in &document.cases {
                all_cases.insert(
                    CaseKey {
                        file: document.case_file.clone(),
                        id: case.id.clone(),
                    },
                    (document.language.clone(), case),
                );
            }
        }
    }
    let mut scope = ScopeCounts::default();
    let mut languages = BTreeSet::new();
    let mut membership_by_case = BTreeMap::new();
    for (key, (language, _)) in &all_cases {
        languages.insert(language.clone());
        scope.total_reported_cases += 1;
        let membership = case_membership(&snapshot, input, key)?;
        membership_by_case.insert(key.clone(), membership);
        match membership {
            HeadlineMembership::Headline => {
                scope.headline_cases += 1;
                scope.balanced_core_cases += usize::from(
                    input.evidence_tier == PublicationEvidenceTier::RetrospectiveReviewedLegacy,
                );
            }
            HeadlineMembership::Overflow => scope.overflow_cases += 1,
            HeadlineMembership::Control => scope.control_cases += 1,
            HeadlineMembership::Development => scope.excluded_from_headline_cases += 1,
        }
    }
    if input.evidence_tier == PublicationEvidenceTier::RetrospectiveReviewedLegacy
        && scope.balanced_core_cases == 0
    {
        bail!("legacy publication slice contains no balanced-core cases");
    }
    if input.evidence_tier != PublicationEvidenceTier::RetrospectiveReviewedLegacy {
        scope.balanced_core_cases = scope.headline_cases;
    }
    scope.excluded_from_headline_cases = scope
        .overflow_cases
        .checked_add(scope.control_cases)
        .and_then(|value| value.checked_add(scope.excluded_from_headline_cases))
        .context("publication scope count overflow")?;

    let mut candidates = Vec::new();
    for verified in snapshot.reports.values() {
        let mut per_language = BTreeMap::<String, Vec<&CaseRunReport>>::new();
        for document in &verified.report.documents {
            for case in &document.cases {
                let key = CaseKey {
                    file: document.case_file.clone(),
                    id: case.id.clone(),
                };
                if membership_by_case
                    .get(&key)
                    .is_some_and(|membership| *membership == HeadlineMembership::Headline)
                {
                    per_language
                        .entry(document.language.clone())
                        .or_default()
                        .push(case);
                }
            }
        }
        let languages_report = per_language
            .into_iter()
            .map(|(language, cases)| (language, summarize_language(&cases)))
            .collect();
        candidates.push(PublicationCandidateReport {
            candidate_id: verified.candidate.id.clone(),
            name: verified.candidate.name.clone(),
            runner: verified.candidate.runner.clone(),
            requested_version: verified.candidate.requested_version.clone(),
            profile: verified.candidate.profile.clone(),
            report: ArtifactLink {
                file: repo_relative(root, &verified.path)?,
                sha256: verified.sha256.clone(),
            },
            languages: languages_report,
        });
    }
    candidates.sort_by(|left, right| left.candidate_id.cmp(&right.candidate_id));
    let _ = snapshot.manifest_path;
    let _ = snapshot.manifest_sha256;
    Ok(PublicationSliceReport {
        id: input.id.clone(),
        label: input.label.clone(),
        evidence_tier: input.evidence_tier,
        selection_provenance: input.selection_provenance,
        snapshot_kind: snapshot.manifest.snapshot_kind,
        snapshot_version: snapshot.manifest.version,
        snapshot_revision: snapshot.manifest.revision,
        snapshot: input.snapshot.clone(),
        scope: PublicationScope {
            total_reported_cases: scope.total_reported_cases,
            headline_cases: scope.headline_cases,
            balanced_core_cases: scope.balanced_core_cases,
            overflow_cases: scope.overflow_cases,
            control_cases: scope.control_cases,
            excluded_from_headline_cases: scope.excluded_from_headline_cases,
            denominator_policy: if input.evidence_tier
                == PublicationEvidenceTier::RetrospectiveReviewedLegacy
            {
                "balanced_core_only_equal_language"
            } else if input.evidence_tier == PublicationEvidenceTier::DevelopmentRegression {
                "development_regression_not_headline"
            } else {
                "all_frozen_evaluation_cases_per_language"
            }
            .to_string(),
            languages: languages.into_iter().collect(),
        },
        candidates,
    })
}

fn case_membership(
    snapshot: &VerifiedSnapshot,
    input: &PublicationSliceInput,
    key: &CaseKey,
) -> Result<HeadlineMembership> {
    match input.evidence_tier {
        PublicationEvidenceTier::ProspectiveEvaluation => Ok(HeadlineMembership::Headline),
        PublicationEvidenceTier::DevelopmentRegression => Ok(HeadlineMembership::Development),
        PublicationEvidenceTier::RetrospectiveReviewedLegacy => {
            match snapshot.legacy_memberships.get(key) {
                Some(PromotionMembership::BalancedCore) => Ok(HeadlineMembership::Headline),
                Some(PromotionMembership::Overflow) => Ok(HeadlineMembership::Overflow),
                Some(PromotionMembership::Control) => Ok(HeadlineMembership::Control),
                None => bail!(
                    "legacy report case {} / {} is absent from the promotion manifest",
                    key.file,
                    key.id
                ),
            }
        }
    }
}

#[derive(Default)]
struct ScopeCounts {
    total_reported_cases: usize,
    headline_cases: usize,
    balanced_core_cases: usize,
    overflow_cases: usize,
    control_cases: usize,
    excluded_from_headline_cases: usize,
}

fn summarize_language(cases: &[&CaseRunReport]) -> PublicationLanguageReport {
    let mut status_counts = BTreeMap::new();
    let mut strict_scoreable_cases = 0;
    let mut strict_exact_cases = 0;
    let mut required_scoreable_cases = 0;
    let mut required_found_cases = 0;
    let mut required_missing_cases = 0;
    for case in cases {
        *status_counts
            .entry(status_name(case.status).to_string())
            .or_insert(0) += 1;
        if strict_scoreable(case.status) {
            strict_scoreable_cases += 1;
        }
        if matches!(case.status, CaseStatus::Passed | CaseStatus::Improved) {
            strict_exact_cases += 1;
        }
        match case.required_destination_status {
            Some(RequiredDestinationStatus::Found) => {
                required_scoreable_cases += 1;
                required_found_cases += 1;
            }
            Some(RequiredDestinationStatus::Missing) => {
                required_scoreable_cases += 1;
                required_missing_cases += 1;
            }
            _ => {}
        }
    }
    PublicationLanguageReport {
        headline_denominator: cases.len(),
        strict_scoreable_cases,
        strict_exact_cases,
        required_scoreable_cases,
        required_found_cases,
        required_missing_cases,
        status_counts,
    }
}

fn strict_scoreable(status: CaseStatus) -> bool {
    !matches!(
        status,
        CaseStatus::NotPlanned | CaseStatus::Unsupported | CaseStatus::Skipped | CaseStatus::Error
    )
}

fn status_name(status: CaseStatus) -> &'static str {
    match status {
        CaseStatus::Passed => "passed",
        CaseStatus::NearMiss => "near_miss",
        CaseStatus::PositionUnverified => "position_unverified",
        CaseStatus::Improved => "improved",
        CaseStatus::Failed => "failed",
        CaseStatus::ExpectedFailure => "expected_failure",
        CaseStatus::NotPlanned => "not_planned",
        CaseStatus::Unsupported => "unsupported",
        CaseStatus::Skipped => "skipped",
        CaseStatus::Error => "error",
    }
}

fn validate_artifact(root: &Path, link: &ArtifactLink, label: &str) -> Result<Vec<u8>> {
    let path = checked_path(
        root,
        &root.join(safe_repo_relative_path(&link.file, label)?),
        label,
    )?;
    let bytes = fs::read(&path).with_context(|| format!("read {label} {}", link.file))?;
    let actual = sha256(&bytes);
    if actual != link.sha256 {
        bail!(
            "{label} hash mismatch for {}: manifest {}, actual {}",
            link.file,
            link.sha256,
            actual
        );
    }
    Ok(bytes)
}

fn checked_path(root: &Path, path: &Path, label: &str) -> Result<PathBuf> {
    let canonical_root = root
        .canonicalize()
        .with_context(|| format!("canonicalize publication root {}", root.display()))?;
    let canonical_path = path
        .canonicalize()
        .with_context(|| format!("canonicalize {label} {}", path.display()))?;
    if !canonical_path.starts_with(&canonical_root) {
        bail!(
            "{label} resolves outside the repository: {}",
            path.display()
        );
    }
    Ok(canonical_path)
}

fn repo_relative(root: &Path, path: &Path) -> Result<String> {
    Ok(path
        .canonicalize()
        .with_context(|| format!("canonicalize artifact {}", path.display()))?
        .strip_prefix(
            root.canonicalize()
                .context("canonicalize publication root")?,
        )
        .with_context(|| format!("artifact {} is outside repository", path.display()))?
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/"))
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_mixed_evidence_contracts() {
        let manifest = StratifiedPublicationManifest {
            schema_version: 1,
            publication_id: "test".into(),
            slices: vec![PublicationSliceInput {
                id: "legacy-reviewed".into(),
                label: "legacy".into(),
                evidence_tier: PublicationEvidenceTier::RetrospectiveReviewedLegacy,
                selection_provenance: SelectionProvenance::PreRegistered,
                snapshot_kind: SnapshotKind::LegacyPromoted,
                snapshot: ArtifactLink {
                    file: "snapshot.json".into(),
                    sha256: "0".repeat(64),
                },
                include_in_headline: true,
            }],
            aggregate: AggregatePolicy {
                method: AggregateMethod::None,
                included_slices: Vec::new(),
                weighting_name: "none".into(),
            },
        };
        let error = validate_manifest_shape(&manifest).unwrap_err();
        assert!(error.to_string().contains("incompatible evidence"));
    }

    #[test]
    fn rejects_development_in_named_aggregate() {
        let manifest = StratifiedPublicationManifest {
            schema_version: 1,
            publication_id: "test".into(),
            slices: vec![PublicationSliceInput {
                id: "development".into(),
                label: "development".into(),
                evidence_tier: PublicationEvidenceTier::DevelopmentRegression,
                selection_provenance: SelectionProvenance::AnalyzerInformed,
                snapshot_kind: SnapshotKind::Development,
                snapshot: ArtifactLink {
                    file: "snapshot.json".into(),
                    sha256: "0".repeat(64),
                },
                include_in_headline: false,
            }],
            aggregate: AggregatePolicy {
                method: AggregateMethod::EqualSlice,
                included_slices: vec!["development".into()],
                weighting_name: "equal slices".into(),
            },
        };
        let error = validate_manifest_shape(&manifest).unwrap_err();
        assert!(error.to_string().contains("development regression"));
    }

    #[test]
    fn summarizes_only_explicitly_scoreable_required_destinations() {
        let cases = [
            CaseRunReport {
                id: "passed".into(),
                status: CaseStatus::Passed,
                required_destination_status: Some(RequiredDestinationStatus::Found),
                location_metrics: None,
                expected_failure_reason: None,
                not_planned_reason: None,
                unsupported_reason: None,
                declaration_to_usages: None,
                usage_to_declaration: Vec::new(),
                compatible_usage_to_declaration: Vec::new(),
                type_lookups: Vec::new(),
                diagnostics: Vec::new(),
            },
            CaseRunReport {
                id: "unsupported".into(),
                status: CaseStatus::Unsupported,
                required_destination_status: Some(RequiredDestinationStatus::Unsupported),
                location_metrics: None,
                expected_failure_reason: None,
                not_planned_reason: None,
                unsupported_reason: Some("unsupported".into()),
                declaration_to_usages: None,
                usage_to_declaration: Vec::new(),
                compatible_usage_to_declaration: Vec::new(),
                type_lookups: Vec::new(),
                diagnostics: Vec::new(),
            },
        ];
        let result = summarize_language(&cases.iter().collect::<Vec<_>>());
        assert_eq!(result.headline_denominator, 2);
        assert_eq!(result.strict_scoreable_cases, 1);
        assert_eq!(result.required_scoreable_cases, 1);
        assert_eq!(result.required_found_cases, 1);
    }
}
