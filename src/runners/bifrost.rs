use super::mcp::{McpSession, ToolClient as SearchToolsClient};
use super::{
    combine_case_status, compute_totals, location_match, normalize_symbol_location, path_to_slash,
    resolve_usagebench_provenance, score_declaration_locations, score_navigation_response,
    symbol_kind_name, CapabilitySupport, LocationMatch, RunInvocation, RunReport, RunnerCapability,
    RunnerMetadata, RunnerOperation,
};
pub use super::{
    CaseRunReport, CaseStatus, DeclarationUsageReport, DocumentRunReport, NormalizedLocation,
    RunDiagnostic, RunTotals, TypeLookupReport, UsageDefinitionReport,
};
use crate::{
    benchmark_source_path, find_repo_root_for_path, BenchmarkCase, BenchmarkDocument, Location,
    NavigationOperation, PositionEncoding, ReferencePolicy, Source, SymbolKind, SymbolLocation,
};
use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{hash_map::DefaultHasher, BTreeSet},
    fs,
    hash::{Hash, Hasher},
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use url::{Host, Url};

const DEFAULT_BIFROST_COMMIT: &str = "origin/master";
const BIFROST_SOURCE_URL: &str = "https://github.com/BrokkAi/bifrost";
const GET_DEFINITIONS_BY_LOCATION_TOOL: &str = "get_definitions_by_location";
const GET_TYPE_BY_LOCATION_TOOL: &str = "get_type_by_location";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone)]
pub struct RunBifrostOptions {
    pub case_path: PathBuf,
    pub bifrost_repo: Option<PathBuf>,
    pub bifrost_commit: String,
    pub bifrost_working_tree: bool,
    pub bifrost_binary: Option<PathBuf>,
    pub bifrost_resolved_commit: Option<String>,
    pub work_dir: PathBuf,
    pub output: Option<PathBuf>,
    pub include_unsupported: bool,
    pub include_definition_lookups: bool,
    pub keep_worktrees: bool,
    pub case_id: Option<String>,
}

impl RunBifrostOptions {
    pub fn with_defaults(case_path: PathBuf) -> Self {
        Self {
            case_path,
            bifrost_repo: None,
            bifrost_commit: DEFAULT_BIFROST_COMMIT.to_string(),
            bifrost_working_tree: false,
            bifrost_binary: None,
            bifrost_resolved_commit: None,
            work_dir: PathBuf::from("target/usagebench"),
            output: None,
            include_unsupported: false,
            include_definition_lookups: true,
            keep_worktrees: false,
            case_id: None,
        }
    }
}

pub type BifrostRunReport = RunReport;

