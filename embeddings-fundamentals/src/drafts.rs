//! Drafts of the owner inputs for a new corpus (`TESTS.md`): a manifest skeleton with
//! measured checksums and guessed input types, and a relevance-label skeleton pooled
//! from the top results of indexed runs.
//!
//! A draft never records a review or a judgment. Its sources carry no anonymization
//! review, so the gate refuses every one of them, and its labels hold `"TODO"`, which
//! does not parse as a label, until the owner fills them in.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::Arc;

use crate::anonymization::{self, Finding};
use crate::domain::{InputType, Modality};
use crate::embedding::EmbeddingStrategy;
use crate::evaluation::QuerySet;
use crate::extraction::images::{ImageExtractor, ImageFilter};
use crate::extraction::{ExtractionConfig, ExtractionError, TextExtractor};
use crate::ids::{Checksum, ChunkId, EmbeddingSpaceId, ExperimentId, IdError, SourceId};
use crate::retrieval::{Scope, SearchError, SearchHit, search_images, search_text};
use crate::storage::{FsRecordStore, StoreError};

/// Longest text anchor a label draft proposes, in characters.
const ANCHOR_CHARS: usize = 80;
/// Longest chunk text a label draft shows in a comment, in characters.
const SHOWN_CHARS: usize = 2000;

/// Errors raised while drafting.
#[derive(Debug, thiserror::Error)]
pub enum DraftError {
    /// A corpus file or directory could not be read.
    #[error("cannot read {path}: {source}")]
    Read {
        /// Path.
        path: String,
        /// I/O error.
        #[source]
        source: std::io::Error,
    },
    /// A path is not valid UTF-8, so a manifest cannot name it.
    #[error("a path under {0} is not valid UTF-8")]
    NotUtf8(String),
    /// The directory holds no PDF, PNG, or JPEG file.
    #[error("no PDF, PNG, or JPEG file under {0}")]
    Empty(String),
    /// A listed file is not a PDF, PNG, or JPEG file.
    #[error("source {index} (in path order) is not a PDF, PNG, or JPEG file")]
    Unsupported {
        /// 1-based position of the source in path order.
        index: usize,
    },
    /// A source id could not be formed.
    #[error(transparent)]
    Id(#[from] IdError),
    /// Extraction of a source failed. Sources are named by position, never by file name.
    #[error("source {index} (in path order): {source}")]
    Extraction {
        /// 1-based position of the source in path order.
        index: usize,
        /// Extraction error.
        #[source]
        source: ExtractionError,
    },
    /// A query could not be run.
    #[error(transparent)]
    Search(#[from] SearchError),
    /// A stored asset could not be read.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// A blocking task failed.
    #[error("blocking task failed: {0}")]
    Join(String),
}

/// What extraction measured on a PDF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PdfSummary {
    /// Pages in the document.
    pub pages: u32,
    /// Pages whose text layer passes the native-text rule of the extraction settings.
    pub native_pages: u32,
    /// Embedded images the image filter keeps.
    pub kept_images: usize,
}

impl PdfSummary {
    /// The input type the measurements suggest: no usable text layer is a scan, every
    /// page native without kept images is searchable, anything else is mixed.
    #[must_use]
    pub const fn input_type(&self) -> InputType {
        if self.native_pages == 0 {
            InputType::ScannedPdf
        } else if self.native_pages == self.pages && self.kept_images == 0 {
            InputType::SearchablePdf
        } else {
            InputType::MixedPdf
        }
    }
}

/// One drafted manifest entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceDraft {
    /// File path relative to the corpus directory, with `/` separators.
    pub file: String,
    /// Proposed source id: the file stem when it is a clean slug, else `source-NNN`.
    pub source_id: SourceId,
    /// Checksum of the file bytes.
    pub checksum: Checksum,
    /// Guessed input type.
    pub input_type: InputType,
    /// PDF measurements; `None` for image files.
    pub pdf: Option<PdfSummary>,
    /// Filename and embedded-metadata findings (keys and patterns, never values).
    pub findings: Vec<Finding>,
}

impl SourceDraft {
    /// True when the file name itself was flagged for review.
    #[must_use]
    pub fn name_flagged(&self) -> bool {
        self.findings.iter().any(|f| f.area == "filename")
    }
}

