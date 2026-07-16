use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use usagebench::bifrost_runner::{
    run_bifrost, BifrostRunReport, CaseStatus, NormalizedLocation, RunBifrostOptions,
    TypeLookupReport, UsageDefinitionReport,
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
    /// Validate benchmark case YAML files.
    Validate {
        /// Case file or directory to validate.
        path: PathBuf,
    },
    /// Print the JSON Schema generated from the Rust model.
    Schema,
    /// Print the JSON Schema generated for analyzer run reports.
    ReportSchema,
    /// Deprecated compatibility alias for `report-schema`.
    #[command(hide = true)]
    BifrostReportSchema,
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
        /// Keep temporary git source checkouts after the run.
        #[arg(long)]
        keep_worktrees: bool,
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
        Command::Validate { path } => {
            let files = usagebench::validate_path(&path)?;
            println!("validated {} benchmark case file(s)", files.len());
        }
        Command::Schema => {
            println!("{}", usagebench::generated_schema_json()?);
        }
        Command::ReportSchema | Command::BifrostReportSchema => {
            println!("{}", usagebench::runners::generated_report_schema_json()?);
        }
        Command::RunBifrost {
            path,
            bifrost_repo,
            bifrost_commit,
            bifrost_working_tree,
            work_dir,
            output,
            include_unsupported,
            include_definition_lookups: _,
            keep_worktrees,
        } => {
            let mut options = RunBifrostOptions::with_defaults(path);
            options.bifrost_repo = bifrost_repo;
            options.bifrost_commit = bifrost_commit;
            options.bifrost_working_tree = bifrost_working_tree;
            options.work_dir = work_dir;
            options.output = output;
            options.include_unsupported = include_unsupported;
            options.keep_worktrees = keep_worktrees;
            let report = run_bifrost(options)?;
            println!(
                "ran {} planned case(s): {} passed, {} near miss(es), {} improved, {} failed, {} expected failure(s), {} not planned, {} unsupported, {} skipped, {} error(s)",
                report.totals.cases,
                report.totals.passed,
                report.totals.near_misses,
                report.totals.improved,
                report.totals.failed,
                report.totals.expected_failures,
                report.totals.not_planned,
                report.totals.unsupported,
                report.totals.skipped,
                report.totals.errors
            );
            print_run_details(&report);
            if report.totals.failed > 0 || report.totals.errors > 0 {
                bail!(
                    "Bifrost benchmark run failed: {} failed, {} error(s)",
                    report.totals.failed,
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
                "ran {} planned case(s) with {} {}: {} passed, {} near miss(es), {} failed, {} not planned, {} unsupported, {} skipped, {} error(s)",
                report.totals.cases,
                report.runner.name,
                report.runner.resolved_version,
                report.totals.passed,
                report.totals.near_misses,
                report.totals.failed,
                report.totals.not_planned,
                report.totals.unsupported,
                report.totals.skipped,
                report.totals.errors
            );
            print_run_details(&report);
            if report.totals.failed > 0 || report.totals.errors > 0 {
                bail!(
                    "LSP benchmark run failed: {} failed, {} error(s)",
                    report.totals.failed,
                    report.totals.errors
                );
            }
        }
    }

    Ok(())
}

fn print_run_details(report: &BifrostRunReport) {
    for document in &report.documents {
        for case in &document.cases {
            let Some(declaration) = &case.declaration_to_usages else {
                if matches!(
                    case.status,
                    CaseStatus::NearMiss
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
            "  usage lookup {}: {} expected {}, got {} ({})",
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
}
