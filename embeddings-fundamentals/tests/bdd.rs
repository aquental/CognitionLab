//! Acceptance suite (PRD §5, PLAN §5.2).
//!
//! Executes `tests/features` with `fail_on_skipped`, writes `reports/bdd/cucumber.json`,
//! and fails unless every scenario the PRD requires for the configured milestone
//! (`config/acceptance.toml`) was executed and passed. Pass cucumber filters through
//! `BDD_ARGS` (for example `BDD_ARGS='--name hemoglobina'`); a filtered run fails the
//! inventory check by design.

#[path = "bdd/steps/mod.rs"]
mod steps;
mod support;

use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, Result};
use cucumber::World;
use embeddings_fundamentals::embedding::ErrorClass;
use embeddings_fundamentals::embedding::deterministic::DeterministicEmbedder;

use support::runner::{Suite, run_suite, split_args};

/// Per-scenario state. Cucumber builds a fresh `World` for every scenario, so each one
/// gets its own temporary store directory, adapter call log, and fault plan.
#[derive(Debug, World)]
#[world(init = Self::new)]
pub struct LabWorld {
    /// Isolated store root, deleted when the scenario ends.
    pub store: tempfile::TempDir,
    /// Deterministic adapter: its call log and fault plan belong to this scenario.
    pub embedder: Arc<DeterministicEmbedder>,
    /// Outcome of the last embedding attempt made by a step.
    pub last_embedding: Option<Result<(), ErrorClass>>,
}

impl LabWorld {
    fn new() -> Result<Self> {
        Ok(Self {
            store: tempfile::tempdir().context("create scenario store")?,
            embedder: Arc::new(DeterministicEmbedder::builder().build()?),
            last_embedding: None,
        })
    }
}

#[tokio::main]
async fn main() -> Result<ExitCode> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let milestone = support::inventory::acceptance_milestone(&root.join("config/acceptance.toml"))?;
    let cucumber_args = match std::env::var("BDD_ARGS") {
        Ok(args) => split_args(&args)?,
        Err(_) => Vec::new(),
    };
    let report = run_suite::<LabWorld>(Suite {
        features: &root.join("tests/features"),
        prd: &root.join("spec/PRD.md"),
        milestone,
        json_report: root.join("reports/bdd/cucumber.json"),
        console: true,
        cucumber_args,
    })
    .await?;
    println!(
        "bdd inventory: milestone {milestone}, {} required, {} executed",
        report.required, report.executed
    );
    for problem in &report.problems {
        eprintln!("bdd inventory: {problem}");
    }
    Ok(if report.passed() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
