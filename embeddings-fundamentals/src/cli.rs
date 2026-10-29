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
use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::adapters;
use crate::anonymization;
use crate::config::{CombinedConfig, EngineConfig, ExperimentConfig};
use crate::corpus::LoadedManifest;
use crate::extraction::poppler::{PopplerImageExtractor, PopplerTextExtractor, TesseractOcr};
use crate::ids::{EmbeddingSpaceId, ExperimentId};
use crate::pipeline::{Pipeline, TokioSleeper, ValidationError, check_remote_permission};
use crate::retrieval::{Leg, Scope, search_combined, search_images, search_text};
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
    /// Search an index with a text query (text records, or image records of a
    /// cross-modal space).
    Search(SearchArgs),
    /// Ingest, evaluate the frozen queries, and measure one configuration.
    Experiment(ExperimentArgs),
    /// Evaluate and measure combined retrieval (RRF) over a text run and an image run.
    Combine(CombineArgs),
    /// Combine experiment results into a comparison report (JSON and Markdown).
    Report(ReportArgs),
    /// Draft a corpus manifest for a directory of anonymized files (`TESTS.md`).
    DraftManifest(DraftManifestArgs),
    /// Draft relevance labels pooled from the top results of indexed runs
    /// (`TESTS.md`).
    DraftLabels(DraftLabelsArgs),
}

/// Arguments of `draft-manifest`.
#[derive(Debug, Clone, Args)]
pub struct DraftManifestArgs {
    /// Directory of the anonymized corpus files (PDF, PNG, JPEG), searched
    /// recursively; the manifest belongs in it.
    #[arg(long)]
    pub dir: PathBuf,
    /// Corpus identifier.
    #[arg(long)]
    pub corpus_id: String,
    /// Configuration whose extraction settings decide which pages have a usable text
    /// layer; the default settings otherwise.
    #[arg(long)]
    pub config: Option<PathBuf>,
    /// Write the draft to this new file (an existing file is never overwritten)
    /// instead of stdout.
    #[arg(long)]
    pub out: Option<PathBuf>,
    /// Write the draft even when file names were flagged for review.
    #[arg(long)]
    pub allow_flagged_names: bool,
}

/// Arguments of `draft-labels`.
#[derive(Debug, Clone, Args)]
pub struct DraftLabelsArgs {
    /// Store root directory.
    #[arg(long, default_value = "store")]
    pub store: PathBuf,
    /// A pooled run as `<experiment>=<configuration>` (repeatable): the revisions the
    /// experiment pinned are searched with the configuration's query model and
    /// modality.
    #[arg(long = "run", required = true, value_parser = parse_run)]
    pub runs: Vec<(ExperimentId, PathBuf)>,
    /// Frozen query set.
    #[arg(long, default_value = "config/evaluation/queries-v1.toml")]
    pub queries: PathBuf,
    /// Results pooled from each run for every query.
    #[arg(long, default_value = "10")]
    pub depth: std::num::NonZeroUsize,
    /// Corpus the labels describe (its `corpus_id`).
    #[arg(long)]
    pub corpus: String,
    /// Write the draft to this new file (an existing file is never overwritten)
    /// instead of stdout.
    #[arg(long)]
    pub out: Option<PathBuf>,
    /// Corpus manifest of the index; required for a remote engine, whose permission
    /// is granted per corpus.
    #[arg(long)]
    pub manifest: Option<PathBuf>,
}

/// Parses `<experiment>=<configuration>`.
fn parse_run(value: &str) -> Result<(ExperimentId, PathBuf), String> {
    let (id, config) = value
        .split_once('=')
        .ok_or_else(|| "expected <experiment>=<configuration>".to_owned())?;
    let id = id.parse::<ExperimentId>().map_err(|e| e.to_string())?;
    if config.is_empty() {
        return Err("expected <experiment>=<configuration>".to_owned());
    }
    Ok((id, PathBuf::from(config)))
}

