//! The single feature-gated adapter factory (PLAN §2.2).
//!
//! Engine features are additive: an engine compiled out yields
//! [`BuildError::EngineNotCompiled`] instead of disappearing.

use std::sync::Arc;

use crate::config::{EngineConfig, ModelKind};
use crate::embedding::deterministic::DeterministicEmbedder;
use crate::embedding::{EmbeddingStrategy, EngineId};

/// Adapter construction failures.
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    /// The binary was built without the engine's Cargo feature.
    #[error("engine {engine} is not compiled in; rebuild with --features {feature}")]
    EngineNotCompiled {
        /// Engine identifier.
        engine: EngineId,
        /// Cargo feature that provides it.
        feature: &'static str,
    },
    /// The engine is planned for a later phase.
    #[error("engine {0} is not implemented yet")]
    NotImplemented(EngineId),
    /// Real engines must pin an immutable model revision (PRD §6.4).
    #[error("engine {engine} model {model} has no pinned model_revision")]
    MissingRevision {
        /// Engine identifier.
        engine: EngineId,
        /// Model identifier.
        model: String,
    },
    /// A local engine's revision is a branch or tag, which can move (PRD §6.4).
    #[error(
        "engine {engine} model {model}: model_revision {revision:?} is not an immutable commit id (40 lowercase hex characters)"
    )]
    MutableRevision {
        /// Engine identifier.
        engine: EngineId,
        /// Model identifier.
        model: String,
        /// The configured revision.
        revision: String,
    },
    /// The engine's credential is not configured.
    #[error("engine {engine} needs the {variable} environment variable (see .env.example)")]
    MissingCredential {
        /// Engine identifier.
        engine: EngineId,
        /// Environment variable holding the credential.
        variable: &'static str,
    },
    /// The engine has no adapter for this kind of model.
    #[error("engine {engine} has no {kind:?} adapter; cross-modal models run on embed_anything")]
    UnsupportedModelKind {
        /// Engine identifier.
        engine: EngineId,
        /// Requested model kind.
        kind: ModelKind,
    },
    /// The model takes no prefix, but the configuration sets one.
    #[error("engine {engine} model {model} takes no prefix; set prefix_policy = \"none\"")]
    PrefixNotSupported {
        /// Engine identifier.
        engine: EngineId,
        /// Model identifier.
        model: String,
    },
    /// The model could not be loaded.
    #[error("cannot load {engine} adapter: {reason}")]
    Load {
        /// Engine identifier.
        engine: EngineId,
        /// Reason.
        reason: String,
    },
}

impl BuildError {
    /// CLI exit code: configuration refusals map to
    /// [`EXIT_VALIDATION`](crate::pipeline::EXIT_VALIDATION), load failures to 1.
    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::EngineNotCompiled { .. }
            | Self::NotImplemented(_)
            | Self::MissingRevision { .. }
            | Self::MutableRevision { .. }
            | Self::MissingCredential { .. }
            | Self::UnsupportedModelKind { .. }
            | Self::PrefixNotSupported { .. } => crate::pipeline::EXIT_VALIDATION,
            Self::Load { .. } => 1,
        }
    }
}

