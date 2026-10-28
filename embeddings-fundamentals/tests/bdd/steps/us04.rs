//! Steps for `us04.feature` (PRD §5.4): interchangeable embedding adapters.
//!
//! Contract tests substitute deterministic provider responses (the deterministic
//! adapter under the selected engine's identity, built by
//! `adapters::build_contract_test`) but run the real orchestration. They do not
//! establish model quality or live provider integration (`tests/integration.rs`).

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;

use anyhow::{Context, Result, ensure};
use cucumber::{given, then, when};
use embeddings_fundamentals::adapters::build_contract_test;
use embeddings_fundamentals::config::ExperimentConfig;
use embeddings_fundamentals::domain::Modality;
use embeddings_fundamentals::embedding::deterministic::{DeterministicEmbedder, Method};
use embeddings_fundamentals::embedding::{EmbeddingStrategy, EngineId, TextInput};
use embeddings_fundamentals::evaluation::evaluate_text;
use embeddings_fundamentals::retrieval::{Scope, search_vector};

use super::eval_fixture::{self, repo};
use crate::{LabWorld, SpaceOf};

const LABELS: &str = "config/evaluation/labels-synthetic-v1.toml";

/// Token limit of the fixed pipeline configuration shared by every contract run, so
/// the chunking budget does not change with the selected engine.
const PIPELINE_TOKEN_LIMIT: usize = 128;

/// Saved experiment configuration of each engine.
fn config_file(engine: EngineId) -> Result<&'static str> {
    Ok(match engine {
        EngineId::EmbedAnything => "config/experiments/baseline-text-embed-anything.toml",
        EngineId::FastEmbed => "config/experiments/text-fastembed-minilm.toml",
        EngineId::OpenAi => "config/experiments/text-openai-3-small.toml",
        EngineId::Deterministic => "config/experiments/deterministic.toml",
    })
}

/// Ingests with the world's adapter and evaluates the frozen queries in its space.
async fn ingest_and_evaluate(world: &mut LabWorld) -> Result<()> {
    world.ingest().await?;
    world.last_run()?;
    let dataset = world.dataset.as_ref().context("no dataset")?;
    let space = world.embedder.model_info_space();
    let report = evaluate_text(
        &world.store,
        &space,
        &*world.embedder,
        dataset,
        Scope::Active,
    )
    .await?;
    world.evaluation = Some(report);
    Ok(())
}

fn fingerprints(world: &LabWorld) -> Result<BTreeMap<&'static str, String>> {
    let run = world.last_run()?;
    let evaluation = world.evaluation.as_ref().context("no evaluation")?;
    Ok(BTreeMap::from([
        ("extraction", run.extraction_fingerprint.to_string()),
        ("chunking", run.chunking_fingerprint.to_string()),
        ("evaluation", evaluation.evaluation_fingerprint.to_string()),
    ]))
}

#[given("fixed extracted chunks, queries, relevance labels, and pipeline configuration")]
async fn fixed_inputs(world: &mut LabWorld) -> Result<()> {
    eval_fixture::add_pdf_corpus(world)?;
    world.dataset = Some(eval_fixture::dataset(LABELS)?);
    // Reference run: the same pipeline configuration with the plain deterministic
    // adapter. Its chunks and fingerprints are what every engine must leave unchanged.
    let mut caps = DeterministicEmbedder::builder()
        .build()?
        .capabilities()
        .clone();
    caps.max_input_tokens = NonZeroUsize::new(PIPELINE_TOKEN_LIMIT);
    world.embedder = Arc::new(
        DeterministicEmbedder::builder()
            .model_id("reference")
            .capabilities(caps)
            .build()?,
    );
    ingest_and_evaluate(world).await?;
    for (name, value) in fingerprints(world)? {
        world.notes.insert(format!("reference-{name}"), value);
    }
    let mut chunks: Vec<String> = world
        .active_records()?
        .into_iter()
        .filter_map(|r| r.chunk_text)
        .collect();
    chunks.sort();
    ensure!(chunks.len() == 28, "{} reference chunks", chunks.len());
    world
        .notes
        .insert("reference-chunks".into(), serde_json::to_string(&chunks)?);
    Ok(())
}

#[given(expr = "a contract-test implementation for engine {string}")]
fn contract(world: &mut LabWorld, engine: String) -> Result<()> {
    let id = EngineId::from_display(&engine).with_context(|| format!("unknown engine {engine}"))?;
    let mut config = ExperimentConfig::load(&repo(config_file(id)?))?.engine;
    ensure!(
        config.engine == id,
        "{} configures {}",
        config_file(id)?,
        config.engine
    );
    config.max_input_tokens = NonZeroUsize::new(PIPELINE_TOKEN_LIMIT);
    world.engine_config = Some(config);
    Ok(())
}

