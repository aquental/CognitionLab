//! FastEmbed adapter (ONNX Runtime), behind feature `engine-fastembed`.
//!
//! FastEmbed's catalogue loader downloads a model's `main` branch, so this adapter
//! instead fetches the ONNX weights and tokenizer files at the pinned revision with
//! `hf-hub` and builds the model with `try_new_from_user_defined`. Pooling is mean
//! pooling (MiniLM and E5 are both mean-pooled sentence models). FastEmbed adds no
//! prefix, so E5's `query: ` / `passage: ` come from the shared [`LocalTextAdapter`]
//! through the configured [`PrefixPolicy`](super::PrefixPolicy), exactly once.

use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Mutex;

use fastembed::{
    InitOptionsUserDefined, Pooling, TextEmbedding, TokenizerFiles, UserDefinedEmbeddingModel,
};

use super::hf::{self, HfError, HfTokenCounter};
use super::local::{LocalTextAdapter, TextBackend};
use super::{Capabilities, DistanceMetric, EmbeddingError, EngineId, ModelInfo, SpaceDescriptor};
use crate::domain::Execution;

/// FastEmbed crate version pinned in `Cargo.lock` (recorded in model provenance).
pub const ENGINE_VERSION: &str = "7.5.0";

/// ONNX weights loaded when the configuration names none.
pub const DEFAULT_WEIGHTS: &str = "onnx/model.onnx";

/// The FastEmbed adapter: the shared local contract over an ONNX session.
pub type FastEmbedAdapter = LocalTextAdapter<FastEmbedBackend, super::hf::HfTokenCounter>;

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
    /// A file could not be downloaded at the pinned revision.
    #[error("cannot fetch {file} of {model}: {reason}")]
    Fetch {
        /// Model identifier.
        model: String,
        /// Repository file.
        file: String,
        /// Reason.
        reason: String,
    },
    /// The ONNX session could not be built.
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

/// The ONNX session behind [`FastEmbedAdapter`]. `TextEmbedding::embed` takes
/// `&mut self`, so calls are serialized through a mutex; each call already runs a
/// whole batch.
pub struct FastEmbedBackend {
    model: Mutex<TextEmbedding>,
}

impl TextBackend for FastEmbedBackend {
    fn embed_blocking(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let mut model = self.model.lock().map_err(|_| EmbeddingError::Permanent {
            reason: "fastembed session lock poisoned".to_owned(),
        })?;
        model
            .embed(texts, Some(texts.len().max(1)))
            .map_err(|e| EmbeddingError::Permanent {
                reason: format!("inference failed: {e}"),
            })
    }
}

/// Repository files of one pinned model.
struct ModelFiles {
    weights: PathBuf,
    tokenizer: PathBuf,
    config: PathBuf,
    special_tokens_map: PathBuf,
    tokenizer_config: PathBuf,
}

/// Serializes downloads within the process: hf-hub's file lock gives up after about
/// five seconds instead of waiting for a concurrent download of the same blob.
static FETCH: Mutex<()> = Mutex::new(());

/// Attempts per file while another process holds the blob lock (about 5 s each).
const LOCK_ATTEMPTS: u32 = 120;

/// Downloads (or reuses from the Hugging Face cache) the files at `revision`.
fn fetch(model_id: &str, revision: &str, weights: &str) -> Result<ModelFiles, LoadError> {
    let fail = |file: &str, reason: String| LoadError::Fetch {
        model: model_id.to_owned(),
        file: file.to_owned(),
        reason,
    };
    // A poisoned guard only means another download panicked; the cache is still valid.
    let _guard = FETCH
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let api = hf_hub::api::sync::ApiBuilder::from_env()
        .with_progress(false)
        .build()
        .map_err(|e| fail("*", e.to_string()))?;
    let repo = api.repo(hf_hub::Repo::with_revision(
        model_id.to_owned(),
        hf_hub::RepoType::Model,
        revision.to_owned(),
    ));
    let get = |file: &str| {
        let mut attempt = 1;
        loop {
            match repo.get(file) {
                Err(hf_hub::api::sync::ApiError::LockAcquisition(_)) if attempt < LOCK_ATTEMPTS => {
                    attempt += 1;
                }
                other => return other.map_err(|e| fail(file, e.to_string())),
            }
        }
    };
    Ok(ModelFiles {
        weights: get(weights)?,
        tokenizer: get("tokenizer.json")?,
        config: get("config.json")?,
        special_tokens_map: get("special_tokens_map.json")?,
        tokenizer_config: get("tokenizer_config.json")?,
    })
}

