//! Steps for `us03.feature` (PRD §5.3): comparable retrieval evaluation. Milestone 2
//! covers the text-only example and the low-relevance scenario, milestone 3 the
//! image-only example, milestone 4 the combined example and rank fusion.

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{Context, Result, bail, ensure};
use cucumber::{given, then, when};
use embeddings_fundamentals::domain::Modality;
use embeddings_fundamentals::evaluation::{
    EvaluationMode, evaluate_combined, evaluate_images, evaluate_text,
};
use embeddings_fundamentals::fusion::{FusionDepth, FusionMethod, FusionParams};
use embeddings_fundamentals::ids::RecordId;
use embeddings_fundamentals::report::ComparisonReport;
use embeddings_fundamentals::retrieval::{Leg, Scope, SearchHit, fuse};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::eval_fixture::{self, repo};
use crate::{LabWorld, SpaceOf, fixture};

const QUERIES: &str = "config/evaluation/queries-v1.toml";
const LABELS: &str = "config/evaluation/labels-synthetic-v4.toml";
const IMAGE_ONLY_LABELS: &str = "tests/fixtures/evaluation/labels-images-only.toml";

/// Checksum of a file computed here, independently of the library.
fn sha256_file(rel: &str) -> Result<String> {
    let bytes = std::fs::read(repo(rel)).with_context(|| format!("read {rel}"))?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(&bytes))))
}

#[derive(Debug, Deserialize)]
struct LabelFile {
    label: Vec<LabelRow>,
}

#[derive(Debug, Deserialize)]
struct LabelRow {
    query: String,
    item: String,
    label: u8,
}

#[derive(Debug, Deserialize)]
struct MetricFixture {
    query: Vec<RankRow>,
    expected: Vec<MetricRow>,
    #[serde(rename = "macro")]
    macro_avg: MetricValues,
}

#[derive(Debug, Deserialize)]
struct RankRow {
    id: String,
    ranks: Vec<u8>,
    /// Record id at each rank, where the derivation depends on the record-id
    /// tie-break (combined mode).
    #[serde(default)]
    records: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct MetricRow {
    query: String,
    #[serde(flatten)]
    values: MetricValues,
}

#[derive(Debug, Deserialize)]
struct MetricValues {
    precision_at_3: f64,
    hit_at_3: f64,
    ndcg_at_3: f64,
}

fn close(name: &str, actual: f64, expected: f64) -> Result<()> {
    ensure!(
        (actual - expected).abs() <= 1e-6,
        "{name}: observed {actual}, expected {expected}"
    );
    Ok(())
}

#[given("ten frozen Portuguese queries and frozen relevance labels")]
fn frozen_dataset(world: &mut LabWorld) -> Result<()> {
    let dataset = eval_fixture::dataset(LABELS)?;
    let queries = &dataset.queries.value;
    ensure!(
        queries.queries.len() == 10,
        "{} queries",
        queries.queries.len()
    );
    ensure!(queries.language == "pt-BR", "language {}", queries.language);
    world.dataset = Some(dataset);
    Ok(())
}

#[given(
    expr = "a compatible fixture index with at least three candidates for each query in {string} mode"
)]
async fn fixture_index(world: &mut LabWorld, mode: String) -> Result<()> {
    let (modality, count) = match EvaluationMode::from_name(&mode) {
        Some(EvaluationMode::TextOnly) => {
            world.embedder = Arc::new(eval_fixture::keyword_builder()?.build()?);
            eval_fixture::add_pdf_corpus(world)?;
            world.ingest().await?;
            (Modality::Text, world.active_records()?.len())
        }
        Some(EvaluationMode::ImageOnly) => {
            let images = Arc::new(eval_fixture::image_keyword_builder()?.build()?);
            world.image_embedder = Some(Arc::clone(&images));
            eval_fixture::add_image_corpus(world)?;
            world.ingest_images().await?;
            let records = world.store.active_records(&images.model_info_space())?;
            (Modality::Image, records.len())
        }
        Some(EvaluationMode::Combined) => {
            // Both indexes of the text-only and image-only examples, side by side in
            // one store: the text run over the PDFs, then the image run over the
            // image-bearing sources, each in its own space.
            world.embedder = Arc::new(eval_fixture::keyword_builder()?.build()?);
            eval_fixture::add_pdf_corpus(world)?;
            world.ingest().await?;
            ensure!(
                world.validation_error.is_none(),
                "the text run was refused: {:?}",
                world.validation_error
            );
            let text_count = world
                .store
                .active_records(&world.embedder.model_info_space())?
                .len();
            ensure!(
                text_count >= 3,
                "text index has only {text_count} candidates"
            );
            let images = Arc::new(eval_fixture::image_keyword_builder()?.build()?);
            world.image_embedder = Some(Arc::clone(&images));
            world.sources.clear();
            eval_fixture::add_image_corpus(world)?;
            world.ingest_images().await?;
            let records = world.store.active_records(&images.model_info_space())?;
            (Modality::Image, records.len())
        }
        None => bail!("unknown retrieval mode {mode:?}"),
    };
    ensure!(
        world.validation_error.is_none(),
        "the {modality:?} run was refused: {:?}",
        world.validation_error
    );
    ensure!(count >= 3, "index has only {count} candidates");
    world.notes.insert("metrics_fixture".into(), mode);
    Ok(())
}

