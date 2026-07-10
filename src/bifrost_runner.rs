use crate::{
    benchmark_source_path, find_repo_root_for_path, BenchmarkCase, BenchmarkDocument, Location,
    PositionEncoding, Source, SymbolKind, SymbolLocation,
};
use anyhow::{anyhow, bail, Context, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{hash_map::DefaultHasher, BTreeSet},
    fs,
    hash::{Hash, Hasher},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use url::{Host, Url};

const DEFAULT_BIFROST_COMMIT: &str = "origin/master";
const GET_DEFINITIONS_BY_LOCATION_TOOL: &str = "get_definitions_by_location";
const GET_TYPE_BY_LOCATION_TOOL: &str = "get_type_by_location";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const MCP_REQUEST_TIMEOUT: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Clone)]
pub struct RunBifrostOptions {
    pub case_path: PathBuf,
    pub bifrost_repo: Option<PathBuf>,
    pub bifrost_commit: String,
    pub bifrost_working_tree: bool,
    pub work_dir: PathBuf,
    pub output: Option<PathBuf>,
    pub include_unsupported: bool,
    pub include_definition_lookups: bool,
    pub keep_worktrees: bool,
}

impl RunBifrostOptions {
    pub fn with_defaults(case_path: PathBuf) -> Self {
        Self {
            case_path,
            bifrost_repo: None,
            bifrost_commit: DEFAULT_BIFROST_COMMIT.to_string(),
            bifrost_working_tree: false,
            work_dir: PathBuf::from("target/usagebench"),
            output: None,
            include_unsupported: false,
            include_definition_lookups: true,
            keep_worktrees: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BifrostRunReport {
    pub usagebench_version: String,
    pub bifrost_repo: String,
    pub bifrost_commit: String,
    pub bifrost_resolved_commit: String,
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
    pub allowed_extra: Vec<NormalizedLocation>,
    pub actual: Vec<NormalizedLocation>,
    pub missing: Vec<NormalizedLocation>,
    pub unexpected: Vec<NormalizedLocation>,
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

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunDiagnostic {
    pub kind: String,
    pub message: String,
}

pub fn generated_bifrost_report_schema_json() -> Result<String> {
    let schema = schemars::schema_for!(BifrostRunReport);
    serde_json::to_string_pretty(&schema).context("serialize generated Bifrost report schema")
}

pub fn default_bifrost_repo(repo_root: &Path) -> Option<PathBuf> {
    repo_root
        .parent()
        .map(|parent| parent.join("bifrost"))
        .filter(|path| path.is_dir())
}

pub fn run_bifrost(options: RunBifrostOptions) -> Result<BifrostRunReport> {
    let started_at = unix_seconds_now()?;
    let repo_root = find_repo_root_for_path(&options.case_path)?;
    let case_files = crate::validate_path(&options.case_path)?;

    let work_dir = if options.work_dir.is_absolute() {
        options.work_dir.clone()
    } else {
        repo_root.join(&options.work_dir)
    };
    fs::create_dir_all(&work_dir).with_context(|| format!("create {}", work_dir.display()))?;
    let _source_cleanup = CleanupGuard::new(work_dir.join("sources"), !options.keep_worktrees);

    let bifrost_source_repo =
        resolve_bifrost_source_repo(&repo_root, options.bifrost_repo.as_ref())?;
    let bifrost_checkout = if options.bifrost_working_tree {
        bifrost_source_repo.clone()
    } else {
        prepare_bifrost_checkout(&bifrost_source_repo, &options.bifrost_commit, &work_dir)?
    };
    let bifrost_resolved_commit = git_output(&bifrost_checkout, ["rev-parse", "HEAD"])?;
    build_bifrost(&bifrost_checkout)?;
    let bifrost_binary = bifrost_binary_path(&bifrost_checkout);

    let mut documents = Vec::new();
    for case_file in &case_files {
        let yaml = fs::read_to_string(case_file)
            .with_context(|| format!("read benchmark cases {}", case_file.display()))?;
        let document: BenchmarkDocument = serde_yaml::from_str(&yaml)
            .with_context(|| format!("deserialize benchmark cases {}", case_file.display()))?;
        let source_root = prepare_source_root(&document.source, &repo_root, &work_dir)?;
        let cases = run_document_cases(
            &document,
            &source_root,
            &bifrost_binary,
            options.include_unsupported,
            options.include_definition_lookups,
        )
        .with_context(|| format!("run benchmark cases {}", case_file.display()))?;
        documents.push(DocumentRunReport {
            case_file: display_path(case_file),
            language: document.language,
            source_root: display_path(&source_root),
            cases,
        });
    }

    let finished_at = unix_seconds_now()?;
    let mut report = BifrostRunReport {
        usagebench_version: env!("CARGO_PKG_VERSION").to_string(),
        bifrost_repo: display_path(&bifrost_source_repo),
        bifrost_commit: options.bifrost_commit,
        bifrost_resolved_commit,
        started_at_unix_seconds: started_at,
        finished_at_unix_seconds: finished_at,
        case_files: case_files.iter().map(|path| display_path(path)).collect(),
        totals: RunTotals::default(),
        documents,
    };
    report.totals = compute_totals(&report.documents);

    if let Some(output) = &options.output {
        let output = if output.is_absolute() {
            output.clone()
        } else {
            repo_root.join(output)
        };
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        fs::write(&output, serde_json::to_vec_pretty(&report)?)
            .with_context(|| format!("write {}", output.display()))?;
    }

    Ok(report)
}

fn run_document_cases(
    document: &BenchmarkDocument,
    source_root: &Path,
    bifrost_binary: &Path,
    include_unsupported: bool,
    include_definition_lookups: bool,
) -> Result<Vec<CaseRunReport>> {
    let mut session = McpSession::start(bifrost_binary, source_root)?;
    session.initialize()?;

    let mut reports = Vec::new();
    for case in &document.cases {
        reports.push(run_case(
            case,
            document.position_encoding,
            &mut session,
            include_unsupported,
            include_definition_lookups,
        ));
    }
    Ok(reports)
}

fn run_case(
    case: &BenchmarkCase,
    encoding: PositionEncoding,
    session: &mut impl SearchToolsClient,
    include_unsupported: bool,
    include_definition_lookups: bool,
) -> CaseRunReport {
    if let Some(unsupported) = &case.unsupported {
        if !include_unsupported {
            return CaseRunReport {
                id: case.id.clone(),
                status: CaseStatus::Unsupported,
                expected_failure_reason: case
                    .expected_failure
                    .as_ref()
                    .map(|expected_failure| expected_failure.reason.clone()),
                not_planned_reason: case
                    .not_planned
                    .as_ref()
                    .map(|not_planned| not_planned.reason.clone()),
                unsupported_reason: Some(unsupported.reason.clone()),
                declaration_to_usages: None,
                usage_to_declaration: Vec::new(),
                type_lookups: Vec::new(),
                diagnostics: Vec::new(),
            };
        }
    }

    let mut diagnostics = Vec::new();
    let declaration_to_usages = case.declaration.as_ref().map(|declaration| {
        run_declaration_to_usages(case, declaration, encoding, session, &mut diagnostics)
    });
    let usage_to_declaration = if include_definition_lookups {
        case.usage_lookups
            .iter()
            .map(|lookup| run_usage_to_declaration(lookup, encoding, session))
            .collect::<Vec<_>>()
    } else {
        case.usage_lookups
            .iter()
            .map(skipped_definition_lookup)
            .collect::<Vec<_>>()
    };
    let type_lookups = if include_definition_lookups {
        case.type_lookups
            .iter()
            .map(|lookup| run_type_lookup(lookup, encoding, session))
            .collect::<Vec<_>>()
    } else {
        case.type_lookups
            .iter()
            .map(skipped_type_lookup)
            .collect::<Vec<_>>()
    };

    let observed_status = combine_case_status(
        declaration_to_usages.as_ref(),
        &usage_to_declaration,
        &type_lookups,
        &diagnostics,
    );
    let expected_failure_reason = case
        .expected_failure
        .as_ref()
        .map(|expected_failure| expected_failure.reason.clone());
    let not_planned_reason = case
        .not_planned
        .as_ref()
        .map(|not_planned| not_planned.reason.clone());
    let status = apply_case_expectation(
        observed_status,
        expected_failure_reason.as_deref(),
        not_planned_reason.as_deref(),
        &mut diagnostics,
    );
    CaseRunReport {
        id: case.id.clone(),
        status,
        expected_failure_reason,
        not_planned_reason,
        unsupported_reason: case
            .unsupported
            .as_ref()
            .map(|unsupported| unsupported.reason.clone()),
        declaration_to_usages,
        usage_to_declaration,
        type_lookups,
        diagnostics,
    }
}

fn run_declaration_to_usages(
    case: &BenchmarkCase,
    declaration: &SymbolLocation,
    encoding: PositionEncoding,
    session: &mut impl SearchToolsClient,
    diagnostics: &mut Vec<RunDiagnostic>,
) -> DeclarationUsageReport {
    let expected = case
        .expected_usages
        .iter()
        .map(normalize_symbol_location)
        .collect::<Result<Vec<_>>>();
    let allowed_extra = case
        .allowed_extra_usages
        .iter()
        .map(normalize_symbol_location)
        .collect::<Result<Vec<_>>>();
    let (expected, allowed_extra) = match (expected, allowed_extra) {
        (Ok(expected), Ok(allowed_extra)) => (expected, allowed_extra),
        (Err(error), _) | (_, Err(error)) => {
            diagnostics.push(RunDiagnostic {
                kind: "invalid_expected_location".to_string(),
                message: format!("{error:#}"),
            });
            return DeclarationUsageReport {
                status: CaseStatus::Error,
                selector: None,
                expected: Vec::new(),
                allowed_extra: Vec::new(),
                actual: Vec::new(),
                missing: Vec::new(),
                unexpected: Vec::new(),
                partial: false,
                raw_statuses: vec!["invalid_expected_location".to_string()],
            };
        }
    };

    let selector = match resolve_declaration_selector(session, declaration) {
        Ok(selector) => selector,
        Err(error) => {
            diagnostics.push(RunDiagnostic {
                kind: "symbol_resolution_failed".to_string(),
                message: format!("{error:#}"),
            });
            return DeclarationUsageReport {
                status: CaseStatus::Failed,
                selector: None,
                expected: expected.clone(),
                allowed_extra,
                actual: Vec::new(),
                missing: expected,
                unexpected: Vec::new(),
                partial: false,
                raw_statuses: vec!["symbol_resolution_failed".to_string()],
            };
        }
    };

    let target = match reference_query(&declaration.location, &declaration.display_name, encoding) {
        Ok(mut target) => {
            target
                .as_object_mut()
                .expect("reference query object")
                .remove("symbol");
            target
        }
        Err(error) => {
            diagnostics.push(RunDiagnostic {
                kind: "invalid_declaration_location".to_string(),
                message: format!("{error:#}"),
            });
            return DeclarationUsageReport {
                status: CaseStatus::Error,
                selector: Some(selector.selector),
                expected: expected.clone(),
                allowed_extra,
                actual: Vec::new(),
                missing: expected,
                unexpected: Vec::new(),
                partial: false,
                raw_statuses: vec!["invalid_declaration_location".to_string()],
            };
        }
    };

    let result = match session.call_tool(
        "scan_usages_by_location",
        json!({
            "targets": [target],
            "include_tests": true,
        }),
    ) {
        Ok(result) => result,
        Err(error) => {
            diagnostics.push(RunDiagnostic {
                kind: "scan_usages_failed".to_string(),
                message: format!("{error:#}"),
            });
            return DeclarationUsageReport {
                status: CaseStatus::Error,
                selector: Some(selector.selector),
                expected: expected.clone(),
                allowed_extra,
                actual: Vec::new(),
                missing: expected,
                unexpected: Vec::new(),
                partial: false,
                raw_statuses: vec!["scan_usages_failed".to_string()],
            };
        }
    };

    let parsed = parse_scan_usages(&result);
    let has_failure_status = parsed.has_failure_status();
    let actual = parsed.locations;
    let expected_keys = expected
        .iter()
        .map(LocationLine::from)
        .collect::<BTreeSet<_>>();
    let allowed_keys = allowed_extra
        .iter()
        .map(LocationLine::from)
        .collect::<BTreeSet<_>>();
    let actual_keys = actual
        .iter()
        .map(LocationLine::from)
        .collect::<BTreeSet<_>>();
    let missing = expected
        .iter()
        .filter(|location| !actual_keys.contains(&LocationLine::from(*location)))
        .cloned()
        .collect::<Vec<_>>();
    let unexpected = actual
        .iter()
        .filter(|location| {
            let key = LocationLine::from(*location);
            !expected_keys.contains(&key) && !allowed_keys.contains(&key)
        })
        .cloned()
        .collect::<Vec<_>>();
    let status =
        if has_failure_status || parsed.partial || !missing.is_empty() || !unexpected.is_empty() {
            CaseStatus::Failed
        } else {
            CaseStatus::Passed
        };

    DeclarationUsageReport {
        status,
        selector: Some(selector.selector),
        expected,
        allowed_extra,
        actual,
        missing,
        unexpected,
        partial: parsed.partial,
        raw_statuses: parsed.raw_statuses,
    }
}

fn run_usage_to_declaration(
    lookup: &crate::UsageLookup,
    encoding: PositionEncoding,
    session: &mut impl SearchToolsClient,
) -> UsageDefinitionReport {
    let usage = normalize_symbol_location(&lookup.usage).unwrap_or_else(|_| NormalizedLocation {
        path: "<invalid>".to_string(),
        line: 0,
        column: None,
        display_name: Some(lookup.usage.display_name.clone()),
        kind: Some(symbol_kind_name(&lookup.usage.kind).to_string()),
    });
    let expected_declaration = normalize_symbol_location(&lookup.expected_declaration)
        .unwrap_or_else(|_| NormalizedLocation {
            path: "<invalid>".to_string(),
            line: 0,
            column: None,
            display_name: Some(lookup.expected_declaration.display_name.clone()),
            kind: Some(symbol_kind_name(&lookup.expected_declaration.kind).to_string()),
        });
    let query = match reference_query(&lookup.usage.location, &lookup.usage.display_name, encoding)
    {
        Ok(query) => query,
        Err(error) => {
            return UsageDefinitionReport {
                status: CaseStatus::Error,
                usage,
                expected_declaration,
                actual_declarations: Vec::new(),
                raw_status: "invalid_reference_location".to_string(),
                diagnostics: vec![RunDiagnostic {
                    kind: "invalid_reference_location".to_string(),
                    message: format!("{error:#}"),
                }],
            };
        }
    };

    let result = match session.call_tool(
        GET_DEFINITIONS_BY_LOCATION_TOOL,
        json!({ "references": [query] }),
    ) {
        Ok(result) => result,
        Err(error) => {
            let message = format!("{error:#}");
            let (status, raw_status) =
                if message.contains("Unknown tool: get_definitions_by_location") {
                    (CaseStatus::Failed, "unsupported_tool")
                } else {
                    (CaseStatus::Error, "get_definitions_by_location_failed")
                };
            return UsageDefinitionReport {
                status,
                usage,
                expected_declaration,
                actual_declarations: Vec::new(),
                raw_status: raw_status.to_string(),
                diagnostics: vec![RunDiagnostic {
                    kind: raw_status.to_string(),
                    message,
                }],
            };
        }
    };

    let parsed = parse_get_definition(&result);
    let status = if parsed.raw_status != "resolved" {
        CaseStatus::Failed
    } else if parsed
        .actual_declarations
        .iter()
        .any(|actual| same_path_line(actual, &expected_declaration))
    {
        CaseStatus::Passed
    } else {
        CaseStatus::Failed
    };

    UsageDefinitionReport {
        status,
        usage,
        expected_declaration,
        actual_declarations: parsed.actual_declarations,
        raw_status: parsed.raw_status,
        diagnostics: parsed.diagnostics,
    }
}

fn run_type_lookup(
    lookup: &crate::TypeLookup,
    encoding: PositionEncoding,
    session: &mut impl SearchToolsClient,
) -> TypeLookupReport {
    let expression =
        normalize_symbol_location(&lookup.expression).unwrap_or_else(|_| NormalizedLocation {
            path: "<invalid>".to_string(),
            line: 0,
            column: None,
            display_name: Some(lookup.expression.display_name.clone()),
            kind: Some(symbol_kind_name(&lookup.expression.kind).to_string()),
        });
    let expected_type =
        normalize_symbol_location(&lookup.expected_type).unwrap_or_else(|_| NormalizedLocation {
            path: "<invalid>".to_string(),
            line: 0,
            column: None,
            display_name: Some(lookup.expected_type.display_name.clone()),
            kind: Some(symbol_kind_name(&lookup.expected_type.kind).to_string()),
        });
    let query = match reference_query(
        &lookup.expression.location,
        &lookup.expression.display_name,
        encoding,
    ) {
        Ok(query) => query,
        Err(error) => {
            return TypeLookupReport {
                status: CaseStatus::Error,
                expression,
                expected_type,
                actual_types: Vec::new(),
                raw_status: "invalid_reference_location".to_string(),
                diagnostics: vec![RunDiagnostic {
                    kind: "invalid_reference_location".to_string(),
                    message: format!("{error:#}"),
                }],
            };
        }
    };

    let result = match session
        .call_tool(GET_TYPE_BY_LOCATION_TOOL, json!({ "references": [query] }))
    {
        Ok(result) => result,
        Err(error) => {
            let message = format!("{error:#}");
            let (status, raw_status) = if message.contains("Unknown tool: get_type_by_location") {
                (CaseStatus::Failed, "unsupported_tool")
            } else {
                (CaseStatus::Error, "get_type_by_location_failed")
            };
            return TypeLookupReport {
                status,
                expression,
                expected_type,
                actual_types: Vec::new(),
                raw_status: raw_status.to_string(),
                diagnostics: vec![RunDiagnostic {
                    kind: raw_status.to_string(),
                    message,
                }],
            };
        }
    };

    let parsed = parse_get_type(&result);
    let status = if parsed.raw_status != "resolved" {
        CaseStatus::Failed
    } else if parsed
        .actual_types
        .iter()
        .any(|actual| same_type_location(actual, &expected_type))
    {
        CaseStatus::Passed
    } else {
        CaseStatus::Failed
    };

    TypeLookupReport {
        status,
        expression,
        expected_type,
        actual_types: parsed.actual_types,
        raw_status: parsed.raw_status,
        diagnostics: parsed.diagnostics,
    }
}

fn skipped_definition_lookup(lookup: &crate::UsageLookup) -> UsageDefinitionReport {
    let usage = normalize_symbol_location(&lookup.usage).unwrap_or_else(|_| NormalizedLocation {
        path: "<invalid>".to_string(),
        line: 0,
        column: None,
        display_name: Some(lookup.usage.display_name.clone()),
        kind: Some(symbol_kind_name(&lookup.usage.kind).to_string()),
    });
    let expected_declaration = normalize_symbol_location(&lookup.expected_declaration)
        .unwrap_or_else(|_| NormalizedLocation {
            path: "<invalid>".to_string(),
            line: 0,
            column: None,
            display_name: Some(lookup.expected_declaration.display_name.clone()),
            kind: Some(symbol_kind_name(&lookup.expected_declaration.kind).to_string()),
        });
    UsageDefinitionReport {
        status: CaseStatus::Skipped,
        usage,
        expected_declaration,
        actual_declarations: Vec::new(),
        raw_status: "definition_lookups_disabled".to_string(),
        diagnostics: vec![RunDiagnostic {
            kind: "definition_lookups_disabled".to_string(),
            message: "definition lookups were disabled for this run".to_string(),
        }],
    }
}

fn skipped_type_lookup(lookup: &crate::TypeLookup) -> TypeLookupReport {
    let expression =
        normalize_symbol_location(&lookup.expression).unwrap_or_else(|_| NormalizedLocation {
            path: "<invalid>".to_string(),
            line: 0,
            column: None,
            display_name: Some(lookup.expression.display_name.clone()),
            kind: Some(symbol_kind_name(&lookup.expression.kind).to_string()),
        });
    let expected_type =
        normalize_symbol_location(&lookup.expected_type).unwrap_or_else(|_| NormalizedLocation {
            path: "<invalid>".to_string(),
            line: 0,
            column: None,
            display_name: Some(lookup.expected_type.display_name.clone()),
            kind: Some(symbol_kind_name(&lookup.expected_type.kind).to_string()),
        });
    TypeLookupReport {
        status: CaseStatus::Skipped,
        expression,
        expected_type,
        actual_types: Vec::new(),
        raw_status: "definition_lookups_disabled".to_string(),
        diagnostics: vec![RunDiagnostic {
            kind: "definition_lookups_disabled".to_string(),
            message: "definition lookups were disabled for this run".to_string(),
        }],
    }
}

fn resolve_declaration_selector(
    session: &mut impl SearchToolsClient,
    declaration: &SymbolLocation,
) -> Result<ResolvedSelector> {
    let expected_path = benchmark_source_path(&declaration.location.uri)?;
    let expected_path = path_to_slash(&expected_path);
    let expected_line = declaration.location.range.start.line as usize + 1;
    let result = session.call_tool(
        "search_symbols",
        json!({
            "patterns": [declaration.display_name],
            "include_tests": true,
            "limit": 100,
        }),
    )?;
    let search = parse_search_symbols(&result)?;
    let candidates = search
        .files
        .into_iter()
        .filter(|file| file.path == expected_path)
        .flat_map(|file| {
            file.hits_for_kind(&declaration.kind)
                .into_iter()
                .map(move |hit| (file.path.clone(), hit))
        })
        .filter(|(_, hit)| {
            hit.line == expected_line && symbol_name_matches(&hit.symbol, &declaration.display_name)
        })
        .collect::<Vec<_>>();

    let selected = match candidates.as_slice() {
        [(path, hit)] => Some((path, hit)),
        [] => None,
        _ if declaration.disambiguation == Some(crate::Disambiguation::FirstMatchingSymbol) => {
            candidates.first().map(|(path, hit)| (path, hit))
        }
        _ => bail!(
            "multiple Bifrost symbols matched {}:{} `{}` ({})",
            expected_path,
            expected_line,
            declaration.display_name,
            symbol_kind_name(&declaration.kind)
        ),
    };

    match selected {
        Some((path, hit)) => {
            let selector = if count_symbol_occurrences(&result, &hit.symbol) > 1 {
                format!("{path}#{}", hit.symbol)
            } else {
                hit.symbol.clone()
            };
            Ok(ResolvedSelector { selector })
        }
        None => bail!(
            "no Bifrost symbol matched {}:{} `{}` ({})",
            expected_path,
            expected_line,
            declaration.display_name,
            symbol_kind_name(&declaration.kind)
        ),
    }
}

#[derive(Debug)]
struct ResolvedSelector {
    selector: String,
}

#[derive(Debug, Deserialize)]
struct SearchSymbolsResult {
    #[serde(default)]
    files: Vec<SearchSymbolsFile>,
}

#[derive(Debug, Deserialize)]
struct SearchSymbolsFile {
    path: String,
    #[serde(default)]
    classes: Vec<SearchSymbolHit>,
    #[serde(default)]
    functions: Vec<SearchSymbolHit>,
    #[serde(default)]
    fields: Vec<SearchSymbolHit>,
    #[serde(default)]
    modules: Vec<SearchSymbolHit>,
    #[serde(default)]
    macros: Vec<SearchSymbolHit>,
}

impl SearchSymbolsFile {
    fn hits_for_kind(&self, kind: &SymbolKind) -> Vec<SearchSymbolHit> {
        match kind {
            SymbolKind::Class | SymbolKind::Interface | SymbolKind::Type => self.classes.clone(),
            SymbolKind::Constructor => self.functions.clone(),
            SymbolKind::Method | SymbolKind::Function => self.functions.clone(),
            SymbolKind::Field
            | SymbolKind::Variable
            | SymbolKind::Constant
            | SymbolKind::Property => self.fields.clone(),
            SymbolKind::Module | SymbolKind::Package => self.modules.clone(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct SearchSymbolHit {
    symbol: String,
    line: usize,
}

fn parse_search_symbols(value: &Value) -> Result<SearchSymbolsResult> {
    serde_json::from_value(value.clone()).context("parse search_symbols result")
}

fn count_symbol_occurrences(value: &Value, symbol: &str) -> usize {
    parse_search_symbols(value)
        .map(|search| {
            search
                .files
                .into_iter()
                .flat_map(|file| {
                    file.classes
                        .into_iter()
                        .chain(file.functions)
                        .chain(file.fields)
                        .chain(file.modules)
                        .chain(file.macros)
                })
                .filter(|hit| hit.symbol == symbol)
                .count()
        })
        .unwrap_or(1)
}

#[derive(Debug)]
struct ParsedScanUsages {
    locations: Vec<NormalizedLocation>,
    partial: bool,
    raw_statuses: Vec<String>,
}

impl ParsedScanUsages {
    fn has_failure_status(&self) -> bool {
        self.raw_statuses.iter().any(|status| {
            matches!(
                status.as_str(),
                "not_found" | "ambiguous" | "failure" | "too_many_callsites"
            )
        })
    }
}

fn parse_scan_usages(value: &Value) -> ParsedScanUsages {
    let mut locations = BTreeSet::new();

    let mut raw_statuses = Vec::new();
    let mut partial = value
        .get("summary")
        .and_then(|summary| summary.get("partial"))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    if let Some(results) = value.get("results").and_then(Value::as_array) {
        for result in results {
            collect_scan_usage_locations(result, &mut locations);
            if let Some(status) = result.get("status").and_then(Value::as_str) {
                raw_statuses.push(status.to_string());
            }
            if result
                .get("complete")
                .and_then(Value::as_bool)
                .map(|complete| !complete)
                .unwrap_or(false)
            {
                partial = true;
            }
        }
    } else {
        for usage in value
            .get("usages")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            collect_scan_usage_locations(usage, &mut locations);
        }

        for key in ["not_found", "ambiguous", "failures", "too_many_callsites"] {
            if value
                .get(key)
                .and_then(Value::as_array)
                .map(|items| !items.is_empty())
                .unwrap_or(false)
            {
                raw_statuses.push(
                    match key {
                        "failures" => "failure",
                        other => other,
                    }
                    .to_string(),
                );
            }
        }
    }

    if raw_statuses.is_empty() {
        raw_statuses.push("ok".to_string());
    }

    if partial && !raw_statuses.iter().any(|status| status == "partial") {
        raw_statuses.push("partial".to_string());
    }

    ParsedScanUsages {
        locations: locations.into_iter().collect(),
        partial,
        raw_statuses,
    }
}

fn collect_scan_usage_locations(value: &Value, locations: &mut BTreeSet<NormalizedLocation>) {
    for file in value
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(path) = file.get("path").and_then(Value::as_str) else {
            continue;
        };
        for hit in file
            .get("hits")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(line) = hit.get("line").and_then(Value::as_u64) else {
                continue;
            };
            locations.insert(NormalizedLocation {
                path: path.to_string(),
                line: line as u32,
                column: None,
                display_name: None,
                kind: None,
            });
        }
    }
}

#[derive(Debug)]
struct ParsedGetDefinition {
    raw_status: String,
    actual_declarations: Vec<NormalizedLocation>,
    diagnostics: Vec<RunDiagnostic>,
}

#[derive(Debug)]
struct ParsedGetType {
    raw_status: String,
    actual_types: Vec<NormalizedLocation>,
    diagnostics: Vec<RunDiagnostic>,
}

fn parse_get_definition(value: &Value) -> ParsedGetDefinition {
    let Some(result) = value
        .get("results")
        .and_then(Value::as_array)
        .and_then(|results| results.first())
    else {
        return ParsedGetDefinition {
            raw_status: "missing_result".to_string(),
            actual_declarations: Vec::new(),
            diagnostics: vec![RunDiagnostic {
                kind: "missing_result".to_string(),
                message: "get_definitions_by_location returned no result".to_string(),
            }],
        };
    };
    let raw_status = result
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("missing_status")
        .to_string();
    let actual_declarations = result
        .get("definitions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|definition| {
            let path = definition.get("path").and_then(Value::as_str)?;
            let line = definition.get("start_line").and_then(Value::as_u64)?;
            Some(NormalizedLocation {
                path: path.to_string(),
                line: line as u32,
                column: None,
                display_name: definition
                    .get("fqn")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                kind: definition
                    .get("kind")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            })
        })
        .collect();
    let diagnostics = result
        .get("diagnostics")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|diagnostic| RunDiagnostic {
            kind: diagnostic
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("diagnostic")
                .to_string(),
            message: diagnostic
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        })
        .collect();

    ParsedGetDefinition {
        raw_status,
        actual_declarations,
        diagnostics,
    }
}

fn parse_get_type(value: &Value) -> ParsedGetType {
    let Some(result) = value
        .get("results")
        .and_then(Value::as_array)
        .and_then(|results| results.first())
    else {
        return ParsedGetType {
            raw_status: "missing_result".to_string(),
            actual_types: Vec::new(),
            diagnostics: vec![RunDiagnostic {
                kind: "missing_result".to_string(),
                message: "get_type_by_location returned no result".to_string(),
            }],
        };
    };
    let raw_status = result
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("missing_status")
        .to_string();
    let actual_types = result
        .get("types")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|lookup_type| {
            lookup_type
                .get("definitions")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|definition| {
                    let path = definition.get("path").and_then(Value::as_str)?;
                    let line = definition.get("start_line").and_then(Value::as_u64)?;
                    Some(NormalizedLocation {
                        path: path.to_string(),
                        line: line as u32,
                        column: None,
                        display_name: definition
                            .get("fqn")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        kind: definition
                            .get("kind")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    })
                })
        })
        .collect();
    let diagnostics = result
        .get("diagnostics")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|diagnostic| RunDiagnostic {
            kind: diagnostic
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("diagnostic")
                .to_string(),
            message: diagnostic
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        })
        .collect();

