use super::bifrost::{command_output_with_timeout, prepare_source_root};
use super::mcp::{McpSession, ToolClient};
use super::{
    compute_totals, normalize_symbol_location, path_to_slash, score_declaration_locations,
    symbol_kind_name, CapabilitySupport, CaseRunReport, CaseStatus, DeclarationUsageReport,
    DocumentRunReport, NormalizedLocation, RunDiagnostic, RunReport, RunTotals, RunnerCapability,
    RunnerMetadata, RunnerOperation, TypeLookupReport, UsageDefinitionReport,
};
use crate::{
    find_repo_root_for_path, BenchmarkCase, BenchmarkDocument, SymbolKind, SymbolLocation,
    TypeLookup, UsageLookup,
};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub const DEFAULT_REPOWISE_VERSION: &str = "0.31.0";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(20 * 60);

#[derive(Debug, Clone)]
pub struct RunRepowiseOptions {
    pub case_path: PathBuf,
    pub repowise_version: String,
    pub repowise_command: Option<PathBuf>,
    pub work_dir: PathBuf,
    pub output: Option<PathBuf>,
    pub include_unsupported: bool,
    pub keep_worktrees: bool,
}

impl RunRepowiseOptions {
    pub fn with_defaults(case_path: PathBuf) -> Self {
        Self {
            case_path,
            repowise_version: DEFAULT_REPOWISE_VERSION.to_string(),
            repowise_command: None,
            work_dir: PathBuf::from("target/usagebench"),
            output: None,
            include_unsupported: false,
            keep_worktrees: false,
        }
    }
}

