//! A measured experiment: ingestion, evaluation, latency, resources, and cost of one
//! configuration (PRD §9.4, §10).
//!
//! Sequence (every interval is read from the injected [`Clock`]):
//! 1. load the adapter (ingestion model-load time) and ingest the corpus through the
//!    measuring decorator (ingestion duration, per-batch embedding latency, usage);
//!    The process is fresh, so its cold start (PRD §9.4) is that first load plus the
//!    first embedding call (the first ingestion batch, which also opens the first
//!    connection of a remote engine);
//! 2. check the labels of the run's modality against the pinned snapshot, then load a
//!    second adapter for the query side and run the first frozen query (the discarded
//!    warm-up query). This runs in the already warm process and is reported as
//!    `query_side_start`, never as a cold start;
//! 3. evaluate every frozen query once in the experiment's pinned snapshot (top 3; its
//!    provider usage is the query-set cost): text-only after a text run, image-only
//!    (text queries against image records) after an image run;
//! 4. repeat every frozen query `latency_repeats` times for warm latency (nearest-rank
//!    median and p95 over the pooled samples);
//! 5. read peak local RAM, the index size, and fidelity over the indexed records.
//!
//! A combined experiment ([`run_combined`], PRD §6.6) ingests nothing: it pairs the
//! snapshots of a text run and an image run, pins both in a snapshot of its own, and
//! measures fused retrieval over them.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;

use crate::adapters::BuildError;
use crate::corpus::LoadedManifest;
use crate::cost::{Billed, PriceTable, estimate};
use crate::domain::{Execution, Modality, RunStatus, SourceStatus};
use crate::embedding::{EmbeddingStrategy, InputKind};
use crate::evaluation::{
    Dataset, EvalError, EvaluationReport, check_image_labels, check_text_labels, evaluate_combined,
    evaluate_images, evaluate_text,
};
use crate::extraction::images::ImageExtractor;
use crate::extraction::{OcrEngine, TextExtractor};
use crate::fidelity::{self, LoadedSample};
use crate::fusion::FusionParams;
use crate::ids::{Checksum, EmbeddingSpaceId, ExperimentId, Fingerprint};
use crate::measurement::{
    Clock, Hardware, LatencyStats, MeasuredEmbedder, MemoryProbe, Observation, millis,
};
use crate::pipeline::{IngestSettings, Pipeline, RunReport, Sleeper, ValidationError};
use crate::report::{
    ColdStart, CombinedCosts, CombinedMeasurements, CombinedResult, CombinedStart,
    ConfigurationResult, Costs, EvaluationNotRun, EvaluationNotRunStatus, Fingerprints,
    IncompleteExperiment, IngestionMeasurement, LegConfiguration, Measurements, QuerySideStart,
    Reliability,
};
use crate::retrieval::{
    Leg, Scope, SearchError, SearchHit, read_records, search_combined, search_images, search_text,
};
use crate::storage::{EvaluationProvenance, ExperimentSnapshot, RevisionRef};
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
pub struct Tools<'a, T, O, I, S> {
    /// Text-layer extractor.
    pub text: &'a T,
    /// OCR engine.
    pub ocr: &'a O,
    /// Embedded-image extractor.
    pub images: &'a I,
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
    /// The leg experiments of a combined run cannot be paired.
    #[error("cannot combine: {0}")]
    Combination(String),
    /// The ingestion was incomplete and the query side failed (for example a frozen
    /// label points into a failed source, or the query adapter did not load), so the
    /// evaluation could not run. Carries the ingestion outcome to report (exit 2).
    #[error("ingestion is incomplete and the evaluation could not run: {}", .0.evaluation.reason)]
    EvaluationNotRun(Box<IncompleteExperiment>),
}

