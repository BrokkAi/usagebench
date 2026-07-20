use super::bifrost::{command_output_with_timeout, prepare_source_root};
use super::lsp_protocol::{InitializeResult, LspSession};
use super::{
    combine_case_status, compute_totals, location_match, navigation_response_status,
    normalize_symbol_location, path_to_slash, score_declaration_locations,
    score_navigation_response, symbol_kind_name, CapabilitySupport, CaseRunReport, CaseStatus,
    ClassifiedExtraUsage, DeclarationUsageReport, DocumentRunReport, ExtraUsageClassification,
    ExtraUsageDisposition, LocationMatch, NormalizedLocation, RunDiagnostic, RunReport, RunTotals,
    RunnerCapability, RunnerMetadata, RunnerOperation, TypeLookupReport, UsageDefinitionReport,
};
use crate::{
    benchmark_source_path, find_repo_root_for_path, BenchmarkCase, BenchmarkDocument,
    NavigationOperation, ReferencePolicy, SymbolLocation, TypeLookup, UsageLookup,
};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use url::Url;

#[derive(Debug, Clone)]
pub struct RunLspOptions {
    pub case_path: PathBuf,
    pub profile: PathBuf,
    pub server_command: Option<PathBuf>,
    pub work_dir: PathBuf,
    pub output: Option<PathBuf>,
    pub include_unsupported: bool,
    pub keep_worktrees: bool,
    pub case_id: Option<String>,
}

