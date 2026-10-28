//! Ingestion orchestration (PRD §4 reliability, PLAN Phase 1 step 5, §4).
//!
//! Validation runs before any extraction or adapter call: required capabilities and
//! remote-processing permission. Each source then goes through
//! `manifest → extraction → revision check → chunking → embedding → validation →
//! staging → publish`, and any failure is recorded with its stage, reason, and attempt
//! count. Independent sources keep going after a failure. A run with a failed required
//! source is `incomplete`.

use std::future::Future;
use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::chunking::{Chunk, ChunkingConfig, StructureAwareV1};
use crate::corpus::{LoadedManifest, SourceEntry};
use crate::domain::{Execution, ExtractionMethod, FailureStage, Modality, RunStatus, SourceStatus};
use crate::embedding::{
    Capability, EmbeddingError, EmbeddingStrategy, ErrorClass, InputKind, TextInput, validate_batch,
};
use crate::extraction::{ExtractionConfig, OcrEngine, PageText, TextExtractor};
use crate::ids::{
    Checksum, ChunkId, EmbeddingSpaceId, ExperimentId, Fingerprint, IdError, ItemId, RecordId,
    RevisionId, SourceId,
};
use crate::record::{Record, RecordEmbedding, RecordMetadata};
use crate::storage::{ExperimentSnapshot, FsRecordStore, PublishOutcome, StoreError};

/// Process exit code for a run with a failed required source.
pub const EXIT_INCOMPLETE: u8 = 2;
/// Process exit code for a run refused by validation.
pub const EXIT_VALIDATION: u8 = 3;

/// Waits between retries; replaced by a no-op in tests.
pub trait Sleeper: Send + Sync {
    /// Sleeps for `duration`.
    fn sleep(&self, duration: Duration) -> impl Future<Output = ()> + Send;
}

/// Real sleeper backed by `tokio::time::sleep`.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokioSleeper;

impl Sleeper for TokioSleeper {
    async fn sleep(&self, duration: Duration) {
        tokio::time::sleep(duration).await;
    }
}

/// Sleeper that returns immediately (deterministic tests).
#[derive(Debug, Clone, Copy, Default)]
pub struct NoSleep;

impl Sleeper for NoSleep {
    async fn sleep(&self, _duration: Duration) {}
}

/// Settings of one ingestion run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IngestSettings {
    /// Identifier of this run; records and the snapshot carry it.
    pub experiment_id: ExperimentId,
    /// Attempts per embedding batch, including the first.
    pub max_attempts: NonZeroU32,
    /// Base backoff; attempt `n` waits `backoff_ms × 2^(n−1)`.
    pub backoff_ms: u64,
    /// Upper bound on inputs per adapter call (the adapter's own limit also applies).
    pub batch_size: NonZeroUsize,
    /// Language tag recorded on text records.
    pub language: String,
    /// Extraction settings.
    pub extraction: ExtractionConfig,
    /// Chunking settings.
    pub chunking: ChunkingConfig,
}

impl IngestSettings {
    /// Defaults from PLAN §0.1 for a given experiment id.
    #[must_use]
    pub fn new(experiment_id: ExperimentId) -> Self {
        Self {
            experiment_id,
            max_attempts: NonZeroU32::new(3).unwrap_or(NonZeroU32::MIN),
            backoff_ms: 200,
            batch_size: NonZeroUsize::new(32).unwrap_or(NonZeroUsize::MIN),
            language: "pt-BR".to_owned(),
            extraction: ExtractionConfig::default(),
            chunking: ChunkingConfig::default(),
        }
    }
}

