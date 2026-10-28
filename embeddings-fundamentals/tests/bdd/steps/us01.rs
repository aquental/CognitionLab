//! Steps for `us01.feature` (PRD §5.1): ingestion through one pipeline.

use std::collections::BTreeSet;

use anyhow::{Context, Result, ensure};
use cucumber::{given, then, when};
use embeddings_fundamentals::domain::{ExtractionMethod, Modality, SourceStatus};
use embeddings_fundamentals::embedding::deterministic::DeterministicEmbedder;
use embeddings_fundamentals::extraction::fixture::FixtureExtractor;
use embeddings_fundamentals::ids::SourceId;

use super::common::{
    assert_resolves_to_original, expected, fixture_for_type, text_records_per_page,
};
use crate::LabWorld;

fn current_source(world: &LabWorld) -> Result<SourceId> {
    world
        .sources
        .last()
        .map(|s| s.source_id.clone())
        .context("no source in the manifest")
}

#[given(expr = "an eligible anonymized fixture of type {string} in the corpus manifest")]
fn eligible_fixture(world: &mut LabWorld, input_type: String) -> Result<()> {
    world.use_captured_extraction()?;
    let id = fixture_for_type(&input_type)?;
    let entry = world.add_fixture_source(id, None)?;
    ensure!(entry.eligible, "fixture {id} is not eligible");
    world.notes.insert("fixture".into(), id.to_owned());
    Ok(())
}

#[given("a compatible deterministic embedding adapter")]
fn compatible_adapter(world: &mut LabWorld) -> Result<()> {
    world.embedder = std::sync::Arc::new(DeterministicEmbedder::builder().build()?);
    Ok(())
}

#[when("I ingest the fixture")]
async fn ingest_fixture(world: &mut LabWorld) -> Result<()> {
    world.ingest().await
}

#[when("I ingest the page")]
async fn ingest_page(world: &mut LabWorld) -> Result<()> {
    world.ingest().await
}

#[when("I ingest the PDF")]
async fn ingest_pdf(world: &mut LabWorld) -> Result<()> {
    world.ingest().await
}

#[when("I ingest the source")]
async fn ingest_source(world: &mut LabWorld) -> Result<()> {
    world.ingest().await
}

#[then(expr = "the source status is {string}")]
fn source_status(world: &mut LabWorld, status: String) -> Result<()> {
    let id = current_source(world)?;
    let report = world
        .last_run()?
        .source(&id)
        .context("source missing from report")?;
    ensure!(
        report.status.as_str() == status,
        "expected status {status}, observed {} (stage {:?}, reason {:?})",
        report.status.as_str(),
        report.stage,
        report.reason
    );
    Ok(())
}

#[then("the active records match the fixture's expected item count and modalities")]
fn records_match_expected(world: &mut LabWorld) -> Result<()> {
    let fixture = world.notes.get("fixture").context("no fixture")?.clone();
    let expected = expected(&fixture)?;
    let records = world.active_records()?;
    let per_page = text_records_per_page(&records);
    let expected_pages: std::collections::BTreeMap<u32, usize> = expected
        .text_records
        .iter()
        .enumerate()
        .filter(|(_, n)| **n > 0)
        .map(|(i, n)| (u32::try_from(i + 1).unwrap_or(u32::MAX), *n))
        .collect();
    ensure!(
        per_page == expected_pages,
        "text records per page: expected {expected_pages:?}, observed {per_page:?}"
    );
    for r in &records {
        let index = usize::try_from(r.metadata.page_number)? - 1;
        let method = expected
            .pages
            .get(index)
            .context("page outside the fixture")?;
        ensure!(
            r.metadata.extraction_method.as_str() == method,
            "page {} record uses {}, fixture expects {method}",
            r.metadata.page_number,
            r.metadata.extraction_method.as_str()
        );
    }
    let images = records
        .iter()
        .filter(|r| r.modality == Modality::Image)
        .count();
    // Text runs never produce image records; image records are covered from milestone 3.
    ensure!(images == 0, "a text run produced {images} image records");
    Ok(())
}

#[then("every active record has the manifest source ID and checksum")]
fn records_have_source_and_checksum(world: &mut LabWorld) -> Result<()> {
    let entry = world.sources.last().context("no source")?.clone();
    let records = world.active_records()?;
    ensure!(!records.is_empty(), "no active records");
    for r in &records {
        ensure!(
            r.source_id == entry.source_id,
            "record {} has source {}",
            r.record_id,
            r.source_id
        );
        ensure!(
            r.metadata.source_checksum == entry.checksum,
            "record {} has checksum {}, manifest has {}",
            r.record_id,
            r.metadata.source_checksum,
            entry.checksum
        );
    }
    Ok(())
}

