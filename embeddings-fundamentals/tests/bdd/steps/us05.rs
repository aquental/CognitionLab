//! Steps for `us05.feature` (PRD §5.5): quality alongside resources and costs.
//!
//! Measurements come from real experiment runs (`experiment::run`) whose clock and
//! memory probe are controlled, so every expected value is derived by hand in
//! `tests/fixtures/measurements/expected.toml`. Usage and index size are recomputed
//! here from the store, independently of the measuring code.

use std::num::NonZeroUsize;

use anyhow::{Context, Result, ensure};
use cucumber::{given, then, when};
use embeddings_fundamentals::adapters::build_contract_test;
use embeddings_fundamentals::config::ExperimentConfig;
use embeddings_fundamentals::cost::{PriceTable, estimate};
use embeddings_fundamentals::domain::Execution;
use embeddings_fundamentals::embedding::{EmbeddingStrategy, EngineId};
use embeddings_fundamentals::report::ConfigurationResult;
use serde_json::Value;

use super::eval_fixture::{self, repo};
use crate::{LabWorld, fixture};

const LABELS: &str = "config/evaluation/labels-synthetic-v1.toml";
const PRICE: &str = "prices/test-price-v1.toml";

fn number(value: &Value, path: &str) -> Result<f64> {
    let mut v = value;
    for key in path.split('.') {
        v = &v[key];
    }
    v.as_f64()
        .with_context(|| format!("{path} is not a number: {v}"))
}

#[expect(
    clippy::cast_precision_loss,
    reason = "fixture counts and token sums are far below 2^52"
)]
fn expected(world: &LabWorld, key: &str) -> Result<f64> {
    let table: toml::Table =
        toml::from_str(world.notes.get("expected").context("no expectations")?)?;
    let v = table
        .get(key)
        .with_context(|| format!("no expected {key}"))?;
    v.as_float()
        .or_else(|| v.as_integer().map(|i| i as f64))
        .with_context(|| format!("expected {key} is not a number"))
}

/// Index bytes recomputed by walking the active revision directories.
fn index_bytes(world: &LabWorld, c: &ConfigurationResult) -> Result<u64> {
    let mut total = 0;
    for revision in world.store.active(&c.embedding_space_id)? {
        let dir = world
            .store_dir
            .path()
            .join(c.embedding_space_id.as_str())
            .join("revisions")
            .join(revision.source_id.as_str())
            .join(revision.revision_id.as_str());
        for entry in std::fs::read_dir(&dir)? {
            total += entry?.metadata()?.len();
        }
    }
    Ok(total)
}

#[given("two completed configurations evaluated against the same frozen corpus and queries")]
async fn two_configurations(world: &mut LabWorld) -> Result<()> {
    eval_fixture::add_pdf_corpus(world)?;
    for model in ["config-a-model", "config-b-model"] {
        let builder = eval_fixture::keyword_builder()?.model_id(model);
        let result = eval_fixture::measured_run(world, builder, LABELS, None, true).await?;
        ensure!(
            result.run_status == embeddings_fundamentals::domain::RunStatus::Succeeded,
            "run {} did not complete",
            result.experiment_id
        );
    }
    let [a, b] = world.configs.as_slice() else {
        anyhow::bail!("expected two configurations");
    };
    ensure!(a.corpus_checksum == b.corpus_checksum, "different corpora");
    ensure!(
        a.evaluation.query_set == b.evaluation.query_set,
        "different queries"
    );
    Ok(())
}

#[given("controlled clock, memory, and usage observations with known expected measurements")]
fn controlled(world: &mut LabWorld) -> Result<()> {
    let controlled = eval_fixture::controlled()?;
    world
        .notes
        .insert("expected".into(), toml::to_string(&controlled.expected)?);
    world
        .notes
        .insert("peak".into(), controlled.peak_rss_bytes.to_string());
    for c in &world.configs {
        let usage = eval_fixture::whitespace_tokens(world, c)?;
        ensure!(
            c.measurements.embedding.usage_tokens == Some(usage),
            "usage {:?}, recomputed {usage}",
            c.measurements.embedding.usage_tokens
        );
    }
    Ok(())
}

