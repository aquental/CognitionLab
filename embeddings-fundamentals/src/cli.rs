//! Command-line interface. `main` only parses arguments and maps the outcome to an
//! exit code; every command delegates to library code.

use std::process::ExitCode;

use clap::{Parser, Subcommand};

use crate::tools;

pub use crate::pipeline::{EXIT_INCOMPLETE, EXIT_VALIDATION};

/// Medical Document Embedding Lab.
#[derive(Debug, Parser)]
#[command(name = "embeddings-fundamentals", version, about)]
pub struct Cli {
    /// Command to run.
    #[command(subcommand)]
    pub command: Command,
}

/// Available commands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Check that the external extraction and OCR tools are installed.
    Doctor,
}

/// Runs the parsed command.
///
/// # Errors
///
/// Returns an error when a command fails unexpectedly.
pub async fn run(cli: Cli) -> anyhow::Result<ExitCode> {
    match cli.command {
        Command::Doctor => doctor().await,
    }
}

async fn doctor() -> anyhow::Result<ExitCode> {
    let statuses = tools::probe(tools::REQUIRED_TOOLS).await;
    let mut missing = 0usize;
    for status in &statuses {
        match &status.version {
            Some(version) => println!("ok       {:<10} {version}", status.name),
            None => {
                missing += 1;
                println!("missing  {}", status.name);
            }
        }
    }
    Ok(if missing == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(EXIT_VALIDATION)
    })
}
