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
//! An image run sets `modality = "image"` under `[ingest]` and selects a cross-modal
//! model with `model_kind = "text_to_image"` under `[engine]`.
//! Unknown keys are rejected, so a typo cannot silently fall back to a default.
//!
//! A combined-retrieval configuration ([`CombinedConfig`], PRD §6.6) names a text
//! configuration and an image configuration (paths relative to its own file) and the
//! fusion parameters:
//! ```toml
//! text = "text-fastembed-e5-base.toml"
//! image = "image-siglip-multilingual.toml"
//!
//! [fusion]
//! method = "rrf"
//! k = 60
//! depth = "all"
//! ```

use std::num::{NonZeroU32, NonZeroUsize};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::chunking::ChunkingConfig;
use crate::domain::{Execution, Modality};
use crate::embedding::{EngineId, PrefixPolicy};
use crate::extraction::ExtractionConfig;
use crate::fusion::FusionParams;

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
    /// Weight file inside the model repository (FastEmbed: defaults to
    /// `onnx/model.onnx`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weights: Option<String>,
    /// API base URL of a remote engine (OpenAI: defaults to the public endpoint).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_base: Option<String>,
    /// What the model embeds: document text, or text queries and images in one
    /// cross-modal space (CLIP, SigLIP).
    #[serde(default, skip_serializing_if = "ModelKind::is_text")]
    pub model_kind: ModelKind,
}

/// Kind of embedding model; selects the adapter of an engine.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelKind {
    /// A text model: documents and queries.
    #[default]
    Text,
    /// A cross-modal model: text queries and images in one space, no document text.
    TextToImage,
}

impl ModelKind {
    /// True for [`ModelKind::Text`].
    #[must_use]
    pub const fn is_text(&self) -> bool {
        matches!(self, Self::Text)
    }
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
    /// Modality the run embeds (PLAN §0.1: one modality per run); defaults to text.
    pub modality: Option<Modality>,
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
    /// A combined configuration pairs legs that cannot serve their roles.
    #[error("invalid combined config {path}: {reason}")]
    Combination {
        /// Path of the combined configuration.
        path: PathBuf,
        /// Why the pair is refused.
        reason: String,
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
        if let Some(modality) = self.ingest.modality {
            settings.modality = modality;
        }
        if let Some(lang) = &self.ingest.language {
            settings.language.clone_from(lang);
        }
        settings.extraction = self.extraction.clone();
        settings.chunking = self.chunking.clone();
        settings
    }
}

/// A saved combined-retrieval configuration as written: an ordered pair of experiment
/// configurations (text leg, image leg) and the fusion parameters (PRD §6.6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombinedConfigFile {
    /// Text-leg configuration, relative to this file's directory.
    pub text: PathBuf,
    /// Image-leg configuration, relative to this file's directory.
    pub image: PathBuf,
    /// Fusion parameters (RRF, `k = 60`, depth `"all"` when absent).
    #[serde(default)]
    pub fusion: FusionParams,
}

/// A loaded combined-retrieval configuration whose legs were checked for their roles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CombinedConfig {
    /// Text leg: a text model over a text run.
    pub text: ExperimentConfig,
    /// Image leg: a cross-modal model over an image run.
    pub image: ExperimentConfig,
    /// Fusion parameters.
    pub fusion: FusionParams,
}

