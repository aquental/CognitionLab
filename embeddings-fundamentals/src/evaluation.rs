//! Retrieval evaluation against a frozen query set and frozen relevance labels
//! (PRD §5.3, §9).
//!
//! Ranks start at 1, labels 1 and 2 count as relevant, and every metric is computed at
//! `k = 3`:
//! - Precision@3 = relevant results among ranks 1–3 / 3 (always divided by 3);
//! - Hit@3 = 1 if any of ranks 1–3 is relevant, else 0;
//! - nDCG@3 = DCG@3 / IDCG@3 with gain `(2^label − 1) / log2(rank + 1)`; IDCG uses the
//!   query's three highest frozen labels; nDCG is 0 when IDCG is 0.
//!
//! Metrics are macro-averaged over the queries. Fewer than three results are reported
//! as such and never padded. Results without a label are irrelevant (label 0) and say
//! so in their note.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use crate::domain::Modality;
use crate::embedding::EmbeddingStrategy;
use crate::fusion::FusionParams;
use crate::ids::{
    Checksum, ChunkId, EmbeddingSpaceId, Fingerprint, IdError, QueryId, RecordId, SourceId,
};
use crate::record::Record;
use crate::retrieval::{
    FusionTrace, Leg, Scope, SearchError, SearchHit, read_records, search_combined, search_images,
    search_text,
};
use crate::storage::FsRecordStore;

/// Cut-off of every metric.
pub const K: usize = 3;

/// Retrieval mode under evaluation (PRD §6.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvaluationMode {
    /// Text queries against text records.
    TextOnly,
    /// Text queries against image records in a cross-modal space.
    ImageOnly,
    /// Rank fusion of text and image results.
    Combined,
}

impl EvaluationMode {
    /// Gherkin and report name, for example `text-only`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TextOnly => "text-only",
            Self::ImageOnly => "image-only",
            Self::Combined => "combined",
        }
    }

    /// Parses a Gherkin or CLI mode name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "text-only" => Some(Self::TextOnly),
            "image-only" => Some(Self::ImageOnly),
            "combined" => Some(Self::Combined),
            _ => None,
        }
    }
}

/// One frozen query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Query {
    /// Query identifier.
    pub id: QueryId,
    /// Portuguese query text, passed raw to the adapter.
    pub text: String,
    /// What the query is meant to find.
    pub target: String,
}

/// The frozen query set file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuerySet {
    /// Version label.
    pub version: String,
    /// Language tag.
    pub language: String,
    /// Queries, in report order.
    #[serde(rename = "query")]
    pub queries: Vec<Query>,
}

/// Relevance of one item for one query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Label {
    /// Query identifier.
    pub query: QueryId,
    /// Labeled item (chunk id, stable across embedding spaces but unique only within one
    /// chunking revision).
    pub item: ChunkId,
    /// What the label was written for. Text items (required): a distinctive substring
    /// of the chunk text; a text label is scored only when its chunk exists in the
    /// evaluated index and still contains the anchor (NFC, case-insensitive, whitespace
    /// collapsed). Image items: the original-image reference
    /// ([`crate::record::OriginalAsset::reference`]), required when images are
    /// evaluated; an image label is scored only when its chunk names that original.
    #[serde(default)]
    pub anchor: Option<String>,
    /// Image items: SHA-256 of the stored asset the label was written for, required when
    /// images are evaluated. A file name and a PDF image number do not identify pixels
    /// across source revisions; an image label is scored only when its chunk's asset has
    /// this content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_checksum: Option<Checksum>,
    /// 0 = irrelevant, 1 = partially relevant, 2 = directly relevant.
    pub label: u8,
    /// Why the item is (ir)relevant.
    pub note: String,
}

/// The frozen relevance-label file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabelSet {
    /// Version label.
    pub version: String,
    /// Version of the query set these labels belong to.
    pub query_set: String,
    /// Corpus the labels describe (for example `synthetic`).
    pub corpus: String,
    /// Labels.
    #[serde(rename = "label", default)]
    pub labels: Vec<Label>,
}

/// A frozen input together with the checksum of its exact bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frozen<T> {
    /// Parsed content.
    pub value: T,
    /// Checksum of the file bytes.
    pub checksum: Checksum,
}

