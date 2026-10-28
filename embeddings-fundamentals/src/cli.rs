//! Command-line interface. `main` only parses arguments and maps the outcome to an
//! exit code; every command delegates to library code.
//!
//! Exit codes: 0 success, 1 internal error, [`EXIT_INCOMPLETE`] (2) incomplete run,
//! [`EXIT_VALIDATION`] (3) refused by validation (ineligible sources, missing
//! permission, unpinned model, missing tools).

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};

use crate::adapters;
use crate::anonymization;
use crate::config::ExperimentConfig;
use crate::corpus::LoadedManifest;
use crate::extraction::poppler::{PopplerTextExtractor, TesseractOcr};
use crate::ids::{EmbeddingSpaceId, ExperimentId};
use crate::pipeline::{Pipeline, TokioSleeper, ValidationError, check_remote_permission};
use crate::retrieval::{Scope, search_text};
use crate::storage::FsRecordStore;
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
    /// Verify manifest checksums and eligibility (anonymization gate).
    Manifest(ManifestArgs),
    /// Report what an anonymization reviewer must inspect (metadata, filenames).
    AnonymizationCheck(ManifestArgs),
    /// Validate a run without processing: permission, model pin, and capabilities.
    Validate(RunArgs),
    /// Extract, chunk, embed, and publish every eligible source.
    Ingest(IngestArgs),
    /// Search an index with a text query.
    Search(SearchArgs),
}

/// Arguments naming a manifest.
#[derive(Debug, Args)]
pub struct ManifestArgs {
    /// Corpus manifest (`manifest.toml`).
    #[arg(long)]
    pub manifest: PathBuf,
}

/// Arguments of a run.
#[derive(Debug, Args)]
pub struct RunArgs {
    /// Corpus manifest.
    #[arg(long)]
    pub manifest: PathBuf,
    /// Saved experiment configuration.
    #[arg(long)]
    pub config: PathBuf,
}

/// Arguments of `ingest`.
#[derive(Debug, Args)]
pub struct IngestArgs {
    /// Corpus manifest and configuration.
    #[command(flatten)]
    pub run: RunArgs,
    /// Store root directory.
    #[arg(long, default_value = "store")]
    pub store: PathBuf,
    /// Experiment id of this run (snapshots are create-only).
    #[arg(long)]
    pub experiment: ExperimentId,
    /// Write the JSON run report here instead of stdout.
    #[arg(long)]
    pub report: Option<PathBuf>,
}

/// Arguments of `search`.
#[derive(Debug, Args)]
pub struct SearchArgs {
    /// Saved experiment configuration (selects the query model and index space).
    #[arg(long)]
    pub config: PathBuf,
    /// Store root directory.
    #[arg(long, default_value = "store")]
    pub store: PathBuf,
    /// Query text.
    #[arg(long)]
    pub query: String,
    /// Maximum number of results.
    #[arg(long, default_value_t = 3)]
    pub limit: usize,
    /// Search the revisions pinned by this experiment instead of the active ones.
    #[arg(long)]
    pub experiment: Option<ExperimentId>,
    /// Index space to search; defaults to the query model's space.
    #[arg(long)]
    pub space: Option<EmbeddingSpaceId>,
}

/// Runs the parsed command.
///
/// # Errors
///
/// Returns an error when a command fails unexpectedly; expected refusals map to exit
/// codes instead.
pub async fn run(cli: Cli) -> Result<ExitCode> {
    match cli.command {
        Command::Doctor => doctor().await,
        Command::Manifest(args) => manifest_check(&args.manifest),
        Command::AnonymizationCheck(args) => anonymization_check(&args.manifest),
        Command::Validate(args) => validate(&args).await,
        Command::Ingest(args) => ingest(&args).await,
        Command::Search(args) => search(&args).await,
    }
}

async fn doctor() -> Result<ExitCode> {
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

fn manifest_check(path: &Path) -> Result<ExitCode> {
    let manifest = LoadedManifest::load(path)?;
    let mut refused = 0usize;
    for source in &manifest.manifest.sources {
        match manifest.verify(source) {
            Ok(_) => println!("eligible    {}", source.source_id),
            Err(reason) => {
                if source.required {
                    refused += 1;
                }
                println!("ineligible  {}: {reason}", source.source_id);
            }
        }
    }
    println!(
        "corpus {} ({})",
        manifest.manifest.corpus_id, manifest.manifest_checksum
    );
    Ok(if refused == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(EXIT_VALIDATION)
    })
}

fn anonymization_check(path: &Path) -> Result<ExitCode> {
    let manifest = LoadedManifest::load(path)?;
    let mut unacknowledged = 0usize;
    for source in &manifest.manifest.sources {
        let file = manifest.path_of(source);
        let mut findings = anonymization::filename_findings(&source.file);
        findings.extend(if source.input_type.is_pdf() {
            anonymization::pdf_findings(&file)
        } else {
            anonymization::image_findings(&file)
        });
        let acknowledged = source
            .anonymization
            .as_ref()
            .is_some_and(|a| a.missing().is_empty());
        if !findings.is_empty() && !acknowledged {
            unacknowledged += 1;
        }
        println!(
            "{}  {} finding(s){}",
            source.source_id,
            findings.len(),
            if acknowledged {
                ", review recorded"
            } else {
                ", REVIEW MISSING"
            }
        );
        for finding in findings {
            println!("    [{}] {}", finding.area, finding.detail);
        }
    }
    Ok(if unacknowledged == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(EXIT_VALIDATION)
    })
}

