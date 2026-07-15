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
const TRACE_HOOK: &str = include_str!("repowise/sitecustomize.py");

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
    let hook_dir = run_dir.join("trace-hook");
    fs::create_dir_all(&hook_dir).with_context(|| format!("create {}", hook_dir.display()))?;
    fs::write(hook_dir.join("sitecustomize.py"), TRACE_HOOK)
        .with_context(|| format!("write Repowise trace hook under {}", hook_dir.display()))?;

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
        let trace_path = run_dir.join(format!("calls-{index}.jsonl"));
        index_source(&options, &source_root, &hook_dir, &trace_path)?;
        let call_trace = load_call_trace(&trace_path)?;
        let cases = run_document_cases(
            &options,
            &document,
            &source_root,
            &call_trace,
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
                    notes: "individual call sites captured from Repowise's pinned CallResolver before graph-edge folding; non-call references are unsupported"
                        .to_string(),
                },
                RunnerCapability {
                    operation: RunnerOperation::UsageToDeclaration,
                    support: CapabilitySupport::Recovered,
                    notes: "resolved CallResolver targets for exact call-site lines; non-call references are unsupported"
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
    call_trace: &[CallTraceRecord],
    include_unsupported: bool,
) -> Result<Vec<CaseRunReport>> {
    let mut command = repowise_command(options);
    command.arg("mcp").current_dir(source_root);
    let mut session = McpSession::start(&mut command, "Repowise")?;
    session.initialize()?;
    Ok(document
        .cases
        .iter()
        .map(|case| {
            run_case(
                case,
                source_root,
                call_trace,
                &mut session,
                include_unsupported,
            )
        })
        .collect())
}

