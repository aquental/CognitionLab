//! A measured experiment: ingestion, evaluation, latency, resources, and cost of one
//! configuration (PRD §9.4, §10).
//!
//! Sequence (every interval is read from the injected [`Clock`]):
//! 1. load the adapter (ingestion model-load time) and ingest the corpus through the
//!    measuring decorator (ingestion duration, per-batch embedding latency, usage);
//!    The process is fresh, so its cold start (PRD §9.4) is that first load plus the
//!    first embedding call (the first ingestion batch, which also opens the first
//!    connection of a remote engine);
//! 2. load a second adapter for the query side and run the first frozen query (the
//!    discarded warm-up query). This runs in the already warm process and is reported
//!    as `query_side_start`, never as a cold start;
//! 3. evaluate every frozen query once in the experiment's pinned snapshot (top 3; its
//!    provider usage is the query-set cost);
//! 4. repeat every frozen query `latency_repeats` times for warm latency (nearest-rank
//!    median and p95 over the pooled samples);
//! 5. read peak local RAM, the index size, and fidelity over the indexed records.

use std::num::NonZeroUsize;
use std::sync::Arc;

use crate::adapters::BuildError;
use crate::corpus::LoadedManifest;
use crate::cost::{Billed, PriceTable, estimate};
use crate::domain::{Execution, SourceStatus};
use crate::embedding::{EmbeddingStrategy, InputKind};
use crate::evaluation::{Dataset, EvalError, evaluate_text};
use crate::extraction::{OcrEngine, TextExtractor};
use crate::fidelity::{self, LoadedSample};
use crate::measurement::{
    Clock, Hardware, LatencyStats, MeasuredEmbedder, MemoryProbe, Observation, millis,
};
use crate::pipeline::{IngestSettings, Pipeline, Sleeper, ValidationError};
use crate::report::{
    ColdStart, ConfigurationResult, Costs, Fingerprints, IngestionMeasurement, Measurements,
    QuerySideStart, Reliability,
};
use crate::retrieval::{Scope, SearchError, search_text};
use crate::storage::EvaluationProvenance;
use crate::storage::{FsRecordStore, StoreError};

/// Inputs of one measured experiment.
#[derive(Debug, Clone, Copy)]
pub struct ExperimentInputs<'a> {
    /// Corpus.
    pub manifest: &'a LoadedManifest,
    /// Ingestion settings (experiment id, batch size, extraction, chunking).
    pub settings: &'a IngestSettings,
    /// Frozen queries and labels.
    pub dataset: &'a Dataset,
    /// Warm repetitions of every frozen query.
    pub latency_repeats: NonZeroUsize,
    /// Provider price table, when the engine is billed.
    pub price: Option<&'a PriceTable>,
    /// Extraction-fidelity sample.
    pub fidelity: Option<&'a LoadedSample>,
}

/// Clock, memory probe, and hardware description used for every measurement. Generic
/// over the clock and probe (static dispatch, PLAN §2.2): the CLI uses the system clock
/// and `getrusage`, BDD a manual clock and a fixed probe.
#[derive(Debug, Clone)]
pub struct Instruments<C, M> {
    /// Monotonic clock, shared with the measuring decorators.
    pub clock: Arc<C>,
    /// Peak-memory probe.
    pub memory: M,
    /// The machine the measurements are taken on; recorded in every result.
    pub hardware: Hardware,
}

/// Extraction tools and retry sleeper of the run.
#[derive(Debug, Clone, Copy)]
pub struct Tools<'a, T, O, S> {
    /// Text-layer extractor.
    pub text: &'a T,
    /// OCR engine.
    pub ocr: &'a O,
    /// Retry sleeper.
    pub sleeper: &'a S,
}

