//! Closed sets of domain values shared across modules (PLAN §2.3: enums, not strings).

use serde::{Deserialize, Serialize};

/// Modality of an indexed item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Modality {
    /// Extracted text.
    Text,
    /// An image asset.
    Image,
}

impl Modality {
    /// Canonical lowercase name used in identifiers and records.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Image => "image",
        }
    }
}

/// Where a process runs, used for remote-permission checks and cost reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Execution {
    /// Runs inside this process or on this machine.
    Local,
    /// Sends data to a remote provider.
    Remote,
}

/// Kind of document recorded in the manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentType {
    /// Laboratory results.
    LaboratoryReport,
    /// Imaging or other medical report.
    ImagingReport,
    /// Radiograph image.
    Radiograph,
    /// Ultrasound image.
    UltrasoundImage,
}

/// Input format recorded in the manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputType {
    /// PDF with a text layer.
    SearchablePdf,
    /// PDF made of page scans.
    ScannedPdf,
    /// PDF mixing native text, scans, and embedded images.
    MixedPdf,
    /// PNG image.
    Png,
    /// JPEG image.
    Jpeg,
}

impl InputType {
    /// Whether the input is a PDF (text-bearing for text runs).
    #[must_use]
    pub const fn is_pdf(self) -> bool {
        matches!(
            self,
            Self::SearchablePdf | Self::ScannedPdf | Self::MixedPdf
        )
    }
}

/// How the text of a page was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionMethod {
    /// The PDF text layer.
    Native,
    /// Optical character recognition of a rendered page.
    Ocr,
}

impl ExtractionMethod {
    /// Lowercase name used in records and Gherkin steps.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Ocr => "ocr",
        }
    }
}

/// Pipeline stage where a source failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureStage {
    /// Manifest checks: checksum or eligibility.
    Manifest,
    /// Text extraction or OCR.
    Extraction,
    /// Chunking and input-limit checks.
    Chunking,
    /// Calls to the embedding adapter.
    Embedding,
    /// Vector validation.
    Validation,
    /// Staging or publishing records.
    Storage,
}

impl FailureStage {
    /// Lowercase name used in reports and Gherkin steps.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Manifest => "manifest",
            Self::Extraction => "extraction",
            Self::Chunking => "chunking",
            Self::Embedding => "embedding",
            Self::Validation => "validation",
            Self::Storage => "storage",
        }
    }
}

/// Outcome of one source in a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceStatus {
    /// A new revision was committed, or the active revision was already current.
    Succeeded,
    /// The source failed; no new records were committed.
    Failed,
    /// The source does not apply to this run's modality.
    NotApplicable,
}

impl SourceStatus {
    /// Lowercase name used in reports and Gherkin steps.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::NotApplicable => "not_applicable",
        }
    }
}

/// Outcome of a whole run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// Every required, applicable source succeeded.
    Succeeded,
    /// At least one required source failed.
    Incomplete,
}

impl RunStatus {
    /// Lowercase name used in reports and Gherkin steps.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Incomplete => "incomplete",
        }
    }
}