fn run_case(
    case: &BenchmarkCase,
    source_root: &Path,
    call_trace: &[CallTraceRecord],
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
        run_declaration_to_usages(
            case,
            declaration,
            source_root,
            call_trace,
            session,
            &mut diagnostics,
        )
    });
    let usage_to_declaration = case
        .usage_lookups
        .iter()
        .map(|lookup| run_usage_to_declaration(lookup, source_root, call_trace, session))
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
    call_trace: &[CallTraceRecord],
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
    match expected_usages_are_calls(case, source_root, call_trace) {
        Ok(true) => {}
        Ok(false) => {
            return unsupported_declaration_report(
                case,
                "Repowise's call resolver does not cover one or more expected non-call references"
                    .to_string(),
            );
        }
        Err(error) => {
            diagnostics.push(RunDiagnostic {
                kind: "classify_expected_usage_failed".to_string(),
                message: format!("{error:#}"),
            });
            return error_declaration_report(case, None, "classify_expected_usage_failed");
        }
    }
    let selector = match resolve_symbol(session, declaration) {
        Ok(symbol) => symbol.symbol_id,
        Err(error) => match trace_selector(declaration, call_trace) {
            Ok(selector) => {
                diagnostics.push(RunDiagnostic {
                        kind: "trace_selector_fallback".to_string(),
                        message: format!(
                            "public symbol lookup did not match the authored declaration; using the unique resolved-call target `{selector}`: {error:#}"
                        ),
                    });
                selector
            }
            Err(trace_error) => {
                diagnostics.push(RunDiagnostic {
                    kind: "symbol_resolution_failed".to_string(),
                    message: format!("{error:#}; trace fallback: {trace_error:#}"),
                });
                return failed_declaration_report(case, "symbol_resolution_failed");
            }
        },
    };
    let mut actual = BTreeSet::new();
    let mut unproven = BTreeSet::new();
    let mut location_failed = false;
    for call in call_trace
        .iter()
        .filter(|call| call.callee_id.as_deref() == Some(&selector))
    {
        match trace_location(source_root, call, &declaration.display_name) {
            Ok(location) if call.confidence.unwrap_or_default() >= 0.7 => {
                actual.insert(location);
            }
            Ok(location) => {
                unproven.insert(location);
            }
            Err(error) => {
                location_failed = true;
                diagnostics.push(RunDiagnostic {
                    kind: "call_trace_location_failed".to_string(),
                    message: format!("{error:#}"),
                });
            }
        }
    }
    let mut statuses = vec!["resolved_call_trace".to_string()];
    if !unproven.is_empty() {
        statuses.push("low_confidence_calls".to_string());
    }
    score_declaration_locations(
        case,
        Some(selector),
        actual.into_iter().collect(),
        unproven.into_iter().collect(),
        false,
        statuses,
        location_failed,
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
    call_trace: &[CallTraceRecord],
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
    match usage_is_call(&lookup.usage, source_root, call_trace) {
        Ok(true) => {}
        Ok(false) => {
            return UsageDefinitionReport {
                status: CaseStatus::Unsupported,
                usage,
                expected_declaration,
                actual_declarations: Vec::new(),
                raw_status: "unsupported_non_call_reference".to_string(),
                diagnostics: vec![RunDiagnostic {
                    kind: "unsupported_non_call_reference".to_string(),
                    message: "Repowise's CallResolver only resolves call sites".to_string(),
                }],
            }
        }
        Err(error) => {
            return UsageDefinitionReport {
                status: CaseStatus::Error,
                usage,
                expected_declaration,
                actual_declarations: Vec::new(),
                raw_status: "classify_usage_failed".to_string(),
                diagnostics: vec![RunDiagnostic {
                    kind: "classify_usage_failed".to_string(),
                    message: format!("{error:#}"),
                }],
            }
        }
    }
    let matching_calls = match matching_trace_calls(&lookup.usage, call_trace) {
        Ok(calls) => calls,
        Err(error) => {
            return UsageDefinitionReport {
                status: CaseStatus::Error,
                usage,
                expected_declaration,
                actual_declarations: Vec::new(),
                raw_status: "match_call_trace_failed".to_string(),
                diagnostics: vec![RunDiagnostic {
                    kind: "match_call_trace_failed".to_string(),
                    message: format!("{error:#}"),
                }],
            }
        }
    };
    if matching_calls.is_empty() {
        return UsageDefinitionReport {
            status: CaseStatus::Failed,
            usage,
            expected_declaration,
            actual_declarations: Vec::new(),
            raw_status: "call_site_not_extracted".to_string(),
            diagnostics: Vec::new(),
        };
    }
    let mut actual_declarations = Vec::new();
    let mut diagnostics = Vec::new();
    let mut query_failed = false;
    let mut low_confidence = false;
    for call in matching_calls {
        let Some(callee_id) = &call.callee_id else {
            continue;
        };
        if call.confidence.unwrap_or_default() < 0.7 {
            low_confidence = true;
        }
        match symbol_location(session, callee_id) {
            Ok(location) => actual_declarations.push(location),
            Err(error) => {
                query_failed = true;
                diagnostics.push(RunDiagnostic {
                    kind: "callee_location_failed".to_string(),
                    message: format!("{callee_id}: {error:#}"),
                });
            }
        }
    }
    actual_declarations.sort();
    actual_declarations.dedup();
    let status = if query_failed {
        CaseStatus::Error
    } else if low_confidence {
        CaseStatus::Failed
    } else if actual_declarations.iter().any(|actual| {
        actual.path == expected_declaration.path && actual.line == expected_declaration.line
    }) {
        CaseStatus::Passed
    } else {
        CaseStatus::Failed
    };
    let raw_status = if actual_declarations.is_empty() {
        "unresolved_call"
    } else if low_confidence {
        "low_confidence_resolved_call"
    } else {
        "resolved_call_trace"
    };
    UsageDefinitionReport {
        status,
        usage,
        expected_declaration,
        actual_declarations,
        raw_status: raw_status.to_string(),
        diagnostics,
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

fn expected_usages_are_calls(
    case: &BenchmarkCase,
    source_root: &Path,
    call_trace: &[CallTraceRecord],
) -> Result<bool> {
    for usage in case
        .expected_usages
        .iter()
        .chain(&case.expected_unproven_usages)
    {
        if !usage_is_call(usage, source_root, call_trace)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn usage_is_call(
    usage: &SymbolLocation,
    source_root: &Path,
    call_trace: &[CallTraceRecord],
) -> Result<bool> {
    if source_usage_looks_call_like(usage, source_root)? {
        return Ok(true);
    }
    if matching_trace_calls(usage, call_trace)?.is_empty() {
        return Ok(false);
    }
    let (line, _) = source_line_and_token_start(usage, source_root)?;
    Ok(token_columns(&line, &usage.display_name, false).len() == 1)
}

fn matching_trace_calls<'a>(
    usage: &SymbolLocation,
    call_trace: &'a [CallTraceRecord],
) -> Result<Vec<&'a CallTraceRecord>> {
    let expected_path = path_to_slash(&crate::benchmark_source_path(&usage.location.uri)?);
    let expected_line = usage.location.range.start.line + 1;
    Ok(call_trace
        .iter()
        .filter(|call| {
            call.source_file == expected_path
                && call.line == expected_line
                && call.target_name == usage.display_name
        })
        .collect())
}

fn source_usage_looks_call_like(usage: &SymbolLocation, source_root: &Path) -> Result<bool> {
    let (line, start) = source_line_and_token_start(usage, source_root)?;
    let suffix = line[start + usage.display_name.len()..].trim_start();
    Ok(is_direct_call_suffix(suffix))
}

fn source_line_and_token_start(
    usage: &SymbolLocation,
    source_root: &Path,
) -> Result<(String, usize)> {
    let path = crate::benchmark_source_path(&usage.location.uri)?;
    let source = fs::read_to_string(source_root.join(&path))
        .with_context(|| format!("read expected usage source {}", path.display()))?;
    let line = source
        .lines()
        .nth(usage.location.range.start.line as usize)
        .with_context(|| {
            format!(
                "expected usage line {} is outside {}",
                usage.location.range.start.line + 1,
                path.display()
            )
        })?;
    let hinted_column = usage.location.range.start.character as usize;
    let exact_start = line
        .get(hinted_column..)
        .is_some_and(|suffix| suffix.starts_with(&usage.display_name))
        .then_some(hinted_column);
    let Some(start) = exact_start.or_else(|| {
        line.match_indices(&usage.display_name)
            .map(|(start, _)| start)
            .min_by_key(|start| start.abs_diff(hinted_column))
    }) else {
        bail!(
            "expected usage token `{}` was not found on {} line {}",
            usage.display_name,
            path.display(),
            usage.location.range.start.line + 1
        );
    };
    Ok((line.to_string(), start))
}

fn is_direct_call_suffix(suffix: &str) -> bool {
    suffix.starts_with('(')
        || suffix.starts_with('{')
        || suffix.starts_with('!')
        || suffix.starts_with("::")
}

fn trace_selector(declaration: &SymbolLocation, call_trace: &[CallTraceRecord]) -> Result<String> {
    let expected_path = path_to_slash(&crate::benchmark_source_path(&declaration.location.uri)?);
    let candidates = call_trace
        .iter()
        .filter(|call| call.target_name == declaration.display_name)
        .filter_map(|call| call.callee_id.as_deref())
        .filter(|callee_id| callee_file(callee_id) == expected_path)
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    match candidates.len() {
        1 => Ok(candidates.into_iter().next().expect("one candidate")),
        0 => bail!(
            "no resolved call target matched {} `{}`",
            expected_path,
            declaration.display_name
        ),
        count => bail!(
            "{count} resolved call targets matched {} `{}`: {}",
            expected_path,
            declaration.display_name,
            candidates.into_iter().collect::<Vec<_>>().join(", ")
        ),
    }
}

fn callee_file(callee_id: &str) -> &str {
    callee_id
        .split_once("::")
        .map_or(callee_id, |(path, _)| path)
}

fn trace_location(
    source_root: &Path,
    call: &CallTraceRecord,
    display_name: &str,
) -> Result<NormalizedLocation> {
    let source = fs::read_to_string(source_root.join(&call.source_file))
        .with_context(|| format!("read traced call source {}", call.source_file))?;
    let line = source
        .lines()
        .nth(call.line.saturating_sub(1) as usize)
        .with_context(|| {
            format!(
                "traced call line {} is outside {}",
                call.line, call.source_file
            )
        })?;
    let columns = token_columns(line, display_name, false);
    let call_columns = columns
        .iter()
        .copied()
        .filter(|column| is_direct_call_suffix(line[*column + display_name.len()..].trim_start()))
        .collect::<Vec<_>>();
    let column = call_columns
        .into_iter()
        .next()
        .or_else(|| (columns.len() == 1).then(|| columns[0]))
        .map(|column| column as u32 + 1);
    Ok(NormalizedLocation {
        path: call.source_file.clone(),
        line: call.line,
        column,
        display_name: Some(display_name.to_string()),
        kind: None,
    })
}

fn symbol_location(session: &mut impl ToolClient, symbol_id: &str) -> Result<NormalizedLocation> {
    let result =
        repowise_result(session.call_tool("get_symbol", json!({ "symbol_id": symbol_id }))?)?;
    Ok(NormalizedLocation {
        path: required_str(&result, "file")?.to_string(),
        line: required_u32(&result, "symbol_start_line")?,
        column: None,
        display_name: result
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string),
        kind: result
            .get("kind")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
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
        SymbolKind::Function | SymbolKind::Method | SymbolKind::Constructor | SymbolKind::Class
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

fn index_source(
    options: &RunRepowiseOptions,
    source_root: &Path,
    hook_dir: &Path,
    trace_path: &Path,
) -> Result<()> {
    let mut command = repowise_command(options);
    command
        .env("PYTHONPATH", hook_dir)
        .env("USAGEBENCH_REPOWISE_CALL_TRACE", trace_path)
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

fn load_call_trace(path: &Path) -> Result<Vec<CallTraceRecord>> {
    let raw = fs::read_to_string(path).with_context(|| {
        format!(
            "read Repowise call trace {}; the selected command may not load Python sitecustomize hooks",
            path.display()
        )
    })?;
    let mut ready = false;
    let mut calls = Vec::new();
    for (index, line) in raw.lines().enumerate() {
        let value: Value = serde_json::from_str(line).with_context(|| {
            format!(
                "parse Repowise call trace {} line {}",
                path.display(),
                index + 1
            )
        })?;
        match value.get("record_type").and_then(Value::as_str) {
            Some("trace_ready") => ready = true,
            Some("call_site") => calls.push(serde_json::from_value(value).with_context(|| {
                format!(
                    "decode Repowise call trace {} line {}",
                    path.display(),
                    index + 1
                )
            })?),
            Some(other) => bail!(
                "unsupported Repowise call trace record `{other}` in {} line {}",
                path.display(),
                index + 1
            ),
            None => bail!(
                "Repowise call trace record missing record_type in {} line {}",
                path.display(),
                index + 1
            ),
        }
    }
    if !ready {
        bail!(
            "Repowise call trace {} did not contain the trace_ready marker",
            path.display()
        );
    }
    Ok(calls)
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

#[derive(Debug, Clone, Deserialize)]
struct CallTraceRecord {
    source_file: String,
    target_name: String,
    line: u32,
    callee_id: Option<String>,
    confidence: Option<f64>,
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
    fn traced_constructor_prefers_call_token_over_type_annotation() {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("Consumer.cs"),
            "Service service = new Service(repository);\n",
        )
        .unwrap();
        let call = CallTraceRecord {
            source_file: "Consumer.cs".to_string(),
            target_name: "Service".to_string(),
            line: 1,
            callee_id: Some("Service.cs::Service".to_string()),
            confidence: Some(0.95),
        };

        let location = trace_location(root.path(), &call, "Service").unwrap();

        assert_eq!(location.column, Some(23));
    }

    #[test]
    fn reads_ready_call_trace_and_ignores_marker() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("calls.jsonl");
        fs::write(
            &path,
            concat!(
                "{\"record_type\":\"trace_ready\",\"schema_version\":1}\n",
                "{\"record_type\":\"call_site\",\"source_file\":\"src/lib.rs\",",
                "\"target_name\":\"run\",\"line\":4,\"callee_id\":\"src/run.rs::run\",",
                "\"confidence\":0.9}\n"
            ),
        )
        .unwrap();

        let trace = load_call_trace(&path).unwrap();

        assert_eq!(trace.len(), 1);
        assert_eq!(trace[0].target_name, "run");
        assert_eq!(trace[0].line, 4);
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
