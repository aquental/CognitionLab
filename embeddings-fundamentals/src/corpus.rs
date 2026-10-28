//! Corpus manifest: source identity, checksums, and the anonymization eligibility gate
//! (PRD §4, §11; PLAN §4.4).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::domain::{DocumentType, InputType};
use crate::ids::{Checksum, SourceId};

/// Errors raised while loading a manifest.
#[derive(Debug, thiserror::Error)]
pub enum CorpusError {
    /// The manifest could not be read.
    #[error("cannot read manifest {path}")]
    Read {
        /// Manifest path.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// The manifest is not valid TOML for the expected schema.
    #[error("invalid manifest {path}")]
    Parse {
        /// Manifest path.
        path: PathBuf,
        /// Parse error.
        #[source]
        source: toml::de::Error,
    },
    /// Two sources share an identifier.
    #[error("duplicate source id {0} in manifest")]
    DuplicateSource(SourceId),
}

/// Why a source may not be processed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Ineligible {
    /// The manifest marks the source as not eligible.
    #[error("source is marked not eligible in the manifest")]
    NotEligible,
    /// One or more anonymization checks are missing or not passed.
    #[error("anonymization checks incomplete: {missing:?}")]
    AnonymizationIncomplete {
        /// Names of the missing or failed checks.
        missing: Vec<&'static str>,
    },
    /// The file could not be read.
    #[error("source file {file} cannot be read: {reason}")]
    Unreadable {
        /// File path relative to the manifest.
        file: PathBuf,
        /// I/O error message.
        reason: String,
    },
    /// The file bytes do not match the manifest checksum.
    #[error("checksum mismatch: manifest {expected}, file {actual}")]
    ChecksumMismatch {
        /// Checksum recorded in the manifest.
        expected: Checksum,
        /// Checksum of the file on disk.
        actual: Checksum,
    },
}

/// Result of one anonymization check recorded by a reviewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckResult {
    /// The check passed.
    Ok,
    /// The check found identifying information.
    Failed,
}

/// Anonymization checks required before a source is eligible (PRD §11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Anonymization {
    /// Filename reviewed for identifiers.
    pub filename: Option<CheckResult>,
    /// Embedded metadata (PDF info/XMP, EXIF/IPTC/XMP) reviewed.
    pub metadata: Option<CheckResult>,
    /// Document text reviewed.
    pub text: Option<CheckResult>,
    /// Image pixels reviewed.
    pub pixels: Option<CheckResult>,
    /// Who reviewed the source.
    #[serde(default)]
    pub reviewer: String,
    /// When the review happened (ISO date).
    #[serde(default)]
    pub date: String,
}

impl Anonymization {
    /// Names of checks that are missing or did not pass, including reviewer and date.
    #[must_use]
    pub fn missing(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        for (name, check) in [
            ("filename", self.filename),
            ("metadata", self.metadata),
            ("text", self.text),
            ("pixels", self.pixels),
        ] {
            if check != Some(CheckResult::Ok) {
                missing.push(name);
            }
        }
        if self.reviewer.trim().is_empty() {
            missing.push("reviewer");
        }
        if self.date.trim().is_empty() {
            missing.push("date");
        }
        missing
    }
}

/// One source entry of the manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceEntry {
    /// Stable source identifier.
    pub source_id: SourceId,
    /// File path relative to the manifest directory.
    pub file: PathBuf,
    /// Expected checksum of the file.
    pub checksum: Checksum,
    /// Document kind.
    pub document_type: DocumentType,
    /// Input format.
    pub input_type: InputType,
    /// Whether the source may be processed at all.
    pub eligible: bool,
    /// Whether a failure of this source makes a run incomplete.
    #[serde(default = "default_true")]
    pub required: bool,
    /// Anonymization review; absent means not reviewed (ineligible).
    pub anonymization: Option<Anonymization>,
}

const fn default_true() -> bool {
    true
}

/// A corpus manifest as stored on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Corpus identifier.
    pub corpus_id: String,
    /// Remote-provider processing is allowed for this corpus. Defaults to `false`.
    #[serde(default)]
    pub remote_allowed: bool,
    /// Sources in the corpus.
    #[serde(default)]
    pub sources: Vec<SourceEntry>,
}

/// A manifest together with the directory its relative paths resolve against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedManifest {
    /// Parsed manifest.
    pub manifest: Manifest,
    /// Directory containing the manifest.
    pub base_dir: PathBuf,
    /// Checksum of the manifest file itself (corpus fingerprint).
    pub manifest_checksum: Checksum,
}

impl LoadedManifest {
    /// Loads and validates a manifest file.
    ///
    /// # Errors
    ///
    /// Returns [`CorpusError`] when the file cannot be read or parsed, or when two
    /// sources share an identifier.
    pub fn load(path: &Path) -> Result<Self, CorpusError> {
        let bytes = std::fs::read(path).map_err(|source| CorpusError::Read {
            path: path.to_owned(),
            source,
        })?;
        let text = String::from_utf8_lossy(&bytes);
        let manifest: Manifest = toml::from_str(&text).map_err(|source| CorpusError::Parse {
            path: path.to_owned(),
            source,
        })?;
        Self::from_manifest(manifest, path.parent().unwrap_or(Path::new(".")), &bytes)
    }

