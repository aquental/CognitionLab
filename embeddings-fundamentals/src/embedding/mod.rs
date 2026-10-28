//! Embedding strategy contract (PLAN §2.2) and vector validation.
//!
//! `EmbeddingStrategy` is the **only** trait in the crate that uses `#[async_trait]`.
//! It is consumed as `Arc<dyn EmbeddingStrategy>` because the adapter is chosen from
//! configuration at runtime, and combined retrieval holds two adapters at once.
//! Native `async fn` in traits is not dyn-compatible, so this trait boxes its futures.

pub mod deterministic;
#[cfg(feature = "engine-embed-anything")]
pub mod embed_anything;
#[cfg(feature = "engine-fastembed")]
pub mod fastembed;
#[cfg(any(feature = "engine-embed-anything", feature = "engine-fastembed"))]
pub mod hf;
pub mod local;
#[cfg(feature = "engine-openai")]
pub mod openai;

use std::borrow::Cow;
use std::fmt;
use std::num::NonZeroUsize;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::domain::Execution;
use crate::ids::{EmbeddingSpaceId, IdError, ItemId};

/// Canonical engine identifiers used in records, configuration, and reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineId {
    /// EmbedAnything (candle runtime).
    EmbedAnything,
    /// FastEmbed (ONNX runtime).
    #[serde(rename = "fastembed")]
    FastEmbed,
    /// OpenAI embeddings API (remote).
    #[serde(rename = "openai")]
    OpenAi,
    /// Hash-based adapter for deterministic tests.
    Deterministic,
}

impl EngineId {
    /// Canonical identifier, for example `embed_anything`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EmbedAnything => "embed_anything",
            Self::FastEmbed => "fastembed",
            Self::OpenAi => "openai",
            Self::Deterministic => "deterministic",
        }
    }

    /// Maps a Gherkin display name (`EmbedAnything`, `FastEmbed`, `OpenAI`) or a
    /// canonical identifier to an engine.
    #[must_use]
    pub fn from_display(name: &str) -> Option<Self> {
        match name {
            "EmbedAnything" | "embed_anything" => Some(Self::EmbedAnything),
            "FastEmbed" | "fastembed" => Some(Self::FastEmbed),
            "OpenAI" | "openai" => Some(Self::OpenAi),
            "Deterministic" | "deterministic" => Some(Self::Deterministic),
            _ => None,
        }
    }
}

impl fmt::Display for EngineId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Whether text is embedded as a stored document or as a search query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputKind {
    /// Indexed document text.
    Document,
    /// Search query text.
    Query,
}

/// Model-specific text prefixes, applied exactly once inside the adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrefixPolicy {
    /// No prefix (CLIP, SigLIP, OpenAI).
    None,
    /// E5 family: `query: ` for queries, `passage: ` for documents.
    E5,
}

impl PrefixPolicy {
    /// The prefix this policy adds for `kind`.
    #[must_use]
    pub const fn prefix(self, kind: InputKind) -> &'static str {
        match (self, kind) {
            (Self::None, _) => "",
            (Self::E5, InputKind::Query) => "query: ",
            (Self::E5, InputKind::Document) => "passage: ",
        }
    }

    /// Returns the text submitted to the model for `raw` input.
    #[must_use]
    pub fn apply(self, kind: InputKind, raw: &str) -> Cow<'_, str> {
        match self.prefix(kind) {
            "" => Cow::Borrowed(raw),
            prefix => Cow::Owned(format!("{prefix}{raw}")),
        }
    }
}

/// Similarity metric of an embedding space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistanceMetric {
    /// Cosine similarity (dot product of L2-normalized vectors).
    Cosine,
}

/// A capability an adapter may lack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// Embedding document text.
    TextDocuments,
    /// Embedding query text.
    TextQueries,
    /// Embedding images.
    Images,
    /// Text queries and images share one compatible space.
    TextToImage,
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TextDocuments => "text-document embedding",
            Self::TextQueries => "text-query embedding",
            Self::Images => "image embedding",
            Self::TextToImage => "compatible text-to-image retrieval",
        })
    }
}