impl RunLspOptions {
    pub fn with_defaults(case_path: PathBuf, profile: PathBuf) -> Self {
        Self {
            case_path,
            profile,
            server_command: None,
            work_dir: PathBuf::from("target/usagebench"),
            output: None,
            include_unsupported: false,
            keep_worktrees: false,
            case_id: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LspProfile {
    id: String,
    name: String,
    languages: Vec<String>,
    requested_version: String,
    source: String,
    command: Vec<String>,
    file_extensions: Vec<String>,
    language_ids: BTreeMap<String, String>,
    #[serde(default)]
    initialization_options: Value,
    #[serde(default)]
    client_capabilities: Value,
    #[serde(default)]
    configuration: Value,
    #[serde(default)]
    environment: BTreeMap<String, String>,
    #[serde(default)]
    workspace_files: BTreeMap<String, String>,
    #[serde(default)]
    post_initialize_notifications: Vec<ProfileNotification>,
    readiness_notification: Option<String>,
    readiness_timeout_milliseconds: Option<u64>,
    project_context_request: Option<String>,
    #[serde(default)]
    query_declaration: bool,
    #[serde(default)]
    accept_first_action_requests: bool,
    #[serde(default)]
    prepare_command: Vec<String>,
    prepare_timeout_milliseconds: Option<u64>,
    #[serde(default)]
    settle_milliseconds: u64,
    #[serde(default)]
    generate_compile_commands: bool,
    request_timeout_milliseconds: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileNotification {
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Clone, Default)]
struct ServerCapabilities {
    references: bool,
    declaration: bool,
    definition: bool,
    type_definition: bool,
}

struct CaseRunContext<'a> {
    language: &'a str,
    reference_policy: ReferencePolicy,
    profile: &'a LspProfile,
    source_root: &'a Path,
    capabilities: &'a ServerCapabilities,
}

pub fn run_lsp(options: RunLspOptions) -> Result<RunReport> {
    let started_at = unix_seconds_now()?;
    let repo_root = find_repo_root_for_path(&options.case_path)?;
    let usagebench_provenance = super::resolve_usagebench_provenance(&repo_root)?;
    let profile_path = if options.profile.is_absolute() {
        options.profile.clone()
    } else {
        repo_root.join(&options.profile)
    };
    let profile = load_profile(&profile_path)?;
    validate_profile(&profile)?;
    let case_files = crate::validate_path(&options.case_path)?;
    let work_dir = if options.work_dir.is_absolute() {
        options.work_dir.clone()
    } else {
        repo_root.join(&options.work_dir)
    };
    let run_dir = work_dir
        .join("lsp")
        .join(&profile.id)
        .join(format!("run-{started_at}"));
    fs::create_dir_all(&run_dir).with_context(|| format!("create {}", run_dir.display()))?;
    let _cleanup = CleanupGuard::new(run_dir.clone(), !options.keep_worktrees);

    let mut documents = Vec::new();
    let mut observed_name = None;
    let mut observed_version = None;
    let mut observed_capabilities = None;
    let mut executed_case_files = Vec::new();
    for (index, case_file) in case_files.iter().enumerate() {
        let yaml = fs::read_to_string(case_file)
            .with_context(|| format!("read benchmark cases {}", case_file.display()))?;
        let document: BenchmarkDocument = serde_yaml::from_str(&yaml)
            .with_context(|| format!("deserialize benchmark cases {}", case_file.display()))?;
        if !profile.languages.contains(&document.language) {
            continue;
        }
        executed_case_files.push(display_path(case_file));

        let source = prepare_source_root(&document.source, &repo_root, &run_dir)?;
        let source_root = run_dir.join(format!("source-{index}"));
        copy_source_tree(&source, &source_root)?;
        write_workspace_files(&profile, &source_root)?;
        match run_document(&options, &profile, &document, &source_root, &run_dir) {
            Ok((cases, initialize)) => {
                observed_name = observed_name.or(initialize.server_name);
                observed_version = observed_version.or_else(|| {
                    initialize
                        .server_version
                        .as_deref()
                        .map(normalize_server_version)
                });
                observed_capabilities
                    .get_or_insert_with(|| capabilities_from_initialize(&initialize.capabilities));
                documents.push(DocumentRunReport {
                    case_file: display_path(case_file),
                    language: document.language,
                    source_root: display_path(&source_root),
                    corpus_partition: document.corpus.partition,
                    corpus_selection: document.corpus.selection,
                    ground_truth_status: document.ground_truth.status,
                    reference_policy: document.reference_policy,
                    cases,
                });
            }
            Err(error) => {
                let message = format!("{error:#}");
                documents.push(DocumentRunReport {
                    case_file: display_path(case_file),
                    language: document.language,
                    source_root: display_path(&source_root),
                    corpus_partition: document.corpus.partition,
                    corpus_selection: document.corpus.selection,
                    ground_truth_status: document.ground_truth.status,
                    reference_policy: document.reference_policy,
                    cases: document
                        .cases
                        .iter()
                        .map(|case| error_case(case, "lsp_session_failed", &message))
                        .collect(),
                });
            }
        }
    }

    let capabilities = observed_capabilities.unwrap_or_default();
    let resolved_version = observed_version.unwrap_or_else(|| "not reported".to_string());
    let runner_name = observed_name.unwrap_or(profile.name.clone());
    let mut report = RunReport {
        usagebench_version: env!("CARGO_PKG_VERSION").to_string(),
        usagebench_revision: usagebench_provenance.revision,
        usagebench_release: usagebench_provenance.release,
        runner: RunnerMetadata {
            name: runner_name,
            requested_version: profile.requested_version.clone(),
            resolved_version,
            source: profile.source.clone(),
            adapter_version: env!("CARGO_PKG_VERSION").to_string(),
            capabilities: vec![
                capability(
                    RunnerOperation::DeclarationToUsages,
                    capabilities.references,
                    "textDocument/references with includeDeclaration=false",
                ),
                capability(
                    RunnerOperation::DeclarationLookup,
                    capabilities.declaration,
                    "textDocument/declaration",
                ),
                capability(
                    RunnerOperation::DefinitionLookup,
                    capabilities.definition,
                    "textDocument/definition",
                ),
                capability(
                    RunnerOperation::TypeLookup,
                    capabilities.type_definition,
                    "textDocument/typeDefinition",
                ),
            ],
        },
        bifrost_repo: None,
        bifrost_commit: None,
        bifrost_resolved_commit: None,
        started_at_unix_seconds: started_at,
        finished_at_unix_seconds: unix_seconds_now()?,
        case_files: executed_case_files,
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

fn run_document(
    options: &RunLspOptions,
    profile: &LspProfile,
    document: &BenchmarkDocument,
    source_root: &Path,
    run_dir: &Path,
) -> Result<(Vec<CaseRunReport>, InitializeResult)> {
    run_prepare_command(profile, source_root, run_dir)?;
    let workspace_uri = Url::from_directory_path(source_root)
        .map_err(|_| anyhow::anyhow!("convert {} to file URI", source_root.display()))?
        .to_string();
    let mut command = lsp_command(options, profile, source_root, run_dir)?;
    let mut session = LspSession::start(
        &mut command,
        &profile.name,
        workspace_uri.clone(),
        profile.configuration.clone(),
        profile.accept_first_action_requests,
        std::time::Duration::from_millis(profile.request_timeout_milliseconds.unwrap_or(60_000)),
    )?;
    let initialize = session.initialize(
        std::process::id(),
        source_root,
        &profile.initialization_options,
        &profile.client_capabilities,
    )?;
    send_profile_notifications(profile, source_root, run_dir, &workspace_uri, &mut session)?;
    if let Some(notification) = &profile.readiness_notification {
        session.wait_for_notification(
            notification,
            Duration::from_millis(profile.readiness_timeout_milliseconds.unwrap_or(120_000)),
        )?;
    }
    let capabilities = capabilities_from_initialize(&initialize.capabilities);
    let context = CaseRunContext {
        language: &document.language,
        reference_policy: document.reference_policy,
        profile,
        source_root,
        capabilities: &capabilities,
    };
    open_source_files(profile, source_root, &mut session)?;
    if profile.settle_milliseconds > 0 {
        session.pump_for(Duration::from_millis(profile.settle_milliseconds))?;
    }
    let cases = document
        .cases
        .iter()
        .filter(|case| {
            options
                .case_id
                .as_ref()
                .is_none_or(|case_id| case.id == *case_id)
        })
        .map(|case| run_case(case, &context, &mut session, options.include_unsupported))
        .collect();
    Ok((cases, initialize))
}

fn run_case(
    case: &BenchmarkCase,
    context: &CaseRunContext<'_>,
    session: &mut LspSession,
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
            message: "existing expectedFailure markers describe the Bifrost baseline and are not applied to LSP servers"
                .to_string(),
        });
    }
    let usage_to_declaration = case
        .usage_lookups
        .iter()
        .map(|lookup| {
            run_definition(
                lookup,
                context.profile,
                context.source_root,
                context.capabilities,
                session,
            )
        })
        .collect::<Vec<_>>();
    let type_lookups = case
        .type_lookups
        .iter()
        .map(|lookup| {
            run_type_definition(
                lookup,
                context.profile,
                context.source_root,
                context.capabilities.type_definition,
                session,
            )
        })
        .collect::<Vec<_>>();
    let declaration_to_usages = case
        .declaration
        .as_ref()
        .map(|declaration| run_references(case, declaration, context, session));
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

fn run_references(
    case: &BenchmarkCase,
    declaration: &SymbolLocation,
    context: &CaseRunContext<'_>,
    session: &mut LspSession,
) -> DeclarationUsageReport {
    if !context.capabilities.references {
        let mut report = failed_declaration_report(case, "references_not_advertised");
        report.status = CaseStatus::Unsupported;
        return report;
    }
    if declaration.location.range.start == declaration.location.range.end {
        let mut report = failed_declaration_report(case, "lsp_selector_has_no_source_token");
        report.status = CaseStatus::Unsupported;
        return report;
    }
    let params = match position_params_with_context(
        declaration,
        context.profile,
        context.source_root,
        session,
    ) {
        Ok(mut params) => {
            params["context"] = json!({"includeDeclaration": false});
            params
        }
        Err(error) => return error_declaration_report(case, "invalid_declaration", error),
    };
    match session
        .query("textDocument/references", params)
        .and_then(|result| locations_from_response(&result, context.source_root))
    {
        Ok(actual) => {
            let mut report = score_declaration_locations(
                case,
                Some(format!(
                    "{}:{}:{}",
                    benchmark_path(declaration),
                    declaration.location.range.start.line,
                    declaration.location.range.start.character
                )),
                actual,
                Vec::new(),
                false,
                vec!["ok".to_string()],
                false,
            )
            .unwrap_or_else(|error| error_declaration_report(case, "score_failed", error));
            classify_reference_policy_extras(
                &mut report,
                context.language,
                context.source_root,
                context.reference_policy,
            );
            report
        }
        Err(error) => error_declaration_report(case, "references_failed", error),
    }
}

fn run_definition(
    lookup: &UsageLookup,
    profile: &LspProfile,
    source_root: &Path,
    capabilities: &ServerCapabilities,
    session: &mut LspSession,
) -> UsageDefinitionReport {
    let Some(method) = navigation_method(lookup.operation, profile, capabilities) else {
        let reason = match lookup.operation {
            NavigationOperation::Declaration => "declaration_not_advertised",
            NavigationOperation::Definition => "definition_not_advertised",
            NavigationOperation::ProfileDefault => "profile_navigation_operation_not_advertised",
        };
        return unsupported_definition_report(lookup, reason);
    };
    let expected = normalized_or_invalid(&lookup.expected_declaration);
    let usage = normalized_or_invalid(&lookup.usage);
    let result = position_params_with_context(&lookup.usage, profile, source_root, session)
        .and_then(|params| session.query(method, params))
        .and_then(|value| locations_from_response(&value, source_root));
    match result {
        Ok(actual) => {
            let (status, raw_status) =
                score_navigation_response(&actual, &expected, lookup.expect_no_movement);
            UsageDefinitionReport {
                status,
                operation: lookup.operation,
                usage,
                expected_declaration: expected.clone(),
                raw_status: raw_status.to_string(),
                actual_declarations: actual,
                diagnostics: Vec::new(),
            }
        }
        Err(error) => UsageDefinitionReport {
            status: CaseStatus::Error,
            operation: lookup.operation,
            usage,
            expected_declaration: expected,
            actual_declarations: Vec::new(),
            raw_status: "definition_failed".to_string(),
            diagnostics: vec![RunDiagnostic {
                kind: "definition_failed".to_string(),
                message: format!("{error:#}"),
            }],
        },
    }
}

fn run_type_definition(
    lookup: &TypeLookup,
    profile: &LspProfile,
    source_root: &Path,
    supported: bool,
    session: &mut LspSession,
) -> TypeLookupReport {
    let expression = normalized_or_invalid(&lookup.expression);
    let expected_type = normalized_or_invalid(&lookup.expected_type);
    if !supported {
        return TypeLookupReport {
            status: CaseStatus::Unsupported,
            expression,
            expected_type,
            actual_types: Vec::new(),
            raw_status: "type_definition_not_advertised".to_string(),
            diagnostics: Vec::new(),
        };
    }
    let result = position_params_with_context(&lookup.expression, profile, source_root, session)
        .and_then(|params| session.query("textDocument/typeDefinition", params))
        .and_then(|value| locations_from_response(&value, source_root));
    match result {
        Ok(actual) => {
            let status = navigation_response_status(&actual, &expected_type, false);
            TypeLookupReport {
                status,
                expression,
                expected_type: expected_type.clone(),
                raw_status: if actual.is_empty() {
                    "no_type_definition".to_string()
                } else if status == CaseStatus::Passed {
                    "ok".to_string()
                } else if status == CaseStatus::PositionUnverified {
                    "position_unverified".to_string()
                } else if actual.len() > 1
                    && actual.iter().any(|location| {
                        location_match(location, &expected_type) == LocationMatch::Exact
                    })
                {
                    "unexpected_alternate_types".to_string()
                } else {
                    "mismatched_type_definition".to_string()
                },
                actual_types: actual,
                diagnostics: Vec::new(),
            }
        }
        Err(error) => TypeLookupReport {
            status: CaseStatus::Error,
            expression,
            expected_type,
            actual_types: Vec::new(),
            raw_status: "type_definition_failed".to_string(),
            diagnostics: vec![RunDiagnostic {
                kind: "type_definition_failed".to_string(),
                message: format!("{error:#}"),
            }],
        },
    }
}

fn position_params(location: &SymbolLocation, source_root: &Path) -> Result<Value> {
    let relative = benchmark_source_path(&location.location.uri)?;
    let absolute = source_root.join(relative);
    let uri = Url::from_file_path(&absolute)
        .map_err(|_| anyhow::anyhow!("convert {} to file URI", absolute.display()))?;
    Ok(json!({
        "textDocument": {"uri": uri},
        "position": {
            "line": location.location.range.start.line,
            "character": location.location.range.start.character
        }
    }))
}

fn position_params_with_context(
    location: &SymbolLocation,
    profile: &LspProfile,
    source_root: &Path,
    session: &mut LspSession,
) -> Result<Value> {
    let mut params = position_params(location, source_root)?;
    let Some(method) = &profile.project_context_request else {
        return Ok(params);
    };
    let text_document = params
        .get("textDocument")
        .cloned()
        .context("position params missing text document")?;
    let contexts = session.query(method, json!({"_vs_textDocument": text_document}))?;
    let default_index = contexts
        .get("_vs_defaultIndex")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let context = contexts
        .get("_vs_projectContexts")
        .and_then(Value::as_array)
        .and_then(|values| values.get(default_index))
        .cloned()
        .context("project context response has no default context")?;
    params["textDocument"]["_vs_projectContext"] = context;
    Ok(params)
}

fn locations_from_response(
    response: &Value,
    source_root: &Path,
) -> Result<Vec<NormalizedLocation>> {
    let values = match response {
        Value::Null => Vec::new(),
        Value::Array(items) => items.iter().collect(),
        Value::Object(_) => vec![response],
        _ => bail!("unexpected LSP location response: {response}"),
    };
    let mut locations = values
        .into_iter()
        .map(|value| normalize_lsp_location(value, source_root))
        .collect::<Result<Vec<_>>>()?;
    locations.sort();
    locations.dedup();
    Ok(locations)
}

fn normalize_lsp_location(value: &Value, source_root: &Path) -> Result<NormalizedLocation> {
    let (uri, range) = if let Some(uri) = value.get("uri") {
        (uri, value.get("range"))
    } else {
        (
            value.get("targetUri").context("LSP location missing URI")?,
            value
                .get("targetSelectionRange")
                .or_else(|| value.get("targetRange")),
        )
    };
    let uri = uri.as_str().context("LSP location URI is not a string")?;
    let range = range.context("LSP location missing range")?;
    let line = range
        .pointer("/start/line")
        .and_then(Value::as_u64)
        .context("LSP location missing start line")? as u32;
    let column = range
        .pointer("/start/character")
        .and_then(Value::as_u64)
        .context("LSP location missing start character")? as u32;
    let end_line = range
        .pointer("/end/line")
        .and_then(Value::as_u64)
        .context("LSP location missing end line")? as u32;
    let end_column = range
        .pointer("/end/character")
        .and_then(Value::as_u64)
        .context("LSP location missing end character")? as u32;
    let url = Url::parse(uri).with_context(|| format!("parse LSP location URI `{uri}`"))?;
    let path = if url.scheme() == "file" {
        let absolute = url
            .to_file_path()
            .map_err(|_| anyhow::anyhow!("convert LSP file URI `{uri}`"))?;
        absolute
            .strip_prefix(source_root)
            .map(path_to_slash)
            .unwrap_or_else(|_| format!("external://{}", path_to_slash(&absolute)))
    } else {
        uri.to_string()
    };
    Ok(NormalizedLocation {
        path,
        line: line + 1,
        column: Some(column + 1),
        end_line: Some(end_line + 1),
        end_column: Some(end_column + 1),
        display_name: None,
        kind: None,
    })
}

fn navigation_method(
    operation: NavigationOperation,
    profile: &LspProfile,
    capabilities: &ServerCapabilities,
) -> Option<&'static str> {
    match operation {
        NavigationOperation::Declaration if capabilities.declaration => {
            Some("textDocument/declaration")
        }
        NavigationOperation::Definition if capabilities.definition => {
            Some("textDocument/definition")
        }
        NavigationOperation::ProfileDefault
            if profile.query_declaration && capabilities.declaration =>
        {
            Some("textDocument/declaration")
        }
        NavigationOperation::ProfileDefault if capabilities.definition => {
            Some("textDocument/definition")
        }
        _ => None,
    }
}

pub(crate) fn classify_reference_policy_extras(
    report: &mut DeclarationUsageReport,
    language: &str,
    source_root: &Path,
    reference_policy: ReferencePolicy,
) {
    let mut unexpected = Vec::new();
    for location in std::mem::take(&mut report.unexpected) {
        let (classification, rationale) = classify_extra_usage(language, source_root, &location);
        let is_binding = matches!(
            classification,
            ExtraUsageClassification::ImportBinding
                | ExtraUsageClassification::ReexportBinding
                | ExtraUsageClassification::ExportMetadata
        );
        let disposition = if is_binding && reference_policy == ReferencePolicy::BindingsOptional {
            ExtraUsageDisposition::AllowedPolicyExtra
        } else {
            unexpected.push(location.clone());
            ExtraUsageDisposition::Unexpected
        };
        report.extra_usages.push(ClassifiedExtraUsage {
            location,
            classification,
            disposition,
            rationale,
        });
    }

    report.unexpected = unexpected;
    let has_allowed_policy_extra = report
        .extra_usages
        .iter()
        .any(|extra| extra.disposition == ExtraUsageDisposition::AllowedPolicyExtra);
    if !report.partial
        && report.missing.is_empty()
        && report.missing_unproven.is_empty()
        && report.unexpected.is_empty()
        && report.unexpected_unproven.is_empty()
        && has_allowed_policy_extra
    {
        report.status = if report.position_unverified.is_empty() {
            CaseStatus::Passed
        } else {
            CaseStatus::PositionUnverified
        };
        report
            .raw_statuses
            .push("optional_binding_extras".to_string());
    }
}

fn classify_extra_usage(
    language: &str,
    source_root: &Path,
    location: &NormalizedLocation,
) -> (ExtraUsageClassification, String) {
    let Ok(source) = fs::read_to_string(source_root.join(&location.path)) else {
        return (
            ExtraUsageClassification::Unclassified,
            "source text was unavailable for classification".to_string(),
        );
    };
    let lines = source.lines().collect::<Vec<_>>();
    let line_index = location.line.saturating_sub(1) as usize;
    let Some(line) = lines.get(line_index).copied() else {
        return (
            ExtraUsageClassification::Unclassified,
            "reported line was outside the source file".to_string(),
        );
    };
    let trimmed = line.trim_start();
    let path = location.path.as_str();

    if language == "python" && trimmed.starts_with("__all__") {
        return (
            ExtraUsageClassification::ExportMetadata,
            "Python __all__ export metadata is included by the language server".to_string(),
        );
    }
    if language == "rust" && rust_line_is_in_pub_use(&lines, line_index) {
        return (
            ExtraUsageClassification::ReexportBinding,
            "the language server includes a re-export binding in find-references results"
                .to_string(),
        );
    }
    if is_reexport_binding(language, path, trimmed) {
        return (
            ExtraUsageClassification::ReexportBinding,
            "the language server includes a re-export binding in find-references results"
                .to_string(),
        );
    }
    if is_export_metadata(language, trimmed) {
        return (
            ExtraUsageClassification::ExportMetadata,
            "the language server includes export metadata in find-references results".to_string(),
        );
    }
    if is_import_binding(language, trimmed) {
        return (
            ExtraUsageClassification::ImportBinding,
            "the language server includes an import binding in find-references results".to_string(),
        );
    }
    if is_declaration_or_definition(language, trimmed) {
        return (
            ExtraUsageClassification::DeclarationOrDefinition,
            "the extra location appears to be a declaration or definition, despite includeDeclaration=false"
                .to_string(),
        );
    }
    (
        ExtraUsageClassification::Unclassified,
        "the extra location is not an allowed import, re-export, or export-metadata difference"
            .to_string(),
    )
}

fn rust_line_is_in_pub_use(lines: &[&str], line_index: usize) -> bool {
    for (index, line) in lines[..=line_index].iter().enumerate().rev() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("pub use ") {
            return true;
        }
        if index < line_index && line.contains(';') {
            return false;
        }
    }
    false
}

