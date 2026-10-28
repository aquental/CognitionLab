//! EmbedAnything adapter (candle, safetensors), behind feature `engine-embed-anything`.
//!
//! Loads a `BertModel`-architecture sentence model at a pinned revision with mean
//! pooling. Vectors are L2-normalized by the adapter. Token counting uses the model's
//! own `tokenizer.json` from the Hugging Face cache, which EmbedAnything fills when it
//! downloads the model. The pipeline enforces the input limit before text reaches the
//! engine, so EmbedAnything's internal truncation never applies. Model loading and
//! inference are CPU-bound and run in `spawn_blocking`.

use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;

use embed_anything::embeddings::embed::{Embedder, EmbeddingResult};
use embed_anything::embeddings::local::pooling::Pooling;
use tokenizers::Tokenizer;

use super::{
    AssetRef, Capabilities, Capability, DistanceMetric, Embedding, EmbeddingBatch, EmbeddingError,
    EmbeddingStrategy, EngineId, InputKind, ModelInfo, SpaceDescriptor, TextInput, TokenCounter,
    l2_normalize,
};
use crate::domain::Execution;

/// EmbedAnything crate version pinned in `Cargo.lock` (recorded in model provenance).
pub const ENGINE_VERSION: &str = "0.7.1";

/// Errors raised while loading the model.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// Real engines must pin an immutable model revision (PRD §6.4).
    #[error("model {model} has no pinned revision")]
    MissingRevision {
        /// Model identifier.
        model: String,
    },
    /// The model could not be downloaded or loaded.
    #[error("cannot load model {model}: {reason}")]
    Model {
        /// Model identifier.
        model: String,
        /// Reason.
        reason: String,
    },
    /// The tokenizer could not be loaded from the Hugging Face cache.
    #[error("cannot load tokenizer {path}: {reason}")]
    Tokenizer {
        /// Expected tokenizer path.
        path: PathBuf,
        /// Reason.
        reason: String,
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

/// Hugging Face hub cache directory: `$HF_HUB_CACHE`, `$HF_HOME/hub`, or
/// `~/.cache/huggingface/hub`.
fn hf_hub_cache() -> PathBuf {
    if let Some(dir) = std::env::var_os("HF_HUB_CACHE") {
        return PathBuf::from(dir);
    }
    if let Some(home) = std::env::var_os("HF_HOME") {
        return PathBuf::from(home).join("hub");
    }
    let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from);
    home.join(".cache").join("huggingface").join("hub")
}

/// Path of `file` in the cached snapshot of `model_id` at `revision`.
fn cached_file(model_id: &str, revision: &str, file: &str) -> PathBuf {
    hf_hub_cache()
        .join(format!("models--{}", model_id.replace('/', "--")))
        .join("snapshots")
        .join(revision)
        .join(file)
}

/// Token counter backed by the model's `tokenizer.json`.
#[derive(Debug)]
pub struct HfTokenCounter {
    tokenizer: Tokenizer,
    special_tokens: usize,
}

impl HfTokenCounter {
    fn load(path: PathBuf) -> Result<Self, LoadError> {
        let tokenizer = Tokenizer::from_file(&path).map_err(|e| LoadError::Tokenizer {
            path: path.clone(),
            reason: e.to_string(),
        })?;
        let special_tokens = tokenizer
            .encode("", true)
            .map_err(|e| LoadError::Tokenizer {
                path,
                reason: e.to_string(),
            })?
            .len();
        Ok(Self {
            tokenizer,
            special_tokens,
        })
    }
}

impl TokenCounter for HfTokenCounter {
    fn count(&self, text: &str) -> usize {
        // Encoding cannot fail for valid UTF-8 with a loaded tokenizer; count words as a
        // conservative fallback rather than panicking.
        self.tokenizer
            .encode(text, false)
            .map_or_else(|_| text.split_whitespace().count(), |e| e.len())
    }
}

/// The EmbedAnything adapter.
pub struct EmbedAnythingAdapter {
    embedder: Arc<Embedder>,
    tokens: HfTokenCounter,
    info: ModelInfo,
    capabilities: Capabilities,
}

impl std::fmt::Debug for EmbedAnythingAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmbedAnythingAdapter")
            .field("info", &self.info)
            .finish_non_exhaustive()
    }
}

fn dense(result: EmbeddingResult) -> Result<Vec<f32>, EmbeddingError> {
    match result {
        EmbeddingResult::DenseVector(v) => Ok(v),
        EmbeddingResult::MultiVector(_) => Err(EmbeddingError::Permanent {
            reason: "model returned a multi-vector embedding".to_owned(),
        }),
    }
}

