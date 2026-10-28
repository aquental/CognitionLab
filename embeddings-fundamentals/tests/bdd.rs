//! Acceptance suite (PRD §5, PLAN §5.2).
//!
//! Executes `tests/features` with `fail_on_skipped`, writes `reports/bdd/cucumber.json`,
//! and fails unless every scenario the PRD requires for the configured milestone
//! (`config/acceptance.toml`) was executed and passed. Pass cucumber filters through
//! `BDD_ARGS` (for example `BDD_ARGS='--name hemoglobina'`); a filtered run fails the
//! inventory check by design.

#[path = "bdd/steps/mod.rs"]
mod steps;
mod support;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use cucumber::World;
use embeddings_fundamentals::corpus::{LoadedManifest, Manifest, SourceEntry};
use embeddings_fundamentals::embedding::ErrorClass;
use embeddings_fundamentals::embedding::deterministic::DeterministicEmbedder;
use embeddings_fundamentals::extraction::fixture::FixtureExtractor;
use embeddings_fundamentals::ids::ExperimentId;
use embeddings_fundamentals::pipeline::{IngestSettings, NoSleep, Pipeline, RunReport};
use embeddings_fundamentals::record::Record;
use embeddings_fundamentals::retrieval::SearchHit;
use embeddings_fundamentals::storage::FsRecordStore;

use support::runner::{Suite, run_suite, split_args};

/// Path of a file under `tests/fixtures`.
pub fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(rel)
}

/// Per-scenario state. Cucumber builds a fresh `World` for every scenario, so each one
/// gets its own store, corpus directory, adapter call log, and fault plan.
#[derive(Debug, World)]
#[world(init = Self::new)]
pub struct LabWorld {
    /// Isolated store root, deleted when the scenario ends.
    pub store_dir: tempfile::TempDir,
    /// Isolated corpus directory holding the manifest and copied fixtures.
    pub corpus_dir: tempfile::TempDir,
    /// Record store over `store_dir`.
    pub store: Arc<FsRecordStore>,
    /// Replayed extraction (captured tool output or pages set by steps).
    pub extractor: Arc<FixtureExtractor>,
    /// Deterministic adapter: its call log and fault plan belong to this scenario.
    pub embedder: Arc<DeterministicEmbedder>,
    /// Manifest sources under construction.
    pub sources: Vec<SourceEntry>,
    /// Corpus-level remote permission.
    pub remote_allowed: bool,
    /// Settings template for runs (each run gets a fresh experiment id).
    pub settings: IngestSettings,
    /// Reports of completed runs, in order.
    pub runs: Vec<RunReport>,
    /// Validation error of the last refused run.
    pub validation_error: Option<String>,
    /// Outcome of the last search.
    pub search: Option<Result<Vec<SearchHit>, String>>,
    /// Named values remembered across steps (checksums, revisions, record sets).
    pub notes: std::collections::BTreeMap<String, String>,
    /// Outcome of the last direct embedding attempt (harness steps).
    pub last_embedding: Option<Result<(), ErrorClass>>,
}

impl LabWorld {
    fn new() -> Result<Self> {
        let store_dir = tempfile::tempdir().context("create scenario store")?;
        let store = Arc::new(FsRecordStore::open(store_dir.path())?);
        Ok(Self {
            store_dir,
            corpus_dir: tempfile::tempdir().context("create scenario corpus")?,
            store,
            extractor: Arc::new(FixtureExtractor::default()),
            embedder: Arc::new(DeterministicEmbedder::builder().build()?),
            sources: Vec::new(),
            remote_allowed: false,
            settings: IngestSettings::new("template".parse()?),
            runs: Vec::new(),
            validation_error: None,
            search: None,
            notes: std::collections::BTreeMap::new(),
            last_embedding: None,
        })
    }

    /// Replays the captured extraction of every fixture PDF.
    pub fn use_captured_extraction(&mut self) -> Result<()> {
        self.extractor = Arc::new(FixtureExtractor::from_dir(&fixture("extraction"))?);
        Ok(())
    }