/// A drafted corpus manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestDraft {
    /// Corpus identifier.
    pub corpus_id: String,
    /// Sources in path order.
    pub sources: Vec<SourceDraft>,
}

/// Lists the PDF, PNG, and JPEG files under `dir` (recursively, hidden entries and
/// symbolic links skipped), as paths relative to `dir` in path order.
///
/// # Errors
///
/// Returns [`DraftError::Read`] when a directory cannot be read,
/// [`DraftError::NotUtf8`] for a path a manifest cannot name, and
/// [`DraftError::Empty`] when no file qualifies.
pub fn corpus_files(dir: &Path) -> Result<Vec<String>, DraftError> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), DraftError> {
        let read = |source| DraftError::Read {
            path: dir.display().to_string(),
            source,
        };
        for entry in std::fs::read_dir(dir).map_err(read)? {
            let entry = entry.map_err(read)?;
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
            let kind = entry.file_type().map_err(read)?;
            let path = entry.path();
            if kind.is_dir() {
                walk(root, &path, out)?;
            } else if kind.is_file() && file_kind(&path).is_some() {
                let relative = path
                    .strip_prefix(root)
                    .ok()
                    .and_then(Path::to_str)
                    .ok_or_else(|| DraftError::NotUtf8(root.display().to_string()))?;
                out.push(relative.replace(std::path::MAIN_SEPARATOR, "/"));
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(dir, dir, &mut files)?;
    if files.is_empty() {
        return Err(DraftError::Empty(dir.display().to_string()));
    }
    files.sort();
    Ok(files)
}

/// What an extension names: a PDF (whose input type is measured) or an image file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileKind {
    Pdf,
    Image(InputType),
}

fn file_kind(path: &Path) -> Option<FileKind> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "pdf" => Some(FileKind::Pdf),
        "png" => Some(FileKind::Image(InputType::Png)),
        "jpg" | "jpeg" => Some(FileKind::Image(InputType::Jpeg)),
        _ => None,
    }
}

/// Runs synchronous file work on the blocking pool.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, DraftError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| DraftError::Join(e.to_string()))
}

/// Measures every file of `files` (relative to `dir`, from [`corpus_files`]) and
/// drafts its manifest entry: checksum, a source id, the input type that the
/// native-text rule of `extraction` and the image `filter` suggest, and the
/// filename and metadata findings of the anonymization check.
///
/// # Errors
///
/// Returns [`DraftError::Read`] when a file cannot be read and
/// [`DraftError::Extraction`] when a PDF cannot be measured.
pub async fn draft_manifest<T: TextExtractor, I: ImageExtractor>(
    text: &T,
    images: &I,
    dir: &Path,
    files: &[String],
    extraction: &ExtractionConfig,
    filter: &ImageFilter,
    corpus_id: &str,
) -> Result<ManifestDraft, DraftError> {
    let mut sources = Vec::with_capacity(files.len());
    let mut taken = BTreeSet::new();
    for (i, file) in files.iter().enumerate() {
        let index = i + 1;
        let path = dir.join(file);
        let bytes = tokio::fs::read(&path)
            .await
            .map_err(|source| DraftError::Read {
                path: format!("source {index} (in path order)"),
                source,
            })?;
        let checksum = Checksum::of_bytes(&bytes);
        let kind = file_kind(&path).ok_or(DraftError::Unsupported { index })?;
        let (input_type, pdf) = match kind {
            FileKind::Pdf => {
                let summary = measure_pdf(text, images, &path, extraction, filter)
                    .await
                    .map_err(|source| DraftError::Extraction { index, source })?;
                (summary.input_type(), Some(summary))
            }
            FileKind::Image(input_type) => (input_type, None),
        };
        let scan = path.clone();
        let mut findings = anonymization::filename_findings(Path::new(file));
        findings.extend(
            blocking(move || match kind {
                FileKind::Pdf => anonymization::pdf_findings(&scan),
                FileKind::Image(_) => anonymization::image_findings(&scan),
            })
            .await?,
        );
        let flagged = findings.iter().any(|f| f.area == "filename");
        let source_id = source_id_for(file, index, flagged, &mut taken)?;
        sources.push(SourceDraft {
            file: file.clone(),
            source_id,
            checksum,
            input_type,
            pdf,
            findings,
        });
    }
    Ok(ManifestDraft {
        corpus_id: corpus_id.to_owned(),
        sources,
    })
}