pub fn run_repowise(options: RunRepowiseOptions) -> Result<RunReport> {
    if options.repowise_version != DEFAULT_REPOWISE_VERSION {
        bail!(
            "the Repowise adapter supports exactly version {}; got {}",
            DEFAULT_REPOWISE_VERSION,
            options.repowise_version
        );
    }

    let started_at = unix_seconds_now()?;
    let repo_root = find_repo_root_for_path(&options.case_path)?;
    let case_files = crate::validate_path(&options.case_path)?;
    let work_dir = if options.work_dir.is_absolute() {
        options.work_dir.clone()
    } else {
        repo_root.join(&options.work_dir)
    };
    let run_dir = work_dir.join("repowise").join(format!("run-{started_at}"));
    fs::create_dir_all(&run_dir).with_context(|| format!("create {}", run_dir.display()))?;
    let _cleanup = CleanupGuard::new(run_dir.clone(), !options.keep_worktrees);

    verify_repowise_version(&options)?;

    let mut documents = Vec::new();
    for (index, case_file) in case_files.iter().enumerate() {
        let yaml = fs::read_to_string(case_file)
            .with_context(|| format!("read benchmark cases {}", case_file.display()))?;
        let document: BenchmarkDocument = serde_yaml::from_str(&yaml)
            .with_context(|| format!("deserialize benchmark cases {}", case_file.display()))?;
        let source = prepare_source_root(&document.source, &repo_root, &run_dir)?;
        let source_root = run_dir.join(format!("source-{index}"));
        copy_source_tree(&source, &source_root)?;
        index_source(&options, &source_root)?;
        let cases = run_document_cases(
            &options,
            &document,
            &source_root,
            options.include_unsupported,
        )
        .with_context(|| format!("run Repowise cases {}", case_file.display()))?;
        documents.push(DocumentRunReport {
            case_file: display_path(case_file),
            language: document.language,
            source_root: display_path(&source_root),
            cases,
        });
    }

    let mut report = RunReport {
        usagebench_version: env!("CARGO_PKG_VERSION").to_string(),
        runner: RunnerMetadata {
            name: "repowise".to_string(),
            requested_version: options.repowise_version.clone(),
            resolved_version: options.repowise_version.clone(),
            source: format!("https://pypi.org/project/repowise/{}/", options.repowise_version),
            adapter_version: env!("CARGO_PKG_VERSION").to_string(),
            capabilities: vec![
                RunnerCapability {
                    operation: RunnerOperation::DeclarationToUsages,
                    support: CapabilitySupport::Recovered,
                    notes: "call/heritage edges from get_context; exact tokens recovered inside caller symbol bodies"
                        .to_string(),
                },
                RunnerCapability {
                    operation: RunnerOperation::UsageToDeclaration,
                    support: CapabilitySupport::Recovered,
                    notes: "callee edges from the enclosing symbol; call/heritage references only"
                        .to_string(),
                },
                RunnerCapability {
                    operation: RunnerOperation::TypeLookup,
                    support: CapabilitySupport::Unsupported,
                    notes: "Repowise v0.31.0 has no source-location type query".to_string(),
                },
            ],
        },
        bifrost_repo: None,
        bifrost_commit: None,
        bifrost_resolved_commit: None,
        started_at_unix_seconds: started_at,
        finished_at_unix_seconds: unix_seconds_now()?,
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
    options: &RunRepowiseOptions,
    document: &BenchmarkDocument,
    source_root: &Path,
    include_unsupported: bool,
) -> Result<Vec<CaseRunReport>> {
    let mut command = repowise_command(options);
    command.arg("mcp").current_dir(source_root);
    let mut session = McpSession::start(&mut command, "Repowise")?;
    session.initialize()?;
    Ok(document
        .cases
        .iter()
        .map(|case| run_case(case, source_root, &mut session, include_unsupported))
        .collect())
}

fn run_case(
    case: &BenchmarkCase,
    source_root: &Path,
    session: &mut impl ToolClient,
    include_unsupported: bool,
) -> CaseRunReport {
    if let Some(unsupported) = &case.unsupported {
        if !include_unsupported {
            return CaseRunReport {
                id: case.id.clone(),
                status: CaseStatus::Unsupported,
                expected_failure_reason: None,
                not_planned_reason: case.not_planned.as_ref().map(|item| item.reason.clone()),
                unsupported_reason: Some(unsupported.reason.clone()),
                declaration_to_usages: None,
                usage_to_declaration: Vec::new(),
                type_lookups: Vec::new(),
                diagnostics: Vec::new(),
            };
        }
    }

    let mut diagnostics = Vec::new();
    if case.expected_failure.is_some() {
        diagnostics.push(RunDiagnostic {
            kind: "runner_specific_expectation_ignored".to_string(),
            message: "existing expectedFailure markers describe the Bifrost baseline and are not applied to Repowise"
                .to_string(),
        });
    }
    let declaration_to_usages = case.declaration.as_ref().map(|declaration| {
        run_declaration_to_usages(case, declaration, source_root, session, &mut diagnostics)
    });
    let usage_to_declaration = case
        .usage_lookups
        .iter()
        .map(|lookup| run_usage_to_declaration(lookup, source_root, session))
        .collect::<Vec<_>>();
    let type_lookups = case
        .type_lookups
        .iter()
        .map(unsupported_type_lookup)
        .collect::<Vec<_>>();

    let mut status = combine_case_status(
        declaration_to_usages.as_ref(),
        &usage_to_declaration,
        &type_lookups,
    );
    if status != CaseStatus::Error && case.not_planned.is_some() {
        status = CaseStatus::NotPlanned;
    }
    CaseRunReport {
        id: case.id.clone(),
        status,
        expected_failure_reason: None,
        not_planned_reason: case.not_planned.as_ref().map(|item| item.reason.clone()),
        unsupported_reason: case.unsupported.as_ref().map(|item| item.reason.clone()),
        declaration_to_usages,
        usage_to_declaration,
        type_lookups,
        diagnostics,
    }
}

fn run_declaration_to_usages(
    case: &BenchmarkCase,
    declaration: &SymbolLocation,
    source_root: &Path,
    session: &mut impl ToolClient,
    diagnostics: &mut Vec<RunDiagnostic>,
) -> DeclarationUsageReport {
    if !supports_call_graph_kind(&declaration.kind) {
        return unsupported_declaration_report(
            case,
            format!(
                "Repowise v0.31.0 does not expose exhaustive {} references",
                symbol_kind_name(&declaration.kind)
            ),
        );
    }
    let symbol = match resolve_symbol(session, declaration) {
        Ok(symbol) => symbol,
        Err(error) => {
            diagnostics.push(RunDiagnostic {
                kind: "symbol_resolution_failed".to_string(),
                message: format!("{error:#}"),
            });
            return failed_declaration_report(case, "symbol_resolution_failed");
        }
    };
    let context = match context_for_symbol(session, &symbol.symbol_id, "callers") {
        Ok(context) => context,
        Err(error) => {
            diagnostics.push(RunDiagnostic {
                kind: "repowise_callers_failed".to_string(),
                message: format!("{error:#}"),
            });
            return error_declaration_report(
                case,
                Some(symbol.symbol_id),
                "repowise_callers_failed",
            );
        }
    };
    let callers = context
        .get("callers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut actual = BTreeSet::new();
    for caller in callers {
        let Some(caller_id) = caller.get("symbol_id").and_then(Value::as_str) else {
            continue;
        };
        let edge_type = caller
            .get("edge_type")
            .and_then(Value::as_str)
            .unwrap_or("calls");
        match symbol_bounds(session, caller_id) {
            Ok(bounds) => recover_reference_locations(
                source_root,
                &bounds,
                &declaration.display_name,
                edge_type,
                &mut actual,
            ),
            Err(error) => diagnostics.push(RunDiagnostic {
                kind: "caller_bounds_failed".to_string(),
                message: format!("{caller_id}: {error:#}"),
            }),
        }
    }
    let partial = context
        .get("callers_truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let statuses = if partial {
        vec!["recovered_call_sites".to_string(), "partial".to_string()]
    } else {
        vec!["recovered_call_sites".to_string()]
    };
    score_declaration_locations(
        case,
        Some(symbol.symbol_id),
        actual.into_iter().collect(),
        Vec::new(),
        partial,
        statuses,
        false,
    )
    .unwrap_or_else(|error| {
        diagnostics.push(RunDiagnostic {
            kind: "invalid_expected_location".to_string(),
            message: format!("{error:#}"),
        });
        error_declaration_report(case, None, "invalid_expected_location")
    })
}

fn run_usage_to_declaration(
    lookup: &UsageLookup,
    source_root: &Path,
    session: &mut impl ToolClient,
) -> UsageDefinitionReport {
    let usage = normalized_or_invalid(&lookup.usage);
    let expected_declaration = normalized_or_invalid(&lookup.expected_declaration);
    if !supports_call_graph_kind(&lookup.expected_declaration.kind) {
        return UsageDefinitionReport {
            status: CaseStatus::Unsupported,
            usage,
            expected_declaration,
            actual_declarations: Vec::new(),
            raw_status: "unsupported_reference_kind".to_string(),
            diagnostics: vec![RunDiagnostic {
                kind: "unsupported_reference_kind".to_string(),
                message: format!(
                    "Repowise v0.31.0 does not expose exhaustive {} references",
                    symbol_kind_name(&lookup.expected_declaration.kind)
                ),
            }],
        };
    }
    let caller = match enclosing_symbol(session, source_root, &usage) {
        Ok(Some(caller)) => caller,
        Ok(None) => {
            return UsageDefinitionReport {
                status: CaseStatus::Failed,
                usage,
                expected_declaration,
                actual_declarations: Vec::new(),
                raw_status: "enclosing_symbol_not_found".to_string(),
                diagnostics: Vec::new(),
            }
        }
        Err(error) => {
            return UsageDefinitionReport {
                status: CaseStatus::Error,
                usage,
                expected_declaration,
                actual_declarations: Vec::new(),
                raw_status: "repowise_context_failed".to_string(),
                diagnostics: vec![RunDiagnostic {
                    kind: "repowise_context_failed".to_string(),
                    message: format!("{error:#}"),
                }],
            }
        }
    };
    let context = match context_for_symbol(session, &caller.symbol_id, "callees") {
        Ok(context) => context,
        Err(error) => {
            return UsageDefinitionReport {
                status: CaseStatus::Error,
                usage,
                expected_declaration,
                actual_declarations: Vec::new(),
                raw_status: "repowise_callees_failed".to_string(),
                diagnostics: vec![RunDiagnostic {
                    kind: "repowise_callees_failed".to_string(),
                    message: format!("{error:#}"),
                }],
            }
        }
    };
    let mut actual_declarations = context
        .get("callees")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|callee| {
            callee.get("name").and_then(Value::as_str) == Some(lookup.usage.display_name.as_str())
        })
        .filter_map(|callee| {
            Some(NormalizedLocation {
                path: callee.get("file")?.as_str()?.to_string(),
                line: callee.get("line")?.as_u64()? as u32,
                column: None,
                display_name: callee.get("name")?.as_str().map(str::to_string),
                kind: callee
                    .get("kind")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            })
        })
        .collect::<Vec<_>>();
    actual_declarations.sort();
    actual_declarations.dedup();
    let status = if actual_declarations.iter().any(|actual| {
        actual.path == expected_declaration.path && actual.line == expected_declaration.line
    }) {
        CaseStatus::Passed
    } else {
        CaseStatus::Failed
    };
    UsageDefinitionReport {
        status,
        usage,
        expected_declaration,
        actual_declarations,
        raw_status: "recovered_callee_edge".to_string(),
        diagnostics: Vec::new(),
    }
}

fn unsupported_type_lookup(lookup: &TypeLookup) -> TypeLookupReport {
    TypeLookupReport {
        status: CaseStatus::Unsupported,
        expression: normalized_or_invalid(&lookup.expression),
        expected_type: normalized_or_invalid(&lookup.expected_type),
        actual_types: Vec::new(),
        raw_status: "unsupported_operation".to_string(),
        diagnostics: vec![RunDiagnostic {
            kind: "unsupported_operation".to_string(),
            message: "Repowise v0.31.0 has no source-location type query".to_string(),
        }],
    }
}

fn resolve_symbol(
    session: &mut impl ToolClient,
    declaration: &SymbolLocation,
) -> Result<RepowiseSymbol> {
    let expected_path = path_to_slash(&crate::benchmark_source_path(&declaration.location.uri)?);
    let expected_line = declaration.location.range.start.line + 1;
    let result = repowise_result(session.call_tool(
        "search_codebase",
        json!({
            "query": declaration.display_name,
            "mode": "symbol",
            "limit": 100,
        }),
    )?)?;
    let candidates = result
        .get("results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| serde_json::from_value::<RepowiseSymbol>(value.clone()).ok())
        .filter(|symbol| {
            symbol.file == expected_path
                && symbol.start_line == expected_line
                && symbol.name == declaration.display_name
                && repowise_kind_matches(&symbol.kind, &declaration.kind)
        })
        .collect::<Vec<_>>();
    match candidates.as_slice() {
        [candidate] => Ok(candidate.clone()),
        [] => bail!(
            "no Repowise symbol matched {}:{} `{}` ({})",
            expected_path,
            expected_line,
            declaration.display_name,
            symbol_kind_name(&declaration.kind)
        ),
        candidates
            if declaration.disambiguation == Some(crate::Disambiguation::FirstMatchingSymbol) =>
        {
            Ok(candidates[0].clone())
        }
        _ => bail!(
            "multiple Repowise symbols matched {}:{} `{}`",
            expected_path,
            expected_line,
            declaration.display_name
        ),
    }
}

fn context_for_symbol(
    session: &mut impl ToolClient,
    symbol_id: &str,
    relationship: &str,
) -> Result<Value> {
    let result = repowise_result(session.call_tool(
        "get_context",
        json!({
            "targets": [symbol_id],
            "include": [relationship],
        }),
    )?)?;
    result
        .get("targets")
        .and_then(|targets| targets.get(symbol_id))
        .cloned()
        .with_context(|| format!("Repowise get_context omitted target {symbol_id}"))
}

fn enclosing_symbol(
    session: &mut impl ToolClient,
    _source_root: &Path,
    usage: &NormalizedLocation,
) -> Result<Option<SymbolBounds>> {
    let result =
        repowise_result(session.call_tool("get_context", json!({ "targets": [usage.path] }))?)?;
    let symbols = result
        .get("targets")
        .and_then(|targets| targets.get(&usage.path))
        .and_then(|target| target.get("docs"))
        .and_then(|docs| docs.get("symbols"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut enclosing = Vec::new();
    for symbol in symbols {
        let Some(symbol_id) = symbol.get("symbol_id").and_then(Value::as_str) else {
            continue;
        };
        let kind = symbol.get("kind").and_then(Value::as_str).unwrap_or("");
        if !matches!(
            kind,
            "function" | "method" | "class" | "struct" | "impl" | "interface" | "trait"
        ) {
            continue;
        }
        let bounds = symbol_bounds(session, symbol_id)?;
        if bounds.file == usage.path
            && bounds.start_line <= usage.line
            && usage.line <= bounds.end_line
        {
            enclosing.push(bounds);
        }
    }
    enclosing.sort_by_key(|bounds| bounds.end_line - bounds.start_line);
    Ok(enclosing.into_iter().next())
}

fn symbol_bounds(session: &mut impl ToolClient, symbol_id: &str) -> Result<SymbolBounds> {
    let result =
        repowise_result(session.call_tool("get_symbol", json!({ "symbol_id": symbol_id }))?)?;
    Ok(SymbolBounds {
        symbol_id: symbol_id.to_string(),
        file: required_str(&result, "file")?.to_string(),
        start_line: required_u32(&result, "symbol_start_line")?,
        end_line: required_u32(&result, "symbol_end_line")?,
    })
}

fn recover_reference_locations(
    source_root: &Path,
    bounds: &SymbolBounds,
    display_name: &str,
    edge_type: &str,
    output: &mut BTreeSet<NormalizedLocation>,
) {
    let Ok(source) = fs::read_to_string(source_root.join(&bounds.file)) else {
        return;
    };
    for (index, line) in source.lines().enumerate() {
        let line_number = index as u32 + 1;
        if line_number < bounds.start_line || line_number > bounds.end_line {
            continue;
        }
        for column in token_columns(line, display_name, edge_type == "calls") {
            output.insert(NormalizedLocation {
                path: bounds.file.clone(),
                line: line_number,
                column: Some(column as u32 + 1),
                display_name: Some(display_name.to_string()),
                kind: None,
            });
        }
    }
}

fn token_columns(line: &str, token: &str, require_call_syntax: bool) -> Vec<usize> {
    let mut columns = Vec::new();
    for (start, _) in line.match_indices(token) {
        let end = start + token.len();
        let before = line[..start].chars().next_back();
        let after = line[end..].chars().next();
        if before.is_some_and(is_identifier_char) || after.is_some_and(is_identifier_char) {
            continue;
        }
        if require_call_syntax {
            let suffix = line[end..].trim_start();
            if !(suffix.starts_with('(') || suffix.starts_with("::") || suffix.starts_with('!')) {
                continue;
            }
        }
        columns.push(start);
    }
    columns
}

fn is_identifier_char(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '_' | '$')
}

fn repowise_result(value: Value) -> Result<Value> {
    value
        .get("result")
        .cloned()
        .context("Repowise structuredContent missing result wrapper")
}

fn required_str<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .with_context(|| format!("Repowise response missing string `{field}`"))
}

fn required_u32(value: &Value, field: &str) -> Result<u32> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .map(|value| value as u32)
        .with_context(|| format!("Repowise response missing integer `{field}`"))
}

fn supports_call_graph_kind(kind: &SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Function
            | SymbolKind::Method
            | SymbolKind::Constructor
            | SymbolKind::Class
            | SymbolKind::Interface
            | SymbolKind::Type
    )
}