#[then("every active record resolves to its original page or image asset")]
async fn records_resolve(world: &mut LabWorld) -> Result<()> {
    for record in world.active_records()? {
        assert_resolves_to_original(world, &record).await?;
    }
    Ok(())
}

/// Builds Tesseract TSV for `lines`, one word per whitespace token, confidence `conf`.
fn tsv_for(lines: &[&str], conf: f64) -> String {
    let mut out = String::from(
        "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n",
    );
    for (l, line) in lines.iter().enumerate() {
        for (w, word) in line.split_whitespace().enumerate() {
            let (left, top) = (100 + w * 120, 200 + l * 60);
            out.push_str(&format!(
                "5\t1\t1\t1\t{}\t{}\t{left}\t{top}\t100\t40\t{conf}\t{word}\n",
                l + 1,
                w + 1
            ));
        }
    }
    out
}

#[given(expr = "a scanned laboratory page containing the row {string}")]
fn scanned_page_with_row(world: &mut LabWorld, row: String) -> Result<()> {
    world.add_fixture_source("lab-scanned-002", None)?;
    world.notes.insert("row".into(), row);
    Ok(())
}

#[given("a controlled OCR fixture returning that row and its header")]
fn controlled_ocr(world: &mut LabWorld) -> Result<()> {
    let row = world.notes.get("row").context("no row")?.clone();
    let extractor = FixtureExtractor::default();
    // Page 1 has no text layer (scanned), so OCR runs; every word has confidence 95.
    extractor.set_native("lab-scanned-002", 1, "");
    let lines = [
        "Hemograma",
        "Exame | Resultado | Unidade | Intervalo de referência",
        row.as_str(),
    ];
    extractor.set_ocr_tsv("lab-scanned-002", 1, &tsv_for(&lines, 95.0));
    world.extractor = std::sync::Arc::new(extractor);
    world.notes.insert("ocr_confidence".into(), "0.95".into());
    Ok(())
}

#[then("a text chunk contains the test name, value, unit, and reference range together")]
fn chunk_keeps_row(world: &mut LabWorld) -> Result<()> {
    let row = world.notes.get("row").context("no row")?.clone();
    let fields: Vec<&str> = row.split('|').map(str::trim).collect();
    ensure!(fields.len() == 4, "row {row:?} does not have 4 fields");
    let records = world.active_records()?;
    let hit = records
        .iter()
        .find(|r| {
            r.chunk_text
                .as_deref()
                .is_some_and(|t| fields.iter().all(|f| t.contains(f)))
        })
        .with_context(|| format!("no chunk contains all of {fields:?}"))?;
    world
        .notes
        .insert("row_record".into(), hit.record_id.to_string());
    Ok(())
}

#[then("that chunk references the original page and the OCR extraction method")]
fn chunk_references_page_and_ocr(world: &mut LabWorld) -> Result<()> {
    let id = world
        .notes
        .get("row_record")
        .context("no row record")?
        .clone();
    let record = world
        .active_records()?
        .into_iter()
        .find(|r| r.record_id.as_str() == id)
        .context("record")?;
    ensure!(
        record.metadata.page_number == 1,
        "page {}",
        record.metadata.page_number
    );
    ensure!(
        record.metadata.source_file == "lab-scanned-002.pdf",
        "file {}",
        record.metadata.source_file
    );
    ensure!(
        record.metadata.extraction_method == ExtractionMethod::Ocr,
        "method {:?}",
        record.metadata.extraction_method
    );
    Ok(())
}

#[then("extraction confidence equals the OCR value or is null when not supplied")]
fn confidence_matches(world: &mut LabWorld) -> Result<()> {
    let id = world
        .notes
        .get("row_record")
        .context("no row record")?
        .clone();
    let expected: f32 = world
        .notes
        .get("ocr_confidence")
        .context("no confidence")?
        .parse()?;
    let record = world
        .active_records()?
        .into_iter()
        .find(|r| r.record_id.as_str() == id)
        .context("record")?;
    let actual = record
        .metadata
        .extraction_confidence
        .context("confidence is null but OCR supplied 95")?;
    ensure!(
        (actual - expected).abs() < 1e-6,
        "confidence {actual}, OCR supplied {expected}"
    );
    Ok(())
}

#[given("a two-page PDF with native text on page 1 and a scanned image on page 2")]
fn mixed_pdf(world: &mut LabWorld) -> Result<()> {
    world.add_fixture_source("us-mixed-003", None)?;
    Ok(())
}

#[given("controlled native extraction and OCR outputs for those pages")]
fn controlled_mixed(world: &mut LabWorld) -> Result<()> {
    world.use_captured_extraction()
}