/// Evaluation input and computation errors.
#[derive(Debug, thiserror::Error)]
pub enum EvalError {
    /// A dataset file cannot be read.
    #[error("cannot read {path}: {source}")]
    Read {
        /// Path.
        path: String,
        /// I/O error.
        #[source]
        source: std::io::Error,
    },
    /// A dataset file is not valid TOML for its schema.
    #[error("invalid {path}: {source}")]
    Parse {
        /// Path.
        path: String,
        /// Parse error.
        #[source]
        source: toml::de::Error,
    },
    /// The dataset is inconsistent.
    #[error("invalid evaluation dataset: {0}")]
    Invalid(String),
    /// Rankings were supplied for another set of queries than the frozen one.
    #[error("rankings do not match the query set: {0}")]
    RankingMismatch(String),
    /// A frozen label does not apply to the evaluated index: its chunk is missing or
    /// holds other text (for example after a chunking change shifted chunk indices).
    #[error("label {query} / {item} does not apply to this index: {reason}")]
    LabelNotApplicable {
        /// Query.
        query: QueryId,
        /// Labeled chunk.
        item: ChunkId,
        /// Why.
        reason: String,
    },
    /// A ranking returned the same item more than once, which would count one
    /// judgment several times (precision above its true value, nDCG above 1).
    #[error("ranking for {query} returns {item} more than once")]
    DuplicateHit {
        /// Query.
        query: QueryId,
        /// Repeated item.
        item: ChunkId,
    },
    /// A query could not be run.
    #[error(transparent)]
    Search(#[from] SearchError),
    /// A fingerprint could not be computed.
    #[error(transparent)]
    Fingerprint(#[from] IdError),
}

fn load_toml<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Frozen<T>, EvalError> {
    let shown = path.display().to_string();
    let bytes = std::fs::read(path).map_err(|source| EvalError::Read {
        path: shown.clone(),
        source,
    })?;
    let text = String::from_utf8_lossy(&bytes);
    let value = toml::from_str(&text).map_err(|source| EvalError::Parse {
        path: shown,
        source,
    })?;
    Ok(Frozen {
        value,
        checksum: Checksum::of_bytes(&bytes),
    })
}

/// Loads a frozen query set on its own (for example to draft labels for it).
///
/// # Errors
///
/// Returns [`EvalError::Read`] or [`EvalError::Parse`] for an unreadable or invalid
/// file.
pub fn load_queries(path: &Path) -> Result<Frozen<QuerySet>, EvalError> {
    load_toml(path)
}

/// A query set and its labels, loaded and cross-checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dataset {
    /// Frozen queries.
    pub queries: Frozen<QuerySet>,
    /// Frozen labels.
    pub labels: Frozen<LabelSet>,
}

impl Dataset {
    /// Loads both files and validates them together.
    ///
    /// # Errors
    ///
    /// Returns [`EvalError`] for unreadable files or an inconsistent dataset.
    pub fn load(queries: &Path, labels: &Path) -> Result<Self, EvalError> {
        Self::new(load_toml(queries)?, load_toml(labels)?)
    }

    /// Validates a dataset: unique query ids, labels for known queries only, labels in
    /// 0..=2, at most one label per (query, item), and matching set versions.
    ///
    /// # Errors
    ///
    /// Returns [`EvalError::Invalid`] describing the first inconsistency.
    pub fn new(queries: Frozen<QuerySet>, labels: Frozen<LabelSet>) -> Result<Self, EvalError> {
        let mut ids = BTreeSet::new();
        for q in &queries.value.queries {
            if !ids.insert(&q.id) {
                return Err(EvalError::Invalid(format!("duplicate query {}", q.id)));
            }
        }
        if labels.value.query_set != queries.value.version {
            return Err(EvalError::Invalid(format!(
                "labels are for query set {}, not {}",
                labels.value.query_set, queries.value.version
            )));
        }
        let mut seen = BTreeSet::new();
        for l in &labels.value.labels {
            if !ids.contains(&l.query) {
                return Err(EvalError::Invalid(format!(
                    "label for unknown query {}",
                    l.query
                )));
            }
            if l.label > 2 {
                return Err(EvalError::Invalid(format!(
                    "label {} for {} / {} is outside 0..=2",
                    l.label, l.query, l.item
                )));
            }
            if l.item.modality() == Modality::Text
                && l.anchor.as_deref().is_none_or(|a| a.trim().is_empty())
            {
                return Err(EvalError::Invalid(format!(
                    "text label {} / {} has no anchor; a chunk id alone does not identify its text",
                    l.query, l.item
                )));
            }
            if !seen.insert((&l.query, &l.item)) {
                return Err(EvalError::Invalid(format!(
                    "duplicate label for {} / {}",
                    l.query, l.item
                )));
            }
        }
        Ok(Self { queries, labels })
    }

    fn label(&self, query: &QueryId, item: &ChunkId) -> Option<&Label> {
        self.labels
            .value
            .labels
            .iter()
            .find(|l| &l.query == query && &l.item == item)
    }

    fn frozen_labels(&self, query: &QueryId) -> Vec<u8> {
        self.labels
            .value
            .labels
            .iter()
            .filter(|l| &l.query == query)
            .map(|l| l.label)
            .collect()
    }

    /// Fingerprint of everything that defines the evaluation but not the model: the
    /// dataset checksums, `k`, the metrics, and the relevance threshold.
    ///
    /// # Errors
    ///
    /// Returns [`IdError`] if the fingerprint cannot be computed.
    pub fn fingerprint(&self) -> Result<Fingerprint, IdError> {
        #[derive(Serialize)]
        struct Input<'a> {
            query_set: &'a Checksum,
            labels: &'a Checksum,
            k: usize,
            metrics: [&'static str; 3],
            relevant_label_at_least: u8,
        }
        Fingerprint::of(&Input {
            query_set: &self.queries.checksum,
            labels: &self.labels.checksum,
            k: K,
            metrics: ["precision@3", "hit@3", "ndcg@3"],
            relevant_label_at_least: 1,
        })
    }
}

/// Precision@3 of the labels of the returned results, in rank order.
#[must_use]
pub fn precision_at_3(returned: &[u8]) -> f64 {
    let relevant = returned.iter().take(K).filter(|&&l| l >= 1).count();
    count_f64(relevant) / count_f64(K)
}

/// Hit@3 of the labels of the returned results, in rank order.
#[must_use]
pub fn hit_at_3(returned: &[u8]) -> f64 {
    if returned.iter().take(K).any(|&l| l >= 1) {
        1.0
    } else {
        0.0
    }
}

fn dcg(labels: impl Iterator<Item = u8>) -> f64 {
    labels
        .take(K)
        .zip(1_u32..)
        .map(|(label, rank)| {
            let gain = 2_f64.powi(i32::from(label)) - 1.0;
            gain / f64::from(rank + 1).log2()
        })
        .sum()
}

/// nDCG@3 of the returned labels against all frozen labels of the query.
#[must_use]
pub fn ndcg_at_3(returned: &[u8], frozen: &[u8]) -> f64 {
    let mut ideal = frozen.to_vec();
    ideal.sort_unstable_by(|a, b| b.cmp(a));
    let idcg = dcg(ideal.into_iter());
    if idcg <= 0.0 {
        return 0.0;
    }
    dcg(returned.iter().copied()) / idcg
}

#[expect(
    clippy::cast_precision_loss,
    reason = "counts are tiny (k = 3, at most a few hundred queries)"
)]
const fn count_f64(n: usize) -> f64 {
    n as f64
}