    ParsedGetType {
        raw_status,
        actual_types,
        diagnostics,
    }
}

fn normalize_symbol_location(symbol: &SymbolLocation) -> Result<NormalizedLocation> {
    let path = benchmark_source_path(&symbol.location.uri)?;
    Ok(NormalizedLocation {
        path: path_to_slash(&path),
        line: symbol.location.range.start.line + 1,
        column: Some(symbol.location.range.start.character + 1),
        display_name: Some(symbol.display_name.clone()),
        kind: Some(symbol_kind_name(&symbol.kind).to_string()),
    })
}

fn reference_query(location: &Location, symbol: &str, encoding: PositionEncoding) -> Result<Value> {
    let path = benchmark_source_path(&location.uri)?;
    let position = one_based_position(location, encoding)?;
    Ok(json!({
        "path": path_to_slash(&path),
        "line": position.line,
        "column": position.column,
        "symbol": symbol,
    }))
}

#[derive(Debug, PartialEq, Eq)]
struct OneBasedPosition {
    line: u32,
    column: u32,
}

fn one_based_position(location: &Location, encoding: PositionEncoding) -> Result<OneBasedPosition> {
    if encoding != PositionEncoding::Utf16 {
        bail!(
            "definition lookups currently require utf-16 benchmark positions; got {:?}",
            encoding
        );
    }
    Ok(OneBasedPosition {
        line: location.range.start.line + 1,
        column: location.range.start.character + 1,
    })
}

