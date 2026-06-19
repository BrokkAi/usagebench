use crate::{
    benchmark_source_path, collect_case_files, find_repo_root_for_path, BenchmarkCase,
    BenchmarkDocument, Location, PositionEncoding, Source, SymbolKind, SymbolLocation,
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
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};
use url::Url;

const DEFAULT_BIFROST_COMMIT: &str = "origin/master";

#[derive(Debug, Clone)]
pub struct RunBifrostOptions {
    pub case_path: PathBuf,
    pub bifrost_repo: Option<PathBuf>,
    pub bifrost_commit: String,
    pub work_dir: PathBuf,
    pub output: Option<PathBuf>,
    pub include_unsupported: bool,
    pub keep_worktrees: bool,
}

impl RunBifrostOptions {
    pub fn with_defaults(case_path: PathBuf) -> Self {
        Self {
            case_path,
            bifrost_repo: None,
            bifrost_commit: DEFAULT_BIFROST_COMMIT.to_string(),
            work_dir: PathBuf::from("target/usagebench"),
            output: None,
            include_unsupported: false,
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
    pub failed: usize,
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
    Failed,
    Skipped,
    Error,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CaseRunReport {
    pub id: String,
    pub status: CaseStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unsupported_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declaration_to_usages: Option<DeclarationUsageReport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub usage_to_declaration: Vec<UsageDefinitionReport>,
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

pub fn default_bifrost_repo() -> PathBuf {
    let local_sibling = PathBuf::from("../bifrost");
    if local_sibling.is_dir() {
        local_sibling
    } else {
        PathBuf::from("/Users/dave/Workspace/BrokkAi/bifrost")
    }
}

pub fn run_bifrost(options: RunBifrostOptions) -> Result<BifrostRunReport> {
    let started_at = unix_seconds_now()?;
    let repo_root = find_repo_root_for_path(&options.case_path)?;
    let mut case_files = Vec::new();
    collect_case_files(&options.case_path, &mut case_files)?;
    case_files.sort();
    if case_files.is_empty() {
        bail!(
            "no benchmark case YAML files found under {}",
            options.case_path.display()
        );
    }

    let work_dir = if options.work_dir.is_absolute() {
        options.work_dir.clone()
    } else {
        repo_root.join(&options.work_dir)
    };
    fs::create_dir_all(&work_dir).with_context(|| format!("create {}", work_dir.display()))?;

    let bifrost_repo = options
        .bifrost_repo
        .clone()
        .unwrap_or_else(default_bifrost_repo);
    let bifrost_repo = if bifrost_repo.is_absolute() {
        bifrost_repo
    } else {
        repo_root.join(bifrost_repo)
    };
    prepare_bifrost_repo(&bifrost_repo, &options.bifrost_commit)?;
    let bifrost_resolved_commit = git_output(&bifrost_repo, ["rev-parse", "HEAD"])?;
    build_bifrost(&bifrost_repo)?;
    let bifrost_binary = bifrost_binary_path(&bifrost_repo);

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
        )
        .with_context(|| format!("run benchmark cases {}", case_file.display()))?;
        documents.push(DocumentRunReport {
            case_file: display_path(case_file),
            language: document.language,
            source_root: display_path(&source_root),
            cases,
        });
    }

    if !options.keep_worktrees {
        let git_sources = work_dir.join("sources");
        if git_sources.is_dir() {
            let _ = fs::remove_dir_all(&git_sources);
        }
    }

    let finished_at = unix_seconds_now()?;
    let mut report = BifrostRunReport {
        usagebench_version: env!("CARGO_PKG_VERSION").to_string(),
        bifrost_repo: display_path(&bifrost_repo),
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
        ));
    }
    Ok(reports)
}