/// Retrieval-quality metrics.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Metrics {
    /// Precision@3.
    pub precision_at_3: f64,
    /// Hit@3.
    pub hit_at_3: f64,
    /// nDCG@3.
    pub ndcg_at_3: f64,
}

/// One evaluated result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResultEntry {
    /// 1-based rank.
    pub rank: usize,
    /// Record identifier.
    pub record_id: RecordId,
    /// Source identifier.
    pub source_id: SourceId,
    /// Chunk identifier (the label key).
    pub chunk_id: ChunkId,
    /// Modality.
    pub modality: Modality,
    /// 1-based page number.
    pub page_number: u32,
    /// Image asset reference (image records).
    pub asset_uri: Option<String>,
    /// Chunk text (text records).
    pub chunk_text: Option<String>,
    /// Similarity score or fused score.
    pub score: f32,
    /// Relevance label (0 when unlabeled).
    pub label: u8,
    /// Whether the item has a frozen label.
    pub labeled: bool,
    /// Relevance or failure note.
    pub note: String,
    /// Leg ranks and exact fused score of a combined-mode result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fusion: Option<FusionTrace>,
}

/// Evaluation of one query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueryEntry {
    /// Query identifier.
    pub query_id: QueryId,
    /// Query text.
    pub text: String,
    /// Up to three results, in rank order; never padded.
    pub results: Vec<ResultEntry>,
    /// True when fewer than three candidates existed.
    pub fewer_than_k: bool,
    /// Per-query metrics.
    pub metrics: Metrics,
}

/// Outcome of an evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationStatus {
    /// Every query ran; low scores are still a completed evaluation.
    Completed,
}

/// Reference to a frozen dataset file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatasetRef {
    /// Version label.
    pub version: String,
    /// Checksum of the file bytes.
    pub checksum: Checksum,
}

/// Evaluation report of one configuration in one mode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluationReport {
    /// Mode.
    pub mode: EvaluationMode,
    /// Status.
    pub status: EvaluationStatus,
    /// Query set used.
    pub query_set: DatasetRef,
    /// Relevance labels used.
    pub labels: DatasetRef,
    /// Corpus the labels describe.
    pub labels_corpus: String,
    /// Fingerprint of the evaluation configuration.
    pub evaluation_fingerprint: Fingerprint,
    /// Embedding spaces searched (two for combined mode).
    pub spaces: Vec<EmbeddingSpaceId>,
    /// Metric cut-off.
    pub k: usize,
    /// One entry per frozen query, in query-set order.
    pub queries: Vec<QueryEntry>,
    /// Macro-averaged metrics.
    pub metrics: Metrics,
    /// Fusion parameters of a combined-mode evaluation (PRD §6.6: recorded in every
    /// combined-mode report); absent for single-modality modes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fusion: Option<FusionParams>,
}