/// Builds the adapter selected by `config`. Callers must run the remote-permission
/// check ([`crate::pipeline::check_remote_permission`]) before calling this for a
/// remote engine.
///
/// # Errors
///
/// Returns [`BuildError`] when the engine is unavailable or the model does not load.
pub async fn build(config: &EngineConfig) -> Result<Arc<dyn EmbeddingStrategy>, BuildError> {
    match config.engine {
        EngineId::Deterministic => {
            let load = |e: crate::ids::IdError| BuildError::Load {
                engine: EngineId::Deterministic,
                reason: e.to_string(),
            };
            let mut caps = deterministic_capabilities(config.model_kind).map_err(load)?;
            caps.max_input_tokens = config.max_input_tokens;
            let adapter = DeterministicEmbedder::builder()
                .model_id(config.model_id.clone())
                .dimensions(config.dimensions)
                .prefix_policy(config.prefix_policy)
                .capabilities(caps)
                .build()
                .map_err(load)?;
            Ok(Arc::new(adapter))
        }
        // Cross-modal and OpenAI models take no prefix; refused before any download or
        // credential read.
        EngineId::EmbedAnything | EngineId::OpenAi
            if (config.engine == EngineId::OpenAi || !config.model_kind.is_text())
                && config.prefix_policy != crate::embedding::PrefixPolicy::None =>
        {
            Err(BuildError::PrefixNotSupported {
                engine: config.engine,
                model: config.model_id.clone(),
            })
        }
        EngineId::FastEmbed | EngineId::OpenAi if !config.model_kind.is_text() => {
            Err(BuildError::UnsupportedModelKind {
                engine: config.engine,
                kind: config.model_kind,
            })
        }
        EngineId::EmbedAnything | EngineId::FastEmbed | EngineId::OpenAi
            if config.model_revision.is_none() =>
        {
            Err(BuildError::MissingRevision {
                engine: config.engine,
                model: config.model_id.clone(),
            })
        }
        EngineId::EmbedAnything | EngineId::FastEmbed
            if !config
                .model_revision
                .as_deref()
                .is_some_and(crate::embedding::is_immutable_hub_revision) =>
        {
            Err(BuildError::MutableRevision {
                engine: config.engine,
                model: config.model_id.clone(),
                revision: config.model_revision.clone().unwrap_or_default(),
            })
        }
        EngineId::EmbedAnything => match config.model_kind {
            ModelKind::Text => build_embed_anything(config).await,
            ModelKind::TextToImage => build_vision(config).await,
        },
        EngineId::FastEmbed => build_fastembed(config).await,
        EngineId::OpenAi => build_openai(config),
    }
}

/// A contract-test stand-in for `config.engine` (PRD §5.4): the deterministic adapter
/// reporting the configured engine, model, prefix policy, input limit, and execution
/// location, so the real orchestration runs under that engine's identity without
/// loading a model or calling a provider. It never establishes model quality or live
/// provider integration.
///
/// # Errors
///
/// Returns [`BuildError::Load`] if the space identity cannot be derived.
pub fn build_contract_test(config: &EngineConfig) -> Result<DeterministicEmbedder, BuildError> {
    let load = |e: crate::ids::IdError| BuildError::Load {
        engine: config.engine,
        reason: e.to_string(),
    };
    let mut caps = deterministic_capabilities(config.model_kind).map_err(load)?;
    caps.max_input_tokens = config.max_input_tokens;
    caps.execution = config.engine.execution();
    DeterministicEmbedder::builder()
        .engine_label(config.engine)
        .model_id(config.model_id.clone())
        .dimensions(config.dimensions)
        .prefix_policy(config.prefix_policy)
        .capabilities(caps)
        .build()
        .map_err(load)
}

/// Capabilities the deterministic adapter advertises for a model kind.
fn deterministic_capabilities(
    kind: ModelKind,
) -> Result<crate::embedding::Capabilities, crate::ids::IdError> {
    let builder = DeterministicEmbedder::builder();
    let builder = match kind {
        ModelKind::Text => builder,
        ModelKind::TextToImage => builder.text_to_image(),
    };
    Ok(builder.build()?.capabilities().clone())
}

#[cfg(feature = "engine-embed-anything")]
async fn build_vision(config: &EngineConfig) -> Result<Arc<dyn EmbeddingStrategy>, BuildError> {
    let adapter = crate::embedding::vision::load(
        &config.model_id,
        config.model_revision.as_deref(),
        config.dimensions,
        config.max_input_tokens,
    )
    .await
    .map_err(|e| BuildError::Load {
        engine: EngineId::EmbedAnything,
        reason: e.to_string(),
    })?;
    Ok(Arc::new(adapter))
}

#[cfg(not(feature = "engine-embed-anything"))]
async fn build_vision(config: &EngineConfig) -> Result<Arc<dyn EmbeddingStrategy>, BuildError> {
    build_embed_anything(config).await
}