fn read(model_id: &str, path: &PathBuf) -> Result<Vec<u8>, LoadError> {
    std::fs::read(path).map_err(|e| LoadError::Model {
        model: model_id.to_owned(),
        reason: format!("cannot read {}: {e}", path.display()),
    })
}

/// Fetches the pinned files, builds the ONNX session, and checks the vector size with
/// a probe embedding. `weights` selects the ONNX file in the repository
/// ([`DEFAULT_WEIGHTS`] when `None`).
///
/// # Errors
///
/// Returns [`LoadError`] when the revision is missing or a file, the session, the
/// limit, or the dimensions do not load as configured.
pub async fn load(
    model_id: &str,
    model_revision: Option<&str>,
    weights: Option<&str>,
    dimensions: NonZeroUsize,
    max_input_tokens: Option<NonZeroUsize>,
    prefix_policy: super::PrefixPolicy,
) -> Result<FastEmbedAdapter, LoadError> {
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
    let weights = weights.unwrap_or(DEFAULT_WEIGHTS).to_owned();
    let model_error = |reason: String| LoadError::Model {
        model: model_id.to_owned(),
        reason,
    };
    let (model, rev, file) = (model_id.to_owned(), revision.clone(), weights.clone());
    let files = tokio::task::spawn_blocking(move || fetch(&model, &rev, &file))
        .await
        .map_err(|e| model_error(e.to_string()))??;
    // Tokenizer deserialization and config reads are blocking file work (C2-8).
    let (model, tokenizer, config) = (
        model_id.to_owned(),
        files.tokenizer.clone(),
        files.config.clone(),
    );
    let (tokens, limit) = tokio::task::spawn_blocking(move || {
        let tokens = HfTokenCounter::load(&tokenizer)?;
        let limit = hf::effective_limit(&model, &config, max_input_tokens)?;
        Ok::<_, LoadError>((tokens, limit))
    })
    .await
    .map_err(|e| model_error(e.to_string()))??;
    let model = model_id.to_owned();
    let session = tokio::task::spawn_blocking(move || {
        let user_model = UserDefinedEmbeddingModel::new(
            read(&model, &files.weights)?,
            TokenizerFiles {
                tokenizer_file: read(&model, &files.tokenizer)?,
                config_file: read(&model, &files.config)?,
                special_tokens_map_file: read(&model, &files.special_tokens_map)?,
                tokenizer_config_file: read(&model, &files.tokenizer_config)?,
            },
        )
        .with_pooling(Pooling::Mean);
        // The adapter rejects longer inputs first, so this truncation never applies.
        let options = InitOptionsUserDefined::new().with_max_length(limit.get());
        TextEmbedding::try_new_from_user_defined(user_model, options).map_err(|e| {
            LoadError::Model {
                model,
                reason: e.to_string(),
            }
        })
    })
    .await
    .map_err(|e| model_error(e.to_string()))??;
    let info = ModelInfo::from_descriptor(SpaceDescriptor {
        engine: EngineId::FastEmbed,
        engine_version: ENGINE_VERSION.to_owned(),
        model_id: model_id.to_owned(),
        model_revision: Some(revision),
        weight_format: Some("onnx".to_owned()),
        dimensions,
        normalized: true,
        metric: DistanceMetric::Cosine,
        prefix_policy,
        preprocessing: format!("mean-pooling;weights={weights};max_input_tokens={limit}"),
    })
    .map_err(|e| model_error(e.to_string()))?;
    let adapter = LocalTextAdapter::new(
        FastEmbedBackend {
            model: Mutex::new(session),
        },
        tokens,
        info,
        Capabilities {
            text_documents: true,
            text_queries: true,
            images: false,
            text_to_image_compatible: false,
            batch: true,
            max_input_tokens: Some(limit),
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