/// Loads inputs and runs the checks that must precede adapter construction.
fn preflight(
    args: &RunArgs,
) -> Result<Result<(LoadedManifest, ExperimentConfig), ValidationError>> {
    let manifest = LoadedManifest::load(&args.manifest)?;
    let config = ExperimentConfig::load(&args.config)?;
    let engine = &config.engine;
    // Permission is checked before any adapter (and so any remote client or
    // credential) is constructed (PLAN §4.4).
    Ok(check_remote_permission(
        &manifest,
        engine.engine.execution(),
        engine.engine.as_str(),
        &engine.model_id,
    )
    .map(|()| (manifest, config)))
}

async fn validate(args: &RunArgs) -> Result<ExitCode> {
    let (manifest, config) = match preflight(args)? {
        Ok(inputs) => inputs,
        Err(e) => {
            eprintln!("validation failed: {e}");
            return Ok(ExitCode::from(EXIT_VALIDATION));
        }
    };
    let adapter = match adapters::build(&config.engine).await {
        Ok(adapter) => adapter,
        Err(e) => {
            eprintln!("validation failed: {e}");
            return Ok(ExitCode::from(EXIT_VALIDATION));
        }
    };
    let info = adapter.model_info();
    println!(
        "engine {} model {} space {}",
        info.engine, info.model_id, info.embedding_space_id
    );
    println!(
        "corpus {} with {} sources",
        manifest.manifest.corpus_id,
        manifest.manifest.sources.len()
    );
    Ok(ExitCode::SUCCESS)
}

async fn ingest(args: &IngestArgs) -> Result<ExitCode> {
    let (manifest, config) = match preflight(&args.run)? {
        Ok(inputs) => inputs,
        Err(e) => {
            eprintln!("validation failed: {e}");
            return Ok(ExitCode::from(EXIT_VALIDATION));
        }
    };
    let embedder = match adapters::build(&config.engine).await {
        Ok(adapter) => adapter,
        Err(e) => {
            eprintln!("validation failed: {e}");
            return Ok(ExitCode::from(EXIT_VALIDATION));
        }
    };
    let store = Arc::new(FsRecordStore::open(&args.store)?);
    let text = PopplerTextExtractor::detect().await;
    let ocr = TesseractOcr::detect(config.extraction.clone()).await;
    let pipeline = Pipeline {
        text: &text,
        ocr: &ocr,
        sleeper: &TokioSleeper,
        store,
        embedder,
    };
    let settings = config.ingest_settings(args.experiment.clone());
    let report = match pipeline.ingest(&manifest, &settings).await {
        Ok(report) => report,
        Err(e) => {
            let code = e.exit_code();
            let kind = if code == EXIT_VALIDATION {
                "validation failed"
            } else {
                "error"
            };
            eprintln!("{kind}: {e}");
            return Ok(ExitCode::from(code));
        }
    };
    let json = serde_json::to_string_pretty(&report)?;
    match &args.report {
        Some(path) => {
            std::fs::write(path, json).with_context(|| format!("write {}", path.display()))?
        }
        None => println!("{json}"),
    }
    for source in &report.sources {
        eprintln!(
            "{:<20} {:<15} stage={:<11} attempts={} records={}{}",
            source.source_id,
            source.status.as_str(),
            source.stage.map_or("-", |s| s.as_str()),
            source.attempts,
            source.records_committed,
            source
                .reason
                .as_deref()
                .map(|r| format!(" reason={r}"))
                .unwrap_or_default()
        );
    }
    eprintln!(
        "run {}: {}",
        report.experiment_id,
        report.run_status.as_str()
    );
    Ok(ExitCode::from(report.exit_code()))
}

async fn search(args: &SearchArgs) -> Result<ExitCode> {
    let config = ExperimentConfig::load(&args.config)?;
    if config.engine.engine.execution() == crate::domain::Execution::Remote {
        // Queries are not corpus content, but the permission model is per corpus;
        // remote query embedding arrives with the remote adapter in Phase 2.
        eprintln!("validation failed: remote query embedding is not enabled");
        return Ok(ExitCode::from(EXIT_VALIDATION));
    }
    let embedder = match adapters::build(&config.engine).await {
        Ok(adapter) => adapter,
        Err(e) => {
            eprintln!("validation failed: {e}");
            return Ok(ExitCode::from(EXIT_VALIDATION));
        }
    };
    let store = FsRecordStore::open(&args.store)?;
    let space = args
        .space
        .clone()
        .unwrap_or_else(|| embedder.model_info().embedding_space_id.clone());
    let snapshot = args
        .experiment
        .as_ref()
        .map(|id| store.load_experiment(id))
        .transpose()?;
    let scope = snapshot.as_ref().map_or(Scope::Active, Scope::Experiment);
    let hits = search_text(&store, &space, &*embedder, &args.query, args.limit, scope).await?;
    println!("{}", serde_json::to_string_pretty(&hits)?);
    Ok(ExitCode::SUCCESS)
}
