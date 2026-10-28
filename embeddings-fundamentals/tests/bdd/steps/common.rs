//! Helpers shared by step modules: expected fixture behaviour and provenance checks.

use std::collections::BTreeMap;
use std::num::NonZeroU32;

use anyhow::{Context, Result, ensure};
use embeddings_fundamentals::domain::{ExtractionMethod, Modality};
use embeddings_fundamentals::extraction::{OcrEngine, TextExtractor};
use embeddings_fundamentals::record::Record;
use serde::Deserialize;

use crate::{LabWorld, fixture};

/// Expected extraction behaviour of one fixture (`tests/fixtures/corpus/expected.toml`).
/// `image_records` is read from milestone 3 on, when image runs exist.
#[derive(Debug, Deserialize)]
pub struct ExpectedSource {
    pub source_id: String,
    pub pages: Vec<String>,
    pub text_records: Vec<usize>,
}

#[derive(Debug, Deserialize)]
struct ExpectedFile {
    sources: Vec<ExpectedSource>,
}

/// Expected behaviour of fixture `id`.
pub fn expected(id: &str) -> Result<ExpectedSource> {
    let text = std::fs::read_to_string(fixture("corpus/expected.toml")).context("read expected.toml")?;
    let file: ExpectedFile = toml::from_str(&text).context("parse expected.toml")?;
    file.sources.into_iter().find(|s| s.source_id == id).with_context(|| format!("no expectation for {id}"))
}

/// Fixture id behind a Gherkin input-type name.
pub fn fixture_for_type(input_type: &str) -> Result<&'static str> {
    Ok(match input_type {
        "searchable PDF" => "lab-searchable-001",
        "scanned PDF" => "lab-scanned-002",
        "mixed PDF" => "us-mixed-003",
        "PNG" => "chest-001",
        "JPEG" => "abd-us-001",
        other => anyhow::bail!("unknown input type {other:?}"),
    })
}

/// Active text records per page: `{page: count}`.
pub fn text_records_per_page(records: &[Record]) -> BTreeMap<u32, usize> {
    let mut counts = BTreeMap::new();
    for r in records.iter().filter(|r| r.modality == Modality::Text) {
        *counts.entry(r.metadata.page_number).or_insert(0) += 1;
    }
    counts
}

/// Checks that a record points at an existing page of its original file and that
/// every line of its text occurs on that page of the extraction fixture.
pub async fn assert_resolves_to_original(world: &LabWorld, record: &Record) -> Result<()> {
    let path = world.corpus_dir.path().join(&record.metadata.source_file);
    ensure!(path.is_file(), "record {} points at missing file {}", record.record_id, path.display());
    let doc = lopdf::Document::load(&path).context("open original PDF")?;
    let pages = u32::try_from(doc.get_pages().len())?;
    ensure!(
        (1..=pages).contains(&record.metadata.page_number),
        "record {} cites page {} of a {pages}-page file",
        record.record_id,
        record.metadata.page_number
    );
    let page = NonZeroU32::new(record.metadata.page_number).context("page 0")?;
    let page_text = match record.metadata.extraction_method {
        ExtractionMethod::Native => world.extractor.extract_page(&path, page).await?,
        ExtractionMethod::Ocr => world.extractor.ocr_page(&path, page).await?,
    };
    let page_lines: Vec<String> = page_text.lines.iter().map(|l| l.text.trim().to_owned()).collect();
    let text = record.chunk_text.as_deref().context("text record without text")?;
    for line in text.lines() {
        ensure!(
            page_lines.iter().any(|l| l == line.trim()),
            "line {line:?} of record {} is not on page {page} of {}",
            record.record_id,
            record.metadata.source_file
        );
    }
    Ok(())
}