#[when(expr = "I evaluate {string} mode")]
async fn evaluate(world: &mut LabWorld, mode: String) -> Result<()> {
    let dataset = world.dataset.as_ref().context("no dataset")?;
    let report = match EvaluationMode::from_name(&mode) {
        Some(EvaluationMode::TextOnly) => {
            let space = world.embedder.model_info_space();
            evaluate_text(
                &world.store,
                &space,
                &*world.embedder,
                dataset,
                Scope::Active,
            )
            .await?
        }
        Some(EvaluationMode::ImageOnly) => {
            let images = world.image_embedder.clone().context("no image index")?;
            let space = images.model_info_space();
            evaluate_images(&world.store, &space, &*images, dataset, Scope::Active).await?
        }
        Some(EvaluationMode::Combined) => {
            let images = world.image_embedder.clone().context("no image index")?;
            let (text_space, image_space) =
                (world.embedder.model_info_space(), images.model_info_space());
            let report = evaluate_combined(
                &world.store,
                Leg {
                    space: &text_space,
                    embedder: &*world.embedder,
                },
                Leg {
                    space: &image_space,
                    embedder: &*images,
                },
                FusionParams::default(),
                dataset,
                Scope::Active,
            )
            .await?;
            ensure!(
                report.spaces == [text_space, image_space],
                "the report names spaces {:?}, not the text and image spaces in that order",
                report.spaces
            );
            ensure!(
                report.fusion == Some(FusionParams::default()),
                "the report records fusion {:?}",
                report.fusion
            );
            report
        }
        None => bail!("unknown retrieval mode {mode:?}"),
    };
    ensure!(
        report.mode.as_str() == mode,
        "evaluated {} mode, requested {mode}",
        report.mode.as_str()
    );
    world.evaluation = Some(report);
    Ok(())
}

#[then("the report contains exactly ten query entries with three ranked results each")]
fn ten_entries(world: &mut LabWorld) -> Result<()> {
    let report = world.evaluation.as_ref().context("no evaluation")?;
    ensure!(
        report.queries.len() == 10,
        "{} entries",
        report.queries.len()
    );
    for q in &report.queries {
        let ranks: Vec<usize> = q.results.iter().map(|r| r.rank).collect();
        ensure!(ranks == [1, 2, 3], "{}: ranks {ranks:?}", q.query_id);
        ensure!(!q.fewer_than_k, "{} flagged as short", q.query_id);
    }
    Ok(())
}

