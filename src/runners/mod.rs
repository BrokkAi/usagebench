//! Analyzer adapters and shared benchmark-runner contracts.
//!
//! Each adapter is responsible for preparing an exact tool version and
//! translating that tool's public query surface into UsageBench locations.

use crate::{
    benchmark_source_path, BenchmarkCase, CorpusPartition, CorpusSelection,
    GroundTruthReviewStatus, NavigationOperation, ReferencePolicy, SymbolKind, SymbolLocation,
};
use anyhow::{bail, Context, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path, process::Command};

pub mod bifrost;
mod environment;
pub mod lsp;
mod lsp_protocol;
mod mcp;
pub mod report_compare;

/// Version of the machine-readable analyzer-run report contract.
pub const RUN_REPORT_SCHEMA_VERSION: u32 = 1;

pub use environment::{
    ContainerProvenance, ExecutableProvenance, ExecutionEnvironment, ExecutionMode, PlatformScope,
    ReferenceEnvironmentProvenance,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunnerMetadata {
    pub name: String,
    pub requested_version: String,
    pub resolved_version: String,
    pub source: String,
    pub adapter_version: String,
    pub capabilities: Vec<RunnerCapability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunnerCapability {
    pub operation: RunnerOperation,
    pub support: CapabilitySupport,
    pub notes: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunnerOperation {
    DeclarationToUsages,
    DeclarationLookup,
    DefinitionLookup,
    TypeLookup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CapabilitySupport {
    Native,
    Recovered,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunReport {
    /// Version of the Rust CLI and runner adapters.
    pub usagebench_version: String,
    /// Exact UsageBench source commit, with `-dirty` when local changes exist.
    pub usagebench_revision: String,
    /// Benchmark release tag for a clean tagged checkout or release bundle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usagebench_release: Option<String>,
    pub runner: RunnerMetadata,
    pub invocation: RunInvocation,
    pub environment: ExecutionEnvironment,
    /// Compatibility fields retained for existing Bifrost report consumers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bifrost_repo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bifrost_commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bifrost_resolved_commit: Option<String>,
    pub started_at_unix_seconds: u64,
    pub finished_at_unix_seconds: u64,
    pub case_files: Vec<String>,
    pub totals: RunTotals,
    pub documents: Vec<DocumentRunReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunInvocation {
    pub include_unsupported: bool,
    pub include_definition_lookups: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub case_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UsagebenchProvenance {
    pub revision: String,
    pub release: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseMetadata {
    revision: String,
    release_tag: String,
}

pub(crate) fn resolve_usagebench_provenance(repo_root: &Path) -> Result<UsagebenchProvenance> {
    let canonical_repo_root = fs::canonicalize(repo_root)
        .with_context(|| format!("canonicalize UsageBench root {}", repo_root.display()))?;
    let owns_git_worktree = git_stdout(repo_root, &["rev-parse", "--show-toplevel"])
        .and_then(|path| fs::canonicalize(path).ok())
        .is_some_and(|git_root| git_root == canonical_repo_root);
    if owns_git_worktree {
        let commit = git_stdout(repo_root, &["rev-parse", "HEAD"])
            .context("resolve UsageBench Git revision")?;
        let status = git_stdout(
            repo_root,
            &["status", "--porcelain", "--untracked-files=normal"],
        )
        .context("inspect UsageBench working tree for provenance")?;
        let dirty = !status.is_empty();
        let revision = if dirty {
            format!("{commit}-dirty")
        } else {
            commit
        };
        let release = if dirty {
            None
        } else {
            git_stdout(
                repo_root,
                &["tag", "--points-at", "HEAD", "--list", "v[0-9]*"],
            )
            .and_then(|tags| {
                tags.lines()
                    .map(str::trim)
                    .find(|tag| is_release_tag(tag))
                    .map(str::to_string)
            })
        };
        return Ok(UsagebenchProvenance { revision, release });
    }

    let metadata_path = repo_root.join(".usagebench-release.json");
    if metadata_path.is_file() {
        let metadata: ReleaseMetadata = serde_json::from_slice(
            &fs::read(&metadata_path)
                .with_context(|| format!("read {}", metadata_path.display()))?,
        )
        .with_context(|| format!("parse {}", metadata_path.display()))?;
        if metadata.revision.is_empty() || !is_release_tag(&metadata.release_tag) {
            bail!(
                "invalid UsageBench release provenance in {}",
                metadata_path.display()
            );
        }
        return Ok(UsagebenchProvenance {
            revision: metadata.revision,
            release: Some(metadata.release_tag),
        });
    }

    bail!(
        "could not resolve UsageBench source revision from Git or {}",
        metadata_path.display()
    )
}

fn git_stdout(repo_root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn is_release_tag(tag: &str) -> bool {
    let Some(version) = tag.strip_prefix('v') else {
        return false;
    };
    let parts = version.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunTotals {
    pub documents: usize,
    pub cases: usize,
    pub development_cases: usize,
    pub evaluation_cases: usize,
    pub passed: usize,
    pub near_misses: usize,
    pub position_unverified: usize,
    pub improved: usize,
    pub failed: usize,
    pub expected_failures: usize,
    pub not_planned: usize,
    pub unsupported: usize,
    pub skipped: usize,
    pub errors: usize,
    /// Recall-forward result derived from the raw locations in each case.
    /// Unlike the strict counters above, this tolerates line-only or containing
    /// ranges and extra results, and may use an explicitly reviewed compatible
    /// operation.
    #[serde(default)]
    pub required_destinations: RequiredDestinationTotals,
    /// Location-level reference and navigation evidence. Reports produced
    /// before UsageBench 0.2.0 omit this field; new reports always include it.
    /// A zero-valued value means the case has no scoreable location query.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location_metrics: Option<LocationMetrics>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RequiredDestinationTotals {
    /// Cases for which every required lookup was available and executed.
    pub scoreable_cases: usize,
    pub found: usize,
    pub missing: usize,
    pub not_planned: usize,
    pub unsupported: usize,
    pub skipped: usize,
    pub errors: usize,
    /// Reports produced before this metric was added.
    pub unreported: usize,
}

/// Raw location-level counts from reference and navigation queries.
///
/// Rates are deliberately derived from these integer counts so report
/// consumers can reproduce micro, case-macro, and equal-profile views without
/// depending on serialized floating-point values.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LocationMetrics {
    /// Cases contributing at least one scoreable location query.
    pub cases: usize,
    /// Cases for which every scoreable query returned exactly the required set.
    pub exact_set_cases: usize,
    pub queries: usize,
    pub successful_queries: usize,
    /// Query-level evidence retained for reproducible case derivation.
    pub exact_set_queries: usize,
    pub required_locations: usize,
    pub true_positives: usize,
    /// Returned locations that are neither required nor policy-allowed.
    pub false_positives: usize,
    pub false_negatives: usize,
    pub successful_query_extras: usize,
    pub returned_locations: ReturnedLocationTotals,
    pub range_quality: RangeQualityTotals,
}

impl LocationMetrics {
    pub(crate) fn merge(&mut self, other: &Self) {
        self.cases += other.cases;
        self.exact_set_cases += other.exact_set_cases;
        self.queries += other.queries;
        self.successful_queries += other.successful_queries;
        self.exact_set_queries += other.exact_set_queries;
        self.required_locations += other.required_locations;
        self.true_positives += other.true_positives;
        self.false_positives += other.false_positives;
        self.false_negatives += other.false_negatives;
        self.successful_query_extras += other.successful_query_extras;
        self.returned_locations.merge(&other.returned_locations);
        self.range_quality.merge(&other.range_quality);
    }

    pub(crate) fn checked_merge(&mut self, other: &Self) -> Result<()> {
        checked_add_assign(&mut self.cases, other.cases, "location cases")?;
        checked_add_assign(
            &mut self.exact_set_cases,
            other.exact_set_cases,
            "exact-set cases",
        )?;
        checked_add_assign(&mut self.queries, other.queries, "location queries")?;
        checked_add_assign(
            &mut self.successful_queries,
            other.successful_queries,
            "successful location queries",
        )?;
        checked_add_assign(
            &mut self.exact_set_queries,
            other.exact_set_queries,
            "exact-set queries",
        )?;
        checked_add_assign(
            &mut self.required_locations,
            other.required_locations,
            "required locations",
        )?;
        checked_add_assign(
            &mut self.true_positives,
            other.true_positives,
            "location true positives",
        )?;
        checked_add_assign(
            &mut self.false_positives,
            other.false_positives,
            "location false positives",
        )?;
        checked_add_assign(
            &mut self.false_negatives,
            other.false_negatives,
            "location false negatives",
        )?;
        checked_add_assign(
            &mut self.successful_query_extras,
            other.successful_query_extras,
            "successful-query extras",
        )?;
        self.returned_locations
            .checked_merge(&other.returned_locations)?;
        self.range_quality.checked_merge(&other.range_quality)?;
        Ok(())
    }

    fn record_query(&mut self, query: QueryLocationMetrics) {
        self.queries += 1;
        self.required_locations += query.required_locations;
        self.true_positives += query.true_positives;
        self.false_positives += query.false_positives();
        self.false_negatives += query.false_negatives;
        self.returned_locations.merge(&query.returned_locations);
        self.range_quality.merge(&query.range_quality);
        if query.false_negatives == 0 {
            self.successful_queries += 1;
            self.successful_query_extras += query.returned_locations.extra_count();
        }
        if query.exact_set() {
            self.exact_set_queries += 1;
        }
    }

    pub(crate) fn validate(&self) -> Result<()> {
        let required_outcomes = self
            .true_positives
            .checked_add(self.false_negatives)
            .context("location TP + FN overflow")?;
        if self.required_locations != required_outcomes {
            bail!(
                "requiredLocations {} does not equal TP + FN {}",
                self.required_locations,
                required_outcomes
            );
        }

        let false_positives = self
            .returned_locations
            .related_unallowed
            .checked_add(self.returned_locations.unrelated)
            .context("related-unallowed + unrelated overflow")?;
        if self.false_positives != false_positives {
            bail!(
                "falsePositives {} does not equal related-unallowed + unrelated {}",
                self.false_positives,
                false_positives
            );
        }
        if self.returned_locations.required != self.true_positives {
            bail!(
                "returned required locations {} do not equal truePositives {}",
                self.returned_locations.required,
                self.true_positives
            );
        }
        if self.range_quality.wrong_location != self.false_positives {
            bail!(
                "wrongLocation {} does not equal falsePositives {}",
                self.range_quality.wrong_location,
                self.false_positives
            );
        }
        if self.range_quality.missing != self.false_negatives {
            bail!(
                "range missing {} does not equal falseNegatives {}",
                self.range_quality.missing,
                self.false_negatives
            );
        }

        let ranged_required = self
            .range_quality
            .exact_token
            .checked_add(self.range_quality.containing)
            .and_then(|value| value.checked_add(self.range_quality.line_only))
            .and_then(|value| value.checked_add(self.range_quality.missing))
            .context("required range-quality counts overflow")?;
        if ranged_required != self.required_locations {
            bail!(
                "required range-quality outcomes {} do not equal requiredLocations {}",
                ranged_required,
                self.required_locations
            );
        }
        if self.successful_queries > self.queries
            || self.exact_set_queries > self.queries
            || self.exact_set_queries > self.successful_queries
            || self.cases > self.queries
            || self.exact_set_cases > self.cases
        {
            bail!("location case/query counts are inconsistent");
        }
        if self.successful_queries == 0 && self.successful_query_extras != 0 {
            bail!("successfulQueryExtras is non-zero without a successful query");
        }
        let total_extras = self
            .returned_locations
            .policy_allowed
            .checked_add(self.false_positives)
            .context("returned extra count overflow")?;
        if self.successful_query_extras > total_extras {
            bail!("successfulQueryExtras exceeds all returned extras");
        }
        if self.false_negatives == 0
            && (self.successful_queries != self.queries
                || self.successful_query_extras != total_extras)
        {
            bail!("all queries must be successful when there are no false negatives");
        }
        self.true_positives
            .checked_add(self.false_positives)
            .and_then(|value| value.checked_add(self.returned_locations.policy_allowed))
            .context("location precision denominator overflow")?;
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReturnedLocationTotals {
    pub required: usize,
    pub policy_allowed: usize,
    pub related_unallowed: usize,
    pub unrelated: usize,
}

impl ReturnedLocationTotals {
    fn merge(&mut self, other: &Self) {
        self.required += other.required;
        self.policy_allowed += other.policy_allowed;
        self.related_unallowed += other.related_unallowed;
        self.unrelated += other.unrelated;
    }

    fn checked_merge(&mut self, other: &Self) -> Result<()> {
        checked_add_assign(&mut self.required, other.required, "returned required")?;
        checked_add_assign(
            &mut self.policy_allowed,
            other.policy_allowed,
            "returned policy-allowed",
        )?;
        checked_add_assign(
            &mut self.related_unallowed,
            other.related_unallowed,
            "returned related-unallowed",
        )?;
        checked_add_assign(&mut self.unrelated, other.unrelated, "returned unrelated")?;
        Ok(())
    }

    fn extra_count(&self) -> usize {
        self.policy_allowed + self.related_unallowed + self.unrelated
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RangeQualityTotals {
    pub exact_token: usize,
    pub containing: usize,
    pub line_only: usize,
    pub wrong_location: usize,
    pub missing: usize,
}

impl RangeQualityTotals {
    fn merge(&mut self, other: &Self) {
        self.exact_token += other.exact_token;
        self.containing += other.containing;
        self.line_only += other.line_only;
        self.wrong_location += other.wrong_location;
        self.missing += other.missing;
    }

    fn checked_merge(&mut self, other: &Self) -> Result<()> {
        checked_add_assign(
            &mut self.exact_token,
            other.exact_token,
            "exact-token ranges",
        )?;
        checked_add_assign(&mut self.containing, other.containing, "containing ranges")?;
        checked_add_assign(&mut self.line_only, other.line_only, "line-only ranges")?;
        checked_add_assign(
            &mut self.wrong_location,
            other.wrong_location,
            "wrong-location ranges",
        )?;
        checked_add_assign(&mut self.missing, other.missing, "missing ranges")?;
        Ok(())
    }
}

fn checked_add_assign(target: &mut usize, value: usize, label: &str) -> Result<()> {
    *target = target
        .checked_add(value)
        .with_context(|| format!("{label} overflow while aggregating location metrics"))?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DocumentRunReport {
    pub case_file: String,
    pub language: String,
    pub source_root: String,
    pub corpus_partition: CorpusPartition,
    pub corpus_selection: CorpusSelection,
    pub ground_truth_status: GroundTruthReviewStatus,
    pub reference_policy: ReferencePolicy,
    pub cases: Vec<CaseRunReport>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CaseStatus {
    Passed,
    NearMiss,
    PositionUnverified,
    Improved,
    Failed,
    ExpectedFailure,
    NotPlanned,
    Unsupported,
    Skipped,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RequiredDestinationStatus {
    Found,
    Missing,
    NotPlanned,
    Unsupported,
    Skipped,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CaseRunReport {
    pub id: String,
    pub status: CaseStatus,
    /// Recall-forward destination result. This is computed from raw locations,
    /// tolerates line-only or containing ranges and extra results, and may use
    /// an explicitly reviewed compatible operation. `status` remains the strict
    /// canonical endpoint score.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_destination_status: Option<RequiredDestinationStatus>,
    /// Raw reference/navigation metrics for reports produced by UsageBench
    /// 0.2.0 and newer. `None` denotes a legacy report or a case without a
    /// scoreable reference/navigation query.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location_metrics: Option<LocationMetrics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_failure_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not_planned_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unsupported_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declaration_to_usages: Option<DeclarationUsageReport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub usage_to_declaration: Vec<UsageDefinitionReport>,
    /// Alternate endpoint results. Each entry identifies its canonical
    /// `usageToDeclaration` lookup and is compatibility evidence, not a
    /// silent retry of the canonical score.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compatible_usage_to_declaration: Vec<CompatibleUsageDefinitionReport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub type_lookups: Vec<TypeLookupReport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<RunDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeclarationUsageReport {
    pub status: CaseStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    pub expected: Vec<NormalizedLocation>,
    pub expected_unproven: Vec<NormalizedLocation>,
    pub allowed_extra: Vec<NormalizedLocation>,
    pub allowed_unproven: Vec<NormalizedLocation>,
    pub actual: Vec<NormalizedLocation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unproven: Vec<NormalizedLocation>,
    pub missing: Vec<NormalizedLocation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_unproven: Vec<NormalizedLocation>,
    pub unexpected: Vec<NormalizedLocation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unexpected_unproven: Vec<NormalizedLocation>,
    /// Expected locations for which the adapter returned only path/line data.
    /// These are not exact matches because the token range was not verified.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub position_unverified: Vec<NormalizedLocation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_usages: Vec<ClassifiedExtraUsage>,
    pub partial: bool,
    pub raw_statuses: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClassifiedExtraUsage {
    pub location: NormalizedLocation,
    pub classification: ExtraUsageClassification,
    pub disposition: ExtraUsageDisposition,
    pub rationale: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExtraUsageClassification {
    ImportBinding,
    ReexportBinding,
    ExportMetadata,
    DeclarationOrDefinition,
    Unclassified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExtraUsageDisposition {
    AllowedPolicyExtra,
    Unexpected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UsageDefinitionReport {
    pub status: CaseStatus,
    pub operation: NavigationOperation,
    pub usage: NormalizedLocation,
    pub expected_declaration: NormalizedLocation,
    pub actual_declarations: Vec<NormalizedLocation>,
    pub raw_status: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<RunDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompatibleUsageDefinitionReport {
    pub usage_lookup_index: usize,
    pub canonical_operation: NavigationOperation,
    pub reports: Vec<UsageDefinitionReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TypeLookupReport {
    pub status: CaseStatus,
    pub expression: NormalizedLocation,
    pub expected_type: NormalizedLocation,
    pub actual_types: Vec<NormalizedLocation>,
    pub raw_status: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<RunDiagnostic>,
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedLocation {
    pub path: String,
    pub line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_column: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunDiagnostic {
    pub kind: String,
    pub message: String,
}

pub fn generated_report_schema_json() -> Result<String> {
    let schema = schemars::schema_for!(RunReport);
    serde_json::to_string_pretty(&schema).context("serialize generated runner report schema")
}

#[cfg(test)]
mod provenance_tests {
    use super::*;

    #[test]
    fn accepts_release_semver_tags() {
        assert!(is_release_tag("v0.1.0"));
        assert!(is_release_tag("v12.34.56"));
        assert!(!is_release_tag("0.1.0"));
        assert!(!is_release_tag("v0.1"));
        assert!(!is_release_tag("v0.1.0-rc.1"));
    }

    #[test]
    fn reads_provenance_from_release_bundle() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(".usagebench-release.json"),
            r#"{"revision":"abc123","releaseTag":"v0.1.0"}"#,
        )
        .unwrap();

        let provenance = resolve_usagebench_provenance(tempdir.path()).unwrap();

        assert_eq!(provenance.revision, "abc123");
        assert_eq!(provenance.release.as_deref(), Some("v0.1.0"));
    }

    #[test]
    fn release_bundle_nested_in_another_worktree_uses_its_manifest() {
        let tempdir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        fs::write(
            tempdir.path().join(".usagebench-release.json"),
            r#"{"revision":"release123","releaseTag":"v1.2.3"}"#,
        )
        .unwrap();

        let provenance = resolve_usagebench_provenance(tempdir.path()).unwrap();

        assert_eq!(provenance.revision, "release123");
        assert_eq!(provenance.release.as_deref(), Some("v1.2.3"));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LocationMatch {
    None,
    LineOnly,
    Containing,
    Exact,
}

pub(crate) fn location_match(
    actual: &NormalizedLocation,
    expected: &NormalizedLocation,
) -> LocationMatch {
    if actual.path != expected.path || actual.line != expected.line {
        return LocationMatch::None;
    }
    match (
        actual.column,
        actual.end_line,
        actual.end_column,
        expected.column,
        expected.end_line,
        expected.end_column,
    ) {
        (
            Some(actual_column),
            Some(actual_end_line),
            Some(actual_end_column),
            Some(expected_column),
            Some(expected_end_line),
            Some(expected_end_column),
        ) if actual_column == expected_column
            && actual_end_line == expected_end_line
            && actual_end_column == expected_end_column =>
        {
            LocationMatch::Exact
        }
        (
            Some(actual_column),
            Some(actual_end_line),
            Some(actual_end_column),
            Some(expected_column),
            Some(expected_end_line),
            Some(expected_end_column),
        ) if actual_column <= expected_column
            && (actual_end_line > expected_end_line
                || (actual_end_line == expected_end_line
                    && actual_end_column >= expected_end_column)) =>
        {
            LocationMatch::Containing
        }
        (None, _, _, _, _, _) | (_, None, None, _, _, _) => LocationMatch::LineOnly,
        _ => LocationMatch::None,
    }
}

#[derive(Debug, Default)]
struct QueryLocationMetrics {
    required_locations: usize,
    true_positives: usize,
    false_negatives: usize,
    returned_locations: ReturnedLocationTotals,
    range_quality: RangeQualityTotals,
}

impl QueryLocationMetrics {
    fn exact_set(&self) -> bool {
        self.range_quality.exact_token == self.required_locations
            && self.returned_locations.required == self.true_positives
            && self.returned_locations.extra_count() == 0
    }

    fn record_returned(&mut self, class: ReturnedLocationClass) {
        match class {
            ReturnedLocationClass::Required => self.returned_locations.required += 1,
            ReturnedLocationClass::PolicyAllowed => self.returned_locations.policy_allowed += 1,
            ReturnedLocationClass::RelatedUnallowed => {
                self.returned_locations.related_unallowed += 1
            }
            ReturnedLocationClass::Unrelated => self.returned_locations.unrelated += 1,
        }
    }

    fn finish(mut self) -> Self {
        self.range_quality.wrong_location = self.false_positives();
        self
    }

    fn false_positives(&self) -> usize {
        self.returned_locations.related_unallowed + self.returned_locations.unrelated
    }
}

#[derive(Debug, Clone, Copy)]
enum ReturnedLocationClass {
    Required,
    PolicyAllowed,
    RelatedUnallowed,
    Unrelated,
}

/// Compute location evidence while the authored case is still available.
/// Navigation reports do not retain `allowedExtraTargets`, so this cannot be
/// reconstructed faithfully from a legacy serialized report.
pub(crate) fn case_location_metrics(
    case: &BenchmarkCase,
    declaration: Option<&DeclarationUsageReport>,
    canonical_definitions: &[UsageDefinitionReport],
    compatible_definitions: &[CompatibleUsageDefinitionReport],
) -> LocationMetrics {
    if case.not_planned.is_some() {
        return LocationMetrics::default();
    }

    let mut metrics = LocationMetrics::default();
    if let Some(report) = declaration.filter(|report| metric_status_is_scoreable(report.status)) {
        metrics.record_query(reference_query_metrics(report));
    }

    for (index, canonical) in canonical_definitions.iter().enumerate() {
        let Some(lookup) = case.usage_lookups.get(index) else {
            continue;
        };
        // A no-movement contract permits an empty response and therefore has
        // no returned destination whose precision or range can be measured.
        if lookup.expect_no_movement {
            continue;
        }

        let mut selected =
            metric_status_is_scoreable(canonical.status).then_some((true, canonical));
        for alternative in navigation_reports_for_lookup(index, canonical, compatible_definitions)
            .skip(1)
            .filter(|report| metric_status_is_scoreable(report.status))
        {
            let alternative_rank = (
                best_location_match(
                    &alternative.actual_declarations,
                    &alternative.expected_declaration,
                ),
                false,
            );
            let selected_rank = selected.map(|(canonical, report)| {
                (
                    best_location_match(&report.actual_declarations, &report.expected_declaration),
                    canonical,
                )
            });
            if selected_rank.is_none_or(|rank| alternative_rank > rank) {
                selected = Some((false, alternative));
            }
        }

        if let Some((_, report)) = selected {
            let allowed_extra_targets = lookup
                .allowed_extra_targets
                .iter()
                .map(normalize_symbol_location)
                .collect::<Result<Vec<_>>>()
                .expect("validated navigation targets must normalize");
            metrics.record_query(navigation_query_metrics(report, &allowed_extra_targets));
        }
    }

    if metrics.queries == 0 {
        return metrics;
    }
    metrics.cases = 1;
    metrics.exact_set_cases = usize::from(metrics.exact_set_queries == metrics.queries);
    metrics
}

fn metric_status_is_scoreable(status: CaseStatus) -> bool {
    !matches!(
        status,
        CaseStatus::NotPlanned | CaseStatus::Unsupported | CaseStatus::Skipped | CaseStatus::Error
    )
}

fn reference_query_metrics(report: &DeclarationUsageReport) -> QueryLocationMetrics {
    let mut query = QueryLocationMetrics {
        required_locations: report.expected.len() + report.expected_unproven.len(),
        ..QueryLocationMetrics::default()
    };

    let required = report
        .expected
        .iter()
        .chain(&report.expected_unproven)
        .collect::<Vec<_>>();
    let returned = report
        .actual
        .iter()
        .chain(&report.unproven)
        .collect::<Vec<_>>();
    let matches = match_required_locations(&required, &returned);
    for quality in &matches.required_quality {
        record_range_quality(*quality, &mut query);
    }

    let classified_extras = report
        .extra_usages
        .iter()
        .map(|extra| (&extra.location, extra))
        .collect::<BTreeMap<_, _>>();
    let actual_count = report.actual.len();
    for (index, location) in returned.iter().enumerate() {
        let classified = (index < actual_count)
            .then(|| classified_extras.get(location).copied())
            .flatten();
        let class = if matches.returned_required[index] {
            ReturnedLocationClass::Required
        } else if matches_any_expected(
            location,
            report.allowed_extra.iter().chain(&report.allowed_unproven),
        ) || classified
            .is_some_and(|extra| extra.disposition == ExtraUsageDisposition::AllowedPolicyExtra)
        {
            ReturnedLocationClass::PolicyAllowed
        } else if matches_any_expected(location, required.iter().copied()) {
            ReturnedLocationClass::RelatedUnallowed
        } else {
            classified
                .map(|extra| match extra.classification {
                    ExtraUsageClassification::Unclassified => ReturnedLocationClass::Unrelated,
                    _ => ReturnedLocationClass::RelatedUnallowed,
                })
                .unwrap_or(ReturnedLocationClass::Unrelated)
        };
        query.record_returned(class);
    }

    query.finish()
}

fn navigation_query_metrics(
    report: &UsageDefinitionReport,
    allowed_extra_targets: &[NormalizedLocation],
) -> QueryLocationMetrics {
    let mut query = QueryLocationMetrics {
        required_locations: 1,
        ..QueryLocationMetrics::default()
    };
    let required = [&report.expected_declaration];
    let returned = report.actual_declarations.iter().collect::<Vec<_>>();
    let matches = match_required_locations(&required, &returned);
    record_range_quality(matches.required_quality[0], &mut query);
    for (index, location) in returned.iter().enumerate() {
        let class = if matches.returned_required[index] {
            ReturnedLocationClass::Required
        } else if matches_any_expected(location, allowed_extra_targets.iter()) {
            ReturnedLocationClass::PolicyAllowed
        } else {
            // Every navigation response is presented by the analyzer as a
            // declaration or definition candidate. An unauthored candidate is
            // therefore a related-but-unallowed result, even when it points at
            // the wrong symbol entirely.
            ReturnedLocationClass::RelatedUnallowed
        };
        query.record_returned(class);
    }
    query.finish()
}

#[derive(Debug)]
struct RequiredLocationMatches {
    required_quality: Vec<LocationMatch>,
    returned_required: Vec<bool>,
}

fn match_required_locations(
    required: &[&NormalizedLocation],
    returned: &[&NormalizedLocation],
) -> RequiredLocationMatches {
    let mut required_quality = vec![LocationMatch::None; required.len()];
    let mut returned_required = vec![false; returned.len()];
    if required.is_empty() {
        return RequiredLocationMatches {
            required_quality,
            returned_required,
        };
    }

    // Solve a maximum-weight assignment. The cardinality bonus is larger than
    // every possible aggregate quality difference, so the result first
    // maximizes TP count and then prefers exact, containing, and line-only
    // evidence in that order. Dummy columns represent unmatched requirements.
    let column_count = returned.len() + required.len();
    let cardinality_bonus = 3_i64 * required.len() as i64 + 1;
    let mut row_potential = vec![0_i64; required.len() + 1];
    let mut column_potential = vec![0_i64; column_count + 1];
    let mut column_owner = vec![0_usize; column_count + 1];
    let mut previous_column = vec![0_usize; column_count + 1];

    for row in 1..=required.len() {
        column_owner[0] = row;
        let mut column = 0;
        let mut minimum = vec![i64::MAX; column_count + 1];
        let mut used = vec![false; column_count + 1];
        loop {
            used[column] = true;
            let current_row = column_owner[column];
            let mut delta = i64::MAX;
            let mut next_column = 0;
            for candidate_column in 1..=column_count {
                if used[candidate_column] {
                    continue;
                }
                let weight = if candidate_column <= returned.len() {
                    let quality =
                        location_match(returned[candidate_column - 1], required[current_row - 1]);
                    (quality != LocationMatch::None)
                        .then_some(cardinality_bonus + location_match_weight(quality))
                        .unwrap_or(0)
                } else {
                    0
                };
                let reduced_cost =
                    -weight - row_potential[current_row] - column_potential[candidate_column];
                if reduced_cost < minimum[candidate_column] {
                    minimum[candidate_column] = reduced_cost;
                    previous_column[candidate_column] = column;
                }
                if minimum[candidate_column] < delta {
                    delta = minimum[candidate_column];
                    next_column = candidate_column;
                }
            }
            for candidate_column in 0..=column_count {
                if used[candidate_column] {
                    row_potential[column_owner[candidate_column]] += delta;
                    column_potential[candidate_column] -= delta;
                } else {
                    minimum[candidate_column] -= delta;
                }
            }
            column = next_column;
            if column_owner[column] == 0 {
                break;
            }
        }
        loop {
            let previous = previous_column[column];
            column_owner[column] = column_owner[previous];
            column = previous;
            if column == 0 {
                break;
            }
        }
    }

    for returned_column in 1..=returned.len() {
        let owner = column_owner[returned_column];
        if owner != 0 {
            let required_index = owner - 1;
            let quality = location_match(returned[returned_column - 1], required[required_index]);
            if quality == LocationMatch::None {
                continue;
            }
            required_quality[required_index] = quality;
            returned_required[returned_column - 1] = true;
        }
    }
    RequiredLocationMatches {
        required_quality,
        returned_required,
    }
}

fn location_match_weight(quality: LocationMatch) -> i64 {
    match quality {
        LocationMatch::None => 0,
        LocationMatch::LineOnly => 1,
        LocationMatch::Containing => 2,
        LocationMatch::Exact => 3,
    }
}

fn record_range_quality(quality: LocationMatch, query: &mut QueryLocationMetrics) {
    match quality {
        LocationMatch::Exact => {
            query.true_positives += 1;
            query.range_quality.exact_token += 1;
        }
        LocationMatch::Containing => {
            query.true_positives += 1;
            query.range_quality.containing += 1;
        }
        LocationMatch::LineOnly => {
            query.true_positives += 1;
            query.range_quality.line_only += 1;
        }
        LocationMatch::None => {
            query.false_negatives += 1;
            query.range_quality.missing += 1;
        }
    }
}

pub(crate) fn navigation_response_status(
    actual: &[NormalizedLocation],
    expected: &NormalizedLocation,
    expect_no_movement: bool,
) -> CaseStatus {
    score_navigation_response(actual, expected, &[], expect_no_movement).0
}

pub(crate) fn score_navigation_response(
    actual: &[NormalizedLocation],
    expected: &NormalizedLocation,
    allowed_extra_targets: &[NormalizedLocation],
    expect_no_movement: bool,
) -> (CaseStatus, &'static str) {
    if actual.is_empty() {
        return if expect_no_movement {
            (CaseStatus::Passed, "no_movement")
        } else {
            (CaseStatus::Failed, "no_definition")
        };
    }

    if expect_no_movement {
        if actual.len() != 1 {
            return (CaseStatus::Failed, "multiple_targets");
        }
        return match location_match(&actual[0], expected) {
            LocationMatch::Exact => (CaseStatus::Passed, "self_target"),
            LocationMatch::LineOnly | LocationMatch::Containing => {
                (CaseStatus::PositionUnverified, "position_unverified")
            }
            LocationMatch::None => (CaseStatus::Failed, "wrong_target"),
        };
    }

    let expected_matches = actual
        .iter()
        .map(|location| location_match(location, expected))
        .filter(|result| *result != LocationMatch::None)
        .collect::<Vec<_>>();
    if expected_matches.is_empty() {
        return (CaseStatus::Failed, "wrong_target");
    }
    if expected_matches.len() != 1 {
        return (CaseStatus::Failed, "multiple_targets");
    }

    let mut position_unverified = matches!(
        expected_matches[0],
        LocationMatch::LineOnly | LocationMatch::Containing
    );
    for location in actual {
        if location_match(location, expected) != LocationMatch::None {
            continue;
        }
        let extra_match = allowed_extra_targets
            .iter()
            .map(|target| location_match(location, target))
            .max()
            .unwrap_or(LocationMatch::None);
        match extra_match {
            LocationMatch::Exact => {}
            LocationMatch::LineOnly | LocationMatch::Containing => position_unverified = true,
            LocationMatch::None => return (CaseStatus::Failed, "multiple_targets"),
        }
    }

    if position_unverified {
        (CaseStatus::PositionUnverified, "position_unverified")
    } else if actual.len() > 1 {
        (CaseStatus::Passed, "ok_with_allowed_extra_targets")
    } else {
        (CaseStatus::Passed, "ok")
    }
}

pub(crate) fn combine_case_status(
    declaration: Option<&DeclarationUsageReport>,
    definitions: &[UsageDefinitionReport],
    types: &[TypeLookupReport],
) -> CaseStatus {
    combine_case_status_from_statuses(
        declaration
            .into_iter()
            .map(|report| report.status)
            .chain(definitions.iter().map(|report| report.status)),
        types.iter().map(|report| report.status),
    )
}

pub(crate) fn required_destination_status(
    declaration: Option<&DeclarationUsageReport>,
    canonical_definitions: &[UsageDefinitionReport],
    compatible_definitions: &[CompatibleUsageDefinitionReport],
    types: &[TypeLookupReport],
) -> RequiredDestinationStatus {
    let mut components = Vec::new();
    if let Some(report) = declaration {
        components.push(declaration_destination_status(report));
    }
    for (index, canonical) in canonical_definitions.iter().enumerate() {
        components.push(best_navigation_destination_status(
            navigation_reports_for_lookup(index, canonical, compatible_definitions),
        ));
    }
    components.extend(types.iter().map(type_destination_status));
    combine_required_destination_statuses(components)
}

fn navigation_reports_for_lookup<'a>(
    index: usize,
    canonical: &'a UsageDefinitionReport,
    compatible_definitions: &'a [CompatibleUsageDefinitionReport],
) -> impl Iterator<Item = &'a UsageDefinitionReport> {
    std::iter::once(canonical).chain(
        compatible_definitions
            .iter()
            .filter(move |compatible| compatible.usage_lookup_index == index)
            .flat_map(|compatible| compatible.reports.iter()),
    )
}

fn declaration_destination_status(report: &DeclarationUsageReport) -> RequiredDestinationStatus {
    match report.status {
        CaseStatus::Error => RequiredDestinationStatus::Error,
        CaseStatus::Unsupported => RequiredDestinationStatus::Unsupported,
        CaseStatus::Skipped => RequiredDestinationStatus::Skipped,
        _ if !report.missing.is_empty() || !report.missing_unproven.is_empty() => {
            RequiredDestinationStatus::Missing
        }
        _ => RequiredDestinationStatus::Found,
    }
}

fn best_navigation_destination_status<'a>(
    reports: impl IntoIterator<Item = &'a UsageDefinitionReport>,
) -> RequiredDestinationStatus {
    let statuses = reports
        .into_iter()
        .map(navigation_destination_status)
        .collect::<Vec<_>>();
    if statuses.contains(&RequiredDestinationStatus::Found) {
        RequiredDestinationStatus::Found
    } else if statuses.contains(&RequiredDestinationStatus::Missing) {
        RequiredDestinationStatus::Missing
    } else if statuses.contains(&RequiredDestinationStatus::Error) {
        RequiredDestinationStatus::Error
    } else if statuses.contains(&RequiredDestinationStatus::Skipped) {
        RequiredDestinationStatus::Skipped
    } else {
        RequiredDestinationStatus::Unsupported
    }
}

fn navigation_destination_status(report: &UsageDefinitionReport) -> RequiredDestinationStatus {
    match report.status {
        CaseStatus::Error => RequiredDestinationStatus::Error,
        CaseStatus::Unsupported => RequiredDestinationStatus::Unsupported,
        CaseStatus::Skipped => RequiredDestinationStatus::Skipped,
        CaseStatus::Passed | CaseStatus::PositionUnverified => RequiredDestinationStatus::Found,
        _ if best_location_match(&report.actual_declarations, &report.expected_declaration)
            != LocationMatch::None =>
        {
            RequiredDestinationStatus::Found
        }
        _ => RequiredDestinationStatus::Missing,
    }
}

fn type_destination_status(report: &TypeLookupReport) -> RequiredDestinationStatus {
    match report.status {
        CaseStatus::Error => RequiredDestinationStatus::Error,
        CaseStatus::Unsupported => RequiredDestinationStatus::Unsupported,
        CaseStatus::Skipped => RequiredDestinationStatus::Skipped,
        CaseStatus::Passed | CaseStatus::PositionUnverified => RequiredDestinationStatus::Found,
        _ if best_location_match(&report.actual_types, &report.expected_type)
            != LocationMatch::None =>
        {
            RequiredDestinationStatus::Found
        }
        _ => RequiredDestinationStatus::Missing,
    }
}

fn combine_required_destination_statuses(
    statuses: impl IntoIterator<Item = RequiredDestinationStatus>,
) -> RequiredDestinationStatus {
    let statuses = statuses.into_iter().collect::<Vec<_>>();
    if statuses.contains(&RequiredDestinationStatus::Error) {
        RequiredDestinationStatus::Error
    } else if statuses.contains(&RequiredDestinationStatus::Skipped) {
        RequiredDestinationStatus::Skipped
    } else if statuses.contains(&RequiredDestinationStatus::Unsupported) {
        RequiredDestinationStatus::Unsupported
    } else if statuses.contains(&RequiredDestinationStatus::Missing) {
        RequiredDestinationStatus::Missing
    } else {
        RequiredDestinationStatus::Found
    }
}

fn combine_case_status_from_statuses(
    navigation_statuses: impl IntoIterator<Item = CaseStatus>,
    type_statuses: impl IntoIterator<Item = CaseStatus>,
) -> CaseStatus {
    let statuses = navigation_statuses
        .into_iter()
        .chain(type_statuses)
        .collect::<Vec<_>>();
    if statuses.contains(&CaseStatus::Error) {
        CaseStatus::Error
    } else if statuses.contains(&CaseStatus::Failed) {
        CaseStatus::Failed
    } else if statuses.contains(&CaseStatus::Unsupported) {
        CaseStatus::Unsupported
    } else if statuses.contains(&CaseStatus::PositionUnverified) {
        CaseStatus::PositionUnverified
    } else if statuses.contains(&CaseStatus::NearMiss) {
        CaseStatus::NearMiss
    } else if statuses.is_empty() || statuses.iter().all(|status| *status == CaseStatus::Skipped) {
        CaseStatus::Skipped
    } else {
        CaseStatus::Passed
    }
}

fn best_location_match(
    actual: &[NormalizedLocation],
    expected: &NormalizedLocation,
) -> LocationMatch {
    best_location_match_iter(actual.iter(), expected)
}

fn best_location_match_iter<'a>(
    actual: impl Iterator<Item = &'a NormalizedLocation>,
    expected: &NormalizedLocation,
) -> LocationMatch {
    actual
        .map(|location| location_match(location, expected))
        .max_by_key(|quality| match quality {
            LocationMatch::None => 0,
            LocationMatch::LineOnly => 1,
            LocationMatch::Containing => 2,
            LocationMatch::Exact => 3,
        })
        .unwrap_or(LocationMatch::None)
}

fn matches_any_expected<'a>(
    actual: &NormalizedLocation,
    mut expected: impl Iterator<Item = &'a NormalizedLocation>,
) -> bool {
    expected.any(|location| location_match(actual, location) != LocationMatch::None)
}

pub(crate) fn score_declaration_locations(
    case: &BenchmarkCase,
    selector: Option<String>,
    actual: Vec<NormalizedLocation>,
    unproven: Vec<NormalizedLocation>,
    partial: bool,
    raw_statuses: Vec<String>,
    adapter_failed: bool,
) -> Result<DeclarationUsageReport> {
    let expected = case
        .expected_usages
        .iter()
        .map(normalize_symbol_location)
        .collect::<Result<Vec<_>>>()?;
    let expected_unproven = case
        .expected_unproven_usages
        .iter()
        .map(normalize_symbol_location)
        .collect::<Result<Vec<_>>>()?;
    let allowed_extra = case
        .allowed_extra_usages
        .iter()
        .map(normalize_symbol_location)
        .collect::<Result<Vec<_>>>()?;
    let allowed_unproven = case
        .allowed_unproven_usages
        .iter()
        .map(normalize_symbol_location)
        .collect::<Result<Vec<_>>>()?;
    let missing = expected
        .iter()
        .filter(|location| best_location_match(&actual, location) == LocationMatch::None)
        .cloned()
        .collect::<Vec<_>>();
    let missing_unproven = expected_unproven
        .iter()
        .filter(|location| {
            best_location_match(&actual, location) == LocationMatch::None
                && best_location_match(&unproven, location) == LocationMatch::None
        })
        .cloned()
        .collect::<Vec<_>>();
    let position_unverified = expected
        .iter()
        .filter(|location| {
            matches!(
                best_location_match(&actual, location),
                LocationMatch::LineOnly | LocationMatch::Containing
            )
        })
        .chain(expected_unproven.iter().filter(|location| {
            matches!(
                best_location_match(&actual, location),
                LocationMatch::LineOnly | LocationMatch::Containing
            ) || matches!(
                best_location_match(&unproven, location),
                LocationMatch::LineOnly | LocationMatch::Containing
            )
        }))
        .cloned()
        .collect::<Vec<_>>();
    let unexpected = actual
        .iter()
        .filter(|location| {
            !matches_any_expected(
                location,
                expected
                    .iter()
                    .chain(&expected_unproven)
                    .chain(&allowed_extra),
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    let unexpected_unproven = unproven
        .iter()
        .filter(|location| {
            !matches_any_expected(location, expected_unproven.iter().chain(&allowed_unproven))
        })
        .cloned()
        .collect::<Vec<_>>();
    let status = if adapter_failed
        || partial
        || !missing.is_empty()
        || !missing_unproven.is_empty()
        || !unexpected.is_empty()
        || !unexpected_unproven.is_empty()
    {
        CaseStatus::Failed
    } else if !position_unverified.is_empty() {
        CaseStatus::PositionUnverified
    } else {
        CaseStatus::Passed
    };

    Ok(DeclarationUsageReport {
        status,
        selector,
        expected,
        expected_unproven,
        allowed_extra,
        allowed_unproven,
        actual,
        unproven,
        missing,
        missing_unproven,
        unexpected,
        unexpected_unproven,
        position_unverified,
        extra_usages: Vec::new(),
        partial,
        raw_statuses,
    })
}

pub(crate) fn normalize_symbol_location(symbol: &SymbolLocation) -> Result<NormalizedLocation> {
    let path = benchmark_source_path(&symbol.location.uri)?;
    Ok(NormalizedLocation {
        path: path_to_slash(&path),
        line: symbol.location.range.start.line + 1,
        column: Some(symbol.location.range.start.character + 1),
        end_line: Some(symbol.location.range.end.line + 1),
        end_column: Some(symbol.location.range.end.character + 1),
        display_name: Some(symbol.display_name.clone()),
        kind: Some(symbol_kind_name(&symbol.kind).to_string()),
    })
}

pub(crate) fn symbol_kind_name(kind: &SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Class => "class",
        SymbolKind::Constructor => "constructor",
        SymbolKind::Method => "method",
        SymbolKind::Function => "function",
        SymbolKind::Field => "field",
        SymbolKind::Variable => "variable",
        SymbolKind::Constant => "constant",
        SymbolKind::Module => "module",
        SymbolKind::Package => "package",
        SymbolKind::Interface => "interface",
        SymbolKind::Type => "type",
        SymbolKind::Property => "property",
    }
}

pub(crate) fn path_to_slash(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

pub(crate) fn compute_totals(documents: &[DocumentRunReport]) -> RunTotals {
    let mut totals = RunTotals {
        documents: documents.len(),
        location_metrics: Some(LocationMetrics::default()),
        ..RunTotals::default()
    };
    for document in documents {
        for case in &document.cases {
            if !matches!(
                case.status,
                CaseStatus::NotPlanned | CaseStatus::Unsupported | CaseStatus::Skipped
            ) {
                totals.cases += 1;
                match document.corpus_partition {
                    CorpusPartition::Development => totals.development_cases += 1,
                    CorpusPartition::Evaluation => totals.evaluation_cases += 1,
                }
            }
            match case.status {
                CaseStatus::Passed => totals.passed += 1,
                CaseStatus::NearMiss => totals.near_misses += 1,
                CaseStatus::PositionUnverified => totals.position_unverified += 1,
                CaseStatus::Improved => totals.improved += 1,
                CaseStatus::Failed => totals.failed += 1,
                CaseStatus::ExpectedFailure => totals.expected_failures += 1,
                CaseStatus::NotPlanned => totals.not_planned += 1,
                CaseStatus::Unsupported => totals.unsupported += 1,
                CaseStatus::Skipped => totals.skipped += 1,
                CaseStatus::Error => totals.errors += 1,
            }
            match case.required_destination_status {
                Some(RequiredDestinationStatus::Found) => {
                    totals.required_destinations.scoreable_cases += 1;
                    totals.required_destinations.found += 1;
                }
                Some(RequiredDestinationStatus::Missing) => {
                    totals.required_destinations.scoreable_cases += 1;
                    totals.required_destinations.missing += 1;
                }
                Some(RequiredDestinationStatus::NotPlanned) => {
                    totals.required_destinations.not_planned += 1
                }
                Some(RequiredDestinationStatus::Unsupported) => {
                    totals.required_destinations.unsupported += 1
                }
                Some(RequiredDestinationStatus::Skipped) => {
                    totals.required_destinations.skipped += 1
                }
                Some(RequiredDestinationStatus::Error) => totals.required_destinations.errors += 1,
                None => totals.required_destinations.unreported += 1,
            }
            if let Some(metrics) = &case.location_metrics {
                totals
                    .location_metrics
                    .as_mut()
                    .expect("newly computed totals include location metrics")
                    .merge(metrics);
            }
        }
    }
    totals
}

#[cfg(test)]
mod location_metric_tests {
    use super::*;

    #[test]
    fn reference_metrics_keep_return_classes_and_range_quality_separate() {
        let exact = location("src/lib.rs", 1, Some((3, 7)));
        let missing = location("src/lib.rs", 2, Some((3, 7)));
        let contained = location("src/lib.rs", 3, Some((3, 7)));
        let line_only = location("src/lib.rs", 4, Some((3, 7)));
        let containing_result = location("src/lib.rs", 3, Some((1, 12)));
        let line_only_result = location("src/lib.rs", 4, None);
        let explicit_allowed = location("src/lib.rs", 5, Some((3, 7)));
        let allowed_binding = location("src/lib.rs", 6, Some((3, 7)));
        let related_unallowed = location("src/lib.rs", 7, Some((3, 7)));
        let unrelated = location("src/lib.rs", 8, Some((3, 7)));
        let report = DeclarationUsageReport {
            status: CaseStatus::Failed,
            selector: None,
            expected: vec![exact.clone(), missing, contained, line_only],
            expected_unproven: Vec::new(),
            allowed_extra: vec![explicit_allowed.clone()],
            allowed_unproven: Vec::new(),
            actual: vec![
                exact,
                containing_result,
                line_only_result,
                explicit_allowed,
                allowed_binding.clone(),
                related_unallowed.clone(),
                unrelated.clone(),
            ],
            unproven: Vec::new(),
            missing: Vec::new(),
            missing_unproven: Vec::new(),
            unexpected: vec![related_unallowed.clone(), unrelated.clone()],
            unexpected_unproven: Vec::new(),
            position_unverified: Vec::new(),
            extra_usages: vec![
                ClassifiedExtraUsage {
                    location: allowed_binding,
                    classification: ExtraUsageClassification::ImportBinding,
                    disposition: ExtraUsageDisposition::AllowedPolicyExtra,
                    rationale: "optional binding".to_string(),
                },
                ClassifiedExtraUsage {
                    location: related_unallowed,
                    classification: ExtraUsageClassification::ImportBinding,
                    disposition: ExtraUsageDisposition::Unexpected,
                    rationale: "binding excluded by policy".to_string(),
                },
                ClassifiedExtraUsage {
                    location: unrelated,
                    classification: ExtraUsageClassification::Unclassified,
                    disposition: ExtraUsageDisposition::Unexpected,
                    rationale: "not a recognized related location".to_string(),
                },
            ],
            partial: false,
            raw_statuses: Vec::new(),
        };

        let metrics = reference_query_metrics(&report);

        assert_eq!(metrics.required_locations, 4);
        assert_eq!(metrics.true_positives, 3);
        assert_eq!(metrics.false_negatives, 1);
        assert_eq!(metrics.returned_locations.required, 3);
        assert_eq!(metrics.returned_locations.policy_allowed, 2);
        assert_eq!(metrics.returned_locations.related_unallowed, 1);
        assert_eq!(metrics.returned_locations.unrelated, 1);
        assert_eq!(metrics.range_quality.exact_token, 1);
        assert_eq!(metrics.range_quality.containing, 1);
        assert_eq!(metrics.range_quality.line_only, 1);
        assert_eq!(metrics.range_quality.wrong_location, 2);
        assert_eq!(metrics.range_quality.missing, 1);
        assert!(!metrics.exact_set());

        let mut aggregate = LocationMetrics::default();
        aggregate.record_query(metrics);
        assert_eq!(aggregate.false_positives, 2);
        assert_eq!(aggregate.returned_locations.policy_allowed, 2);
    }

    #[test]
    fn navigation_metrics_count_authored_extras_without_weakening_range_quality() {
        let expected = location("src/model.py", 5, Some((5, 11)));
        let containing = location("src/model.py", 5, Some((1, 20)));
        let allowed = location("src/model.py", 8, Some((5, 11)));
        let wrong = location("src/other.py", 2, Some((1, 4)));
        let report = UsageDefinitionReport {
            status: CaseStatus::Failed,
            operation: NavigationOperation::Definition,
            usage: location("src/use.py", 1, Some((1, 7))),
            expected_declaration: expected,
            actual_declarations: vec![containing, allowed.clone(), wrong],
            raw_status: "multiple_targets".to_string(),
            diagnostics: Vec::new(),
        };

        let metrics = navigation_query_metrics(&report, &[allowed]);

        assert_eq!(metrics.true_positives, 1);
        assert_eq!(metrics.false_negatives, 0);
        assert_eq!(metrics.returned_locations.required, 1);
        assert_eq!(metrics.returned_locations.policy_allowed, 1);
        assert_eq!(metrics.returned_locations.related_unallowed, 1);
        assert_eq!(metrics.returned_locations.unrelated, 0);
        assert_eq!(metrics.range_quality.containing, 1);
        assert_eq!(metrics.range_quality.wrong_location, 1);
        assert!(!metrics.exact_set());
    }

    #[test]
    fn one_broad_return_cannot_satisfy_two_required_locations() {
        let first = location("src/service.cs", 10, Some((3, 10)));
        let second = location("src/service.cs", 10, Some((20, 27)));
        for returned in [
            location("src/service.cs", 10, None),
            location("src/service.cs", 10, Some((1, 30))),
        ] {
            let report = DeclarationUsageReport {
                status: CaseStatus::Failed,
                selector: None,
                expected: vec![first.clone(), second.clone()],
                expected_unproven: Vec::new(),
                allowed_extra: Vec::new(),
                allowed_unproven: Vec::new(),
                actual: vec![returned],
                unproven: Vec::new(),
                missing: Vec::new(),
                missing_unproven: Vec::new(),
                unexpected: Vec::new(),
                unexpected_unproven: Vec::new(),
                position_unverified: Vec::new(),
                extra_usages: Vec::new(),
                partial: false,
                raw_statuses: Vec::new(),
            };

            let metrics = reference_query_metrics(&report);

            assert_eq!(metrics.required_locations, 2);
            assert_eq!(metrics.true_positives, 1);
            assert_eq!(metrics.false_negatives, 1);
            assert_eq!(metrics.returned_locations.required, 1);
            assert_eq!(metrics.range_quality.missing, 1);
        }
    }

    #[test]
    fn one_to_one_matching_preserves_exact_evidence_before_broader_matches() {
        let outer = location("src/lib.rs", 4, Some((3, 10)));
        let inner = location("src/lib.rs", 4, Some((5, 7)));
        let exact_outer = outer.clone();
        let line_only = location("src/lib.rs", 4, None);
        let required = [&outer, &inner];
        let returned = [&exact_outer, &line_only];

        let matches = match_required_locations(&required, &returned);

        assert_eq!(
            matches.required_quality,
            vec![LocationMatch::Exact, LocationMatch::LineOnly]
        );
        assert_eq!(matches.returned_required, vec![true, true]);
    }

    #[test]
    fn report_schema_keeps_location_metrics_optional_for_legacy_reports() {
        let schema: serde_json::Value =
            serde_json::from_str(&generated_report_schema_json().unwrap()).unwrap();

        for definition in ["CaseRunReport", "RunTotals"] {
            let contract = &schema["definitions"][definition];
            assert!(contract["properties"]["locationMetrics"].is_object());
            assert!(!contract["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field.as_str() == Some("locationMetrics")));
        }
    }

    fn location(path: &str, line: u32, columns: Option<(u32, u32)>) -> NormalizedLocation {
        NormalizedLocation {
            path: path.to_string(),
            line,
            column: columns.map(|(start, _)| start),
            end_line: columns.map(|_| line),
            end_column: columns.map(|(_, end)| end),
            display_name: None,
            kind: None,
        }
    }
}
