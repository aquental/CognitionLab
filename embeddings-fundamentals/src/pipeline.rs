//! Ingestion orchestration (PRD §4 reliability, PLAN Phase 1 step 5, §4).
//!
//! Validation runs before any extraction or adapter call: required capabilities and
//! remote-processing permission. Each source then goes through
//! `manifest → extraction → revision check → chunking → embedding → validation →
//! staging → publish`, and any failure is recorded with its stage, reason, and attempt
//! count. Independent sources keep going after a failure. A run with a failed required
//! source is `incomplete`.
//!
//! A run embeds one modality (PLAN §0.1). A text run chunks the text of PDFs; image
//! files are `not_applicable` to it. An image run embeds image files and the images
//! embedded in PDFs (`manifest → image listing and extraction → metadata-free
//! re-encoding → asset storage → embedding → validation → staging → publish`); a PDF
//! with no qualifying image is `not_applicable` to it.

use std::future::Future;
use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::assets::{MetadataScrubber, PreparedImage};
use crate::chunking::{Chunk, ChunkKind, ChunkingConfig, StructureAwareV1};
use crate::corpus::{LoadedManifest, SourceEntry};
use crate::domain::{Execution, ExtractionMethod, FailureStage, Modality, RunStatus, SourceStatus};
use crate::embedding::{
    AssetRef, Capability, EmbeddingBatch, EmbeddingError, EmbeddingStrategy, ErrorClass, InputKind,
    TextInput, validate_batch,
};
use crate::extraction::images::{ImageExtractor, ImageFilter, mask_of};
use crate::extraction::{ExtractionConfig, OcrEngine, PageText, TextExtractor};
use crate::ids::{
    Checksum, ChunkId, EmbeddingSpaceId, ExperimentId, Fingerprint, IdError, ItemId, RecordId,
    RevisionId, SourceId,
};
use crate::record::{OriginalAsset, Record, RecordEmbedding, RecordMetadata};
use crate::storage::{
    EvaluationProvenance, ExperimentSnapshot, FsRecordStore, PublishOutcome, StoreError,
};

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
    /// Modality this run embeds (PLAN §0.1: one modality per run).
    #[serde(default = "text_modality")]
    pub modality: Modality,
    /// Which embedded PDF images an image run keeps.
    #[serde(default)]
    pub images: ImageFilter,
}

const fn text_modality() -> Modality {
    Modality::Text
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
            modality: Modality::Text,
            images: ImageFilter::default(),
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
    /// Sources fail the eligibility gate (not eligible, anonymization review incomplete,
    /// unreadable, or checksum mismatch). The run is refused before any processing.
    #[error("ineligible sources: {}", .0.iter().map(|(s, r)| format!("{s} ({r})")).collect::<Vec<_>>().join("; "))]
    Ineligible(Vec<(SourceId, String)>),
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
            | Self::Ineligible(_)
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

/// Eligibility gate for every source in the manifest (PLAN §4.4): eligible, a complete
/// anonymization review, a readable file, and a matching checksum. Reads and hashes
/// files, so async callers run it on the blocking pool.
///
/// # Errors
///
/// Returns [`ValidationError::Ineligible`] listing every failing source.
pub fn check_eligibility(manifest: &LoadedManifest) -> Result<(), ValidationError> {
    let failures: Vec<(SourceId, String)> = manifest
        .manifest
        .sources
        .iter()
        .filter_map(|source| {
            manifest
                .verify(source)
                .err()
                .map(|e| (source.source_id.clone(), e.to_string()))
        })
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(ValidationError::Ineligible(failures))
    }
}

/// A PDF extraction failure, with the page it happened on.
#[derive(Debug, thiserror::Error)]
pub enum PdfExtractionError {
    /// The page count could not be read; no page was extracted.
    #[error(transparent)]
    PageCount(crate::extraction::ExtractionError),
    /// A page could not be extracted; every earlier page was.
    #[error("page {page}: {source}")]
    Page {
        /// 1-based page that failed.
        page: NonZeroU32,
        /// Native extraction or OCR error.
        #[source]
        source: crate::extraction::ExtractionError,
    },
}