pub fn generated_bifrost_report_schema_json() -> Result<String> {
    super::generated_report_schema_json()
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
    let usagebench_provenance = resolve_usagebench_provenance(&repo_root)?;
    let case_files = crate::validate_path(&options.case_path)?;

    let work_dir = if options.work_dir.is_absolute() {
        options.work_dir.clone()
    } else {
        repo_root.join(&options.work_dir)
    };
    fs::create_dir_all(&work_dir).with_context(|| format!("create {}", work_dir.display()))?;
    let _source_cleanup = CleanupGuard::new(work_dir.join("sources"), !options.keep_worktrees);

    let (bifrost_source, bifrost_resolved_commit, bifrost_binary) =
        if let Some(binary) = &options.bifrost_binary {
            if !binary.is_file() {
                bail!("Bifrost executable does not exist: {}", binary.display());
            }
            let resolved_commit = options
                .bifrost_resolved_commit
                .clone()
                .context("--bifrost-resolved-commit is required with --bifrost-binary")?;
            (
                options
                    .bifrost_repo
                    .as_ref()
                    .map(|path| display_path(path))
                    .unwrap_or_else(|| BIFROST_SOURCE_URL.to_string()),
                resolved_commit,
                binary.clone(),
            )
        } else {
            let bifrost_source_repo =
                resolve_bifrost_source_repo(&repo_root, options.bifrost_repo.as_ref())?;
            let bifrost_checkout = if options.bifrost_working_tree {
                bifrost_source_repo.clone()
            } else {
                prepare_bifrost_checkout(&bifrost_source_repo, &options.bifrost_commit, &work_dir)?
            };
            let resolved_commit = git_output(&bifrost_checkout, ["rev-parse", "HEAD"])?;
            build_bifrost(&bifrost_checkout)?;
            (
                display_path(&bifrost_source_repo),
                resolved_commit,
                bifrost_binary_path(&bifrost_checkout),
            )
        };
    let environment = super::environment::capture_execution_environment(
        super::environment::executable_provenance(&Command::new(&bifrost_binary))?,
        &["rustc", "cargo"],
        "bifrost",
        &usagebench_provenance.revision,
        usagebench_provenance.release.as_deref(),
    )?;

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
            options.case_id.as_deref(),
        )
        .with_context(|| format!("run benchmark cases {}", case_file.display()))?;
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

    let finished_at = unix_seconds_now()?;
    let requested_version = if options.bifrost_binary.is_some() {
        bifrost_resolved_commit.clone()
    } else {
        options.bifrost_commit.clone()
    };
    let runner = RunnerMetadata {
        name: "bifrost".to_string(),
        requested_version: requested_version.clone(),
        resolved_version: bifrost_resolved_commit.clone(),
        source: bifrost_source.clone(),
        adapter_version: env!("CARGO_PKG_VERSION").to_string(),
        capabilities: vec![
            RunnerCapability {
                operation: RunnerOperation::DeclarationToUsages,
                support: CapabilitySupport::Native,
                notes: "Bifrost scan_usages_by_location MCP output".to_string(),
            },
            RunnerCapability {
                operation: RunnerOperation::DeclarationLookup,
                support: CapabilitySupport::Unsupported,
                notes: "Bifrost does not expose a distinct declaration-navigation tool".to_string(),
            },
            RunnerCapability {
                operation: RunnerOperation::DefinitionLookup,
                support: CapabilitySupport::Native,
                notes: "Bifrost get_definitions_by_location MCP output".to_string(),
            },
            RunnerCapability {
                operation: RunnerOperation::TypeLookup,
                support: CapabilitySupport::Native,
                notes: "Bifrost get_type_by_location MCP output".to_string(),
            },
        ],
    };
    let mut report = BifrostRunReport {
        usagebench_version: env!("CARGO_PKG_VERSION").to_string(),
        usagebench_revision: usagebench_provenance.revision,
        usagebench_release: usagebench_provenance.release,
        runner,
        invocation: RunInvocation {
            include_unsupported: options.include_unsupported,
            include_definition_lookups: options.include_definition_lookups,
            profile: None,
            case_id: options.case_id.clone(),
        },
        environment,
        bifrost_repo: Some(bifrost_source),
        bifrost_commit: Some(requested_version),
        bifrost_resolved_commit: Some(bifrost_resolved_commit),
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
    case_id: Option<&str>,
) -> Result<Vec<CaseRunReport>> {
    let mut command = Command::new(bifrost_binary);
    command
        .arg("--root")
        .arg(source_root)
        .arg("--server")
        .arg("searchtools");
    let mut session = McpSession::start(&mut command, "Bifrost")?;
    session.initialize()?;

    let mut reports = Vec::new();
    for case in document
        .cases
        .iter()
        .filter(|case| case_id.is_none_or(|case_id| case.id == case_id))
    {
        reports.push(run_case(
            case,
            document.position_encoding,
            document.reference_policy,
            Some((&document.language, source_root)),
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
    reference_policy: ReferencePolicy,
    reference_context: Option<(&str, &Path)>,
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
        run_declaration_to_usages(
            case,
            declaration,
            encoding,
            reference_policy,
            reference_context,
            session,
            &mut diagnostics,
        )
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
    reference_policy: ReferencePolicy,
    reference_context: Option<(&str, &Path)>,
    session: &mut impl SearchToolsClient,
    diagnostics: &mut Vec<RunDiagnostic>,
) -> DeclarationUsageReport {
    let expected = case
        .expected_usages
        .iter()
        .map(normalize_symbol_location)
        .collect::<Result<Vec<_>>>();
    let expected_unproven = case
        .expected_unproven_usages
        .iter()
        .map(normalize_symbol_location)
        .collect::<Result<Vec<_>>>();
    let allowed_extra = case
        .allowed_extra_usages
        .iter()
        .map(normalize_symbol_location)
        .collect::<Result<Vec<_>>>();
    let allowed_unproven = case
        .allowed_unproven_usages
        .iter()
        .map(normalize_symbol_location)
        .collect::<Result<Vec<_>>>();
    let (expected, expected_unproven, allowed_extra, allowed_unproven) =
        match (expected, expected_unproven, allowed_extra, allowed_unproven) {
            (Ok(expected), Ok(expected_unproven), Ok(allowed_extra), Ok(allowed_unproven)) => {
                (expected, expected_unproven, allowed_extra, allowed_unproven)
            }
            (Err(error), _, _, _)
            | (_, Err(error), _, _)
            | (_, _, Err(error), _)
            | (_, _, _, Err(error)) => {
                diagnostics.push(RunDiagnostic {
                    kind: "invalid_expected_location".to_string(),
                    message: format!("{error:#}"),
                });
                return DeclarationUsageReport {
                    status: CaseStatus::Error,
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
                expected_unproven: expected_unproven.clone(),
                allowed_extra,
                allowed_unproven,
                actual: Vec::new(),
                unproven: Vec::new(),
                missing: expected,
                missing_unproven: expected_unproven,
                unexpected: Vec::new(),
                unexpected_unproven: Vec::new(),
                position_unverified: Vec::new(),
                extra_usages: Vec::new(),
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
                .insert(
                    "symbol".to_string(),
                    Value::String(selector.selector.clone()),
                );
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
                expected_unproven: expected_unproven.clone(),
                allowed_extra,
                allowed_unproven,
                actual: Vec::new(),
                unproven: Vec::new(),
                missing: expected,
                missing_unproven: expected_unproven,
                unexpected: Vec::new(),
                unexpected_unproven: Vec::new(),
                position_unverified: Vec::new(),
                extra_usages: Vec::new(),
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
            "include_bindings": reference_policy != ReferencePolicy::ExternalUsages,
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
                expected_unproven: expected_unproven.clone(),
                allowed_extra,
                allowed_unproven,
                actual: Vec::new(),
                unproven: Vec::new(),
                missing: expected,
                missing_unproven: expected_unproven,
                unexpected: Vec::new(),
                unexpected_unproven: Vec::new(),
                position_unverified: Vec::new(),
                extra_usages: Vec::new(),
                partial: false,
                raw_statuses: vec!["scan_usages_failed".to_string()],
            };
        }
    };

    let parsed = parse_scan_usages(&result);
    let has_failure_status = parsed.has_failure_status();
    let ParsedScanUsages {
        mut locations,
        mut unproven_locations,
        override_declarations,
        unproven_override_declarations,
        partial,
        mut raw_statuses,
    } = parsed;
    let override_count = override_declarations.len() + unproven_override_declarations.len();
    let retained_override_declarations = override_declarations
        .into_iter()
        .filter(|location| {
            expected
                .iter()
                .chain(&expected_unproven)
                .chain(&allowed_extra)
                .any(|authored| authored.path == location.path && authored.line == location.line)
        })
        .collect::<Vec<_>>();
    let retained_unproven_override_declarations = unproven_override_declarations
        .into_iter()
        .filter(|location| {
            expected_unproven
                .iter()
                .chain(&allowed_unproven)
                .any(|authored| authored.path == location.path && authored.line == location.line)
        })
        .collect::<Vec<_>>();
    let retained_override_count =
        retained_override_declarations.len() + retained_unproven_override_declarations.len();
    locations.extend(retained_override_declarations);
    unproven_locations.extend(retained_unproven_override_declarations);
    unproven_locations.retain(|location| !locations.contains(location));
    if retained_override_count < override_count {
        raw_statuses.push("override_declarations_excluded".to_string());
    }
    let mut report = score_declaration_locations(
        case,
        Some(selector.selector),
        locations,
        unproven_locations,
        partial,
        raw_statuses,
        has_failure_status,
    )
    .expect("expected locations were normalized before scoring");
    if let Some((language, source_root)) = reference_context {
        super::lsp::classify_reference_policy_extras(
            &mut report,
            language,
            source_root,
            reference_policy,
        );
    }
    report
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
        end_line: None,
        end_column: None,
        display_name: Some(lookup.usage.display_name.clone()),
        kind: Some(symbol_kind_name(&lookup.usage.kind).to_string()),
    });
    let expected_declaration = normalize_symbol_location(&lookup.expected_declaration)
        .unwrap_or_else(|_| NormalizedLocation {
            path: "<invalid>".to_string(),
            line: 0,
            column: None,
            end_line: None,
            end_column: None,
            display_name: Some(lookup.expected_declaration.display_name.clone()),
            kind: Some(symbol_kind_name(&lookup.expected_declaration.kind).to_string()),
        });
    if lookup.operation == NavigationOperation::Declaration {
        return UsageDefinitionReport {
            status: CaseStatus::Unsupported,
            operation: lookup.operation,
            usage,
            expected_declaration,
            actual_declarations: Vec::new(),
            raw_status: "declaration_lookup_unsupported".to_string(),
            diagnostics: vec![RunDiagnostic {
                kind: "declaration_lookup_unsupported".to_string(),
                message: "Bifrost does not expose a declaration-navigation operation distinct from get_definitions_by_location"
                    .to_string(),
            }],
        };
    }
    let query = match reference_query(&lookup.usage.location, &lookup.usage.display_name, encoding)
    {
        Ok(query) => query,
        Err(error) => {
            return UsageDefinitionReport {
                status: CaseStatus::Error,
                operation: lookup.operation,
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
                operation: lookup.operation,
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
    let (status, raw_status) = if parsed.raw_status == "resolved"
        || (lookup.expect_no_movement
            && parsed.raw_status == "no_definition"
            && parsed.actual_declarations.is_empty())
    {
        let (status, outcome) = score_navigation_response(
            &parsed.actual_declarations,
            &expected_declaration,
            lookup.expect_no_movement,
        );
        (status, outcome.to_string())
    } else {
        (CaseStatus::Failed, parsed.raw_status.clone())
    };

    UsageDefinitionReport {
        status,
        operation: lookup.operation,
        usage,
        expected_declaration,
        actual_declarations: parsed.actual_declarations,
        raw_status,
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
            end_line: None,
            end_column: None,
            display_name: Some(lookup.expression.display_name.clone()),
            kind: Some(symbol_kind_name(&lookup.expression.kind).to_string()),
        });
    let expected_type =
        normalize_symbol_location(&lookup.expected_type).unwrap_or_else(|_| NormalizedLocation {
            path: "<invalid>".to_string(),
            line: 0,
            column: None,
            end_line: None,
            end_column: None,
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
    let status = if parsed.raw_status != "resolved" || parsed.actual_types.len() != 1 {
        CaseStatus::Failed
    } else {
        match location_match(&parsed.actual_types[0], &expected_type) {
            LocationMatch::Exact if type_name_matches(&parsed.actual_types[0], &expected_type) => {
                CaseStatus::Passed
            }
            LocationMatch::LineOnly
                if type_name_matches(&parsed.actual_types[0], &expected_type) =>
            {
                CaseStatus::PositionUnverified
            }
            _ => CaseStatus::Failed,
        }
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
        end_line: None,
        end_column: None,
        display_name: Some(lookup.usage.display_name.clone()),
        kind: Some(symbol_kind_name(&lookup.usage.kind).to_string()),
    });
    let expected_declaration = normalize_symbol_location(&lookup.expected_declaration)
        .unwrap_or_else(|_| NormalizedLocation {
            path: "<invalid>".to_string(),
            line: 0,
            column: None,
            end_line: None,
            end_column: None,
            display_name: Some(lookup.expected_declaration.display_name.clone()),
            kind: Some(symbol_kind_name(&lookup.expected_declaration.kind).to_string()),
        });
    UsageDefinitionReport {
        status: CaseStatus::Skipped,
        operation: lookup.operation,
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
            end_line: None,
            end_column: None,
            display_name: Some(lookup.expression.display_name.clone()),
            kind: Some(symbol_kind_name(&lookup.expression.kind).to_string()),
        });
    let expected_type =
        normalize_symbol_location(&lookup.expected_type).unwrap_or_else(|_| NormalizedLocation {
            path: "<invalid>".to_string(),
            line: 0,
            column: None,
            end_line: None,
            end_column: None,
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
    unproven_locations: Vec<NormalizedLocation>,
    override_declarations: Vec<NormalizedLocation>,
    unproven_override_declarations: Vec<NormalizedLocation>,
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
    let mut unproven_locations = BTreeSet::new();
    let mut override_declarations = BTreeSet::new();
    let mut unproven_override_declarations = BTreeSet::new();

    let mut raw_statuses = Vec::new();
    let mut partial = value
        .get("summary")
        .and_then(|summary| summary.get("partial"))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    if let Some(results) = value.get("results").and_then(Value::as_array) {
        for result in results {
            collect_scan_usage_locations(
                result,
                "files",
                &mut locations,
                &mut override_declarations,
            );
            collect_scan_usage_locations(
                result,
                "unproven_files",
                &mut unproven_locations,
                &mut unproven_override_declarations,
            );
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
            collect_scan_usage_locations(
                usage,
                "files",
                &mut locations,
                &mut override_declarations,
            );
            collect_scan_usage_locations(
                usage,
                "unproven_files",
                &mut unproven_locations,
                &mut unproven_override_declarations,
            );
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
    // A location reported in both tiers has proven evidence. Keep the stronger
    // classification so downstream scoring does not treat it as an unproven
    // extra as well.
    unproven_locations.retain(|location| !locations.contains(location));

    ParsedScanUsages {
        locations: locations.into_iter().collect(),
        unproven_locations: unproven_locations.into_iter().collect(),
        override_declarations: override_declarations.into_iter().collect(),
        unproven_override_declarations: unproven_override_declarations.into_iter().collect(),
        partial,
        raw_statuses,
    }
}

fn collect_scan_usage_locations(
    value: &Value,
    group: &str,
    locations: &mut BTreeSet<NormalizedLocation>,
    override_declarations: &mut BTreeSet<NormalizedLocation>,
) {
    for file in value
        .get(group)
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
            let location = NormalizedLocation {
                path: path.to_string(),
                line: line as u32,
                column: hit
                    .get("column")
                    .and_then(Value::as_u64)
                    .map(|value| value as u32),
                end_line: hit
                    .get("end_line")
                    .and_then(Value::as_u64)
                    .map(|value| value as u32),
                end_column: hit
                    .get("end_column")
                    .and_then(Value::as_u64)
                    .map(|value| value as u32),
                display_name: None,
                kind: None,
            };
            if hit.get("kind").and_then(Value::as_str) == Some("override_declaration") {
                override_declarations.insert(location);
            } else {
                locations.insert(location);
            }
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
                column: definition
                    .get("start_column")
                    .and_then(Value::as_u64)
                    .map(|value| value as u32),
                end_line: definition
                    .get("end_line")
                    .and_then(Value::as_u64)
                    .map(|value| value as u32),
                end_column: definition
                    .get("end_column")
                    .and_then(Value::as_u64)
                    .map(|value| value as u32),
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
                        column: definition
                            .get("start_column")
                            .and_then(Value::as_u64)
                            .map(|value| value as u32),
                        end_line: definition
                            .get("end_line")
                            .and_then(Value::as_u64)
                            .map(|value| value as u32),
                        end_column: definition
                            .get("end_column")
                            .and_then(Value::as_u64)
                            .map(|value| value as u32),
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

fn type_name_matches(actual: &NormalizedLocation, expected: &NormalizedLocation) -> bool {
    actual
        .display_name
        .as_deref()
        .zip(expected.display_name.as_deref())
        .is_some_and(|(actual_name, expected_name)| symbol_name_matches(actual_name, expected_name))
}

fn symbol_name_matches(symbol: &str, display_name: &str) -> bool {
    symbol == display_name
        || symbol.ends_with(&format!(".{display_name}"))
        || symbol.ends_with(&format!("::{display_name}"))
        || symbol.ends_with(&format!("${display_name}"))
        || symbol.ends_with(&format!("#{display_name}"))
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

pub(crate) fn prepare_source_root(
    source: &Source,
    repo_root: &Path,
    work_dir: &Path,
) -> Result<PathBuf> {
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

pub(crate) fn command_output_with_timeout(
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
        assert!(options.case_id.is_none());
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
            usagebench_revision: "def456".to_string(),
            usagebench_release: Some("v0.1.0".to_string()),
            runner: RunnerMetadata {
                name: "bifrost".to_string(),
                requested_version: "origin/master".to_string(),
                resolved_version: "abc123".to_string(),
                source: "/repo/bifrost".to_string(),
                adapter_version: "0.1.0".to_string(),
                capabilities: Vec::new(),
            },
            invocation: RunInvocation {
                include_unsupported: false,
                include_definition_lookups: true,
                profile: None,
                case_id: None,
            },
            environment: super::super::ExecutionEnvironment {
                operating_system: "linux".to_string(),
                architecture: "x86_64".to_string(),
                execution_mode: super::super::ExecutionMode::Container,
                platform_scope: super::super::PlatformScope::CanonicalReference,
                reference_environment: Some(super::super::ReferenceEnvironmentProvenance {
                    version: "1".to_string(),
                    definition_digest: format!("sha256:{}", "a".repeat(64)),
                    canonical_platform: "linux/amd64".to_string(),
                }),
                container: Some(super::super::ContainerProvenance {
                    image_reference: "usagebench-reference:v0.1.0-env1-bifrost".to_string(),
                    image_digest: format!("sha256:{}", "b".repeat(64)),
                }),
                analyzer_executable: super::super::ExecutableProvenance {
                    command: "/usr/local/bin/bifrost".to_string(),
                    resolved_path: Some("/usr/local/bin/bifrost".to_string()),
                    sha256: Some("c".repeat(64)),
                },
                toolchains: std::collections::BTreeMap::new(),
            },
            bifrost_repo: Some("/repo/bifrost".to_string()),
            bifrost_commit: Some("origin/master".to_string()),
            bifrost_resolved_commit: Some("abc123".to_string()),
            started_at_unix_seconds: 1,
            finished_at_unix_seconds: 2,
            case_files: vec!["benchmarks/cases/rust.yaml".to_string()],
            totals: RunTotals {
                documents: 1,
                cases: 1,
                development_cases: 1,
                evaluation_cases: 0,
                passed: 1,
                near_misses: 0,
                position_unverified: 0,
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
                corpus_partition: crate::CorpusPartition::Development,
                corpus_selection: crate::CorpusSelection::AnalyzerInformed,
                ground_truth_status: crate::GroundTruthReviewStatus::LegacyUnattributed,
                reference_policy: ReferencePolicy::BindingsOptional,
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
                        expected_unproven: Vec::new(),
                        allowed_extra: Vec::new(),
                        allowed_unproven: Vec::new(),
                        actual: vec![
                            normalized_location("src/lib.rs", 8),
                            normalized_location("src/extra.rs", 1),
                        ],
                        unproven: Vec::new(),
                        missing: Vec::new(),
                        missing_unproven: Vec::new(),
                        unexpected: vec![normalized_location("src/extra.rs", 1)],
                        unexpected_unproven: Vec::new(),
                        position_unverified: Vec::new(),
                        extra_usages: Vec::new(),
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
        assert_eq!(json["usagebenchRevision"], "def456");
        assert_eq!(json["usagebenchRelease"], "v0.1.0");
        assert_eq!(json["bifrostResolvedCommit"], "abc123");
        assert_eq!(json["invocation"]["includeUnsupported"], false);
        assert_eq!(json["environment"]["executionMode"], "container");
        assert_eq!(
            json["environment"]["referenceEnvironment"]["canonicalPlatform"],
            "linux/amd64"
        );
        assert_eq!(
            json["environment"]["analyzerExecutable"]["sha256"],
            "c".repeat(64)
        );
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

        let report = run_case(
            &case,
            PositionEncoding::Utf16,
            ReferencePolicy::BindingsOptional,
            None,
            &mut client,
            false,
            true,
        );

        assert_eq!(report.status, CaseStatus::Passed);
        assert_eq!(
            client.calls[1].1["targets"][0]["symbol"],
            "example.build_service"
        );
        assert_eq!(client.calls[1].1["include_bindings"], true);
    }

    #[test]
    fn external_usage_policy_does_not_request_bindings() {
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

        let report = run_case(
            &case,
            PositionEncoding::Utf16,
            ReferencePolicy::ExternalUsages,
            None,
            &mut client,
            false,
            false,
        );

        assert_eq!(report.status, CaseStatus::Passed);
        assert_eq!(client.calls[1].1["include_bindings"], false);
    }

    #[test]
    fn bifrost_binding_extra_passes_when_bindings_are_optional() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("src")).unwrap();
        fs::write(
            root.path().join("src/import.rs"),
            "pub use crate::service::build_service;\n",
        )
        .unwrap();
        let case = benchmark_case();
        let mut client = MockClient::new(vec![
            tool(
                "search_symbols",
                search_symbols_json("src/service.rs", "example.build_service", 30),
            ),
            tool(
                "scan_usages_by_location",
                scan_usages_json(vec![("src/lib.rs", 8), ("src/import.rs", 1)], false),
            ),
        ]);

        let report = run_case(
            &case,
            PositionEncoding::Utf16,
            ReferencePolicy::BindingsOptional,
            Some(("rust", root.path())),
            &mut client,
            false,
            false,
        );

        assert_eq!(report.status, CaseStatus::Passed);
        let declaration = report.declaration_to_usages.unwrap();
        assert!(declaration.unexpected.is_empty());
        assert_eq!(declaration.extra_usages.len(), 1);
        assert_eq!(
            declaration.extra_usages[0].classification,
            crate::runners::ExtraUsageClassification::ReexportBinding
        );
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

        let report = run_case(
            &case,
            PositionEncoding::Utf16,
            ReferencePolicy::BindingsOptional,
            None,
            &mut client,
            false,
            true,
        );

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

        let report = run_case(
            &case,
            PositionEncoding::Utf16,
            ReferencePolicy::BindingsOptional,
            None,
            &mut client,
            false,
            true,
        );

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

        let report = run_case(
            &case,
            PositionEncoding::Utf16,
            ReferencePolicy::BindingsOptional,
            None,
            &mut client,
            false,
            true,
        );

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

        let report = run_case(
            &case,
            PositionEncoding::Utf16,
            ReferencePolicy::BindingsOptional,
            None,
            &mut client,
            false,
            true,
        );

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

        let report = run_case(
            &case,
            PositionEncoding::Utf16,
            ReferencePolicy::BindingsOptional,
            None,
            &mut client,
            false,
            true,
        );

        assert_eq!(report.status, CaseStatus::Passed);
    }

    #[test]
    fn scorer_marks_line_only_unproven_locations_position_unverified() {
        let mut case = benchmark_case();
        case.expected_unproven_usages = std::mem::take(&mut case.expected_usages);
        case.allowed_unproven_usages.push(symbol_location(
            "src/conservative_candidate.rs",
            3,
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
                json!({
                    "summary": {"partial": false},
                    "results": [{
                        "status": "found",
                        "files": [],
                        "unproven_files": [{
                            "path": "src/lib.rs",
                            "hits": [{"line": 8}]
                        }, {
                            "path": "src/conservative_candidate.rs",
                            "hits": [{"line": 4}]
                        }]
                    }]
                }),
            ),
            tool(
                GET_DEFINITIONS_BY_LOCATION_TOOL,
                get_definitions_by_location_json("resolved", vec![("src/service.rs", 30)]),
            ),
        ]);

        let report = run_case(
            &case,
            PositionEncoding::Utf16,
            ReferencePolicy::BindingsOptional,
            None,
            &mut client,
            false,
            true,
        );

        assert_eq!(report.status, CaseStatus::PositionUnverified);
        let declaration = report.declaration_to_usages.unwrap();
        assert!(declaration.actual.is_empty());
        assert_eq!(declaration.unproven.len(), 2);
        assert!(declaration.missing.is_empty());
        assert!(declaration.missing_unproven.is_empty());
        assert!(declaration.unexpected.is_empty());
        assert!(declaration.unexpected_unproven.is_empty());
        assert_eq!(declaration.position_unverified.len(), 1);
    }

    #[test]
    fn scorer_fails_when_a_proven_expectation_degrades_to_unproven() {
        let case = benchmark_case();
        let mut client = MockClient::new(vec![
            tool(
                "search_symbols",
                search_symbols_json("src/service.rs", "example.build_service", 30),
            ),
            tool(
                "scan_usages_by_location",
                json!({
                    "summary": {"partial": false},
                    "results": [{
                        "status": "found",
                        "files": [],
                        "unproven_files": [{
                            "path": "src/lib.rs",
                            "hits": [{"line": 8}]
                        }]
                    }]
                }),
            ),
            tool(
                GET_DEFINITIONS_BY_LOCATION_TOOL,
                get_definitions_by_location_json("resolved", vec![("src/service.rs", 30)]),
            ),
        ]);

        let report = run_case(
            &case,
            PositionEncoding::Utf16,
            ReferencePolicy::BindingsOptional,
            None,
            &mut client,
            false,
            true,
        );

        assert_eq!(report.status, CaseStatus::Failed);
        let declaration = report.declaration_to_usages.unwrap();
        assert_eq!(declaration.missing.len(), 1);
        assert_eq!(declaration.unexpected_unproven.len(), 1);
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

        let report = run_case(
            &case,
            PositionEncoding::Utf16,
            ReferencePolicy::BindingsOptional,
            None,
            &mut client,
            false,
            true,
        );

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

        let report = run_case(
            &case,
            PositionEncoding::Utf16,
            ReferencePolicy::BindingsOptional,
            None,
            &mut client,
            false,
            true,
        );

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

        let report = run_case(
            &case,
            PositionEncoding::Utf16,
            ReferencePolicy::BindingsOptional,
            None,
            &mut client,
            false,
            false,
        );

        assert_eq!(report.status, CaseStatus::ExpectedFailure);
        assert_eq!(
            report.expected_failure_reason.as_deref(),
            Some("current Bifrost baseline misses this usage")
        );
        let totals = compute_totals(&[DocumentRunReport {
            case_file: "benchmarks/cases/rust.yaml".to_string(),
            language: "rust".to_string(),
            source_root: "/repo/fixtures/rust".to_string(),
            corpus_partition: crate::CorpusPartition::Development,
            corpus_selection: crate::CorpusSelection::AnalyzerInformed,
            ground_truth_status: crate::GroundTruthReviewStatus::LegacyUnattributed,
            reference_policy: ReferencePolicy::BindingsOptional,
            cases: vec![report],
        }]);
        assert_eq!(totals.failed, 0);
        assert_eq!(totals.expected_failures, 1);
        assert_eq!(totals.development_cases, 1);
        assert_eq!(totals.evaluation_cases, 0);
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

        let report = run_case(
            &case,
            PositionEncoding::Utf16,
            ReferencePolicy::BindingsOptional,
            None,
            &mut client,
            false,
            false,
        );

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

        let report = run_case(
            &case,
            PositionEncoding::Utf16,
            ReferencePolicy::BindingsOptional,
            None,
            &mut client,
            false,
            false,
        );

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
            corpus_partition: crate::CorpusPartition::Development,
            corpus_selection: crate::CorpusSelection::AnalyzerInformed,
            ground_truth_status: crate::GroundTruthReviewStatus::LegacyUnattributed,
            reference_policy: ReferencePolicy::BindingsOptional,
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

        let report = run_case(
            &case,
            PositionEncoding::Utf16,
            ReferencePolicy::BindingsOptional,
            None,
            &mut client,
            false,
            false,
        );

        assert_eq!(report.status, CaseStatus::Improved);
        assert_eq!(report.diagnostics[0].kind, "expected_failure_passed");
        let totals = compute_totals(&[DocumentRunReport {
            case_file: "benchmarks/cases/rust.yaml".to_string(),
            language: "rust".to_string(),
            source_root: "/repo/fixtures/rust".to_string(),
            corpus_partition: crate::CorpusPartition::Development,
            corpus_selection: crate::CorpusSelection::AnalyzerInformed,
            ground_truth_status: crate::GroundTruthReviewStatus::LegacyUnattributed,
            reference_policy: ReferencePolicy::BindingsOptional,
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

        let report = run_case(
            &case,
            PositionEncoding::Utf16,
            ReferencePolicy::BindingsOptional,
            None,
            &mut client,
            false,
            false,
        );

        assert_eq!(report.status, CaseStatus::Unsupported);
        assert_eq!(
            report.unsupported_reason.as_deref(),
            Some("not implemented")
        );
        let totals = compute_totals(&[DocumentRunReport {
            case_file: "benchmarks/cases/rust.yaml".to_string(),
            language: "rust".to_string(),
            source_root: "/repo/fixtures/rust".to_string(),
            corpus_partition: crate::CorpusPartition::Development,
            corpus_selection: crate::CorpusSelection::AnalyzerInformed,
            ground_truth_status: crate::GroundTruthReviewStatus::LegacyUnattributed,
            reference_policy: ReferencePolicy::BindingsOptional,
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

        let report = run_case(
            &case,
            PositionEncoding::Utf16,
            ReferencePolicy::BindingsOptional,
            None,
            &mut client,
            false,
            true,
        );

        assert_eq!(report.status, CaseStatus::Failed);
        assert_eq!(report.usage_to_declaration[0].status, CaseStatus::Failed);
    }

    #[test]
    fn no_movement_lookup_accepts_explicit_no_definition() {
        let mut lookup = benchmark_case().usage_lookups.remove(0);
        lookup.expect_no_movement = true;
        lookup.expected_declaration = lookup.usage.clone();
        let mut client = MockClient::new(vec![tool(
            GET_DEFINITIONS_BY_LOCATION_TOOL,
            get_definitions_by_location_json("no_definition", Vec::new()),
        )]);

        let report = run_usage_to_declaration(&lookup, PositionEncoding::Utf16, &mut client);

        assert_eq!(report.status, CaseStatus::Passed);
        assert_eq!(report.raw_status, "no_movement");
    }

    #[test]
    fn no_movement_lookup_rejects_navigation_to_another_token() {
        let mut lookup = benchmark_case().usage_lookups.remove(0);
        lookup.expect_no_movement = true;
        lookup.expected_declaration = lookup.usage.clone();
        let mut client = MockClient::new(vec![tool(
            GET_DEFINITIONS_BY_LOCATION_TOOL,
            get_definitions_by_location_json("resolved", vec![("src/service.rs", 30)]),
        )]);

        let report = run_usage_to_declaration(&lookup, PositionEncoding::Utf16, &mut client);

        assert_eq!(report.status, CaseStatus::Failed);
    }

    #[test]
    fn declaration_lookup_is_unsupported_without_calling_definition_tool() {
        let mut case = benchmark_case();
        case.declaration = None;
        case.usage_lookups[0].operation = crate::NavigationOperation::Declaration;
        let mut client = MockClient::new(Vec::new());

        let report = run_case(
            &case,
            PositionEncoding::Utf16,
            ReferencePolicy::BindingsOptional,
            None,
            &mut client,
            false,
            true,
        );

        assert_eq!(report.status, CaseStatus::Unsupported);
        assert_eq!(
            report.usage_to_declaration[0].raw_status,
            "declaration_lookup_unsupported"
        );
        assert_eq!(
            report.usage_to_declaration[0].operation,
            crate::NavigationOperation::Declaration
        );
    }

    #[test]
    fn resolved_definition_alternates_have_a_machine_readable_failure() {
        let lookup = benchmark_case().usage_lookups.remove(0);
        let mut client = MockClient::new(vec![tool(
            GET_DEFINITIONS_BY_LOCATION_TOOL,
            get_definitions_by_location_json(
                "resolved",
                vec![("src/service.rs", 30), ("src/implementation.rs", 12)],
            ),
        )]);

        let report = run_usage_to_declaration(&lookup, PositionEncoding::Utf16, &mut client);

        assert_eq!(report.status, CaseStatus::Failed);
        assert_eq!(report.raw_status, "multiple_targets");
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

        let report = run_case(
            &case,
            PositionEncoding::Utf16,
            ReferencePolicy::BindingsOptional,
            None,
            &mut client,
            false,
            false,
        );

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

        let report = run_case(
            &case,
            PositionEncoding::Utf16,
            ReferencePolicy::BindingsOptional,
            None,
            &mut client,
            false,
            false,
        );

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
                    end_line: None,
                    end_column: None,
                    display_name: None,
                    kind: None,
                },
                NormalizedLocation {
                    path: "src/service.cpp".to_string(),
                    line: 17,
                    column: None,
                    end_line: None,
                    end_column: None,
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
    fn parse_scan_usages_preserves_unproven_result_locations_separately() {
        let parsed = parse_scan_usages(&json!({
            "summary": {"partial": false},
            "results": [{
                "status": "found",
                "files": [{
                    "path": "src/service_test.go",
                    "hits": [{"line": 9}]
                }],
                "unproven_files": [{
                    "path": "src/service.go",
                    "hits": [{"line": 29}]
                }, {
                    "path": "src/service_test.go",
                    "hits": [{"line": 9}]
                }]
            }]
        }));

        assert_eq!(
            parsed.locations,
            vec![NormalizedLocation {
                path: "src/service_test.go".to_string(),
                line: 9,
                column: None,
                end_line: None,
                end_column: None,
                display_name: None,
                kind: None,
            }]
        );
        assert_eq!(
            parsed.unproven_locations,
            vec![NormalizedLocation {
                path: "src/service.go".to_string(),
                line: 29,
                column: None,
                end_line: None,
                end_column: None,
                display_name: None,
                kind: None,
            }]
        );
        assert_eq!(parsed.raw_statuses, vec!["found".to_string()]);
        assert!(!parsed.partial);
    }

    #[test]
    fn parse_scan_usages_excludes_labeled_override_declarations() {
        let parsed = parse_scan_usages(&json!({
            "summary": {"partial": false},
            "results": [{
                "status": "found",
                "files": [{
                    "path": "src/Handler.java",
                    "hits": [
                        {"line": 5, "kind": "override_declaration"},
                        {"line": 12}
                    ]
                }]
            }]
        }));

        assert_eq!(
            parsed.locations,
            vec![NormalizedLocation {
                path: "src/Handler.java".to_string(),
                line: 12,
                column: None,
                end_line: None,
                end_column: None,
                display_name: None,
                kind: None,
            }]
        );
        assert_eq!(parsed.raw_statuses, vec!["found".to_string()]);
        assert_eq!(
            parsed.override_declarations,
            vec![NormalizedLocation {
                path: "src/Handler.java".to_string(),
                line: 5,
                column: None,
                end_line: None,
                end_column: None,
                display_name: None,
                kind: None,
            }]
        );
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
        let parsed = parse_scan_usages(&json!({
            "summary": {"partial": false},
            "usages": [{
                "files": [{
                    "path": "src/lib.rs",
                    "hits": [{"line": 8}]
                }],
                "unproven_files": [{
                    "path": "src/conservative.rs",
                    "hits": [{"line": 12}]
                }]
            }]
        }));

        assert_eq!(
            parsed.locations,
            vec![NormalizedLocation {
                path: "src/lib.rs".to_string(),
                line: 8,
                column: None,
                end_line: None,
                end_column: None,
                display_name: None,
                kind: None,
            }]
        );
        assert_eq!(
            parsed.unproven_locations,
            vec![NormalizedLocation {
                path: "src/conservative.rs".to_string(),
                line: 12,
                column: None,
                end_line: None,
                end_column: None,
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

        let report = run_case(
            &case,
            PositionEncoding::Utf16,
            ReferencePolicy::BindingsOptional,
            None,
            &mut client,
            false,
            false,
        );

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

        let report = run_case(
            &case,
            PositionEncoding::Utf16,
            ReferencePolicy::BindingsOptional,
            None,
            &mut client,
            false,
            false,
        );

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

        let report = run_case(
            &case,
            PositionEncoding::Utf16,
            ReferencePolicy::BindingsOptional,
            None,
            &mut client,
            false,
            true,
        );

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

        let report = run_case(
            &case,
            PositionEncoding::Utf16,
            ReferencePolicy::BindingsOptional,
            None,
            &mut client,
            false,
            false,
        );

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

    struct MockClient {
        responses: VecDeque<(String, Value)>,
        calls: Vec<(String, Value)>,
    }

    impl MockClient {
        fn new(responses: Vec<(String, Value)>) -> Self {
            Self {
                responses: VecDeque::from(responses),
                calls: Vec::new(),
            }
        }
    }

    impl SearchToolsClient for MockClient {
        fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value> {
            let (expected_name, value) = self.responses.pop_front().unwrap();
            assert_eq!(expected_name, name);
            self.calls.push((name.to_string(), arguments));
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
            end_line: None,
            end_column: None,
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
            expected_unproven_usages: Vec::new(),
            allowed_extra_usages: Vec::new(),
            allowed_unproven_usages: Vec::new(),
            usage_lookups: vec![UsageLookup {
                operation: crate::NavigationOperation::ProfileDefault,
                expect_no_movement: false,
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
                    let column = if path == "src/lib.rs" && line == 8 { 19 } else { 1 };
                    json!({
                        "path": path,
                        "hits": [{
                            "line": line,
                            "column": column,
                            "end_line": line,
                            "end_column": column + 1,
                            "enclosing": "run_demo"
                        }]
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
                            "start_column": 12,
                            "end_line": line,
                            "end_column": 13,
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
                        "start_column": 8,
                        "end_line": line,
                        "end_column": 9,
                        "kind": "function",
                        "language": "rust"
                    })
                }).collect::<Vec<_>>(),
                "diagnostics": []
            }]
        })
    }
}