#[then(
    "both configurations include retrieval metrics, ingestion duration, and embedding throughput"
)]
fn quality_and_throughput(world: &mut LabWorld) -> Result<()> {
    let json: Value = serde_json::from_str(world.notes.get("json").context("no report")?)?;
    let configs = json["configurations"]
        .as_array()
        .context("configurations")?;
    ensure!(configs.len() == 2, "{} configurations", configs.len());
    for c in configs {
        for path in [
            "evaluation.metrics.precision_at_3",
            "evaluation.metrics.hit_at_3",
            "evaluation.metrics.ndcg_at_3",
            "measurements.ingestion.duration_ms",
            "measurements.embedding.items_per_second",
        ] {
            number(c, path)?;
        }
    }
    Ok(())
}

#[then("both include median and p95 query latency, peak local RAM, and index size")]
fn latency_and_resources(world: &mut LabWorld) -> Result<()> {
    let json: Value = serde_json::from_str(world.notes.get("json").context("no report")?)?;
    for c in json["configurations"]
        .as_array()
        .context("configurations")?
    {
        number(c, "measurements.warm_query_latency.median_ms")?;
        number(c, "measurements.warm_query_latency.p95_ms")?;
        number(c, "measurements.peak_local_ram.value")?;
        number(c, "measurements.index_size.value")?;
    }
    Ok(())
}

#[expect(
    clippy::cast_precision_loss,
    reason = "fixture counts and token sums are far below 2^52"
)]
#[then("all reported measurements match the fixture's expected values and units")]
fn match_expected(world: &mut LabWorld) -> Result<()> {
    let json: Value = serde_json::from_str(world.notes.get("json").context("no report")?)?;
    let checks = [
        (
            "ingestion_model_load_ms",
            "measurements.ingestion_model_load_ms",
        ),
        (
            "ingestion_duration_ms",
            "measurements.ingestion.duration_ms",
        ),
        ("pages", "measurements.ingestion.pages"),
        ("chunks", "measurements.ingestion.chunks"),
        ("ms_per_page", "measurements.ingestion.ms_per_page"),
        (
            "chunks_per_second",
            "measurements.ingestion.chunks_per_second",
        ),
        ("embedding_calls", "measurements.embedding.calls"),
        ("embedding_total_ms", "measurements.embedding.total_ms"),
        (
            "mean_batch_latency_ms",
            "measurements.embedding.mean_batch_latency_ms",
        ),
        (
            "items_per_second",
            "measurements.embedding.items_per_second",
        ),
        (
            "cold_model_load_ms",
            "measurements.cold_start.model_load_ms",
        ),
        (
            "cold_first_query_ms",
            "measurements.cold_start.first_query_ms",
        ),
        ("cold_total_ms", "measurements.cold_start.total_ms"),
        ("warm_samples", "measurements.warm_query_latency.samples"),
        (
            "warm_median_ms",
            "measurements.warm_query_latency.median_ms",
        ),
        ("warm_p95_ms", "measurements.warm_query_latency.p95_ms"),
    ];
    let peak: f64 = world.notes.get("peak").context("no peak")?.parse()?;
    let units: toml::Table = toml::from_str(world.notes.get("expected").context("expected")?)?;
    for (i, c) in json["configurations"]
        .as_array()
        .context("configurations")?
        .iter()
        .enumerate()
    {
        for (key, path) in checks {
            let (actual, want) = (number(c, path)?, expected(world, key)?);
            ensure!(
                (actual - want).abs() <= 1e-9 * want.abs().max(1.0),
                "configuration {i} {path}: {actual}, expected {want}"
            );
        }
        let ram = &c["measurements"]["peak_local_ram"];
        ensure!((number(ram, "value")? - peak).abs() < 0.5, "peak RAM {ram}");
        ensure!(
            ram["unit"] == units["peak_local_ram_unit"].as_str().unwrap_or("?"),
            "RAM unit {ram}"
        );
        let index = &c["measurements"]["index_size"];
        let recomputed = index_bytes(world, &world.configs[i])?;
        ensure!(
            (number(index, "value")? - recomputed as f64).abs() < 0.5,
            "index size {index}, recomputed {recomputed}"
        );
        ensure!(
            index["unit"] == units["index_size_unit"].as_str().unwrap_or("?"),
            "index unit"
        );
    }
    Ok(())
}

