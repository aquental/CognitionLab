//! Text extraction and OCR (PRD §6.2, PLAN §0.1).
//!
//! Extraction is independent of the embedding engine. Each page is read from the PDF
//! text layer when that layer passes the native-text rule, and is otherwise rendered
//! and OCR'd. Empty OCR output is reported, never indexed as valid text.

pub mod fixture;
pub mod poppler;
pub mod tsv;

use std::future::Future;
use std::num::NonZeroU32;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::domain::ExtractionMethod;

/// Axis-aligned box in PDF points with a top-left origin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundingBox {
    /// Left edge in points.
    pub x: f32,
    /// Top edge in points (top-left origin).
    pub y: f32,
    /// Width in points.
    pub w: f32,
    /// Height in points.
    pub h: f32,
    /// Always `"pt"`.
    pub unit: String,
    /// Always `"top-left"`.
    pub origin: String,
}

impl BoundingBox {
    /// Builds a box in points with a top-left origin.
    #[must_use]
    pub fn points(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            x,
            y,
            w,
            h,
            unit: "pt".to_owned(),
            origin: "top-left".to_owned(),
        }
    }

    /// Smallest box containing both boxes.
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        let left = self.x.min(other.x);
        let top = self.y.min(other.y);
        let right = (self.x + self.w).max(other.x + other.w);
        let bottom = (self.y + self.h).max(other.y + other.h);
        Self::points(left, top, right - left, bottom - top)
    }
}

/// One line of extracted text in reading order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextLine {
    /// Line text with trailing whitespace removed.
    pub text: String,
    /// Line box when the extractor provides positions (OCR).
    pub bounding_box: Option<BoundingBox>,
}

/// Extracted text of one page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageText {
    /// 1-based page number.
    pub page: NonZeroU32,
    /// How the text was obtained.
    pub method: ExtractionMethod,
    /// Tool and version, for example `tesseract 5.5.3`.
    pub engine: String,
    /// Lines in reading order.
    pub lines: Vec<TextLine>,
    /// Mean word confidence in `[0, 1]` when the tool reports it.
    pub confidence: Option<f32>,
}

impl PageText {
    /// The page text, one line per line.
    #[must_use]
    pub fn text(&self) -> String {
        self.lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// True when the page has no visible characters.
    #[must_use]
    pub fn is_blank(&self) -> bool {
        self.lines.iter().all(|l| l.text.trim().is_empty())
    }
}

/// Errors raised by extractors.
#[derive(Debug, thiserror::Error)]
pub enum ExtractionError {
    /// An external tool could not be started, timed out, or exited with an error.
    #[error("{tool} failed: {reason}")]
    Tool {
        /// Tool name.
        tool: &'static str,
        /// Short reason without document content.
        reason: String,
    },
    /// A file could not be read or written.
    #[error("i/o error during extraction")]
    Io(#[from] std::io::Error),
    /// Tool output could not be parsed.
    #[error("cannot parse {what}: {reason}")]
    Parse {
        /// What was being parsed.
        what: &'static str,
        /// Reason.
        reason: String,
    },
    /// Two page-count sources disagree.
    #[error("page count mismatch: pdfinfo reports {pdfinfo}, lopdf reports {lopdf}")]
    PageCountMismatch {
        /// Count reported by `pdfinfo`.
        pdfinfo: u32,
        /// Count reported by `lopdf`.
        lopdf: u32,
    },
    /// No fixture data exists for the requested page.
    #[error("no extraction fixture for {file} page {page}")]
    MissingFixture {
        /// File stem.
        file: String,
        /// Page number.
        page: u32,
    },
}

/// Extraction settings. Their canonical JSON is part of the extraction fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractionConfig {
    /// Minimum non-whitespace characters for a usable text layer.
    pub min_native_chars: usize,
    /// Minimum percentage of non-whitespace characters that are letters or digits.
    pub min_alnum_percent: u8,
    /// OCR render resolution.
    pub ocr_dpi: u32,
    /// Tesseract languages.
    pub ocr_lang: String,
    /// Tesseract page segmentation mode.
    pub ocr_psm: u8,
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            min_native_chars: 200,
            min_alnum_percent: 50,
            ocr_dpi: 300,
            ocr_lang: "por+eng".to_owned(),
            ocr_psm: 6,
        }
    }
}

impl ExtractionConfig {
    /// The native-text rule: enough visible characters, mostly letters and digits.
    ///
    /// Character share (not token share) is used because table separators such as
    /// `|` are whole tokens: a pipe-separated lab table has only ~57 % word-like
    /// tokens but ~74 % alphanumeric characters.
    #[must_use]
    pub fn native_text_is_usable(&self, text: &str) -> bool {
        let visible: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
        if visible.len() < self.min_native_chars {
            return false;
        }
        let alnum = visible.iter().filter(|c| c.is_alphanumeric()).count();
        alnum * 100 >= visible.len() * usize::from(self.min_alnum_percent)
    }
}

/// Reads PDF text layers.
pub trait TextExtractor: Send + Sync {
    /// Tool identity and version, included in the extraction fingerprint.
    fn engine_id(&self) -> String;

    /// Number of pages in the PDF.
    fn page_count(
        &self,
        pdf: &Path,
    ) -> impl Future<Output = Result<NonZeroU32, ExtractionError>> + Send;

    /// Text layer of one page (method `native`).
    fn extract_page(
        &self,
        pdf: &Path,
        page: NonZeroU32,
    ) -> impl Future<Output = Result<PageText, ExtractionError>> + Send;
}

/// Renders and recognizes PDF pages.
pub trait OcrEngine: Send + Sync {
    /// Tool identity and version, included in the extraction fingerprint.
    fn engine_id(&self) -> String;

    /// OCR of one page (method `ocr`).
    fn ocr_page(
        &self,
        pdf: &Path,
        page: NonZeroU32,
    ) -> impl Future<Output = Result<PageText, ExtractionError>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_rule_requires_enough_wordlike_text() {
        let config = ExtractionConfig::default();
        let lab = "Hemoglobina | 13,5 | g/dL | 12,0–16,0\n".repeat(10);
        assert!(config.native_text_is_usable(&lab));
        assert!(!config.native_text_is_usable("Hemograma"), "too short");
        // Pipe-separated table only: 57 % word-like tokens, 74 % alphanumeric characters.
        let table_only = "Hemoglobina | 13,5 | g/dL | 12,0–16,0\n".repeat(10);
        assert!(config.native_text_is_usable(&table_only));
        let garbage = "\u{2022} \u{25a0} | | | ~ ".repeat(60);
        assert!(!config.native_text_is_usable(&garbage), "mostly symbols");
        assert!(!config.native_text_is_usable(""));
    }

    #[test]
    fn bounding_box_union_covers_both() {
        let a = BoundingBox::points(10.0, 20.0, 30.0, 5.0);
        let b = BoundingBox::points(50.0, 18.0, 10.0, 10.0);
        let u = a.union(&b);
        assert_eq!((u.x, u.y, u.w, u.h), (10.0, 18.0, 50.0, 10.0));
        assert_eq!((u.unit.as_str(), u.origin.as_str()), ("pt", "top-left"));
    }
}