fn same_path_line(left: &NormalizedLocation, right: &NormalizedLocation) -> bool {
    left.path == right.path && left.line == right.line
}

fn same_type_location(actual: &NormalizedLocation, expected: &NormalizedLocation) -> bool {
    same_path_line(actual, expected)
        && actual
            .display_name
            .as_deref()
            .zip(expected.display_name.as_deref())
            .is_some_and(|(actual_name, expected_name)| {
                symbol_name_matches(actual_name, expected_name)
            })
}

fn symbol_name_matches(symbol: &str, display_name: &str) -> bool {
    symbol == display_name
        || symbol.ends_with(&format!(".{display_name}"))
        || symbol.ends_with(&format!("::{display_name}"))
        || symbol.ends_with(&format!("${display_name}"))
        || symbol.ends_with(&format!("#{display_name}"))
}

fn symbol_kind_name(kind: &SymbolKind) -> &'static str {
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

fn combine_case_status(
    declaration_to_usages: Option<&DeclarationUsageReport>,
    usage_to_declaration: &[UsageDefinitionReport],
    type_lookup: &[TypeLookupReport],
    _diagnostics: &[RunDiagnostic],
) -> CaseStatus {
    let statuses = declaration_to_usages
        .into_iter()
        .map(|report| report.status)
        .chain(usage_to_declaration.iter().map(|report| report.status))
        .chain(type_lookup.iter().map(|report| report.status))
        .collect::<Vec<_>>();
    if statuses.contains(&CaseStatus::Error) {
        CaseStatus::Error
    } else if statuses.contains(&CaseStatus::Failed) {
        CaseStatus::Failed
    } else {
        CaseStatus::Passed
    }
}