#[cfg(feature = "engine-embed-anything")]
async fn build_embed_anything(
    config: &EngineConfig,
) -> Result<Arc<dyn EmbeddingStrategy>, BuildError> {
    let adapter = crate::embedding::embed_anything::load(
        &config.model_id,
        config.model_revision.as_deref(),
        config.dimensions,
        config.max_input_tokens,
        config.prefix_policy,
    )
    .await
    .map_err(|e| BuildError::Load {
        engine: EngineId::EmbedAnything,
        reason: e.to_string(),
    })?;
    Ok(Arc::new(adapter))
}

#[cfg(not(feature = "engine-embed-anything"))]
async fn build_embed_anything(
    _config: &EngineConfig,
) -> Result<Arc<dyn EmbeddingStrategy>, BuildError> {
    Err(BuildError::EngineNotCompiled {
        engine: EngineId::EmbedAnything,
        feature: "engine-embed-anything",
    })
}

#[cfg(feature = "engine-fastembed")]
async fn build_fastembed(config: &EngineConfig) -> Result<Arc<dyn EmbeddingStrategy>, BuildError> {
    let adapter = crate::embedding::fastembed::load(
        &config.model_id,
        config.model_revision.as_deref(),
        config.weights.as_deref(),
        config.dimensions,
        config.max_input_tokens,
        config.prefix_policy,
    )
    .await
    .map_err(|e| BuildError::Load {
        engine: EngineId::FastEmbed,
        reason: e.to_string(),
    })?;
    Ok(Arc::new(adapter))
}

#[cfg(not(feature = "engine-fastembed"))]
async fn build_fastembed(_config: &EngineConfig) -> Result<Arc<dyn EmbeddingStrategy>, BuildError> {
    Err(BuildError::EngineNotCompiled {
        engine: EngineId::FastEmbed,
        feature: "engine-fastembed",
    })
}

#[cfg(feature = "engine-openai")]
fn build_openai(config: &EngineConfig) -> Result<Arc<dyn EmbeddingStrategy>, BuildError> {
    use crate::embedding::openai::{LoadError, OpenAiAdapter};
    let adapter = OpenAiAdapter::load(
        &config.model_id,
        config.model_revision.as_deref(),
        config.dimensions,
        config.max_input_tokens,
        config.api_base.as_deref(),
    )
    .map_err(|e| match e {
        LoadError::MissingRevision { .. } => BuildError::MissingRevision {
            engine: EngineId::OpenAi,
            model: config.model_id.clone(),
        },
        LoadError::MissingKey => BuildError::MissingCredential {
            engine: EngineId::OpenAi,
            variable: crate::embedding::openai::API_KEY_ENV,
        },
        other => BuildError::Load {
            engine: EngineId::OpenAi,
            reason: other.to_string(),
        },
    })?;
    Ok(Arc::new(adapter))
}

