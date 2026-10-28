//! Saved experiment configuration (PRD §12 deliverable 8).
//!
//! ```toml
//! [engine]
//! engine = "embed_anything"
//! model_id = "sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2"
//! model_revision = "e8f8c211226b894fcb81acc59f3b34ba3efd5f42"
//! dimensions = 384
//! max_input_tokens = 128
//!
//! [ingest]          # optional; defaults from PLAN §0.1
//! max_attempts = 3
//! ```
//! Unknown keys are rejected, so a typo cannot silently fall back to a default.

use std::num::{NonZeroU32, NonZeroUsize};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::chunking::ChunkingConfig;
use crate::domain::Execution;
use crate::embedding::{EngineId, PrefixPolicy};
use crate::extraction::ExtractionConfig;

/// Engine and model selection; engine and model are separate choices (PRD §6.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineConfig {
    /// Canonical engine id.
    pub engine: EngineId,
    /// Model identifier.
    pub model_id: String,
    /// Immutable model revision; required for real engines.
    pub model_revision: Option<String>,
    /// Expected vector dimensions (checked against the loaded model).
    pub dimensions: NonZeroUsize,
    /// Model input limit in its own tokens.
    pub max_input_tokens: Option<NonZeroUsize>,
    /// Prefix policy applied inside the adapter.
    #[serde(default = "default_prefix")]
    pub prefix_policy: PrefixPolicy,
}

const fn default_prefix() -> PrefixPolicy {
    PrefixPolicy::None
}

impl EngineId {
    /// Where the engine runs; known before any adapter is built.
    #[must_use]
    pub const fn execution(self) -> Execution {
        match self {
            Self::OpenAi => Execution::Remote,
            Self::EmbedAnything | Self::FastEmbed | Self::Deterministic => Execution::Local,
        }
    }
}

/// Ingestion settings as written in a config file (all optional).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IngestConfig {
    /// Attempts per batch, including the first.
    pub max_attempts: Option<NonZeroU32>,
    /// Base retry backoff in milliseconds.
    pub backoff_ms: Option<u64>,
    /// Upper bound on inputs per adapter call.
    pub batch_size: Option<NonZeroUsize>,
    /// Language tag of the corpus text.
    pub language: Option<String>,
}

/// A saved experiment configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExperimentConfig {
    /// Engine and model.
    pub engine: EngineConfig,
    /// Ingestion settings.
    #[serde(default)]
    pub ingest: IngestConfig,
    /// Extraction settings.
    #[serde(default)]
    pub extraction: ExtractionConfig,
    /// Chunking settings.
    #[serde(default)]
    pub chunking: ChunkingConfig,
}

/// Configuration errors.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The file could not be read.
    #[error("cannot read config {path}")]
    Read {
        /// Path.
        path: PathBuf,
        /// I/O error.
        #[source]
        source: std::io::Error,
    },
    /// The file is not a valid configuration.
    #[error("invalid config {path}")]
    Parse {
        /// Path.
        path: PathBuf,
        /// Parse error.
        #[source]
        source: toml::de::Error,
    },
}

impl ExperimentConfig {
    /// Loads a configuration file.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when the file cannot be read or parsed.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_owned(),
            source,
        })?;
        toml::from_str(&text).map_err(|source| ConfigError::Parse {
            path: path.to_owned(),
            source,
        })
    }

    /// Ingest settings for `experiment_id`, applying configured overrides.
    #[must_use]
    pub fn ingest_settings(
        &self,
        experiment_id: crate::ids::ExperimentId,
    ) -> crate::pipeline::IngestSettings {
        let mut settings = crate::pipeline::IngestSettings::new(experiment_id);
        if let Some(n) = self.ingest.max_attempts {
            settings.max_attempts = n;
        }
        if let Some(ms) = self.ingest.backoff_ms {
            settings.backoff_ms = ms;
        }
        if let Some(n) = self.ingest.batch_size {
            settings.batch_size = n;
        }
        if let Some(lang) = &self.ingest.language {
            settings.language.clone_from(lang);
        }
        settings.extraction = self.extraction.clone();
        settings.chunking = self.chunking.clone();
        settings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_config_parses_and_applies_defaults() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("config/experiments/baseline-text-embed-anything.toml");
        let config = ExperimentConfig::load(&path).expect("load");
        assert_eq!(config.engine.engine, EngineId::EmbedAnything);
        assert!(
            config.engine.model_revision.is_some(),
            "real engines pin a revision"
        );
        let settings = config.ingest_settings("exp-1".parse().expect("id"));
        assert_eq!(settings.max_attempts.get(), 3);
        assert_eq!(settings.extraction, ExtractionConfig::default());
    }

    #[test]
    fn typos_are_rejected() {
        let text = "[engine]\nengine = \"deterministic\"\nmodel_id = \"m\"\ndimensions = 8\nmodel_revison = \"x\"\n";
        assert!(toml::from_str::<ExperimentConfig>(text).is_err());
    }

    #[test]
    fn only_openai_is_remote() {
        assert_eq!(EngineId::OpenAi.execution(), Execution::Remote);
        assert_eq!(EngineId::EmbedAnything.execution(), Execution::Local);
    }
}
