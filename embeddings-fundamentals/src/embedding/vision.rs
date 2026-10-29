//! Cross-modal text-to-image adapter (CLIP, SigLIP) over EmbedAnything 0.7.1 (candle),
//! behind feature `engine-embed-anything` (PLAN Phase 3; spike in
//! `reports/spikes/phase3-models.md`).
//!
//! Only the model weights come from EmbedAnything (`ClipEmbedder::new` at a pinned
//! commit). The adapter does everything else itself, because the engine's own image
//! path hardcodes CLIP-224 sizes with SigLIP normalization and its text path panics on
//! long inputs:
//! - images are decoded and preprocessed as the pinned `preprocessor_config.json`
//!   specifies ([`ImagePreprocessing`]);
//! - query text is tokenized with the pinned `tokenizer.json`; an input above the
//!   model's position limit is refused (never truncated), and inputs are padded to that
//!   limit with the model's pad id, as the engine does;
//! - inference runs on the blocking pool; a panic there becomes a permanent error;
//! - vectors are L2-normalized.
//!
//! The model embeds text queries and images in one space; it does not embed document
//! text (`text_documents = false`), so a text run cannot write into its space.

use std::num::NonZeroUsize;
use std::sync::Arc;

use candle_core::{DType, Tensor};
use embed_anything::embeddings::local::clip::ClipEmbedder;

use super::hf::{self, HfError, HfTokenCounter};
use super::local::ModelTokenizer;
use super::preprocess::ImagePreprocessing;
use super::{
    AssetRef, Capabilities, Capability, DistanceMetric, Embedding, EmbeddingBatch, EmbeddingError,
    EmbeddingStrategy, EngineId, InputKind, ModelInfo, PrefixPolicy, SpaceDescriptor, TextInput,
    TokenCounter, l2_normalize,
};
use crate::domain::Execution;

/// Errors raised while loading a vision model.
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
    /// The model, a model file, or the preprocessing could not be loaded.
    #[error("cannot load model {model}: {reason}")]
    Model {
        /// Model identifier.
        model: String,
        /// Reason.
        reason: String,
    },
    /// A model file (tokenizer) is unusable.
    #[error(transparent)]
    Files(#[from] HfError),
    /// The configured limit exceeds the model's text position limit.
    #[error(
        "model {model} accepts {model_limit} text tokens, but max_input_tokens is {configured}"
    )]
    LimitAboveModel {
        /// Model identifier.
        model: String,
        /// Position limit of the text tower.
        model_limit: usize,
        /// Configured limit.
        configured: usize,
    },
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

/// Model, preprocessing, and tokenizer shared with the blocking inference tasks.
struct Model {
    embedder: ClipEmbedder,
    preprocessing: ImagePreprocessing,
    tokens: HfTokenCounter,
    /// Text position limit (tokens, special tokens included).
    max_len: usize,
}

/// The CLIP/SigLIP adapter.
pub struct VisionAdapter {
    model: Arc<Model>,
    info: ModelInfo,
    capabilities: Capabilities,
}

impl std::fmt::Debug for VisionAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VisionAdapter")
            .field("info", &self.info)
            .finish_non_exhaustive()
    }
}

fn permanent(reason: impl Into<String>) -> EmbeddingError {
    EmbeddingError::Permanent {
        reason: reason.into(),
    }
}

fn inference_failed(e: &candle_core::Error) -> EmbeddingError {
    permanent(format!("inference failed: {e}"))
}

/// Normalized rows of a `[batch, dims]` feature tensor.
fn rows(features: &Tensor) -> Result<Vec<Vec<f32>>, EmbeddingError> {
    let mut vectors = features
        .to_dtype(DType::F32)
        .and_then(|t| t.to_vec2::<f32>())
        .map_err(|e| inference_failed(&e))?;
    for v in &mut vectors {
        l2_normalize(v);
    }
    Ok(vectors)
}