fn apply_case_expectation(
    observed_status: CaseStatus,
    expected_failure_reason: Option<&str>,
    not_planned_reason: Option<&str>,
    diagnostics: &mut Vec<RunDiagnostic>,
) -> CaseStatus {
    if observed_status == CaseStatus::Error {
        return CaseStatus::Error;
    }

    if not_planned_reason.is_some() {
        return CaseStatus::NotPlanned;
    }

    match (expected_failure_reason, observed_status) {
        (Some(_), CaseStatus::Failed) => CaseStatus::ExpectedFailure,
        (Some(_), CaseStatus::Passed) => {
            diagnostics.push(RunDiagnostic {
                kind: "expected_failure_passed".to_string(),
                message: "case is marked expectedFailure but passed; update the baseline"
                    .to_string(),
            });
            CaseStatus::Improved
        }
        _ => observed_status,
    }
}

fn compute_totals(documents: &[DocumentRunReport]) -> RunTotals {
    let mut totals = RunTotals {
        documents: documents.len(),
        ..RunTotals::default()
    };
    for case in documents.iter().flat_map(|document| &document.cases) {
        if is_planned_case_status(case.status) {
            totals.cases += 1;
        }
        match case.status {
            CaseStatus::Passed => totals.passed += 1,
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

fn is_planned_case_status(status: CaseStatus) -> bool {
    !matches!(
        status,
        CaseStatus::NotPlanned | CaseStatus::Unsupported | CaseStatus::Skipped
    )
}

fn resolve_bifrost_source_repo(repo_root: &Path, explicit: Option<&PathBuf>) -> Result<PathBuf> {
    let repo = match explicit {
        Some(path) if path.is_absolute() => path.clone(),
        Some(path) => repo_root.join(path),
        None => default_bifrost_repo(repo_root).ok_or_else(|| {
            anyhow!("could not find sibling Bifrost checkout; pass --bifrost-repo")
        })?,
    };
    if !repo.join(".git").exists() {
        bail!(
            "Bifrost repo {} does not exist or is not a git checkout",
            repo.display()
        );
    }
    Ok(repo)
}

fn prepare_bifrost_checkout(source_repo: &Path, commit: &str, work_dir: &Path) -> Result<PathBuf> {
    let checkout = work_dir.join("bifrost");
    if checkout.join(".git").exists() {
        run_command(
            Command::new("git")
                .arg("-C")
                .arg(&checkout)
                .arg("fetch")
                .arg("origin"),
        )
        .with_context(|| format!("fetch isolated Bifrost checkout {}", checkout.display()))?;
    } else {
        if let Some(parent) = checkout.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        run_command(
            Command::new("git")
                .arg("clone")
                .arg(source_repo)
                .arg(&checkout),
        )
        .with_context(|| format!("clone Bifrost repo {}", source_repo.display()))?;
    }

    if let Ok(origin_url) = git_output(source_repo, ["remote", "get-url", "origin"]) {
        run_command(
            Command::new("git")
                .arg("-C")
                .arg(&checkout)
                .arg("remote")
                .arg("set-url")
                .arg("origin")
                .arg(origin_url),
        )
        .with_context(|| format!("set isolated Bifrost remote for {}", checkout.display()))?;
        run_command(
            Command::new("git")
                .arg("-C")
                .arg(&checkout)
                .arg("fetch")
                .arg("origin"),
        )
        .with_context(|| format!("fetch Bifrost origin for {}", checkout.display()))?;
    }

    let status = git_output(&checkout, ["status", "--porcelain"])?;
    if !status.trim().is_empty() {
        bail!(
            "isolated Bifrost checkout {} has uncommitted changes; refusing to checkout {commit}",
            checkout.display()
        );
    }
    run_command(
        Command::new("git")
            .arg("-C")
            .arg(&checkout)
            .arg("checkout")
            .arg("--detach")
            .arg(commit),
    )
    .with_context(|| format!("checkout Bifrost commit {commit}"))?;
    Ok(checkout)
}

fn build_bifrost(repo: &Path) -> Result<()> {
    run_command(
        Command::new("cargo")
            .arg("build")
            .arg("--bin")
            .arg("bifrost")
            .current_dir(repo),
    )
    .with_context(|| format!("build Bifrost binary in {}", repo.display()))
}

fn bifrost_binary_path(repo: &Path) -> PathBuf {
    repo.join("target").join("debug").join(if cfg!(windows) {
        "bifrost.exe"
    } else {
        "bifrost"
    })
}

fn prepare_source_root(source: &Source, repo_root: &Path, work_dir: &Path) -> Result<PathBuf> {
    match source {
        Source::Fixture { path } => {
            let source_root = repo_root.join(path);
            if !source_root.is_dir() {
                bail!(
                    "fixture source root {} does not exist",
                    source_root.display()
                );
            }
            let canonical_repo = repo_root
                .canonicalize()
                .with_context(|| format!("canonicalize {}", repo_root.display()))?;
            let canonical_source = source_root
                .canonicalize()
                .with_context(|| format!("canonicalize {}", source_root.display()))?;
            let allowed_root = canonical_repo.join("fixtures");
            if !canonical_source.starts_with(&allowed_root) {
                bail!(
                    "fixture source root {} must stay under {}",
                    source_root.display(),
                    allowed_root.display()
                );
            }
            Ok(canonical_source)
        }
        Source::Git { repo, commit } => prepare_git_source(repo, commit, work_dir),
    }
}

fn prepare_git_source(repo: &Url, commit: &str, work_dir: &Path) -> Result<PathBuf> {
    validate_git_source_url(repo)?;
    let source_dir = work_dir
        .join("sources")
        .join(format!("{:016x}", stable_hash(&(repo.as_str(), commit))));
    if source_dir.join(".git").exists() {
        run_command(
            Command::new("git")
                .arg("-C")
                .arg(&source_dir)
                .arg("fetch")
                .arg("origin"),
        )
        .with_context(|| format!("fetch source repo {}", redacted_url(repo)))?;
    } else {
        if let Some(parent) = source_dir.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        run_command(
            Command::new("git")
                .arg("clone")
                .arg(repo.as_str())
                .arg(&source_dir),
        )
        .with_context(|| format!("clone source repo {}", redacted_url(repo)))?;
    }
    run_command(
        Command::new("git")
            .arg("-C")
            .arg(&source_dir)
            .arg("checkout")
            .arg("--detach")
            .arg(commit),
    )
    .with_context(|| format!("checkout source commit {commit}"))?;
    Ok(source_dir)
}

fn validate_git_source_url(repo: &Url) -> Result<()> {
    if !repo.username().is_empty() || repo.password().is_some() {
        bail!("git source URLs must not contain embedded credentials");
    }

    match repo.scheme() {
        "https" => {}
        other => bail!("git source URL scheme `{other}` is not allowed; use https"),
    }

    let Some(host) = repo.host() else {
        bail!("git source URL must include a host");
    };
    if is_private_or_local_host(&host) {
        bail!("git source URL host `{host}` is not allowed");
    }

    Ok(())
}

fn is_private_or_local_host(host: &Host<&str>) -> bool {
    match host {
        Host::Domain(domain) => {
            let domain = domain.trim_end_matches('.').to_ascii_lowercase();
            domain == "localhost" || domain.ends_with(".localhost")
        }
        Host::Ipv4(addr) => {
            addr.is_private()
                || addr.is_loopback()
                || addr.is_link_local()
                || addr.is_broadcast()
                || addr.is_documentation()
                || addr.octets()[0] == 0
        }
        Host::Ipv6(addr) => {
            addr.is_loopback()
                || addr.is_unspecified()
                || addr.is_unique_local()
                || addr.is_unicast_link_local()
        }
    }
}

fn redacted_url(url: &Url) -> String {
    let mut redacted = url.clone();
    let _ = redacted.set_username("");
    let _ = redacted.set_password(None);
    redacted.to_string()
}

fn git_output<const N: usize>(repo: &Path, args: [&str; N]) -> Result<String> {
    let output = command_output_with_timeout(
        Command::new("git").arg("-C").arg(repo).args(args),
        COMMAND_TIMEOUT,
    )
    .with_context(|| format!("run git in {}", repo.display()))?;
    if !output.status.success() {
        bail!(
            "git command failed in {}: {}",
            repo.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn run_command(command: &mut Command) -> Result<()> {
    let output = command_output_with_timeout(command, COMMAND_TIMEOUT)?;
    if !output.status.success() {
        bail!("{}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(())
}

fn command_output_with_timeout(
    command: &mut Command,
    timeout: Duration,
) -> Result<std::process::Output> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawn command")?;
    let stdout = child.stdout.take().context("missing command stdout pipe")?;
    let stderr = child.stderr.take().context("missing command stderr pipe")?;
    let stdout_reader = read_pipe(stdout);
    let stderr_reader = read_pipe(stderr);
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(exit_status) = child.try_wait().context("poll command")? {
            break exit_status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait().context("wait for timed-out command")?;
            let stdout = join_pipe_reader(stdout_reader, "stdout")?;
            let stderr = join_pipe_reader(stderr_reader, "stderr")?;
            bail!(
                "command timed out after {} seconds\nstdout:\n{}\nstderr:\n{}",
                timeout.as_secs(),
                String::from_utf8_lossy(&stdout),
                String::from_utf8_lossy(&stderr)
            );
        }
        thread::sleep(Duration::from_millis(100));
    };

    let stdout = join_pipe_reader(stdout_reader, "stdout")?;
    let stderr = join_pipe_reader(stderr_reader, "stderr")?;
    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

fn read_pipe(mut pipe: impl Read + Send + 'static) -> thread::JoinHandle<Result<Vec<u8>, String>> {
    thread::spawn(move || {
        let mut output = Vec::new();
        pipe.read_to_end(&mut output)
            .map(|_| output)
            .map_err(|error| error.to_string())
    })
}

fn join_pipe_reader(
    reader: thread::JoinHandle<Result<Vec<u8>, String>>,
    stream_name: &str,
) -> Result<Vec<u8>> {
    reader
        .join()
        .map_err(|_| anyhow!("command {stream_name} reader panicked"))?
        .map_err(|error| anyhow!("read command {stream_name}: {error}"))
}

struct CleanupGuard {
    path: PathBuf,
    enabled: bool,
}

impl CleanupGuard {
    fn new(path: PathBuf, enabled: bool) -> Self {
        Self { path, enabled }
    }
}

impl Drop for CleanupGuard {
    fn drop(&mut self) {
        if self.enabled && self.path.is_dir() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn unix_seconds_now() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time is before UNIX epoch")?
        .as_secs())
}

fn stable_hash<T: Hash>(value: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn display_path(path: &Path) -> String {
    path.display().to_string()
}

fn path_to_slash(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

trait SearchToolsClient {
    fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value>;
}

struct McpSession {
    child: Child,
    stdin: ChildStdin,
    stdout_lines: Receiver<Result<String, String>>,
    next_id: u64,
}

impl McpSession {
    fn start(bifrost_binary: &Path, root: &Path) -> Result<Self> {
        let mut child = Command::new(bifrost_binary)
            .arg("--root")
            .arg(root)
            .arg("--server")
            .arg("searchtools")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("spawn Bifrost MCP server {}", bifrost_binary.display()))?;
        let stdin = child.stdin.take().context("missing Bifrost stdin")?;
        let stdout = child.stdout.take().context("missing Bifrost stdout")?;
        let (sender, stdout_lines) = mpsc::channel();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) => {
                        let _ = sender.send(Err("Bifrost MCP server closed stdout".to_string()));
                        break;
                    }
                    Ok(_) => {
                        if sender.send(Ok(line)).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = sender.send(Err(format!("read MCP response: {error}")));
                        break;
                    }
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            stdout_lines,
            next_id: 1,
        })
    }

    fn initialize(&mut self) -> Result<()> {
        let response = self.request(json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {
                    "name": "usagebench",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        }))?;
        if let Some(error) = response.get("error") {
            bail!("Bifrost initialize failed: {error}");
        }
        self.notify(json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }))
    }

    fn request(&mut self, payload: Value) -> Result<Value> {
        let expected_id = payload
            .get("id")
            .cloned()
            .context("JSON-RPC request missing id")?;
        self.write_line(&payload)?;
        self.read_response(expected_id)
    }

    fn notify(&mut self, payload: Value) -> Result<()> {
        self.write_line(&payload)
    }

    fn write_line(&mut self, payload: &Value) -> Result<()> {
        writeln!(self.stdin, "{payload}")
            .and_then(|_| self.stdin.flush())
            .context("write MCP request")
    }

    fn read_response(&mut self, expected_id: Value) -> Result<Value> {
        read_json_rpc_response(&self.stdout_lines, expected_id)
    }

    fn take_id(&mut self) -> u64 {
        let next = self.next_id;
        self.next_id += 1;
        next
    }
}

fn read_json_rpc_response(
    stdout_lines: &Receiver<Result<String, String>>,
    expected_id: Value,
) -> Result<Value> {
    loop {
        let line = stdout_lines
            .recv_timeout(MCP_REQUEST_TIMEOUT)
            .with_context(|| {
                format!(
                    "timed out after {} seconds waiting for Bifrost MCP response",
                    MCP_REQUEST_TIMEOUT.as_secs()
                )
            })?
            .map_err(|message| anyhow!(message))?;
        let response: Value = serde_json::from_str(&line)
            .with_context(|| format!("parse MCP JSON response: {line}"))?;
        if response.get("id") == Some(&expected_id) {
            return Ok(response);
        }
    }
}

impl SearchToolsClient for McpSession {
    fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value> {
        let id = self.take_id();
        let response = self.request(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": arguments,
            }
        }))?;
        if let Some(error) = response.get("error") {
            bail!("Bifrost MCP request failed for `{name}`: {error}");
        }
        let result = response
            .get("result")
            .context("Bifrost MCP response missing result")?;
        if result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            let message = result
                .get("content")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .and_then(|item| item.get("text"))
                .and_then(Value::as_str)
                .unwrap_or("tool returned isError without text");
            bail!("Bifrost tool `{name}` failed: {message}");
        }
        result
            .get("structuredContent")
            .cloned()
            .ok_or_else(|| anyhow!("Bifrost tool `{name}` response missing structuredContent"))
    }
}