/// What an adapter can do and under which constraints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    /// Can embed document text.
    pub text_documents: bool,
    /// Can embed query text.
    pub text_queries: bool,
    /// Can embed images.
    pub images: bool,
    /// Text queries and images live in one compatible space.
    pub text_to_image_compatible: bool,
    /// Accepts more than one input per call.
    pub batch: bool,
    /// Maximum input length in the model's own tokens.
    pub max_input_tokens: Option<NonZeroUsize>,
    /// Maximum number of inputs per call.
    pub max_batch: Option<NonZeroUsize>,
    /// Local or remote execution.
    pub execution: Execution,
}

impl Capabilities {
    /// Largest number of inputs one call may carry: 1 without batch support, otherwise
    /// `max_batch` (unbounded when unset).
    #[must_use]
    pub fn batch_limit(&self) -> Option<usize> {
        if self.batch {
            self.max_batch.map(NonZeroUsize::get)
        } else {
            Some(1)
        }
    }

    /// Rejects a call of `size` inputs that exceeds [`Capabilities::batch_limit`].
    ///
    /// # Errors
    ///
    /// Returns [`EmbeddingError::BatchTooLarge`] when `size` is above the limit.
    pub fn check_batch(&self, size: usize) -> Result<(), EmbeddingError> {
        match self.batch_limit() {
            Some(max) if size > max => Err(EmbeddingError::BatchTooLarge { size, max }),
            _ => Ok(()),
        }
    }

    /// Returns an error naming the engine, model, and missing capability when
    /// `capability` is not supported.
    ///
    /// # Errors
    ///
    /// Returns [`EmbeddingError::Unsupported`] when the capability is missing.
    pub fn require(&self, capability: Capability, info: &ModelInfo) -> Result<(), EmbeddingError> {
        let supported = match capability {
            Capability::TextDocuments => self.text_documents,
            Capability::TextQueries => self.text_queries,
            Capability::Images => self.images,
            Capability::TextToImage => self.text_to_image_compatible,
        };
        if supported {
            Ok(())
        } else {
            Err(EmbeddingError::Unsupported {
                engine: info.engine,
                model: info.model_id.clone(),
                capability,
            })
        }
    }
}

/// Everything that determines whether two vectors are comparable.
///
/// Its canonical JSON is hashed into [`EmbeddingSpaceId`], so it must contain every
/// setting that changes the vectors and nothing that does not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpaceDescriptor {
    /// Engine that runs the model.
    pub engine: EngineId,
    /// Engine version.
    pub engine_version: String,
    /// Model identifier, for example `intfloat/multilingual-e5-base`.
    pub model_id: String,
    /// Immutable model revision (commit hash or digest).
    pub model_revision: Option<String>,
    /// Weight format, for example `safetensors` or `onnx`.
    pub weight_format: Option<String>,
    /// Vector dimensions.
    pub dimensions: NonZeroUsize,
    /// Whether the adapter L2-normalizes vectors.
    pub normalized: bool,
    /// Similarity metric.
    pub metric: DistanceMetric,
    /// Prefix policy applied inside the adapter.
    pub prefix_policy: PrefixPolicy,
    /// Any other preprocessing that changes vectors (image size, pooling, ...).
    pub preprocessing: String,
}

/// Model identity and its derived embedding space.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Engine that runs the model.
    pub engine: EngineId,
    /// Engine version.
    pub engine_version: String,
    /// Model identifier.
    pub model_id: String,
    /// Immutable model revision, required for real-model runs.
    pub model_revision: Option<String>,
    /// Weight format.
    pub weight_format: Option<String>,
    /// Vector dimensions.
    pub dimensions: NonZeroUsize,
    /// Whether vectors are L2-normalized.
    pub normalized: bool,
    /// Similarity metric.
    pub metric: DistanceMetric,
    /// Prefix policy applied inside the adapter.
    pub prefix_policy: PrefixPolicy,
    /// Preprocessing description included in the space identity.
    pub preprocessing: String,
    /// Identity derived from all fields above.
    pub embedding_space_id: EmbeddingSpaceId,
}

impl ModelInfo {
    /// Builds model information and derives its embedding space identity.
    ///
    /// # Errors
    ///
    /// Returns [`IdError`] if the descriptor cannot be hashed.
    pub fn from_descriptor(descriptor: SpaceDescriptor) -> Result<Self, IdError> {
        let embedding_space_id = EmbeddingSpaceId::derive(&descriptor)?;
        let SpaceDescriptor {
            engine,
            engine_version,
            model_id,
            model_revision,
            weight_format,
            dimensions,
            normalized,
            metric,
            prefix_policy,
            preprocessing,
        } = descriptor;
        Ok(Self {
            engine,
            engine_version,
            model_id,
            model_revision,
            weight_format,
            dimensions,
            normalized,
            metric,
            prefix_policy,
            preprocessing,
            embedding_space_id,
        })
    }
}

