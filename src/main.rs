use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use usagebench::bifrost_runner::{
    run_bifrost, BifrostRunReport, CaseStatus, NormalizedLocation, RunBifrostOptions,
};

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
    /// Print the JSON Schema generated for Bifrost run reports.
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
        /// Directory for temporary checkouts and runner artifacts.
        #[arg(long, default_value = "target/usagebench")]
        work_dir: PathBuf,
        /// Write the machine-readable report JSON to this path.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Run cases marked unsupported instead of reporting them as skipped.
        #[arg(long)]
        include_unsupported: bool,
        /// Run usage-to-definition probes that require Bifrost get_definition support.
        #[arg(long)]
        include_definition_lookups: bool,
        /// Keep temporary git source checkouts after the run.
        #[arg(long)]
        keep_worktrees: bool,
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
        Command::BifrostReportSchema => {
            println!(
                "{}",
                usagebench::bifrost_runner::generated_bifrost_report_schema_json()?
            );
        }
        Command::RunBifrost {
            path,
            bifrost_repo,
            bifrost_commit,
            work_dir,
            output,
            include_unsupported,
            include_definition_lookups,
            keep_worktrees,
        } => {
            let mut options = RunBifrostOptions::with_defaults(path);
            options.bifrost_repo = bifrost_repo;
            options.bifrost_commit = bifrost_commit;
            options.work_dir = work_dir;
            options.output = output;
            options.include_unsupported = include_unsupported;
            options.include_definition_lookups = include_definition_lookups;
            options.keep_worktrees = keep_worktrees;
            let report = run_bifrost(options)?;
            println!(
                "ran {} case(s): {} passed, {} failed, {} expected failure(s), {} skipped, {} error(s)",
                report.totals.cases,
                report.totals.passed,
                report.totals.failed,
                report.totals.expected_failures,
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
    }

    Ok(())
}

fn print_run_details(report: &BifrostRunReport) {
    for document in &report.documents {
        for case in &document.cases {
            let Some(declaration) = &case.declaration_to_usages else {
                if matches!(
                    case.status,
                    CaseStatus::Failed | CaseStatus::ExpectedFailure | CaseStatus::Error
                ) {
                    println!(
                        "{} {}: {}",
                        status_label(case.status),
                        case.id,
                        document.case_file
                    );
                }
                continue;
            };

            if declaration.missing.is_empty()
                && declaration.unexpected.is_empty()
                && !matches!(
                    case.status,
                    CaseStatus::Failed | CaseStatus::ExpectedFailure | CaseStatus::Error
                )
            {
                continue;
            }

            println!(
                "{} {}: {} missing, {} extra ({})",
                status_label(case.status),
                case.id,
                declaration.missing.len(),
                declaration.unexpected.len(),
                document.case_file
            );
            print_locations("missing", &declaration.missing);
            print_locations("extra", &declaration.unexpected);
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
        Some(column) => format!("{}:{}:{}", location.path, location.line, column),
        None => format!("{}:{}", location.path, location.line),
    }
}

fn status_label(status: CaseStatus) -> &'static str {
    match status {
        CaseStatus::Passed => "PASS",
        CaseStatus::Failed => "FAIL",
        CaseStatus::ExpectedFailure => "XFAIL",
        CaseStatus::Skipped => "SKIP",
        CaseStatus::Error => "ERROR",
    }
}