async fn measure_pdf<T: TextExtractor, I: ImageExtractor>(
    text: &T,
    images: &I,
    pdf: &Path,
    extraction: &ExtractionConfig,
    filter: &ImageFilter,
) -> Result<PdfSummary, ExtractionError> {
    let pages = text.page_count(pdf).await?;
    let mut native_pages = 0;
    for n in 1..=pages.get() {
        let page = NonZeroU32::new(n).unwrap_or(NonZeroU32::MIN);
        if extraction.native_text_is_usable(&text.extract_page(pdf, page).await?.text()) {
            native_pages += 1;
        }
    }
    let kept_images = images
        .list_images(pdf)
        .await?
        .iter()
        .filter(|info| filter.keeps(info))
        .count();
    Ok(PdfSummary {
        pages: pages.get(),
        native_pages,
        kept_images,
    })
}

/// The file stem when it is a clean, unflagged slug not yet taken; `source-NNN`
/// otherwise, so a flagged name never becomes an identifier.
fn source_id_for(
    file: &str,
    index: usize,
    flagged: bool,
    taken: &mut BTreeSet<SourceId>,
) -> Result<SourceId, IdError> {
    let stem = Path::new(file)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|_| !flagged);
    if let Some(id) = stem.and_then(|s| s.parse::<SourceId>().ok())
        && taken.insert(id.clone())
    {
        return Ok(id);
    }
    let mut suffix = 0_usize;
    loop {
        let candidate = if suffix == 0 {
            format!("source-{index:03}")
        } else {
            format!("source-{index:03}-{suffix}")
        };
        let id: SourceId = candidate.parse()?;
        if taken.insert(id.clone()) {
            return Ok(id);
        }
        suffix += 1;
    }
}

fn input_type_name(input: InputType) -> &'static str {
    match input {
        InputType::SearchablePdf => "searchable_pdf",
        InputType::ScannedPdf => "scanned_pdf",
        InputType::MixedPdf => "mixed_pdf",
        InputType::Png => "png",
        InputType::Jpeg => "jpeg",
    }
}

/// A TOML basic string.
fn toml_string(s: &str) -> String {
    toml::Value::String(s.to_owned()).to_string()
}

/// One comment line: control characters (forbidden in TOML comments) become spaces.
fn comment_text(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() && c != '\t' { ' ' } else { c })
        .collect()
}

impl ManifestDraft {
    /// Renders the draft as a `manifest.toml` that parses but whose sources the
    /// anonymization gate refuses until each review line is filled in.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "# DRAFT corpus manifest, written by `embeddings-fundamentals draft-manifest`.\n\
             # Checksums, page counts, and image counts are measured. `input_type` and\n\
             # `document_type` are guesses: confirm each one. No source has an anonymization\n\
             # review, so the gate refuses every source until you review it (TESTS.md) and\n\
             # replace its `# anonymization = ...` line with the real result.\n\
             # This file is tracked by git: file names and comments must hold no identifier."
        );
        let _ = writeln!(out, "corpus_id = {}", toml_string(&self.corpus_id));
        let _ = writeln!(out, "remote_allowed = false");
        for (i, s) in self.sources.iter().enumerate() {
            let _ = writeln!(out);
            match s.pdf {
                Some(p) => {
                    let _ = writeln!(
                        out,
                        "# {} of {}. PDF: {} page(s), {} with a usable text layer, {} embedded image(s) kept by the image filter.",
                        i + 1,
                        self.sources.len(),
                        p.pages,
                        p.native_pages,
                        p.kept_images
                    );
                }
                None => {
                    let _ = writeln!(out, "# {} of {}. Image file.", i + 1, self.sources.len());
                }
            }
            if s.findings.is_empty() {
                let _ = writeln!(
                    out,
                    "# anonymization-check: no filename or metadata finding."
                );
            } else {
                let _ = writeln!(
                    out,
                    "# anonymization-check: {} finding(s) to review:",
                    s.findings.len()
                );
                for f in &s.findings {
                    let _ = writeln!(out, "#   {}: {}", f.area, comment_text(&f.detail));
                }
            }
            let document_type = if s.pdf.is_some() {
                "laboratory_report"
            } else {
                "radiograph"
            };
            let _ = writeln!(out, "[[sources]]");
            let _ = writeln!(out, "source_id = {}", toml_string(s.source_id.as_str()));
            let _ = writeln!(out, "file = {}", toml_string(&s.file));
            let _ = writeln!(out, "checksum = {}", toml_string(&s.checksum.to_string()));
            let _ = writeln!(
                out,
                "document_type = \"{document_type}\"  # guess: laboratory_report | imaging_report | radiograph | ultrasound_image"
            );
            let _ = writeln!(
                out,
                "input_type = \"{}\"  # measured guess: searchable_pdf | scanned_pdf | mixed_pdf | png | jpeg",
                input_type_name(s.input_type)
            );
            let _ = writeln!(out, "eligible = true");
            let _ = writeln!(out, "required = true");
            let _ = writeln!(
                out,
                "# anonymization = {{ filename = \"ok\", metadata = \"ok\", text = \"ok\", pixels = \"ok\", reviewer = \"<name>\", date = \"<YYYY-MM-DD>\" }}"
            );
        }
        out
    }
}