fn repowise_kind_matches(actual: &str, expected: &SymbolKind) -> bool {
    match expected {
        SymbolKind::Function => actual == "function",
        SymbolKind::Method | SymbolKind::Constructor => matches!(actual, "method" | "function"),
        SymbolKind::Class => matches!(actual, "class" | "struct" | "impl"),
        SymbolKind::Interface => matches!(actual, "interface" | "trait"),
        SymbolKind::Type => matches!(actual, "type" | "class" | "struct" | "enum" | "impl"),
        SymbolKind::Field | SymbolKind::Variable | SymbolKind::Constant | SymbolKind::Property => {
            matches!(actual, "field" | "variable" | "constant" | "property")
        }
        SymbolKind::Module | SymbolKind::Package => actual == "module",
    }
}

fn unsupported_declaration_report(case: &BenchmarkCase, reason: String) -> DeclarationUsageReport {
    let mut report = failed_declaration_report(case, "unsupported_reference_kind");
    report.status = CaseStatus::Unsupported;
    report.raw_statuses = vec![reason];
    report
}

fn failed_declaration_report(case: &BenchmarkCase, raw_status: &str) -> DeclarationUsageReport {
    score_declaration_locations(
        case,
        None,
        Vec::new(),
        Vec::new(),
        false,
        vec![raw_status.to_string()],
        true,
    )
    .unwrap_or_else(|_| empty_declaration_report(CaseStatus::Error, raw_status))
}

