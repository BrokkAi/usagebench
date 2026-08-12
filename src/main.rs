use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use usagebench::bifrost_runner::{
    run_bifrost, BifrostRunReport, CaseStatus, NormalizedLocation, RunBifrostOptions,
    TypeLookupReport, UsageDefinitionReport, DEFAULT_SCAN_USAGES_MAX_DURATION_SECS,
};
use usagebench::freeze::{create_manifest, FreezeManifestOptions, SnapshotKind};
use usagebench::real_project::{
    capture_population, draw_selection, require_committed_population, CapturePopulationOptions,
    DrawSelectionOptions,
};
use usagebench::runners::lsp::{run_lsp, RunLspOptions};

#[derive(Debug, Parser)]
#[command(name = "usagebench")]
#[command(about = "Validate usage benchmark case files")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Capture a source-only GitHub repository population for a real-project slice.
    CaptureRealProjectPopulation {
        /// Frozen real-project protocol to bind into the snapshot.
        #[arg(
            long,
            default_value = "benchmarks/evaluation/real-project-v1/protocol.json"
        )]
        protocol: PathBuf,
        /// Destination population manifest.
        #[arg(long)]
        output: Option<PathBuf>,
        /// GitHub REST API base URL.
        #[arg(long, default_value = "https://api.github.com")]
        github_api_base: String,
        /// Optional UTC RFC3339 capture timestamp, useful for deterministic fixture runs.
        #[arg(long)]
        captured_at: Option<String>,
    },
    /// Draw a real-project selection from an already captured population snapshot.
    DrawRealProjectSelection {
        /// Frozen real-project protocol used for the source-only draw.
        #[arg(
            long,
            default_value = "benchmarks/evaluation/real-project-v1/protocol.json"
        )]
        protocol: PathBuf,
        /// Previously captured and committed population snapshot.
        #[arg(long)]
        population: Option<PathBuf>,
        /// Destination selection manifest.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Exact commit that introduced the frozen protocol.
        #[arg(long)]
        protocol_commit: String,
    },
    /// Validate benchmark case YAML files.
    Validate {
        /// Case file or directory to validate.
        path: PathBuf,
    },
    /// Validate promoted evaluation case files and their frozen evidence links.
    ValidateEvaluation {
        /// Evaluation case file or directory to validate.
        path: PathBuf,
    },
    /// Validate a retrospective legacy-promotion manifest and its hash-bound evidence.
    ValidateLegacyPromotion { manifest: PathBuf },
    /// Print the JSON Schema generated from the Rust model.
    Schema,
    /// Print the JSON Schema generated for analyzer run reports.
    ReportSchema,
    /// Deprecated compatibility alias for `report-schema`.
    #[command(hide = true)]
    BifrostReportSchema,
    /// Compare two reports after removing documented volatile fields.
    CompareReports {
        /// Previously published report.
        expected: PathBuf,
        /// Newly reproduced report.
        actual: PathBuf,
        /// Optional complete machine-readable semantic diff.
        #[arg(long)]
        output_diff: Option<PathBuf>,
    },
    /// Validate selected candidate reports and write an immutable snapshot manifest.
    FreezeManifest {
        /// Development evidence is explicitly labeled; evaluation requires promoted corpus metadata.
        #[arg(long)]
        snapshot_kind: SnapshotKind,
        /// Immutable benchmark tag to create, for example v0.2.0.
        #[arg(long)]
        version: String,
        /// Exact 40-character UsageBench commit being frozen.
        #[arg(long)]
        revision: String,
        /// Central registry of candidate versions and revisions.
        #[arg(long, default_value = "adapters/candidates.json")]
        candidates_file: PathBuf,
        /// Comma-separated candidate IDs, in the order they should appear in the manifest.
        #[arg(long, value_delimiter = ',')]
        candidates: Vec<String>,
        /// Report produced by each selected candidate. Repeat once per candidate.
        #[arg(long, required = true)]
        report: Vec<PathBuf>,
        /// Promoted evaluation corpus to validate and bind into an evaluation snapshot.
        #[arg(long)]
        evaluation_corpus: Option<PathBuf>,
        /// Versioned retrospective promotion manifest for a legacy-promoted snapshot.
        #[arg(long)]
        promotion_manifest: Option<PathBuf>,
        /// Destination for the machine-readable snapshot manifest.
        #[arg(long)]
        output: PathBuf,
    },
    /// Generate public result fragments from a verified immutable snapshot.
    GenerateResults {
        /// Freeze manifest beside the raw report JSON files.
        #[arg(long)]
        manifest: PathBuf,
        /// Directory containing the generated Markdown fragments.
        #[arg(long)]
        output_directory: PathBuf,
        /// Fail instead of writing when generated fragments differ from disk.
        #[arg(long)]
        check: bool,
    },
    /// Run benchmark case YAML files against Bifrost.
    RunBifrost {
        /// Case file or directory to run.
        path: PathBuf,
        /// Bifrost git checkout to fetch, checkout, and build.
        #[arg(long)]
        bifrost_repo: Option<PathBuf>,
        /// Bifrost commit or ref to test.
        #[arg(long, default_value = "origin/master")]
        bifrost_commit: String,
        /// Build and run the provided Bifrost checkout directly, including local commits and uncommitted changes.
        #[arg(long)]
        bifrost_working_tree: bool,
        /// Run an already-built Bifrost executable without fetching or compiling at runtime.
        #[arg(long, requires = "bifrost_resolved_commit")]
        bifrost_binary: Option<PathBuf>,
        /// Exact source commit used to build --bifrost-binary.
        #[arg(long, requires = "bifrost_binary")]
        bifrost_resolved_commit: Option<String>,
        /// Directory for temporary checkouts and runner artifacts.
        #[arg(long, default_value = "target/usagebench")]
        work_dir: PathBuf,
        /// Write the machine-readable report JSON to this path.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Run cases marked unsupported instead of reporting only the unsupported boundary.
        #[arg(long)]
        include_unsupported: bool,
        /// Deprecated; definition lookups are enabled by default.
        #[arg(long)]
        include_definition_lookups: bool,
        /// Per-scan wall-clock budget for Bifrost usage lookup (maximum 300 seconds).
        #[arg(long, default_value_t = DEFAULT_SCAN_USAGES_MAX_DURATION_SECS)]
        scan_usages_max_duration_secs: u64,
        /// Keep temporary git source checkouts after the run.
        #[arg(long)]
        keep_worktrees: bool,
        /// Run only the matching case ID.
        #[arg(long)]
        case_id: Option<String>,
    },
    /// Run benchmark cases against a versioned language-server profile.
    RunLsp {
        /// Case file or directory to run.
        path: PathBuf,
        /// JSON profile describing the language server and requested release.
        #[arg(long)]
        profile: PathBuf,
        /// Override the profile's executable while retaining its arguments.
        #[arg(long)]
        server_command: Option<PathBuf>,
        /// Directory for isolated source copies and runner artifacts.
        #[arg(long, default_value = "target/usagebench")]
        work_dir: PathBuf,
        /// Write the machine-readable report JSON to this path.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Run cases marked unsupported instead of reporting only the unsupported boundary.
        #[arg(long)]
        include_unsupported: bool,
        /// Keep isolated source copies after the run.
        #[arg(long)]
        keep_worktrees: bool,
        /// Run only the matching case ID after language filtering.
        #[arg(long)]
        case_id: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::CaptureRealProjectPopulation {
            protocol,
            output,
            github_api_base,
            captured_at,
        } => {
            let output = sibling_artifact(&protocol, output, "population.json")?;
            let snapshot = capture_population(CapturePopulationOptions {
                protocol,
                output: output.clone(),
                github_api_base,
                captured_at,
            })?;
            println!(
                "captured {} profile population(s) to {}",
                snapshot.profiles.len(),
                output.display()
            );
        }
        Command::DrawRealProjectSelection {
            protocol,
            population,
            output,
            protocol_commit,
        } => {
            let population = sibling_artifact(&protocol, population, "population.json")?;
            let output = sibling_artifact(&protocol, output, "selection.json")?;
            require_committed_population(&population)?;
            let selection = draw_selection(DrawSelectionOptions {
                protocol,
                population,
                output: output.clone(),
                protocol_commit,
            })?;
            println!(
                "drew {} profile selection(s) to {}",
                selection.profiles.len(),
                output.display()
            );
        }
        Command::Validate { path } => {
            let files = usagebench::validate_path(&path)?;
            println!("validated {} benchmark case file(s)", files.len());
        }
        Command::ValidateEvaluation { path } => {
            let files = usagebench::evaluation::validate_path(&path)?;
            println!(
                "validated {} evaluation benchmark case file(s)",
                files.len()
            );
        }
        Command::ValidateLegacyPromotion { manifest } => {
            let audit = usagebench::promotion::build_promotion_audit(&manifest)?;
            println!(
                "validated legacy promotion {} with {} balanced-core cases",
                audit.promotion_id, audit.balanced_core_case_count
            );
        }
        Command::Schema => {
            println!("{}", usagebench::generated_schema_json()?);
        }
        Command::ReportSchema | Command::BifrostReportSchema => {
            println!("{}", usagebench::runners::generated_report_schema_json()?);
        }
        Command::CompareReports {
            expected,
            actual,
            output_diff,
        } => {
            let differences =
                usagebench::runners::report_compare::compare_report_files(&expected, &actual)?;
            if let Some(path) = output_diff {
                usagebench::runners::report_compare::write_differences(&path, &differences)?;
            }
            if differences.is_empty() {
                println!("reports are semantically equivalent");
            } else {
                for difference in &differences {
                    println!(
                        "{}: expected {}, actual {}",
                        difference.path,
                        usagebench::runners::report_compare::compact(&difference.expected),
                        usagebench::runners::report_compare::compact(&difference.actual)
                    );
                }
                bail!("reports differ in {} semantic field(s)", differences.len());
            }
        }
        Command::FreezeManifest {
            snapshot_kind,
            version,
            revision,
            candidates_file,
            candidates,
            report,
            evaluation_corpus,
            promotion_manifest,
            output,
        } => {
            let manifest = create_manifest(FreezeManifestOptions {
                snapshot_kind,
                version,
                revision,
                candidates_file,
                candidate_ids: candidates,
                report_paths: report,
                evaluation_corpus,
                promotion_manifest,
            })?;
            usagebench::freeze::write_manifest(&output, &manifest)?;
            println!(
                "wrote {} {} snapshot manifest for {} candidate(s) to {}",
                manifest.snapshot_kind,
                manifest.version,
                manifest.candidates.len(),
                output.display()
            );
        }
        Command::GenerateResults {
            manifest,
            output_directory,
            check,
        } => {
            usagebench::results::write_result_pages(&manifest, &output_directory, check)?;
            if check {
                println!("generated result pages are current");
            } else {
                println!(
                    "wrote generated result pages to {}",
                    output_directory.display()
                );
            }
        }
        Command::RunBifrost {
            path,
            bifrost_repo,
            bifrost_commit,
            bifrost_working_tree,
            bifrost_binary,
            bifrost_resolved_commit,
            work_dir,
            output,
            include_unsupported,
            include_definition_lookups: _,
            scan_usages_max_duration_secs,
            keep_worktrees,
            case_id,
        } => {
            let mut options = RunBifrostOptions::with_defaults(path);
            options.bifrost_repo = bifrost_repo;
            options.bifrost_commit = bifrost_commit;
            options.bifrost_working_tree = bifrost_working_tree;
            options.bifrost_binary = bifrost_binary;
            options.bifrost_resolved_commit = bifrost_resolved_commit;
            options.work_dir = work_dir;
            options.output = output;
            options.include_unsupported = include_unsupported;
            options.scan_usages_max_duration_secs = scan_usages_max_duration_secs;
            options.keep_worktrees = keep_worktrees;
            options.case_id = case_id;
            let report = run_bifrost(options)?;
            println!(
                "ran {} planned case(s) ({} development, {} evaluation): {} passed, {} near miss(es), {} position-unverified, {} improved, {} failed, {} expected failure(s), {} not planned, {} unsupported, {} skipped, {} error(s)",
                report.totals.cases,
                report.totals.development_cases,
                report.totals.evaluation_cases,
                report.totals.passed,
                report.totals.near_misses,
                report.totals.position_unverified,
                report.totals.improved,
                report.totals.failed,
                report.totals.expected_failures,
                report.totals.not_planned,
                report.totals.unsupported,
                report.totals.skipped,
                report.totals.errors
            );
            print_required_destination_totals(&report);
            print_location_metrics(&report);
            print_run_details(&report);
            if report.totals.failed > 0
                || report.totals.position_unverified > 0
                || report.totals.errors > 0
            {
                bail!(
                    "Bifrost benchmark run was not exact: {} failed, {} position-unverified, {} error(s)",
                    report.totals.failed,
                    report.totals.position_unverified,
                    report.totals.errors
                );
            }
        }
        Command::RunLsp {
            path,
            profile,
            server_command,
            work_dir,
            output,
            include_unsupported,
            keep_worktrees,
            case_id,
        } => {
            let mut options = RunLspOptions::with_defaults(path, profile);
            options.server_command = server_command;
            options.work_dir = work_dir;
            options.output = output;
            options.include_unsupported = include_unsupported;
            options.keep_worktrees = keep_worktrees;
            options.case_id = case_id;
            let report = run_lsp(options)?;
            println!(
                "ran {} planned case(s) ({} development, {} evaluation) with {} {}: {} passed, {} near miss(es), {} position-unverified, {} failed, {} not planned, {} unsupported, {} skipped, {} error(s)",
                report.totals.cases,
                report.totals.development_cases,
                report.totals.evaluation_cases,
                report.runner.name,
                report.runner.resolved_version,
                report.totals.passed,
                report.totals.near_misses,
                report.totals.position_unverified,
                report.totals.failed,
                report.totals.not_planned,
                report.totals.unsupported,
                report.totals.skipped,
                report.totals.errors
            );
            print_required_destination_totals(&report);
            print_location_metrics(&report);
            print_run_details(&report);
            if report.totals.failed > 0
                || report.totals.position_unverified > 0
                || report.totals.errors > 0
            {
                bail!(
                    "LSP benchmark run was not exact: {} failed, {} position-unverified, {} error(s)",
                    report.totals.failed,
                    report.totals.position_unverified,
                    report.totals.errors
                );
            }
        }
    }

    Ok(())
}