#[then(
    "each result includes its source, page or asset, score, relevance label, and relevance note"
)]
fn result_fields(world: &mut LabWorld) -> Result<()> {
    let report = world.evaluation.as_ref().context("no evaluation")?;
    let text = std::fs::read_to_string(repo(LABELS))?;
    let labels: LabelFile = toml::from_str(&text)?;
    for q in &report.queries {
        for r in &q.results {
            ensure!(!r.source_id.as_str().is_empty(), "missing source");
            ensure!(
                r.page_number >= 1 || r.asset_uri.is_some(),
                "{}: no page or asset",
                r.chunk_id
            );
            ensure!(r.score.is_finite(), "{}: score {}", r.chunk_id, r.score);
            ensure!(!r.note.trim().is_empty(), "{}: empty note", r.chunk_id);
            let expected = labels
                .label
                .iter()
                .find(|l| l.query == q.query_id.as_str() && l.item == r.chunk_id.as_str())
                .map_or(0, |l| l.label);
            ensure!(
                r.label == expected,
                "{} / {}: label {} but the frozen file says {expected}",
                q.query_id,
                r.chunk_id,
                r.label
            );
        }
    }
    Ok(())
}

#[then("the report records the same query-set and relevance-label checksums as the inputs")]
fn checksums(world: &mut LabWorld) -> Result<()> {
    let report = world.evaluation.as_ref().context("no evaluation")?;
    ensure!(
        report.query_set.checksum.as_str() == sha256_file(QUERIES)?,
        "query-set checksum {}",
        report.query_set.checksum
    );
    ensure!(
        report.labels.checksum.as_str() == sha256_file(LABELS)?,
        "labels checksum {}",
        report.labels.checksum
    );
    Ok(())
}

#[then(
    "Precision@3, Hit@3, and nDCG@3 match the independently precomputed fixture values within 0.000001"
)]
fn precomputed(world: &mut LabWorld) -> Result<()> {
    let report = world.evaluation.as_ref().context("no evaluation")?;
    let mode = world
        .notes
        .get("metrics_fixture")
        .context("no fixture index")?;
    let text = std::fs::read_to_string(fixture(&format!("metrics/{mode}.toml")))?;
    let fixture: MetricFixture = toml::from_str(&text).context("parse metrics fixture")?;
    let observed: BTreeMap<&str, _> = report
        .queries
        .iter()
        .map(|q| (q.query_id.as_str(), q))
        .collect();
    for row in &fixture.query {
        let q = observed.get(row.id.as_str()).context("missing query")?;
        let labels: Vec<u8> = q.results.iter().map(|r| r.label).collect();
        ensure!(
            labels == row.ranks,
            "{}: labels by rank {labels:?}, hand-derived {:?}",
            row.id,
            row.ranks
        );
        if let Some(records) = &row.records {
            let observed: Vec<&str> = q.results.iter().map(|r| r.record_id.as_str()).collect();
            ensure!(
                observed == *records,
                "{}: records by rank {observed:?}, hand-derived {records:?} (re-derive the \
                 fixture if the corpus, extraction, chunking, or a space changed)",
                row.id
            );
        }
    }
    for row in &fixture.expected {
        let q = observed.get(row.query.as_str()).context("missing query")?;
        close(
            &format!("{} P@3", row.query),
            q.metrics.precision_at_3,
            row.values.precision_at_3,
        )?;
        close(
            &format!("{} Hit@3", row.query),
            q.metrics.hit_at_3,
            row.values.hit_at_3,
        )?;
        close(
            &format!("{} nDCG@3", row.query),
            q.metrics.ndcg_at_3,
            row.values.ndcg_at_3,
        )?;
    }
    let m = &report.metrics;
    close(
        "macro P@3",
        m.precision_at_3,
        fixture.macro_avg.precision_at_3,
    )?;
    close("macro Hit@3", m.hit_at_3, fixture.macro_avg.hit_at_3)?;
    close("macro nDCG@3", m.ndcg_at_3, fixture.macro_avg.ndcg_at_3)?;
    Ok(())
}

