//! Acceptance suite (PRD §5, PLAN §5.2).
//!
//! Executes `tests/features` with `fail_on_skipped`, writes `reports/bdd/cucumber.json`,
//! and fails unless every scenario the PRD requires for the configured milestone
//! (`config/acceptance.toml`) was executed and passed. Pass cucumber filters through
//! `BDD_ARGS` (for example `BDD_ARGS='--name hemoglobina'`); a filtered run fails the
//! inventory check by design.

mod support;

use std::path::Path;
use std::process::ExitCode;

use anyhow::Result;
use cucumber::World;

use support::runner::{Suite, run_suite};

/// Per-scenario state. Each scenario gets a fresh `World` and an isolated store.
#[derive(Debug, Default, World)]
pub struct LabWorld {}

#[tokio::main]
async fn main() -> Result<ExitCode> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let milestone = support::inventory::acceptance_milestone(&root.join("config/acceptance.toml"))?;
    let cucumber_args = std::env::var("BDD_ARGS")
        .map(|args| args.split_whitespace().map(str::to_owned).collect())
        .unwrap_or_default();
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