/// Refusals raised before any source is processed.
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    /// The adapter lacks a capability the run needs.
    #[error(transparent)]
    MissingCapability(#[from] EmbeddingError),
    /// Remote processing is not allowed for this corpus.
    #[error(
        "corpus {corpus} does not allow remote processing, but engine {engine} with model {model} is remote"
    )]
    RemoteNotPermitted {
        /// Corpus identifier.
        corpus: String,
        /// Engine identifier.
        engine: String,
        /// Model identifier.
        model: String,
    },
    /// The experiment id was already used; snapshots are append-only.
    #[error("experiment {0} already exists")]
    ExperimentExists(ExperimentId),
    /// The chunking configuration is invalid.
    #[error(transparent)]
    Chunking(#[from] crate::chunking::ChunkError),
    /// A fingerprint could not be computed.
    #[error(transparent)]
    Fingerprint(#[from] IdError),
    /// The store could not be read or written (an internal error, not a refusal).
    #[error(transparent)]
    Store(#[from] StoreError),
    /// A blocking store task panicked or was cancelled.
    #[error("store task failed: {0}")]
    Task(String),
}

impl ValidationError {
    /// CLI exit code: [`EXIT_VALIDATION`] for refusals, 1 for internal errors.
    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::MissingCapability(_)
            | Self::RemoteNotPermitted { .. }
            | Self::ExperimentExists(_)
            | Self::Chunking(_) => EXIT_VALIDATION,
            Self::Fingerprint(_) | Self::Store(_) | Self::Task(_) => 1,
        }
    }
}

/// Remote-permission check, used both by `ingest` and by the CLI before an adapter
/// is constructed (PLAN §4.4).
///
/// # Errors
///
/// Returns [`ValidationError::RemoteNotPermitted`] for a remote engine on a corpus
/// without `remote_allowed`.
pub fn check_remote_permission(
    manifest: &LoadedManifest,
    execution: Execution,
    engine: &str,
    model: &str,
) -> Result<(), ValidationError> {
    if execution == Execution::Remote && !manifest.manifest.remote_allowed {
        return Err(ValidationError::RemoteNotPermitted {
            corpus: manifest.manifest.corpus_id.clone(),
            engine: engine.to_owned(),
            model: model.to_owned(),
        });
    }
    Ok(())
}

/// Per-page extraction outcome.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageReport {
    /// 1-based page number.
    pub page: u32,
    /// Method used.
    pub method: ExtractionMethod,
    /// OCR produced no text.
    pub unreadable: bool,
    /// Extraction confidence, when reported.
    pub confidence: Option<f32>,
}

/// Outcome of one source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceReport {
    /// Source identifier.
    pub source_id: SourceId,
    /// Whether a failure makes the run incomplete.
    pub required: bool,
    /// Outcome.
    pub status: SourceStatus,
    /// Stage of failure.
    pub stage: Option<FailureStage>,
    /// Failure reason (no document content).
    pub reason: Option<String>,
    /// Adapter calls made for this source, including failed attempts.
    pub attempts: u32,
    /// Retries after transient failures.
    pub retries: u32,
    /// Records written by this run (0 when an intact revision was re-activated).
    pub records_committed: usize,
    /// An intact revision with the same id already existed and was re-activated.
    pub reused_revision: bool,
    /// Revision that is active after this run, or that was attempted.
    pub revision_id: Option<RevisionId>,
    /// The active revision was already current; nothing was re-embedded.
    pub unchanged: bool,
    /// Per-page extraction details.
    pub pages: Vec<PageReport>,
    /// Chunks truncated to fit the model limit.
    pub truncations: usize,
}

impl SourceReport {
    fn new(entry: &SourceEntry) -> Self {
        Self {
            source_id: entry.source_id.clone(),
            required: entry.required,
            status: SourceStatus::Failed,
            stage: None,
            reason: None,
            attempts: 0,
            retries: 0,
            records_committed: 0,
            reused_revision: false,
            revision_id: None,
            unchanged: false,
            pages: Vec::new(),
            truncations: 0,
        }
    }

    fn fail(mut self, stage: FailureStage, reason: impl Into<String>) -> Self {
        self.status = SourceStatus::Failed;
        self.stage = Some(stage);
        self.reason = Some(reason.into());
        warn!(source_id = %self.source_id, stage = stage.as_str(), "source failed");
        self
    }
}