fn run_case(
    case: &BenchmarkCase,
    encoding: PositionEncoding,
    session: &mut impl SearchToolsClient,
    include_unsupported: bool,
) -> CaseRunReport {
    if let Some(unsupported) = &case.unsupported {
        if !include_unsupported {
            return CaseRunReport {
                id: case.id.clone(),
                status: CaseStatus::Skipped,
                unsupported_reason: Some(unsupported.reason.clone()),
                declaration_to_usages: None,
                usage_to_declaration: Vec::new(),
                diagnostics: Vec::new(),
            };
        }
    }

    let mut diagnostics = Vec::new();
    let declaration_to_usages = Some(run_declaration_to_usages(case, session, &mut diagnostics));
    let usage_to_declaration = case
        .usage_lookups
        .iter()
        .map(|lookup| run_usage_to_declaration(lookup, encoding, session))
        .collect::<Vec<_>>();

    let status = combine_case_status(
        declaration_to_usages.as_ref(),
        &usage_to_declaration,
        &diagnostics,
    );
    CaseRunReport {
        id: case.id.clone(),
        status,
        unsupported_reason: case
            .unsupported
            .as_ref()
            .map(|unsupported| unsupported.reason.clone()),
        declaration_to_usages,
        usage_to_declaration,
        diagnostics,
    }
}

