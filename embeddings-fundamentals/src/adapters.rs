//! The single feature-gated adapter factory (PLAN §2.2).
//!
//! Engine features are additive: an engine compiled out yields
//! [`BuildError::EngineNotCompiled`] instead of disappearing.

use std::sync::Arc;

use crate::config::EngineConfig;
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
    /// The engine's credential is not configured.
    #[error("engine {engine} needs the {variable} environment variable (see .env.example)")]
    MissingCredential {
        /// Engine identifier.
        engine: EngineId,
        /// Environment variable holding the credential.
        variable: &'static str,
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
            | Self::MissingCredential { .. } => crate::pipeline::EXIT_VALIDATION,
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
            let mut builder = DeterministicEmbedder::builder()
                .model_id(config.model_id.clone())
                .dimensions(config.dimensions)
                .prefix_policy(config.prefix_policy);
            let defaults =
                DeterministicEmbedder::builder()
                    .build()
                    .map_err(|e| BuildError::Load {
                        engine: EngineId::Deterministic,
                        reason: e.to_string(),
                    })?;
            let mut caps = defaults.capabilities().clone();
            caps.max_input_tokens = config.max_input_tokens;
            builder = builder.capabilities(caps);
            let adapter = builder.build().map_err(|e| BuildError::Load {
                engine: EngineId::Deterministic,
                reason: e.to_string(),
            })?;
            Ok(Arc::new(adapter))
        }
        EngineId::EmbedAnything | EngineId::FastEmbed | EngineId::OpenAi
            if config.model_revision.is_none() =>
        {
            Err(BuildError::MissingRevision {
                engine: config.engine,
                model: config.model_id.clone(),
            })
        }
        EngineId::EmbedAnything => build_embed_anything(config).await,
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
    let mut caps = DeterministicEmbedder::builder()
        .build()
        .map_err(load)?
        .capabilities()
        .clone();
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
    if config.prefix_policy != crate::embedding::PrefixPolicy::None {
        return Err(BuildError::Load {
            engine: EngineId::OpenAi,
            reason: "OpenAI models take no prefix; set prefix_policy = \"none\"".to_owned(),
        });
    }
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

    #[cfg(not(feature = "engine-embed-anything"))]
    #[tokio::test]
    async fn missing_engine_feature_is_an_explicit_error() {
        let mut pinned = config(EngineId::EmbedAnything);
        pinned.model_revision = Some("abc".into());
        let outcome = build(&pinned).await;
        assert!(
            matches!(outcome, Err(BuildError::EngineNotCompiled { .. })),
            "expected EngineNotCompiled"
        );
    }
}
