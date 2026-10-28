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
    /// The model could not be loaded.
    #[error("cannot load {engine} adapter: {reason}")]
    Load {
        /// Engine identifier.
        engine: EngineId,
        /// Reason.
        reason: String,
    },
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
        EngineId::EmbedAnything => build_embed_anything(config).await,
        EngineId::FastEmbed | EngineId::OpenAi => Err(BuildError::NotImplemented(config.engine)),
    }
}

#[cfg(feature = "engine-embed-anything")]
async fn build_embed_anything(
    config: &EngineConfig,
) -> Result<Arc<dyn EmbeddingStrategy>, BuildError> {
    let adapter = crate::embedding::embed_anything::EmbedAnythingAdapter::load(
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
async fn build_embed_anything(
    _config: &EngineConfig,
) -> Result<Arc<dyn EmbeddingStrategy>, BuildError> {
    Err(BuildError::EngineNotCompiled {
        engine: EngineId::EmbedAnything,
        feature: "engine-embed-anything",
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

    #[cfg(not(feature = "engine-embed-anything"))]
    #[tokio::test]
    async fn missing_engine_feature_is_an_explicit_error() {
        let outcome = build(&config(EngineId::EmbedAnything)).await;
        assert!(
            matches!(outcome, Err(BuildError::EngineNotCompiled { .. })),
            "expected EngineNotCompiled"
        );
    }
}
