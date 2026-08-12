//! Publication boundary for retrospective review of the legacy development corpus.
//!
//! Promotion is an immutable overlay. It never rewrites the analyzer-informed
//! source documents and is deliberately not accepted by the prospective
//! evaluation validator.

use crate::{
    evaluation::safe_repo_relative_path, runners::RunReport, BenchmarkDocument, CorpusPartition,
    CorpusSelection,
};
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const SCHEMA: &str = include_str!("../schema/legacy-promotion.schema.json");
const LEGACY_LANGUAGE_COUNT: usize = 11;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactLink {
    pub file: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromotionMembership {
    BalancedCore,
    Overflow,
    Control,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlStatus {
    Unsupported,
    NotPlanned,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PromotionCase {
    pub id: String,
    pub membership: PromotionMembership,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_status: Option<ControlStatus>,
    pub strata: BTreeMap<String, String>,
    pub review_records: Vec<ArtifactLink>,
    pub adjudication: ArtifactLink,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PromotionDocument {
    pub case_file: String,
    pub source_sha256: String,
    pub language: String,
    pub cases: Vec<PromotionCase>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BalancePolicy {
    pub language_count: usize,
    pub maximum_per_language: usize,
    pub eligible_counts: BTreeMap<String, usize>,
    pub balanced_core_per_language: usize,
    pub required_strata: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LegacyPromotionManifest {
    pub schema_version: u32,
    pub promotion_id: String,
    pub supersedes: Option<ArtifactLink>,
    pub claim_scope: String,
    pub selection_provenance: String,
    pub review_tier: String,
    pub selection_basis: String,
    pub analyzer_outcome_use: String,
    pub eligibility_policy: ArtifactLink,
    pub balance_policy: BalancePolicy,
    pub documents: Vec<PromotionDocument>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyPromotionAudit {
    pub promotion_id: String,
    pub claim_scope: String,
    pub manifest: ArtifactLink,
    pub eligibility_policy: ArtifactLink,
    pub balanced_core_per_language: usize,
    pub balanced_core_case_count: usize,
    pub overflow_case_count: usize,
    pub control_case_count: usize,
    pub denominators: BTreeMap<String, usize>,
    pub case_ids_by_file: BTreeMap<String, Vec<String>>,
}

pub fn build_promotion_audit(path: impl AsRef<Path>) -> Result<LegacyPromotionAudit> {
    let path = path.as_ref();
    let bytes =
        fs::read(path).with_context(|| format!("read promotion manifest {}", path.display()))?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).context("parse promotion manifest JSON")?;
    let schema: serde_json::Value =
        serde_json::from_str(SCHEMA).context("parse bundled promotion schema")?;
    let compiled = jsonschema::JSONSchema::compile(&schema)
        .map_err(|e| anyhow!("compile promotion schema: {e}"))?;
    if let Err(errors) = compiled.validate(&value) {
        bail!(
            "promotion manifest schema validation failed: {}",
            errors.map(|e| e.to_string()).collect::<Vec<_>>().join("; ")
        );
    }
    let manifest: LegacyPromotionManifest =
        serde_json::from_value(value).context("deserialize promotion manifest")?;
    validate_manifest(path, &bytes, &manifest)
}

fn validate_manifest(
    path: &Path,
    bytes: &[u8],
    manifest: &LegacyPromotionManifest,
) -> Result<LegacyPromotionAudit> {
    if manifest.schema_version != 1 {
        bail!("unsupported legacy promotion schema version");
    }
    if manifest.selection_provenance != "retrospectively_selected"
        || manifest.review_tier != "legacy_promoted"
    {
        bail!("legacy promotion cannot claim pre_registered selection or another review tier");
    }
    if manifest.selection_basis != "source_only" || manifest.analyzer_outcome_use != "forbidden" {
        bail!("legacy promotion selection, rejection, replacement, and weighting must exclude analyzer outcomes");
    }
    if !manifest.claim_scope.to_ascii_lowercase().contains("corpus") {
        bail!("legacy promotion claimScope must be explicitly corpus-bounded");
    }
    let policy = &manifest.balance_policy;
    if policy.language_count != LEGACY_LANGUAGE_COUNT
        || policy.eligible_counts.len() != LEGACY_LANGUAGE_COUNT
    {
        bail!("legacy promotion must bind exactly 11 legacy languages");
    }
    if policy.maximum_per_language != 10 {
        bail!("balanced core maximumPerLanguage must be 10");
    }
    let lowest = policy
        .eligible_counts
        .values()
        .copied()
        .min()
        .context("eligibleCounts is empty")?;
    let expected_n = 10.min(lowest);
    if policy.balanced_core_per_language != expected_n {
        bail!(
            "balanced-core denominator drift: expected N={expected_n}, got {}",
            policy.balanced_core_per_language
        );
    }
    let root = crate::find_repo_root_for_path(path)?;
    validate_link(&root, &manifest.eligibility_policy, "eligibility policy")?;
    if let Some(previous) = &manifest.supersedes {
        validate_link(&root, previous, "superseded manifest")?;
    }
    let required = policy
        .required_strata
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut denominators = policy
        .eligible_counts
        .keys()
        .map(|l| (l.clone(), 0))
        .collect::<BTreeMap<_, _>>();
    let mut ids = BTreeSet::new();
    let mut by_file = BTreeMap::new();
    let (mut overflow, mut controls) = (0, 0);
    for document in &manifest.documents {
        let source_path = safe_join(&root, &document.case_file)?;
        let source_bytes = fs::read(&source_path)
            .with_context(|| format!("read historical source document {}", document.case_file))?;
        if sha256(&source_bytes) != document.source_sha256 {
            bail!(
                "historical source document hash changed: {}",
                document.case_file
            );
        }
        let source: BenchmarkDocument =
            serde_yaml::from_slice(&source_bytes).context("parse historical benchmark document")?;
        if source.corpus.partition != CorpusPartition::Development
            || source.corpus.selection != CorpusSelection::AnalyzerInformed
        {
            bail!("legacy promotion may only reference development/analyzer_informed documents");
        }
        if source.language != document.language || !denominators.contains_key(&document.language) {
            bail!("promotion document language is not in the frozen 11-language policy");
        }
        let source_cases = source
            .cases
            .iter()
            .map(|c| (c.id.as_str(), c))
            .collect::<BTreeMap<_, _>>();
        let mut file_ids = Vec::new();
        for case in &document.cases {
            let Some(source_case) = source_cases.get(case.id.as_str()) else {
                bail!("promotion case ID is missing: {}", case.id);
            };
            if !ids.insert(case.id.clone()) {
                bail!("promotion case ID is missing or duplicated: {}", case.id);
            }
            if !required.iter().all(|key| case.strata.contains_key(*key)) {
                bail!(
                    "promotion case {} lacks a required balance stratum",
                    case.id
                );
            }
            match case.membership {
                PromotionMembership::BalancedCore => {
                    if case.control_status.is_some()
                        || source_case.unsupported.is_some()
                        || source_case.not_planned.is_some()
                    {
                        bail!("balanced-core case cannot be unsupported/not-planned control");
                    }
                    *denominators.get_mut(&document.language).unwrap() += 1;
                }
                PromotionMembership::Overflow => {
                    if case.control_status.is_some()
                        || source_case.unsupported.is_some()
                        || source_case.not_planned.is_some()
                    {
                        bail!("overflow case cannot be a control");
                    }
                    overflow += 1;
                }
                PromotionMembership::Control => {
                    if case.control_status.is_none() {
                        bail!("control case requires unsupported or not_planned status");
                    }
                    let matches_source = matches!(
                        (
                            &case.control_status,
                            source_case.unsupported.is_some(),
                            source_case.not_planned.is_some()
                        ),
                        (Some(ControlStatus::Unsupported), true, false)
                            | (Some(ControlStatus::NotPlanned), false, true)
                    );
                    if !matches_source {
                        bail!(
                            "control status for {} does not match the historical case",
                            case.id
                        );
                    }
                    controls += 1;
                }
            }
            if case.review_records.len() < 2 {
                bail!(
                    "promotion case {} requires at least two independent review records",
                    case.id
                );
            }
            let mut review_hashes = BTreeSet::new();
            for link in &case.review_records {
                validate_link(&root, link, "raw review evidence")?;
                review_hashes.insert((&link.file, &link.sha256));
            }
            if review_hashes.len() != case.review_records.len() {
                bail!("promotion case {} reuses review evidence", case.id);
            }
            validate_link(&root, &case.adjudication, "adjudication evidence")?;
            file_ids.push(case.id.clone());
        }
        if by_file
            .insert(document.case_file.clone(), file_ids)
            .is_some()
        {
            bail!(
                "promotion manifest contains duplicate document {}",
                document.case_file
            );
        }
    }
    if denominators.values().any(|count| *count != expected_n) {
        bail!("balanced core must contain exactly N={expected_n} cases for every legacy language");
    }
    Ok(LegacyPromotionAudit {
        promotion_id: manifest.promotion_id.clone(),
        claim_scope: manifest.claim_scope.clone(),
        manifest: ArtifactLink {
            file: repo_relative(&root, path)?,
            sha256: sha256(bytes),
        },
        eligibility_policy: manifest.eligibility_policy.clone(),
        balanced_core_per_language: expected_n,
        balanced_core_case_count: expected_n * LEGACY_LANGUAGE_COUNT,
        overflow_case_count: overflow,
        control_case_count: controls,
        denominators,
        case_ids_by_file: by_file,
    })
}

pub fn validate_report_against_promotion(
    report: &RunReport,
    audit: &LegacyPromotionAudit,
) -> Result<()> {
    let actual = report
        .documents
        .iter()
        .map(|d| {
            (
                d.case_file.as_str(),
                d.cases
                    .iter()
                    .map(|c| c.id.as_str())
                    .collect::<BTreeSet<_>>(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let actual_files = actual.keys().copied().collect::<BTreeSet<_>>();
    let expected_files = audit
        .case_ids_by_file
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if actual_files != expected_files {
        bail!("report documents drift from promotion manifest");
    }
    for (file, ids) in &audit.case_ids_by_file {
        let found = actual
            .get(file.as_str())
            .with_context(|| format!("report omits promoted document {file}"))?;
        let expected = ids.iter().map(String::as_str).collect::<BTreeSet<_>>();
        if *found != expected {
            bail!("report case IDs drift from promotion manifest for {file}");
        }
    }
    Ok(())
}

fn validate_link(root: &Path, link: &ArtifactLink, label: &str) -> Result<()> {
    let path = safe_join(root, &link.file)?;
    let bytes = fs::read(&path).with_context(|| format!("read {label} {}", link.file))?;
    if sha256(&bytes) != link.sha256 {
        bail!("{label} hash mismatch for {}", link.file);
    }
    Ok(())
}
fn safe_join(root: &Path, value: &str) -> Result<PathBuf> {
    let path = root.join(safe_repo_relative_path(value, "promotion artifact")?);
    let canonical_root = root
        .canonicalize()
        .context("canonicalize repository root")?;
    let canonical_path = path
        .canonicalize()
        .with_context(|| format!("canonicalize promotion artifact {value}"))?;
    if !canonical_path.starts_with(&canonical_root) {
        bail!("promotion artifact resolves outside the repository: {value}");
    }
    Ok(canonical_path)
}
fn repo_relative(root: &Path, path: &Path) -> Result<String> {
    Ok(path
        .canonicalize()?
        .strip_prefix(root.canonicalize()?)?
        .to_string_lossy()
        .replace('\\', "/"))
}
fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> LegacyPromotionManifest {
        let eligible_counts = (0..11)
            .map(|i| {
                (
                    if i == 0 {
                        "go".to_string()
                    } else {
                        format!("language-{i}")
                    },
                    10,
                )
            })
            .collect();
        LegacyPromotionManifest {
            schema_version: 1,
            promotion_id: "legacy-v1".into(),
            supersedes: None,
            claim_scope: "corpus-bounded reviewed conformance".into(),
            selection_provenance: "retrospectively_selected".into(),
            review_tier: "legacy_promoted".into(),
            selection_basis: "source_only".into(),
            analyzer_outcome_use: "forbidden".into(),
            eligibility_policy: ArtifactLink {
                file: "Cargo.toml".into(),
                sha256: "f219fb9a04d4ee3b21dd12ef166ea52ac766b1e03a1690109fb0e5ac46588d28".into(),
            },
            balance_policy: BalancePolicy {
                language_count: 11,
                maximum_per_language: 10,
                eligible_counts,
                balanced_core_per_language: 10,
                required_strata: vec![
                    "operation",
                    "symbol_kind",
                    "semantic_feature",
                    "source_complexity",
                    "operation_status",
                    "language",
                ]
                .into_iter()
                .map(str::to_string)
                .collect(),
            },
            documents: vec![],
        }
    }

    #[test]
    fn rejects_false_preregistration_claim() {
        let mut value = manifest();
        value.selection_provenance = "pre_registered".into();
        let error = validate_manifest(Path::new("Cargo.toml"), b"{}", &value).unwrap_err();
        assert!(error.to_string().contains("cannot claim pre_registered"));
    }

    #[test]
    fn rejects_balanced_core_denominator_drift() {
        let mut value = manifest();
        value.balance_policy.balanced_core_per_language = 9;
        let error = validate_manifest(Path::new("Cargo.toml"), b"{}", &value).unwrap_err();
        assert!(error.to_string().contains("denominator drift"));
    }

    #[test]
    fn rejects_mutated_historical_source_evidence() {
        let mut value = manifest();
        value.documents.push(PromotionDocument {
            case_file: "benchmarks/cases/go-baseline.yaml".into(),
            source_sha256: "0".repeat(64),
            language: "go".into(),
            cases: vec![],
        });
        let error = validate_manifest(Path::new("Cargo.toml"), b"{}", &value).unwrap_err();
        assert!(error
            .to_string()
            .contains("historical source document hash changed"));
    }

    #[test]
    fn old_benchmark_selection_model_remains_compatible() {
        let source = fs::read("benchmarks/cases/go-baseline.yaml").unwrap();
        let document: BenchmarkDocument = serde_yaml::from_slice(&source).unwrap();
        assert_eq!(document.corpus.selection, CorpusSelection::AnalyzerInformed);
    }
}