fn run_declaration_to_usages(
    case: &BenchmarkCase,
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

    let selector = match resolve_declaration_selector(session, &case.declaration) {
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

    let result = match session.call_tool(
        "scan_usages",
        json!({
            "symbols": [selector.selector.clone()],
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
    let has_error_status = parsed.has_error_status();
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
    let status = if has_error_status {
        CaseStatus::Error
    } else if missing.is_empty() && unexpected.is_empty() {
        CaseStatus::Passed
    } else {
        CaseStatus::Failed
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
        "get_definition",
        json!({
            "references": [query],
            "include_tests": true,
        }),
    ) {
        Ok(result) => result,
        Err(error) => {
            let message = format!("{error:#}");
            let (status, raw_status) = if message.contains("Unknown tool: get_definition") {
                (CaseStatus::Failed, "unsupported_tool")
            } else {
                (CaseStatus::Error, "get_definition_failed")
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

    match candidates.as_slice() {
        [(path, hit)] => {
            let selector = if count_symbol_occurrences(&result, &hit.symbol) > 1 {
                format!("{path}#{}", hit.symbol)
            } else {
                hit.symbol.clone()
            };
            Ok(ResolvedSelector { selector })
        }
        [] => bail!(
            "no Bifrost symbol matched {}:{} `{}` ({})",
            expected_path,
            expected_line,
            declaration.display_name,
            symbol_kind_name(&declaration.kind)
        ),
        _ => bail!(
            "multiple Bifrost symbols matched {}:{} `{}` ({})",
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
            SymbolKind::Class
            | SymbolKind::Constructor
            | SymbolKind::Interface
            | SymbolKind::Type => self.classes.clone(),
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
    fn has_error_status(&self) -> bool {
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
    for usage in value
        .get("usages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        for file in usage
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

    let mut raw_statuses = Vec::new();
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
    if raw_statuses.is_empty() {
        raw_statuses.push("ok".to_string());
    }

    let partial = value
        .get("summary")
        .and_then(|summary| summary.get("partial"))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    ParsedScanUsages {
        locations: locations.into_iter().collect(),
        partial,
        raw_statuses,
    }
}

#[derive(Debug)]
struct ParsedGetDefinition {
    raw_status: String,
    actual_declarations: Vec<NormalizedLocation>,
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
                message: "get_definition returned no result".to_string(),
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

fn one_based_position(
    location: &Location,
    _encoding: PositionEncoding,
) -> Result<OneBasedPosition> {
    Ok(OneBasedPosition {
        line: location.range.start.line + 1,
        column: location.range.start.character + 1,
    })
}

fn same_path_line(left: &NormalizedLocation, right: &NormalizedLocation) -> bool {
    left.path == right.path && left.line == right.line
}

fn symbol_name_matches(symbol: &str, display_name: &str) -> bool {
    symbol == display_name
        || symbol.ends_with(&format!(".{display_name}"))
        || symbol.ends_with(&format!("::{display_name}"))
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
    _diagnostics: &[RunDiagnostic],
) -> CaseStatus {
    let statuses = declaration_to_usages
        .into_iter()
        .map(|report| report.status)
        .chain(usage_to_declaration.iter().map(|report| report.status))
        .collect::<Vec<_>>();
    if statuses.iter().any(|status| *status == CaseStatus::Error) {
        CaseStatus::Error
    } else if statuses.iter().any(|status| *status == CaseStatus::Failed) {
        CaseStatus::Failed
    } else {
        CaseStatus::Passed
    }
}

fn compute_totals(documents: &[DocumentRunReport]) -> RunTotals {
    let mut totals = RunTotals {
        documents: documents.len(),
        ..RunTotals::default()
    };
    for case in documents.iter().flat_map(|document| &document.cases) {
        totals.cases += 1;
        match case.status {
            CaseStatus::Passed => totals.passed += 1,
            CaseStatus::Failed => totals.failed += 1,
            CaseStatus::Skipped => totals.skipped += 1,
            CaseStatus::Error => totals.errors += 1,
        }
    }
    totals
}

fn prepare_bifrost_repo(repo: &Path, commit: &str) -> Result<()> {
    if !repo.join(".git").exists() {
        bail!(
            "Bifrost repo {} does not exist or is not a git checkout",
            repo.display()
        );
    }
    let status = git_output(repo, ["status", "--porcelain"])?;
    if !status.trim().is_empty() {
        bail!(
            "Bifrost repo {} has uncommitted changes; refusing to checkout {commit}",
            repo.display()
        );
    }
    run_command(
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .arg("fetch")
            .arg("origin"),
    )
    .with_context(|| format!("fetch Bifrost repo {}", repo.display()))?;
    run_command(
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .arg("checkout")
            .arg("--detach")
            .arg(commit),
    )
    .with_context(|| format!("checkout Bifrost commit {commit}"))?;
    Ok(())
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
            Ok(source_root)
        }
        Source::Git { repo, commit } => prepare_git_source(repo, commit, work_dir),
    }
}

fn prepare_git_source(repo: &Url, commit: &str, work_dir: &Path) -> Result<PathBuf> {
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
        .with_context(|| format!("fetch source repo {}", repo))?;
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
        .with_context(|| format!("clone source repo {}", repo))?;
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

fn git_output<const N: usize>(repo: &Path, args: [&str; N]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
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
    let output = command.output().context("spawn command")?;
    if !output.status.success() {
        bail!("{}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(())
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
    reader: BufReader<ChildStdout>,
    stderr: ChildStderr,
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
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("spawn Bifrost MCP server {}", bifrost_binary.display()))?;
        let stdin = child.stdin.take().context("missing Bifrost stdin")?;
        let stdout = child.stdout.take().context("missing Bifrost stdout")?;
        let stderr = child.stderr.take().context("missing Bifrost stderr")?;
        Ok(Self {
            child,
            stdin,
            reader: BufReader::new(stdout),
            stderr,
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
        self.write_line(&payload)?;
        self.read_line()
    }

    fn notify(&mut self, payload: Value) -> Result<()> {
        self.write_line(&payload)
    }

    fn write_line(&mut self, payload: &Value) -> Result<()> {
        writeln!(self.stdin, "{payload}")
            .and_then(|_| self.stdin.flush())
            .context("write MCP request")
    }

    fn read_line(&mut self) -> Result<Value> {
        let mut line = String::new();
        let bytes = self
            .reader
            .read_line(&mut line)
            .context("read MCP response")?;
        if bytes == 0 {
            let mut stderr = String::new();
            let _ = self.stderr.read_to_string(&mut stderr);
            bail!("Bifrost MCP server closed early; stderr:\n{stderr}");
        }
        serde_json::from_str(&line).with_context(|| format!("parse MCP JSON response: {line}"))
    }

    fn take_id(&mut self) -> u64 {
        let next = self.next_id;
        self.next_id += 1;
        next
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
    use crate::{Location, Position, Range, UsageLookup};
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

        assert_eq!(root, tempdir.path().join("fixtures/rust/baseline"));
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
                failed: 0,
                skipped: 0,
                errors: 0,
            },
            documents: Vec::new(),
        };

        let json = serde_json::to_value(report).unwrap();

        assert_eq!(json["usagebenchVersion"], "0.1.0");
        assert_eq!(json["bifrostResolvedCommit"], "abc123");
        assert_eq!(json["totals"]["passed"], 1);
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
                "scan_usages",
                scan_usages_json(vec![("src/lib.rs", 8)], false),
            ),
            tool(
                "get_definition",
                get_definition_json("resolved", vec![("src/service.rs", 30)]),
            ),
        ]);

        let report = run_case(&case, PositionEncoding::Utf16, &mut client, false);

        assert_eq!(report.status, CaseStatus::Passed);
    }

    #[test]
    fn scorer_reports_missing_expected_usage() {
        let case = benchmark_case();
        let mut client = MockClient::new(vec![
            tool(
                "search_symbols",
                search_symbols_json("src/service.rs", "example.build_service", 30),
            ),
            tool("scan_usages", scan_usages_json(Vec::new(), false)),
            tool(
                "get_definition",
                get_definition_json("resolved", vec![("src/service.rs", 30)]),
            ),
        ]);

        let report = run_case(&case, PositionEncoding::Utf16, &mut client, false);

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
                "scan_usages",
                scan_usages_json(vec![("src/lib.rs", 8), ("src/extra.rs", 1)], false),
            ),
            tool(
                "get_definition",
                get_definition_json("resolved", vec![("src/service.rs", 30)]),
            ),
        ]);

        let report = run_case(&case, PositionEncoding::Utf16, &mut client, false);

        assert_eq!(report.status, CaseStatus::Passed);
    }

    #[test]
    fn scorer_reports_unexpected_usage() {
        let case = benchmark_case();
        let mut client = MockClient::new(vec![
            tool(
                "search_symbols",
                search_symbols_json("src/service.rs", "example.build_service", 30),
            ),
            tool(
                "scan_usages",
                scan_usages_json(vec![("src/lib.rs", 8), ("src/extra.rs", 1)], false),
            ),
            tool(
                "get_definition",
                get_definition_json("resolved", vec![("src/service.rs", 30)]),
            ),
        ]);

        let report = run_case(&case, PositionEncoding::Utf16, &mut client, false);

        assert_eq!(report.status, CaseStatus::Failed);
        let declaration = report.declaration_to_usages.unwrap();
        assert_eq!(declaration.unexpected.len(), 1);
    }

    #[test]
    fn unsupported_case_is_skipped_by_default() {
        let mut case = benchmark_case();
        case.unsupported = Some(crate::UnsupportedReason {
            reason: "not implemented".to_string(),
        });
        let mut client = MockClient::new(Vec::new());

        let report = run_case(&case, PositionEncoding::Utf16, &mut client, false);

        assert_eq!(report.status, CaseStatus::Skipped);
        assert_eq!(
            report.unsupported_reason.as_deref(),
            Some("not implemented")
        );
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
                "scan_usages",
                scan_usages_json(vec![("src/lib.rs", 8)], false),
            ),
            tool(
                "get_definition",
                get_definition_json("resolved", vec![("src/service.rs", 99)]),
            ),
        ]);

        let report = run_case(&case, PositionEncoding::Utf16, &mut client, false);

        assert_eq!(report.status, CaseStatus::Failed);
        assert_eq!(report.usage_to_declaration[0].status, CaseStatus::Failed);
    }

    #[test]
    fn missing_get_definition_tool_is_scored_failure() {
        let case = benchmark_case();
        let lookup = &case.usage_lookups[0];
        let mut client = ErrorClient {
            message: "Bifrost tool `get_definition` failed: Unknown tool: get_definition"
                .to_string(),
        };

        let report = run_usage_to_declaration(lookup, PositionEncoding::Utf16, &mut client);

        assert_eq!(report.status, CaseStatus::Failed);
        assert_eq!(report.raw_status, "unsupported_tool");
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
                "get_definition",
                get_definition_json("resolved", vec![("src/service.rs", 30)]),
            ),
        ]);

        let report = run_case(&case, PositionEncoding::Utf16, &mut client, false);

        assert_eq!(report.status, CaseStatus::Failed);
        assert_eq!(report.diagnostics[0].kind, "symbol_resolution_failed");
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

    fn benchmark_case() -> BenchmarkCase {
        BenchmarkCase {
            id: "rust-function".to_string(),
            declaration: symbol_location(
                "src/service.rs",
                29,
                7,
                "build_service",
                SymbolKind::Function,
            ),
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

    fn get_definition_json(status: &str, locations: Vec<(&str, usize)>) -> Value {
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