/// Extracts every page of a PDF the way ingestion does: the native text layer when it is
/// usable (`ExtractionConfig::native_text_is_usable`), otherwise OCR of that page. It
/// reads no manifest and applies no eligibility gate: callers run the gate first.
/// `on_page` sees each page as soon as it is extracted, so a caller keeps the pages
/// that preceded a failure.
///
/// # Errors
///
/// Returns [`PdfExtractionError::PageCount`] when the page count fails and
/// [`PdfExtractionError::Page`] for the first page whose native extraction or OCR
/// fails.
pub async fn extract_pdf<T: TextExtractor, O: OcrEngine>(
    text: &T,
    ocr: &O,
    path: &std::path::Path,
    config: &ExtractionConfig,
    mut on_page: impl FnMut(&PageText) + Send,
) -> Result<Vec<PageText>, PdfExtractionError> {
    let count = text
        .page_count(path)
        .await
        .map_err(PdfExtractionError::PageCount)?;
    let mut pages = Vec::new();
    for n in 1..=count.get() {
        let page = NonZeroU32::new(n).unwrap_or(NonZeroU32::MIN);
        let at = |source| PdfExtractionError::Page { page, source };
        let native = text.extract_page(path, page).await.map_err(at)?;
        let extracted = if config.native_text_is_usable(&native.text()) {
            native
        } else {
            ocr.ocr_page(path, page).await.map_err(at)?
        };
        on_page(&extracted);
        pages.push(extracted);
    }
    Ok(pages)
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
pub struct Pipeline<'a, T, O, I, S> {
    /// Text-layer extractor.
    pub text: &'a T,
    /// OCR engine.
    pub ocr: &'a O,
    /// Embedded-image extractor (image runs).
    pub images: &'a I,
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
    rules: &'static str,
    tokenizer: &'a str,
    budget: Option<usize>,
}

/// Extraction fingerprint of an image run: the text layer decides which pages need OCR
/// (their rasters are scans, not image records); `pdfimages` lists and extracts.
#[derive(Serialize)]
struct ImageExtractionFingerprintInput<'a> {
    config: &'a ExtractionConfig,
    text_engine: String,
    image_engine: String,
}

/// Image-preparation fingerprint: an image run's counterpart of the chunking
/// fingerprint (it takes that slot in the revision id and the reports).
#[derive(Serialize)]
struct ImagePreparationFingerprintInput<'a> {
    scrubber: &'a str,
    filter: &'a ImageFilter,
    ocr_pages: &'a str,
}

/// Rule recorded in the image-preparation fingerprint.
const OCR_PAGE_RULE: &str = "images on pages that need OCR are page scans, not image records";

/// Language tag of image records (BCP 47 "no linguistic content").
const IMAGE_LANGUAGE: &str = "zxx";

