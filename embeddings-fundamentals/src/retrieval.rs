//! Exact retrieval over one embedding space (PRD §6.6, PLAN Phase 1 step 7).
//!
//! The query model's space must equal the index space, which is checked before any
//! query embedding or similarity computation. Scores are dot products of L2-normalized
//! vectors (cosine), accumulated in `f64`. Results are ordered by descending score
//! (`total_cmp`), with equal scores broken by ascending `record_id`.
//!
//! Text search ranks the text records of a space; image search ranks its image
//! records. Both embed the query through the model's text-query path
//! (`embed_queries`); image search additionally requires a model whose text queries
//! and images share one compatible space.
//!
//! Combined search runs both legs, each against its own index space with its own
//! query model, and fuses their rankings with RRF (PRD §6.6): only ranks are fused,
//! never the raw scores of the two models.

use std::cmp::Ordering;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::domain::Modality;
use crate::embedding::{Capability, EmbeddingError, EmbeddingStrategy, TextInput, validate_batch};
use crate::fusion::{FusionParams, rrf};
use crate::ids::Fingerprint;
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
    /// Reference to the original image (image records, see
    /// [`crate::record::OriginalAsset::reference`]).
    pub original: Option<String>,
    /// Cosine similarity; for a combined result, the fused RRF score.
    pub score: f32,
    /// How a combined result was fused from the two legs; absent for single-modality
    /// results.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fusion: Option<FusionTrace>,
}

/// Provenance of a fused result: its exact fused score and its place in each leg.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FusionTrace {
    /// Exact fused score (`score` holds it rounded to `f32`).
    pub fused_score: f64,
    /// Rank and cosine score in the text leg, when the text leg returned it.
    pub text: Option<LegTrace>,
    /// Rank and cosine score in the image leg, when the image leg returned it.
    pub image: Option<LegTrace>,
}

/// Position of a result in one leg of a combined search.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LegTrace {
    /// 1-based rank in the leg.
    pub rank: usize,
    /// Cosine similarity in the leg's space.
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
    /// A combined search was scoped to a snapshot that is not the combined run of
    /// these legs and fusion parameters.
    #[error("experiment {experiment} does not match this combined search: {reason}")]
    SnapshotMismatch {
        /// The snapshot's experiment.
        experiment: crate::ids::ExperimentId,
        /// What differs.
        reason: String,
    },
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
pub(crate) async fn read_records(
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
                original: r
                    .metadata
                    .original_asset
                    .as_ref()
                    .map(crate::record::OriginalAsset::reference),
                score,
                fusion: None,
            }
        })
        .collect())
}

/// Searches the text records of `index_space` with a text query embedded by
/// `embedder`.
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
    search(
        store,
        index_space,
        embedder,
        query,
        limit,
        scope,
        Modality::Text,
    )
    .await
}

/// Searches the image records of `index_space` with a text query embedded through
/// the model's text-query path.
///
/// # Errors
///
/// Returns [`SearchError::Embedding`] with [`EmbeddingError::Unsupported`] before any
/// store read when the model has no compatible text-to-image space, and otherwise the
/// errors of [`search_text`].
pub async fn search_images(
    store: &Arc<FsRecordStore>,
    index_space: &EmbeddingSpaceId,
    embedder: &dyn EmbeddingStrategy,
    query: &str,
    limit: usize,
    scope: Scope<'_>,
) -> Result<Vec<SearchHit>, SearchError> {
    embedder
        .capabilities()
        .require(Capability::TextToImage, embedder.model_info())?;
    search(
        store,
        index_space,
        embedder,
        query,
        limit,
        scope,
        Modality::Image,
    )
    .await
}

async fn search(
    store: &Arc<FsRecordStore>,
    index_space: &EmbeddingSpaceId,
    embedder: &dyn EmbeddingStrategy,
    query: &str,
    limit: usize,
    scope: Scope<'_>,
    modality: Modality,
) -> Result<Vec<SearchHit>, SearchError> {
    guard_space(index_space, &embedder.model_info().embedding_space_id)?;
    let mut records = read_records(store, index_space, scope).await?;
    records.retain(|r| r.modality == modality);
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
    // The same finiteness rule as embedded queries (PRD §4): a NaN or infinite component
    // would make every score meaningless, so it is refused before any index read.
    if let Some((index, value)) = query_vector
        .iter()
        .enumerate()
        .find(|(_, v)| !v.is_finite())
    {
        return Err(SearchError::InvalidQueryVector(format!(
            "query vector component {index} is not finite ({value})"
        )));
    }
    let records = read_records(store, index_space, scope).await?;
    if limit == 0 {
        return Ok(Vec::new());
    }
    rank(records, index_space, query_vector, limit)
}

/// One leg of a combined search: an index space and the query model of that space.
#[derive(Clone, Copy)]
pub struct Leg<'a> {
    /// Index space of the leg.
    pub space: &'a EmbeddingSpaceId,
    /// Query model; must belong to `space`.
    pub embedder: &'a dyn EmbeddingStrategy,
}

impl std::fmt::Debug for Leg<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Leg")
            .field("space", self.space)
            .field("model", &self.embedder.model_info().model_id)
            .finish()
    }
}

