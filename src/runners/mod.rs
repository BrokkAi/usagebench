//! Analyzer adapters and shared benchmark-runner contracts.
//!
//! Each adapter is responsible for preparing an exact tool version and
//! translating that tool's public query surface into UsageBench locations.

use crate::{benchmark_source_path, BenchmarkCase, SymbolKind, SymbolLocation};
use anyhow::{Context, Result};
use schemars::JsonSchema;
use serde::Serialize;
use std::{collections::BTreeSet, path::Path};

pub mod bifrost;
pub mod lsp;
mod lsp_protocol;
mod mcp;
pub mod repowise;

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
    pub usagebench_version: String,
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

#[derive(Debug, Clone, Default, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunTotals {
    pub documents: usize,
    pub cases: usize,
    pub passed: usize,
    pub near_misses: usize,
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
    pub cases: Vec<CaseRunReport>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CaseStatus {
    Passed,
    NearMiss,
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
    pub partial: bool,
    pub raw_statuses: Vec<String>,
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct LocationLine {
    path: String,
    line: u32,
}

impl From<&NormalizedLocation> for LocationLine {
    fn from(location: &NormalizedLocation) -> Self {
        Self {
            path: location.path.clone(),
            line: location.line,
        }
    }
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
    let expected_keys = expected
        .iter()
        .map(LocationLine::from)
        .collect::<BTreeSet<_>>();
    let expected_unproven_keys = expected_unproven
        .iter()
        .map(LocationLine::from)
        .collect::<BTreeSet<_>>();
    let allowed_keys = allowed_extra
        .iter()
        .map(LocationLine::from)
        .collect::<BTreeSet<_>>();
    let allowed_unproven_keys = allowed_unproven
        .iter()
        .map(LocationLine::from)
        .collect::<BTreeSet<_>>();
    let actual_keys = actual
        .iter()
        .map(LocationLine::from)
        .collect::<BTreeSet<_>>();
    let all_actual_keys = actual
        .iter()
        .chain(&unproven)
        .map(LocationLine::from)
        .collect::<BTreeSet<_>>();
    let missing = expected
        .iter()
        .filter(|location| !actual_keys.contains(&LocationLine::from(*location)))
        .cloned()
        .collect::<Vec<_>>();
    let missing_unproven = expected_unproven
        .iter()
        .filter(|location| !all_actual_keys.contains(&LocationLine::from(*location)))
        .cloned()
        .collect::<Vec<_>>();
    let unexpected = actual
        .iter()
        .filter(|location| {
            let key = LocationLine::from(*location);
            !expected_keys.contains(&key)
                && !expected_unproven_keys.contains(&key)
                && !allowed_keys.contains(&key)
        })
        .cloned()
        .collect::<Vec<_>>();
    let unexpected_unproven = unproven
        .iter()
        .filter(|location| {
            let key = LocationLine::from(*location);
            !expected_unproven_keys.contains(&key) && !allowed_unproven_keys.contains(&key)
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
    for case in documents.iter().flat_map(|document| &document.cases) {
        if !matches!(
            case.status,
            CaseStatus::NotPlanned | CaseStatus::Unsupported | CaseStatus::Skipped
        ) {
            totals.cases += 1;
        }
        match case.status {
            CaseStatus::Passed => totals.passed += 1,
            CaseStatus::NearMiss => totals.near_misses += 1,
            CaseStatus::Improved => totals.improved += 1,
            CaseStatus::Failed => totals.failed += 1,
            CaseStatus::ExpectedFailure => totals.expected_failures += 1,
            CaseStatus::NotPlanned => totals.not_planned += 1,
            CaseStatus::Unsupported => totals.unsupported += 1,
            CaseStatus::Skipped => totals.skipped += 1,
            CaseStatus::Error => totals.errors += 1,
        }
    }
    totals
}