#[given("a completed fixture evaluation whose retrieved items all have relevance label 0")]
async fn low_relevance(world: &mut LabWorld) -> Result<()> {
    eval_fixture::add_pdf_corpus(world)?;
    let builder = eval_fixture::keyword_builder()?;
    let result = eval_fixture::measured_run(world, builder, IMAGE_ONLY_LABELS, None, false).await?;
    let labels: Vec<u8> = result
        .evaluation
        .queries
        .iter()
        .flat_map(|q| q.results.iter().map(|r| r.label))
        .collect();
    ensure!(labels.len() == 30, "{} results", labels.len());
    ensure!(labels.iter().all(|&l| l == 0), "labels {labels:?}");
    Ok(())
}

#[when("I generate the comparison report")]
fn generate_report(world: &mut LabWorld) -> Result<()> {
    let report = ComparisonReport::new(
        world.configs.clone(),
        vec!["synthetic fixture corpus".to_owned()],
    )?;
    world.notes.insert("markdown".into(), report.to_markdown());
    world
        .notes
        .insert("json".into(), serde_json::to_string(&report)?);
    world.comparison = Some(report);
    Ok(())
}

#[then("Precision@3, Hit@3, and nDCG@3 are all 0")]
fn all_zero(world: &mut LabWorld) -> Result<()> {
    let report = world.comparison.as_ref().context("no report")?;
    for c in &report.configurations {
        let mut all = vec![c.evaluation.metrics];
        all.extend(c.evaluation.queries.iter().map(|q| q.metrics));
        for m in all {
            for (name, value) in [
                ("P@3", m.precision_at_3),
                ("Hit@3", m.hit_at_3),
                ("nDCG@3", m.ndcg_at_3),
            ] {
                ensure!(value.abs() < 1e-12, "{name} = {value}");
            }
        }
    }
    Ok(())
}

#[then(expr = "the evaluation status is {string}")]
fn status(world: &mut LabWorld, expected: String) -> Result<()> {
    let json: serde_json::Value =
        serde_json::from_str(world.notes.get("json").context("no json")?)?;
    let configs = json["configurations"]
        .as_array()
        .context("configurations")?;
    ensure!(!configs.is_empty(), "no configuration");
    for c in configs {
        let status = c["evaluation"]["status"].as_str().unwrap_or_default();
        ensure!(status == expected, "status {status:?}");
        if c["run_status"] != "succeeded" {
            bail!("ingestion was not completed: {}", c["run_status"]);
        }
    }
    Ok(())
}

#[then("the report preserves the irrelevant results and their failure notes")]
fn preserves(world: &mut LabWorld) -> Result<()> {
    let report = world.comparison.as_ref().context("no report")?;
    let markdown = world.notes.get("markdown").context("no markdown")?;
    for c in &report.configurations {
        for q in &c.evaluation.queries {
            ensure!(q.results.len() == 3, "{}: results dropped", q.query_id);
            for r in &q.results {
                ensure!(r.label == 0, "{} is relevant", r.chunk_id);
                ensure!(
                    r.note.contains("label 0"),
                    "{}: note {:?} does not explain the failure",
                    r.chunk_id,
                    r.note
                );
                let text = r.chunk_text.as_deref().context("text result")?;
                let shown = text.replace('\n', " / ").replace('|', "\\|");
                ensure!(
                    markdown.contains(&shown),
                    "{} missing from Markdown",
                    r.chunk_id
                );
            }
        }
    }
    ensure!(
        markdown.contains("label 0"),
        "Markdown does not show the failure notes"
    );
    Ok(())
}

/// Record id whose ascending order is the letter order of `letter` (`A` -> `...0a`).
fn letter_id(letter: &str) -> Result<RecordId> {
    ensure!(
        letter.len() == 1 && ("A"..="F").contains(&letter),
        "item {letter:?} is not one of A-F"
    );
    Ok(format!("{:0>16}", letter.to_lowercase()).parse()?)
}

fn letter_of(id: &RecordId) -> String {
    id.as_str().trim_start_matches('0').to_uppercase()
}

