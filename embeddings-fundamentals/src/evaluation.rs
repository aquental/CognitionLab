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

use crate::domain::Modality;
use crate::embedding::EmbeddingStrategy;
use crate::ids::{
    Checksum, ChunkId, EmbeddingSpaceId, Fingerprint, IdError, QueryId, RecordId, SourceId,
};
use crate::retrieval::{Scope, SearchError, SearchHit, search_text};
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
    /// Labeled item (chunk id, stable across embedding spaces).
    pub item: ChunkId,
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
}

/// Evaluates `rankings` (one list per query id, best first) against the dataset.
///
/// # Errors
///
/// Returns [`EvalError::RankingMismatch`] unless there is exactly one ranking per
/// frozen query, and [`EvalError::Fingerprint`] if the fingerprint fails.
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
        let results: Vec<ResultEntry> = hits
            .iter()
            .take(K)
            .enumerate()
            .map(|(i, hit)| {
                let label = dataset.label(&query.id, &hit.chunk_id);
                ResultEntry {
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
                }
            })
            .collect();
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
    })
}

/// Runs every frozen query in text-only mode (top 3) and evaluates the results.
///
/// # Errors
///
/// Returns [`EvalError::Search`] when a query fails (for example a space mismatch).
pub async fn evaluate_text(
    store: &Arc<FsRecordStore>,
    space: &EmbeddingSpaceId,
    embedder: &dyn EmbeddingStrategy,
    dataset: &Dataset,
    scope: Scope<'_>,
) -> Result<EvaluationReport, EvalError> {
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
            label: value,
            note: "n".into(),
        }
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
            &root.join("labels-synthetic-v1.toml"),
        )
        .expect("dataset");
        assert_eq!(dataset.queries.value.queries.len(), 10);
        assert!(dataset.fingerprint().is_ok());
    }
}