#[cfg(not(feature = "engine-openai"))]
fn build_openai(_config: &EngineConfig) -> Result<Arc<dyn EmbeddingStrategy>, BuildError> {
    Err(BuildError::EngineNotCompiled {
        engine: EngineId::OpenAi,
        feature: "engine-openai",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroUsize;

    fn config(engine: EngineId) -> EngineConfig {
        EngineConfig {
            engine,
            model_id: "m".into(),
            model_revision: None,
            dimensions: NonZeroUsize::new(8).expect("dims"),
            max_input_tokens: NonZeroUsize::new(64),
            prefix_policy: crate::embedding::PrefixPolicy::None,
            weights: None,
            api_base: None,
            model_kind: ModelKind::Text,
        }
    }

    #[tokio::test]
    async fn a_prefix_on_a_model_without_one_is_a_validation_refusal() {
        // Regression (G3-2): a configuration mismatch, refused before any download or
        // credential read, exits 3 like the other configuration refusals.
        let pinned = Some("3d74acf9a28c67741b2f4f2ea7635f0aaf6f0268".to_owned());
        let mut cross = config(EngineId::EmbedAnything);
        cross.model_revision.clone_from(&pinned);
        cross.model_kind = ModelKind::TextToImage;
        cross.prefix_policy = crate::embedding::PrefixPolicy::E5;
        let mut openai = config(EngineId::OpenAi);
        openai.model_revision = pinned;
        openai.prefix_policy = crate::embedding::PrefixPolicy::E5;
        for refused in [cross, openai] {
            let err = build(&refused).await.err().expect("refused");
            assert!(
                matches!(err, BuildError::PrefixNotSupported { .. }),
                "{err}"
            );
            assert_eq!(err.exit_code(), crate::pipeline::EXIT_VALIDATION);
        }
    }

    #[tokio::test]
    async fn a_cross_modal_kind_selects_a_text_to_image_adapter() {
        let mut cross = config(EngineId::Deterministic);
        cross.model_kind = ModelKind::TextToImage;
        let adapter = build(&cross).await.expect("build");
        let caps = adapter.capabilities();
        assert!(caps.images && caps.text_to_image_compatible && caps.text_queries);
        assert!(
            !caps.text_documents,
            "a cross-modal model embeds no document text"
        );
        assert_eq!(caps.max_input_tokens, NonZeroUsize::new(64));
        let text = build(&config(EngineId::Deterministic))
            .await
            .expect("build");
        assert!(!text.capabilities().text_to_image_compatible);
        for engine in [EngineId::FastEmbed, EngineId::OpenAi] {
            let mut other = config(engine);
            other.model_revision = Some("e8f8c211226b894fcb81acc59f3b34ba3efd5f42".into());
            other.model_kind = ModelKind::TextToImage;
            let err = build(&other).await.err().expect("refused");
            assert!(
                matches!(err, BuildError::UnsupportedModelKind { .. }),
                "{err}"
            );
            assert_eq!(err.exit_code(), crate::pipeline::EXIT_VALIDATION);
        }
    }

    #[tokio::test]
    async fn deterministic_adapter_honours_the_configured_limit() {
        let adapter = build(&config(EngineId::Deterministic))
            .await
            .expect("build");
        assert_eq!(
            adapter.capabilities().max_input_tokens,
            NonZeroUsize::new(64)
        );
        assert_eq!(adapter.model_info().model_id, "m");
    }

    #[tokio::test]
    async fn a_real_engine_without_a_pinned_revision_is_refused() {
        let outcome = build(&config(EngineId::EmbedAnything)).await;
        let err = outcome.err().expect("refused");
        assert!(matches!(err, BuildError::MissingRevision { .. }), "{err}");
        assert_eq!(err.exit_code(), crate::pipeline::EXIT_VALIDATION);
        let load = BuildError::Load {
            engine: EngineId::EmbedAnything,
            reason: "tokenizer missing".into(),
        };
        assert_eq!(load.exit_code(), 1, "load failures are internal errors");
    }

    #[tokio::test]
    async fn a_moving_hub_revision_is_refused_before_any_download() {
        // Regression (C2-13): a branch or tag can change the weights behind one space id.
        for engine in [EngineId::EmbedAnything, EngineId::FastEmbed] {
            for revision in ["main", "v1.0", "e8f8c21"] {
                let mut moving = config(engine);
                moving.model_revision = Some(revision.into());
                let err = build(&moving).await.err().expect("refused");
                assert!(
                    matches!(err, BuildError::MutableRevision { .. }),
                    "{engine:?} {revision}: {err}"
                );
                assert_eq!(err.exit_code(), crate::pipeline::EXIT_VALIDATION);
            }
        }
    }

    #[cfg(not(feature = "engine-embed-anything"))]
    #[tokio::test]
    async fn missing_engine_feature_is_an_explicit_error() {
        let mut pinned = config(EngineId::EmbedAnything);
        pinned.model_revision = Some("e8f8c211226b894fcb81acc59f3b34ba3efd5f42".into());
        let outcome = build(&pinned).await;
        assert!(
            matches!(outcome, Err(BuildError::EngineNotCompiled { .. })),
            "expected EngineNotCompiled"
        );
    }
}