fn is_reexport_binding(language: &str, path: &str, line: &str) -> bool {
    match language {
        "javascript" | "typescript" => {
            line.starts_with("export * from ")
                || ((line.starts_with("export {") || line.starts_with("export type {"))
                    && line.contains(" from "))
        }
        "python" => {
            path.ends_with("/__init__.py")
                && (line.starts_with("from ") || line.starts_with("import "))
        }
        "rust" => line.starts_with("pub use "),
        _ => false,
    }
}

fn is_export_metadata(language: &str, line: &str) -> bool {
    matches!(language, "javascript" | "typescript")
        && (line.starts_with("export {") || line.starts_with("export type {"))
        && !line.contains(" from ")
}

fn is_import_binding(language: &str, line: &str) -> bool {
    match language {
        "javascript" | "typescript" | "java" | "scala" => line.starts_with("import "),
        "python" => line.starts_with("from ") || line.starts_with("import "),
        "php" => line.starts_with("use "),
        "rust" => line.starts_with("use "),
        "csharp" => line.starts_with("using "),
        _ => false,
    }
}

fn is_declaration_or_definition(language: &str, line: &str) -> bool {
    match language {
        "ruby" => {
            line.starts_with("class ")
                || line.starts_with("module ")
                || line.starts_with("def ")
                || line.starts_with("attr_")
                || line.starts_with("alias ")
                || line.starts_with("alias_method ")
                || line.starts_with("autoload ")
        }
        "rust" => line.starts_with("fn ") || line.starts_with("pub fn "),
        "cpp" => {
            line.starts_with("class ")
                || line.starts_with("struct ")
                || line.starts_with("explicit ")
        }
        _ => false,
    }
}