impl Model {
    /// Tokenizes, checks the budget, pads, and embeds query texts.
    fn embed_texts(
        &self,
        inputs: &[TextInput],
        budget: usize,
    ) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let mut batch = Vec::with_capacity(inputs.len());
        for input in inputs {
            let tokens = self.tokens.count(&input.text);
            if tokens > budget {
                return Err(EmbeddingError::InputTooLong {
                    item: input.id.clone(),
                    tokens,
                    limit: budget,
                });
            }
            let mut ids = self
                .tokens
                .encode_ids(&input.text)
                .map_err(|e| permanent(format!("tokenization failed: {e}")))?;
            if ids.len() > self.max_len {
                return Err(EmbeddingError::InputTooLong {
                    item: input.id.clone(),
                    tokens,
                    limit: budget,
                });
            }
            ids.resize(self.max_len, self.embedder.pad_id);
            batch.push(ids);
        }
        let ids = Tensor::new(batch, &self.embedder.device).map_err(|e| inference_failed(&e))?;
        let features = self
            .embedder
            .model
            .get_text_features(&ids)
            .map_err(|e| inference_failed(&e))?;
        rows(&features)
    }

    /// Decodes, preprocesses, and embeds images.
    fn embed_images(&self, assets: &[AssetRef]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let mut tensors = Vec::with_capacity(assets.len());
        for asset in assets {
            let bytes = std::fs::read(&asset.path)
                .map_err(|e| permanent(format!("cannot read image {}: {e}", asset.id)))?;
            let pixels = self
                .preprocessing
                .pixels(&bytes)
                .map_err(|e| permanent(format!("cannot preprocess image {}: {e}", asset.id)))?;
            let tensor = Tensor::from_vec(
                pixels.values,
                (3, pixels.height, pixels.width),
                &self.embedder.device,
            )
            .map_err(|e| inference_failed(&e))?;
            tensors.push(tensor);
        }
        let batch = Tensor::stack(&tensors, 0).map_err(|e| inference_failed(&e))?;
        let features = self
            .embedder
            .model
            .get_image_features(&batch)
            .map_err(|e| inference_failed(&e))?;
        rows(&features)
    }
}

impl VisionAdapter {
    async fn run(
        &self,
        work: impl FnOnce(&Model) -> Result<Vec<Vec<f32>>, EmbeddingError> + Send + 'static,
        ids: Vec<super::ItemId>,
    ) -> Result<EmbeddingBatch, EmbeddingError> {
        let model = Arc::clone(&self.model);
        let expected = ids.len();
        // A panic inside the engine is a permanent failure of this call, not a crash.
        let vectors = tokio::task::spawn_blocking(move || work(&model))
            .await
            .map_err(|e| permanent(format!("inference task failed: {e}")))??;
        if vectors.len() != expected {
            return Err(permanent(format!(
                "engine returned {} vectors for {expected} inputs",
                vectors.len()
            )));
        }
        Ok(EmbeddingBatch {
            embeddings: ids
                .into_iter()
                .zip(vectors)
                .map(|(id, vector)| Embedding { id, vector })
                .collect(),
            usage: None,
        })
    }
}

#[async_trait::async_trait]
impl EmbeddingStrategy for VisionAdapter {
    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    fn model_info(&self) -> &ModelInfo {
        &self.info
    }

    fn token_counter(&self) -> &dyn TokenCounter {
        &self.model.tokens
    }

    fn input_budget(&self, _kind: InputKind) -> Option<usize> {
        let limit = self.capabilities.max_input_tokens?.get();
        Some(limit.saturating_sub(self.model.tokens.special_tokens()))
    }

    async fn embed_documents(&self, _raw: &[TextInput]) -> Result<EmbeddingBatch, EmbeddingError> {
        self.capabilities
            .require(Capability::TextDocuments, &self.info)?;
        Ok(EmbeddingBatch::default())
    }

    async fn embed_queries(&self, raw: &[TextInput]) -> Result<EmbeddingBatch, EmbeddingError> {
        self.capabilities
            .require(Capability::TextQueries, &self.info)?;
        self.capabilities.check_batch(raw.len())?;
        if raw.is_empty() {
            return Ok(EmbeddingBatch::default());
        }
        let budget = self.input_budget(InputKind::Query).unwrap_or(0);
        let inputs = raw.to_vec();
        let ids = raw.iter().map(|i| i.id.clone()).collect();
        self.run(move |model| model.embed_texts(&inputs, budget), ids)
            .await
    }

    async fn embed_images(&self, assets: &[AssetRef]) -> Result<EmbeddingBatch, EmbeddingError> {
        self.capabilities.require(Capability::Images, &self.info)?;
        self.capabilities.check_batch(assets.len())?;
        if assets.is_empty() {
            return Ok(EmbeddingBatch::default());
        }
        let inputs = assets.to_vec();
        let ids = assets.iter().map(|a| a.id.clone()).collect();
        self.run(move |model| model.embed_images(&inputs), ids)
            .await
    }
}

