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

/// Return the immutable membership assigned to each case by a validated
/// retrospective promotion manifest. The manifest remains analyzer-neutral:
/// agentic re-review can strengthen its source contract, but never changes
/// the original analyzer-informed selection into preregistered evidence.
pub fn case_memberships(
    path: impl AsRef<Path>,
) -> Result<BTreeMap<(String, String), PromotionMembership>> {
    let path = path.as_ref();
    let bytes =
        fs::read(path).with_context(|| format!("read promotion manifest {}", path.display()))?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).context("parse promotion manifest JSON")?;
    let schema: serde_json::Value =
        serde_json::from_str(SCHEMA).context("parse bundled promotion schema")?;
    let compiled = jsonschema::JSONSchema::compile(&schema)
        .map_err(|error| anyhow!("compile promotion schema: {error}"))?;
    if let Err(errors) = compiled.validate(&value) {
        bail!(
            "promotion manifest schema validation failed: {}",
            errors
                .map(|error| error.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
    let manifest: LegacyPromotionManifest =
        serde_json::from_value(value).context("deserialize promotion manifest")?;
    validate_manifest(path, &bytes, &manifest)?;
    let mut memberships = BTreeMap::new();
    for document in manifest.documents {
        for case in document.cases {
            if memberships
                .insert(
                    (document.case_file.clone(), case.id.clone()),
                    case.membership,
                )
                .is_some()
            {
                bail!("promotion manifest contains duplicate case membership");
            }
        }
    }
    Ok(memberships)
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

/// Load the canonical language assigned to each promoted case document.
///
/// The language is intentionally read from the hash-bound promotion manifest
/// rather than inferred from a report.  Reports are allowed to be stratified
/// by registered analyzer profile, but cannot relabel a source document.
pub fn promotion_document_languages(path: impl AsRef<Path>) -> Result<BTreeMap<String, String>> {
    let path = path.as_ref();
    let bytes =
        fs::read(path).with_context(|| format!("read promotion manifest {}", path.display()))?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).context("parse promotion manifest JSON")?;
    let schema: serde_json::Value =
        serde_json::from_str(SCHEMA).context("parse bundled promotion schema")?;
    let compiled = jsonschema::JSONSchema::compile(&schema)
        .map_err(|error| anyhow!("compile promotion schema: {error}"))?;
    if let Err(errors) = compiled.validate(&value) {
        bail!(
            "promotion manifest schema validation failed: {}",
            errors
                .map(|error| error.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
    let manifest: LegacyPromotionManifest =
        serde_json::from_value(value).context("deserialize promotion manifest")?;
    validate_manifest(path, &bytes, &manifest)?;
    let mut languages = BTreeMap::new();
    for document in manifest.documents {
        if languages
            .insert(document.case_file.clone(), document.language)
            .is_some()
        {
            bail!(
                "promotion manifest contains duplicate document language: {}",
                document.case_file
            );
        }
    }
    Ok(languages)
}

/// Read the languages registered by an analyzer candidate's checked-in LSP
/// profile.  Bifrost is the full-corpus candidate and therefore returns
/// `None`, while an LSP profile returns its exact language set.
pub fn registered_candidate_languages(
    root: &Path,
    runner: &str,
    profile: Option<&str>,
) -> Result<Option<BTreeSet<String>>> {
    match runner {
        "bifrost" => {
            if profile.is_some() {
                bail!("Bifrost candidate unexpectedly declares an LSP profile");
            }
            Ok(None)
        }
        "lsp" => {
            let profile = profile.context("LSP candidate is missing its registered profile")?;
            let relative = safe_repo_relative_path(profile, "candidate LSP profile")?;
            let root = root
                .canonicalize()
                .context("canonicalize candidate registry root")?;
            let path = root.join(relative);
            let canonical = path
                .canonicalize()
                .with_context(|| format!("canonicalize candidate LSP profile {profile}"))?;
            if !canonical.starts_with(&root) {
                bail!("candidate LSP profile resolves outside the repository: {profile}");
            }
            let value: serde_json::Value = serde_json::from_slice(
                &fs::read(&canonical)
                    .with_context(|| format!("read candidate LSP profile {profile}"))?,
            )
            .with_context(|| format!("parse candidate LSP profile {profile}"))?;
            let languages = value
                .get("languages")
                .and_then(serde_json::Value::as_array)
                .context("candidate LSP profile has no languages array")?;
            let mut result = BTreeSet::new();
            for language in languages {
                let language = language
                    .as_str()
                    .filter(|language| !language.trim().is_empty())
                    .context("candidate LSP profile contains an invalid language")?;
                if !result.insert(language.to_string()) {
                    bail!("candidate LSP profile repeats language {language}");
                }
            }
            if result.is_empty() {
                bail!("candidate LSP profile declares no languages");
            }
            Ok(Some(result))
        }
        other => bail!("unsupported candidate runner {other}"),
    }
}

/// Read the immutable language scope serialized for a legacy-promoted
/// candidate. Historical evaluation manifests may omit this field, but every
/// legacy-promoted manifest uses schema v5 and must carry it explicitly.
pub fn required_legacy_candidate_languages(
    candidate_id: &str,
    languages: Option<&[String]>,
) -> Result<BTreeSet<String>> {
    let languages = languages.with_context(|| {
        format!("legacy-promoted candidate {candidate_id} is missing its registered language scope")
    })?;
    let languages = languages.iter().cloned().collect::<BTreeSet<_>>();
    if languages.is_empty() {
        bail!("legacy-promoted candidate {candidate_id} has an empty registered language scope");
    }
    Ok(languages)
}

/// Validate one report against the exact language slice registered for its
/// candidate.  The returned keys can be unioned across reports to prove that
/// the selected candidates cover the complete promotion denominator.
pub fn validate_report_against_promotion_scope(
    report: &RunReport,
    audit: &LegacyPromotionAudit,
    document_languages: &BTreeMap<String, String>,
    allowed_languages: Option<&BTreeSet<String>>,
) -> Result<BTreeSet<(String, String)>> {
    let expected_files = audit
        .case_ids_by_file
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let language_files = document_languages
        .iter()
        .filter(|(_, language)| {
            allowed_languages
                .map(|allowed| allowed.contains(language.as_str()))
                .unwrap_or(true)
        })
        .map(|(file, _)| file.as_str())
        .collect::<BTreeSet<_>>();
    if document_languages.len() != expected_files.len()
        || !expected_files
            .iter()
            .all(|file| document_languages.contains_key(*file))
    {
        bail!("promotion document language map drifted from the promotion audit");
    }

    let listed_files = report
        .case_files
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if listed_files.len() != report.case_files.len() || listed_files != language_files {
        bail!("report case-file scope does not match its registered promotion language slice");
    }

    let mut actual_documents = BTreeMap::new();
    let mut keys = BTreeSet::new();
    for document in &report.documents {
        if actual_documents
            .insert(document.case_file.as_str(), document)
            .is_some()
        {
            bail!(
                "report contains duplicate promotion document {}",
                document.case_file
            );
        }
        let expected_language = document_languages
            .get(&document.case_file)
            .with_context(|| {
                format!(
                    "report contains unknown promotion document {}",
                    document.case_file
                )
            })?;
        if document.language != *expected_language {
            bail!(
                "report document {} has language {}, expected {}",
                document.case_file,
                document.language,
                expected_language
            );
        }
        if !language_files.contains(document.case_file.as_str()) {
            bail!(
                "report document {} is outside the candidate's registered language slice",
                document.case_file
            );
        }
        let expected_ids = audit
            .case_ids_by_file
            .get(&document.case_file)
            .with_context(|| {
                format!(
                    "promotion audit omits report document {}",
                    document.case_file
                )
            })?
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let mut actual_ids = BTreeSet::new();
        for case in &document.cases {
            if !actual_ids.insert(case.id.as_str()) {
                bail!(
                    "report contains duplicate promotion case {} / {}",
                    document.case_file,
                    case.id
                );
            }
            keys.insert((document.case_file.clone(), case.id.clone()));
        }
        if actual_ids != expected_ids {
            bail!(
                "report case IDs drift from promotion manifest for {}",
                document.case_file
            );
        }
    }
    if actual_documents.len() != language_files.len()
        || actual_documents.keys().copied().collect::<BTreeSet<_>>() != language_files
    {
        bail!("report documents do not match its registered promotion language slice");
    }
    Ok(keys)
}

pub fn promotion_case_keys(audit: &LegacyPromotionAudit) -> BTreeSet<(String, String)> {
    audit
        .case_ids_by_file
        .iter()
        .flat_map(|(file, ids)| ids.iter().map(move |id| (file.clone(), id.clone())))
        .collect()
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
    use crate::{
        runners::{
            CaseRunReport, DocumentRunReport, ExecutionEnvironment, ExecutionMode, PlatformScope,
            RunInvocation, RunReport, RunTotals, RunnerMetadata,
        },
        CorpusPartition, CorpusSelection, GroundTruthReviewStatus, ReferencePolicy,
    };

    fn partition_fixture() -> (LegacyPromotionAudit, BTreeMap<String, String>) {
        let languages = (0..11)
            .map(|index| format!("language-{index}"))
            .collect::<Vec<_>>();
        let mut case_ids_by_file = BTreeMap::new();
        let mut document_languages = BTreeMap::new();
        let mut denominators = BTreeMap::new();
        for language in &languages {
            let file = format!("benchmarks/cases/{language}.yaml");
            let ids = (0..10)
                .map(|index| format!("{language}-{index}"))
                .collect::<Vec<_>>();
            case_ids_by_file.insert(file.clone(), ids);
            document_languages.insert(file, language.clone());
            denominators.insert(language.clone(), 10);
        }
        (
            LegacyPromotionAudit {
                promotion_id: "legacy-v1".into(),
                claim_scope: "corpus-bounded".into(),
                manifest: ArtifactLink {
                    file: "manifest.json".into(),
                    sha256: "a".repeat(64),
                },
                eligibility_policy: ArtifactLink {
                    file: "policy.json".into(),
                    sha256: "b".repeat(64),
                },
                balanced_core_per_language: 10,
                balanced_core_case_count: 110,
                overflow_case_count: 0,
                control_case_count: 0,
                denominators,
                case_ids_by_file,
            },
            document_languages,
        )
    }

    fn report_for_document(file: &str, language: &str, ids: &[String]) -> RunReport {
        let cases = ids
            .iter()
            .map(|id| CaseRunReport {
                id: id.clone(),
                status: crate::runners::CaseStatus::Passed,
                required_destination_status: None,
                location_metrics: None,
                expected_failure_reason: None,
                not_planned_reason: None,
                unsupported_reason: None,
                declaration_to_usages: None,
                usage_to_declaration: Vec::new(),
                compatible_usage_to_declaration: Vec::new(),
                type_lookups: Vec::new(),
                diagnostics: Vec::new(),
            })
            .collect();
        RunReport {
            usagebench_version: "0.3.0".into(),
            usagebench_revision: "0123456789abcdef0123456789abcdef01234567".into(),
            usagebench_release: Some("v0.3.0".into()),
            runner: RunnerMetadata {
                name: "test-runner".into(),
                requested_version: "1.0.0".into(),
                resolved_version: "1.0.0".into(),
                source: "https://example.test".into(),
                adapter_version: "test".into(),
                capabilities: Vec::new(),
            },
            invocation: RunInvocation {
                include_unsupported: false,
                include_definition_lookups: false,
                scan_usages_max_duration_secs: None,
                profile: None,
                profile_sha256: None,
                case_id: None,
            },
            timings: Default::default(),
            completed: true,
            requested_case_files: Vec::new(),
            requested_totals: Default::default(),
            semantic_pack_runs: Vec::new(),
            environment: ExecutionEnvironment {
                operating_system: "test".into(),
                architecture: "test".into(),
                execution_mode: ExecutionMode::Native,
                platform_scope: PlatformScope::HostSpecific,
                reference_environment: None,
                container: None,
                analyzer_executable: crate::runners::ExecutableProvenance {
                    command: "test".into(),
                    resolved_path: None,
                    sha256: None,
                },
                toolchains: BTreeMap::new(),
            },
            bifrost_repo: None,
            bifrost_commit: None,
            bifrost_resolved_commit: None,
            started_at_unix_seconds: 1,
            finished_at_unix_seconds: 2,
            case_files: vec![file.into()],
            totals: RunTotals::default(),
            documents: vec![DocumentRunReport {
                case_file: file.into(),
                language: language.into(),
                source_root: "fixtures/test".into(),
                corpus_partition: CorpusPartition::Development,
                corpus_selection: CorpusSelection::AnalyzerInformed,
                ground_truth_status: GroundTruthReviewStatus::LegacyUnattributed,
                reference_policy: ReferencePolicy::BindingsOptional,
                cases,
            }],
        }
    }

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
                sha256: sha256(&fs::read("Cargo.toml").unwrap()),
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
    fn accepts_the_complete_eleven_language_partition() {
        let (audit, document_languages) = partition_fixture();
        let mut union = BTreeSet::new();
        for (file, language) in &document_languages {
            let ids = audit.case_ids_by_file.get(file).unwrap();
            let allowed = BTreeSet::from([language.clone()]);
            union.extend(
                validate_report_against_promotion_scope(
                    &report_for_document(file, language, ids),
                    &audit,
                    &document_languages,
                    Some(&allowed),
                )
                .unwrap(),
            );
        }
        assert_eq!(union, promotion_case_keys(&audit));
    }

    #[test]
    fn legacy_candidate_scope_is_required_and_non_empty() {
        let missing = required_legacy_candidate_languages("gopls", None).unwrap_err();
        assert!(missing
            .to_string()
            .contains("missing its registered language scope"));

        let empty = required_legacy_candidate_languages("gopls", Some(&[])).unwrap_err();
        assert!(empty
            .to_string()
            .contains("empty registered language scope"));

        assert_eq!(
            required_legacy_candidate_languages("gopls", Some(&["go".into()])).unwrap(),
            BTreeSet::from(["go".into()])
        );
    }

    #[test]
    fn rejects_missing_case_from_a_candidate_report() {
        let (audit, document_languages) = partition_fixture();
        let (file, language) = document_languages.iter().next().unwrap();
        let mut ids = audit.case_ids_by_file.get(file).unwrap().clone();
        ids.pop();
        let allowed = BTreeSet::from([language.clone()]);
        let error = validate_report_against_promotion_scope(
            &report_for_document(file, language, &ids),
            &audit,
            &document_languages,
            Some(&allowed),
        )
        .unwrap_err();
        assert!(error.to_string().contains("case IDs drift"));
    }

    #[test]
    fn rejects_extra_case_from_a_candidate_report() {
        let (audit, document_languages) = partition_fixture();
        let (file, language) = document_languages.iter().next().unwrap();
        let mut ids = audit.case_ids_by_file.get(file).unwrap().clone();
        ids.push("not-promoted".into());
        let allowed = BTreeSet::from([language.clone()]);
        let error = validate_report_against_promotion_scope(
            &report_for_document(file, language, &ids),
            &audit,
            &document_languages,
            Some(&allowed),
        )
        .unwrap_err();
        assert!(error.to_string().contains("case IDs drift"));
    }

    #[test]
    fn rejects_cross_language_document_from_a_candidate_report() {
        let (audit, document_languages) = partition_fixture();
        let (file, language) = document_languages.iter().next().unwrap();
        let other_language = document_languages
            .values()
            .find(|candidate| *candidate != language)
            .unwrap();
        let ids = audit.case_ids_by_file.get(file).unwrap();
        let allowed = BTreeSet::from([language.clone()]);
        let error = validate_report_against_promotion_scope(
            &report_for_document(file, other_language, ids),
            &audit,
            &document_languages,
            Some(&allowed),
        )
        .unwrap_err();
        assert!(error.to_string().contains("expected"));
    }

    #[test]
    fn detects_union_drift_when_a_language_report_is_omitted() {
        let (audit, document_languages) = partition_fixture();
        let mut union = BTreeSet::new();
        for (file, language) in document_languages.iter().skip(1) {
            let allowed = BTreeSet::from([language.clone()]);
            union.extend(
                validate_report_against_promotion_scope(
                    &report_for_document(file, language, audit.case_ids_by_file.get(file).unwrap()),
                    &audit,
                    &document_languages,
                    Some(&allowed),
                )
                .unwrap(),
            );
        }
        assert_ne!(union, promotion_case_keys(&audit));
        assert_eq!(union.len(), 100);
    }

    #[test]
    fn old_benchmark_selection_model_remains_compatible() {
        let source = fs::read("benchmarks/cases/go-baseline.yaml").unwrap();
        let document: BenchmarkDocument = serde_yaml::from_slice(&source).unwrap();
        assert_eq!(document.corpus.selection, CorpusSelection::AnalyzerInformed);
    }
}
