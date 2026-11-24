//! BDD specs (decision D9, spec/migration-plan.md §3.15 item 5): Gherkin features in
//! `tests/features/` over the same parity scenarios and harness as `tests/parity.rs`.
//!
//! Each step is thin: it loads a JSON scenario, runs it through the Python reference and the Rust
//! library, and asserts on the normalized results. The runner fails on skipped steps, and after
//! the run the executed scenario ids must equal the `parity/scenarios/` manifest.

// `harness = false`: no `cfg(test)`, so clippy.toml's test exemptions do not apply here.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use std::collections::BTreeSet;
use std::sync::Mutex;

use cucumber::writer::Stats as _;
use cucumber::{World, given, then, when};
use serde_json::Value;

/// Scenario ids run by the features, for the manifest check.
static EXECUTED: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

#[derive(Debug, Default, World)]
struct ParityWorld {
    scenario: Option<Value>,
    python: Option<Value>,
    rust: Option<Value>,
}

impl ParityWorld {
    fn scenario(&self) -> &Value {
        self.scenario.as_ref().expect("no scenario loaded")
    }

    fn results(&self) -> [(&'static str, &Value); 2] {
        [
            ("python", self.python.as_ref().expect("not run")),
            ("rust", self.rust.as_ref().expect("not run")),
        ]
    }
}

#[given(expr = "the scenario {string}")]
fn given_scenario(world: &mut ParityWorld, id: String) {
    world.scenario = Some(support::scenario(&id));
    EXECUTED.lock().unwrap().insert(id);
}

#[when("I run it in the Python and Rust harnesses")]
fn run_both(world: &mut ParityWorld) {
    let harness = support::Harness::new().expect("reference checked in main");
    let id = world.scenario()["id"].as_str().unwrap().to_owned();
    let mut python = harness.run_python(&[&id]);
    world.python = Some(python.remove(&id).expect("no Python result"));
    world.rust = Some(harness.run_rust(world.scenario()));
}

#[then(expr = "each harness made exactly {int} model query/queries")]
fn query_count(world: &mut ParityWorld, expected: usize) {
    for (side, result) in world.results() {
        assert_eq!(support::query_count(result), expected, "{side}");
    }
}

#[then(expr = "both end with exit status {string}")]
fn exit_status(world: &mut ParityWorld, expected: String) {
    for (side, result) in world.results() {
        let status = support::exit_status(result).map(|s| match side {
            "python" => support::mapped(&s, world.scenario()),
            _ => s,
        });
        assert_eq!(status.as_deref(), Some(expected.as_str()), "{side}");
    }
}

#[then(expr = "command {int} returned code {int}")]
fn command_code(world: &mut ParityWorld, index: usize, code: i64) {
    for (side, result) in world.results() {
        let output = &result["results"][index - 1]["output"];
        assert_eq!(
            output["returncode"].as_i64(),
            Some(code),
            "{side}: {output}"
        );
    }
}

#[then(expr = "command {int} output contains {string}")]
fn command_output(world: &mut ParityWorld, index: usize, text: String) {
    for (side, result) in world.results() {
        let output = result["results"][index - 1]["output"]["output"]
            .as_str()
            .unwrap_or_default();
        assert!(output.contains(&text), "{side}: {output:?}");
    }
}

#[then(expr = "command {int} submitted {string}")]
fn command_submitted(world: &mut ParityWorld, index: usize, text: String) {
    // Gherkin strings keep `\n` literally.
    let expected = text.replace("\\n", "\n");
    for (side, result) in world.results() {
        let submitted = &result["results"][index - 1]["submitted"];
        assert_eq!(
            submitted["extra"]["submission"].as_str(),
            Some(expected.as_str()),
            "{side}: {submitted}"
        );
    }
}

#[then("the normalized results are equal")]
fn results_equal(world: &mut ParityWorld) {
    let scenario = world.scenario().clone();
    let (python, rust) = (world.python.clone().unwrap(), world.rust.clone().unwrap());
    if let Err(problem) = support::check(&scenario, python, rust) {
        panic!("{}:\n{problem}", scenario["id"]);
    }
}

fn main() {
    if support::reference_dir().is_none() {
        eprintln!("bdd: skipped on purpose (MSWEA_RS_NO_REFERENCE=1)");
        return;
    }
    let features = support::crate_dir().join("tests/features");
    let writer = futures::executor::block_on(
        ParityWorld::cucumber()
            .max_concurrent_scenarios(4)
            .fail_on_skipped()
            .run(features),
    );
    let manifest: BTreeSet<String> = support::load_scenarios()
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_owned())
        .collect();
    let executed = EXECUTED.lock().unwrap().clone();
    let mut failed = writer.execution_has_failed() || writer.skipped_steps() > 0;
    if executed != manifest {
        let missing: Vec<_> = manifest.difference(&executed).collect();
        let unknown: Vec<_> = executed.difference(&manifest).collect();
        eprintln!("bdd: scenario manifest mismatch; missing {missing:?}, unknown {unknown:?}");
        failed = true;
    }
    if failed {
        std::process::exit(1);
    }
}