fn open_source_files(
    profile: &LspProfile,
    source_root: &Path,
    session: &mut LspSession,
) -> Result<()> {
    let mut files = Vec::new();
    collect_source_files(source_root, &profile.file_extensions, &mut files)?;
    files.sort();
    for file in files {
        let extension = file
            .extension()
            .map(|value| format!(".{}", value.to_string_lossy()))
            .unwrap_or_default();
        let language_id = profile
            .language_ids
            .get(&extension)
            .with_context(|| format!("profile missing language ID for `{extension}`"))?;
        let text = fs::read_to_string(&file)
            .with_context(|| format!("read source document {}", file.display()))?;
        let uri = Url::from_file_path(&file)
            .map_err(|_| anyhow::anyhow!("convert {} to file URI", file.display()))?;
        session.did_open(uri.as_str(), language_id, &text)?;
    }
    Ok(())
}

fn collect_source_files(
    root: &Path,
    extensions: &[String],
    files: &mut Vec<PathBuf>,
) -> Result<()> {
    for entry in fs::read_dir(root).with_context(|| format!("read {}", root.display()))? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            if is_generated_workspace_directory(&entry.file_name()) {
                continue;
            }
            collect_source_files(&path, extensions, files)?;
        } else if extensions.iter().any(|extension| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().ends_with(extension))
        }) {
            files.push(path);
        }
    }
    Ok(())
}

