use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

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
    }

    Ok(())
}