impl CombinedConfig {
    /// Loads a combined configuration and both leg configurations.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Read`] or [`ConfigError::Parse`] for any of the three
    /// files, and [`ConfigError::Combination`] when the text leg is not a text model
    /// over a text run or the image leg is not a cross-modal model over an image run
    /// (an unsupported combination is refused before any processing).
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_owned(),
            source,
        })?;
        let file: CombinedConfigFile =
            toml::from_str(&text).map_err(|source| ConfigError::Parse {
                path: path.to_owned(),
                source,
            })?;
        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        let config = Self {
            text: ExperimentConfig::load(&dir.join(&file.text))?,
            image: ExperimentConfig::load(&dir.join(&file.image))?,
            fusion: file.fusion,
        };
        config
            .check_roles()
            .map_err(|reason| ConfigError::Combination {
                path: path.to_owned(),
                reason: reason.to_owned(),
            })?;
        Ok(config)
    }

    fn check_roles(&self) -> Result<(), &'static str> {
        let modality = |c: &ExperimentConfig| c.ingest.modality.unwrap_or(Modality::Text);
        if self.text.engine.model_kind != ModelKind::Text || modality(&self.text) != Modality::Text
        {
            return Err("the text leg must be a text model over a text run");
        }
        if self.image.engine.model_kind != ModelKind::TextToImage
            || modality(&self.image) != Modality::Image
        {
            return Err(
                "the image leg must be a cross-modal text-to-image model over an image run",
            );
        }
        Ok(())
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
    fn image_configs_select_a_cross_modal_model_and_an_image_run() {
        for file in [
            "image-clip-vit-b32.toml",
            "image-siglip-multilingual.toml",
            "image-deterministic.toml",
        ] {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("config/experiments")
                .join(file);
            let config = ExperimentConfig::load(&path).expect("load");
            assert_eq!(config.engine.model_kind, ModelKind::TextToImage, "{file}");
            let settings = config.ingest_settings("exp-1".parse().expect("id"));
            assert_eq!(settings.modality, Modality::Image, "{file}");
        }
        let text = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("config/experiments/baseline-text-embed-anything.toml");
        let config = ExperimentConfig::load(&text).expect("load");
        assert_eq!(config.engine.model_kind, ModelKind::Text);
        assert_eq!(
            config
                .ingest_settings("exp-1".parse().expect("id"))
                .modality,
            Modality::Text
        );
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

    #[test]
    fn combined_configs_pair_a_text_leg_with_an_image_leg() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("config/experiments");
        for file in [
            "combined-e5-base-siglip.toml",
            "combined-minilm-clip.toml",
            "combined-deterministic.toml",
        ] {
            let config = CombinedConfig::load(&dir.join(file)).expect(file);
            assert_eq!(config.fusion, FusionParams::default(), "{file}");
            assert_eq!(config.text.engine.model_kind, ModelKind::Text, "{file}");
            assert_eq!(
                config.image.engine.model_kind,
                ModelKind::TextToImage,
                "{file}"
            );
        }
    }

    #[test]
    fn combined_configs_refuse_swapped_or_unsupported_legs() {
        let dir = tempfile::tempdir().expect("tmp");
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("config/experiments");
        let write = |name: &str, text: &str| {
            let path = dir.path().join(name);
            std::fs::write(&path, text).expect("write");
            path
        };
        let leg = |name: &str| repo.join(name).display().to_string();
        let swapped = write(
            "swapped.toml",
            &format!(
                "text = {:?}\nimage = {:?}\n",
                leg("image-clip-vit-b32.toml"),
                leg("baseline-text-embed-anything.toml")
            ),
        );
        assert!(matches!(
            CombinedConfig::load(&swapped),
            Err(ConfigError::Combination { .. })
        ));
        let two_text = write(
            "two-text.toml",
            &format!(
                "text = {:?}\nimage = {:?}\n",
                leg("baseline-text-embed-anything.toml"),
                leg("text-fastembed-minilm.toml")
            ),
        );
        assert!(matches!(
            CombinedConfig::load(&two_text),
            Err(ConfigError::Combination { .. })
        ));
        let typo = write(
            "typo.toml",
            &format!(
                "text = {:?}\nimage = {:?}\n[fusion]\nkk = 60\n",
                leg("baseline-text-embed-anything.toml"),
                leg("image-clip-vit-b32.toml")
            ),
        );
        assert!(matches!(
            CombinedConfig::load(&typo),
            Err(ConfigError::Parse { .. })
        ));
    }
}
