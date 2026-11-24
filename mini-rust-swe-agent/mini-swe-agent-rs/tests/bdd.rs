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

/// CLI gate cases run by `cli.feature`, for the same check.
static EXECUTED_CLI: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

#[derive(Debug, Default, World)]
struct ParityWorld {
    scenario: Option<Value>,
    python: Option<Value>,
    rust: Option<Value>,
    cli_case: Option<&'static support::cli::Case>,
    cli_result: Option<Result<(), String>>,
    sandbox: Option<support::cli::Sandbox>,
    trajectory: Option<Value>,
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

// ---- cli.feature (P7) ----------------------------------------------------------------------

#[given(expr = "the CLI case {string}")]
fn given_cli_case(world: &mut ParityWorld, name: String) {
    let case = support::cli::CASES
        .iter()
        .find(|c| c.name == name)
        .expect("unknown CLI case");
    world.cli_case = Some(case);
    EXECUTED_CLI.lock().unwrap().insert(name);
}

#[when("I run the Python and Rust CLIs")]
fn run_both_clis(world: &mut ParityWorld) {
    let harness = support::Harness::new().expect("reference checked in main");
    let mini = harness.python_bin_dir().join("mini");
    world.cli_result = Some(support::cli::compare_case(
        &mini,
        world.cli_case.expect("no case"),
    ));
}

#[then(expr = "both CLIs exit with code {int} and write equal trajectories")]
fn clis_agree(world: &mut ParityWorld, code: i32) {
    assert_eq!(world.cli_case.expect("no case").exit_code, code);
    if let Some(Err(problem)) = &world.cli_result {
        panic!("{problem}");
    }
}

#[given("a Rust CLI sandbox")]
fn given_sandbox(world: &mut ParityWorld) {
    world.sandbox = Some(support::cli::Sandbox::new());
}

#[given(expr = "{string} in MSWEA_CONFIG_DIR sets the system template to {string}")]
fn given_config_file(world: &mut ParityWorld, name: String, system: String) {
    let sandbox = world.sandbox.get_or_insert_with(support::cli::Sandbox::new);
    let dir = sandbox.root.join("configs");
    std::fs::create_dir_all(&dir).unwrap();
    let text =
        format!("agent:\n  system_template: {system}\n  instance_template: '{{{{task}}}}'\n");
    std::fs::write(dir.join(name), text).unwrap();
    sandbox
        .env
        .insert("MSWEA_CONFIG_DIR".into(), dir.to_str().unwrap().into());
}

fn run_rust_cli(world: &mut ParityWorld, config: &str, flag: &str, config_task: &str, stdin: &str) {
    let sandbox = world.sandbox.as_ref().expect("no sandbox");
    let mut args: Vec<String> = [
        "--model-class",
        "deterministic",
        "-m",
        "deterministic",
        "-c",
        config,
    ]
    .iter()
    .map(|a| (*a).to_owned())
    .collect();
    args.extend([
        "-c".to_owned(),
        support::cli::outputs(&[support::cli::SUBMIT]),
        "-o".to_owned(),
        sandbox.out("bdd.json"),
    ]);
    if !config_task.is_empty() {
        args.extend(["-c".to_owned(), format!("run.task={config_task}")]);
    }
    if !flag.is_empty() {
        args.extend(["-t".to_owned(), flag.to_owned()]);
    }
    let output = sandbox.run(std::path::Path::new(support::cli::BIN), &args, Some(stdin));
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        support::cli::stderr(&output)
    );
    world.trajectory = Some(sandbox.trajectory("bdd.json"));
}

#[when(expr = "the Rust CLI runs with task flag {string}, config task {string} and stdin {string}")]
fn run_with_task_sources(
    world: &mut ParityWorld,
    flag: String,
    config_task: String,
    stdin: String,
) {
    run_rust_cli(world, "mini_textbased", &flag, &config_task, &stdin);
}

#[when(expr = "the Rust CLI runs with config {string} and task {string}")]
fn run_with_config(world: &mut ParityWorld, config: String, task: String) {
    run_rust_cli(world, &config, &task, "", "");
}

fn message(world: &ParityWorld, index: usize) -> String {
    let trajectory = world.trajectory.as_ref().expect("no CLI run");
    trajectory["messages"][index]["content"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

#[then(expr = "the task message contains {string}")]
fn task_message(world: &mut ParityWorld, text: String) {
    let content = message(world, 1);
    assert!(content.contains(&text), "{content:?}");
}

#[then(expr = "the system message starts with {string}")]
fn system_message(world: &mut ParityWorld, text: String) {
    let content = message(world, 0);
    assert!(content.starts_with(&text), "{content:?}");
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
    let cli_manifest: BTreeSet<String> = support::cli::CASES
        .iter()
        .map(|c| c.name.to_owned())
        .collect();
    let cli_executed = EXECUTED_CLI.lock().unwrap().clone();
    if cli_executed != cli_manifest {
        eprintln!("bdd: CLI case mismatch; expected {cli_manifest:?}, ran {cli_executed:?}");
        failed = true;
    }
    if failed {
        std::process::exit(1);
    }
}