/// Runs one measured experiment. `build` constructs a fresh adapter for the selected
/// configuration; it is called twice (ingestion side and query side).
///
/// # Errors
///
/// Returns [`ExperimentError`] when the adapter cannot be built, ingestion is refused,
/// or evaluation fails. An incomplete ingestion still yields a result whose
/// `run_status` says so. When the ingestion is incomplete and any later step fails
/// (labels that cannot apply, query adapter, evaluation, latency), the evaluation
/// cannot run and [`ExperimentError::EvaluationNotRun`] carries the ingestion outcome
/// instead.
pub async fn run<T, O, I, S, C, M>(
    tools: Tools<'_, T, O, I, S>,
    store: &Arc<FsRecordStore>,
    build: impl AsyncFn() -> Result<Arc<dyn EmbeddingStrategy>, BuildError>,
    instruments: &Instruments<C, M>,
    inputs: ExperimentInputs<'_>,
) -> Result<ConfigurationResult, ExperimentError>
where
    T: TextExtractor,
    O: OcrEngine,
    I: ImageExtractor,
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
        images: tools.images,
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

    // 2.-5. Query side. Once the ingestion is incomplete, any failure from here on is
    // reported together with the ingestion outcome (exit 2) instead of discarding it.
    let failed_batches = embedding.failed_calls;
    let outcome = async {
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
        let modality = inputs.settings.modality;
        // Labels are checked against the pinned snapshot before any query-side work.
        let records = read_records(store, &space, scope).await?;
        match modality {
            Modality::Text => check_text_labels(inputs.dataset, &records)?,
            Modality::Image => check_image_labels(inputs.dataset, &records)?,
        }
        drop(records);
        let started = clock.now();
        let adapter = build().await?;
        let model_load_ms = millis(clock.now().saturating_sub(started));
        let measured = MeasuredEmbedder::new(adapter, Arc::clone(clock));
        let query_calls = measured.calls();
        let started = clock.now();
        search(modality, store, &space, &measured, &first_query.text, scope).await?;
        let first_query_ms = millis(clock.now().saturating_sub(started));

        // 3. One evaluation pass; its usage is the query-set cost.
        query_calls.clear();
        let evaluation =
            evaluate(modality, store, &space, &measured, inputs.dataset, scope).await?;
        let query_usage = query_calls.summary(InputKind::Query).usage_tokens;

        // 4. Warm latency.
        let mut samples = Vec::new();
        for _ in 0..inputs.latency_repeats.get() {
            for query in &inputs.dataset.queries.value.queries {
                let started = clock.now();
                search(modality, store, &space, &measured, &query.text, scope).await?;
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
        let provider_inference_memory = (execution == Execution::Remote).then(|| {
            Observation::unavailable("remote provider: inference memory is not observable")
        });
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
        Ok::<_, ExperimentError>(ConfigurationResult {
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
            reliability: reliability(&report, embedding.failed_calls),
            evaluation,
            measurements,
            fidelity,
        })
    }
    .await;
    match outcome {
        Err(e) if report.run_status == RunStatus::Incomplete => Err(
            ExperimentError::EvaluationNotRun(Box::new(IncompleteExperiment {
                hardware: instruments.hardware.clone(),
                experiment_id: report.experiment_id.clone(),
                run_status: report.run_status,
                evaluation: EvaluationNotRun {
                    status: EvaluationNotRunStatus::NotRun,
                    reason: e.to_string(),
                },
                reliability: reliability(&report, failed_batches),
                ingestion: report,
            })),
        ),
        outcome => outcome,
    }
}

/// Top-3 search in the retrieval mode of the run's modality.
async fn search(
    modality: Modality,
    store: &Arc<FsRecordStore>,
    space: &crate::ids::EmbeddingSpaceId,
    embedder: &dyn EmbeddingStrategy,
    query: &str,
    scope: Scope<'_>,
) -> Result<Vec<SearchHit>, SearchError> {
    match modality {
        Modality::Text => search_text(store, space, embedder, query, 3, scope).await,
        Modality::Image => search_images(store, space, embedder, query, 3, scope).await,
    }
}

/// One evaluation pass in the retrieval mode of the run's modality.
async fn evaluate(
    modality: Modality,
    store: &Arc<FsRecordStore>,
    space: &crate::ids::EmbeddingSpaceId,
    embedder: &dyn EmbeddingStrategy,
    dataset: &Dataset,
    scope: Scope<'_>,
) -> Result<EvaluationReport, EvalError> {
    match modality {
        Modality::Text => evaluate_text(store, space, embedder, dataset, scope).await,
        Modality::Image => evaluate_images(store, space, embedder, dataset, scope).await,
    }
}

fn reliability(report: &RunReport, failed_batches: usize) -> Reliability {
    Reliability {
        failed_sources: report
            .sources
            .iter()
            .filter(|s| s.status == SourceStatus::Failed)
            .count(),
        failed_pages: report.unreadable_pages,
        failed_batches,
        retries: report.retries,
        truncations: report.truncations,
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "page and chunk counts are far below 2^52"
)]
const fn count_f64(n: usize) -> f64 {
    n as f64
}

/// Inputs of one combined experiment.
#[derive(Debug, Clone, Copy)]
pub struct CombinedInputs<'a> {
    /// Id of the combined run; its snapshot pins both legs' revisions.
    pub experiment_id: &'a ExperimentId,
    /// Text run whose snapshot the text leg searches.
    pub text_experiment: &'a ExperimentId,
    /// Image run whose snapshot the image leg searches.
    pub image_experiment: &'a ExperimentId,
    /// Checksum of the corpus manifest both runs must have indexed.
    pub corpus_checksum: &'a Checksum,
    /// Frozen queries and labels both runs must have been evaluated with.
    pub dataset: &'a Dataset,
    /// Fusion parameters.
    pub fusion: FusionParams,
    /// Warm repetitions of every frozen query.
    pub latency_repeats: NonZeroUsize,
    /// Price table of the text leg's provider, when billed.
    pub text_price: Option<&'a PriceTable>,
    /// Price table of the image leg's provider, when billed.
    pub image_price: Option<&'a PriceTable>,
}