/// Arguments of `experiment`.
#[derive(Debug, Clone, Args)]
pub struct ExperimentArgs {
    /// Corpus manifest, configuration, store, and experiment id.
    #[command(flatten)]
    pub ingest: IngestArgs,
    /// Frozen query set.
    #[arg(long, default_value = "config/evaluation/queries-v1.toml")]
    pub queries: PathBuf,
    /// Frozen relevance labels.
    #[arg(long, default_value = "config/evaluation/labels-synthetic-v4.toml")]
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

/// Arguments of `combine`.
#[derive(Debug, Clone, Args)]
pub struct CombineArgs {
    /// Corpus manifest both leg runs indexed (checksum and remote permission).
    #[arg(long)]
    pub manifest: PathBuf,
    /// Saved combined configuration (text leg, image leg, fusion).
    #[arg(long)]
    pub config: PathBuf,
    /// Store root holding both leg runs.
    #[arg(long, default_value = "store")]
    pub store: PathBuf,
    /// Text run (`experiment`) whose pinned revisions the text leg searches.
    #[arg(long)]
    pub text_experiment: ExperimentId,
    /// Image run (`experiment`) whose pinned revisions the image leg searches.
    #[arg(long)]
    pub image_experiment: ExperimentId,
    /// Id of this combined run (snapshots are create-only).
    #[arg(long)]
    pub experiment: ExperimentId,
    /// Write the JSON result here instead of stdout.
    #[arg(long)]
    pub report: Option<PathBuf>,
    /// Frozen query set.
    #[arg(long, default_value = "config/evaluation/queries-v1.toml")]
    pub queries: PathBuf,
    /// Frozen relevance labels.
    #[arg(long, default_value = "config/evaluation/labels-synthetic-v4.toml")]
    pub labels: PathBuf,
    /// Warm repetitions of every frozen query (PRD §9.4 default: 20).
    #[arg(long, default_value_t = 20)]
    pub latency_repeats: usize,
    /// Price table of the text leg's provider (remote engines).
    #[arg(long)]
    pub text_price: Option<PathBuf>,
    /// Price table of the image leg's provider (remote engines).
    #[arg(long)]
    pub image_price: Option<PathBuf>,
}

/// Arguments of `report`.
#[derive(Debug, Clone, Args)]
pub struct ReportArgs {
    /// Experiment result files written by `experiment --report`, in report order.
    #[arg(long = "result", required = true)]
    pub results: Vec<PathBuf>,
    /// Combined result files written by `combine --report`, in report order.
    #[arg(long = "combined")]
    pub combined: Vec<PathBuf>,
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
#[derive(Debug, Clone, Args)]
pub struct ManifestArgs {
    /// Corpus manifest (`manifest.toml`).
    #[arg(long)]
    pub manifest: PathBuf,
}

/// Arguments of a run.
#[derive(Debug, Clone, Args)]
pub struct RunArgs {
    /// Corpus manifest.
    #[arg(long)]
    pub manifest: PathBuf,
    /// Saved experiment configuration.
    #[arg(long)]
    pub config: PathBuf,
}

/// Arguments of `ingest`.
#[derive(Debug, Clone, Args)]
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
    /// Saved experiment configuration (selects the query model and index space); a
    /// combined configuration in `combined` mode.
    #[arg(long)]
    pub config: PathBuf,
    /// Store root directory.
    #[arg(long, default_value = "store")]
    pub store: PathBuf,
    /// Query text.
    #[arg(long)]
    pub query: String,
    /// Records to search: text chunks, or images through the model's text-query path.
    #[arg(long, value_enum, default_value_t = SearchMode::Text)]
    pub mode: SearchMode,
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

/// Retrieval mode of `search`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SearchMode {
    /// Text queries against text records.
    Text,
    /// Text queries against image records (needs a cross-modal model).
    Image,
    /// Both legs of a combined configuration, fused with RRF.
    Combined,
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
        Command::Manifest(args) => blocking(move || manifest_check(&args.manifest)).await,
        Command::AnonymizationCheck(args) => {
            blocking(move || anonymization_check(&args.manifest)).await
        }
        Command::Validate(args) => validate(&args).await,
        Command::Ingest(args) => ingest(&args).await,
        Command::Search(args) => match args.mode {
            SearchMode::Text => search(&args, crate::domain::Modality::Text).await,
            SearchMode::Image => search(&args, crate::domain::Modality::Image).await,
            SearchMode::Combined => search_fused(&args).await,
        },
        Command::Experiment(args) => experiment(&args).await,
        Command::Combine(args) => combine(&args).await,
        Command::Report(args) => blocking(move || report(&args)).await,
        Command::DraftManifest(args) => draft_manifest(&args).await,
        Command::DraftLabels(args) => draft_labels(&args).await,
    }
}