/// Outcome of a run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunReport {
    /// Experiment id of the run.
    pub experiment_id: ExperimentId,
    /// Overall status.
    pub run_status: RunStatus,
    /// Embedding space written to.
    pub space: EmbeddingSpaceId,
    /// Checksum of the corpus manifest.
    pub corpus_checksum: Checksum,
    /// Extraction fingerprint.
    pub extraction_fingerprint: Fingerprint,
    /// Chunking fingerprint.
    pub chunking_fingerprint: Fingerprint,
    /// Per-source outcomes in manifest order.
    pub sources: Vec<SourceReport>,
    /// Total retries.
    pub retries: u32,
    /// Total truncations.
    pub truncations: usize,
    /// Total unreadable pages.
    pub unreadable_pages: usize,
}

impl RunReport {
    /// Process exit code the CLI uses for this run: 0 when it succeeded,
    /// [`EXIT_INCOMPLETE`] otherwise.
    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        match self.run_status {
            RunStatus::Succeeded => 0,
            RunStatus::Incomplete => EXIT_INCOMPLETE,
        }
    }

    /// Report of one source.
    #[must_use]
    pub fn source(&self, id: &SourceId) -> Option<&SourceReport> {
        self.sources.iter().find(|s| &s.source_id == id)
    }
}

/// Runs ingestion for one embedding space.
pub struct Pipeline<'a, T, O, S> {
    /// Text-layer extractor.
    pub text: &'a T,
    /// OCR engine.
    pub ocr: &'a O,
    /// Retry sleeper.
    pub sleeper: &'a S,
    /// Record store.
    pub store: Arc<FsRecordStore>,
    /// Embedding adapter.
    pub embedder: Arc<dyn EmbeddingStrategy>,
}

#[derive(Serialize)]
struct ExtractionFingerprintInput<'a> {
    config: &'a ExtractionConfig,
    text_engine: String,
    ocr_engine: String,
}

#[derive(Serialize)]
struct ChunkingFingerprintInput<'a> {
    config: &'a ChunkingConfig,
    tokenizer_model: &'a str,
    tokenizer_revision: Option<&'a str>,
    budget: Option<usize>,
}

