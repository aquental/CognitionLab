//! Indexed record schema (PRD §8).
//!
//! Each record is one searchable item in one embedding space. Records are immutable
//! after commit.

use serde::{Deserialize, Serialize};

use crate::chunking::{ChunkKind, Truncation};
use crate::domain::{DocumentType, ExtractionMethod, Modality};
use crate::embedding::{DistanceMetric, EngineId, ModelInfo};
use crate::extraction::BoundingBox;
use crate::ids::{
    Checksum, ChunkId, EmbeddingSpaceId, ExperimentId, RecordId, RevisionId, SourceId,
};

/// Source, page, language, and extraction details of a record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordMetadata {
    /// Source file name (relative to the manifest).
    pub source_file: String,
    /// Checksum of the source file.
    pub source_checksum: Checksum,
    /// Document kind.
    pub document_type: DocumentType,
    /// Language tag of the text.
    pub language: String,
    /// 1-based page number.
    pub page_number: u32,
    /// Section heading in effect.
    pub section: Option<String>,
    /// 0-based chunk order within the page.
    pub chunk_index: u32,
    /// Structural kind of the chunk.
    pub chunk_kind: ChunkKind,
    /// Chunk box in PDF points (top-left origin), when available.
    pub bounding_box: Option<BoundingBox>,
    /// How the text was obtained.
    pub extraction_method: ExtractionMethod,
    /// Extraction tool and version.
    pub extraction_engine: String,
    /// Extraction confidence in `[0, 1]`, or `null` when not supplied.
    pub extraction_confidence: Option<f32>,
    /// Chunking strategy identifier.
    pub chunking_strategy: String,
    /// Tokens shared with the previous chunk.
    pub overlap_tokens: usize,
    /// Present when the submitted text was truncated.
    pub truncation: Option<Truncation>,
    /// `verified` once the anonymization gate passed.
    pub anonymization_status: String,
}

/// Vector and model provenance of a record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordEmbedding {
    /// Engine identifier.
    pub engine: EngineId,
    /// Engine version.
    pub engine_version: String,
    /// Model identifier.
    pub model_id: String,
    /// Model revision.
    pub model_revision: Option<String>,
    /// Vector dimensions.
    pub dimensions: usize,
    /// Embedding space identity.
    pub embedding_space_id: EmbeddingSpaceId,
    /// Whether the vector is L2-normalized.
    pub normalized: bool,
    /// Similarity metric.
    pub distance_metric: DistanceMetric,
    /// The complete vector.
    pub vector: Vec<f32>,
}

impl RecordEmbedding {
    /// Provenance of `vector` produced by the model described in `info`.
    #[must_use]
    pub fn new(info: &ModelInfo, vector: Vec<f32>) -> Self {
        Self {
            engine: info.engine,
            engine_version: info.engine_version.clone(),
            model_id: info.model_id.clone(),
            model_revision: info.model_revision.clone(),
            dimensions: info.dimensions.get(),
            embedding_space_id: info.embedding_space_id.clone(),
            normalized: info.normalized,
            distance_metric: info.metric,
            vector,
        }
    }
}

/// One indexed record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    /// Unique record identifier.
    pub record_id: RecordId,
    /// Chunk identifier within the revision.
    pub chunk_id: ChunkId,
    /// Source document identifier.
    pub source_id: SourceId,
    /// Immutable source revision.
    pub revision_id: RevisionId,
    /// Extracted text; `null` for image records.
    pub chunk_text: Option<String>,
    /// Text or image.
    pub modality: Modality,
    /// Image asset reference; `null` for text records.
    pub asset_uri: Option<String>,
    /// Source and extraction details.
    pub metadata: RecordMetadata,
    /// Vector and model provenance.
    pub embedding: RecordEmbedding,
    /// Experiment run that produced this record.
    pub experiment_id: ExperimentId,
}