/// One run whose top results feed the label pool.
#[derive(Clone, Copy)]
pub struct PoolRun<'a> {
    /// Experiment the run indexed; its snapshot is the scope.
    pub experiment: &'a ExperimentId,
    /// Records the run embedded (text chunks or images).
    pub modality: Modality,
    /// Index space of the run.
    pub space: &'a EmbeddingSpaceId,
    /// Query model of the run.
    pub embedder: &'a dyn EmbeddingStrategy,
    /// Revisions to search.
    pub scope: Scope<'a>,
}

/// One pooled item for one query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// Chunk id (the label item).
    pub chunk_id: ChunkId,
    /// Text chunk or image.
    pub modality: Modality,
    /// Page of the item.
    pub page_number: u32,
    /// Chunk text (text items).
    pub text: Option<String>,
    /// Original-image reference (image items): the label anchor.
    pub original: Option<String>,
    /// Stored asset URI (image items).
    pub asset_uri: Option<String>,
    /// Checksum of the stored asset (image items).
    pub asset_checksum: Option<Checksum>,
    /// Runs that returned the item, with its rank in each.
    pub found_by: Vec<(ExperimentId, usize)>,
    /// Runs that returned other content under the same chunk id; a label written for
    /// this candidate will not apply to them.
    pub conflicts: Vec<ExperimentId>,
}

impl Candidate {
    fn best_rank(&self) -> usize {
        self.found_by
            .iter()
            .map(|(_, r)| *r)
            .min()
            .unwrap_or(usize::MAX)
    }

    fn same_content(&self, other: &Self) -> bool {
        self.text == other.text
            && self.original == other.original
            && self.asset_checksum == other.asset_checksum
    }
}

/// The candidates of one query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PooledQuery {
    /// Query id.
    pub id: String,
    /// Query text.
    pub text: String,
    /// What the query is meant to find.
    pub target: String,
    /// Candidates, best rank first, then by chunk id.
    pub candidates: Vec<Candidate>,
}

/// A drafted relevance-label file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelDraft {
    /// Query-set version the labels belong to.
    pub query_set: String,
    /// Corpus the labels describe.
    pub corpus: String,
    /// Depth pooled from each run.
    pub depth: usize,
    /// Runs pooled, with their modality.
    pub runs: Vec<(ExperimentId, Modality)>,
    /// Candidates per query, in query-set order.
    pub queries: Vec<PooledQuery>,
}