#[then("cold-start and warm-run measurements are reported separately")]
fn cold_and_warm(world: &mut LabWorld) -> Result<()> {
    let json: Value = serde_json::from_str(world.notes.get("json").context("no report")?)?;
    for c in json["configurations"]
        .as_array()
        .context("configurations")?
    {
        let m = &c["measurements"];
        ensure!(
            m["cold_start"].is_object() && m["warm_query_latency"].is_object(),
            "{m}"
        );
        ensure!(
            (number(m, "cold_start.total_ms")? - number(m, "warm_query_latency.median_ms")?).abs()
                > 1e-9,
            "cold and warm are the same value"
        );
    }
    let markdown = world.notes.get("markdown").context("no markdown")?;
    ensure!(
        markdown.contains("Cold start ms") && markdown.contains("Warm median ms"),
        "Markdown does not separate cold and warm"
    );
    Ok(())
}

#[then(
    "the report records hardware, corpus checksum, batch size, engine, model, and configuration fingerprints"
)]
fn provenance(world: &mut LabWorld) -> Result<()> {
    let report = world.comparison.as_ref().context("no report")?;
    let hardware = eval_fixture::controlled()?.hardware;
    ensure!(
        report.hardware == hardware,
        "hardware {:?}",
        report.hardware
    );
    let manifest = world.manifest()?;
    ensure!(
        report.corpus_checksum == manifest.manifest_checksum,
        "corpus checksum"
    );
    let markdown = world.notes.get("markdown").context("no markdown")?;
    ensure!(
        markdown.contains(&hardware.os),
        "hardware missing from Markdown"
    );
    ensure!(
        markdown.contains(report.corpus_checksum.as_str()),
        "checksum missing"
    );
    for c in &report.configurations {
        ensure!(
            c.batch_size == world.settings.batch_size.get(),
            "batch {}",
            c.batch_size
        );
        ensure!(c.engine == EngineId::Deterministic, "engine {}", c.engine);
        ensure!(
            markdown.contains(&c.model_id),
            "model {} missing",
            c.model_id
        );
        for fp in [
            &c.fingerprints.extraction,
            &c.fingerprints.chunking,
            &c.fingerprints.evaluation,
        ] {
            ensure!(
                markdown.contains(&fp.as_str()[..12]),
                "fingerprint {fp} missing"
            );
        }
        ensure!(
            c.fingerprints.extraction == report.configurations[0].fingerprints.extraction,
            "extraction differs"
        );
    }
    Ok(())
}

#[given("a remote provider run with known usage and a versioned price table")]
async fn remote_run(world: &mut LabWorld) -> Result<()> {
    eval_fixture::add_pdf_corpus(world)?;
    world.remote_allowed = true;
    let mut config =
        ExperimentConfig::load(&repo("config/experiments/text-openai-3-small.toml"))?.engine;
    config.max_input_tokens = NonZeroUsize::new(512);
    ensure!(config.engine.execution() == Execution::Remote, "not remote");
    let stand_in = build_contract_test(&config)?;
    ensure!(
        stand_in.capabilities().execution == Execution::Remote,
        "stand-in is local"
    );
    let builder = eval_fixture::keyword_builder()?
        .engine_label(config.engine)
        .model_id(config.model_id.clone())
        .capabilities(stand_in.capabilities().clone());
    let price = fixture(PRICE);
    let result = eval_fixture::measured_run(world, builder, LABELS, Some(&price), false).await?;
    ensure!(result.execution == Execution::Remote, "run was local");
    let usage = eval_fixture::whitespace_tokens(world, &result)?;
    world.notes.insert("usage".into(), usage.to_string());
    Ok(())
}

#[given("provider inference memory is not observable")]
fn provider_memory(world: &mut LabWorld) -> Result<()> {
    let c = world.configs.last().context("no run")?;
    ensure!(
        c.execution == Execution::Remote,
        "local engines observe their own memory"
    );
    Ok(())
}

#[when("I generate its cost and resource report")]
fn cost_and_resource(world: &mut LabWorld) -> Result<()> {
    let c = world.configs.last().context("no run")?;
    world.notes.insert("json".into(), serde_json::to_string(c)?);
    Ok(())
}

#[expect(
    clippy::cast_precision_loss,
    reason = "fixture counts and token sums are far below 2^52"
)]
#[then(
    "estimated cost equals the fixture usage divided by the price table's unit quantity and multiplied by its unit price"
)]
fn cost_formula(world: &mut LabWorld) -> Result<()> {
    let json: Value = serde_json::from_str(world.notes.get("json").context("no report")?)?;
    let price = PriceTable::load(&fixture(PRICE))?;
    let usage: u64 = world.notes.get("usage").context("no usage")?.parse()?;
    let want = usage as f64 / price.unit_quantity as f64 * price.unit_price;
    let cost = &json["cost"]["ingestion"];
    let actual = number(cost, "estimated_cost")?;
    ensure!(
        (actual - want).abs() <= 1e-15,
        "cost {actual}, expected {want}"
    );
    ensure!(
        cost["usage"].as_u64() == Some(usage),
        "usage {}",
        cost["usage"]
    );
    ensure!(actual > 0.0, "a known usage has a positive cost");
    Ok(())
}