/// One raw text input. Adapters apply their [`PrefixPolicy`]; callers never do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextInput {
    /// Identifier echoed back in the matching [`Embedding`].
    pub id: ItemId,
    /// Raw text, exactly as produced by chunking or typed by the user.
    pub text: String,
}

/// A reference to an image asset to embed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetRef {
    /// Identifier echoed back in the matching [`Embedding`].
    pub id: ItemId,
    /// Path of the prepared (metadata-free) asset.
    pub path: PathBuf,
}

/// One vector returned by an adapter.
#[derive(Debug, Clone, PartialEq)]
pub struct Embedding {
    /// Identifier of the input this vector belongs to.
    pub id: ItemId,
    /// The vector.
    pub vector: Vec<f32>,
}

/// Provider usage reported for a call, used for cost estimates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    /// Tokens billed by the provider.
    pub tokens: u64,
}

/// The vectors returned for one call, in input order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EmbeddingBatch {
    /// One embedding per input, in input order.
    pub embeddings: Vec<Embedding>,
    /// Provider usage, when the provider reports it.
    pub usage: Option<Usage>,
}

/// Whether a failure may succeed when retried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorClass {
    /// Timeouts, rate limits, 5xx, connection resets, model-load I/O.
    Transient,
    /// Everything else; never retried.
    Permanent,
}

/// Errors returned by embedding adapters. Messages never contain input text.
#[derive(Debug, thiserror::Error)]
pub enum EmbeddingError {
    /// The adapter lacks a required capability.
    #[error("engine {engine} with model {model} does not support {capability}")]
    Unsupported {
        /// Engine identifier.
        engine: EngineId,
        /// Model identifier.
        model: String,
        /// Missing capability.
        capability: Capability,
    },
    /// A failure that may succeed when retried.
    #[error("transient embedding failure: {reason}")]
    Transient {
        /// Short reason without document content.
        reason: String,
    },
    /// A failure that will not succeed when retried.
    #[error("permanent embedding failure: {reason}")]
    Permanent {
        /// Short reason without document content.
        reason: String,
    },
    /// A call contains more inputs than the adapter accepts at once.
    #[error("batch of {size} inputs exceeds the adapter maximum of {max}")]
    BatchTooLarge {
        /// Number of inputs in the call.
        size: usize,
        /// Maximum accepted (1 when the adapter does not support batching).
        max: usize,
    },
    /// An input is longer than the model accepts.
    #[error("input {item} has {tokens} tokens, above the model limit of {limit}")]
    InputTooLong {
        /// Offending input.
        item: ItemId,
        /// Token count of the submitted text.
        tokens: usize,
        /// Model limit.
        limit: usize,
    },
}

impl EmbeddingError {
    /// Retry classification of this error.
    #[must_use]
    pub const fn class(&self) -> ErrorClass {
        match self {
            Self::Transient { .. } => ErrorClass::Transient,
            Self::Unsupported { .. }
            | Self::Permanent { .. }
            | Self::BatchTooLarge { .. }
            | Self::InputTooLong { .. } => ErrorClass::Permanent,
        }
    }
}

/// Counts tokens with a model's own tokenizer.
pub trait TokenCounter: Send + Sync {
    /// Number of model tokens in `text`, excluding special tokens.
    fn count(&self, text: &str) -> usize;

    /// Content identity of the tokenizer (for example a digest of `tokenizer.json`).
    /// It enters the chunking fingerprint, because chunk boundaries depend on the
    /// tokenizer, not on the model that later embeds the chunks.
    fn id(&self) -> &str;
}

/// The embedding strategy contract (PRD §7).
#[async_trait::async_trait]
pub trait EmbeddingStrategy: Send + Sync {
    /// Supported modalities and constraints.
    fn capabilities(&self) -> &Capabilities;

    /// Model identity and embedding space.
    fn model_info(&self) -> &ModelInfo;

    /// The model's tokenizer.
    fn token_counter(&self) -> &dyn TokenCounter;

    /// Tokens available for raw text of `kind`: the model limit minus the prefix
    /// and special tokens. `None` when the model has no input limit.
    fn input_budget(&self, kind: InputKind) -> Option<usize>;