/// A ranked leg: descending raw scores starting at `top`, one result per letter.
fn ranking(letters: &str, modality: Modality, top: f32) -> Result<Vec<SearchHit>> {
    let source: embeddings_fundamentals::ids::SourceId = "fusion-fixture".parse()?;
    let page = std::num::NonZeroU32::MIN;
    letters
        .split(',')
        .enumerate()
        .map(|(i, letter)| {
            let index = u32::try_from(i)?;
            let chunk = embeddings_fundamentals::ids::ChunkId::new(&source, page, modality, index)?;
            #[expect(clippy::cast_precision_loss, reason = "a handful of positions")]
            let score = top - 0.1 * i as f32;
            Ok(SearchHit {
                rank: i + 1,
                record_id: letter_id(letter.trim())?,
                source_id: source.clone(),
                source_file: "fusion-fixture.pdf".to_owned(),
                revision_id: "rev-0000000000000001".parse()?,
                chunk_id: chunk,
                page_number: 1,
                modality,
                chunk_text: None,
                asset_uri: None,
                original: None,
                score,
                fusion: None,
            })
        })
        .collect()
}

#[given(expr = "text ranking {string} and image ranking {string}")]
fn rankings(world: &mut LabWorld, text: String, image: String) -> Result<()> {
    // Raw scores of unrelated scales: text cosines near 0.9, image cosines near 0.3.
    world.fusion.text = ranking(&text, Modality::Text, 0.9)?;
    world.fusion.image = ranking(&image, Modality::Image, 0.3)?;
    Ok(())
}

#[given("reciprocal rank fusion with k equal to 60 and ranks starting at 1")]
fn rrf_k60(world: &mut LabWorld) -> Result<()> {
    let params = FusionParams::default();
    ensure!(
        params.method == FusionMethod::Rrf && params.k == 60 && params.depth == FusionDepth::All,
        "default fusion is {params:?}, not RRF with k = 60 over every candidate"
    );
    world.fusion.params = params;
    Ok(())
}

#[when("I combine the rankings")]
fn combine(world: &mut LabWorld) -> Result<()> {
    let state = &mut world.fusion;
    state.fused = fuse(&state.text, &state.image, state.params, usize::MAX);
    Ok(())
}

fn fused_score(hit: &SearchHit) -> Result<f64> {
    Ok(hit
        .fusion
        .as_ref()
        .with_context(|| format!("{} has no fusion trace", letter_of(&hit.record_id)))?
        .fused_score)
}

#[then("the fused score of each item is the sum of 1 / (60 + rank) over every list containing it")]
fn sum_of_reciprocals(world: &mut LabWorld) -> Result<()> {
    let state = &world.fusion;
    let mut inputs: Vec<&RecordId> = state
        .text
        .iter()
        .chain(&state.image)
        .map(|h| &h.record_id)
        .collect();
    inputs.sort();
    inputs.dedup();
    let mut fused: Vec<&RecordId> = state.fused.iter().map(|h| &h.record_id).collect();
    fused.sort();
    ensure!(
        inputs == fused,
        "fused items {fused:?} are not the input items {inputs:?} once each"
    );
    for hit in &state.fused {
        let position = |leg: &[SearchHit]| leg.iter().position(|h| h.record_id == hit.record_id);
        let (in_text, in_image) = (position(&state.text), position(&state.image));
        let expected: f64 = [in_text, in_image]
            .into_iter()
            .flatten()
            .map(|p| Ok(1.0 / (60.0 + f64::from(u32::try_from(p + 1)?))))
            .try_fold(0.0, |sum, term: Result<f64>| term.map(|t| sum + t))?;
        let observed = fused_score(hit)?;
        ensure!(
            (observed - expected).abs() <= 1e-12,
            "{}: fused score {observed}, sum of 1/(60+rank) {expected}",
            letter_of(&hit.record_id)
        );
        let trace = hit.fusion.as_ref().context("trace")?;
        ensure!(
            trace.text.map(|t| t.rank) == in_text.map(|p| p + 1)
                && trace.image.map(|t| t.rank) == in_image.map(|p| p + 1),
            "{}: leg ranks {:?}",
            letter_of(&hit.record_id),
            trace
        );
    }
    Ok(())
}