#[then(
    "the report identifies the price version, currency, usage units, unit quantity, and estimate status"
)]
fn price_fields(world: &mut LabWorld) -> Result<()> {
    let json: Value = serde_json::from_str(world.notes.get("json").context("no report")?)?;
    let cost = &json["cost"]["ingestion"];
    ensure!(cost["price_version"] == "test-price-v1", "{cost}");
    ensure!(cost["currency"] == "USD", "{cost}");
    ensure!(cost["usage_unit"] == "token", "{cost}");
    ensure!(cost["unit_quantity"] == 1_000_000, "{cost}");
    ensure!(cost["estimate_status"] == "estimated", "{cost}");
    Ok(())
}

#[then(expr = "provider inference memory is marked {string}")]
fn marked(world: &mut LabWorld, status: String) -> Result<()> {
    let json: Value = serde_json::from_str(world.notes.get("json").context("no report")?)?;
    let memory = &json["measurements"]["provider_inference_memory"];
    ensure!(memory["status"] == status.as_str(), "{memory}");
    ensure!(
        memory.get("value").is_none(),
        "an unavailable value has no number: {memory}"
    );
    Ok(())
}

#[then("local peak RAM is labeled as local process memory")]
fn local_ram(world: &mut LabWorld) -> Result<()> {
    let json: Value = serde_json::from_str(world.notes.get("json").context("no report")?)?;
    let ram = &json["measurements"]["peak_local_ram"];
    let scope = ram["scope"].as_str().unwrap_or_default();
    ensure!(scope.starts_with("local process memory"), "{ram}");
    Ok(())
}

#[given("a run with missing provider usage or missing pricing data")]
async fn missing_data(world: &mut LabWorld) -> Result<()> {
    // Missing pricing: a real measured remote run without a price table.
    eval_fixture::add_pdf_corpus(world)?;
    world.remote_allowed = true;
    let config =
        ExperimentConfig::load(&repo("config/experiments/text-openai-3-small.toml"))?.engine;
    let stand_in = build_contract_test(&config)?;
    let builder = eval_fixture::keyword_builder()?
        .engine_label(config.engine)
        .model_id(config.model_id.clone())
        .capabilities(stand_in.capabilities().clone());
    let run = eval_fixture::measured_run(world, builder, LABELS, None, false).await?;
    // Missing usage: a provider response without usage, priced with a valid table.
    let price = PriceTable::load(&fixture(PRICE))?;
    let no_usage = estimate(Execution::Remote, None, Some(&price));
    world.notes.insert(
        "costs".into(),
        serde_json::to_string(&[run.cost.ingestion.clone(), no_usage])?,
    );
    Ok(())
}

#[when("I generate its cost report")]
fn cost_report(world: &mut LabWorld) -> Result<()> {
    let costs = world.notes.get("costs").context("no costs")?.clone();
    world.notes.insert("json".into(), costs);
    Ok(())
}

#[then("estimated provider cost is null with the missing-data reason")]
fn null_cost(world: &mut LabWorld) -> Result<()> {
    let json: Value = serde_json::from_str(world.notes.get("json").context("no report")?)?;
    let costs = json.as_array().context("costs")?;
    let reasons: Vec<&str> = costs.iter().filter_map(|c| c["reason"].as_str()).collect();
    ensure!(
        reasons.iter().any(|r| r.contains("missing pricing"))
            && reasons.iter().any(|r| r.contains("missing provider usage")),
        "reasons {reasons:?}"
    );
    for c in costs {
        ensure!(c["estimated_cost"].is_null(), "{c}");
        ensure!(c["estimate_status"] == "unavailable", "{c}");
    }
    Ok(())
}

#[then("unknown cost is not reported as zero")]
fn not_zero(world: &mut LabWorld) -> Result<()> {
    let json: Value = serde_json::from_str(world.notes.get("json").context("no report")?)?;
    for c in json.as_array().context("costs")? {
        ensure!(
            c["estimated_cost"] != 0 && c["estimated_cost"] != 0.0,
            "{c}"
        );
        ensure!(!c["estimated_cost"].is_number(), "{c}");
    }
    Ok(())
}