#[then(expr = "page {int} text records have extraction method {string}")]
fn page_method(world: &mut LabWorld, page: u32, method: String) -> Result<()> {
    let records: Vec<_> = world
        .active_records()?
        .into_iter()
        .filter(|r| r.metadata.page_number == page)
        .collect();
    ensure!(!records.is_empty(), "no records on page {page}");
    for r in &records {
        ensure!(
            r.metadata.extraction_method.as_str() == method,
            "record {} on page {page} has method {}",
            r.record_id,
            r.metadata.extraction_method.as_str()
        );
    }
    Ok(())
}

#[then("both pages retain their original page numbers and fixture reading order")]
async fn pages_keep_order(world: &mut LabWorld) -> Result<()> {
    let records = world.active_records()?;
    let pages: BTreeSet<u32> = records.iter().map(|r| r.metadata.page_number).collect();
    ensure!(pages == BTreeSet::from([1, 2]), "pages {pages:?}");
    for page in [1u32, 2] {
        let mut on_page: Vec<_> = records
            .iter()
            .filter(|r| r.metadata.page_number == page)
            .collect();
        on_page.sort_by_key(|r| r.metadata.chunk_index);
        // Position of each chunk's last line in the fixture page must increase with chunk order.
        let path = world.corpus_dir.path().join("us-mixed-003.pdf");
        let n = std::num::NonZeroU32::new(page).context("page")?;
        let page_text = if page == 1 {
            embeddings_fundamentals::extraction::TextExtractor::extract_page(
                &*world.extractor,
                &path,
                n,
            )
            .await?
        } else {
            embeddings_fundamentals::extraction::OcrEngine::ocr_page(&*world.extractor, &path, n)
                .await?
        };
        let lines: Vec<String> = page_text
            .lines
            .iter()
            .map(|l| l.text.trim().to_owned())
            .collect();
        let mut previous = None;
        for r in on_page {
            let last = r
                .chunk_text
                .as_deref()
                .and_then(|t| t.lines().last())
                .context("text")?;
            let position = lines
                .iter()
                .position(|l| l == last.trim())
                .context("line not on page")?;
            ensure!(
                previous.is_none_or(|p| position > p),
                "page {page}: chunk {} is out of reading order",
                r.chunk_id
            );
            previous = Some(position);
        }
        assert_resolves_all(world, &records).await?;
    }
    Ok(())
}

async fn assert_resolves_all(
    world: &LabWorld,
    records: &[embeddings_fundamentals::record::Record],
) -> Result<()> {
    for r in records {
        assert_resolves_to_original(world, r).await?;
    }
    Ok(())
}

#[given("a source successfully ingested with a fixed configuration")]
async fn ingested_once(world: &mut LabWorld) -> Result<()> {
    world.use_captured_extraction()?;
    world.add_fixture_source("lab-searchable-001", None)?;
    world.ingest().await?;
    let id = current_source(world)?;
    let status = world.last_run()?.source(&id).context("report")?.status;
    ensure!(
        status == SourceStatus::Succeeded,
        "first ingestion {status:?}"
    );
    let ids: Vec<String> = world
        .active_records()?
        .iter()
        .map(|r| r.record_id.to_string())
        .collect();
    world.notes.insert("record_ids".into(), ids.join(","));
    Ok(())
}

#[when("I ingest the same source checksum with the same configuration again")]
async fn ingest_again(world: &mut LabWorld) -> Result<()> {
    world.ingest().await
}

fn active_ids(world: &LabWorld) -> Result<Vec<String>> {
    Ok(world
        .active_records()?
        .iter()
        .map(|r| r.record_id.to_string())
        .collect())
}

#[then("the set of active record IDs is unchanged")]
fn ids_unchanged(world: &mut LabWorld) -> Result<()> {
    let before: BTreeSet<String> = world
        .notes
        .get("record_ids")
        .context("no ids")?
        .split(',')
        .map(str::to_owned)
        .collect();
    let after: BTreeSet<String> = active_ids(world)?.into_iter().collect();
    ensure!(before == after, "active record ids changed");
    Ok(())
}

#[then("the active record count is unchanged")]
fn count_unchanged(world: &mut LabWorld) -> Result<()> {
    let before = world
        .notes
        .get("record_ids")
        .context("no ids")?
        .split(',')
        .count();
    let after = active_ids(world)?.len();
    ensure!(before == after, "active record count {before} → {after}");
    Ok(())
}

#[then("no duplicate source chunk exists in that embedding space")]
fn no_duplicate_chunks(world: &mut LabWorld) -> Result<()> {
    let records = world.active_records()?;
    let chunks: BTreeSet<_> = records
        .iter()
        .map(|r| (r.source_id.clone(), r.chunk_id.clone()))
        .collect();
    ensure!(
        chunks.len() == records.len(),
        "{} records but {} distinct chunks",
        records.len(),
        chunks.len()
    );
    Ok(())
}
