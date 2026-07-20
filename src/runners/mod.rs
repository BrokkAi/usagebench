//! Analyzer adapters and shared benchmark-runner contracts.
//!
//! Each adapter is responsible for preparing an exact tool version and
//! translating that tool's public query surface into UsageBench locations.

use crate::{
    benchmark_source_path, BenchmarkCase, CorpusPartition, CorpusSelection,
    GroundTruthReviewStatus, ReferencePolicy, SymbolKind, SymbolLocation,
};
use anyhow::{bail, Context, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path, process::Command};

pub mod bifrost;
pub mod lsp;
mod lsp_protocol;
mod mcp;

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunnerMetadata {
    pub name: String,
    pub requested_version: String,
    pub resolved_version: String,
    pub source: String,
    pub adapter_version: String,
    pub capabilities: Vec<RunnerCapability>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunnerCapability {
    pub operation: RunnerOperation,
    pub support: CapabilitySupport,
    pub notes: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunnerOperation {
    DeclarationToUsages,
    UsageToDeclaration,
    TypeLookup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CapabilitySupport {
    Native,
    Recovered,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
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

#[derive(Debug, Clone, Default, Serialize, JsonSchema)]
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
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CaseRunReport {
    pub id: String,
    pub status: CaseStatus,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub type_lookups: Vec<TypeLookupReport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<RunDiagnostic>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClassifiedExtraUsage {
    pub location: NormalizedLocation,
    pub classification: ExtraUsageClassification,
    pub disposition: ExtraUsageDisposition,
    pub rationale: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExtraUsageClassification {
    ImportBinding,
    ReexportBinding,
    ExportMetadata,
    DeclarationOrDefinition,
    Unclassified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExtraUsageDisposition {
    AllowedPolicyExtra,
    Unexpected,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UsageDefinitionReport {
    pub status: CaseStatus,
    pub usage: NormalizedLocation,
    pub expected_declaration: NormalizedLocation,
    pub actual_declarations: Vec<NormalizedLocation>,
    pub raw_status: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<RunDiagnostic>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, JsonSchema)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LocationMatch {
    None,
    LineOnly,
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
        (None, _, _, _, _, _) | (_, None, None, _, _, _) => LocationMatch::LineOnly,
        _ => LocationMatch::None,
    }
}

pub(crate) fn navigation_response_status(
    actual: &[NormalizedLocation],
    expected: &NormalizedLocation,
    expect_no_movement: bool,
) -> CaseStatus {
    if expect_no_movement && actual.is_empty() {
        return CaseStatus::Passed;
    }
    if actual.len() != 1 {
        return CaseStatus::Failed;
    }
    match location_match(&actual[0], expected) {
        LocationMatch::Exact => CaseStatus::Passed,
        LocationMatch::LineOnly => CaseStatus::PositionUnverified,
        LocationMatch::None => CaseStatus::Failed,
    }
}

fn best_location_match(
    actual: &[NormalizedLocation],
    expected: &NormalizedLocation,
) -> LocationMatch {
    actual
        .iter()
        .map(|location| location_match(location, expected))
        .max_by_key(|quality| match quality {
            LocationMatch::None => 0,
            LocationMatch::LineOnly => 1,
            LocationMatch::Exact => 2,
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
        .filter(|location| best_location_match(&actual, location) == LocationMatch::LineOnly)
        .chain(expected_unproven.iter().filter(|location| {
            best_location_match(&actual, location) == LocationMatch::LineOnly
                || best_location_match(&unproven, location) == LocationMatch::LineOnly
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
        }
    }
    totals
}
