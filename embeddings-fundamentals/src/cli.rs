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
    /// Ingest, evaluate the frozen queries, and measure one configuration.
    Experiment(ExperimentArgs),
    /// Combine experiment results into a comparison report (JSON and Markdown).
    Report(ReportArgs),
}

/// Arguments of `experiment`.
#[derive(Debug, Args)]
pub struct ExperimentArgs {
    /// Corpus manifest, configuration, store, and experiment id.
    #[command(flatten)]
    pub ingest: IngestArgs,
    /// Frozen query set.
    #[arg(long, default_value = "config/evaluation/queries-v1.toml")]
    pub queries: PathBuf,
    /// Frozen relevance labels.
    #[arg(long, default_value = "config/evaluation/labels-synthetic-v2.toml")]
    pub labels: PathBuf,
    /// Warm repetitions of every frozen query (PRD §9.4 default: 20).
    #[arg(long, default_value_t = 20)]
    pub latency_repeats: usize,
    /// Provider price table (remote engines).
    #[arg(long)]
    pub price: Option<PathBuf>,
    /// Extraction-fidelity sample.
    #[arg(long)]
    pub fidelity: Option<PathBuf>,
}

/// Arguments of `report`.
#[derive(Debug, Args)]
pub struct ReportArgs {
    /// Experiment result files written by `experiment --report`, in report order.
    #[arg(long = "result", required = true)]
    pub results: Vec<PathBuf>,
    /// Markdown output.
    #[arg(long)]
    pub markdown: PathBuf,
    /// JSON output.
    #[arg(long)]
    pub json: PathBuf,
    /// Limitation or outstanding item to list in the report (repeatable).
    #[arg(long = "note")]
    pub notes: Vec<String>,
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
    /// Corpus manifest of the index; required for a remote engine, whose permission
    /// is granted per corpus.
    #[arg(long)]
    pub manifest: Option<PathBuf>,
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
        Command::Experiment(args) => experiment(&args).await,
        Command::Report(args) => report(&args),
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

/// Reports an adapter build failure with the label and exit code of its kind.
fn build_failure(error: &adapters::BuildError) -> ExitCode {
    let code = error.exit_code();
    let kind = if code == EXIT_VALIDATION {
        "validation failed"
    } else {
        "error"
    };
    eprintln!("{kind}: {error}");
    ExitCode::from(code)
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
    let gate = manifest.clone();
    if let Err(e) =
        tokio::task::spawn_blocking(move || crate::pipeline::check_eligibility(&gate)).await?
    {
        eprintln!("validation failed: {e}");
        return Ok(ExitCode::from(e.exit_code()));
    }
    let adapter = match adapters::build(&config.engine).await {
        Ok(adapter) => adapter,
        Err(e) => return Ok(build_failure(&e)),
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
        Err(e) => return Ok(build_failure(&e)),
    };
    let store = Arc::new(FsRecordStore::open_writer(&args.store)?);
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
    write_json(args.report.as_deref(), &report)?;
    print_sources(&report);
    eprintln!(
        "run {}: {}",
        report.experiment_id,
        report.run_status.as_str()
    );
    Ok(ExitCode::from(report.exit_code()))
}

/// Writes `value` as pretty JSON to `path`, or to stdout when no path is given.
fn write_json<T: serde::Serialize>(path: Option<&std::path::Path>, value: &T) -> Result<()> {
    let json = serde_json::to_string_pretty(value)?;
    match path {
        Some(path) => {
            std::fs::write(path, json).with_context(|| format!("write {}", path.display()))?;
        }
        None => println!("{json}"),
    }
    Ok(())
}

/// Prints one status line per source (ids, stages, and counts only).
fn print_sources(report: &crate::pipeline::RunReport) {
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
}

async fn search(args: &SearchArgs) -> Result<ExitCode> {
    let config = ExperimentConfig::load(&args.config)?;
    let engine = &config.engine;
    if engine.engine.execution() == crate::domain::Execution::Remote {
        // The permission model is per corpus: a remote query model needs the manifest
        // of the indexed corpus, checked before any client or credential exists.
        let Some(path) = &args.manifest else {
            eprintln!(
                "validation failed: a remote engine needs --manifest for its permission check"
            );
            return Ok(ExitCode::from(EXIT_VALIDATION));
        };
        let manifest = LoadedManifest::load(path)?;
        if let Err(e) = check_remote_permission(
            &manifest,
            engine.engine.execution(),
            engine.engine.as_str(),
            &engine.model_id,
        ) {
            eprintln!("validation failed: {e}");
            return Ok(ExitCode::from(EXIT_VALIDATION));
        }
    }
    let embedder = match adapters::build(&config.engine).await {
        Ok(adapter) => adapter,
        Err(e) => return Ok(build_failure(&e)),
    };
    let (root, experiment) = (args.store.clone(), args.experiment.clone());
    let (store, snapshot) = tokio::task::spawn_blocking(move || -> Result<_> {
        let store = FsRecordStore::open_reader(&root)?;
        let snapshot = experiment
            .as_ref()
            .map(|id| store.load_experiment(id))
            .transpose()?;
        Ok((Arc::new(store), snapshot))
    })
    .await??;
    let space = args
        .space
        .clone()
        .unwrap_or_else(|| embedder.model_info().embedding_space_id.clone());
    let scope = snapshot.as_ref().map_or(Scope::Active, Scope::Experiment);
    let hits = search_text(&store, &space, &*embedder, &args.query, args.limit, scope).await?;
    println!("{}", serde_json::to_string_pretty(&hits)?);
    Ok(ExitCode::SUCCESS)
}

async fn experiment(args: &ExperimentArgs) -> Result<ExitCode> {
    use crate::experiment::{ExperimentError, ExperimentInputs, Instruments, Tools};
    let (manifest, config) = match preflight(&args.ingest.run)? {
        Ok(inputs) => inputs,
        Err(e) => {
            eprintln!("validation failed: {e}");
            return Ok(ExitCode::from(EXIT_VALIDATION));
        }
    };
    let Some(repeats) = std::num::NonZeroUsize::new(args.latency_repeats) else {
        eprintln!("validation failed: --latency-repeats must be at least 1");
        return Ok(ExitCode::from(EXIT_VALIDATION));
    };
    let dataset = crate::evaluation::Dataset::load(&args.queries, &args.labels)?;
    let price = args
        .price
        .as_deref()
        .map(crate::cost::PriceTable::load)
        .transpose()?;
    let fidelity = args
        .fidelity
        .as_deref()
        .map(crate::fidelity::LoadedSample::load)
        .transpose()?;
    let store = Arc::new(FsRecordStore::open_writer(&args.ingest.store)?);
    let text = PopplerTextExtractor::detect().await;
    let ocr = TesseractOcr::detect(config.extraction.clone()).await;
    let settings = config.ingest_settings(args.ingest.experiment.clone());
    let instruments = Instruments {
        clock: Arc::new(crate::measurement::SystemClock::new()),
        memory: crate::measurement::RusageProbe,
        hardware: crate::measurement::Hardware::detect().await,
    };
    let outcome = crate::experiment::run(
        Tools {
            text: &text,
            ocr: &ocr,
            sleeper: &TokioSleeper,
        },
        &store,
        async || adapters::build(&config.engine).await,
        &instruments,
        ExperimentInputs {
            manifest: &manifest,
            settings: &settings,
            dataset: &dataset,
            latency_repeats: repeats,
            price: price.as_ref(),
            fidelity: fidelity.as_ref(),
        },
    )
    .await;
    let result = match outcome {
        Ok(result) => result,
        Err(ExperimentError::Build(e)) => return Ok(build_failure(&e)),
        Err(ExperimentError::EvaluationNotRun(incomplete)) => {
            write_json(args.ingest.report.as_deref(), &incomplete)?;
            eprintln!(
                "experiment {}: {} | evaluation not run: {}",
                incomplete.experiment_id,
                incomplete.run_status.as_str(),
                incomplete.evaluation.reason
            );
            print_sources(&incomplete.ingestion);
            return Ok(ExitCode::from(EXIT_INCOMPLETE));
        }
        Err(ExperimentError::Validation(e)) => {
            let code = e.exit_code();
            let kind = if code == EXIT_VALIDATION {
                "validation failed"
            } else {
                "error"
            };
            eprintln!("{kind}: {e}");
            return Ok(ExitCode::from(code));
        }
        Err(e) => return Err(e.into()),
    };
    write_json(args.ingest.report.as_deref(), &result)?;
    let m = &result.evaluation.metrics;
    eprintln!(
        "experiment {}: {} | P@3 {:.4} Hit@3 {:.4} nDCG@3 {:.4} | ingestion {:.0} ms | warm median {:.2} ms",
        result.experiment_id,
        result.run_status.as_str(),
        m.precision_at_3,
        m.hit_at_3,
        m.ndcg_at_3,
        result.measurements.ingestion.duration_ms,
        result.measurements.warm_query_latency.median_ms
    );
    Ok(ExitCode::from(match result.run_status {
        crate::domain::RunStatus::Succeeded => 0,
        crate::domain::RunStatus::Incomplete => EXIT_INCOMPLETE,
    }))
}

fn report(args: &ReportArgs) -> Result<ExitCode> {
    let mut results = Vec::with_capacity(args.results.len());
    for path in &args.results {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        results.push(
            serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))?,
        );
    }
    let report = match crate::report::ComparisonReport::new(results, args.notes.clone()) {
        Ok(report) => report,
        Err(e) => {
            eprintln!("validation failed: {e}");
            return Ok(ExitCode::from(EXIT_VALIDATION));
        }
    };
    std::fs::write(&args.json, serde_json::to_string_pretty(&report)?)
        .with_context(|| format!("write {}", args.json.display()))?;
    std::fs::write(&args.markdown, report.to_markdown())
        .with_context(|| format!("write {}", args.markdown.display()))?;
    Ok(ExitCode::SUCCESS)
}