fn error_declaration_report(
    case: &BenchmarkCase,
    selector: Option<String>,
    raw_status: &str,
) -> DeclarationUsageReport {
    let mut report = failed_declaration_report(case, raw_status);
    report.status = CaseStatus::Error;
    report.selector = selector;
    report
}

fn empty_declaration_report(status: CaseStatus, raw_status: &str) -> DeclarationUsageReport {
    DeclarationUsageReport {
        status,
        selector: None,
        expected: Vec::new(),
        expected_unproven: Vec::new(),
        allowed_extra: Vec::new(),
        allowed_unproven: Vec::new(),
        actual: Vec::new(),
        unproven: Vec::new(),
        missing: Vec::new(),
        missing_unproven: Vec::new(),
        unexpected: Vec::new(),
        unexpected_unproven: Vec::new(),
        partial: false,
        raw_statuses: vec![raw_status.to_string()],
    }
}

fn normalized_or_invalid(location: &SymbolLocation) -> NormalizedLocation {
    normalize_symbol_location(location).unwrap_or_else(|_| NormalizedLocation {
        path: "<invalid>".to_string(),
        line: 0,
        column: None,
        display_name: Some(location.display_name.clone()),
        kind: Some(symbol_kind_name(&location.kind).to_string()),
    })
}

