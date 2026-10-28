//! Exact retrieval over one embedding space (PRD §6.6, PLAN Phase 1 step 7).
//!
//! The query model's space must equal the index space, which is checked before any
//! query embedding or similarity computation. Scores are dot products of L2-normalized
//! vectors (cosine), accumulated in `f64`. Results are ordered by descending score
//! (`total_cmp`), with equal scores broken by ascending `record_id`.

use std::cmp::Ordering;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::domain::Modality;
use crate::embedding::{EmbeddingError, EmbeddingStrategy, TextInput, validate_batch};
use crate::ids::{ChunkId, EmbeddingSpaceId, ItemId, RecordId, RevisionId, SourceId};
use crate::storage::{ExperimentSnapshot, FsRecordStore, StoreError};

/// Which revisions to search.
#[derive(Debug, Clone, Copy)]
pub enum Scope<'a> {
    /// The currently active revisions.
    Active,
    /// The revisions pinned by an experiment snapshot.
    Experiment(&'a ExperimentSnapshot),
}

/// One ranked result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    /// 1-based rank.
    pub rank: usize,
    /// Record identifier.
    pub record_id: RecordId,
    /// Source identifier.
    pub source_id: SourceId,
    /// Source file name.
    pub source_file: String,
    /// Revision of the record.
    pub revision_id: RevisionId,
    /// Chunk identifier.
    pub chunk_id: ChunkId,
    /// 1-based page number.
    pub page_number: u32,
    /// Modality of the record.
    pub modality: Modality,
    /// Chunk text (text records).
    pub chunk_text: Option<String>,
    /// Asset reference (image records).
    pub asset_uri: Option<String>,
    /// Cosine similarity.
    pub score: f32,
}

/// Retrieval errors.
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    /// The query model belongs to another embedding space than the index.
    #[error("embedding-space mismatch: index {index}, query model {query}")]
    SpaceMismatch {
        /// Space of the index.
        index: EmbeddingSpaceId,
        /// Space of the query model.
        query: EmbeddingSpaceId,
    },
    /// A stored record belongs to another space than the index it was read from.
    #[error("record {record} belongs to space {actual}, not {expected}")]
    CorruptIndex {
        /// Offending record.
        record: RecordId,
        /// Index space.
        expected: EmbeddingSpaceId,
        /// Record space.
        actual: EmbeddingSpaceId,
    },
    /// The query could not be embedded.
    #[error(transparent)]
    Embedding(#[from] EmbeddingError),
    /// The query vector failed validation.
    #[error("invalid query vector: {0}")]
    InvalidQueryVector(String),
    /// The store could not be read.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// A blocking store task panicked or was cancelled.
    #[error("store task failed: {0}")]
    Task(String),
}

/// Dot product accumulated in `f64`.
fn dot(a: &[f32], b: &[f32]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| f64::from(*x) * f64::from(*y))
        .sum()
}

/// Refuses a query from another embedding space; runs before any store read, query
/// embedding, or similarity computation.
fn guard_space(index: &EmbeddingSpaceId, query: &EmbeddingSpaceId) -> Result<(), SearchError> {
    if query == index {
        Ok(())
    } else {
        Err(SearchError::SpaceMismatch {
            index: index.clone(),
            query: query.clone(),
        })
    }
}

/// Reads the records of `index_space` in `scope` on the blocking pool.
async fn read_records(
    store: &Arc<FsRecordStore>,
    index_space: &EmbeddingSpaceId,
    scope: Scope<'_>,
) -> Result<Vec<crate::record::Record>, SearchError> {
    let pinned: Option<Vec<_>> = match scope {
        Scope::Active => None,
        Scope::Experiment(snapshot) => Some(
            snapshot
                .revisions
                .iter()
                .filter(|r| &r.space == index_space)
                .cloned()
                .collect(),
        ),
    };
    // Reading and parsing records.jsonl is blocking file I/O.
    let (store, space) = (Arc::clone(store), index_space.clone());
    tokio::task::spawn_blocking(move || {
        let revisions = match pinned {
            Some(revisions) => revisions,
            None => store.active(&space)?,
        };
        store.records(&revisions)
    })
    .await
    .map_err(|e| SearchError::Task(e.to_string()))?
    .map_err(SearchError::Store)
}