fn sibling_artifact(
    protocol: &std::path::Path,
    explicit: Option<PathBuf>,
    file_name: &str,
) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path);
    }
    let parent = protocol
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| anyhow::anyhow!("protocol path must have a parent directory"))?;
    Ok(parent.join(file_name))
}

fn print_required_destination_totals(report: &BifrostRunReport) {
    let totals = &report.totals.required_destinations;
    println!(
        "required destinations: {}/{} found ({} missing), {} not planned, {} unsupported, {} skipped, {} error(s), {} unreported",
        totals.found,
        totals.scoreable_cases,
        totals.missing,
        totals.not_planned,
        totals.unsupported,
        totals.skipped,
        totals.errors,
        totals.unreported,
    );
}

fn print_location_metrics(report: &BifrostRunReport) {
    let Some(metrics) = &report.totals.location_metrics else {
        println!("location metrics: unavailable (requires UsageBench >=0.2.0)");
        return;
    };
    println!(
        "location metrics: {} TP, {} FP, {} FN, {} exact, {} policy-allowed extra(s), {}/{} exact-set case(s) across {} query/queries",
        metrics.true_positives,
        metrics.false_positives,
        metrics.false_negatives,
        metrics.range_quality.exact_token,
        metrics.returned_locations.policy_allowed,
        metrics.exact_set_cases,
        metrics.cases,
        metrics.queries,
    );
}