/// Runs synchronous file and store work on the blocking pool (PLAN §2.3), never on a
/// runtime worker.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .context("blocking task failed")?
}

/// Opens the store for writing (directory setup and abandoned-staging cleanup) on the
/// blocking pool.
async fn open_writer(root: &Path) -> Result<Arc<FsRecordStore>> {
    let root = root.to_owned();
    blocking(move || Ok(Arc::new(FsRecordStore::open_writer(&root)?))).await
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

/// Loads inputs (on the blocking pool) and runs the checks that must precede adapter
/// construction.
async fn preflight(
    args: &RunArgs,
) -> Result<Result<(LoadedManifest, ExperimentConfig), ValidationError>> {
    let (manifest, config) = (args.manifest.clone(), args.config.clone());
    let (manifest, config) = blocking(move || {
        Ok((
            LoadedManifest::load(&manifest)?,
            ExperimentConfig::load(&config)?,
        ))
    })
    .await?;
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
    let (manifest, config) = match preflight(args).await? {
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
    let (manifest, config) = match preflight(&args.run).await? {
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
    let store = open_writer(&args.store).await?;
    let text = PopplerTextExtractor::detect().await;
    let ocr = TesseractOcr::detect(config.extraction.clone()).await;
    let images = PopplerImageExtractor::detect().await;
    let pipeline = Pipeline {
        text: &text,
        ocr: &ocr,
        images: &images,
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
    write_json(args.report.as_deref(), &report).await?;
    print_sources(&report);
    eprintln!(
        "run {}: {}",
        report.experiment_id,
        report.run_status.as_str()
    );
    Ok(ExitCode::from(report.exit_code()))
}

/// Writes `value` as pretty JSON to `path`, or to stdout when no path is given.
async fn write_json<T: serde::Serialize>(path: Option<&Path>, value: &T) -> Result<()> {
    let json = serde_json::to_string_pretty(value)?;
    match path {
        Some(path) => {
            tokio::fs::write(path, json)
                .await
                .with_context(|| format!("write {}", path.display()))?;
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

/// Checks the per-corpus permission of every remote engine among `engines` before any
/// client or credential exists; local engines need no manifest. Returns the refusal
/// to report, if any.
async fn remote_permission(
    engines: &[&EngineConfig],
    manifest: Option<&Path>,
) -> Result<Option<String>> {
    let remote: Vec<&&EngineConfig> = engines
        .iter()
        .filter(|e| e.engine.execution() == crate::domain::Execution::Remote)
        .collect();
    if remote.is_empty() {
        return Ok(None);
    }
    // The permission model is per corpus: a remote query model needs the manifest of
    // the indexed corpus.
    let Some(path) = manifest else {
        return Ok(Some(
            "a remote engine needs --manifest for its permission check".to_owned(),
        ));
    };
    let path = path.to_owned();
    let manifest = blocking(move || Ok(LoadedManifest::load(&path)?)).await?;
    for engine in remote {
        if let Err(e) = check_remote_permission(
            &manifest,
            engine.engine.execution(),
            engine.engine.as_str(),
            &engine.model_id,
        ) {
            return Ok(Some(e.to_string()));
        }
    }
    Ok(None)
}

/// Writes `content` to a new file, never overwriting one, or to stdout.
async fn write_new(path: Option<&Path>, content: String) -> Result<()> {
    match path {
        Some(path) => {
            let path = path.to_owned();
            blocking(move || {
                use std::io::Write as _;
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .with_context(|| {
                        format!(
                            "create {} (an existing file is never overwritten)",
                            path.display()
                        )
                    })?;
                file.write_all(content.as_bytes())
                    .with_context(|| format!("write {}", path.display()))?;
                Ok(())
            })
            .await
        }
        None => {
            print!("{content}");
            Ok(())
        }
    }
}

/// `draft-manifest`: measures the files of a corpus directory and drafts their
/// manifest entries. Flagged file names are refused unless allowed, because the
/// manifest is tracked.
async fn draft_manifest(args: &DraftManifestArgs) -> Result<ExitCode> {
    let extraction = match &args.config {
        Some(path) => {
            let path = path.clone();
            blocking(move || Ok(ExperimentConfig::load(&path)?.extraction)).await?
        }
        None => crate::extraction::ExtractionConfig::default(),
    };
    let dir = args.dir.clone();
    let files = blocking(move || Ok(crate::drafts::corpus_files(&dir)?)).await?;
    let text = PopplerTextExtractor::detect().await;
    let images = PopplerImageExtractor::detect().await;
    let draft = crate::drafts::draft_manifest(
        &text,
        &images,
        &args.dir,
        &files,
        &extraction,
        &crate::extraction::images::ImageFilter::default(),
        &args.corpus_id,
    )
    .await?;
    let flagged: Vec<String> = draft
        .sources
        .iter()
        .enumerate()
        .filter(|(_, s)| s.name_flagged())
        .map(|(i, _)| (i + 1).to_string())
        .collect();
    if !flagged.is_empty() && !args.allow_flagged_names {
        eprintln!(
            "validation failed: the file names of sources {} (in path order) were flagged for review; rename them to neutral names (TESTS.md) or pass --allow-flagged-names",
            flagged.join(", ")
        );
        return Ok(ExitCode::from(EXIT_VALIDATION));
    }
    write_new(args.out.as_deref(), draft.render()).await?;
    eprintln!(
        "drafted {} source(s); none is eligible until its anonymization review is recorded",
        draft.sources.len()
    );
    Ok(ExitCode::SUCCESS)
}

/// `draft-labels`: pools the top results of every run for every frozen query into a
/// label file to judge.
async fn draft_labels(args: &DraftLabelsArgs) -> Result<ExitCode> {
    use crate::drafts::{DraftError, PoolRun};
    let paths: Vec<PathBuf> = args.runs.iter().map(|(_, p)| p.clone()).collect();
    let queries_path = args.queries.clone();
    let (configs, queries) = blocking(move || {
        let configs = paths
            .iter()
            .map(|p| ExperimentConfig::load(p))
            .collect::<Result<Vec<_>, _>>()?;
        Ok((configs, crate::evaluation::load_queries(&queries_path)?))
    })
    .await?;
    let engines: Vec<&EngineConfig> = configs.iter().map(|c| &c.engine).collect();
    if let Some(refusal) = remote_permission(&engines, args.manifest.as_deref()).await? {
        eprintln!("validation failed: {refusal}");
        return Ok(ExitCode::from(EXIT_VALIDATION));
    }
    // Every snapshot is read before any model loads, so a missing experiment is
    // refused without a model load or download.
    let (root, ids) = (
        args.store.clone(),
        args.runs
            .iter()
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>(),
    );
    let (store, snapshots) = blocking(move || {
        let store = FsRecordStore::open_reader(&root)?;
        let snapshots = ids
            .iter()
            .map(|id| store.load_experiment(id))
            .collect::<Result<Vec<_>, _>>()?;
        Ok((Arc::new(store), snapshots))
    })
    .await?;
    let mut embedders = Vec::with_capacity(configs.len());
    for config in &configs {
        match adapters::build(&config.engine).await {
            Ok(adapter) => embedders.push(adapter),
            Err(e) => return Ok(build_failure(&e)),
        }
    }
    let spaces: Vec<EmbeddingSpaceId> = embedders
        .iter()
        .map(|e| e.model_info().embedding_space_id.clone())
        .collect();
    // A run whose experiment pinned nothing in its model's space would pool nothing,
    // silently: refuse the pairing instead.
    for (((id, config), space), snapshot) in args
        .runs
        .iter()
        .map(|(id, config)| (id, config))
        .zip(&spaces)
        .zip(&snapshots)
    {
        if !snapshot.revisions.iter().any(|r| &r.space == space) {
            eprintln!(
                "validation failed: experiment {id} pins no revision in the space {space} of {}",
                config.display()
            );
            return Ok(ExitCode::from(EXIT_VALIDATION));
        }
    }
    let runs: Vec<PoolRun<'_>> = args
        .runs
        .iter()
        .zip(&configs)
        .zip(&embedders)
        .zip(&spaces)
        .zip(&snapshots)
        .map(|((((run, config), embedder), space), snapshot)| PoolRun {
            experiment: &run.0,
            modality: config
                .ingest
                .modality
                .unwrap_or(crate::domain::Modality::Text),
            space,
            embedder: &**embedder,
            scope: Scope::Experiment(snapshot),
        })
        .collect();
    let draft = match crate::drafts::draft_labels(
        &store,
        &queries.value,
        &runs,
        args.depth.get(),
        &args.corpus,
    )
    .await
    {
        Ok(draft) => draft,
        Err(DraftError::Search(
            e @ crate::retrieval::SearchError::Embedding(
                crate::embedding::EmbeddingError::Unsupported { .. },
            ),
        )) => {
            eprintln!("validation failed: {e}");
            return Ok(ExitCode::from(EXIT_VALIDATION));
        }
        Err(e) => return Err(e.into()),
    };
    write_new(args.out.as_deref(), draft.render()).await?;
    let candidates: usize = draft.queries.iter().map(|q| q.candidates.len()).sum();
    eprintln!(
        "drafted {candidates} candidate(s) for {} queries; replace every \"TODO\" before using the file",
        draft.queries.len()
    );
    Ok(ExitCode::SUCCESS)
}

/// Single-modality search: text records, or image records of a cross-modal space.
async fn search(args: &SearchArgs, modality: crate::domain::Modality) -> Result<ExitCode> {
    let path = args.config.clone();
    let config = blocking(move || Ok(ExperimentConfig::load(&path)?)).await?;
    if let Some(refusal) = remote_permission(&[&config.engine], args.manifest.as_deref()).await? {
        eprintln!("validation failed: {refusal}");
        return Ok(ExitCode::from(EXIT_VALIDATION));
    }
    // The store and snapshot are read before any model loads, so a missing experiment
    // is refused without a model load or download.
    let (store, snapshot) = open_scope(&args.store, args.experiment.as_ref()).await?;
    let embedder = match adapters::build(&config.engine).await {
        Ok(adapter) => adapter,
        Err(e) => return Ok(build_failure(&e)),
    };
    let space = args
        .space
        .clone()
        .unwrap_or_else(|| embedder.model_info().embedding_space_id.clone());
    let scope = snapshot.as_ref().map_or(Scope::Active, Scope::Experiment);
    let outcome = match modality {
        crate::domain::Modality::Text => {
            search_text(&store, &space, &*embedder, &args.query, args.limit, scope).await
        }
        crate::domain::Modality::Image => {
            search_images(&store, &space, &*embedder, &args.query, args.limit, scope).await
        }
    };
    let hits = match outcome {
        Ok(hits) => hits,
        Err(
            e @ crate::retrieval::SearchError::Embedding(
                crate::embedding::EmbeddingError::Unsupported { .. },
            ),
        ) => {
            eprintln!("validation failed: {e}");
            return Ok(ExitCode::from(EXIT_VALIDATION));
        }
        Err(e) => return Err(e.into()),
    };
    println!("{}", serde_json::to_string_pretty(&hits)?);
    Ok(ExitCode::SUCCESS)
}

/// Opens the store read-only with the snapshot of `experiment`, if one is named.
async fn open_scope(
    root: &Path,
    experiment: Option<&ExperimentId>,
) -> Result<(
    Arc<FsRecordStore>,
    Option<crate::storage::ExperimentSnapshot>,
)> {
    let (root, experiment) = (root.to_owned(), experiment.cloned());
    blocking(move || {
        let store = FsRecordStore::open_reader(&root)?;
        let snapshot = experiment
            .as_ref()
            .map(|id| store.load_experiment(id))
            .transpose()?;
        Ok((Arc::new(store), snapshot))
    })
    .await
}

/// Loads a combined configuration on the blocking pool; a refused leg pairing is a
/// validation failure (exit 3), reported before any model is loaded.
async fn load_combined(path: &Path) -> Result<Result<CombinedConfig, String>> {
    let path = path.to_owned();
    blocking(move || match CombinedConfig::load(&path) {
        Ok(config) => Ok(Ok(config)),
        Err(e @ crate::config::ConfigError::Combination { .. }) => Ok(Err(e.to_string())),
        Err(e) => Err(e.into()),
    })
    .await
}

/// `search --mode combined`: both legs of a combined configuration, fused with RRF.
async fn search_fused(args: &SearchArgs) -> Result<ExitCode> {
    if args.space.is_some() {
        eprintln!(
            "validation failed: --space names one index; a combined search uses the space of each leg's model"
        );
        return Ok(ExitCode::from(EXIT_VALIDATION));
    }
    let config = match load_combined(&args.config).await? {
        Ok(config) => config,
        Err(refusal) => {
            eprintln!("validation failed: {refusal}");
            return Ok(ExitCode::from(EXIT_VALIDATION));
        }
    };
    if let Some(refusal) = remote_permission(
        &[&config.text.engine, &config.image.engine],
        args.manifest.as_deref(),
    )
    .await?
    {
        eprintln!("validation failed: {refusal}");
        return Ok(ExitCode::from(EXIT_VALIDATION));
    }
    // Snapshot first: a missing experiment, or one that is not a combined run with these
    // fusion parameters, is refused before either model loads. Its leg spaces are
    // checked by `search_combined` once the models give them.
    let (store, snapshot) = open_scope(&args.store, args.experiment.as_ref()).await?;
    if let Some(snapshot) = &snapshot
        && let Err(e) = crate::retrieval::check_fusion_pinned(snapshot, config.fusion)
    {
        eprintln!("validation failed: {e}");
        return Ok(ExitCode::from(EXIT_VALIDATION));
    }
    let text = match adapters::build(&config.text.engine).await {
        Ok(adapter) => adapter,
        Err(e) => return Ok(build_failure(&e)),
    };
    let image = match adapters::build(&config.image.engine).await {
        Ok(adapter) => adapter,
        Err(e) => return Ok(build_failure(&e)),
    };
    let scope = snapshot.as_ref().map_or(Scope::Active, Scope::Experiment);
    let (text_space, image_space) = (
        text.model_info().embedding_space_id.clone(),
        image.model_info().embedding_space_id.clone(),
    );
    let outcome = search_combined(
        &store,
        Leg {
            space: &text_space,
            embedder: &*text,
        },
        Leg {
            space: &image_space,
            embedder: &*image,
        },
        config.fusion,
        &args.query,
        args.limit,
        scope,
    )
    .await;
    let hits = match outcome {
        Ok(hits) => hits,
        Err(
            e @ (crate::retrieval::SearchError::Embedding(
                crate::embedding::EmbeddingError::Unsupported { .. },
            )
            | crate::retrieval::SearchError::SnapshotMismatch { .. }),
        ) => {
            eprintln!("validation failed: {e}");
            return Ok(ExitCode::from(EXIT_VALIDATION));
        }
        Err(e) => return Err(e.into()),
    };
    println!("{}", serde_json::to_string_pretty(&hits)?);
    Ok(ExitCode::SUCCESS)
}

async fn combine(args: &CombineArgs) -> Result<ExitCode> {
    use crate::experiment::{CombinedInputs, ExperimentError, Instruments};
    let config = match load_combined(&args.config).await? {
        Ok(config) => config,
        Err(refusal) => {
            eprintln!("validation failed: {refusal}");
            return Ok(ExitCode::from(EXIT_VALIDATION));
        }
    };
    if let Some(refusal) = remote_permission(
        &[&config.text.engine, &config.image.engine],
        Some(&args.manifest),
    )
    .await?
    {
        eprintln!("validation failed: {refusal}");
        return Ok(ExitCode::from(EXIT_VALIDATION));
    }
    let Some(repeats) = std::num::NonZeroUsize::new(args.latency_repeats) else {
        eprintln!("validation failed: --latency-repeats must be at least 1");
        return Ok(ExitCode::from(EXIT_VALIDATION));
    };
    let inputs = args.clone();
    let (manifest, dataset, text_price, image_price) = blocking(move || {
        let price = |path: Option<&Path>| path.map(crate::cost::PriceTable::load).transpose();
        Ok((
            LoadedManifest::load(&inputs.manifest)?,
            crate::evaluation::Dataset::load(&inputs.queries, &inputs.labels)?,
            price(inputs.text_price.as_deref())?,
            price(inputs.image_price.as_deref())?,
        ))
    })
    .await?;
    let store = open_writer(&args.store).await?;
    let instruments = Instruments {
        clock: Arc::new(crate::measurement::SystemClock::new()),
        memory: crate::measurement::RusageProbe,
        hardware: crate::measurement::Hardware::detect().await,
    };
    let outcome = crate::experiment::run_combined(
        &store,
        async || adapters::build(&config.text.engine).await,
        async || adapters::build(&config.image.engine).await,
        &instruments,
        CombinedInputs {
            experiment_id: &args.experiment,
            text_experiment: &args.text_experiment,
            image_experiment: &args.image_experiment,
            corpus_checksum: &manifest.manifest_checksum,
            dataset: &dataset,
            fusion: config.fusion,
            latency_repeats: repeats,
            text_price: text_price.as_ref(),
            image_price: image_price.as_ref(),
        },
    )
    .await;
    let result = match outcome {
        Ok(result) => result,
        Err(ExperimentError::Build(e)) => return Ok(build_failure(&e)),
        Err(e @ ExperimentError::Combination(_)) => {
            eprintln!("validation failed: {e}");
            return Ok(ExitCode::from(EXIT_VALIDATION));
        }
        Err(e) => return Err(e.into()),
    };
    write_json(args.report.as_deref(), &result).await?;
    let m = &result.evaluation.metrics;
    eprintln!(
        "combined {}: P@3 {:.4} Hit@3 {:.4} nDCG@3 {:.4} | cold start {:.0} ms | warm median {:.2} ms",
        result.experiment_id,
        m.precision_at_3,
        m.hit_at_3,
        m.ndcg_at_3,
        result.measurements.cold_start.total_ms,
        result.measurements.warm_query_latency.median_ms
    );
    Ok(ExitCode::SUCCESS)
}

async fn experiment(args: &ExperimentArgs) -> Result<ExitCode> {
    use crate::experiment::{ExperimentError, ExperimentInputs, Instruments, Tools};
    let (manifest, config) = match preflight(&args.ingest.run).await? {
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
    let inputs = args.clone();
    let (dataset, price, fidelity) = blocking(move || {
        Ok((
            crate::evaluation::Dataset::load(&inputs.queries, &inputs.labels)?,
            inputs
                .price
                .as_deref()
                .map(crate::cost::PriceTable::load)
                .transpose()?,
            inputs
                .fidelity
                .as_deref()
                .map(crate::fidelity::LoadedSample::load)
                .transpose()?,
        ))
    })
    .await?;
    let store = open_writer(&args.ingest.store).await?;
    let text = PopplerTextExtractor::detect().await;
    let ocr = TesseractOcr::detect(config.extraction.clone()).await;
    let images = PopplerImageExtractor::detect().await;
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
            images: &images,
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
            write_json(args.ingest.report.as_deref(), &incomplete).await?;
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
    write_json(args.ingest.report.as_deref(), &result).await?;
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
    let mut combined = Vec::with_capacity(args.combined.len());
    for path in &args.combined {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        combined.push(
            serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))?,
        );
    }
    let report = match crate::report::ComparisonReport::new(results, args.notes.clone())
        .and_then(|report| report.with_combined(combined))
    {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn blocking_work_does_not_stall_the_runtime_worker() {
        // Regression (C2-15): store opening and report I/O go through `blocking`; slow
        // synchronous work there must leave the single runtime worker free.
        let ticks = async {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(10));
            for _ in 0..5 {
                interval.tick().await;
            }
            std::time::Instant::now()
        };
        let work = blocking(|| {
            std::thread::sleep(std::time::Duration::from_millis(300));
            Ok(std::time::Instant::now())
        });
        let (ticked, done) = tokio::join!(ticks, work);
        let done = done.expect("blocking work");
        assert!(
            ticked < done,
            "the timer must finish while the blocking work still sleeps"
        );
    }

    #[tokio::test]
    async fn the_writer_store_opens_on_the_blocking_pool() {
        let dir = tempfile::tempdir().expect("tmp");
        let store = open_writer(dir.path()).await.expect("store");
        drop(store);
        open_writer(dir.path()).await.expect("reopened after drop");
    }
}