/// Evaluates `rankings` (one list per query id, best first) against the dataset.
///
/// # Errors
///
/// Returns [`EvalError::RankingMismatch`] unless there is exactly one ranking per
/// frozen query, [`EvalError::DuplicateHit`] when a ranking repeats an item in its top
/// `k`, [`EvalError::LabelNotApplicable`] when a judged text hit does not
/// contain its label's anchor (the label belongs to other text), and
/// [`EvalError::Fingerprint`] if the fingerprint fails.
pub fn evaluate(
    mode: EvaluationMode,
    dataset: &Dataset,
    spaces: Vec<EmbeddingSpaceId>,
    rankings: &BTreeMap<QueryId, Vec<SearchHit>>,
) -> Result<EvaluationReport, EvalError> {
    let expected: BTreeSet<&QueryId> = dataset
        .queries
        .value
        .queries
        .iter()
        .map(|q| &q.id)
        .collect();
    let supplied: BTreeSet<&QueryId> = rankings.keys().collect();
    if expected != supplied {
        return Err(EvalError::RankingMismatch(format!(
            "expected {expected:?}, got {supplied:?}"
        )));
    }
    let mut entries = Vec::with_capacity(expected.len());
    for query in &dataset.queries.value.queries {
        let hits = rankings.get(&query.id).map_or(&[][..], Vec::as_slice);
        let mut seen = BTreeSet::new();
        if let Some(hit) = hits.iter().take(K).find(|h| !seen.insert(&h.chunk_id)) {
            return Err(EvalError::DuplicateHit {
                query: query.id.clone(),
                item: hit.chunk_id.clone(),
            });
        }
        let results: Vec<ResultEntry> = hits
            .iter()
            .take(K)
            .enumerate()
            .map(|(i, hit)| {
                let label = dataset.label(&query.id, &hit.chunk_id);
                if let Some(l) = label {
                    check_hit_applies(l, hit)?;
                }
                Ok(ResultEntry {
                    rank: i + 1,
                    record_id: hit.record_id.clone(),
                    source_id: hit.source_id.clone(),
                    chunk_id: hit.chunk_id.clone(),
                    modality: hit.modality,
                    page_number: hit.page_number,
                    asset_uri: hit.asset_uri.clone(),
                    chunk_text: hit.chunk_text.clone(),
                    score: hit.score,
                    label: label.map_or(0, |l| l.label),
                    labeled: label.is_some(),
                    note: label.map_or_else(
                        || "unlabeled: not judged relevant for this query (label 0)".to_owned(),
                        |l| l.note.clone(),
                    ),
                    fusion: hit.fusion.clone(),
                })
            })
            .collect::<Result<_, EvalError>>()?;
        let returned: Vec<u8> = results.iter().map(|r| r.label).collect();
        let metrics = Metrics {
            precision_at_3: precision_at_3(&returned),
            hit_at_3: hit_at_3(&returned),
            ndcg_at_3: ndcg_at_3(&returned, &dataset.frozen_labels(&query.id)),
        };
        entries.push(QueryEntry {
            query_id: query.id.clone(),
            text: query.text.clone(),
            fewer_than_k: results.len() < K,
            results,
            metrics,
        });
    }
    let n = count_f64(entries.len().max(1));
    let mean = |f: fn(&Metrics) -> f64| entries.iter().map(|e| f(&e.metrics)).sum::<f64>() / n;
    let metrics = Metrics {
        precision_at_3: mean(|m| m.precision_at_3),
        hit_at_3: mean(|m| m.hit_at_3),
        ndcg_at_3: mean(|m| m.ndcg_at_3),
    };
    Ok(EvaluationReport {
        mode,
        status: EvaluationStatus::Completed,
        query_set: DatasetRef {
            version: dataset.queries.value.version.clone(),
            checksum: dataset.queries.checksum.clone(),
        },
        labels: DatasetRef {
            version: dataset.labels.value.version.clone(),
            checksum: dataset.labels.checksum.clone(),
        },
        labels_corpus: dataset.labels.value.corpus.clone(),
        evaluation_fingerprint: dataset.fingerprint()?,
        spaces,
        k: K,
        queries: entries,
        metrics,
        fusion: None,
    })
}

/// The form anchors are compared in: NFC, whitespace collapsed, lower case.
pub(crate) fn anchor_form(text: &str) -> String {
    text.nfc()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Refuses a judged text hit whose text does not contain the label's anchor.
fn check_hit_applies(label: &Label, hit: &SearchHit) -> Result<(), EvalError> {
    let not_applicable = |reason: &str| EvalError::LabelNotApplicable {
        query: label.query.clone(),
        item: label.item.clone(),
        reason: reason.to_owned(),
    };
    match hit.modality {
        Modality::Text => {
            let text = hit.chunk_text.as_deref().unwrap_or_default();
            let anchor = label.anchor.as_deref().unwrap_or_default();
            if anchor_form(text).contains(&anchor_form(anchor)) {
                Ok(())
            } else {
                Err(not_applicable(
                    "the returned chunk does not contain the label's anchor",
                ))
            }
        }
        Modality::Image => {
            image_anchor_matches(label, hit.original.as_deref(), hit.asset_uri.as_deref())
                .map_err(not_applicable)
        }
    }
}

/// An image label applies when its anchor names the record's original image and its
/// asset checksum is the content of the record's stored asset.
fn image_anchor_matches(
    label: &Label,
    original: Option<&str>,
    asset_uri: Option<&str>,
) -> Result<(), &'static str> {
    let Some(anchor) = label.anchor.as_deref() else {
        return Err("an image label needs an anchor naming its original image");
    };
    let Some(checksum) = label.asset_checksum.as_ref() else {
        return Err("an image label needs the asset checksum of the image it judges");
    };
    if original != Some(anchor) {
        return Err(
            "the image at this chunk id is another original (extraction or filtering changed)",
        );
    }
    if asset_uri != Some(crate::storage::asset_uri(checksum).as_str()) {
        return Err(
            "the image at this chunk id has other content (the original or its preparation changed)",
        );
    }
    Ok(())
}

/// Refuses image labels that do not apply to `records` (the evaluated image index):
/// the labeled image must exist and come from the original named by its anchor.
///
/// # Errors
///
/// Returns [`EvalError::LabelNotApplicable`] for the first label that does not apply.
pub fn check_image_labels(dataset: &Dataset, records: &[Record]) -> Result<(), EvalError> {
    let originals: BTreeMap<&ChunkId, (Option<String>, Option<&str>)> = records
        .iter()
        .filter(|r| r.modality == Modality::Image)
        .map(|r| {
            (
                &r.chunk_id,
                (
                    r.metadata
                        .original_asset
                        .as_ref()
                        .map(crate::record::OriginalAsset::reference),
                    r.asset_uri.as_deref(),
                ),
            )
        })
        .collect();
    for l in &dataset.labels.value.labels {
        if l.item.modality() != Modality::Image {
            continue;
        }
        let not_applicable = |reason: &str| EvalError::LabelNotApplicable {
            query: l.query.clone(),
            item: l.item.clone(),
            reason: reason.to_owned(),
        };
        let Some((original, asset)) = originals.get(&l.item) else {
            return Err(not_applicable("the image is not in the evaluated index"));
        };
        image_anchor_matches(l, original.as_deref(), *asset).map_err(not_applicable)?;
    }
    Ok(())
}