fn is_generated_workspace_directory(name: &std::ffi::OsStr) -> bool {
    matches!(
        name.to_str(),
        Some(
            ".git"
                | ".bifrost"
                | ".bloop"
                | ".bsp"
                | ".metals"
                | "bin"
                | "node_modules"
                | "obj"
                | "target"
        )
    )
}

fn capabilities_from_initialize(value: &Value) -> ServerCapabilities {
    ServerCapabilities {
        references: provider_enabled(value.get("referencesProvider")),
        declaration: provider_enabled(value.get("declarationProvider")),
        definition: provider_enabled(value.get("definitionProvider")),
        type_definition: provider_enabled(value.get("typeDefinitionProvider")),
    }
}

fn provider_enabled(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(enabled)) => *enabled,
        Some(Value::Object(_)) => true,
        _ => false,
    }
}

fn capability(operation: RunnerOperation, supported: bool, endpoint: &str) -> RunnerCapability {
    RunnerCapability {
        operation,
        support: if supported {
            CapabilitySupport::Native
        } else {
            CapabilitySupport::Unsupported
        },
        notes: if supported {
            format!("native LSP {endpoint}")
        } else {
            format!("server did not advertise {endpoint}")
        },
    }
}

fn normalize_server_version(version: &str) -> String {
    if let Ok(value) = serde_json::from_str::<Value>(version) {
        if let Some(main_version) = value.pointer("/Main/Version").and_then(Value::as_str) {
            return main_version.to_string();
        }
        if let Some(version) = value.get("Version").and_then(Value::as_str) {
            return version.to_string();
        }
    }
    version.to_string()
}