impl<T: TextExtractor, O: OcrEngine, S: Sleeper> Pipeline<'_, T, O, S> {
    /// Runs a store operation on the blocking pool (directory fsync and renames block).
    async fn on_store<R, F>(&self, op: F) -> Result<R, ValidationError>
    where
        R: Send + 'static,
        F: FnOnce(&FsRecordStore) -> Result<R, StoreError> + Send + 'static,
    {
        let store = Arc::clone(&self.store);
        tokio::task::spawn_blocking(move || op(&store))
            .await
            .map_err(|e| ValidationError::Task(e.to_string()))?
            .map_err(ValidationError::Store)
    }

    /// Validates the run, processes every source, saves the experiment snapshot, and
    /// returns the run report.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError`] when the run is refused before any processing.
    /// Per-source failures are reported in the [`RunReport`], not as errors.
    pub async fn ingest(
        &self,
        manifest: &LoadedManifest,
        settings: &IngestSettings,
    ) -> Result<RunReport, ValidationError> {
        let info = self.embedder.model_info();
        let caps = self.embedder.capabilities();
        caps.require(Capability::TextDocuments, info)?;
        check_remote_permission(
            manifest,
            caps.execution,
            info.engine.as_str(),
            &info.model_id,
        )?;
        let experiment = settings.experiment_id.clone();
        // Any existing file, even an unreadable one, makes the id unusable: snapshots are
        // create-only, so the run is refused before any source is processed.
        if self
            .on_store(move |s| s.experiment_exists(&experiment))
            .await?
        {
            return Err(ValidationError::ExperimentExists(
                settings.experiment_id.clone(),
            ));
        }
        let chunker = StructureAwareV1::new(settings.chunking.clone())?;
        let budget = self.embedder.input_budget(InputKind::Document);
        let extraction_fingerprint = Fingerprint::of(&ExtractionFingerprintInput {
            config: &settings.extraction,
            text_engine: self.text.engine_id(),
            ocr_engine: self.ocr.engine_id(),
        })?;
        let chunking_fingerprint = Fingerprint::of(&ChunkingFingerprintInput {
            config: &settings.chunking,
            tokenizer_model: &info.model_id,
            tokenizer_revision: info.model_revision.as_deref(),
            budget,
        })?;
        let space = info.embedding_space_id.clone();
        info!(experiment_id = %settings.experiment_id, space = %space, "ingestion started");

        let mut sources = Vec::with_capacity(manifest.manifest.sources.len());
        for entry in &manifest.manifest.sources {
            let ctx = SourceContext {
                manifest,
                entry,
                settings,
                chunker: &chunker,
                budget,
                extraction_fingerprint: &extraction_fingerprint,
                chunking_fingerprint: &chunking_fingerprint,
            };
            sources.push(self.process_source(&ctx).await);
        }

        let run_status = if sources
            .iter()
            .any(|s| s.required && s.status == SourceStatus::Failed)
        {
            RunStatus::Incomplete
        } else {
            RunStatus::Succeeded
        };
        let active_space = space.clone();
        let revisions = self.on_store(move |s| s.active(&active_space)).await?;
        let snapshot = ExperimentSnapshot {
            experiment_id: settings.experiment_id.clone(),
            revisions,
            fingerprints: [
                ("extraction".to_owned(), extraction_fingerprint.clone()),
                ("chunking".to_owned(), chunking_fingerprint.clone()),
                ("settings".to_owned(), Fingerprint::of(settings)?),
            ]
            .into_iter()
            .collect(),
            corpus_checksum: manifest.manifest_checksum.clone(),
        };
        self.on_store(move |s| s.save_experiment(&snapshot)).await?;
        let report = RunReport {
            experiment_id: settings.experiment_id.clone(),
            run_status,
            space,
            corpus_checksum: manifest.manifest_checksum.clone(),
            extraction_fingerprint,
            chunking_fingerprint,
            retries: sources.iter().map(|s| s.retries).sum(),
            truncations: sources.iter().map(|s| s.truncations).sum(),
            unreadable_pages: sources
                .iter()
                .map(|s| s.pages.iter().filter(|p| p.unreadable).count())
                .sum(),
            sources,
        };
        info!(experiment_id = %report.experiment_id, status = report.run_status.as_str(), "ingestion finished");
        Ok(report)
    }

    async fn extract_pages(
        &self,
        path: &std::path::Path,
        config: &ExtractionConfig,
        report: &mut SourceReport,
    ) -> Result<Vec<PageText>, String> {
        let count = self
            .text
            .page_count(path)
            .await
            .map_err(|e| e.to_string())?;
        let mut pages = Vec::new();
        for n in 1..=count.get() {
            let page = NonZeroU32::new(n).unwrap_or(NonZeroU32::MIN);
            let native = self
                .text
                .extract_page(path, page)
                .await
                .map_err(|e| e.to_string())?;
            let text = if config.native_text_is_usable(&native.text()) {
                native
            } else {
                self.ocr
                    .ocr_page(path, page)
                    .await
                    .map_err(|e| e.to_string())?
            };
            report.pages.push(PageReport {
                page: n,
                method: text.method,
                unreadable: text.is_blank(),
                confidence: text.confidence,
            });
            pages.push(text);
        }
        Ok(pages)
    }

    async fn process_source(&self, ctx: &SourceContext<'_>) -> SourceReport {
        let entry = ctx.entry;
        let mut report = SourceReport::new(entry);
        if !entry.input_type.is_pdf() {
            report.status = SourceStatus::NotApplicable;
            report.reason = Some("image input does not apply to a text run".to_owned());
            return report;
        }
        let checksum = match ctx.manifest.verify(entry) {
            Ok(checksum) => checksum,
            Err(e) => return report.fail(FailureStage::Manifest, e.to_string()),
        };
        // The revision depends only on the file checksum and the extraction and chunking
        // fingerprints, so an unchanged source is recognised before any extraction work.
        let revision = match RevisionId::derive(
            &checksum,
            ctx.extraction_fingerprint,
            ctx.chunking_fingerprint,
        ) {
            Ok(revision) => revision,
            Err(e) => return report.fail(FailureStage::Manifest, e.to_string()),
        };
        report.revision_id = Some(revision.clone());
        let space = self.embedder.model_info().embedding_space_id.clone();
        let (pointer_space, pointer_source) = (space.clone(), entry.source_id.clone());
        let current = self
            .on_store(move |s| {
                let pointer = s.active_pointer(&pointer_space, &pointer_source)?;
                let intact = s.active_is_intact(&pointer_space, &pointer_source)?;
                Ok(pointer.map(|p| (p.revision_id, intact)))
            })
            .await;
        match current {
            Ok(Some((active, true))) if active == revision => {
                report.status = SourceStatus::Succeeded;
                report.unchanged = true;
                return report;
            }
            Ok(Some((active, false))) if active == revision => {
                // The pointer names this revision but its records are missing or altered:
                // rebuild it; `publish` quarantines the damaged directory.
                warn!(source_id = %entry.source_id, "active revision is damaged; rebuilding");
            }
            Ok(_) => {}
            Err(e) => return report.fail(FailureStage::Storage, e.to_string()),
        }

        let path = ctx.manifest.path_of(entry);
        let pages = match self
            .extract_pages(&path, &ctx.settings.extraction, &mut report)
            .await
        {
            Ok(pages) => pages,
            Err(reason) => return report.fail(FailureStage::Extraction, reason),
        };
        let unreadable: Vec<u32> = report
            .pages
            .iter()
            .filter(|p| p.unreadable)
            .map(|p| p.page)
            .collect();
        if !unreadable.is_empty() {
            return report.fail(
                FailureStage::Extraction,
                format!("unreadable pages {unreadable:?}: OCR produced no text"),
            );
        }

        let mut chunks: Vec<Chunk> = Vec::new();
        for page in &pages {
            match ctx
                .chunker
                .chunk(page, self.embedder.token_counter(), ctx.budget)
            {
                Ok(page_chunks) => chunks.extend(page_chunks),
                Err(e) => return report.fail(FailureStage::Chunking, e.to_string()),
            }
        }
        report.truncations = chunks.iter().filter(|c| c.truncation.is_some()).count();
        let mut chunk_ids = Vec::with_capacity(chunks.len());
        for chunk in &chunks {
            match ChunkId::new(&entry.source_id, chunk.page, Modality::Text, chunk.index) {
                Ok(id) => chunk_ids.push(id),
                Err(e) => return report.fail(FailureStage::Chunking, e.to_string()),
            }
        }
        let inputs: Vec<TextInput> = chunk_ids
            .iter()
            .zip(&chunks)
            .map(|(id, chunk)| TextInput {
                id: ItemId::from(id),
                text: chunk.text.clone(),
            })
            .collect();

        let vectors = match self.embed_all(&inputs, ctx.settings, &mut report).await {
            Ok(vectors) => vectors,
            Err((stage, reason)) => return report.fail(stage, reason),
        };

        let info = self.embedder.model_info();
        let mut records = Vec::with_capacity(chunks.len());
        for ((chunk, chunk_id), vector) in chunks.into_iter().zip(chunk_ids).zip(vectors) {
            let record_id = match RecordId::derive(&revision, &chunk_id, &space) {
                Ok(id) => id,
                Err(e) => return report.fail(FailureStage::Storage, e.to_string()),
            };
            records.push(Record {
                record_id,
                chunk_id,
                source_id: entry.source_id.clone(),
                revision_id: revision.clone(),
                chunk_text: Some(chunk.text),
                modality: Modality::Text,
                asset_uri: None,
                metadata: RecordMetadata {
                    source_file: entry.file.display().to_string(),
                    source_checksum: checksum.clone(),
                    document_type: entry.document_type,
                    language: ctx.settings.language.clone(),
                    page_number: chunk.page.get(),
                    section: chunk.section,
                    chunk_index: chunk.index,
                    chunk_kind: chunk.kind,
                    bounding_box: chunk.bounding_box,
                    extraction_method: chunk.extraction_method,
                    extraction_engine: chunk.extraction_engine,
                    extraction_confidence: chunk.extraction_confidence,
                    chunking_strategy: ctx.settings.chunking.strategy.clone(),
                    overlap_tokens: ctx.settings.chunking.overlap_tokens,
                    truncation: chunk.truncation,
                    anonymization_status: "verified".to_owned(),
                },
                embedding: RecordEmbedding::new(info, vector),
                experiment_id: ctx.settings.experiment_id.clone(),
            });
        }

        let store = Arc::clone(&self.store);
        let source_id = entry.source_id.clone();
        let stored = tokio::task::spawn_blocking(move || {
            let staged = store.stage(&space, &source_id, &revision, &records)?;
            store.publish(staged)
        })
        .await;
        match stored {
            Ok(Ok(outcome)) => {
                report.status = SourceStatus::Succeeded;
                match outcome {
                    PublishOutcome::Installed { records } => report.records_committed = records,
                    PublishOutcome::Reused { records } => {
                        report.reused_revision = true;
                        info!(source_id = %entry.source_id, records, "intact revision re-activated");
                    }
                }
                info!(source_id = %entry.source_id, records = report.records_committed, "source committed");
                report
            }
            Ok(Err(e)) => report.fail(FailureStage::Storage, e.to_string()),
            Err(e) => report.fail(FailureStage::Storage, format!("storage task failed: {e}")),
        }
    }

    /// Embeds `inputs` in batches with class-based retries, validating every batch.
    async fn embed_all(
        &self,
        inputs: &[TextInput],
        settings: &IngestSettings,
        report: &mut SourceReport,
    ) -> Result<Vec<Vec<f32>>, (FailureStage, String)> {
        let caps = self.embedder.capabilities();
        let batch = caps.batch_limit().map_or(settings.batch_size.get(), |max| {
            max.min(settings.batch_size.get())
        });
        let mut vectors = Vec::with_capacity(inputs.len());
        for batch_inputs in inputs.chunks(batch.max(1)) {
            let ids: Vec<ItemId> = batch_inputs.iter().map(|i| i.id.clone()).collect();
            let mut attempt: u32 = 0;
            let embedded = loop {
                attempt += 1;
                report.attempts += 1;
                match self.embedder.embed_documents(batch_inputs).await {
                    Ok(embedded) => break embedded,
                    Err(e)
                        if e.class() == ErrorClass::Transient
                            && attempt < settings.max_attempts.get() =>
                    {
                        report.retries += 1;
                        let factor = 1u64.checked_shl(attempt - 1).unwrap_or(u64::MAX);
                        let wait =
                            Duration::from_millis(settings.backoff_ms.saturating_mul(factor));
                        warn!(source_id = %report.source_id, attempt, "transient embedding failure, retrying");
                        self.sleeper.sleep(wait).await;
                    }
                    Err(e) => {
                        return Err((
                            FailureStage::Embedding,
                            format!(
                                "{e} (class {:?}, attempt {attempt} of {})",
                                e.class(),
                                settings.max_attempts
                            ),
                        ));
                    }
                }
            };
            validate_batch(&embedded, &ids, self.embedder.model_info())
                .map_err(|defect| (FailureStage::Validation, defect.to_string()))?;
            vectors.extend(embedded.embeddings.into_iter().map(|e| e.vector));
        }
        Ok(vectors)
    }
}

struct SourceContext<'a> {
    manifest: &'a LoadedManifest,
    entry: &'a SourceEntry,
    settings: &'a IngestSettings,
    chunker: &'a StructureAwareV1,
    budget: Option<usize>,
    extraction_fingerprint: &'a Fingerprint,
    chunking_fingerprint: &'a Fingerprint,
}