    /// Embeds document text. The adapter applies its prefix policy.
    async fn embed_documents(&self, raw: &[TextInput]) -> Result<EmbeddingBatch, EmbeddingError>;

    /// Embeds query text through the model's query path.
    async fn embed_queries(&self, raw: &[TextInput]) -> Result<EmbeddingBatch, EmbeddingError>;

    /// Embeds images when supported.
    async fn embed_images(&self, assets: &[AssetRef]) -> Result<EmbeddingBatch, EmbeddingError>;
}

/// A defect found in an adapter response.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum VectorDefect {
    /// The vector length differs from the model dimensions.
    #[error("item {item}: incorrect dimensions (expected {expected}, got {actual})")]
    WrongDimensions {
        /// Affected item.
        item: ItemId,
        /// Expected dimensions.
        expected: usize,
        /// Actual length.
        actual: usize,
    },
    /// A component is NaN.
    #[error("item {item}: NaN component at index {index}")]
    NaN {
        /// Affected item.
        item: ItemId,
        /// Component index.
        index: usize,
    },
    /// A component is infinite.
    #[error("item {item}: infinite component at index {index}")]
    Infinite {
        /// Affected item.
        item: ItemId,
        /// Component index.
        index: usize,
    },
    /// The response has a different number of vectors than inputs.
    #[error("response does not match inputs: expected {expected} items, got {actual}")]
    CountMismatch {
        /// Number of inputs.
        expected: usize,
        /// Number of vectors returned.
        actual: usize,
    },
    /// The vector at `index` belongs to a different item than the input at `index`.
    #[error("vector {index} is for item {actual}, expected item {expected}")]
    ItemOrder {
        /// Position in the batch.
        index: usize,
        /// Item id of the input at that position.
        expected: ItemId,
        /// Item id found in the response at that position.
        actual: ItemId,
    },
}

/// Checks every vector before anything is committed: one vector per input in input
/// order, correct dimensions, and only finite components.
///
/// # Errors
///
/// Returns the first [`VectorDefect`] found.
pub fn validate_batch(
    batch: &EmbeddingBatch,
    expected_items: &[ItemId],
    info: &ModelInfo,
) -> Result<(), VectorDefect> {
    if batch.embeddings.len() != expected_items.len() {
        return Err(VectorDefect::CountMismatch {
            expected: expected_items.len(),
            actual: batch.embeddings.len(),
        });
    }
    for (index, (embedding, expected)) in batch.embeddings.iter().zip(expected_items).enumerate() {
        if &embedding.id != expected {
            return Err(VectorDefect::ItemOrder {
                index,
                expected: expected.clone(),
                actual: embedding.id.clone(),
            });
        }
    }
    let expected = info.dimensions.get();
    for embedding in &batch.embeddings {
        if embedding.vector.len() != expected {
            return Err(VectorDefect::WrongDimensions {
                item: embedding.id.clone(),
                expected,
                actual: embedding.vector.len(),
            });
        }
        for (index, component) in embedding.vector.iter().enumerate() {
            if component.is_nan() {
                return Err(VectorDefect::NaN {
                    item: embedding.id.clone(),
                    index,
                });
            }
            if component.is_infinite() {
                return Err(VectorDefect::Infinite {
                    item: embedding.id.clone(),
                    index,
                });
            }
        }
    }
    Ok(())
}

