use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use usagebench::bifrost_runner::{run_bifrost, RunBifrostOptions};

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
            keep_worktrees,
        } => {
            let mut options = RunBifrostOptions::with_defaults(path);
            options.bifrost_repo = bifrost_repo;
            options.bifrost_commit = bifrost_commit;
            options.work_dir = work_dir;
            options.output = output;
            options.include_unsupported = include_unsupported;
            options.keep_worktrees = keep_worktrees;
            let report = run_bifrost(options)?;
            println!(
                "ran {} case(s): {} passed, {} failed, {} skipped, {} error(s)",
                report.totals.cases,
                report.totals.passed,
                report.totals.failed,
                report.totals.skipped,
                report.totals.errors
            );
        }
    }

    Ok(())
}