fn print_run_details(report: &BifrostRunReport) {
    for document in &report.documents {
        for case in &document.cases {
            let Some(declaration) = &case.declaration_to_usages else {
                if matches!(
                    case.status,
                    CaseStatus::NearMiss
                        | CaseStatus::PositionUnverified
                        | CaseStatus::Improved
                        | CaseStatus::Failed
                        | CaseStatus::ExpectedFailure
                        | CaseStatus::NotPlanned
                        | CaseStatus::Unsupported
                        | CaseStatus::Error
                ) {
                    println!(
                        "{} {}: {}",
                        status_label(case.status),
                        safe_display(&case.id),
                        safe_display(&document.case_file)
                    );
                    print_usage_definition_issues(&case.usage_to_declaration);
                    print_type_lookup_issues(&case.type_lookups);
                }
                continue;
            };

            if declaration.missing.is_empty()
                && declaration.missing_unproven.is_empty()
                && declaration.unexpected.is_empty()
                && declaration.unexpected_unproven.is_empty()
                && !matches!(
                    case.status,
                    CaseStatus::NearMiss
                        | CaseStatus::PositionUnverified
                        | CaseStatus::Improved
                        | CaseStatus::Failed
                        | CaseStatus::ExpectedFailure
                        | CaseStatus::NotPlanned
                        | CaseStatus::Unsupported
                        | CaseStatus::Error
                )
            {
                continue;
            }

            println!(
                "{} {}: {} proven missing, {} conservative missing, {} proven extra, {} unproven extra ({})",
                status_label(case.status),
                safe_display(&case.id),
                declaration.missing.len(),
                declaration.missing_unproven.len(),
                declaration.unexpected.len(),
                declaration.unexpected_unproven.len(),
                safe_display(&document.case_file)
            );
            print_locations("missing", &declaration.missing);
            print_locations("missing conservative", &declaration.missing_unproven);
            print_locations("extra", &declaration.unexpected);
            print_locations("extra unproven", &declaration.unexpected_unproven);
            print_locations("position unverified", &declaration.position_unverified);
            print_usage_definition_issues(&case.usage_to_declaration);
            print_type_lookup_issues(&case.type_lookups);
        }
    }
}