    /// Wraps an in-memory manifest, as used by tests that build corpora on the fly.
    ///
    /// # Errors
    ///
    /// Returns [`CorpusError::DuplicateSource`] when two sources share an identifier.
    pub fn from_manifest(
        manifest: Manifest,
        base_dir: &Path,
        raw: &[u8],
    ) -> Result<Self, CorpusError> {
        let mut seen = std::collections::BTreeSet::new();
        for source in &manifest.sources {
            if !seen.insert(source.source_id.clone()) {
                return Err(CorpusError::DuplicateSource(source.source_id.clone()));
            }
        }
        Ok(Self {
            manifest,
            base_dir: base_dir.to_owned(),
            manifest_checksum: Checksum::of_bytes(raw),
        })
    }

    /// Absolute path of a source file.
    #[must_use]
    pub fn path_of(&self, source: &SourceEntry) -> PathBuf {
        self.base_dir.join(&source.file)
    }

    /// Looks up a source by identifier.
    #[must_use]
    pub fn source(&self, id: &SourceId) -> Option<&SourceEntry> {
        self.manifest.sources.iter().find(|s| &s.source_id == id)
    }

    /// Checks eligibility, the anonymization gate, and the file checksum, and returns
    /// the file bytes' checksum on success.
    ///
    /// # Errors
    ///
    /// Returns the first [`Ineligible`] reason found.
    pub fn verify(&self, source: &SourceEntry) -> Result<Checksum, Ineligible> {
        if !source.eligible {
            return Err(Ineligible::NotEligible);
        }
        let missing = source.anonymization.as_ref().map_or_else(
            || vec!["filename", "metadata", "text", "pixels", "reviewer", "date"],
            Anonymization::missing,
        );
        if !missing.is_empty() {
            return Err(Ineligible::AnonymizationIncomplete { missing });
        }
        let bytes = std::fs::read(self.path_of(source)).map_err(|e| Ineligible::Unreadable {
            file: source.file.clone(),
            reason: e.to_string(),
        })?;
        let actual = Checksum::of_bytes(&bytes);
        if actual != source.checksum {
            return Err(Ineligible::ChecksumMismatch {
                expected: source.checksum.clone(),
                actual,
            });
        }
        Ok(actual)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_manifest() -> LoadedManifest {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/corpus/manifest.toml");
        LoadedManifest::load(&path).expect("load fixture manifest")
    }

    #[test]
    fn fixture_manifest_loads_and_every_source_verifies() {
        let loaded = fixture_manifest();
        assert!(!loaded.manifest.remote_allowed);
        assert_eq!(loaded.manifest.sources.len(), 5);
        for source in &loaded.manifest.sources {
            assert_eq!(
                loaded.verify(source).as_ref(),
                Ok(&source.checksum),
                "{}",
                source.source_id
            );
        }
    }

    #[test]
    fn remote_allowed_defaults_to_false() {
        let manifest: Manifest = toml::from_str("corpus_id = \"c\"").expect("parse");
        assert!(!manifest.remote_allowed);
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let err = toml::from_str::<Manifest>("corpus_id = \"c\"\nremote_alowed = true")
            .expect_err("typo");
        assert!(err.to_string().contains("remote_alowed"), "{err}");
    }

    #[test]
    fn incomplete_anonymization_makes_a_source_ineligible() {
        let loaded = fixture_manifest();
        let mut source = loaded.manifest.sources[0].clone();
        if let Some(a) = source.anonymization.as_mut() {
            a.pixels = None;
            a.reviewer.clear();
        }
        assert_eq!(
            loaded.verify(&source),
            Err(Ineligible::AnonymizationIncomplete {
                missing: vec!["pixels", "reviewer"]
            })
        );
        source.anonymization = None;
        assert!(matches!(
            loaded.verify(&source),
            Err(Ineligible::AnonymizationIncomplete { .. })
        ));
    }

    #[test]
    fn checksum_mismatch_and_ineligibility_are_reported() {
        let loaded = fixture_manifest();
        let mut source = loaded.manifest.sources[0].clone();
        source.checksum = Checksum::of_bytes(b"other");
        assert!(matches!(
            loaded.verify(&source),
            Err(Ineligible::ChecksumMismatch { .. })
        ));
        source.eligible = false;
        assert_eq!(loaded.verify(&source), Err(Ineligible::NotEligible));
    }

    #[test]
    fn duplicate_source_ids_are_rejected() {
        let mut manifest = fixture_manifest().manifest;
        manifest.sources.push(manifest.sources[0].clone());
        assert!(matches!(
            LoadedManifest::from_manifest(manifest, Path::new("."), b""),
            Err(CorpusError::DuplicateSource(_))
        ));
    }
}