impl Drop for McpSession {
    fn drop(&mut self) {
        let _ = self.stdin.flush();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Location, Position, Range, TypeLookup, UsageLookup};
    use std::collections::VecDeque;

    #[test]
    fn normalizes_benchmark_uri_to_one_based_location() {
        let location = symbol_location("src/lib.rs", 4, 2, "run_demo", SymbolKind::Function);

        let normalized = normalize_symbol_location(&location).unwrap();

        assert_eq!(normalized.path, "src/lib.rs");
        assert_eq!(normalized.line, 5);
        assert_eq!(normalized.column, Some(3));
    }

    #[test]
    fn bifrost_options_enable_definition_lookups_by_default() {
        let options = RunBifrostOptions::with_defaults(PathBuf::from("benchmarks/cases"));

        assert!(options.include_definition_lookups);
        assert!(!options.bifrost_working_tree);
    }

    #[test]
    fn converts_zero_based_position_to_one_based_query_position() {
        let location = location("src/lib.rs", 7, 18);

        let position = one_based_position(&location, PositionEncoding::Utf16).unwrap();

        assert_eq!(
            position,
            OneBasedPosition {
                line: 8,
                column: 19
            }
        );
    }

    #[test]
    fn fixture_source_root_resolves_under_repo_root() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::create_dir_all(tempdir.path().join("fixtures/rust/baseline")).unwrap();
        let source = Source::Fixture {
            path: "fixtures/rust/baseline".to_string(),
        };

        let root = prepare_source_root(&source, tempdir.path(), tempdir.path()).unwrap();

