//! Deterministic extractor that replays captured tool output (PLAN §0.1).
//!
//! Pages are keyed by the PDF file stem, so a copied fixture keeps its extraction. The
//! directory layout is `<root>/<file-stem>/p<N>.native.txt` and `p<N>.ocr.tsv`, captured
//! from the real tools by `cargo run --example capture_extraction`. OCR replays go
//! through the same TSV parser as live OCR.

use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

use super::poppler::native_lines;
use super::{ExtractionError, OcrEngine, PageText, TextExtractor, tsv};
use crate::domain::ExtractionMethod;

/// Engine id recorded for replayed text layers.
pub const FIXTURE_NATIVE_ENGINE: &str = "fixture-pdftotext";
/// Engine id recorded for replayed OCR.
pub const FIXTURE_OCR_ENGINE: &str = "fixture-tesseract";
/// Resolution the captured OCR boxes refer to.
pub const FIXTURE_OCR_DPI: u32 = 300;

#[derive(Debug, Default, Clone)]
struct FixturePage {
    native: Option<String>,
    ocr_tsv: Option<String>,
}

#[derive(Debug, Default)]
struct State {
    pages: BTreeMap<(String, u32), FixturePage>,
    page_counts: BTreeMap<String, u32>,
    ocr_calls: Vec<(String, u32)>,
    failing: BTreeMap<String, String>,
}

/// Replays captured extraction output; steps may also register pages directly.
#[derive(Debug, Default)]
pub struct FixtureExtractor {
    state: Mutex<State>,
}

fn stem(pdf: &Path) -> String {
    pdf.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

impl FixtureExtractor {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Loads every `<stem>/p<N>.{native.txt,ocr.tsv}` file under `root`.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when the directory cannot be read.
    pub fn from_dir(root: &Path) -> Result<Self, ExtractionError> {
        let extractor = Self::default();
        for entry in std::fs::read_dir(root)? {
            let dir = entry?.path();
            if !dir.is_dir() {
                continue;
            }
            let file = stem(&dir.with_extension("pdf"));
            for page_entry in std::fs::read_dir(&dir)? {
                let path = page_entry?.path();
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let Some(rest) = name.strip_prefix('p') else {
                    continue;
                };
                let (number, kind) = rest.split_once('.').unwrap_or((rest, ""));
                let Ok(page) = number.parse::<u32>() else {
                    continue;
                };
                let content = std::fs::read_to_string(&path)?;
                match kind {
                    "native.txt" => extractor.set_native(&file, page, &content),
                    "ocr.tsv" => extractor.set_ocr_tsv(&file, page, &content),
                    _ => {}
                }
            }
        }
        Ok(extractor)
    }

    fn bump_count(state: &mut State, file: &str, page: u32) {
        let count = state.page_counts.entry(file.to_owned()).or_insert(0);
        *count = (*count).max(page);
    }

    /// Registers the text layer of a page.
    pub fn set_native(&self, file: &str, page: u32, text: &str) {
        let mut state = self.state();
        state
            .pages
            .entry((file.to_owned(), page))
            .or_default()
            .native = Some(text.to_owned());
        Self::bump_count(&mut state, file, page);
    }

    /// Registers the Tesseract TSV of a page.
    pub fn set_ocr_tsv(&self, file: &str, page: u32, tsv: &str) {
        let mut state = self.state();
        state
            .pages
            .entry((file.to_owned(), page))
            .or_default()
            .ocr_tsv = Some(tsv.to_owned());
        Self::bump_count(&mut state, file, page);
    }

    /// Makes every extraction of `file` fail with `reason`.
    pub fn fail_file(&self, file: &str, reason: &str) {
        self.state()
            .failing
            .insert(file.to_owned(), reason.to_owned());
    }

    /// Pages that were sent to OCR, in call order.
    #[must_use]
    pub fn ocr_calls(&self) -> Vec<(String, u32)> {
        self.state().ocr_calls.clone()
    }

    fn check_failing(&self, file: &str) -> Result<(), ExtractionError> {
        match self.state().failing.get(file) {
            Some(reason) => Err(ExtractionError::Tool {
                tool: "fixture",
                reason: reason.clone(),
            }),
            None => Ok(()),
        }
    }
}

impl TextExtractor for FixtureExtractor {
    fn engine_id(&self) -> String {
        FIXTURE_NATIVE_ENGINE.to_owned()
    }

    async fn page_count(&self, pdf: &Path) -> Result<NonZeroU32, ExtractionError> {
        let file = stem(pdf);
        self.check_failing(&file)?;
        self.state()
            .page_counts
            .get(&file)
            .copied()
            .and_then(NonZeroU32::new)
            .ok_or(ExtractionError::MissingFixture { file, page: 0 })
    }

    async fn extract_page(
        &self,
        pdf: &Path,
        page: NonZeroU32,
    ) -> Result<PageText, ExtractionError> {
        let file = stem(pdf);
        self.check_failing(&file)?;
        let native = self
            .state()
            .pages
            .get(&(file, page.get()))
            .and_then(|p| p.native.clone());
        Ok(PageText {
            page,
            method: ExtractionMethod::Native,
            engine: FIXTURE_NATIVE_ENGINE.to_owned(),
            lines: native_lines(native.as_deref().unwrap_or("")),
            confidence: None,
        })
    }
}

impl OcrEngine for FixtureExtractor {
    fn engine_id(&self) -> String {
        FIXTURE_OCR_ENGINE.to_owned()
    }

    async fn ocr_page(&self, pdf: &Path, page: NonZeroU32) -> Result<PageText, ExtractionError> {
        let file = stem(pdf);
        self.check_failing(&file)?;
        let tsv_text = {
            let mut state = self.state();
            state.ocr_calls.push((file.clone(), page.get()));
            state
                .pages
                .get(&(file.clone(), page.get()))
                .and_then(|p| p.ocr_tsv.clone())
        };
        let tsv_text = tsv_text.ok_or(ExtractionError::MissingFixture {
            file,
            page: page.get(),
        })?;
        tsv::parse(&tsv_text, page, FIXTURE_OCR_DPI, FIXTURE_OCR_ENGINE)
    }
}