/// L2-normalizes `vector` in place. A zero vector is left unchanged.
///
/// Components are scaled by the largest magnitude and accumulated in `f64`, so very
/// large components do not overflow and very small non-zero vectors still reach unit
/// length. Non-finite inputs are left for [`validate_batch`] to reject.
pub fn l2_normalize(vector: &mut [f32]) {
    let max_abs = vector
        .iter()
        .fold(0.0_f64, |m, x| m.max(f64::from(x.abs())));
    if !(max_abs > 0.0 && max_abs.is_finite()) {
        return;
    }
    let norm = vector
        .iter()
        .map(|x| {
            let scaled = f64::from(*x) / max_abs;
            scaled * scaled
        })
        .sum::<f64>()
        .sqrt()
        * max_abs;
    for x in vector.iter_mut() {
        // The quotient has magnitude at most 1, so narrowing back to f32 is lossless in range.
        #[expect(clippy::cast_possible_truncation, reason = "value is in [-1, 1]")]
        let unit = (f64::from(*x) / norm) as f32;
        *x = unit;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(dims: usize) -> ModelInfo {
        ModelInfo::from_descriptor(SpaceDescriptor {
            engine: EngineId::Deterministic,
            engine_version: "test".into(),
            model_id: "fixture".into(),
            model_revision: None,
            weight_format: None,
            dimensions: NonZeroUsize::new(dims).expect("dims"),
            normalized: true,
            metric: DistanceMetric::Cosine,
            prefix_policy: PrefixPolicy::None,
            preprocessing: String::new(),
        })
        .expect("info")
    }

    fn item(s: &str) -> ItemId {
        s.parse().expect("item id")
    }

    fn batch(vectors: Vec<Vec<f32>>) -> (EmbeddingBatch, Vec<ItemId>) {
        let ids: Vec<ItemId> = (0..vectors.len()).map(|i| item(&format!("i{i}"))).collect();
        let embeddings = ids
            .iter()
            .cloned()
            .zip(vectors)
            .map(|(id, vector)| Embedding { id, vector })
            .collect();
        (
            EmbeddingBatch {
                embeddings,
                usage: None,
            },
            ids,
        )
    }

    #[test]
    fn valid_batch_passes() {
        let (b, ids) = batch(vec![vec![0.6, 0.8], vec![1.0, 0.0]]);
        assert_eq!(validate_batch(&b, &ids, &info(2)), Ok(()));
    }

    #[test]
    fn wrong_dimensions_name_the_item() {
        let (b, ids) = batch(vec![vec![1.0, 0.0], vec![1.0, 0.0, 0.0]]);
        assert_eq!(
            validate_batch(&b, &ids, &info(2)),
            Err(VectorDefect::WrongDimensions {
                item: item("i1"),
                expected: 2,
                actual: 3
            })
        );
    }

    #[test]
    fn nan_and_infinite_components_are_rejected() {
        let (b, ids) = batch(vec![vec![f32::NAN, 0.0]]);
        assert_eq!(
            validate_batch(&b, &ids, &info(2)),
            Err(VectorDefect::NaN {
                item: item("i0"),
                index: 0
            })
        );
        let (b, ids) = batch(vec![vec![0.0, f32::NEG_INFINITY]]);
        assert_eq!(
            validate_batch(&b, &ids, &info(2)),
            Err(VectorDefect::Infinite {
                item: item("i0"),
                index: 1
            })
        );
    }

    #[test]
    fn missing_or_reordered_items_are_rejected() {
        let (b, mut ids) = batch(vec![vec![1.0, 0.0], vec![0.0, 1.0]]);
        ids.reverse();
        let err = validate_batch(&b, &ids, &info(2)).expect_err("reordered ids");
        assert_eq!(
            err,
            VectorDefect::ItemOrder {
                index: 0,
                expected: item("i1"),
                actual: item("i0")
            }
        );
        assert_eq!(err.to_string(), "vector 0 is for item i0, expected item i1");
        let (b, _) = batch(vec![vec![1.0, 0.0]]);
        let two = vec![item("i0"), item("i1")];
        assert!(matches!(
            validate_batch(&b, &two, &info(2)),
            Err(VectorDefect::CountMismatch {
                expected: 2,
                actual: 1
            })
        ));
    }

    #[test]
    fn e5_prefix_is_applied_once_per_kind() {
        assert_eq!(
            PrefixPolicy::E5.apply(InputKind::Query, "hemoglobina"),
            "query: hemoglobina"
        );
        assert_eq!(
            PrefixPolicy::E5.apply(InputKind::Document, "hemoglobina"),
            "passage: hemoglobina"
        );
        assert_eq!(PrefixPolicy::None.apply(InputKind::Query, "tórax"), "tórax");
    }

    #[test]
    fn equal_dimensions_do_not_imply_equal_spaces() {
        let a = info(4);
        let mut descriptor_b = SpaceDescriptor {
            engine: EngineId::Deterministic,
            engine_version: "test".into(),
            model_id: "other".into(),
            model_revision: None,
            weight_format: None,
            dimensions: NonZeroUsize::new(4).expect("dims"),
            normalized: true,
            metric: DistanceMetric::Cosine,
            prefix_policy: PrefixPolicy::None,
            preprocessing: String::new(),
        };
        let b = ModelInfo::from_descriptor(descriptor_b.clone()).expect("info");
        assert_eq!(a.dimensions, b.dimensions);
        assert_ne!(a.embedding_space_id, b.embedding_space_id);
        descriptor_b.prefix_policy = PrefixPolicy::E5;
        let c = ModelInfo::from_descriptor(descriptor_b).expect("info");
        assert_ne!(b.embedding_space_id, c.embedding_space_id);
    }

    #[test]
    fn embedding_space_id_is_frozen_for_known_descriptor() {
        let info = ModelInfo::from_descriptor(SpaceDescriptor {
            engine: EngineId::Deterministic,
            engine_version: "0.1.0".into(),
            model_id: "fixture-model".into(),
            model_revision: Some("abc123".into()),
            weight_format: None,
            dimensions: NonZeroUsize::new(8).expect("dims"),
            normalized: true,
            metric: DistanceMetric::Cosine,
            prefix_policy: PrefixPolicy::E5,
            preprocessing: "sha256-seeded".into(),
        })
        .expect("info");
        // Golden value computed independently with Python over the same field set
        // (canonical JSON, sha256, first 32 hex characters, `space-` prefix; PLAN §2.1).
        assert_eq!(
            info.embedding_space_id.as_str(),
            "space-35007f7bfcdac3ce4c66ebe17fa1793b"
        );
    }

    #[test]
    fn engine_display_names_map_to_canonical_ids() {
        assert_eq!(
            EngineId::from_display("EmbedAnything"),
            Some(EngineId::EmbedAnything)
        );
        assert_eq!(
            EngineId::from_display("FastEmbed"),
            Some(EngineId::FastEmbed)
        );
        assert_eq!(EngineId::from_display("OpenAI"), Some(EngineId::OpenAi));
        assert_eq!(EngineId::OpenAi.as_str(), "openai");
        assert_eq!(
            serde_json::to_string(&EngineId::EmbedAnything).expect("json"),
            "\"embed_anything\""
        );
        assert_eq!(
            serde_json::to_string(&EngineId::FastEmbed).expect("json"),
            "\"fastembed\""
        );
        assert_eq!(EngineId::from_display("Unknown"), None);
    }

    #[test]
    fn transient_errors_are_the_only_retryable_class() {
        assert_eq!(
            EmbeddingError::Transient {
                reason: "429".into()
            }
            .class(),
            ErrorClass::Transient
        );
        assert_eq!(
            EmbeddingError::Permanent {
                reason: "400".into()
            }
            .class(),
            ErrorClass::Permanent
        );
    }

    #[test]
    fn l2_normalize_produces_unit_vectors() {
        let mut v = vec![3.0, 4.0];
        l2_normalize(&mut v);
        approx::assert_relative_eq!(v[0], 0.6, epsilon = 1e-6);
        approx::assert_relative_eq!(v[1], 0.8, epsilon = 1e-6);
        let mut zero = vec![0.0, 0.0];
        l2_normalize(&mut zero);
        assert!(zero.iter().all(|x| x.abs() <= f32::EPSILON));
    }

    #[test]
    fn l2_normalize_handles_extreme_magnitudes() {
        for input in [
            vec![1e20_f32, 0.0],
            vec![1e-8_f32, 0.0],
            vec![f32::MAX, f32::MAX],
        ] {
            let mut v = input.clone();
            l2_normalize(&mut v);
            let norm = v.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>().sqrt();
            approx::assert_abs_diff_eq!(norm, 1.0, epsilon = 1e-6);
            assert!(
                v.iter().all(|x| x.is_finite()),
                "{input:?} normalized to {v:?}"
            );
        }
        // IEEE 754 floats are sign-symmetric: f32::MIN.abs() == f32::MAX (unlike integers).
        assert_eq!(f32::MIN.abs().to_bits(), f32::MAX.to_bits());
        for input in [vec![f32::MIN, 0.0], vec![f32::MIN, f32::MIN]] {
            let mut v = input.clone();
            l2_normalize(&mut v);
            let norm = v.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>().sqrt();
            approx::assert_abs_diff_eq!(norm, 1.0, epsilon = 1e-6);
            assert!(v[0] < 0.0, "{input:?} normalized to {v:?}");
        }
        let mut tiny = vec![1e-8_f32, 0.0];
        l2_normalize(&mut tiny);
        approx::assert_abs_diff_eq!(tiny[0], 1.0, epsilon = 1e-6);
    }
}