fn print_usage_definition_issues(reports: &[UsageDefinitionReport]) {
    for report in reports {
        if matches!(report.status, CaseStatus::Passed | CaseStatus::Skipped) {
            continue;
        }
        let actual = if report.actual_declarations.is_empty() {
            "none".to_string()
        } else {
            report
                .actual_declarations
                .iter()
                .map(format_location)
                .collect::<Vec<_>>()
                .join(", ")
        };
        println!(
            "  {} lookup {}: {} expected {}, got {} ({})",
            report.operation.as_str(),
            format_location(&report.usage),
            status_label(report.status),
            format_location(&report.expected_declaration),
            actual,
            safe_display(&report.raw_status)
        );
        for diagnostic in &report.diagnostics {
            println!(
                "    {}: {}",
                safe_display(&diagnostic.kind),
                safe_display(&diagnostic.message)
            );
        }
    }
}

fn print_type_lookup_issues(reports: &[TypeLookupReport]) {
    for report in reports {
        if matches!(report.status, CaseStatus::Passed | CaseStatus::Skipped) {
            continue;
        }
        let actual = if report.actual_types.is_empty() {
            "none".to_string()
        } else {
            report
                .actual_types
                .iter()
                .map(format_location)
                .collect::<Vec<_>>()
                .join(", ")
        };
        println!(
            "  type lookup {}: {} expected {}, got {} ({})",
            format_location(&report.expression),
            status_label(report.status),
            format_location(&report.expected_type),
            actual,
            safe_display(&report.raw_status)
        );
        for diagnostic in &report.diagnostics {
            println!(
                "    {}: {}",
                safe_display(&diagnostic.kind),
                safe_display(&diagnostic.message)
            );
        }
    }
}