impl EmbedAnythingAdapter {
    /// Downloads (if needed) and loads the model and its tokenizer, then checks the
    /// vector size with a probe embedding.
    ///
    /// # Errors
    ///
    /// Returns [`LoadError`] when the revision is missing or the model, tokenizer, or
    /// dimensions do not load as configured.
    pub async fn load(
        model_id: &str,
        model_revision: Option<&str>,
        dimensions: NonZeroUsize,
        max_input_tokens: Option<NonZeroUsize>,
    ) -> Result<Self, LoadError> {
        let revision = model_revision
            .ok_or_else(|| LoadError::MissingRevision {
                model: model_id.to_owned(),
            })?
            .to_owned();
        let (model, rev) = (model_id.to_owned(), revision.clone());
        let embedder = tokio::task::spawn_blocking(move || {
            Embedder::from_pretrained_hf(&model, Some(&rev), None, None, Some(Pooling::Mean))
        })
        .await
        .map_err(|e| LoadError::Model {
            model: model_id.to_owned(),
            reason: e.to_string(),
        })?
        .map_err(|e| LoadError::Model {
            model: model_id.to_owned(),
            reason: e.to_string(),
        })?;
        let tokens = HfTokenCounter::load(cached_file(model_id, &revision, "tokenizer.json"))?;
        let info = ModelInfo::from_descriptor(SpaceDescriptor {
            engine: EngineId::EmbedAnything,
            engine_version: ENGINE_VERSION.to_owned(),
            model_id: model_id.to_owned(),
            model_revision: Some(revision),
            weight_format: Some("safetensors".to_owned()),
            dimensions,
            normalized: true,
            metric: DistanceMetric::Cosine,
            prefix_policy: super::PrefixPolicy::None,
            preprocessing: format!(
                "mean-pooling;max_input_tokens={}",
                max_input_tokens.map_or(0, NonZeroUsize::get)
            ),
        })
        .map_err(|e| LoadError::Model {
            model: model_id.to_owned(),
            reason: e.to_string(),
        })?;
        let adapter = Self {
            embedder: Arc::new(embedder),
            tokens,
            info,
            capabilities: Capabilities {
                text_documents: true,
                text_queries: true,
                images: false,
                text_to_image_compatible: false,
                batch: true,
                max_input_tokens,
                max_batch: NonZeroUsize::new(32),
                execution: Execution::Local,
            },
        };
        let probe = adapter
            .run(vec!["teste".to_owned()])
            .await
            .map_err(|e| LoadError::Model {
                model: model_id.to_owned(),
                reason: e.to_string(),
            })?;
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

    async fn run(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let embedder = Arc::clone(&self.embedder);
        let results = tokio::task::spawn_blocking(move || {
            let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
            let batch = refs.len();
            tokio::runtime::Handle::current().block_on(embedder.embed(&refs, Some(batch), None))
        })
        .await
        .map_err(|e| EmbeddingError::Permanent {
            reason: format!("inference task failed: {e}"),
        })?
        .map_err(|e| EmbeddingError::Permanent {
            reason: format!("inference failed: {e}"),
        })?;
        results
            .into_iter()
            .map(|r| {
                let mut v = dense(r)?;
                l2_normalize(&mut v);
                Ok(v)
            })
            .collect()
    }

    async fn embed_text(
        &self,
        kind: InputKind,
        raw: &[TextInput],
    ) -> Result<EmbeddingBatch, EmbeddingError> {
        let capability = match kind {
            InputKind::Document => Capability::TextDocuments,
            InputKind::Query => Capability::TextQueries,
        };
        self.capabilities.require(capability, &self.info)?;
        self.capabilities.check_batch(raw.len())?;
        if let Some(limit) = self.capabilities.max_input_tokens {
            let budget = limit.get().saturating_sub(self.tokens.special_tokens);
            for input in raw {
                let tokens = self.tokens.count(&input.text);
                if tokens > budget {
                    return Err(EmbeddingError::InputTooLong {
                        item: input.id.clone(),
                        tokens,
                        limit: budget,
                    });
                }
            }
        }
        let texts = raw
            .iter()
            .map(|i| self.info.prefix_policy.apply(kind, &i.text).into_owned())
            .collect();
        let vectors = self.run(texts).await?;
        if vectors.len() != raw.len() {
            return Err(EmbeddingError::Permanent {
                reason: format!(
                    "engine returned {} vectors for {} inputs",
                    vectors.len(),
                    raw.len()
                ),
            });
        }
        let embeddings = raw
            .iter()
            .zip(vectors)
            .map(|(i, vector)| Embedding {
                id: i.id.clone(),
                vector,
            })
            .collect();
        Ok(EmbeddingBatch {
            embeddings,
            usage: None,
        })
    }
}

#[async_trait::async_trait]
impl EmbeddingStrategy for EmbedAnythingAdapter {
    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    fn model_info(&self) -> &ModelInfo {
        &self.info
    }

    fn token_counter(&self) -> &dyn TokenCounter {
        &self.tokens
    }

    fn input_budget(&self, kind: InputKind) -> Option<usize> {
        let limit = self.capabilities.max_input_tokens?.get();
        let prefix = self.tokens.count(self.info.prefix_policy.prefix(kind));
        Some(limit.saturating_sub(self.tokens.special_tokens + prefix))
    }

    async fn embed_documents(&self, raw: &[TextInput]) -> Result<EmbeddingBatch, EmbeddingError> {
        self.embed_text(InputKind::Document, raw).await
    }

    async fn embed_queries(&self, raw: &[TextInput]) -> Result<EmbeddingBatch, EmbeddingError> {
        self.embed_text(InputKind::Query, raw).await
    }

    async fn embed_images(&self, _assets: &[AssetRef]) -> Result<EmbeddingBatch, EmbeddingError> {
        self.capabilities.require(Capability::Images, &self.info)?;
        Ok(EmbeddingBatch::default())
    }
}