/// Inputs of one source's adapter calls.
#[derive(Clone, Copy)]
enum Inputs<'a> {
    Text(&'a [TextInput]),
    Images(&'a [AssetRef]),
}

/// One extracted image before preparation.
struct RawImage {
    page: NonZeroU32,
    index: u32,
    pdf_image_num: Option<u32>,
    method: ExtractionMethod,
    bytes: Vec<u8>,
    /// The PDF image's soft or explicit mask, as `pdfimages` extracted it.
    mask: Option<Vec<u8>>,
}

/// One image to embed: where it came from and its prepared asset.
struct SourceImage {
    page: NonZeroU32,
    index: u32,
    pdf_image_num: Option<u32>,
    method: ExtractionMethod,
    prepared: PreparedImage,
}

/// How the shared start of a source ended.
enum Start {
    /// The source is finished (unchanged or failed); its report is final.
    Done(SourceReport),
    /// Work continues with the verified checksum and the derived revision.
    Proceed {
        report: SourceReport,
        checksum: Checksum,
        revision: RevisionId,
    },
}

impl<T: TextExtractor, O: OcrEngine, I: ImageExtractor, S: Sleeper> Pipeline<'_, T, O, I, S> {
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
        self.ingest_for_evaluation(manifest, settings, None).await
    }

    /// [`Self::ingest`] for an experiment: the create-only snapshot also records the
    /// frozen query set and labels the experiment evaluates (PLAN §4.2).
    ///
    /// # Errors
    ///
    /// As [`Self::ingest`].
    pub async fn ingest_for_evaluation(
        &self,
        manifest: &LoadedManifest,
        settings: &IngestSettings,
        evaluation: Option<EvaluationProvenance>,
    ) -> Result<RunReport, ValidationError> {
        let info = self.embedder.model_info();
        let caps = self.embedder.capabilities();
        // Capabilities are checked before anything else, so an unsupported run touches
        // no source, extractor, adapter call, or store write.
        match settings.modality {
            Modality::Text => caps.require(Capability::TextDocuments, info)?,
            Modality::Image => {
                caps.require(Capability::TextToImage, info)?;
                caps.require(Capability::Images, info)?;
            }
        }
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
        let gate = manifest.clone();
        tokio::task::spawn_blocking(move || check_eligibility(&gate))
            .await
            .map_err(|e| ValidationError::Task(e.to_string()))??;
        let chunker = StructureAwareV1::new(settings.chunking.clone())?;
        let (budget, extraction_fingerprint, chunking_fingerprint) = match settings.modality {
            Modality::Text => {
                let budget = self.embedder.input_budget(InputKind::Document);
                (
                    budget,
                    Fingerprint::of(&ExtractionFingerprintInput {
                        config: &settings.extraction,
                        text_engine: self.text.engine_id(),
                        ocr_engine: self.ocr.engine_id(),
                    })?,
                    Fingerprint::of(&ChunkingFingerprintInput {
                        config: &settings.chunking,
                        rules: crate::chunking::RULES_ID,
                        tokenizer: self.embedder.token_counter().id(),
                        budget,
                    })?,
                )
            }
            Modality::Image => (
                None,
                Fingerprint::of(&ImageExtractionFingerprintInput {
                    config: &settings.extraction,
                    text_engine: self.text.engine_id(),
                    image_engine: self.images.engine_id(),
                })?,
                Fingerprint::of(&ImagePreparationFingerprintInput {
                    scrubber: MetadataScrubber.id(),
                    filter: &settings.images,
                    ocr_pages: OCR_PAGE_RULE,
                })?,
            ),
        };
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
            evaluation,
            run_status: Some(run_status),
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

    /// Extracts every page of one manifest source exactly as ingestion does (native text
    /// or OCR per page), without chunking, embedding, or storing anything. Extraction
    /// fidelity reads this output, so chunking budgets cannot change it.
    ///
    /// # Errors
    ///
    /// Returns a reason when the source is not in the manifest or extraction fails.
    pub async fn extract_source(
        &self,
        manifest: &LoadedManifest,
        source: &crate::ids::SourceId,
        config: &ExtractionConfig,
    ) -> Result<Vec<PageText>, String> {
        let entry = manifest
            .manifest
            .sources
            .iter()
            .find(|e| &e.source_id == source)
            .ok_or_else(|| format!("source {source} is not in the manifest"))?;
        if !entry.input_type.is_pdf() {
            return Ok(Vec::new());
        }
        let mut report = SourceReport::new(entry);
        self.extract_pages(&manifest.path_of(entry), config, &mut report)
            .await
    }

    async fn extract_pages(
        &self,
        path: &std::path::Path,
        config: &ExtractionConfig,
        report: &mut SourceReport,
    ) -> Result<Vec<PageText>, String> {
        // Each page is reported as it is extracted, so a failed source still shows how
        // far extraction got.
        let pages = &mut report.pages;
        extract_pdf(self.text, self.ocr, path, config, |text| {
            pages.push(PageReport {
                page: text.page.get(),
                method: text.method,
                unreadable: text.is_blank(),
                confidence: text.confidence,
            });
        })
        .await
        .map_err(|e| e.to_string())
    }

    async fn process_source(&self, ctx: &SourceContext<'_>) -> SourceReport {
        match ctx.settings.modality {
            Modality::Text => self.process_text_source(ctx).await,
            Modality::Image => self.process_image_source(ctx).await,
        }
    }

    /// Shared start of every source: re-verify the checksum, derive the revision, and
    /// stop when that revision is already active and intact.
    async fn begin_source(&self, ctx: &SourceContext<'_>, mut report: SourceReport) -> Start {
        let entry = ctx.entry;
        // Re-verified per source (the file may change after the gate); reading and
        // hashing the file runs on the blocking pool.
        let (gate, gate_entry) = (ctx.manifest.clone(), entry.clone());
        let checksum = match tokio::task::spawn_blocking(move || gate.verify(&gate_entry)).await {
            Ok(Ok(checksum)) => checksum,
            Ok(Err(e)) => return Start::Done(report.fail(FailureStage::Manifest, e.to_string())),
            Err(e) => {
                return Start::Done(report.fail(
                    FailureStage::Manifest,
                    format!("verification task failed: {e}"),
                ));
            }
        };
        // The revision depends only on the file checksum and the extraction and chunking
        // (or image-preparation) fingerprints, so an unchanged source is recognised
        // before any extraction work.
        let revision = match RevisionId::derive(
            &checksum,
            ctx.extraction_fingerprint,
            ctx.chunking_fingerprint,
        ) {
            Ok(revision) => revision,
            Err(e) => return Start::Done(report.fail(FailureStage::Manifest, e.to_string())),
        };
        report.revision_id = Some(revision.clone());
        let space = self.embedder.model_info().embedding_space_id.clone();
        let (pointer_space, pointer_source) = (space, entry.source_id.clone());
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
                return Start::Done(report);
            }
            Ok(Some((active, false))) if active == revision => {
                // The pointer names this revision but its records or assets are missing
                // or altered: rebuild it. `publish` quarantines a damaged directory, and
                // an image run rewrites its content-addressed assets.
                warn!(source_id = %entry.source_id, "active revision is damaged; rebuilding");
            }
            Ok(_) => {}
            Err(e) => return Start::Done(report.fail(FailureStage::Storage, e.to_string())),
        }
        Start::Proceed {
            report,
            checksum,
            revision,
        }
    }

    async fn process_text_source(&self, ctx: &SourceContext<'_>) -> SourceReport {
        let entry = ctx.entry;
        let report = SourceReport::new(entry);
        if !entry.input_type.is_pdf() {
            let mut report = report;
            report.status = SourceStatus::NotApplicable;
            report.reason = Some("image input does not apply to a text run".to_owned());
            return report;
        }
        let (mut report, checksum, revision) = match self.begin_source(ctx, report).await {
            Start::Done(report) => return report,
            Start::Proceed {
                report,
                checksum,
                revision,
            } => (report, checksum, revision),
        };
        let space = self.embedder.model_info().embedding_space_id.clone();

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

        // Tokenizing every unit is CPU-bound: chunk on the blocking pool.
        let (chunker, embedder, budget) =
            (ctx.chunker.clone(), Arc::clone(&self.embedder), ctx.budget);
        let chunked = tokio::task::spawn_blocking(
            move || -> Result<Vec<Chunk>, crate::chunking::ChunkError> {
                let mut chunks = Vec::new();
                for page in &pages {
                    chunks.extend(chunker.chunk(page, embedder.token_counter(), budget)?);
                }
                Ok(chunks)
            },
        )
        .await;
        let chunks: Vec<Chunk> = match chunked {
            Ok(Ok(chunks)) => chunks,
            Ok(Err(e)) => return report.fail(FailureStage::Chunking, e.to_string()),
            Err(e) => {
                return report.fail(FailureStage::Chunking, format!("chunking task failed: {e}"));
            }
        };
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

        let vectors = match self
            .embed_all(Inputs::Text(&inputs), ctx.settings, &mut report)
            .await
        {
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
                    original_asset: None,
                    generated_caption: None,
                },
                embedding: RecordEmbedding::new(info, vector),
                experiment_id: ctx.settings.experiment_id.clone(),
            });
        }

        self.commit(space, entry.source_id.clone(), revision, records, report)
            .await
    }

    /// Stages and publishes the records of one source revision.
    async fn commit(
        &self,
        space: EmbeddingSpaceId,
        source_id: SourceId,
        revision: RevisionId,
        records: Vec<Record>,
        mut report: SourceReport,
    ) -> SourceReport {
        let store = Arc::clone(&self.store);
        let committed = source_id.clone();
        let stored = tokio::task::spawn_blocking(move || {
            let staged = store.stage(&space, &committed, &revision, &records)?;
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
                        info!(source_id = %source_id, records, "intact revision re-activated");
                    }
                }
                info!(source_id = %source_id, records = report.records_committed, "source committed");
                report
            }
            Ok(Err(e)) => report.fail(FailureStage::Storage, e.to_string()),
            Err(e) => report.fail(FailureStage::Storage, format!("storage task failed: {e}")),
        }
    }

    /// One adapter call: documents for a text run, images for an image run.
    async fn call(&self, inputs: Inputs<'_>) -> Result<EmbeddingBatch, EmbeddingError> {
        match inputs {
            Inputs::Text(inputs) => self.embedder.embed_documents(inputs).await,
            Inputs::Images(inputs) => self.embedder.embed_images(inputs).await,
        }
    }

    /// Collects the images of one source: the file itself for PNG/JPEG, otherwise every
    /// listed image the filter keeps on a page whose text layer is usable (on a page that
    /// needs OCR the raster is the page scan), together with its mask when it has one.
    /// Each image is masked and re-encoded without metadata on the blocking pool.
    async fn source_images(
        &self,
        ctx: &SourceContext<'_>,
    ) -> Result<Vec<SourceImage>, (FailureStage, String)> {
        let path = ctx.manifest.path_of(ctx.entry);
        let extraction =
            |e: crate::extraction::ExtractionError| (FailureStage::Extraction, e.to_string());
        let mut raw: Vec<RawImage> = Vec::new();
        if ctx.entry.input_type.is_pdf() {
            let listed = self.images.list_images(&path).await.map_err(extraction)?;
            // Whether each inspected page has a usable text layer.
            let mut usable_text = std::collections::BTreeMap::new();
            for image in listed.iter().filter(|i| ctx.settings.images.keeps(i)) {
                let usable = match usable_text.get(&image.page) {
                    Some(usable) => *usable,
                    None => {
                        let native = self
                            .text
                            .extract_page(&path, image.page)
                            .await
                            .map_err(extraction)?;
                        let usable = ctx
                            .settings
                            .extraction
                            .native_text_is_usable(&native.text());
                        usable_text.insert(image.page, usable);
                        usable
                    }
                };
                if !usable {
                    continue;
                }
                let bytes = self
                    .images
                    .extract_image(&path, image)
                    .await
                    .map_err(extraction)?;
                let mask = match mask_of(&listed, image) {
                    Some(mask) => Some(
                        self.images
                            .extract_image(&path, mask)
                            .await
                            .map_err(extraction)?,
                    ),
                    None => None,
                };
                let index = u32::try_from(raw.iter().filter(|r| r.page == image.page).count())
                    .map_err(|_| {
                        (
                            FailureStage::Extraction,
                            "too many images on one page".to_owned(),
                        )
                    })?;
                raw.push(RawImage {
                    page: image.page,
                    index,
                    pdf_image_num: Some(image.num),
                    method: ExtractionMethod::EmbeddedImage,
                    bytes,
                    mask,
                });
            }
        } else {
            let bytes = tokio::fs::read(&path)
                .await
                .map_err(|e| (FailureStage::Extraction, format!("cannot read image: {e}")))?;
            raw.push(RawImage {
                page: NonZeroU32::MIN,
                index: 0,
                pdf_image_num: None,
                method: ExtractionMethod::ImageFile,
                bytes,
                mask: None,
            });
        }
        tokio::task::spawn_blocking(move || {
            raw.into_iter()
                .map(|image| {
                    let prepared = MetadataScrubber
                        .scrub_masked(&image.bytes, image.mask.as_deref())
                        .map_err(|e| (FailureStage::Extraction, e.to_string()))?;
                    Ok(SourceImage {
                        page: image.page,
                        index: image.index,
                        pdf_image_num: image.pdf_image_num,
                        method: image.method,
                        prepared,
                    })
                })
                .collect()
        })
        .await
        .map_err(|e| {
            (
                FailureStage::Extraction,
                format!("image preparation task failed: {e}"),
            )
        })?
    }

    async fn process_image_source(&self, ctx: &SourceContext<'_>) -> SourceReport {
        let entry = ctx.entry;
        let (mut report, checksum, revision) =
            match self.begin_source(ctx, SourceReport::new(entry)).await {
                Start::Done(report) => return report,
                Start::Proceed {
                    report,
                    checksum,
                    revision,
                } => (report, checksum, revision),
            };
        let images = match self.source_images(ctx).await {
            Ok(images) => images,
            Err((stage, reason)) => return report.fail(stage, reason),
        };
        if images.is_empty() {
            // Nothing to publish. A previously active revision of this source (other
            // bytes, filter, or preparation) must stop answering searches, so its pointer
            // is removed; no new revision exists.
            let space = self.embedder.model_info().embedding_space_id.clone();
            let source = entry.source_id.clone();
            let deactivated = match self.on_store(move |s| s.deactivate(&space, &source)).await {
                Ok(deactivated) => deactivated,
                Err(e) => return report.fail(FailureStage::Storage, e.to_string()),
            };
            report.revision_id = None;
            report.status = SourceStatus::NotApplicable;
            report.reason = Some(format!(
                "no image qualifies for an image run (type image, both sides at least {} px, on a page with a usable text layer){}",
                ctx.settings.images.min_side_px,
                if deactivated {
                    "; the previously active revision was deactivated"
                } else {
                    ""
                }
            ));
            return report;
        }
        // Assets are content-addressed: storing them before the revision is harmless
        // even if the source fails later.
        let pngs: Vec<Vec<u8>> = images.iter().map(|i| i.prepared.png.clone()).collect();
        let stored = self
            .on_store(move |s| {
                pngs.iter()
                    .map(|png| {
                        let uri = s.put_asset(png)?;
                        let path = s.asset_path(&uri)?;
                        Ok((uri, path))
                    })
                    .collect::<Result<Vec<_>, StoreError>>()
            })
            .await;
        let assets = match stored {
            Ok(assets) => assets,
            Err(e) => return report.fail(FailureStage::Storage, e.to_string()),
        };
        let mut chunk_ids = Vec::with_capacity(images.len());
        for image in &images {
            match ChunkId::new(&entry.source_id, image.page, Modality::Image, image.index) {
                Ok(id) => chunk_ids.push(id),
                Err(e) => return report.fail(FailureStage::Extraction, e.to_string()),
            }
        }
        let inputs: Vec<AssetRef> = chunk_ids
            .iter()
            .zip(&assets)
            .map(|(id, (_, path))| AssetRef {
                id: ItemId::from(id),
                path: path.clone(),
            })
            .collect();
        let vectors = match self
            .embed_all(Inputs::Images(&inputs), ctx.settings, &mut report)
            .await
        {
            Ok(vectors) => vectors,
            Err((stage, reason)) => return report.fail(stage, reason),
        };
        let info = self.embedder.model_info();
        let space = info.embedding_space_id.clone();
        let engine = if entry.input_type.is_pdf() {
            self.images.engine_id()
        } else {
            MetadataScrubber.id().to_owned()
        };
        let mut records = Vec::with_capacity(images.len());
        for (((image, chunk_id), (uri, _)), vector) in
            images.into_iter().zip(chunk_ids).zip(assets).zip(vectors)
        {
            let record_id = match RecordId::derive(&revision, &chunk_id, &space) {
                Ok(id) => id,
                Err(e) => return report.fail(FailureStage::Storage, e.to_string()),
            };
            records.push(Record {
                record_id,
                chunk_id,
                source_id: entry.source_id.clone(),
                revision_id: revision.clone(),
                chunk_text: None,
                modality: Modality::Image,
                asset_uri: Some(uri),
                metadata: RecordMetadata {
                    source_file: entry.file.display().to_string(),
                    source_checksum: checksum.clone(),
                    document_type: entry.document_type,
                    language: IMAGE_LANGUAGE.to_owned(),
                    page_number: image.page.get(),
                    section: None,
                    chunk_index: image.index,
                    chunk_kind: ChunkKind::Image,
                    bounding_box: None,
                    extraction_method: image.method,
                    extraction_engine: engine.clone(),
                    extraction_confidence: None,
                    chunking_strategy: MetadataScrubber.id().to_owned(),
                    overlap_tokens: 0,
                    truncation: None,
                    anonymization_status: "verified".to_owned(),
                    original_asset: Some(OriginalAsset {
                        file: entry.file.display().to_string(),
                        page: image.page.get(),
                        pdf_image_num: image.pdf_image_num,
                        width: image.prepared.width,
                        height: image.prepared.height,
                    }),
                    generated_caption: None,
                },
                embedding: RecordEmbedding::new(info, vector),
                experiment_id: ctx.settings.experiment_id.clone(),
            });
        }
        self.commit(space, entry.source_id.clone(), revision, records, report)
            .await
    }

    /// Embeds `inputs` in batches with class-based retries, validating every batch.
    async fn embed_all(
        &self,
        inputs: Inputs<'_>,
        settings: &IngestSettings,
        report: &mut SourceReport,
    ) -> Result<Vec<Vec<f32>>, (FailureStage, String)> {
        let caps = self.embedder.capabilities();
        let batch = caps
            .batch_limit()
            .map_or(settings.batch_size.get(), |max| {
                max.min(settings.batch_size.get())
            })
            .max(1);
        let mut vectors = Vec::with_capacity(inputs.len());
        for start in (0..inputs.len()).step_by(batch) {
            let batch_inputs = inputs.slice(start, (start + batch).min(inputs.len()));
            let ids = batch_inputs.ids();
            let mut attempt: u32 = 0;
            let embedded = loop {
                attempt += 1;
                report.attempts += 1;
                match self.call(batch_inputs).await {
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

impl Inputs<'_> {
    fn len(&self) -> usize {
        match self {
            Self::Text(inputs) => inputs.len(),
            Self::Images(inputs) => inputs.len(),
        }
    }

    fn slice(&self, start: usize, end: usize) -> Self {
        match self {
            Self::Text(inputs) => Self::Text(inputs.get(start..end).unwrap_or_default()),
            Self::Images(inputs) => Self::Images(inputs.get(start..end).unwrap_or_default()),
        }
    }

    fn ids(&self) -> Vec<ItemId> {
        match self {
            Self::Text(inputs) => inputs.iter().map(|i| i.id.clone()).collect(),
            Self::Images(inputs) => inputs.iter().map(|i| i.id.clone()).collect(),
        }
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