fn load_profile(path: &Path) -> Result<LspProfile> {
    let bytes = fs::read(path).with_context(|| format!("read LSP profile {}", path.display()))?;
    serde_json::from_slice(&bytes)
        .with_context(|| format!("deserialize LSP profile {}", path.display()))
}

fn validate_profile(profile: &LspProfile) -> Result<()> {
    if profile.id.is_empty()
        || profile.name.is_empty()
        || profile.requested_version.is_empty()
        || profile.source.is_empty()
    {
        bail!("LSP profile identity and release fields must not be empty");
    }
    if profile.languages.is_empty() {
        bail!("LSP profile `{}` has no languages", profile.id);
    }
    if profile.command.is_empty() {
        bail!("LSP profile `{}` has no command", profile.id);
    }
    if profile.file_extensions.is_empty() {
        bail!("LSP profile `{}` has no file extensions", profile.id);
    }
    for extension in &profile.file_extensions {
        if !profile.language_ids.contains_key(extension) {
            bail!(
                "LSP profile `{}` has no languageIds entry for `{extension}`",
                profile.id
            );
        }
    }
    Ok(())
}

fn lsp_command(
    options: &RunLspOptions,
    profile: &LspProfile,
    source_root: &Path,
    run_dir: &Path,
) -> Result<Command> {
    let program = options
        .server_command
        .as_ref()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| substitute(&profile.command[0], source_root, run_dir));
    let mut command = Command::new(program);
    for argument in profile.command.iter().skip(1) {
        command.arg(substitute(argument, source_root, run_dir));
    }
    for (key, value) in &profile.environment {
        command.env(key, substitute(value, source_root, run_dir));
    }
    command.current_dir(source_root);
    Ok(command)
}

fn run_prepare_command(profile: &LspProfile, source_root: &Path, run_dir: &Path) -> Result<()> {
    let Some(program) = profile.prepare_command.first() else {
        return Ok(());
    };
    let mut command = Command::new(substitute(program, source_root, run_dir));
    for argument in profile.prepare_command.iter().skip(1) {
        command.arg(substitute(argument, source_root, run_dir));
    }
    for (key, value) in &profile.environment {
        command.env(key, substitute(value, source_root, run_dir));
    }
    command.current_dir(source_root);
    let timeout = Duration::from_millis(profile.prepare_timeout_milliseconds.unwrap_or(300_000));
    let output = command_output_with_timeout(&mut command, timeout).with_context(|| {
        format!(
            "prepare {} workspace with `{}`",
            profile.name,
            profile.prepare_command.join(" ")
        )
    })?;
    if !output.status.success() {
        bail!(
            "prepare {} workspace failed with {}\nstdout:\n{}\nstderr:\n{}",
            profile.name,
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

fn send_profile_notifications(
    profile: &LspProfile,
    source_root: &Path,
    run_dir: &Path,
    workspace_uri: &str,
    session: &mut LspSession,
) -> Result<()> {
    for notification in &profile.post_initialize_notifications {
        session
            .notify(
                &notification.method,
                substitute_json(&notification.params, source_root, run_dir, workspace_uri),
            )
            .with_context(|| {
                format!("send {} post-initialize notification", notification.method)
            })?;
    }
    Ok(())
}

fn substitute_json(
    value: &Value,
    source_root: &Path,
    run_dir: &Path,
    workspace_uri: &str,
) -> Value {
    match value {
        Value::String(value) => Value::String(
            substitute(value, source_root, run_dir).replace("{workspaceUri}", workspace_uri),
        ),
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(|value| substitute_json(value, source_root, run_dir, workspace_uri))
                .collect(),
        ),
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        substitute_json(value, source_root, run_dir, workspace_uri),
                    )
                })
                .collect(),
        ),
        _ => value.clone(),
    }
}

fn substitute(value: &str, source_root: &Path, run_dir: &Path) -> String {
    value
        .replace("{workspace}", &source_root.to_string_lossy())
        .replace("{runDir}", &run_dir.to_string_lossy())
}

fn write_workspace_files(profile: &LspProfile, source_root: &Path) -> Result<()> {
    for (relative, content) in &profile.workspace_files {
        let path = source_root.join(relative);
        if path.exists() {
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        fs::write(&path, content).with_context(|| format!("write {}", path.display()))?;
    }
    if profile.generate_compile_commands {
        write_compile_commands(profile, source_root)?;
    }
    Ok(())
}

fn write_compile_commands(profile: &LspProfile, source_root: &Path) -> Result<()> {
    let mut files = Vec::new();
    collect_source_files(source_root, &profile.file_extensions, &mut files)?;
    files.retain(|path| {
        matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("c" | "cc" | "cpp" | "cxx")
        )
    });
    files.sort();
    let commands = files
        .into_iter()
        .map(|file| {
            json!({
                "directory": source_root,
                "file": file,
                "arguments": [
                    "clang++",
                    "-std=c++20",
                    "-I",
                    source_root,
                    "-I",
                    source_root.join("include"),
                    "-c",
                    file
                ]
            })
        })
        .collect::<Vec<_>>();
    let path = source_root.join("compile_commands.json");
    fs::write(&path, serde_json::to_vec_pretty(&commands)?)
        .with_context(|| format!("write {}", path.display()))
}