/// Loads a pinned CLIP or SigLIP checkpoint, its tokenizer, and its image
/// preprocessing, then checks the vector size with a probe query.
///
/// # Errors
///
/// Returns [`LoadError`] when the revision is missing or mutable, a file or the model
/// does not load, the limit exceeds the model, or the dimensions differ.
pub async fn load(
    model_id: &str,
    model_revision: Option<&str>,
    dimensions: NonZeroUsize,
    max_input_tokens: Option<NonZeroUsize>,
) -> Result<VisionAdapter, LoadError> {
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
    let (id, rev) = (model_id.to_owned(), revision.clone());
    // Downloads, weight loading, and tokenizer deserialization are blocking work.
    let model = tokio::task::spawn_blocking(move || -> Result<Model, LoadError> {
        let fail = |reason: String| LoadError::Model {
            model: id.clone(),
            reason,
        };
        let files = hf::download(&id, &rev, &["tokenizer.json", "preprocessor_config.json"])?;
        let [tokenizer, preprocessor] = <[std::path::PathBuf; 2]>::try_from(files)
            .map_err(|_| fail("download returned another number of files".to_owned()))?;
        let tokens = HfTokenCounter::load(&tokenizer)?;
        let preprocessing = std::fs::read_to_string(&preprocessor)
            .map_err(|e| fail(e.to_string()))
            .and_then(|text| ImagePreprocessing::from_json(&text).map_err(fail))?;
        if preprocessing.fixed_output_size().is_none() {
            return Err(fail(
                "the image preprocessing has no fixed input size, so images cannot be batched"
                    .to_owned(),
            ));
        }
        let embedder =
            ClipEmbedder::new(id.clone(), Some(&rev), None).map_err(|e| fail(e.to_string()))?;
        if !embedder.model.supports_text() {
            return Err(fail("the checkpoint has no text tower".to_owned()));
        }
        // Queries are padded to the position limit and both text towers pool at the end
        // token (CLIP: first maximal id; SigLIP: the last position, unmasked). Padding
        // leaves the pooled token unchanged only when the pad id is the end token the
        // tokenizer appends ("sticky EOS": `</s>` for SigLIP, `<|endoftext|>` for CLIP).
        let end = tokens
            .encode_ids("probe")
            .map_err(|e| fail(format!("tokenization failed: {e}")))?
            .last()
            .copied();
        if end != Some(embedder.pad_id) {
            return Err(fail(format!(
                "pad id {} is not the end token {end:?} of the tokenizer, so padding would change the pooled token",
                embedder.pad_id
            )));
        }
        let max_len = embedder.max_len;
        Ok(Model {
            embedder,
            preprocessing,
            tokens,
            max_len,
        })
    })
    .await
    .map_err(|e| model_error(e.to_string()))??;
    let model_limit = model.max_len;
    let limit = match max_input_tokens {
        Some(configured) if configured.get() > model_limit => {
            return Err(LoadError::LimitAboveModel {
                model: model_id.to_owned(),
                model_limit,
                configured: configured.get(),
            });
        }
        Some(configured) => configured,
        None => NonZeroUsize::new(model_limit)
            .ok_or_else(|| model_error("the model has no text position limit".to_owned()))?,
    };
    let preprocessing = format!(
        "{};text:max_tokens={limit};pad_to={model_limit};pad_id={}",
        model.preprocessing.descriptor(),
        model.embedder.pad_id
    );
    let info = ModelInfo::from_descriptor(SpaceDescriptor {
        engine: EngineId::EmbedAnything,
        engine_version: super::embed_anything::ENGINE_VERSION.to_owned(),
        model_id: model_id.to_owned(),
        model_revision: Some(revision),
        weight_format: Some("candle".to_owned()),
        dimensions,
        normalized: true,
        metric: DistanceMetric::Cosine,
        prefix_policy: PrefixPolicy::None,
        preprocessing,
    })
    .map_err(|e| model_error(e.to_string()))?;
    let adapter = VisionAdapter {
        model: Arc::new(model),
        info,
        capabilities: Capabilities {
            text_documents: false,
            text_queries: true,
            images: true,
            text_to_image_compatible: true,
            batch: true,
            max_input_tokens: Some(limit),
            max_batch: NonZeroUsize::new(16),
            execution: Execution::Local,
        },
    };
    let probe = adapter
        .embed_queries(&[TextInput {
            id: "probe"
                .parse()
                .map_err(|e: crate::ids::IdError| model_error(e.to_string()))?,
            text: "teste".to_owned(),
        }])
        .await
        .map_err(|e| model_error(e.to_string()))?;
    let actual = probe.embeddings.first().map_or(0, |e| e.vector.len());
    if actual != dimensions.get() {
        return Err(LoadError::Dimensions {
            model: model_id.to_owned(),
            expected: dimensions.get(),
            actual,
        });
    }
    Ok(adapter)
}