    /// Copies a fixture from the synthetic corpus and adds it to the manifest, under
    /// `source_id` (defaults to the fixture's own id). Replaces an existing entry.
    pub fn add_fixture_source(&mut self, fixture_id: &str, source_id: Option<&str>) -> Result<SourceEntry> {
        let corpus = LoadedManifest::load(&fixture("corpus/manifest.toml"))?;
        let mut entry = corpus
            .manifest
            .sources
            .iter()
            .find(|s| s.source_id.as_str() == fixture_id)
            .cloned()
            .with_context(|| format!("no fixture {fixture_id}"))?;
        std::fs::copy(corpus.path_of(&entry), self.corpus_dir.path().join(&entry.file))
            .context("copy fixture")?;
        if let Some(id) = source_id {
            entry.source_id = id.parse()?;
        }
        self.sources.retain(|s| s.source_id != entry.source_id);
        self.sources.push(entry.clone());
        Ok(entry)
    }

    /// Writes the manifest to the corpus directory and loads it like the CLI does.
    pub fn manifest(&self) -> Result<LoadedManifest> {
        let manifest = Manifest {
            corpus_id: "bdd-corpus".to_owned(),
            remote_allowed: self.remote_allowed,
            sources: self.sources.clone(),
        };
        let path = self.corpus_dir.path().join("manifest.toml");
        std::fs::write(&path, toml::to_string(&manifest)?).context("write manifest")?;
        Ok(LoadedManifest::load(&path)?)
    }

    /// Runs ingestion through the real pipeline with a fresh experiment id.
    pub async fn ingest(&mut self) -> Result<()> {
        let manifest = self.manifest()?;
        let mut settings = self.settings.clone();
        settings.experiment_id = ExperimentId::try_from(format!("exp-{}", self.runs.len() + 1))?;
        let pipeline = Pipeline {
            text: &*self.extractor,
            ocr: &*self.extractor,
            sleeper: &NoSleep,
            store: Arc::clone(&self.store),
            embedder: self.embedder.clone(),
        };
        match pipeline.ingest(&manifest, &settings).await {
            Ok(report) => {
                self.validation_error = None;
                self.runs.push(report);
            }
            Err(e) => self.validation_error = Some(e.to_string()),
        }
        Ok(())
    }

    /// Report of the last completed run.
    pub fn last_run(&self) -> Result<&RunReport> {
        if let Some(error) = &self.validation_error {
            bail!("the last run was refused by validation: {error}");
        }
        self.runs.last().context("no run has completed")
    }

    /// Active records in the adapter's embedding space.
    pub fn active_records(&self) -> Result<Vec<Record>> {
        Ok(self.store.active_records(&self.embedder.model_info_space())?)
    }
}

/// Small extension so steps read naturally.
pub trait SpaceOf {
    /// Embedding space of the adapter.
    fn model_info_space(&self) -> embeddings_fundamentals::ids::EmbeddingSpaceId;
}

impl SpaceOf for DeterministicEmbedder {
    fn model_info_space(&self) -> embeddings_fundamentals::ids::EmbeddingSpaceId {
        use embeddings_fundamentals::embedding::EmbeddingStrategy;
        self.model_info().embedding_space_id.clone()
    }
}

#[tokio::main]
async fn main() -> Result<ExitCode> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let milestone = support::inventory::acceptance_milestone(&root.join("config/acceptance.toml"))?;
    let cucumber_args = match std::env::var("BDD_ARGS") {
        Ok(args) => split_args(&args)?,
        Err(_) => Vec::new(),
    };
    let report = run_suite::<LabWorld>(Suite {
        features: &root.join("tests/features"),
        prd: &root.join("spec/PRD.md"),
        milestone,
        json_report: root.join("reports/bdd/cucumber.json"),
        console: true,
        cucumber_args,
        fingerprinted: vec![
            ("acceptance", root.join("config/acceptance.toml")),
            ("corpus_manifest", root.join("tests/fixtures/corpus/manifest.toml")),
        ],
    })
    .await?;
    println!(
        "bdd inventory: milestone {milestone}, {} required, {} executed",
        report.required, report.executed
    );
    for problem in &report.problems {
        eprintln!("bdd inventory: {problem}");
    }
    Ok(if report.passed() { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}