        assert_eq!(
            root,
            tempdir
                .path()
                .join("fixtures/rust/baseline")
                .canonicalize()
                .unwrap()
        );
    }

    #[test]
    fn fixture_source_root_rejects_paths_outside_fixtures() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::create_dir_all(tempdir.path().join("fixtures")).unwrap();
        fs::create_dir_all(tempdir.path().join("outside")).unwrap();
        let source = Source::Fixture {
            path: "outside".to_string(),
        };

        let error = prepare_source_root(&source, tempdir.path(), tempdir.path()).unwrap_err();

        assert!(format!("{error:#}").contains("must stay under"));
    }

    #[test]
    fn default_bifrost_repo_uses_repo_root_sibling() {
        let tempdir = tempfile::tempdir().unwrap();
        let repo_root = tempdir.path().join("usagebench");
        let bifrost = tempdir.path().join("bifrost");
        fs::create_dir_all(&repo_root).unwrap();
        fs::create_dir_all(&bifrost).unwrap();

        assert_eq!(default_bifrost_repo(&repo_root), Some(bifrost));
    }

    #[test]
    fn serializes_report_with_stable_camel_case_fields() {
        let report = BifrostRunReport {
            usagebench_version: "0.1.0".to_string(),
            bifrost_repo: "/repo/bifrost".to_string(),
            bifrost_commit: "origin/master".to_string(),
            bifrost_resolved_commit: "abc123".to_string(),
            started_at_unix_seconds: 1,
            finished_at_unix_seconds: 2,
            case_files: vec!["benchmarks/cases/rust.yaml".to_string()],
            totals: RunTotals {
                documents: 1,
                cases: 1,
                passed: 1,
                improved: 0,
                failed: 0,
                expected_failures: 0,
                not_planned: 0,
                unsupported: 0,
                skipped: 0,
                errors: 0,
            },
            documents: vec![DocumentRunReport {
                case_file: "benchmarks/cases/rust.yaml".to_string(),
                language: "rust".to_string(),
                source_root: "/repo/fixtures/rust".to_string(),
                cases: vec![CaseRunReport {
                    id: "rust-function".to_string(),
                    status: CaseStatus::Passed,
                    expected_failure_reason: None,
                    not_planned_reason: None,
                    unsupported_reason: None,
                    declaration_to_usages: Some(DeclarationUsageReport {
                        status: CaseStatus::Passed,
                        selector: Some("example.build_service".to_string()),
                        expected: vec![normalized_location("src/lib.rs", 8)],
                        allowed_extra: Vec::new(),
                        actual: vec![
                            normalized_location("src/lib.rs", 8),
                            normalized_location("src/extra.rs", 1),
                        ],
                        missing: Vec::new(),
                        unexpected: vec![normalized_location("src/extra.rs", 1)],
                        partial: false,
                        raw_statuses: vec!["ok".to_string()],
                    }),
                    usage_to_declaration: Vec::new(),
                    type_lookups: Vec::new(),
                    diagnostics: Vec::new(),
                }],
            }],
        };

        let json = serde_json::to_value(report).unwrap();

        assert_eq!(json["usagebenchVersion"], "0.1.0");
        assert_eq!(json["bifrostResolvedCommit"], "abc123");
        assert_eq!(json["totals"]["passed"], 1);
        assert_eq!(
            json["documents"][0]["cases"][0]["declarationToUsages"]["unexpected"][0]["path"],
            "src/extra.rs"
        );
    }

    #[test]
    fn scorer_passes_exact_usage_result() {
        let case = benchmark_case();
        let mut client = MockClient::new(vec![
            tool(
                "search_symbols",
                search_symbols_json("src/service.rs", "example.build_service", 30),
            ),
            tool(
                "scan_usages_by_location",
                scan_usages_json(vec![("src/lib.rs", 8)], false),
            ),
            tool(
                GET_DEFINITIONS_BY_LOCATION_TOOL,
                get_definitions_by_location_json("resolved", vec![("src/service.rs", 30)]),
            ),
        ]);

        let report = run_case(&case, PositionEncoding::Utf16, &mut client, false, true);

        assert_eq!(report.status, CaseStatus::Passed);
    }

    #[test]
    fn scorer_passes_exact_type_lookup_result() {
        let mut case = benchmark_case();
        case.type_lookups.push(TypeLookup {
            expression: symbol_location("src/lib.rs", 7, 12, "service", SymbolKind::Variable),
            expected_type: symbol_location("src/service.rs", 14, 11, "Service", SymbolKind::Type),
        });
        let mut client = MockClient::new(vec![
            tool(
                "search_symbols",
                search_symbols_json("src/service.rs", "example.build_service", 30),
            ),
            tool(
                "scan_usages_by_location",
                scan_usages_json(vec![("src/lib.rs", 8)], false),
            ),
            tool(
                GET_DEFINITIONS_BY_LOCATION_TOOL,
                get_definitions_by_location_json("resolved", vec![("src/service.rs", 30)]),
            ),
            tool(
                GET_TYPE_BY_LOCATION_TOOL,
                get_type_by_location_json("resolved", vec![("src/service.rs", 15)]),
            ),
        ]);

        let report = run_case(&case, PositionEncoding::Utf16, &mut client, false, true);

        assert_eq!(report.status, CaseStatus::Passed);
        assert_eq!(report.type_lookups[0].status, CaseStatus::Passed);
    }

    #[test]
    fn type_lookup_fails_same_line_wrong_type() {
        let mut case = benchmark_case();
        case.type_lookups.push(TypeLookup {
            expression: symbol_location("src/lib.rs", 7, 12, "service", SymbolKind::Variable),
            expected_type: symbol_location("src/service.rs", 14, 11, "Service", SymbolKind::Type),
        });
        let mut client = MockClient::new(vec![
            tool(
                "search_symbols",
                search_symbols_json("src/service.rs", "example.build_service", 30),
            ),
            tool(
                "scan_usages_by_location",
                scan_usages_json(vec![("src/lib.rs", 8)], false),
            ),
            tool(
                GET_DEFINITIONS_BY_LOCATION_TOOL,
                get_definitions_by_location_json("resolved", vec![("src/service.rs", 30)]),
            ),
            tool(
                GET_TYPE_BY_LOCATION_TOOL,
                get_type_by_location_json_with_fqns(
                    "resolved",
                    vec![("src/service.rs", 15, "example.Other")],
                ),
            ),
        ]);

        let report = run_case(&case, PositionEncoding::Utf16, &mut client, false, true);

        assert_eq!(report.status, CaseStatus::Failed);
        assert_eq!(report.type_lookups[0].status, CaseStatus::Failed);
    }

    #[test]
    fn scorer_reports_type_lookup_without_declaration_scan() {
        let mut case = benchmark_case();
        case.declaration = None;
        case.expected_usages.clear();
        case.usage_lookups.clear();
        case.type_lookups.push(TypeLookup {
            expression: symbol_location("src/lib.rs", 7, 12, "service", SymbolKind::Variable),
            expected_type: symbol_location("src/service.rs", 14, 11, "Service", SymbolKind::Type),
        });
        let mut client = MockClient::new(vec![tool(
            GET_TYPE_BY_LOCATION_TOOL,
            get_type_by_location_json("resolved", vec![("src/service.rs", 15)]),
        )]);

        let report = run_case(&case, PositionEncoding::Utf16, &mut client, false, true);

        assert_eq!(report.status, CaseStatus::Passed);
        assert!(report.declaration_to_usages.is_none());
        assert_eq!(report.type_lookups[0].status, CaseStatus::Passed);
    }

    #[test]
    fn scorer_reports_missing_expected_usage() {
        let case = benchmark_case();
        let mut client = MockClient::new(vec![
            tool(
                "search_symbols",
                search_symbols_json("src/service.rs", "example.build_service", 30),
            ),
            tool(
                "scan_usages_by_location",
                scan_usages_json(Vec::new(), false),
            ),
            tool(
                GET_DEFINITIONS_BY_LOCATION_TOOL,
                get_definitions_by_location_json("resolved", vec![("src/service.rs", 30)]),
            ),
        ]);

        let report = run_case(&case, PositionEncoding::Utf16, &mut client, false, true);

        assert_eq!(report.status, CaseStatus::Failed);
        let declaration = report.declaration_to_usages.unwrap();
        assert_eq!(declaration.missing.len(), 1);
    }

    #[test]
    fn scorer_accepts_allowed_extra_usage() {
        let mut case = benchmark_case();
        case.allowed_extra_usages.push(symbol_location(
            "src/extra.rs",
            0,
            0,
            "build_service",
            SymbolKind::Function,
        ));
        let mut client = MockClient::new(vec![
            tool(
                "search_symbols",
                search_symbols_json("src/service.rs", "example.build_service", 30),
            ),
            tool(
                "scan_usages_by_location",
                scan_usages_json(vec![("src/lib.rs", 8), ("src/extra.rs", 1)], false),
            ),
            tool(
                GET_DEFINITIONS_BY_LOCATION_TOOL,
                get_definitions_by_location_json("resolved", vec![("src/service.rs", 30)]),
            ),
        ]);

        let report = run_case(&case, PositionEncoding::Utf16, &mut client, false, true);

        assert_eq!(report.status, CaseStatus::Passed);
    }

    #[test]
    fn scorer_fails_unexpected_usage() {
        let case = benchmark_case();
        let mut client = MockClient::new(vec![
            tool(
                "search_symbols",
                search_symbols_json("src/service.rs", "example.build_service", 30),
            ),
            tool(
                "scan_usages_by_location",
                scan_usages_json(vec![("src/lib.rs", 8), ("src/extra.rs", 1)], false),
            ),
            tool(
                GET_DEFINITIONS_BY_LOCATION_TOOL,
                get_definitions_by_location_json("resolved", vec![("src/service.rs", 30)]),
            ),
        ]);

        let report = run_case(&case, PositionEncoding::Utf16, &mut client, false, true);

        assert_eq!(report.status, CaseStatus::Failed);
        let declaration = report.declaration_to_usages.unwrap();
        assert_eq!(declaration.status, CaseStatus::Failed);
        assert_eq!(declaration.unexpected.len(), 1);
    }

    #[test]
    fn scorer_fails_missing_expected_usage_even_with_unexpected_usage() {
        let case = benchmark_case();
        let mut client = MockClient::new(vec![
            tool(
                "search_symbols",
                search_symbols_json("src/service.rs", "example.build_service", 30),
            ),
            tool(
                "scan_usages_by_location",
                scan_usages_json(vec![("src/extra.rs", 1)], false),
            ),
            tool(
                GET_DEFINITIONS_BY_LOCATION_TOOL,
                get_definitions_by_location_json("resolved", vec![("src/service.rs", 30)]),
            ),
        ]);

        let report = run_case(&case, PositionEncoding::Utf16, &mut client, false, true);

        assert_eq!(report.status, CaseStatus::Failed);
        let declaration = report.declaration_to_usages.unwrap();
        assert_eq!(declaration.status, CaseStatus::Failed);
        assert_eq!(declaration.missing.len(), 1);
        assert_eq!(declaration.unexpected.len(), 1);
    }

    #[test]
    fn expected_failure_marks_observed_failure_without_counting_as_failed() {
        let mut case = benchmark_case();
        case.expected_failure = Some(crate::ExpectedFailure {
            reason: "current Bifrost baseline misses this usage".to_string(),
        });
        let mut client = MockClient::new(vec![
            tool(
                "search_symbols",
                search_symbols_json("src/service.rs", "example.build_service", 30),
            ),
            tool(
                "scan_usages_by_location",
                scan_usages_json(Vec::new(), false),
            ),
        ]);

        let report = run_case(&case, PositionEncoding::Utf16, &mut client, false, false);

        assert_eq!(report.status, CaseStatus::ExpectedFailure);
        assert_eq!(
            report.expected_failure_reason.as_deref(),
            Some("current Bifrost baseline misses this usage")
        );
        let totals = compute_totals(&[DocumentRunReport {
            case_file: "benchmarks/cases/rust.yaml".to_string(),
            language: "rust".to_string(),
            source_root: "/repo/fixtures/rust".to_string(),
            cases: vec![report],
        }]);
        assert_eq!(totals.failed, 0);
        assert_eq!(totals.expected_failures, 1);
    }

    #[test]
    fn expected_failure_marks_scan_usages_failure_without_counting_as_error() {
        let mut case = benchmark_case();
        case.expected_failure = Some(crate::ExpectedFailure {
            reason: "current Bifrost baseline cannot scan this declaration".to_string(),
        });
        let mut client = MockClient::new(vec![
            tool(
                "search_symbols",
                search_symbols_json("src/service.rs", "example.build_service", 30),
            ),
            tool(
                "scan_usages_by_location",
                json!({
                    "summary": {
                        "requested_symbols": 1,
                        "resolved_symbols": 0,
                        "total_hits": 0,
                        "partial": false
                    },
                    "usages": [],
                    "failures": [{"symbol": "example.build_service"}]
                }),
            ),
        ]);

        let report = run_case(&case, PositionEncoding::Utf16, &mut client, false, false);

        assert_eq!(report.status, CaseStatus::ExpectedFailure);
        assert_eq!(
            report.declaration_to_usages.as_ref().unwrap().status,
            CaseStatus::Failed
        );
    }

    #[test]
    fn not_planned_case_runs_but_is_excluded_from_planned_totals() {
        let mut case = benchmark_case();
        case.not_planned = Some(crate::NotPlannedReason {
            reason: "runtime reflection is retained for parity tracking".to_string(),
        });
        let mut client = MockClient::new(vec![
            tool(
                "search_symbols",
                search_symbols_json("src/service.rs", "example.build_service", 30),
            ),
            tool(
                "scan_usages_by_location",
                scan_usages_json(Vec::new(), false),
            ),
        ]);

        let report = run_case(&case, PositionEncoding::Utf16, &mut client, false, false);

        assert_eq!(report.status, CaseStatus::NotPlanned);
        assert_eq!(
            report.not_planned_reason.as_deref(),
            Some("runtime reflection is retained for parity tracking")
        );
        assert_eq!(
            report.declaration_to_usages.as_ref().unwrap().status,
            CaseStatus::Failed
        );
        let totals = compute_totals(&[DocumentRunReport {
            case_file: "benchmarks/cases/rust.yaml".to_string(),
            language: "rust".to_string(),
            source_root: "/repo/fixtures/rust".to_string(),
            cases: vec![report],
        }]);
        assert_eq!(totals.cases, 0);
        assert_eq!(totals.failed, 0);
        assert_eq!(totals.not_planned, 1);
    }

    #[test]
    fn expected_failure_does_not_mask_runner_errors() {
        let mut diagnostics = Vec::new();

        let status = apply_case_expectation(
            CaseStatus::Error,
            Some("current Bifrost baseline misses this usage"),
            None,
            &mut diagnostics,
        );

        assert_eq!(status, CaseStatus::Error);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn not_planned_does_not_mask_runner_errors() {
        let mut diagnostics = Vec::new();

        let status = apply_case_expectation(
            CaseStatus::Error,
            None,
            Some("runtime reflection is out of scope"),
            &mut diagnostics,
        );

        assert_eq!(status, CaseStatus::Error);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn expected_failure_that_passes_is_reported_as_improved() {
        let mut case = benchmark_case();
        case.expected_failure = Some(crate::ExpectedFailure {
            reason: "current Bifrost baseline misses this usage".to_string(),
        });
        let mut client = MockClient::new(vec![
            tool(
                "search_symbols",
                search_symbols_json("src/service.rs", "example.build_service", 30),
            ),
            tool(
                "scan_usages_by_location",
                scan_usages_json(vec![("src/lib.rs", 8)], false),
            ),
        ]);

        let report = run_case(&case, PositionEncoding::Utf16, &mut client, false, false);

        assert_eq!(report.status, CaseStatus::Improved);
        assert_eq!(report.diagnostics[0].kind, "expected_failure_passed");
        let totals = compute_totals(&[DocumentRunReport {
            case_file: "benchmarks/cases/rust.yaml".to_string(),
            language: "rust".to_string(),
            source_root: "/repo/fixtures/rust".to_string(),
            cases: vec![report],
        }]);
        assert_eq!(totals.failed, 0);
        assert_eq!(totals.improved, 1);
    }

    #[test]
    fn unsupported_case_reports_boundary_status_by_default() {
        let mut case = benchmark_case();
        case.unsupported = Some(crate::UnsupportedReason {
            reason: "not implemented".to_string(),
        });
        let mut client = MockClient::new(Vec::new());

        let report = run_case(&case, PositionEncoding::Utf16, &mut client, false, false);

        assert_eq!(report.status, CaseStatus::Unsupported);
        assert_eq!(
            report.unsupported_reason.as_deref(),
            Some("not implemented")
        );
        let totals = compute_totals(&[DocumentRunReport {
            case_file: "benchmarks/cases/rust.yaml".to_string(),
            language: "rust".to_string(),
            source_root: "/repo/fixtures/rust".to_string(),
            cases: vec![report],
        }]);
        assert_eq!(totals.cases, 0);
        assert_eq!(totals.unsupported, 1);
        assert_eq!(totals.skipped, 0);
    }

    #[test]
    fn reverse_lookup_fails_when_definition_line_differs() {
        let case = benchmark_case();
        let mut client = MockClient::new(vec![
            tool(
                "search_symbols",
                search_symbols_json("src/service.rs", "example.build_service", 30),
            ),
            tool(
                "scan_usages_by_location",
                scan_usages_json(vec![("src/lib.rs", 8)], false),
            ),
            tool(
                GET_DEFINITIONS_BY_LOCATION_TOOL,
                get_definitions_by_location_json("resolved", vec![("src/service.rs", 99)]),
            ),
        ]);

        let report = run_case(&case, PositionEncoding::Utf16, &mut client, false, true);

        assert_eq!(report.status, CaseStatus::Failed);
        assert_eq!(report.usage_to_declaration[0].status, CaseStatus::Failed);
    }

    #[test]
    fn missing_get_definitions_by_location_tool_is_scored_failure() {
        let case = benchmark_case();
        let lookup = &case.usage_lookups[0];
        let mut client = ErrorClient {
            message:
                "Bifrost tool `get_definitions_by_location` failed: Unknown tool: get_definitions_by_location"
                    .to_string(),
        };

        let report = run_usage_to_declaration(lookup, PositionEncoding::Utf16, &mut client);

        assert_eq!(report.status, CaseStatus::Failed);
        assert_eq!(report.raw_status, "unsupported_tool");
    }

    #[test]
    fn definition_lookups_are_skipped_when_disabled() {
        let case = benchmark_case();
        let mut client = MockClient::new(vec![
            tool(
                "search_symbols",
                search_symbols_json("src/service.rs", "example.build_service", 30),
            ),
            tool(
                "scan_usages_by_location",
                scan_usages_json(vec![("src/lib.rs", 8)], false),
            ),
        ]);

        let report = run_case(&case, PositionEncoding::Utf16, &mut client, false, false);

        assert_eq!(report.status, CaseStatus::Passed);
        assert_eq!(report.usage_to_declaration[0].status, CaseStatus::Skipped);
        assert_eq!(
            report.usage_to_declaration[0].raw_status,
            "definition_lookups_disabled"
        );
    }

    #[test]
    fn partial_scan_usages_result_fails_case() {
        let case = benchmark_case();
        let mut client = MockClient::new(vec![
            tool(
                "search_symbols",
                search_symbols_json("src/service.rs", "example.build_service", 30),
            ),
            tool(
                "scan_usages_by_location",
                scan_usages_json(vec![("src/lib.rs", 8)], true),
            ),
        ]);

        let report = run_case(&case, PositionEncoding::Utf16, &mut client, false, false);

        assert_eq!(report.status, CaseStatus::Failed);
        let declaration = report.declaration_to_usages.unwrap();
        assert!(declaration.partial);
        assert!(declaration.raw_statuses.contains(&"partial".to_string()));
    }

    #[test]
    fn parse_scan_usages_reads_results_shape_with_hits() {
        let parsed = parse_scan_usages(&scan_usages_results_json(
            "found",
            vec![("src/service.cpp", 17), ("src/main.cpp", 7)],
            true,
            false,
        ));

        assert_eq!(
            parsed.locations,
            vec![
                NormalizedLocation {
                    path: "src/main.cpp".to_string(),
                    line: 7,
                    column: None,
                    display_name: None,
                    kind: None,
                },
                NormalizedLocation {
                    path: "src/service.cpp".to_string(),
                    line: 17,
                    column: None,
                    display_name: None,
                    kind: None,
                }
            ]
        );
        assert_eq!(parsed.raw_statuses, vec!["found".to_string()]);
        assert!(!parsed.partial);
        assert!(!parsed.has_failure_status());
    }

    #[test]
    fn parse_scan_usages_treats_failure_status_as_failure() {
        let parsed = parse_scan_usages(&scan_usages_results_json(
            "failure",
            Vec::new(),
            true,
            false,
        ));

        assert_eq!(parsed.raw_statuses, vec!["failure".to_string()]);
        assert!(parsed.has_failure_status());
    }

    #[test]
    fn parse_scan_usages_marks_incomplete_results_partial() {
        let parsed =
            parse_scan_usages(&scan_usages_results_json("found", Vec::new(), false, false));

        assert!(parsed.partial);
        assert_eq!(
            parsed.raw_statuses,
            vec!["found".to_string(), "partial".to_string()]
        );
    }

    #[test]
    fn parse_scan_usages_keeps_legacy_usages_shape() {
        let parsed = parse_scan_usages(&scan_usages_json(vec![("src/lib.rs", 8)], false));

        assert_eq!(
            parsed.locations,
            vec![NormalizedLocation {
                path: "src/lib.rs".to_string(),
                line: 8,
                column: None,
                display_name: None,
                kind: None,
            }]
        );
        assert_eq!(parsed.raw_statuses, vec!["ok".to_string()]);
        assert!(!parsed.partial);
    }

    #[test]
    fn constructor_declarations_resolve_from_functions_bucket() {
        let mut case = benchmark_case();
        case.declaration = Some(symbol_location(
            "src/service.rs",
            29,
            7,
            "Service",
            SymbolKind::Constructor,
        ));
        let mut client = MockClient::new(vec![
            tool(
                "search_symbols",
                json!({
                    "files": [{
                        "path": "src/service.rs",
                        "loc": 10,
                        "classes": [{"symbol": "example.Service", "signature": "", "line": 30}],
                        "functions": [{"symbol": "example.Service", "signature": "", "line": 30}],
                        "fields": [],
                        "modules": [],
                        "macros": []
                    }]
                }),
            ),
            tool(
                "scan_usages_by_location",
                scan_usages_json(vec![("src/lib.rs", 8)], false),
            ),
        ]);

        let report = run_case(&case, PositionEncoding::Utf16, &mut client, false, false);

        assert_eq!(report.status, CaseStatus::Passed);
    }

    #[test]
    fn declaration_resolution_accepts_dollar_namespace_separator() {
        let mut case = benchmark_case();
        case.declaration = Some(symbol_location(
            "lib/billing/invoice.rb",
            6,
            6,
            "Invoice",
            SymbolKind::Class,
        ));
        let mut client = MockClient::new(vec![
            tool(
                "search_symbols",
                json!({
                    "files": [{
                        "path": "lib/billing/invoice.rb",
                        "loc": 10,
                        "classes": [{"symbol": "Billing$Invoice", "signature": "", "line": 7}],
                        "functions": [],
                        "fields": [],
                        "modules": [],
                        "macros": []
                    }]
                }),
            ),
            tool(
                "scan_usages_by_location",
                scan_usages_json(vec![("src/lib.rs", 8)], false),
            ),
        ]);

        let report = run_case(&case, PositionEncoding::Utf16, &mut client, false, false);

        assert_eq!(report.status, CaseStatus::Passed);
        assert_eq!(
            report.declaration_to_usages.unwrap().selector.as_deref(),
            Some("Billing$Invoice")
        );
    }

    #[test]
    fn ambiguous_declaration_candidate_fails_case() {
        let case = benchmark_case();
        let mut client = MockClient::new(vec![
            tool(
                "search_symbols",
                json!({
                    "files": [{
                        "path": "src/service.rs",
                        "loc": 10,
                        "classes": [],
                        "functions": [
                            {"symbol": "a.build_service", "signature": "", "line": 30},
                            {"symbol": "b.build_service", "signature": "", "line": 30}
                        ],
                        "fields": [],
                        "modules": [],
                        "macros": []
                    }]
                }),
            ),
            tool(
                GET_DEFINITIONS_BY_LOCATION_TOOL,
                get_definitions_by_location_json("resolved", vec![("src/service.rs", 30)]),
            ),
        ]);

        let report = run_case(&case, PositionEncoding::Utf16, &mut client, false, true);

        assert_eq!(report.status, CaseStatus::Failed);
        assert_eq!(report.diagnostics[0].kind, "symbol_resolution_failed");
    }

    #[test]
    fn first_matching_symbol_disambiguation_selects_first_candidate() {
        let mut case = benchmark_case();
        case.declaration.as_mut().unwrap().disambiguation =
            Some(crate::Disambiguation::FirstMatchingSymbol);
        let mut client = MockClient::new(vec![
            tool(
                "search_symbols",
                json!({
                    "files": [{
                        "path": "src/service.rs",
                        "loc": 10,
                        "classes": [],
                        "functions": [
                            {"symbol": "a.build_service", "signature": "", "line": 30},
                            {"symbol": "b.build_service", "signature": "", "line": 30}
                        ],
                        "fields": [],
                        "modules": [],
                        "macros": []
                    }]
                }),
            ),
            tool(
                "scan_usages_by_location",
                scan_usages_json(vec![("src/lib.rs", 8)], false),
            ),
        ]);

        let report = run_case(&case, PositionEncoding::Utf16, &mut client, false, false);

        assert_eq!(report.status, CaseStatus::Passed);
    }

    #[test]
    fn git_source_url_policy_rejects_unsafe_inputs() {
        validate_git_source_url(&Url::parse("https://github.com/BrokkAi/bifrost.git").unwrap())
            .unwrap();

        for url in [
            "file:///tmp/repo",
            "ssh://github.com/BrokkAi/bifrost.git",
            "git://github.com/BrokkAi/bifrost.git",
            "https://user:secret@example.com/repo.git",
            "https://localhost/repo.git",
            "https://127.0.0.1/repo.git",
            "https://169.254.169.254/latest/meta-data",
            "https://[::1]/repo.git",
        ] {
            assert!(
                validate_git_source_url(&Url::parse(url).unwrap()).is_err(),
                "{url} should be rejected"
            );
        }
    }

    #[test]
    fn redacted_url_removes_embedded_credentials() {
        let url = Url::parse("https://user:secret@example.com/repo.git").unwrap();

        let redacted = redacted_url(&url);

        assert_eq!(redacted, "https://example.com/repo.git");
        assert!(!redacted.contains("secret"));
    }

    #[test]
    fn command_output_with_timeout_drains_large_output() {
        let output = command_output_with_timeout(
            Command::new("sh").arg("-c").arg(
                "i=0; while [ $i -lt 20000 ]; do printf 'abcdefghijklmnopqrstuvwxyz\\n'; i=$((i+1)); done",
            ),
            Duration::from_secs(10),
        )
        .unwrap();

        assert!(output.status.success());
        assert!(output.stdout.len() > 500_000);
    }

    #[test]
    fn json_rpc_response_reader_skips_notifications_and_other_ids() {
        let (sender, receiver) = mpsc::channel();
        sender
            .send(Ok(
                json!({"jsonrpc": "2.0", "method": "progress"}).to_string()
            ))
            .unwrap();
        sender
            .send(Ok(
                json!({"jsonrpc": "2.0", "id": 1, "result": "wrong"}).to_string()
            ))
            .unwrap();
        sender
            .send(Ok(
                json!({"jsonrpc": "2.0", "id": 2, "result": "right"}).to_string()
            ))
            .unwrap();

        let response = read_json_rpc_response(&receiver, json!(2)).unwrap();

        assert_eq!(response["result"], "right");
    }

    struct MockClient {
        responses: VecDeque<(String, Value)>,
    }

    impl MockClient {
        fn new(responses: Vec<(String, Value)>) -> Self {
            Self {
                responses: VecDeque::from(responses),
            }
        }
    }

    impl SearchToolsClient for MockClient {
        fn call_tool(&mut self, name: &str, _arguments: Value) -> Result<Value> {
            let (expected_name, value) = self.responses.pop_front().unwrap();
            assert_eq!(expected_name, name);
            Ok(value)
        }
    }

    struct ErrorClient {
        message: String,
    }

    impl SearchToolsClient for ErrorClient {
        fn call_tool(&mut self, _name: &str, _arguments: Value) -> Result<Value> {
            Err(anyhow!(self.message.clone()))
        }
    }

    fn tool(name: &str, value: Value) -> (String, Value) {
        (name.to_string(), value)
    }

    fn normalized_location(path: &str, line: u32) -> NormalizedLocation {
        NormalizedLocation {
            path: path.to_string(),
            line,
            column: None,
            display_name: None,
            kind: None,
        }
    }

    fn benchmark_case() -> BenchmarkCase {
        BenchmarkCase {
            id: "rust-function".to_string(),
            declaration: Some(symbol_location(
                "src/service.rs",
                29,
                7,
                "build_service",
                SymbolKind::Function,
            )),
            expected_usages: vec![symbol_location(
                "src/lib.rs",
                7,
                18,
                "build_service",
                SymbolKind::Function,
            )],
            allowed_extra_usages: Vec::new(),
            usage_lookups: vec![UsageLookup {
                usage: symbol_location("src/lib.rs", 7, 18, "build_service", SymbolKind::Function),
                expected_declaration: symbol_location(
                    "src/service.rs",
                    29,
                    7,
                    "build_service",
                    SymbolKind::Function,
                ),
            }],
            type_lookups: Vec::new(),
            expected_failure: None,
            not_planned: None,
            unsupported: None,
            verification: None,
        }
    }

    fn symbol_location(
        path: &str,
        line: u32,
        character: u32,
        name: &str,
        kind: SymbolKind,
    ) -> SymbolLocation {
        SymbolLocation {
            location: location(path, line, character),
            kind,
            display_name: name.to_string(),
            disambiguation: None,
        }
    }

    fn location(path: &str, line: u32, character: u32) -> Location {
        Location {
            uri: Url::parse(&format!("benchmark://source/{path}")).unwrap(),
            range: Range {
                start: Position { line, character },
                end: Position {
                    line,
                    character: character + 1,
                },
            },
        }
    }

    fn search_symbols_json(path: &str, symbol: &str, line: usize) -> Value {
        json!({
            "patterns": ["build_service"],
            "truncated": false,
            "total_files": 1,
            "files": [{
                "path": path,
                "loc": 10,
                "classes": [],
                "functions": [{"symbol": symbol, "signature": "", "line": line}],
                "fields": [],
                "modules": [],
                "macros": []
            }]
        })
    }

    fn scan_usages_json(locations: Vec<(&str, usize)>, partial: bool) -> Value {
        json!({
            "summary": {
                "requested_symbols": 1,
                "resolved_symbols": 1,
                "total_hits": locations.len(),
                "partial": partial
            },
            "usages": [{
                "symbol": "example.build_service",
                "total_hits": locations.len(),
                "rendering": "full",
                "files": locations.into_iter().map(|(path, line)| {
                    json!({
                        "path": path,
                        "hits": [{"line": line, "enclosing": "run_demo"}]
                    })
                }).collect::<Vec<_>>()
            }]
        })
    }

    fn scan_usages_results_json(
        status: &str,
        locations: Vec<(&str, usize)>,
        complete: bool,
        summary_partial: bool,
    ) -> Value {
        json!({
            "summary": {
                "requested_symbols": 1,
                "resolved_symbols": 1,
                "total_hits": locations.len(),
                "partial": summary_partial
            },
            "results": [{
                "symbol": "example.build_service",
                "status": status,
                "complete": complete,
                "files": locations.into_iter().map(|(path, line)| {
                    json!({
                        "path": path,
                        "hits": [{"line": line, "enclosing": "run_demo"}]
                    })
                }).collect::<Vec<_>>()
            }]
        })
    }

    fn get_type_by_location_json(status: &str, locations: Vec<(&str, usize)>) -> Value {
        get_type_by_location_json_with_fqns(
            status,
            locations
                .into_iter()
                .map(|(path, line)| (path, line, "example.Service"))
                .collect(),
        )
    }

    fn get_type_by_location_json_with_fqns(
        status: &str,
        locations: Vec<(&str, usize, &str)>,
    ) -> Value {
        json!({
            "results": [{
                "query": {"path": "src/lib.rs", "line": 8, "column": 19, "symbol": "build_service"},
                "status": status,
                "types": [{
                    "fqn": "example.Service",
                    "definitions": locations.into_iter().map(|(path, line, fqn)| {
                        json!({
                            "fqn": fqn,
                            "path": path,
                            "start_line": line,
                            "end_line": line,
                            "kind": "class",
                            "language": "rust"
                        })
                    }).collect::<Vec<_>>()
                }],
                "diagnostics": []
            }]
        })
    }

    fn get_definitions_by_location_json(status: &str, locations: Vec<(&str, usize)>) -> Value {
        json!({
            "results": [{
                "query": {"path": "src/lib.rs", "line": 8, "column": 19, "symbol": "build_service"},
                "status": status,
                "definitions": locations.into_iter().map(|(path, line)| {
                    json!({
                        "fqn": "example.build_service",
                        "path": path,
                        "start_line": line,
                        "end_line": line,
                        "kind": "function",
                        "language": "rust"
                    })
                }).collect::<Vec<_>>(),
                "diagnostics": []
            }]
        })
    }
}
