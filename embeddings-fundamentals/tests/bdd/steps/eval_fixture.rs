//! Shared fixtures for the evaluation and measurement steps (US-03, US-04, US-05):
//! the keyword-vector index over the synthetic corpus, the frozen dataset, and
//! measured experiment runs with controlled instruments.

use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use embeddings_fundamentals::adapters::BuildError;
use embeddings_fundamentals::cost::PriceTable;
use embeddings_fundamentals::embedding::EmbeddingStrategy;
use embeddings_fundamentals::embedding::deterministic::DeterministicBuilder;
use embeddings_fundamentals::evaluation::Dataset;
use embeddings_fundamentals::experiment::{self, ExperimentInputs, Instruments, Tools};
use embeddings_fundamentals::fidelity::LoadedSample;
use embeddings_fundamentals::measurement::{FixedMemory, ManualClock};
use embeddings_fundamentals::pipeline::NoSleep;
use embeddings_fundamentals::report::ConfigurationResult;
use serde::Deserialize;

use crate::{LabWorld, fixture};

/// The three synthetic PDFs (6 pages, 28 text chunks).
pub const PDF_FIXTURES: [&str; 3] = ["lab-searchable-001", "us-mixed-003", "lab-scanned-002"];

/// Path under the crate root.
pub fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)
}

#[derive(Debug, Deserialize)]
struct KeywordFile {
    dimensions: usize,
    keywords: Vec<Keyword>,
}

#[derive(Debug, Deserialize)]
struct Keyword {
    keyword: String,
    components: Vec<(usize, f32)>,
}

/// Deterministic builder with the keyword axes of `tests/fixtures/evaluation`.
pub fn keyword_builder() -> Result<DeterministicBuilder> {
    let text = std::fs::read_to_string(fixture("evaluation/keywords.toml"))
        .context("read keywords.toml")?;
    let file: KeywordFile = toml::from_str(&text).context("parse keywords.toml")?;
    let mut builder =
        embeddings_fundamentals::embedding::deterministic::DeterministicEmbedder::builder()
            .dimensions(NonZeroUsize::new(file.dimensions).context("dimensions")?);
    for k in &file.keywords {
        builder = builder.with_keyword_vector(&k.keyword, &k.components);
    }
    Ok(builder)
}

/// The frozen query set with `labels` (a path under the crate root).
pub fn dataset(labels: &str) -> Result<Dataset> {
    Ok(Dataset::load(
        &repo("config/evaluation/queries-v1.toml"),
        &repo(labels),
    )?)
}

/// Adds the three synthetic PDFs with replayed extraction.
pub fn add_pdf_corpus(world: &mut LabWorld) -> Result<()> {
    world.use_captured_extraction()?;
    for id in PDF_FIXTURES {
        world.add_fixture_source(id, None)?;
    }
    Ok(())
}

/// Controlled observations of `tests/fixtures/measurements/expected.toml`.
#[derive(Debug, Deserialize)]
pub struct Controlled {
    pub tick_ms: u64,
    pub peak_rss_bytes: u64,
    pub latency_repeats: usize,
    pub hardware: embeddings_fundamentals::measurement::Hardware,
    pub expected: toml::Table,
}

/// Loads the controlled observations.
pub fn controlled() -> Result<Controlled> {
    let text = std::fs::read_to_string(fixture("measurements/expected.toml"))
        .context("read measurements/expected.toml")?;
    Ok(toml::from_str(&text)?)
}

/// Runs one measured experiment with a fresh `ManualClock` and a fixed memory probe.
pub async fn measured_run(
    world: &mut LabWorld,
    builder: DeterministicBuilder,
    labels: &str,
    price: Option<&Path>,
    fidelity: bool,
) -> Result<ConfigurationResult> {
    let controlled = controlled()?;
    let manifest = world.manifest()?;
    let dataset = dataset(labels)?;
    let mut settings = world.settings.clone();
    settings.experiment_id = format!("measured-{}", world.configs.len() + 1).parse()?;
    let price = price.map(PriceTable::load).transpose()?;
    let sample = if fidelity {
        Some(LoadedSample::load(&repo(
            "config/fidelity/sample-synthetic-v1.toml",
        ))?)
    } else {
        None
    };
    let instruments = Instruments {
        clock: Arc::new(ManualClock::new(Duration::from_millis(controlled.tick_ms))),
        memory: Arc::new(FixedMemory(Some(controlled.peak_rss_bytes))),
    };
    let build = async || -> Result<Arc<dyn EmbeddingStrategy>, BuildError> {
        let adapter = builder.clone().build().map_err(|e| BuildError::Load {
            engine: embeddings_fundamentals::embedding::EngineId::Deterministic,
            reason: e.to_string(),
        })?;
        let adapter: Arc<dyn EmbeddingStrategy> = Arc::new(adapter);
        Ok(adapter)
    };
    let result = experiment::run(
        Tools {
            text: &*world.extractor,
            ocr: &*world.extractor,
            sleeper: &NoSleep,
        },
        &world.store,
        build,
        &instruments,
        ExperimentInputs {
            manifest: &manifest,
            settings: &settings,
            dataset: &dataset,
            latency_repeats: NonZeroUsize::new(controlled.latency_repeats)
                .context("latency_repeats")?,
            price: price.as_ref(),
            fidelity: sample.as_ref(),
        },
    )
    .await?;
    world.configs.push(result.clone());
    Ok(result)
}

/// Whitespace tokens of every chunk text in the run's active index, recomputed from
/// the store (the deterministic adapter reports whitespace tokens as usage).
pub fn whitespace_tokens(world: &LabWorld, result: &ConfigurationResult) -> Result<u64> {
    let records = world.store.active_records(&result.embedding_space_id)?;
    Ok(records
        .iter()
        .filter_map(|r| r.chunk_text.as_deref())
        .map(|t| u64::try_from(t.split_whitespace().count()).unwrap_or(u64::MAX))
        .sum())
}