/// Scores every record against `query_vector` and returns the top `limit` hits.
fn rank(
    records: Vec<crate::record::Record>,
    index_space: &EmbeddingSpaceId,
    query_vector: &[f32],
    limit: usize,
) -> Result<Vec<SearchHit>, SearchError> {
    let mut scored = Vec::with_capacity(records.len());
    for record in records {
        if &record.embedding.embedding_space_id != index_space {
            return Err(SearchError::CorruptIndex {
                record: record.record_id,
                expected: index_space.clone(),
                actual: record.embedding.embedding_space_id,
            });
        }
        if record.embedding.vector.len() != query_vector.len() {
            return Err(SearchError::InvalidQueryVector(format!(
                "query has {} dimensions, index records have {}",
                query_vector.len(),
                record.embedding.vector.len()
            )));
        }
        let score = dot(query_vector, &record.embedding.vector);
        scored.push((score, record));
    }
    scored.sort_by(|(sa, ra), (sb, rb)| match sb.total_cmp(sa) {
        Ordering::Equal => ra.record_id.cmp(&rb.record_id),
        other => other,
    });
    Ok(scored
        .into_iter()
        .take(limit)
        .enumerate()
        .map(|(i, (score, r))| {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "cosine of unit vectors is in [-1, 1]"
            )]
            let score = score as f32;
            SearchHit {
                rank: i + 1,
                record_id: r.record_id,
                source_id: r.source_id,
                source_file: r.metadata.source_file,
                revision_id: r.revision_id,
                chunk_id: r.chunk_id,
                page_number: r.metadata.page_number,
                modality: r.modality,
                chunk_text: r.chunk_text,
                asset_uri: r.asset_uri,
                score,
            }
        })
        .collect())
}

/// Searches `index_space` with a text query embedded by `embedder`.
///
/// # Errors
///
/// Returns [`SearchError::SpaceMismatch`] before embedding when the query model does
/// not belong to `index_space`, and other [`SearchError`]s for embedding or store
/// failures. An empty index yields an empty result list.
pub async fn search_text(
    store: &Arc<FsRecordStore>,
    index_space: &EmbeddingSpaceId,
    embedder: &dyn EmbeddingStrategy,
    query: &str,
    limit: usize,
    scope: Scope<'_>,
) -> Result<Vec<SearchHit>, SearchError> {
    guard_space(index_space, &embedder.model_info().embedding_space_id)?;
    let records = read_records(store, index_space, scope).await?;
    if records.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }
    let id: ItemId = "query"
        .parse()
        .map_err(|e: crate::ids::IdError| SearchError::InvalidQueryVector(e.to_string()))?;
    let batch = embedder
        .embed_queries(&[TextInput {
            id: id.clone(),
            text: query.to_owned(),
        }])
        .await?;
    validate_batch(&batch, std::slice::from_ref(&id), embedder.model_info())
        .map_err(|d| SearchError::InvalidQueryVector(d.to_string()))?;
    let Some(query_vector) = batch.embeddings.first().map(|e| e.vector.as_slice()) else {
        return Err(SearchError::InvalidQueryVector(
            "no vector returned".to_owned(),
        ));
    };
    rank(records, index_space, query_vector, limit)
}

/// Searches `index_space` with a query vector already embedded in `query_space`.
///
/// # Errors
///
/// Returns [`SearchError::SpaceMismatch`] before reading the index when the spaces
/// differ, even if the dimensions match (PRD §6.5), and other [`SearchError`]s for
/// store failures or a vector of the wrong size.
pub async fn search_vector(
    store: &Arc<FsRecordStore>,
    index_space: &EmbeddingSpaceId,
    query_space: &EmbeddingSpaceId,
    query_vector: &[f32],
    limit: usize,
    scope: Scope<'_>,
) -> Result<Vec<SearchHit>, SearchError> {
    guard_space(index_space, query_space)?;
    let records = read_records(store, index_space, scope).await?;
    if limit == 0 {
        return Ok(Vec::new());
    }
    rank(records, index_space, query_vector, limit)
}