#[then(expr = "the fused scores are {string} within 0.000001")]
fn fused_scores(world: &mut LabWorld, expected: String) -> Result<()> {
    for entry in expected.split(", ") {
        let (letter, formula) = entry.split_once('=').context("entry is not item=formula")?;
        let value = formula
            .split('+')
            .map(|term| {
                let denominator = term
                    .strip_prefix("1/")
                    .with_context(|| format!("term {term:?} is not 1/n"))?;
                Ok(1.0 / denominator.parse::<f64>()?)
            })
            .try_fold(0.0, |sum, term: Result<f64>| term.map(|t| sum + t))?;
        let id = letter_id(letter)?;
        let hit = world
            .fusion
            .fused
            .iter()
            .find(|h| h.record_id == id)
            .with_context(|| format!("{letter} missing from the fused result"))?;
        close(
            &format!("fused score of {letter}"),
            fused_score(hit)?,
            value,
        )?;
        close(
            &format!("reported score of {letter}"),
            f64::from(hit.score),
            value,
        )?;
    }
    Ok(())
}

#[then(
    expr = "the result order is {string} by descending fused score, breaking equal-score ties by ascending record ID"
)]
fn fused_order(world: &mut LabWorld, expected: String) -> Result<()> {
    let fused = &world.fusion.fused;
    let order: Vec<String> = fused.iter().map(|h| letter_of(&h.record_id)).collect();
    ensure!(
        order.join(",") == expected,
        "order {}, expected {expected}",
        order.join(",")
    );
    let ranks: Vec<usize> = fused.iter().map(|h| h.rank).collect();
    let expected_ranks: Vec<usize> = (1..=fused.len()).collect();
    ensure!(ranks == expected_ranks, "fused ranks {ranks:?}");
    for pair in fused.windows(2) {
        let (a, b) = (fused_score(&pair[0])?, fused_score(&pair[1])?);
        ensure!(
            a.total_cmp(&b)
                .then(pair[1].record_id.cmp(&pair[0].record_id))
                .is_gt(),
            "{} ({a}) is ranked before {} ({b})",
            letter_of(&pair[0].record_id),
            letter_of(&pair[1].record_id)
        );
    }
    Ok(())
}

#[when("I multiply all raw text similarity scores by 100 without changing their order")]
fn rescale(world: &mut LabWorld) -> Result<()> {
    let state = &mut world.fusion;
    let before: Vec<RecordId> = state.text.iter().map(|h| h.record_id.clone()).collect();
    for hit in &mut state.text {
        hit.score *= 100.0;
    }
    let after: Vec<RecordId> = state.text.iter().map(|h| h.record_id.clone()).collect();
    ensure!(
        before == after && state.text.windows(2).all(|p| p[0].score > p[1].score),
        "rescaling changed the text order"
    );
    state.before_rescaling = Some(std::mem::take(&mut state.fused));
    state.fused = fuse(&state.text, &state.image, state.params, usize::MAX);
    Ok(())
}

#[then("the fused scores and result order are unchanged")]
fn unchanged(world: &mut LabWorld) -> Result<()> {
    let state = &world.fusion;
    let before = state
        .before_rescaling
        .as_ref()
        .context("no fusion before rescaling")?;
    let outline = |hits: &[SearchHit]| -> Result<Vec<(RecordId, u64)>> {
        hits.iter()
            .map(|h| Ok((h.record_id.clone(), fused_score(h)?.to_bits())))
            .collect()
    };
    ensure!(
        outline(before)? == outline(&state.fused)?,
        "rescaling the text scores changed the fused result"
    );
    let text_scores: Vec<f32> = state
        .fused
        .iter()
        .filter_map(|h| h.fusion.as_ref()?.text.map(|t| t.score))
        .collect();
    ensure!(
        text_scores.iter().all(|s| *s > 1.0),
        "the fused result does not carry the rescaled text scores: {text_scores:?}"
    );
    Ok(())
}