/// Pools the top `depth` results of every run for every query of `queries`: each
/// distinct chunk id becomes one candidate, with the runs and ranks that returned it.
/// Image candidates carry the checksum of their stored asset, read from `store`.
///
/// # Errors
///
/// Returns [`DraftError::Search`] when a run cannot be searched (for example an image
/// run whose model has no text-to-image space) and [`DraftError::Store`] when an
/// asset cannot be read.
pub async fn draft_labels(
    store: &Arc<FsRecordStore>,
    queries: &QuerySet,
    runs: &[PoolRun<'_>],
    depth: usize,
    corpus: &str,
) -> Result<LabelDraft, DraftError> {
    let mut checksums: BTreeMap<String, Checksum> = BTreeMap::new();
    let mut pooled = Vec::with_capacity(queries.queries.len());
    for query in &queries.queries {
        let mut by_item: BTreeMap<ChunkId, Candidate> = BTreeMap::new();
        for run in runs {
            let hits = match run.modality {
                Modality::Text => {
                    search_text(
                        store,
                        run.space,
                        run.embedder,
                        &query.text,
                        depth,
                        run.scope,
                    )
                    .await?
                }
                Modality::Image => {
                    search_images(
                        store,
                        run.space,
                        run.embedder,
                        &query.text,
                        depth,
                        run.scope,
                    )
                    .await?
                }
            };
            for hit in hits {
                let candidate = candidate(store, &mut checksums, run.experiment, hit).await?;
                match by_item.get_mut(&candidate.chunk_id) {
                    None => {
                        by_item.insert(candidate.chunk_id.clone(), candidate);
                    }
                    Some(known) if known.same_content(&candidate) => {
                        known.found_by.extend(candidate.found_by);
                    }
                    Some(known) => known.conflicts.push(run.experiment.clone()),
                }
            }
        }
        let mut candidates: Vec<Candidate> = by_item.into_values().collect();
        candidates.sort_by(|a, b| {
            a.best_rank()
                .cmp(&b.best_rank())
                .then_with(|| a.chunk_id.cmp(&b.chunk_id))
        });
        pooled.push(PooledQuery {
            id: query.id.to_string(),
            text: query.text.clone(),
            target: query.target.clone(),
            candidates,
        });
    }
    Ok(LabelDraft {
        query_set: queries.version.clone(),
        corpus: corpus.to_owned(),
        depth,
        runs: runs
            .iter()
            .map(|r| (r.experiment.clone(), r.modality))
            .collect(),
        queries: pooled,
    })
}

async fn candidate(
    store: &Arc<FsRecordStore>,
    checksums: &mut BTreeMap<String, Checksum>,
    run: &ExperimentId,
    hit: SearchHit,
) -> Result<Candidate, DraftError> {
    let asset_checksum = match hit.asset_uri.as_deref() {
        Some(uri) if hit.modality == Modality::Image => {
            if let Some(known) = checksums.get(uri) {
                Some(known.clone())
            } else {
                let (owned, reader) = (uri.to_owned(), Arc::clone(store));
                let bytes = blocking(move || reader.read_asset(&owned)).await??;
                let checksum = Checksum::of_bytes(&bytes);
                checksums.insert(uri.to_owned(), checksum.clone());
                Some(checksum)
            }
        }
        _ => None,
    };
    Ok(Candidate {
        chunk_id: hit.chunk_id,
        modality: hit.modality,
        page_number: hit.page_number,
        text: hit.chunk_text,
        original: hit.original,
        asset_uri: hit.asset_uri,
        asset_checksum,
        found_by: vec![(run.clone(), hit.rank)],
        conflicts: Vec::new(),
    })
}

/// Collapses whitespace, the form the evaluator compares anchors in (besides NFC and
/// case).
fn collapsed(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Proposes a text anchor: the longest line of `text` that no other pooled chunk
/// shares (`shared` counts the chunks holding each collapsed line), else the longest
/// line, collapsed and cut to whole words within [`ANCHOR_CHARS`] characters.
fn propose_anchor(text: &str, shared: &BTreeMap<String, usize>) -> Option<String> {
    let lines: Vec<String> = text
        .lines()
        .map(collapsed)
        .filter(|l| !l.is_empty())
        .collect();
    let longest = |unique_only: bool| {
        lines
            .iter()
            .filter(|l| !unique_only || shared.get(*l).copied().unwrap_or(0) <= 1)
            .max_by(|a, b| {
                a.chars()
                    .count()
                    .cmp(&b.chars().count())
                    .then_with(|| b.cmp(a))
            })
    };
    let line = longest(true).or_else(|| longest(false))?;
    let mut anchor = String::new();
    for word in line.split(' ') {
        let len = anchor.chars().count() + usize::from(!anchor.is_empty()) + word.chars().count();
        if len > ANCHOR_CHARS {
            break;
        }
        if !anchor.is_empty() {
            anchor.push(' ');
        }
        anchor.push_str(word);
    }
    if anchor.is_empty() {
        anchor = line.chars().take(ANCHOR_CHARS).collect();
    }
    Some(anchor)
}

impl LabelDraft {
    /// Renders the draft as a label file that does not load until every `"TODO"`
    /// judgment is replaced by 0, 1, or 2.
    #[must_use]
    pub fn render(&self) -> String {
        let mut shared: BTreeMap<String, usize> = BTreeMap::new();
        let mut seen = BTreeSet::new();
        for c in self.queries.iter().flat_map(|q| &q.candidates) {
            if let Some(text) = c.text.as_deref()
                && seen.insert(&c.chunk_id)
            {
                let lines: BTreeSet<String> = text.lines().map(collapsed).collect();
                for line in lines {
                    *shared.entry(line).or_default() += 1;
                }
            }
        }
        let mut out = String::new();
        let runs: Vec<String> = self
            .runs
            .iter()
            .map(|(id, m)| format!("{id} ({})", modality_name(*m)))
            .collect();
        let _ = writeln!(
            out,
            "# DRAFT relevance labels, written by `embeddings-fundamentals draft-labels`.\n\
             # Candidates: the top {} results of each run for every query (pooling).\n\
             # Runs: {}.\n\
             #\n\
             # Judge every candidate: replace \"TODO\" with 0 (irrelevant), 1 (partially\n\
             # relevant), or 2 (directly relevant), and write the note. The file does not\n\
             # load until every \"TODO\" is gone. You may delete a candidate you judge 0:\n\
             # items not listed count as irrelevant.\n\
             # Check each text `anchor`: a distinctive substring of the chunk text. The\n\
             # evaluator refuses a label whose chunk no longer contains its anchor.\n\
             #\n\
             # This file holds document text: keep it out of version control.",
            self.depth,
            comment_text(&runs.join(", "))
        );
        let _ = writeln!(
            out,
            "version = {}",
            toml_string(&format!("labels-{}-v1", self.corpus))
        );
        let _ = writeln!(out, "query_set = {}", toml_string(&self.query_set));
        let _ = writeln!(out, "corpus = {}", toml_string(&self.corpus));
        for q in &self.queries {
            let _ = writeln!(
                out,
                "\n# ==== {}: {} (target: {})",
                q.id,
                comment_text(&q.text),
                comment_text(&q.target)
            );
            if q.candidates.is_empty() {
                let _ = writeln!(out, "# No run returned a candidate for this query.");
            }
            for (i, c) in q.candidates.iter().enumerate() {
                render_candidate(&mut out, &q.id, i + 1, c, &shared);
            }
        }
        out
    }
}

fn modality_name(m: Modality) -> &'static str {
    match m {
        Modality::Text => "text",
        Modality::Image => "image",
    }
}

fn render_candidate(
    out: &mut String,
    query: &str,
    n: usize,
    c: &Candidate,
    shared: &BTreeMap<String, usize>,
) {
    let found: Vec<String> = c
        .found_by
        .iter()
        .map(|(run, rank)| format!("{run} at rank {rank}"))
        .collect();
    let _ = writeln!(
        out,
        "\n# {query} candidate {n}: {} {}, page {}, found by {}",
        modality_name(c.modality),
        c.chunk_id,
        c.page_number,
        found.join(", ")
    );
    if !c.conflicts.is_empty() {
        let runs: Vec<String> = c.conflicts.iter().map(ToString::to_string).collect();
        let _ = writeln!(
            out,
            "# WARNING: {} returned other content under this chunk id; this label will not apply to that run.",
            runs.join(", ")
        );
    }
    match c.modality {
        Modality::Text => {
            let text = c.text.as_deref().unwrap_or_default();
            let mut shown: String = text.chars().take(SHOWN_CHARS).collect();
            if text.chars().count() > SHOWN_CHARS {
                shown.push_str(" [...]");
            }
            for line in shown.lines() {
                let _ = writeln!(out, "#   | {}", comment_text(line));
            }
        }
        Modality::Image => {
            if let Some(uri) = c.asset_uri.as_deref() {
                let _ = writeln!(out, "#   view the stored image: <store>/{uri}");
            }
        }
    }
    let _ = writeln!(out, "[[label]]");
    let _ = writeln!(out, "query = {}", toml_string(query));
    let _ = writeln!(out, "item = {}", toml_string(c.chunk_id.as_str()));
    let anchor = match c.modality {
        Modality::Text => c.text.as_deref().and_then(|t| propose_anchor(t, shared)),
        Modality::Image => c.original.clone(),
    };
    match anchor {
        Some(anchor) => {
            let _ = writeln!(out, "anchor = {}", toml_string(&anchor));
        }
        None => {
            let _ = writeln!(out, "anchor = \"TODO\"  # no text to anchor; write one");
        }
    }
    if let Some(checksum) = &c.asset_checksum {
        let _ = writeln!(
            out,
            "asset_checksum = {}",
            toml_string(&checksum.to_string())
        );
    }
    let _ = writeln!(out, "label = \"TODO\"  # 0, 1, or 2");
    let _ = writeln!(out, "note = \"TODO\"");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evaluation::anchor_form;

    #[test]
    fn a_pdf_summary_names_its_input_type() {
        let s = |pages, native_pages, kept_images| PdfSummary {
            pages,
            native_pages,
            kept_images,
        };
        assert_eq!(s(2, 0, 2).input_type(), InputType::ScannedPdf);
        assert_eq!(s(2, 2, 0).input_type(), InputType::SearchablePdf);
        assert_eq!(s(2, 2, 1).input_type(), InputType::MixedPdf);
        assert_eq!(s(2, 1, 1).input_type(), InputType::MixedPdf);
    }

    #[test]
    fn a_flagged_or_taken_name_never_becomes_the_source_id() {
        let mut taken = BTreeSet::new();
        assert_eq!(
            source_id_for("lab-001.pdf", 1, false, &mut taken)
                .unwrap()
                .as_str(),
            "lab-001"
        );
        assert_eq!(
            source_id_for("sub/lab-001.PDF", 2, false, &mut taken)
                .unwrap()
                .as_str(),
            "source-002"
        );
        assert_eq!(
            source_id_for("JOAO DA SILVA.pdf", 3, true, &mut taken)
                .unwrap()
                .as_str(),
            "source-003"
        );
        assert_eq!(
            source_id_for("exame 4.pdf", 4, false, &mut taken)
                .unwrap()
                .as_str(),
            "source-004"
        );
    }

    #[test]
    fn the_anchor_prefers_a_line_no_other_chunk_shares() {
        let text = "Exame   Resultado   Unidade\nHemoglobina  13,5  g/dL";
        let mut shared = BTreeMap::new();
        shared.insert("Exame Resultado Unidade".to_owned(), 3);
        shared.insert("Hemoglobina 13,5 g/dL".to_owned(), 1);
        assert_eq!(
            propose_anchor(text, &shared).as_deref(),
            Some("Hemoglobina 13,5 g/dL")
        );
        // With every line shared, the longest one is still a valid anchor.
        shared.insert("Hemoglobina 13,5 g/dL".to_owned(), 2);
        assert_eq!(
            propose_anchor(text, &shared).as_deref(),
            Some("Exame Resultado Unidade")
        );
        assert_eq!(propose_anchor(" \n\t", &shared), None);
    }

    proptest::proptest! {
        #[test]
        fn a_proposed_anchor_is_always_found_by_the_evaluator(
            text in "[a-zA-Zçãé0-9 ,.|\\t\\n]{1,400}"
        ) {
            if let Some(anchor) = propose_anchor(&text, &BTreeMap::new()) {
                proptest::prop_assert!(anchor.chars().count() <= ANCHOR_CHARS);
                proptest::prop_assert!(!anchor.trim().is_empty());
                proptest::prop_assert!(anchor_form(&text).contains(&anchor_form(&anchor)));
            }
        }
    }

    #[test]
    fn comments_never_carry_control_characters() {
        assert_eq!(comment_text("a\rb\u{7}c\td"), "a b c\td");
    }
}