/// Refuses text labels that do not apply to `records` (the evaluated index): the
/// labeled chunk must exist and contain the label's anchor.
///
/// # Errors
///
/// Returns [`EvalError::LabelNotApplicable`] for the first label that does not apply.
pub fn check_text_labels(dataset: &Dataset, records: &[Record]) -> Result<(), EvalError> {
    let texts: BTreeMap<&ChunkId, &str> = records
        .iter()
        .filter(|r| r.modality == Modality::Text)
        .map(|r| (&r.chunk_id, r.chunk_text.as_deref().unwrap_or_default()))
        .collect();
    for l in &dataset.labels.value.labels {
        if l.item.modality() != Modality::Text {
            continue;
        }
        let not_applicable = |reason: &str| EvalError::LabelNotApplicable {
            query: l.query.clone(),
            item: l.item.clone(),
            reason: reason.to_owned(),
        };
        let Some(text) = texts.get(&l.item) else {
            return Err(not_applicable("the chunk is not in the evaluated index"));
        };
        let anchor = l.anchor.as_deref().unwrap_or_default();
        if !anchor_form(text).contains(&anchor_form(anchor)) {
            return Err(not_applicable(
                "the chunk no longer contains the label's anchor (chunking or extraction changed)",
            ));
        }
    }
    Ok(())
}

/// Runs every frozen query in text-only mode (top 3) and evaluates the results.
///
/// # Errors
///
/// Returns [`EvalError::LabelNotApplicable`] when a text label does not apply to the
/// evaluated index, and [`EvalError::Search`] when a query fails (for example a space
/// mismatch).
pub async fn evaluate_text(
    store: &Arc<FsRecordStore>,
    space: &EmbeddingSpaceId,
    embedder: &dyn EmbeddingStrategy,
    dataset: &Dataset,
    scope: Scope<'_>,
) -> Result<EvaluationReport, EvalError> {
    check_text_labels(dataset, &read_records(store, space, scope).await?)?;
    let mut rankings = BTreeMap::new();
    for query in &dataset.queries.value.queries {
        let hits = search_text(store, space, embedder, &query.text, K, scope).await?;
        rankings.insert(query.id.clone(), hits);
    }
    evaluate(
        EvaluationMode::TextOnly,
        dataset,
        vec![space.clone()],
        &rankings,
    )
}

/// Runs every frozen query in image-only mode (top 3: text queries through the
/// model's text-query path against the image records) and evaluates the results.
///
/// # Errors
///
/// Returns [`EvalError::LabelNotApplicable`] when an image label does not apply to the
/// evaluated index, and [`EvalError::Search`] when the model has no compatible
/// text-to-image space or a query fails.
pub async fn evaluate_images(
    store: &Arc<FsRecordStore>,
    space: &EmbeddingSpaceId,
    embedder: &dyn EmbeddingStrategy,
    dataset: &Dataset,
    scope: Scope<'_>,
) -> Result<EvaluationReport, EvalError> {
    embedder
        .capabilities()
        .require(
            crate::embedding::Capability::TextToImage,
            embedder.model_info(),
        )
        .map_err(|e| EvalError::Search(SearchError::Embedding(e)))?;
    check_image_labels(dataset, &read_records(store, space, scope).await?)?;
    let mut rankings = BTreeMap::new();
    for query in &dataset.queries.value.queries {
        let hits = search_images(store, space, embedder, &query.text, K, scope).await?;
        rankings.insert(query.id.clone(), hits);
    }
    evaluate(
        EvaluationMode::ImageOnly,
        dataset,
        vec![space.clone()],
        &rankings,
    )
}

