//! Helpers shared by step modules: expected fixture behaviour and provenance checks.

use std::collections::BTreeMap;
use std::num::NonZeroU32;

use anyhow::{Context, Result, ensure};
use embeddings_fundamentals::assets::MetadataScrubber;
use embeddings_fundamentals::domain::{ExtractionMethod, Modality};
use embeddings_fundamentals::extraction::images::ImageExtractor;
use embeddings_fundamentals::extraction::{OcrEngine, TextExtractor};
use embeddings_fundamentals::record::Record;
use serde::Deserialize;

use crate::{LabWorld, fixture};

/// Expected extraction behaviour of one fixture (`tests/fixtures/corpus/expected.toml`).
#[derive(Debug, Deserialize)]
pub struct ExpectedSource {
    pub source_id: String,
    pub pages: Vec<String>,
    pub text_records: Vec<usize>,
    pub image_records: Vec<ExpectedImages>,
}

/// Expected image records on one page.
#[derive(Debug, Deserialize)]
pub struct ExpectedImages {
    pub page: u32,
    pub count: usize,
}

#[derive(Debug, Deserialize)]
struct ExpectedFile {
    sources: Vec<ExpectedSource>,
}

/// Expected behaviour of fixture `id`.
pub fn expected(id: &str) -> Result<ExpectedSource> {
    let text =
        std::fs::read_to_string(fixture("corpus/expected.toml")).context("read expected.toml")?;
    let file: ExpectedFile = toml::from_str(&text).context("parse expected.toml")?;
    file.sources
        .into_iter()
        .find(|s| s.source_id == id)
        .with_context(|| format!("no expectation for {id}"))
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

/// Checks that an image record's stored asset is intact and is exactly the
/// metadata-free re-encoding of its original: the image file itself, or the embedded
/// image it names on its PDF page.
async fn assert_image_resolves(
    world: &LabWorld,
    record: &Record,
    path: &std::path::Path,
) -> Result<()> {
    ensure!(
        record.chunk_text.is_none(),
        "image record {} has chunk text",
        record.record_id
    );
    let uri = record
        .asset_uri
        .as_deref()
        .with_context(|| format!("image record {} has no asset uri", record.record_id))?;
    let stored = world
        .store
        .read_asset(uri)
        .context("read the stored asset")?;
    let original = record
        .metadata
        .original_asset
        .as_ref()
        .with_context(|| format!("image record {} has no original asset", record.record_id))?;
    ensure!(
        original.file == record.metadata.source_file
            && original.page == record.metadata.page_number,
        "record {} original asset {original:?} disagrees with its metadata",
        record.record_id
    );
    let source_bytes = match record.metadata.extraction_method {
        ExtractionMethod::ImageFile => std::fs::read(path).context("read the image file")?,
        ExtractionMethod::EmbeddedImage => {
            let num = original
                .pdf_image_num
                .context("embedded image without its number")?;
            let listed = world.extractor.list_images(path).await?;
            let image = listed
                .iter()
                .find(|i| i.num == num && i.page.get() == original.page)
                .with_context(|| {
                    format!(
                        "image {num} is not on page {} of {}",
                        original.page, original.file
                    )
                })?;
            world.extractor.extract_image(path, image).await?
        }
        other => anyhow::bail!(
            "image record {} has text extraction method {other:?}",
            record.record_id
        ),
    };
    let prepared = MetadataScrubber.scrub(&source_bytes)?;
    ensure!(
        prepared.png == stored,
        "asset of record {} is not the re-encoded original",
        record.record_id
    );
    ensure!(
        (prepared.width, prepared.height) == (original.width, original.height),
        "record {} records size {}x{}, the original is {}x{}",
        record.record_id,
        original.width,
        original.height,
        prepared.width,
        prepared.height
    );
    Ok(())
}

/// Checks that a record points at an existing page of its original file and that
/// every line of its text occurs on that page of the extraction fixture; image records
/// must resolve to their original asset.
pub async fn assert_resolves_to_original(world: &LabWorld, record: &Record) -> Result<()> {
    let path = world.corpus_dir.path().join(&record.metadata.source_file);
    ensure!(
        path.is_file(),
        "record {} points at missing file {}",
        record.record_id,
        path.display()
    );
    if record.modality == Modality::Image {
        return assert_image_resolves(world, record, &path).await;
    }
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
        other => anyhow::bail!("text record {} has method {other:?}", record.record_id),
    };
    let page_lines: Vec<String> = page_text
        .lines
        .iter()
        .map(|l| l.text.trim().to_owned())
        .collect();
    let text = record
        .chunk_text
        .as_deref()
        .context("text record without text")?;
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
