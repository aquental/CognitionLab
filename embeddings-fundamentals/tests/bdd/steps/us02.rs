//! Steps for `us02.feature` (PRD §5.2): Portuguese search with source traceability.

use anyhow::{Context, Result, ensure};
use cucumber::{given, then, when};
use embeddings_fundamentals::embedding::deterministic::DeterministicEmbedder;
use embeddings_fundamentals::retrieval::{Scope, search_text};
use serde::Deserialize;

use super::common::assert_resolves_to_original;
use crate::{LabWorld, SpaceOf, fixture};

#[derive(Debug, Deserialize)]
struct Ranking {
    query: String,
    source: String,
    keywords: Vec<Keyword>,
    expected: Vec<Expected>,
}

#[derive(Debug, Deserialize)]
struct Keyword {
    keyword: String,
    components: Vec<(usize, f32)>,
}

#[derive(Debug, Deserialize)]
struct Expected {
    chunk_id: String,
    score: f32,
}

fn ranking() -> Result<Ranking> {
    let text = std::fs::read_to_string(fixture("rankings/hemoglobina.toml")).context("read ranking")?;
    Ok(toml::from_str(&text)?)
}

#[given("a text index with at least three fixture items and deterministic query vectors")]
async fn text_index(world: &mut LabWorld) -> Result<()> {
    let ranking = ranking()?;
    let mut builder = DeterministicEmbedder::builder();
    for k in &ranking.keywords {
        builder = builder.with_keyword_vector(&k.keyword, &k.components);
    }
    world.embedder = std::sync::Arc::new(builder.build()?);
    world.use_captured_extraction()?;
    world.add_fixture_source(&ranking.source, None)?;
    world.ingest().await?;
    let count = world.active_records()?.len();
    ensure!(count >= 3, "index has only {count} items");
    Ok(())
}

#[given(expr = "the expected ranking for {string} is frozen")]
fn ranking_frozen(world: &mut LabWorld, query: String) -> Result<()> {
    let ranking = ranking()?;
    ensure!(ranking.query == query, "frozen ranking is for {:?}", ranking.query);
    world.notes.insert("ranking".into(), query);
    Ok(())
}

async fn search(world: &mut LabWorld, query: &str, limit: usize) -> Result<()> {
    let space = world.embedder.model_info_space();
    let outcome = search_text(&world.store, &space, &*world.embedder, query, limit, Scope::Active).await;
    world.search = Some(outcome.map_err(|e| e.to_string()));
    Ok(())
}

#[when(expr = "I search for {string} in text-only mode with limit {int}")]
async fn search_text_only(world: &mut LabWorld, query: String, limit: usize) -> Result<()> {
    search(world, &query, limit).await
}

#[when(expr = "I search for {string} with limit {int}")]
async fn search_default(world: &mut LabWorld, query: String, limit: usize) -> Result<()> {
    search(world, &query, limit).await
}

fn hits(world: &LabWorld) -> Result<&Vec<embeddings_fundamentals::retrieval::SearchHit>> {
    match world.search.as_ref().context("no search was run")? {
        Ok(hits) => Ok(hits),
        Err(e) => anyhow::bail!("search failed: {e}"),
    }
}

#[then("exactly three text results are returned in the expected order")]
fn three_in_order(world: &mut LabWorld) -> Result<()> {
    let ranking = ranking()?;
    let hits = hits(world)?;
    ensure!(hits.len() == 3, "expected 3 results, got {}", hits.len());
    for (hit, expected) in hits.iter().zip(&ranking.expected) {
        ensure!(
            hit.chunk_id.as_str() == expected.chunk_id,
            "rank {}: expected {}, got {}",
            hit.rank,
            expected.chunk_id,
            hit.chunk_id
        );
        ensure!(
            (hit.score - expected.score).abs() < 1e-5,
            "rank {}: expected score {}, got {}",
            hit.rank,
            expected.score,
            hit.score
        );
        ensure!(hit.modality == embeddings_fundamentals::domain::Modality::Text, "non-text result");
    }
    Ok(())
}

#[then("each result includes its record ID, source ID, page number, chunk text, and score")]
fn results_have_fields(world: &mut LabWorld) -> Result<()> {
    for hit in hits(world)? {
        ensure!(!hit.record_id.as_str().is_empty() && !hit.source_id.as_str().is_empty(), "missing ids");
        ensure!(hit.page_number >= 1, "missing page number");
        ensure!(hit.chunk_text.as_deref().is_some_and(|t| !t.is_empty()), "missing chunk text");
        ensure!(hit.score.is_finite(), "non-finite score");
    }
    Ok(())
}

#[then("each source and page reference resolves to the corresponding original fixture")]
async fn results_resolve(world: &mut LabWorld) -> Result<()> {
    let records = world.active_records()?;
    for hit in hits(world)?.clone() {
        let record = records.iter().find(|r| r.record_id == hit.record_id).context("hit is not an active record")?;
        ensure!(
            record.source_id == hit.source_id && record.metadata.page_number == hit.page_number,
            "hit {} does not match its record",
            hit.record_id
        );
        assert_resolves_to_original(world, record).await?;
    }
    Ok(())
}

#[given("an empty index compatible with the configured query model")]
fn empty_index(world: &mut LabWorld) -> Result<()> {
    ensure!(world.active_records()?.is_empty(), "index is not empty");
    Ok(())
}

#[then("an empty result list is returned without an error")]
fn empty_results(world: &mut LabWorld) -> Result<()> {
    let hits = hits(world)?;
    ensure!(hits.is_empty(), "expected no results, got {}", hits.len());
    Ok(())
}

#[then("no synthetic result or source reference is created")]
fn nothing_created(world: &mut LabWorld) -> Result<()> {
    ensure!(world.store.spaces()?.is_empty(), "the search created an index space");
    ensure!(world.active_records()?.is_empty(), "the search created records");
    ensure!(world.embedder.calls().is_empty(), "the search called the adapter on an empty index");
    Ok(())
}