#[when(expr = "I select {string} through configuration and run ingestion and evaluation")]
async fn select(world: &mut LabWorld, engine: String) -> Result<()> {
    let config = world
        .engine_config
        .clone()
        .context("no engine configured")?;
    ensure!(
        EngineId::from_display(&engine) == Some(config.engine),
        "configured {} but selected {engine}",
        config.engine
    );
    if config.engine.execution() == embeddings_fundamentals::domain::Execution::Remote {
        // The synthetic fixture corpus explicitly allows remote processing for this
        // contract test; no provider is contacted.
        world.remote_allowed = true;
    }
    world.embedder = Arc::new(build_contract_test(&config)?);
    ingest_and_evaluate(world).await
}

#[then("the selected adapter receives the unchanged document chunks and query texts")]
fn unchanged_inputs(world: &mut LabWorld) -> Result<()> {
    let calls = world.embedder.calls();
    let mut documents: Vec<String> = calls
        .iter()
        .filter(|c| c.method == Method::Documents)
        .flat_map(|c| c.raw_inputs.clone())
        .collect();
    documents.sort();
    let reference: Vec<String> = serde_json::from_str(
        world
            .notes
            .get("reference-chunks")
            .context("no reference")?,
    )?;
    ensure!(
        documents == reference,
        "the adapter received {} document texts that differ from the {} reference chunks",
        documents.len(),
        reference.len()
    );
    let queries: Vec<String> = calls
        .iter()
        .filter(|c| c.method == Method::Queries)
        .flat_map(|c| c.raw_inputs.clone())
        .collect();
    let frozen: Vec<String> = world
        .dataset
        .as_ref()
        .context("no dataset")?
        .queries
        .value
        .queries
        .iter()
        .map(|q| q.text.clone())
        .collect();
    ensure!(queries == frozen, "query texts {queries:?}");
    for call in &calls {
        if call.method != Method::Images {
            ensure!(
                call.submitted == call.raw_inputs,
                "texts were changed before submission"
            );
        }
    }
    Ok(())
}

#[then("the saved records identify that engine and its configured model separately")]
fn records_identify(world: &mut LabWorld) -> Result<()> {
    let config = world
        .engine_config
        .as_ref()
        .context("no engine configured")?;
    let records = world.active_records()?;
    ensure!(!records.is_empty(), "no records");
    for r in &records {
        let json = serde_json::to_value(&r.embedding)?;
        ensure!(
            json["engine"] == config.engine.as_str(),
            "engine {}",
            json["engine"]
        );
        ensure!(
            json["model_id"] == config.model_id.as_str(),
            "model {}",
            json["model_id"]
        );
        ensure!(
            r.experiment_id == world.last_run()?.experiment_id,
            "experiment"
        );
    }
    Ok(())
}

#[then("extraction, chunking, and evaluation configuration fingerprints are unchanged")]
fn fingerprints_unchanged(world: &mut LabWorld) -> Result<()> {
    for (name, value) in fingerprints(world)? {
        let reference = world
            .notes
            .get(&format!("reference-{name}"))
            .context("no reference fingerprint")?;
        ensure!(
            &value == reference,
            "{name} fingerprint changed: {value} vs {reference}"
        );
    }
    Ok(())
}

#[given("an adapter that does not support compatible text-to-image retrieval")]
fn text_only_adapter(world: &mut LabWorld) -> Result<()> {
    eval_fixture::add_pdf_corpus(world)?;
    world.add_fixture_source("chest-001", None)?;
    ensure!(
        !world.embedder.capabilities().text_to_image_compatible,
        "the default adapter must lack text-to-image support"
    );
    Ok(())
}

#[when("I request an image-only run")]
async fn image_run(world: &mut LabWorld) -> Result<()> {
    world.settings.modality = Modality::Image;
    world.ingest().await
}

#[then("validation fails with an error identifying the engine, model, and missing capability")]
fn capability_error(world: &mut LabWorld) -> Result<()> {
    let error = world
        .validation_error
        .as_deref()
        .context("the run was not refused")?;
    let info = world.embedder.model_info();
    for part in [
        info.engine.as_str(),
        info.model_id.as_str(),
        "compatible text-to-image retrieval",
    ] {
        ensure!(error.contains(part), "{error:?} does not name {part:?}");
    }
    ensure!(world.runs.is_empty(), "a run report was produced");
    Ok(())
}