fn copy_source_tree(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)
        .with_context(|| format!("create source copy {}", destination.display()))?;
    for entry in fs::read_dir(source).with_context(|| format!("read {}", source.display()))? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            if matches!(entry.file_name().to_str(), Some(".git" | ".bifrost")) {
                continue;
            }
            copy_source_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target).with_context(|| {
                format!("copy {} to {}", entry.path().display(), target.display())
            })?;
        }
    }
    Ok(())
}

fn error_case(case: &BenchmarkCase, kind: &str, message: &str) -> CaseRunReport {
    CaseRunReport {
        id: case.id.clone(),
        status: CaseStatus::Error,
        expected_failure_reason: None,
        not_planned_reason: case.not_planned.as_ref().map(|item| item.reason.clone()),
        unsupported_reason: case.unsupported.as_ref().map(|item| item.reason.clone()),
        declaration_to_usages: None,
        usage_to_declaration: Vec::new(),
        type_lookups: Vec::new(),
        diagnostics: vec![RunDiagnostic {
            kind: kind.to_string(),
            message: message.to_string(),
        }],
    }
}

fn unsupported_definition_report(lookup: &UsageLookup, raw_status: &str) -> UsageDefinitionReport {
    UsageDefinitionReport {
        status: CaseStatus::Unsupported,
        operation: lookup.operation,
        usage: normalized_or_invalid(&lookup.usage),
        expected_declaration: normalized_or_invalid(&lookup.expected_declaration),
        actual_declarations: Vec::new(),
        raw_status: raw_status.to_string(),
        diagnostics: Vec::new(),
    }
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
    raw_status: &str,
    error: anyhow::Error,
) -> DeclarationUsageReport {
    let mut report = failed_declaration_report(case, raw_status);
    report.status = CaseStatus::Error;
    report.raw_statuses.push(format!("{error:#}"));
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
        position_unverified: Vec::new(),
        extra_usages: Vec::new(),
        partial: false,
        raw_statuses: vec![raw_status.to_string()],
    }
}

fn normalized_or_invalid(location: &SymbolLocation) -> NormalizedLocation {
    normalize_symbol_location(location).unwrap_or_else(|_| NormalizedLocation {
        path: "<invalid>".to_string(),
        line: 0,
        column: None,
        end_line: None,
        end_column: None,
        display_name: Some(location.display_name.clone()),
        kind: Some(symbol_kind_name(&location.kind).to_string()),
    })
}