/// Runs one measured combined experiment (PRD §6.6, §9.4).
///
/// 1. Loads both leg snapshots and refuses a pair that indexed another corpus or was
///    evaluated with other frozen queries or labels;
/// 2. loads both query models (the fresh process's cold start) and refuses a leg whose
///    snapshot pins no revision of its model's space;
/// 3. saves a create-only snapshot pinning the text leg's revisions in the text space
///    and the image leg's revisions in the image space, so the combined run is
///    reproducible by its own id;
/// 4. runs the first fused query (discarded warm-up), one evaluation pass (its usage is
///    the query-set cost of each leg), and `latency_repeats` warm passes of fused
///    queries;
/// 5. reads peak local RAM and both legs' index sizes.
///
/// # Errors
///
/// Returns [`ExperimentError::Combination`] for a pair that cannot be combined,
/// [`ExperimentError::Build`] when a model cannot be loaded, [`ExperimentError::Store`]
/// when a snapshot is missing or the combined id is taken, and the evaluation or
/// search errors of the passes.
pub async fn run_combined<C, M>(
    store: &Arc<FsRecordStore>,
    build_text: impl AsyncFn() -> Result<Arc<dyn EmbeddingStrategy>, BuildError>,
    build_image: impl AsyncFn() -> Result<Arc<dyn EmbeddingStrategy>, BuildError>,
    instruments: &Instruments<C, M>,
    inputs: CombinedInputs<'_>,
) -> Result<CombinedResult, ExperimentError>
where
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

    // 1. Leg snapshots.
    let (text_snapshot, image_snapshot) = {
        let (store, text, image) = (
            Arc::clone(store),
            inputs.text_experiment.clone(),
            inputs.image_experiment.clone(),
        );
        tokio::task::spawn_blocking(move || {
            Ok::<_, StoreError>((
                store.load_experiment(&text)?,
                store.load_experiment(&image)?,
            ))
        })
        .await
        .map_err(|e| ExperimentError::Task(e.to_string()))??
    };
    let provenance = EvaluationProvenance {
        query_set_version: inputs.dataset.queries.value.version.clone(),
        query_set_checksum: inputs.dataset.queries.checksum.clone(),
        labels_version: inputs.dataset.labels.value.version.clone(),
        labels_checksum: inputs.dataset.labels.checksum.clone(),
    };
    for snapshot in [&text_snapshot, &image_snapshot] {
        let id = &snapshot.experiment_id;
        if &snapshot.corpus_checksum != inputs.corpus_checksum {
            return Err(ExperimentError::Combination(format!(
                "experiment {id} indexed corpus {}, not {}",
                snapshot.corpus_checksum, inputs.corpus_checksum
            )));
        }
        if snapshot.evaluation.as_ref() != Some(&provenance) {
            return Err(ExperimentError::Combination(format!(
                "experiment {id} was not evaluated with query set {} and labels {}",
                provenance.query_set_checksum, provenance.labels_checksum
            )));
        }
    }

    // 2. Both query models: the cold start of this fresh process.
    let started = clock.now();
    let text_adapter = build_text().await?;
    let text_model_load_ms = millis(clock.now().saturating_sub(started));
    let started = clock.now();
    let image_adapter = build_image().await?;
    let image_model_load_ms = millis(clock.now().saturating_sub(started));
    let text_info = text_adapter.model_info().clone();
    let image_info = image_adapter.model_info().clone();
    let (text_space, image_space) = (
        text_info.embedding_space_id.clone(),
        image_info.embedding_space_id.clone(),
    );
    let pinned = |snapshot: &ExperimentSnapshot, space: &EmbeddingSpaceId| -> Vec<RevisionRef> {
        snapshot
            .revisions
            .iter()
            .filter(|r| &r.space == space)
            .cloned()
            .collect()
    };
    let text_revisions = pinned(&text_snapshot, &text_space);
    let image_revisions = pinned(&image_snapshot, &image_space);
    for (snapshot, space, revisions) in [
        (&text_snapshot, &text_space, &text_revisions),
        (&image_snapshot, &image_space, &image_revisions),
    ] {
        if revisions.is_empty() {
            return Err(ExperimentError::Combination(format!(
                "experiment {} pins no revision in space {space} of its leg's model",
                snapshot.experiment_id
            )));
        }
    }

    // 3. The combined snapshot.
    let mut fingerprints = BTreeMap::new();
    for (leg, snapshot) in [("text", &text_snapshot), ("image", &image_snapshot)] {
        for (name, fingerprint) in &snapshot.fingerprints {
            fingerprints.insert(format!("{leg}.{name}"), fingerprint.clone());
        }
    }
    fingerprints.insert(
        crate::retrieval::FUSION_FINGERPRINT.to_owned(),
        Fingerprint::of(&inputs.fusion).map_err(|e| ExperimentError::Combination(e.to_string()))?,
    );
    let snapshot = ExperimentSnapshot {
        experiment_id: inputs.experiment_id.clone(),
        revisions: text_revisions.into_iter().chain(image_revisions).collect(),
        fingerprints,
        corpus_checksum: inputs.corpus_checksum.clone(),
        evaluation: Some(provenance),
    };
    {
        let (store, snapshot) = (Arc::clone(store), snapshot.clone());
        tokio::task::spawn_blocking(move || store.save_experiment(&snapshot))
            .await
            .map_err(|e| ExperimentError::Task(e.to_string()))??;
    }
    let scope = Scope::Experiment(&snapshot);

    // 4. Warm-up, evaluation, warm latency.
    let text_measured = MeasuredEmbedder::new(text_adapter, Arc::clone(clock));
    let image_measured = MeasuredEmbedder::new(image_adapter, Arc::clone(clock));
    let (text_calls, image_calls) = (text_measured.calls(), image_measured.calls());
    let text_leg = Leg {
        space: &text_space,
        embedder: &text_measured,
    };
    let image_leg = Leg {
        space: &image_space,
        embedder: &image_measured,
    };
    let fused = async |query: &str| {
        search_combined(store, text_leg, image_leg, inputs.fusion, query, 3, scope).await
    };
    let started = clock.now();
    fused(&first_query.text).await?;
    let first_query_ms = millis(clock.now().saturating_sub(started));
    text_calls.clear();
    image_calls.clear();
    let evaluation = evaluate_combined(
        store,
        text_leg,
        image_leg,
        inputs.fusion,
        inputs.dataset,
        scope,
    )
    .await?;
    let text_usage = text_calls.summary(InputKind::Query).usage_tokens;
    let image_usage = image_calls.summary(InputKind::Query).usage_tokens;
    let mut samples = Vec::new();
    for _ in 0..inputs.latency_repeats.get() {
        for query in &inputs.dataset.queries.value.queries {
            let started = clock.now();
            fused(&query.text).await?;
            samples.push(millis(clock.now().saturating_sub(started)));
        }
    }
    let warm_query_latency =
        LatencyStats::from_samples(&samples).ok_or(ExperimentError::NoQueries)?;

    // 5. Resources and cost.
    let peak_local_ram = instruments.memory.peak_rss_bytes().map_or_else(
        || Observation::unavailable("getrusage is not available on this platform"),
        |b| Observation::bytes(b, "local process memory (ru_maxrss high-water mark)"),
    );
    // The index that was scored: the revisions the combined snapshot pins, which later
    // publishes in either space do not change.
    let index_bytes = {
        let (store, pinned) = (Arc::clone(store), snapshot.revisions.clone());
        tokio::task::spawn_blocking(move || store.revisions_size_bytes(&pinned))
            .await
            .map_err(|e| ExperimentError::Task(e.to_string()))??
    };
    let text_execution = text_measured.capabilities().execution;
    let image_execution = image_measured.capabilities().execution;
    let gpu_memory = Observation::unavailable(
        if text_execution == Execution::Local && image_execution == Execution::Local {
            "inference runs on the local CPU; no GPU is used"
        } else {
            "a remote leg's GPU memory is not observable; local legs run on the CPU"
        },
    );
    let leg = |experiment: &ExperimentId,
               info: &crate::embedding::ModelInfo,
               execution: Execution|
     -> LegConfiguration {
        LegConfiguration {
            experiment_id: experiment.clone(),
            engine: info.engine,
            engine_version: info.engine_version.clone(),
            model_id: info.model_id.clone(),
            model_revision: info.model_revision.clone(),
            dimensions: info.dimensions.get(),
            prefix_policy: info.prefix_policy,
            execution,
            embedding_space_id: info.embedding_space_id.clone(),
        }
    };
    let billed = |info: &crate::embedding::ModelInfo, execution, usage, price| {
        estimate(
            execution,
            Billed {
                engine: info.engine,
                model_id: &info.model_id,
            },
            usage,
            price,
        )
    };
    Ok(CombinedResult {
        hardware: instruments.hardware.clone(),
        experiment_id: inputs.experiment_id.clone(),
        corpus_checksum: inputs.corpus_checksum.clone(),
        text: leg(inputs.text_experiment, &text_info, text_execution),
        image: leg(inputs.image_experiment, &image_info, image_execution),
        fusion: inputs.fusion,
        evaluation,
        measurements: CombinedMeasurements {
            cold_start: CombinedStart {
                text_model_load_ms,
                image_model_load_ms,
                first_query_ms,
                total_ms: text_model_load_ms + image_model_load_ms + first_query_ms,
            },
            warm_query_latency,
            latency_repeats: inputs.latency_repeats.get(),
            peak_local_ram,
            gpu_memory,
            index_size: Observation::bytes(
                index_bytes,
                "revisions of both legs pinned by the combined snapshot, on disk",
            ),
        },
        cost: CombinedCosts {
            text_query_set: billed(&text_info, text_execution, text_usage, inputs.text_price),
            image_query_set: billed(
                &image_info,
                image_execution,
                image_usage,
                inputs.image_price,
            ),
        },
    })
}