fn combine_case_status(
    declaration: Option<&DeclarationUsageReport>,
    definitions: &[UsageDefinitionReport],
    types: &[TypeLookupReport],
) -> CaseStatus {
    let statuses = declaration
        .into_iter()
        .map(|report| report.status)
        .chain(definitions.iter().map(|report| report.status))
        .chain(types.iter().map(|report| report.status))
        .collect::<Vec<_>>();
    if statuses.contains(&CaseStatus::Error) {
        CaseStatus::Error
    } else if statuses.contains(&CaseStatus::Failed) {
        CaseStatus::Failed
    } else if statuses.contains(&CaseStatus::Unsupported) {
        CaseStatus::Unsupported
    } else if statuses.is_empty() || statuses.iter().all(|status| *status == CaseStatus::Skipped) {
        CaseStatus::Skipped
    } else {
        CaseStatus::Passed
    }
}

fn verify_repowise_version(options: &RunRepowiseOptions) -> Result<()> {
    let mut command = repowise_command(options);
    command.arg("--version");
    let output = command_output_with_timeout(&mut command, COMMAND_TIMEOUT)
        .context("run Repowise --version")?;
    if !output.status.success() {
        bail!(
            "Repowise --version failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.trim().ends_with(&options.repowise_version) {
        bail!(
            "Repowise command reported `{}`; expected version {}",
            stdout.trim(),
            options.repowise_version
        );
    }
    Ok(())
}

fn index_source(options: &RunRepowiseOptions, source_root: &Path) -> Result<()> {
    let mut command = repowise_command(options);
    command
        .arg("init")
        .arg(source_root)
        .arg("--index-only")
        .arg("-y")
        .arg("--no-claude-md")
        .arg("--no-agents")
        .arg("--no-codex")
        .arg("--no-workspace");
    let output = command_output_with_timeout(&mut command, COMMAND_TIMEOUT)
        .with_context(|| format!("index {} with Repowise", source_root.display()))?;
    if !output.status.success() {
        bail!(
            "Repowise indexing failed for {}\nstdout:\n{}\nstderr:\n{}",
            source_root.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

fn repowise_command(options: &RunRepowiseOptions) -> Command {
    let mut command = if let Some(executable) = &options.repowise_command {
        Command::new(executable)
    } else {
        let mut command = Command::new("uvx");
        command
            .arg("--from")
            .arg(format!("repowise=={}", options.repowise_version))
            .arg("repowise");
        command
    };
    command
        .env("REPOWISE_SKIP_EDITOR_SETUP", "1")
        .env("DO_NOT_TRACK", "1");
    command
}

fn copy_source_tree(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination).with_context(|| format!("create {}", destination.display()))?;
    for entry in fs::read_dir(source).with_context(|| format!("read {}", source.display()))? {
        let entry = entry?;
        let name = entry.file_name();
        if matches!(name.to_str(), Some(".git" | ".repowise" | "target")) {
            continue;
        }
        let target = destination.join(&name);
        if entry.file_type()?.is_dir() {
            copy_source_tree(&entry.path(), &target)?;
        } else if entry.file_type()?.is_file() {
            fs::copy(entry.path(), &target).with_context(|| {
                format!("copy {} to {}", entry.path().display(), target.display())
            })?;
        }
    }
    Ok(())
}

fn unix_seconds_now() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time is before UNIX epoch")?
        .as_secs())
}

fn display_path(path: &Path) -> String {
    path.display().to_string()
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

#[derive(Debug, Clone, Deserialize)]
struct RepowiseSymbol {
    symbol_id: String,
    name: String,
    kind: String,
    file: String,
    start_line: u32,
}

#[derive(Debug, Clone)]
struct SymbolBounds {
    symbol_id: String,
    file: String,
    start_line: u32,
    end_line: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovers_only_complete_call_tokens() {
        assert_eq!(
            token_columns("let x = build_service(repo);", "build_service", true),
            vec![8]
        );
        assert!(token_columns("let build_service_name = 1;", "build_service", true).is_empty());
        assert!(token_columns("// build_service is useful", "build_service", true).is_empty());
    }

    #[test]
    fn rejects_unverified_repowise_versions() {
        let mut options = RunRepowiseOptions::with_defaults(PathBuf::from("cases"));
        options.repowise_version = "0.32.0".to_string();

        let error = run_repowise(options).unwrap_err();

        assert!(format!("{error:#}").contains("supports exactly version 0.31.0"));
    }

    #[test]
    fn unwraps_repowise_structured_content() {
        let value = repowise_result(json!({ "result": { "results": [] } })).unwrap();

        assert_eq!(value["results"], json!([]));
    }
}
