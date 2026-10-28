//! EmbedAnything adapter (candle, safetensors), behind feature `engine-embed-anything`.
//!
//! Loads a `BertModel`-architecture sentence model at a pinned revision with mean
//! pooling. Token counting uses the model's own `tokenizer.json` from the Hugging Face
//! cache, which EmbedAnything fills when it downloads the model. The shared
//! [`LocalTextAdapter`] enforces the input limit before text reaches the engine, so
//! EmbedAnything's internal truncation never applies, and normalizes the vectors.

use std::num::NonZeroUsize;
use std::sync::Arc;

use embed_anything::embeddings::embed::{Embedder, EmbeddingResult};
use embed_anything::embeddings::local::pooling::Pooling;

use super::hf::{self, HfError, HfTokenCounter};
use super::local::{LocalTextAdapter, TextBackend};
use super::{Capabilities, DistanceMetric, EmbeddingError, EngineId, ModelInfo, SpaceDescriptor};
use crate::domain::Execution;

/// EmbedAnything crate version pinned in `Cargo.lock` (recorded in model provenance).
pub const ENGINE_VERSION: &str = "0.7.1";

/// The EmbedAnything adapter: the shared local contract over a candle embedder.
pub type EmbedAnythingAdapter = LocalTextAdapter<EmbedAnythingBackend, super::hf::HfTokenCounter>;

/// Errors raised while loading the model.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// Real engines must pin an immutable model revision (PRD §6.4).
    #[error("model {model} has no pinned revision")]
    MissingRevision {
        /// Model identifier.
        model: String,
    },
    /// The revision is a branch or tag, not an immutable commit id (PRD §6.4).
    #[error("model {model}: revision {revision:?} is not an immutable commit id")]
    MutableRevision {
        /// Model identifier.
        model: String,
        /// The given revision.
        revision: String,
    },
    /// The model could not be downloaded or loaded.
    #[error("cannot load model {model}: {reason}")]
    Model {
        /// Model identifier.
        model: String,
        /// Reason.
        reason: String,
    },
    /// A model file (tokenizer, config, limit) is unusable.
    #[error(transparent)]
    Files(#[from] HfError),
    /// The model returns vectors of another size than configured.
    #[error("model {model} returns {actual}-d vectors, config says {expected}")]
    Dimensions {
        /// Model identifier.
        model: String,
        /// Configured dimensions.
        expected: usize,
        /// Observed dimensions.
        actual: usize,
    },
}

/// The candle embedder behind [`EmbedAnythingAdapter`].
pub struct EmbedAnythingBackend {
    embedder: Arc<Embedder>,
}

fn dense(result: EmbeddingResult) -> Result<Vec<f32>, EmbeddingError> {
    match result {
        EmbeddingResult::DenseVector(v) => Ok(v),
        EmbeddingResult::MultiVector(_) => Err(EmbeddingError::Permanent {
            reason: "model returned a multi-vector embedding".to_owned(),
        }),
    }
}

impl TextBackend for EmbedAnythingBackend {
    fn embed_blocking(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        let batch = refs.len();
        // Called on the blocking pool, where the runtime handle is available.
        tokio::runtime::Handle::current()
            .block_on(self.embedder.embed(&refs, Some(batch), None))
            .map_err(|e| EmbeddingError::Permanent {
                reason: format!("inference failed: {e}"),
            })?
            .into_iter()
            .map(dense)
            .collect()
    }
}

/// Downloads (if needed) and loads the model and its tokenizer, then checks the
/// vector size with a probe embedding.
///
/// # Errors
///
/// Returns [`LoadError`] when the revision is missing or the model, tokenizer, limit,
/// or dimensions do not load as configured.
pub async fn load(
    model_id: &str,
    model_revision: Option<&str>,
    dimensions: NonZeroUsize,
    max_input_tokens: Option<NonZeroUsize>,
    prefix_policy: super::PrefixPolicy,
) -> Result<EmbedAnythingAdapter, LoadError> {
    let revision = model_revision
        .ok_or_else(|| LoadError::MissingRevision {
            model: model_id.to_owned(),
        })?
        .to_owned();
    if !super::is_immutable_hub_revision(&revision) {
        return Err(LoadError::MutableRevision {
            model: model_id.to_owned(),
            revision,
        });
    }
    let model_error = |reason: String| LoadError::Model {
        model: model_id.to_owned(),
        reason,
    };
    let (model, rev) = (model_id.to_owned(), revision.clone());
    let embedder = tokio::task::spawn_blocking(move || {
        Embedder::from_pretrained_hf(&model, Some(&rev), None, None, Some(Pooling::Mean))
    })
    .await
    .map_err(|e| model_error(e.to_string()))?
    .map_err(|e| model_error(e.to_string()))?;
    // Tokenizer deserialization and config reads are blocking file work (C2-8).
    let (model, rev) = (model_id.to_owned(), revision.clone());
    let (tokens, max_input_tokens) = tokio::task::spawn_blocking(move || {
        let tokens = HfTokenCounter::load(&hf::cached_file(&model, &rev, "tokenizer.json"))?;
        let limit = hf::effective_limit(
            &model,
            &hf::cached_file(&model, &rev, "config.json"),
            max_input_tokens,
        )?;
        Ok::<_, LoadError>((tokens, limit))
    })
    .await
    .map_err(|e| model_error(e.to_string()))??;
    let info = ModelInfo::from_descriptor(SpaceDescriptor {
        engine: EngineId::EmbedAnything,
        engine_version: ENGINE_VERSION.to_owned(),
        model_id: model_id.to_owned(),
        model_revision: Some(revision),
        weight_format: Some("safetensors".to_owned()),
        dimensions,
        normalized: true,
        metric: DistanceMetric::Cosine,
        prefix_policy,
        preprocessing: format!("mean-pooling;max_input_tokens={max_input_tokens}"),
    })
    .map_err(|e| model_error(e.to_string()))?;
    let adapter = LocalTextAdapter::new(
        EmbedAnythingBackend {
            embedder: Arc::new(embedder),
        },
        tokens,
        info,
        Capabilities {
            text_documents: true,
            text_queries: true,
            images: false,
            text_to_image_compatible: false,
            batch: true,
            max_input_tokens: Some(max_input_tokens),
            max_batch: NonZeroUsize::new(32),
            execution: Execution::Local,
        },
    );
    let probe = adapter
        .run(vec!["teste".to_owned()])
        .await
        .map_err(|e| model_error(e.to_string()))?;
    let actual = probe.first().map_or(0, Vec::len);
    if actual != dimensions.get() {
        return Err(LoadError::Dimensions {
            model: model_id.to_owned(),
            expected: dimensions.get(),
            actual,
        });
    }
    Ok(adapter)
}