fn print_locations(label: &str, locations: &[NormalizedLocation]) {
    if locations.is_empty() {
        return;
    }
    let rendered = locations
        .iter()
        .map(format_location)
        .collect::<Vec<_>>()
        .join(", ");
    println!("  {label}: {rendered}");
}

fn format_location(location: &NormalizedLocation) -> String {
    match location.column {
        Some(column) => format!(
            "{}:{}:{}",
            safe_display(&location.path),
            location.line,
            column
        ),
        None => format!("{}:{}", safe_display(&location.path), location.line),
    }
}

fn safe_display(value: &str) -> String {
    value.escape_debug().to_string()
}

fn status_label(status: CaseStatus) -> &'static str {
    match status {
        CaseStatus::Passed => "PASS",
        CaseStatus::NearMiss => "NEAR-MISS",
        CaseStatus::PositionUnverified => "POSITION-UNVERIFIED",
        CaseStatus::Improved => "IMPROVED",
        CaseStatus::Failed => "FAIL",
        CaseStatus::ExpectedFailure => "XFAIL",
        CaseStatus::NotPlanned => "NOTPLANNED",
        CaseStatus::Unsupported => "UNSUPPORTED",
        CaseStatus::Skipped => "SKIP",
        CaseStatus::Error => "ERROR",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_locations_escape_control_characters() {
        let location = NormalizedLocation {
            path: "src/\u{1b}[31mspoof\nPASS.rs".to_string(),
            line: 7,
            column: Some(3),
            end_line: Some(7),
            end_column: Some(4),
            display_name: None,
            kind: None,
        };

        let rendered = format_location(&location);

        assert_eq!(rendered, "src/\\u{1b}[31mspoof\\nPASS.rs:7:3");
    }

    #[test]
    fn safe_display_leaves_plain_text_readable() {
        assert_eq!(
            safe_display("benchmarks/cases/rust.yaml"),
            "benchmarks/cases/rust.yaml"
        );
    }

    #[test]
    fn real_project_artifacts_default_next_to_protocol() {
        let protocol = PathBuf::from("benchmarks/evaluation/real-project-v2/protocol.json");

        assert_eq!(
            sibling_artifact(&protocol, None, "population.json").unwrap(),
            PathBuf::from("benchmarks/evaluation/real-project-v2/population.json")
        );
        assert_eq!(
            sibling_artifact(
                &protocol,
                Some(PathBuf::from("custom.json")),
                "population.json"
            )
            .unwrap(),
            PathBuf::from("custom.json")
        );
    }
}