/// Why an experiment could not produce a result.
#[derive(Debug, thiserror::Error)]
pub enum ExperimentError {
    /// The adapter could not be built.
    #[error(transparent)]
    Build(#[from] BuildError),
    /// Ingestion was refused before processing.
    #[error(transparent)]
    Validation(#[from] ValidationError),
    /// Evaluation failed.
    #[error(transparent)]
    Evaluation(#[from] EvalError),
    /// A latency query failed.
    #[error(transparent)]
    Search(#[from] SearchError),
    /// The store could not be read.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// The dataset has no query to measure.
    #[error("the query set is empty")]
    NoQueries,
    /// A blocking task failed.
    #[error("store task failed: {0}")]
    Task(String),
}

/// Runs one measured experiment. `build` constructs a fresh adapter for the selected
/// configuration; it is called twice (ingestion side and query side).
///
/// # Errors
///
/// Returns [`ExperimentError`] when the adapter cannot be built, ingestion is refused,
/// or evaluation fails. An incomplete ingestion still yields a result whose
/// `run_status` says so.
pub async fn run<T, O, S, C, M>(
    tools: Tools<'_, T, O, S>,
    store: &Arc<FsRecordStore>,
    build: impl AsyncFn() -> Result<Arc<dyn EmbeddingStrategy>, BuildError>,
    instruments: &Instruments<C, M>,
    inputs: ExperimentInputs<'_>,
) -> Result<ConfigurationResult, ExperimentError>
where
    T: TextExtractor,
    O: OcrEngine,
    S: Sleeper,
    C: Clock + 'static,
    M: MemoryProbe,
{
    let clock = &instruments.clock;
    let first_query = inputs
        .dataset
        .queries
        .value
        .queries
        .first()
        .ok_or(ExperimentError::NoQueries)?;

    // 1. Ingestion side.
    let started = clock.now();
    let adapter = build().await?;
    let ingestion_model_load_ms = millis(clock.now().saturating_sub(started));
    let measured = Arc::new(MeasuredEmbedder::new(adapter, Arc::clone(clock)));
    let calls = measured.calls();
    let pipeline = Pipeline {
        text: tools.text,
        ocr: tools.ocr,
        sleeper: tools.sleeper,
        store: Arc::clone(store),
        embedder: Arc::clone(&measured) as Arc<dyn EmbeddingStrategy>,
    };
    let started = clock.now();
    let dataset = inputs.dataset;
    let evaluation = EvaluationProvenance {
        query_set_version: dataset.queries.value.version.clone(),
        query_set_checksum: dataset.queries.checksum.clone(),
        labels_version: dataset.labels.value.version.clone(),
        labels_checksum: dataset.labels.checksum.clone(),
    };
    let report = pipeline
        .ingest_for_evaluation(inputs.manifest, inputs.settings, Some(evaluation))
        .await?;
    let ingestion = clock.now().saturating_sub(started);
    let embedding = calls.summary(InputKind::Document);
    // Extraction fidelity reads the extraction output of the sampled sources, never the
    // (possibly truncated) chunks; it runs after the ingestion interval is measured.
    let fidelity = match inputs.fidelity {
        None => None,
        Some(sample) => {
            let sources: std::collections::BTreeSet<_> = sample
                .sample
                .rows
                .iter()
                .map(|r| r.source_id.clone())
                .collect();
            let mut pages = Vec::new();
            for source in sources {
                match pipeline
                    .extract_source(inputs.manifest, &source, &inputs.settings.extraction)
                    .await
                {
                    Ok(extracted) => {
                        pages.extend(extracted.into_iter().map(|page| fidelity::ExtractedPage {
                            source_id: source.clone(),
                            page,
                        }))
                    }
                    // Its rows count as not found with method "missing".
                    Err(reason) => {
                        tracing::warn!(source_id = %source, %reason, "fidelity extraction failed")
                    }
                }
            }
            Some(fidelity::evaluate(sample, &pages))
        }
    };
    let first_call = calls.records().into_iter().next();
    let info = measured.model_info().clone();
    let execution = measured.capabilities().execution;
    drop(pipeline);
    drop(measured);

    let pages: usize = report.sources.iter().map(|s| s.pages.len()).sum();
    let duration_ms = millis(ingestion);
    let chunks = embedding.items;
    let ingestion_measurement = IngestionMeasurement {
        duration_ms,
        pages,
        chunks,
        ms_per_page: (pages > 0).then(|| duration_ms / count_f64(pages)),
        chunks_per_second: (!ingestion.is_zero())
            .then(|| count_f64(chunks) / ingestion.as_secs_f64()),
    };

    // 2. Query side: a second adapter in the same process (not a cold start).
    let experiment_id = inputs.settings.experiment_id.clone();
    let snapshot = {
        let (store, id) = (Arc::clone(store), experiment_id.clone());
        tokio::task::spawn_blocking(move || store.load_experiment(&id))
            .await
            .map_err(|e| ExperimentError::Task(e.to_string()))??
    };
    let scope = Scope::Experiment(&snapshot);
    let space = info.embedding_space_id.clone();
    let started = clock.now();
    let adapter = build().await?;
    let model_load_ms = millis(clock.now().saturating_sub(started));
    let measured = MeasuredEmbedder::new(adapter, Arc::clone(clock));
    let query_calls = measured.calls();
    let started = clock.now();
    search_text(store, &space, &measured, &first_query.text, 3, scope).await?;
    let first_query_ms = millis(clock.now().saturating_sub(started));

    // 3. One evaluation pass; its usage is the query-set cost.
    query_calls.clear();
    let evaluation = evaluate_text(store, &space, &measured, inputs.dataset, scope).await?;
    let query_usage = query_calls.summary(InputKind::Query).usage_tokens;

    // 4. Warm latency.
    let mut samples = Vec::new();
    for _ in 0..inputs.latency_repeats.get() {
        for query in &inputs.dataset.queries.value.queries {
            let started = clock.now();
            search_text(store, &space, &measured, &query.text, 3, scope).await?;
            samples.push(millis(clock.now().saturating_sub(started)));
        }
    }
    let warm_query_latency =
        LatencyStats::from_samples(&samples).ok_or(ExperimentError::NoQueries)?;

    // 5. Resources, cost, fidelity.
    let peak_local_ram = instruments.memory.peak_rss_bytes().map_or_else(
        || Observation::unavailable("getrusage is not available on this platform"),
        |b| Observation::bytes(b, "local process memory (ru_maxrss high-water mark)"),
    );
    let index_bytes = {
        let (store, space) = (Arc::clone(store), space.clone());
        tokio::task::spawn_blocking(move || store.index_size_bytes(&space))
            .await
            .map_err(|e| ExperimentError::Task(e.to_string()))??
    };
    let provider_inference_memory = (execution == Execution::Remote)
        .then(|| Observation::unavailable("remote provider: inference memory is not observable"));
    let measurements = Measurements {
        ingestion: ingestion_measurement,
        ingestion_model_load_ms,
        embedding: embedding.clone(),
        cold_start: {
            let first_operation_ms = first_call.as_ref().map(|c| millis(c.duration));
            ColdStart {
                model_load_ms: ingestion_model_load_ms,
                first_operation_ms,
                first_operation_items: first_call.as_ref().map(|c| c.items),
                total_ms: first_operation_ms.map(|ms| ingestion_model_load_ms + ms),
            }
        },
        query_side_start: QuerySideStart {
            model_load_ms,
            first_query_ms,
            total_ms: model_load_ms + first_query_ms,
        },
        warm_query_latency,
        latency_repeats: inputs.latency_repeats.get(),
        peak_local_ram,
        gpu_memory: Observation::unavailable(match execution {
            Execution::Local => "inference runs on the local CPU; no GPU is used",
            Execution::Remote => "remote provider: GPU memory is not observable",
        }),
        provider_inference_memory,
        index_size: Observation::bytes(index_bytes, "active revisions on disk"),
    };
    let billed = Billed {
        engine: info.engine,
        model_id: &info.model_id,
    };
    Ok(ConfigurationResult {
        hardware: instruments.hardware.clone(),
        experiment_id,
        run_status: report.run_status,
        engine: info.engine,
        engine_version: info.engine_version.clone(),
        model_id: info.model_id.clone(),
        model_revision: info.model_revision.clone(),
        weight_format: info.weight_format.clone(),
        dimensions: info.dimensions.get(),
        prefix_policy: info.prefix_policy,
        execution,
        embedding_space_id: space,
        batch_size: inputs.settings.batch_size.get(),
        corpus_checksum: report.corpus_checksum.clone(),
        fingerprints: Fingerprints {
            extraction: report.extraction_fingerprint.clone(),
            chunking: report.chunking_fingerprint.clone(),
            evaluation: evaluation.evaluation_fingerprint.clone(),
        },
        cost: Costs {
            ingestion: estimate(execution, billed, embedding.usage_tokens, inputs.price),
            query_set: estimate(execution, billed, query_usage, inputs.price),
        },
        reliability: Reliability {
            failed_sources: report
                .sources
                .iter()
                .filter(|s| s.status == SourceStatus::Failed)
                .count(),
            failed_pages: report.unreadable_pages,
            failed_batches: embedding.failed_calls,
            retries: report.retries,
            truncations: report.truncations,
        },
        evaluation,
        measurements,
        fidelity,
    })
}

#[expect(
    clippy::cast_precision_loss,
    reason = "page and chunk counts are far below 2^52"
)]
const fn count_f64(n: usize) -> f64 {
    n as f64
}