#[then("no extraction, embedding, or index-write operation is invoked")]
fn nothing_invoked(world: &mut LabWorld) -> Result<()> {
    ensure!(
        world.extractor.extraction_calls() == 0,
        "{} extraction calls",
        world.extractor.extraction_calls()
    );
    ensure!(world.embedder.calls().is_empty(), "adapter was called");
    let mut stack = vec![world.store_dir.path().to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
            if path.is_dir() {
                stack.push(path);
            } else if name.as_deref() != Some(".writer.lock") {
                anyhow::bail!("index file written: {}", path.display());
            }
        }
    }
    Ok(())
}

#[given("two models with equal vector dimensions and different embedding-space identities")]
fn two_models(world: &mut LabWorld) -> Result<()> {
    let dims = NonZeroUsize::new(8).context("dims")?;
    let a = DeterministicEmbedder::builder()
        .model_id("model-a")
        .dimensions(dims)
        .build()?;
    let b = DeterministicEmbedder::builder()
        .model_id("model-b")
        .dimensions(dims)
        .build()?;
    ensure!(
        a.model_info().dimensions == b.model_info().dimensions,
        "dims differ"
    );
    ensure!(
        a.model_info().embedding_space_id != b.model_info().embedding_space_id,
        "spaces are equal"
    );
    world.other_embedders = vec![Arc::new(a), Arc::new(b)];
    Ok(())
}

#[when("I ingest the same source using both configurations")]
async fn ingest_both(world: &mut LabWorld) -> Result<()> {
    world.use_captured_extraction()?;
    world.add_fixture_source("lab-searchable-001", None)?;
    for adapter in world.other_embedders.clone() {
        world.embedder = adapter;
        world.ingest().await?;
        world.last_run()?;
    }
    Ok(())
}

#[then("their records are stored in separate indexes or namespaces")]
fn separate(world: &mut LabWorld) -> Result<()> {
    let spaces: Vec<_> = world
        .other_embedders
        .iter()
        .map(|e| e.model_info_space())
        .collect();
    let stored = world.store.spaces()?;
    let mut counts = Vec::new();
    for space in &spaces {
        ensure!(stored.contains(space), "space {space} has no index");
        ensure!(
            world.store_dir.path().join(space.as_str()).is_dir(),
            "no namespace directory for {space}"
        );
        let records = world.store.active_records(space)?;
        ensure!(
            records
                .iter()
                .all(|r| &r.embedding.embedding_space_id == space),
            "index {space} holds records of another space"
        );
        counts.push(records.len());
    }
    let expected: usize = super::common::expected("lab-searchable-001")?
        .text_records
        .iter()
        .sum();
    ensure!(
        counts.iter().all(|&n| n == expected),
        "record counts {counts:?}, expected {expected} per index"
    );
    Ok(())
}

#[when("I query the first index using the second model's query vector")]
async fn cross_query(world: &mut LabWorld) -> Result<()> {
    let [first, second] = world.other_embedders.as_slice() else {
        anyhow::bail!("expected two models");
    };
    let batch = second
        .embed_queries(&[TextInput {
            id: "q".parse()?,
            text: "Onde está o resultado da hemoglobina?".into(),
        }])
        .await?;
    let vector = batch
        .embeddings
        .first()
        .context("no vector")?
        .vector
        .clone();
    ensure!(vector.len() == first.model_info().dimensions.get(), "dims");
    let outcome = search_vector(
        &world.store,
        &first.model_info_space(),
        &second.model_info_space(),
        &vector,
        3,
        Scope::Active,
    )
    .await;
    world.search = Some(outcome.map_err(|e| e.to_string()));
    Ok(())
}

#[then("the query fails with an embedding-space mismatch error before similarity search")]
fn mismatch(world: &mut LabWorld) -> Result<()> {
    let outcome = world.search.as_ref().context("no query ran")?;
    let Err(error) = outcome else {
        anyhow::bail!("the query returned results instead of failing");
    };
    ensure!(
        error.contains("embedding-space mismatch"),
        "error {error:?}"
    );
    let [first, second] = world.other_embedders.as_slice() else {
        anyhow::bail!("expected two models");
    };
    for space in [first.model_info_space(), second.model_info_space()] {
        ensure!(
            error.contains(space.as_str()),
            "{error:?} does not name {space}"
        );
    }
    Ok(())
}