/// Runs every frozen query in combined mode (PRD §6.6: the text leg against its text
/// index, the image leg against its cross-modal image index, fused with RRF; top 3)
/// and evaluates the fused results. The report names both spaces, text first, and
/// records the fusion parameters.
///
/// # Errors
///
/// Returns [`EvalError::Search`] when a leg's model does not belong to its space, the
/// image leg's model has no compatible text-to-image space, or an experiment scope is
/// not the combined run of these legs and fusion parameters
/// ([`SearchError::SnapshotMismatch`]), all before any index read;
/// [`EvalError::LabelNotApplicable`] when a text label does not apply to the text index
/// or an image label to the image index; and [`EvalError::Search`] when a query fails.
pub async fn evaluate_combined(
    store: &Arc<FsRecordStore>,
    text: Leg<'_>,
    image: Leg<'_>,
    fusion: FusionParams,
    dataset: &Dataset,
    scope: Scope<'_>,
) -> Result<EvaluationReport, EvalError> {
    for leg in [text, image] {
        let query = &leg.embedder.model_info().embedding_space_id;
        if query != leg.space {
            return Err(EvalError::Search(SearchError::SpaceMismatch {
                index: leg.space.clone(),
                query: query.clone(),
            }));
        }
    }
    image
        .embedder
        .capabilities()
        .require(
            crate::embedding::Capability::TextToImage,
            image.embedder.model_info(),
        )
        .map_err(|e| EvalError::Search(SearchError::Embedding(e)))?;
    // The snapshot is checked before any record is read (Antigravity A4-1): a snapshot
    // missing a leg would otherwise read as an empty index and fail as an inapplicable
    // label, and with no query it would never reach `search_combined`'s own check.
    if let Scope::Experiment(snapshot) = scope {
        crate::retrieval::check_combined_snapshot(snapshot, text.space, image.space, fusion)?;
    }
    check_text_labels(dataset, &read_records(store, text.space, scope).await?)?;
    check_image_labels(dataset, &read_records(store, image.space, scope).await?)?;
    let mut rankings = BTreeMap::new();
    for query in &dataset.queries.value.queries {
        let hits = search_combined(store, text, image, fusion, &query.text, K, scope).await?;
        rankings.insert(query.id.clone(), hits);
    }
    let mut report = evaluate(
        EvaluationMode::Combined,
        dataset,
        vec![text.space.clone(), image.space.clone()],
        &rankings,
    )?;
    report.fusion = Some(fusion);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    #[test]
    fn metrics_follow_the_prd_formulas() {
        // Ranks 1..3 labeled 2, 0, 1; frozen labels {2, 2, 1}.
        let returned = [2, 0, 1];
        assert_abs_diff_eq!(precision_at_3(&returned), 2.0 / 3.0, epsilon = 1e-12);
        assert_abs_diff_eq!(hit_at_3(&returned), 1.0, epsilon = 1e-12);
        let dcg = 3.0 / 2_f64.log2() + 0.0 + 1.0 / 4_f64.log2();
        let idcg = 3.0 / 2_f64.log2() + 3.0 / 3_f64.log2() + 1.0 / 4_f64.log2();
        assert_abs_diff_eq!(
            ndcg_at_3(&returned, &[1, 2, 2]),
            dcg / idcg,
            epsilon = 1e-12
        );
        // Fewer results still divide precision by 3; missing ranks add no gain.
        assert_abs_diff_eq!(precision_at_3(&[1]), 1.0 / 3.0, epsilon = 1e-12);
        // No relevant frozen label: IDCG = 0 gives nDCG 0.
        assert_abs_diff_eq!(ndcg_at_3(&[0, 0, 0], &[0]), 0.0, epsilon = 1e-12);
        assert_abs_diff_eq!(hit_at_3(&[0, 0, 0, 2]), 0.0, epsilon = 1e-12);
        // Only the top three frozen labels form the ideal ranking.
        assert_abs_diff_eq!(ndcg_at_3(&[2, 2, 2], &[2, 2, 2, 2]), 1.0, epsilon = 1e-12);
    }

    fn frozen<T>(value: T, bytes: &[u8]) -> Frozen<T> {
        Frozen {
            value,
            checksum: Checksum::of_bytes(bytes),
        }
    }

    fn queries() -> QuerySet {
        QuerySet {
            version: "v".into(),
            language: "pt-BR".into(),
            queries: vec![Query {
                id: "q1".parse().expect("id"),
                text: "t".into(),
                target: "x".into(),
            }],
        }
    }

    fn label(query: &str, item: &str, value: u8) -> Label {
        Label {
            query: query.parse().expect("query"),
            item: item.parse().expect("item"),
            anchor: Some("hemoglobina".into()),
            asset_checksum: None,
            label: value,
            note: "n".into(),
        }
    }

    fn dataset_with(labels: Vec<Label>) -> Dataset {
        Dataset::new(
            frozen(queries(), b"q"),
            frozen(
                LabelSet {
                    version: "l".into(),
                    query_set: "v".into(),
                    corpus: "synthetic".into(),
                    labels,
                },
                b"l",
            ),
        )
        .expect("dataset")
    }

    fn record(chunk: &str, text: &str) -> Record {
        let mut record: Record =
            serde_json::from_str(include_str!("../reports/examples/record-001.json"))
                .expect("example record");
        record.chunk_id = chunk.parse().expect("chunk");
        record.chunk_text = Some(text.to_owned());
        record
    }

    #[test]
    fn a_text_label_without_an_anchor_is_rejected() {
        let mut unanchored = label("q1", "s-p1-text-0", 2);
        unanchored.anchor = None;
        let outcome = Dataset::new(
            frozen(queries(), b"q"),
            frozen(
                LabelSet {
                    version: "l".into(),
                    query_set: "v".into(),
                    corpus: "synthetic".into(),
                    labels: vec![unanchored],
                },
                b"l",
            ),
        );
        assert!(matches!(outcome, Err(EvalError::Invalid(_))));
        // Image labels need no text anchor.
        let mut image = label("q1", "s-p1-image-0", 2);
        image.anchor = None;
        dataset_with(vec![image]);
    }

    #[test]
    fn labels_that_no_longer_match_their_chunk_are_refused() {
        // Regression (C2-1): a chunking change shifted the hemoglobin row from index 0 to
        // index 1; chunk 0 now holds another row.
        let dataset = dataset_with(vec![label("q1", "s-p1-text-0", 2)]);
        let shifted = [
            record(
                "s-p1-text-0",
                "Hemograma\nHematócrito | 40,2 | % | 36,0–46,0",
            ),
            record(
                "s-p1-text-1",
                "Hemograma\nHemoglobina | 13,5 | g/dL | 12,0–16,0",
            ),
        ];
        assert!(matches!(
            check_text_labels(&dataset, &shifted),
            Err(EvalError::LabelNotApplicable { .. })
        ));
        assert!(matches!(
            check_text_labels(&dataset, &[]),
            Err(EvalError::LabelNotApplicable { .. })
        ));
        // NFC, case, and whitespace differences still match.
        let ok = [record("s-p1-text-0", "Hemograma\n  HEMOGLOBINA  | 13,5")];
        check_text_labels(&dataset, &ok).expect("anchor matches");
    }

    fn hit(chunk: &str, text: &str) -> SearchHit {
        let r = record(chunk, text);
        SearchHit {
            rank: 1,
            record_id: r.record_id,
            source_id: r.source_id,
            source_file: r.metadata.source_file,
            revision_id: r.revision_id,
            chunk_id: r.chunk_id,
            page_number: r.metadata.page_number,
            modality: r.modality,
            chunk_text: r.chunk_text,
            asset_uri: r.asset_uri,
            original: None,
            score: 0.9,
            fusion: None,
        }
    }

    #[test]
    fn public_evaluate_refuses_a_judged_hit_holding_other_text() {
        // Regression (C2-1, round 2): the public operation, not only `evaluate_text`,
        // must refuse a label whose chunk id now holds other text.
        let dataset = dataset_with(vec![label("q1", "s-p1-text-0", 2)]);
        let space: EmbeddingSpaceId = format!("space-{}", "0".repeat(32)).parse().expect("space");
        let rankings = |text: &str| {
            BTreeMap::from([("q1".parse().expect("id"), vec![hit("s-p1-text-0", text)])])
        };
        let shifted = rankings("Hemograma\nHematócrito | 40,2 | % | 36,0–46,0");
        assert!(matches!(
            evaluate(
                EvaluationMode::TextOnly,
                &dataset,
                vec![space.clone()],
                &shifted
            ),
            Err(EvalError::LabelNotApplicable { .. })
        ));
        let intact = rankings("Hemograma\nHemoglobina | 13,5 | g/dL | 12,0–16,0");
        let report = evaluate(EvaluationMode::TextOnly, &dataset, vec![space], &intact)
            .expect("the anchor matches");
        assert_eq!(report.queries[0].results[0].label, 2);
    }

    #[test]
    fn a_ranking_that_repeats_a_chunk_is_refused() {
        // Regression (C2-19): three copies of one relevant hit scored P@3 = 1 and
        // nDCG@3 > 2.
        let dataset = dataset_with(vec![label("q1", "s-p1-text-0", 2)]);
        let space: EmbeddingSpaceId = format!("space-{}", "0".repeat(32)).parse().expect("space");
        let copy = hit("s-p1-text-0", "Hemoglobina | 13,5");
        let rankings = BTreeMap::from([(
            "q1".parse().expect("id"),
            vec![copy.clone(), copy.clone(), copy],
        )]);
        assert!(matches!(
            evaluate(EvaluationMode::TextOnly, &dataset, vec![space], &rankings),
            Err(EvalError::DuplicateHit { .. })
        ));
    }

    #[test]
    fn every_accepted_ranking_keeps_its_metrics_within_zero_and_one() {
        // Exhaustive over every ordered ranking of up to 3 distinct hits drawn from four
        // labeled chunks (labels 2, 2, 1, 0) and one unlabeled chunk.
        let labeled = [
            ("s-p1-text-0", 2),
            ("s-p1-text-1", 2),
            ("s-p1-text-2", 1),
            ("s-p1-text-3", 0),
        ];
        let dataset = dataset_with(
            labeled
                .iter()
                .map(|(item, value)| label("q1", item, *value))
                .collect(),
        );
        let space: EmbeddingSpaceId = format!("space-{}", "0".repeat(32)).parse().expect("space");
        let pool: Vec<&str> = labeled
            .iter()
            .map(|(item, _)| *item)
            .chain(["s-p1-text-4"])
            .collect();
        let mut rankings_checked = 0;
        let mut stack: Vec<Vec<usize>> = vec![Vec::new()];
        while let Some(order) = stack.pop() {
            let hits: Vec<SearchHit> = order
                .iter()
                .map(|&i| hit(pool[i], "Hemoglobina | 13,5"))
                .collect();
            let rankings = BTreeMap::from([("q1".parse().expect("id"), hits)]);
            let report = evaluate(
                EvaluationMode::TextOnly,
                &dataset,
                vec![space.clone()],
                &rankings,
            )
            .expect("distinct hits evaluate");
            let m = &report.metrics;
            for value in [m.precision_at_3, m.hit_at_3, m.ndcg_at_3] {
                assert!((0.0..=1.0).contains(&value), "{order:?}: {m:?}");
            }
            rankings_checked += 1;
            if order.len() < K {
                for next in 0..pool.len() {
                    if !order.contains(&next) {
                        let mut longer = order.clone();
                        longer.push(next);
                        stack.push(longer);
                    }
                }
            }
        }
        // 1 empty + 5 + 5*4 + 5*4*3 orderings.
        assert_eq!(rankings_checked, 86);
    }

    fn image_record(chunk: &str, original: Option<u32>) -> Record {
        let mut r = record(chunk, "");
        r.modality = Modality::Image;
        r.chunk_text = None;
        r.metadata.original_asset = Some(crate::record::OriginalAsset {
            file: "us-mixed-003.pdf".into(),
            page: 1,
            pdf_image_num: original,
            width: 256,
            height: 192,
        });
        r.asset_uri = Some(crate::storage::asset_uri(&Checksum::of_bytes(b"pixels")));
        r
    }

    fn image_label(anchor: Option<&str>) -> Label {
        Label {
            query: "q1".parse().expect("query"),
            item: "s-p1-image-0".parse().expect("item"),
            anchor: anchor.map(str::to_owned),
            asset_checksum: Some(Checksum::of_bytes(b"pixels")),
            label: 2,
            note: "n".into(),
        }
    }

    #[test]
    fn image_labels_apply_only_to_their_anchored_original() {
        let anchored = dataset_with(vec![image_label(Some("us-mixed-003.pdf#page=1&image=0"))]);
        // The same chunk id now names another image (num 1): refused.
        let shifted = [image_record("s-p1-image-0", Some(1))];
        assert!(matches!(
            check_image_labels(&anchored, &shifted),
            Err(EvalError::LabelNotApplicable { .. })
        ));
        assert!(matches!(
            check_image_labels(&anchored, &[]),
            Err(EvalError::LabelNotApplicable { .. })
        ));
        check_image_labels(&anchored, &[image_record("s-p1-image-0", Some(0))])
            .expect("anchor matches");
        // Without an anchor an image label cannot be checked, so it is refused.
        let unanchored = dataset_with(vec![image_label(None)]);
        assert!(matches!(
            check_image_labels(&unanchored, &[image_record("s-p1-image-0", Some(0))]),
            Err(EvalError::LabelNotApplicable { .. })
        ));
        // Text labels are not image labels: an image check ignores them.
        check_image_labels(&dataset_with(vec![label("q1", "s-p1-text-0", 2)]), &[])
            .expect("no image label");
    }

    #[test]
    fn image_labels_apply_only_to_the_content_they_judged() {
        // Regression (C3-5): the same original reference with other pixels is refused.
        let anchored = dataset_with(vec![image_label(Some("us-mixed-003.pdf#page=1&image=0"))]);
        let mut replaced = image_record("s-p1-image-0", Some(0));
        replaced.asset_uri = Some(crate::storage::asset_uri(&Checksum::of_bytes(
            b"other pixels",
        )));
        let refused = check_image_labels(&anchored, &[replaced]);
        assert!(
            matches!(&refused, Err(EvalError::LabelNotApplicable { reason, .. }) if reason.contains("other content")),
            "{refused:?}"
        );
        // A label without the checksum cannot be checked, so it is refused.
        let mut unchecked = image_label(Some("us-mixed-003.pdf#page=1&image=0"));
        unchecked.asset_checksum = None;
        assert!(matches!(
            check_image_labels(
                &dataset_with(vec![unchecked]),
                &[image_record("s-p1-image-0", Some(0))]
            ),
            Err(EvalError::LabelNotApplicable { .. })
        ));
        // The same check guards a judged hit in the public `evaluate`.
        let space: EmbeddingSpaceId = format!("space-{}", "0".repeat(32)).parse().expect("space");
        let mut h = hit("s-p1-image-0", "");
        h.modality = Modality::Image;
        h.chunk_text = None;
        h.original = Some("us-mixed-003.pdf#page=1&image=0".into());
        h.asset_uri = Some(crate::storage::asset_uri(&Checksum::of_bytes(
            b"other pixels",
        )));
        let rankings = BTreeMap::from([("q1".parse().expect("id"), vec![h])]);
        assert!(matches!(
            evaluate(EvaluationMode::ImageOnly, &anchored, vec![space], &rankings),
            Err(EvalError::LabelNotApplicable { .. })
        ));
    }

    #[test]
    fn public_evaluate_refuses_an_image_hit_from_another_original() {
        let dataset = dataset_with(vec![image_label(Some("us-mixed-003.pdf#page=1&image=0"))]);
        let space: EmbeddingSpaceId = format!("space-{}", "0".repeat(32)).parse().expect("space");
        let image_hit = |num: u32| {
            let mut h = hit("s-p1-image-0", "");
            h.modality = Modality::Image;
            h.chunk_text = None;
            h.original = Some(format!("us-mixed-003.pdf#page=1&image={num}"));
            h.asset_uri = Some(crate::storage::asset_uri(&Checksum::of_bytes(b"pixels")));
            BTreeMap::from([("q1".parse().expect("id"), vec![h])])
        };
        assert!(matches!(
            evaluate(
                EvaluationMode::ImageOnly,
                &dataset,
                vec![space.clone()],
                &image_hit(1)
            ),
            Err(EvalError::LabelNotApplicable { .. })
        ));
        let report = evaluate(
            EvaluationMode::ImageOnly,
            &dataset,
            vec![space],
            &image_hit(0),
        )
        .expect("matching original");
        assert_eq!(report.queries[0].results[0].label, 2);
    }

    #[test]
    fn inconsistent_datasets_are_rejected() {
        let labels = |l: Vec<Label>, set: &str| LabelSet {
            version: "l".into(),
            query_set: set.into(),
            corpus: "synthetic".into(),
            labels: l,
        };
        let cases = [
            labels(vec![label("q2", "s-p1-text-0", 1)], "v"),
            labels(vec![label("q1", "s-p1-text-0", 3)], "v"),
            labels(
                vec![label("q1", "s-p1-text-0", 1), label("q1", "s-p1-text-0", 2)],
                "v",
            ),
            labels(vec![], "other"),
        ];
        for case in cases {
            let outcome = Dataset::new(frozen(queries(), b"q"), frozen(case, b"l"));
            assert!(matches!(outcome, Err(EvalError::Invalid(_))));
        }
        let ok = Dataset::new(
            frozen(queries(), b"q"),
            frozen(labels(vec![label("q1", "s-p1-text-0", 2)], "v"), b"l"),
        );
        assert!(ok.is_ok());
    }

    #[test]
    fn the_frozen_dataset_loads_with_ten_queries() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("config/evaluation");
        let dataset = Dataset::load(
            &root.join("queries-v1.toml"),
            &root.join("labels-synthetic-v4.toml"),
        )
        .expect("dataset");
        assert_eq!(dataset.queries.value.queries.len(), 10);
        assert!(dataset.fingerprint().is_ok());
    }
}