fn benchmark_path(location: &SymbolLocation) -> String {
    benchmark_source_path(&location.location.uri)
        .map(|path| path_to_slash(&path))
        .unwrap_or_else(|_| "<invalid>".to_string())
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn unix_seconds_now() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time is before Unix epoch")?
        .as_secs())
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
        if self.enabled {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_boolean_and_options_capabilities() {
        let capabilities = capabilities_from_initialize(&json!({
            "referencesProvider": true,
            "declarationProvider": {"workDoneProgress": true},
            "definitionProvider": {"workDoneProgress": true},
            "typeDefinitionProvider": false
        }));
        assert!(capabilities.references);
        assert!(capabilities.declaration);
        assert!(capabilities.definition);
        assert!(!capabilities.type_definition);
    }

    #[test]
    fn explicit_declaration_lookup_never_falls_back_to_definition() {
        let profile = load_profile(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("adapters/lsp/rust-analyzer.json"),
        )
        .unwrap();
        let capabilities = ServerCapabilities {
            definition: true,
            ..ServerCapabilities::default()
        };

        assert_eq!(
            navigation_method(NavigationOperation::Declaration, &profile, &capabilities),
            None
        );
        assert_eq!(
            navigation_method(NavigationOperation::Definition, &profile, &capabilities),
            Some("textDocument/definition")
        );
    }

    #[test]
    fn parses_locations_and_location_links() {
        let root = Path::new("/tmp/workspace");
        let locations = locations_from_response(
            &json!([
                {
                    "uri": "file:///tmp/workspace/src/lib.rs",
                    "range": {"start": {"line": 2, "character": 4}, "end": {"line": 2, "character": 8}}
                },
                {
                    "targetUri": "file:///tmp/workspace/src/main.rs",
                    "targetRange": {"start": {"line": 5, "character": 1}, "end": {"line": 5, "character": 2}},
                    "targetSelectionRange": {"start": {"line": 6, "character": 2}, "end": {"line": 6, "character": 3}}
                }
            ]),
            root,
        )
        .unwrap();
        assert_eq!(locations[0].path, "src/lib.rs");
        assert_eq!(locations[0].line, 3);
        assert_eq!(locations[1].path, "src/main.rs");
        assert_eq!(locations[1].line, 7);
    }

    #[test]
    fn simplifies_structured_gopls_version() {
        assert_eq!(
            normalize_server_version(
                r#"{"Main":{"Path":"golang.org/x/tools/gopls","Version":"v0.23.0"}}"#
            ),
            "v0.23.0"
        );
    }

    #[test]
    fn definition_response_fails_when_expected_target_is_among_alternates() {
        let expected = test_location("src/interface.cs", 7);
        let actual = vec![expected.clone(), test_location("src/implementation.cs", 19)];
        assert_eq!(
            navigation_response_status(&actual, &expected, false),
            CaseStatus::Failed
        );
    }

    #[test]
    fn definition_response_requires_the_exact_token_range() {
        let expected = test_location("src/interface.cs", 7);
        let mut wrong_token = expected.clone();
        wrong_token.column = Some(4);
        wrong_token.end_column = Some(5);

        assert_eq!(
            navigation_response_status(&[wrong_token], &expected, false),
            CaseStatus::Failed
        );
        assert_eq!(
            navigation_response_status(std::slice::from_ref(&expected), &expected, false),
            CaseStatus::Passed
        );
    }

    #[test]
    fn no_movement_accepts_empty_or_exact_self_target_only() {
        let self_target = test_location("src/lib.rs", 16);
        let trait_target = test_location("src/lib.rs", 6);

        assert_eq!(
            navigation_response_status(&[], &self_target, true),
            CaseStatus::Passed
        );
        assert_eq!(
            navigation_response_status(std::slice::from_ref(&self_target), &self_target, true),
            CaseStatus::Passed
        );
        assert_eq!(
            navigation_response_status(&[trait_target], &self_target, true),
            CaseStatus::Failed
        );
    }

    #[test]
    fn import_binding_extra_passes_when_bindings_are_optional() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("src")).unwrap();
        fs::write(
            root.path().join("src/import.ts"),
            "import { Widget } from './widget';\n",
        )
        .unwrap();
        let mut report = empty_declaration_report(CaseStatus::Failed, "ok");
        report.unexpected.push(test_location("src/import.ts", 1));
        classify_reference_policy_extras(
            &mut report,
            "typescript",
            root.path(),
            ReferencePolicy::BindingsOptional,
        );
        assert_eq!(report.status, CaseStatus::Passed);
        assert!(report.unexpected.is_empty());
        assert_eq!(report.extra_usages.len(), 1);
        assert_eq!(
            report.extra_usages[0].classification,
            ExtraUsageClassification::ImportBinding
        );
        assert!(report
            .raw_statuses
            .contains(&"optional_binding_extras".to_string()));
    }

    #[test]
    fn unclassified_reference_extra_remains_a_failure() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("src")).unwrap();
        fs::write(root.path().join("src/use.ts"), "Widget.create();\n").unwrap();
        let mut report = empty_declaration_report(CaseStatus::Failed, "ok");
        report.unexpected.push(test_location("src/use.ts", 1));
        classify_reference_policy_extras(
            &mut report,
            "typescript",
            root.path(),
            ReferencePolicy::BindingsOptional,
        );
        assert_eq!(report.status, CaseStatus::Failed);
        assert_eq!(report.unexpected.len(), 1);
        assert_eq!(
            report.extra_usages[0].disposition,
            ExtraUsageDisposition::Unexpected
        );
    }

    #[test]
    fn multiline_rust_reexport_is_an_allowed_policy_extra() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("src")).unwrap();
        fs::write(
            root.path().join("src/lib.rs"),
            "pub use crate::service::{\n    Service, build_service,\n};\n",
        )
        .unwrap();
        let mut report = empty_declaration_report(CaseStatus::Failed, "ok");
        report.unexpected.push(test_location("src/lib.rs", 2));
        classify_reference_policy_extras(
            &mut report,
            "rust",
            root.path(),
            ReferencePolicy::BindingsOptional,
        );
        assert_eq!(report.status, CaseStatus::Passed);
        assert_eq!(
            report.extra_usages[0].classification,
            ExtraUsageClassification::ReexportBinding
        );
    }

    #[test]
    fn runtime_javascript_export_expression_is_not_a_policy_extra() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("src")).unwrap();
        fs::write(
            root.path().join("src/export.ts"),
            "export default Widget;\n",
        )
        .unwrap();
        let mut report = empty_declaration_report(CaseStatus::Failed, "ok");
        report.unexpected.push(test_location("src/export.ts", 1));
        classify_reference_policy_extras(
            &mut report,
            "typescript",
            root.path(),
            ReferencePolicy::BindingsOptional,
        );
        assert_eq!(report.status, CaseStatus::Failed);
        assert_eq!(
            report.extra_usages[0].classification,
            ExtraUsageClassification::Unclassified
        );
    }

    #[test]
    fn import_binding_extra_fails_when_policy_excludes_bindings() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("src")).unwrap();
        fs::write(
            root.path().join("src/import.ts"),
            "import { Widget } from './widget';\n",
        )
        .unwrap();
        let mut report = empty_declaration_report(CaseStatus::Failed, "ok");
        report.unexpected.push(test_location("src/import.ts", 1));

        classify_reference_policy_extras(
            &mut report,
            "typescript",
            root.path(),
            ReferencePolicy::ExternalUsages,
        );

        assert_eq!(report.status, CaseStatus::Failed);
        assert_eq!(report.unexpected.len(), 1);
    }

    #[test]
    fn substitutes_workspace_uri_in_profile_notifications() {
        let value = substitute_json(
            &json!({"projects": ["{workspaceUri}/usagebench.csproj"]}),
            Path::new("/tmp/source"),
            Path::new("/tmp/run"),
            "file:///tmp/source",
        );
        assert_eq!(
            value,
            json!({"projects": ["file:///tmp/source/usagebench.csproj"]})
        );
    }

    #[test]
    fn bundled_profiles_are_valid_and_cover_the_corpus_languages() {
        let profiles = Path::new(env!("CARGO_MANIFEST_DIR")).join("adapters/lsp");
        let mut languages = std::collections::BTreeSet::new();
        for entry in fs::read_dir(profiles).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let profile = load_profile(&path).unwrap();
            validate_profile(&profile).unwrap();
            languages.extend(profile.languages);
        }
        assert_eq!(
            languages,
            [
                "cpp",
                "csharp",
                "go",
                "java",
                "javascript",
                "php",
                "python",
                "ruby",
                "rust",
                "scala",
                "typescript",
            ]
            .into_iter()
            .map(str::to_string)
            .collect()
        );
    }

    fn test_location(path: &str, line: u32) -> NormalizedLocation {
        NormalizedLocation {
            path: path.to_string(),
            line,
            column: Some(1),
            end_line: Some(line),
            end_column: Some(2),
            display_name: None,
            kind: None,
        }
    }
}