/// Fuses a text ranking and an image ranking (each best first) with RRF and returns
/// the top `limit` fused results.
///
/// A result's `rank` is its fused rank and its `score` the fused score; `fusion` keeps
/// the exact fused score and the rank and cosine score it held in each leg. The legs'
/// own scores never enter the fusion, only their positions.
#[must_use]
pub fn fuse(
    text: &[SearchHit],
    image: &[SearchHit],
    params: FusionParams,
    limit: usize,
) -> Vec<SearchHit> {
    let keys = |hits: &[SearchHit]| -> Vec<RecordId> {
        hits.iter().map(|h| h.record_id.clone()).collect()
    };
    let (text_keys, image_keys) = (keys(text), keys(image));
    let lookup = |hits: &[SearchHit], rank: Option<usize>| -> Option<(SearchHit, LegTrace)> {
        let rank = rank?;
        let hit = hits.get(rank.checked_sub(1)?)?;
        Some((
            hit.clone(),
            LegTrace {
                rank,
                score: hit.score,
            },
        ))
    };
    rrf(&[&text_keys, &image_keys], params)
        .into_iter()
        .take(limit)
        .enumerate()
        .filter_map(|(i, fused)| {
            let rank_in = |leg: usize| fused.ranks.get(leg).copied().flatten();
            let from_text = lookup(text, rank_in(0));
            let from_image = lookup(image, rank_in(1));
            let (hit, _) = from_text.clone().or_else(|| from_image.clone())?;
            #[expect(
                clippy::cast_possible_truncation,
                reason = "an RRF score is at most the number of legs divided by k + 1"
            )]
            let score = fused.score as f32;
            Some(SearchHit {
                rank: i + 1,
                score,
                fusion: Some(FusionTrace {
                    fused_score: fused.score,
                    text: from_text.map(|(_, t)| t),
                    image: from_image.map(|(_, t)| t),
                }),
                ..hit
            })
        })
        .collect()
}

/// Snapshot key of the fusion fingerprint of a combined run.
pub const FUSION_FINGERPRINT: &str = "fusion";

/// Refuses a snapshot whose recorded fusion parameters are not `params` (a combined
/// run records them under [`FUSION_FINGERPRINT`]; any other snapshot records none).
///
/// # Errors
///
/// Returns [`SearchError::SnapshotMismatch`].
pub fn check_fusion_pinned(
    snapshot: &ExperimentSnapshot,
    params: FusionParams,
) -> Result<(), SearchError> {
    let mismatch = |reason: String| SearchError::SnapshotMismatch {
        experiment: snapshot.experiment_id.clone(),
        reason,
    };
    let expected = Fingerprint::of(&params).map_err(|e| mismatch(e.to_string()))?;
    match snapshot.fingerprints.get(FUSION_FINGERPRINT) {
        Some(pinned) if *pinned == expected => Ok(()),
        Some(_) => Err(mismatch(format!(
            "it was fused with other parameters than {} k = {}",
            params.method.as_str(),
            params.k
        ))),
        None => Err(mismatch("it is not a combined run".to_owned())),
    }
}

/// Refuses a snapshot that is not the combined run of `text`, `image`, and `params`:
/// its fusion parameters must be `params` and it must pin revisions in both spaces, so
/// a leg can never silently drop out of the fusion.
///
/// # Errors
///
/// Returns [`SearchError::SnapshotMismatch`].
pub fn check_combined_snapshot(
    snapshot: &ExperimentSnapshot,
    text: &EmbeddingSpaceId,
    image: &EmbeddingSpaceId,
    params: FusionParams,
) -> Result<(), SearchError> {
    check_fusion_pinned(snapshot, params)?;
    for (leg, space) in [("text", text), ("image", image)] {
        if !snapshot.revisions.iter().any(|r| &r.space == space) {
            return Err(SearchError::SnapshotMismatch {
                experiment: snapshot.experiment_id.clone(),
                reason: format!("it pins no revision in the {leg} leg's space {space}"),
            });
        }
    }
    Ok(())
}

/// Combined search (PRD §6.6): the text leg ranks the text records of its space, the
/// image leg the image records of its cross-modal space, each with its own query
/// model; each leg contributes its top `params.depth` candidates and the rankings are
/// fused with RRF. Both legs search the same `scope`; an experiment scope must be the
/// combined run of these legs and parameters ([`check_combined_snapshot`]).
///
/// # Errors
///
/// Before any store read or query embedding, returns [`SearchError::SpaceMismatch`]
/// when a leg's query model does not belong to its index space,
/// [`SearchError::Embedding`] with [`EmbeddingError::Unsupported`] when the image
/// leg's model has no compatible text-to-image space, and
/// [`SearchError::SnapshotMismatch`] when the experiment scope is not the combined
/// run of these legs and fusion parameters. Otherwise returns the errors of
/// [`search_text`] and [`search_images`].
pub async fn search_combined(
    store: &Arc<FsRecordStore>,
    text: Leg<'_>,
    image: Leg<'_>,
    params: FusionParams,
    query: &str,
    limit: usize,
    scope: Scope<'_>,
) -> Result<Vec<SearchHit>, SearchError> {
    guard_space(text.space, &text.embedder.model_info().embedding_space_id)?;
    guard_space(image.space, &image.embedder.model_info().embedding_space_id)?;
    image
        .embedder
        .capabilities()
        .require(Capability::TextToImage, image.embedder.model_info())?;
    if let Scope::Experiment(snapshot) = scope {
        check_combined_snapshot(snapshot, text.space, image.space, params)?;
    }
    let depth = params.depth.limit();
    let (text_hits, image_hits) = tokio::try_join!(
        search_text(store, text.space, text.embedder, query, depth, scope),
        search_images(store, image.space, image.embedder, query, depth, scope),
    )?;
    Ok(fuse(&text_hits, &image_hits, params, limit))
}
