//! BDD specs (decision D9, spec/migration-plan.md §3.15 item 5): Gherkin features in
//! `tests/features/` over the same parity scenarios and harness as `tests/parity.rs`.
//!
//! Each step is thin: it loads a JSON scenario, runs it through the Python reference and the Rust
//! library, and checks the normalized results. The runner fails on skipped steps, and after the
//! run the executed scenario ids and CLI cases must equal their manifests.
//!
//! `harness = false`: no `cfg(test)`, so clippy.toml's test exemptions do not apply. The steps
//! return `Result` and use no `unwrap`, `expect`, `panic!` or indexing (plan §3.5).

// The shared parity harness (`tests/support/`) declares its own, module-scoped lint exemptions:
// it treats a broken fixture or reference environment as a setup failure and panics with the
// reason. Nothing in this file is exempt.
mod support;

use std::collections::BTreeSet;
use std::sync::Mutex;

use cucumber::writer::Stats as _;
use cucumber::{World, given, then, when};
use serde_json::Value;

/// A step result: the message is what Cucumber reports.
type Step = Result<(), String>;

/// Scenario ids run by the features, for the manifest check.
static EXECUTED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// CLI gate cases that ran and matched, for the same check.
static EXECUTED_CLI: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn record(list: &Mutex<Vec<String>>, name: String) -> Step {
    list.lock()
        .map_err(|_| "a step panicked while recording".to_owned())?
        .push(name);
    Ok(())
}

fn recorded(list: &Mutex<Vec<String>>) -> Vec<String> {
    list.lock().map(|l| l.clone()).unwrap_or_default()
}

fn ensure(condition: bool, message: impl FnOnce() -> String) -> Step {
    if condition { Ok(()) } else { Err(message()) }
}

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
    fn scenario(&self) -> Result<&Value, String> {
        self.scenario
            .as_ref()
            .ok_or_else(|| "no scenario loaded".to_owned())
    }

    fn results(&self) -> Result<[(&'static str, &Value); 2], String> {
        match (&self.python, &self.rust) {
            (Some(python), Some(rust)) => Ok([("python", python), ("rust", rust)]),
            _ => Err("the scenario has not run".to_owned()),
        }
    }
}

/// `result.results[index - 1]` (Gherkin counts from 1).
fn command(result: &Value, index: usize) -> Result<&Value, String> {
    index
        .checked_sub(1)
        .and_then(|i| result.get("results")?.get(i))
        .ok_or_else(|| format!("no command {index}"))
}

#[given(expr = "the scenario {string}")]
fn given_scenario(world: &mut ParityWorld, id: String) -> Step {
    world.scenario = Some(support::scenario(&id));
    record(&EXECUTED, id)
}

#[when("I run it in the Python and Rust harnesses")]
fn run_both(world: &mut ParityWorld) -> Step {
    let harness = support::Harness::new().ok_or("no Python reference")?;
    let id = world
        .scenario()?
        .get("id")
        .and_then(Value::as_str)
        .ok_or("scenario without an id")?
        .to_owned();
    let mut python = harness.run_python(&[&id]);
    world.python = Some(python.remove(&id).ok_or("no Python result")?);
    world.rust = Some(harness.run_rust(world.scenario()?));
    Ok(())
}

#[then(expr = "each harness made exactly {int} model query/queries")]
fn query_count(world: &mut ParityWorld, expected: usize) -> Step {
    for (side, result) in world.results()? {
        let count = support::query_count(result);
        ensure(count == expected, || format!("{side}: {count} queries"))?;
    }
    Ok(())
}

#[then(expr = "both end with exit status {string}")]
fn exit_status(world: &mut ParityWorld, expected: String) -> Step {
    let scenario = world.scenario()?;
    for (side, result) in world.results()? {
        let status = support::exit_status(result).map(|s| match side {
            "python" => support::mapped(&s, scenario),
            _ => s,
        });
        ensure(status.as_deref() == Some(expected.as_str()), || {
            format!("{side}: {status:?}")
        })?;
    }
    Ok(())
}

#[then(expr = "command {int} returned code {int}")]
fn command_code(world: &mut ParityWorld, index: usize, code: i64) -> Step {
    for (side, result) in world.results()? {
        let output = command(result, index)?.get("output");
        let returned = output
            .and_then(|o| o.get("returncode"))
            .and_then(Value::as_i64);
        ensure(returned == Some(code), || format!("{side}: {output:?}"))?;
    }
    Ok(())
}

#[then(expr = "command {int} output contains {string}")]
fn command_output(world: &mut ParityWorld, index: usize, text: String) -> Step {
    for (side, result) in world.results()? {
        let output = command(result, index)?
            .pointer("/output/output")
            .and_then(Value::as_str)
            .unwrap_or_default();
        ensure(output.contains(&text), || format!("{side}: {output:?}"))?;
    }
    Ok(())
}

#[then(expr = "command {int} submitted {string}")]
fn command_submitted(world: &mut ParityWorld, index: usize, text: String) -> Step {
    // Gherkin strings keep `\n` literally.
    let expected = text.replace("\\n", "\n");
    for (side, result) in world.results()? {
        let submitted = command(result, index)?
            .pointer("/submitted/extra/submission")
            .and_then(Value::as_str);
        ensure(submitted == Some(expected.as_str()), || {
            format!("{side}: {submitted:?}")
        })?;
    }
    Ok(())
}

#[then("the normalized results are equal")]
fn results_equal(world: &mut ParityWorld) -> Step {
    let scenario = world.scenario()?.clone();
    let [(_, python), (_, rust)] = world.results()?;
    support::check(&scenario, python.clone(), rust.clone())
        .map_err(|problem| format!("{}:\n{problem}", scenario.get("id").unwrap_or(&Value::Null)))
}

// ---- cli.feature (P7) ----------------------------------------------------------------------

#[given(expr = "the CLI case {string}")]
fn given_cli_case(world: &mut ParityWorld, name: String) -> Step {
    let case = support::cli::CASES
        .iter()
        .find(|c| c.name == name)
        .ok_or_else(|| format!("unknown CLI case {name}"))?;
    world.cli_case = Some(case);
    Ok(())
}

#[when("I run the Python and Rust CLIs")]
fn run_both_clis(world: &mut ParityWorld) -> Step {
    let harness = support::Harness::new().ok_or("no Python reference")?;
    let mini = harness.python_bin_dir().join("mini");
    let case = world.cli_case.ok_or("no CLI case")?;
    world.cli_result = Some(support::cli::compare_case(&mini, case));
    Ok(())
}

#[then(expr = "both CLIs exit with code {int} and write equal trajectories")]
fn clis_agree(world: &mut ParityWorld, code: i32) -> Step {
    let case = world.cli_case.ok_or("no CLI case")?;
    ensure(case.exit_code == code, || {
        format!(
            "{}: the fixture expects exit code {}",
            case.name, case.exit_code
        )
    })?;
    match &world.cli_result {
        Some(Ok(())) => record(&EXECUTED_CLI, case.name.to_owned()),
        Some(Err(problem)) => Err(problem.clone()),
        None => Err("the CLIs did not run".to_owned()),
    }
}

#[given("a Rust CLI sandbox")]
fn given_sandbox(world: &mut ParityWorld) -> Step {
    world.sandbox = Some(support::cli::Sandbox::new());
    Ok(())
}

#[given(expr = "{string} in MSWEA_CONFIG_DIR sets the system template to {string}")]
fn given_config_file(world: &mut ParityWorld, name: String, system: String) -> Step {
    let sandbox = world.sandbox.get_or_insert_with(support::cli::Sandbox::new);
    let dir = sandbox.root.join("configs");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let text =
        format!("agent:\n  system_template: {system}\n  instance_template: '{{{{task}}}}'\n");
    std::fs::write(dir.join(name), text).map_err(|e| e.to_string())?;
    let dir = dir.to_str().ok_or("non-UTF-8 sandbox path")?.to_owned();
    sandbox.env.insert("MSWEA_CONFIG_DIR".into(), dir);
    Ok(())
}

fn run_rust_cli(
    world: &mut ParityWorld,
    config: &str,
    flag: &str,
    config_task: &str,
    stdin: &str,
) -> Step {
    let sandbox = world.sandbox.as_ref().ok_or("no sandbox")?;
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
    ensure(output.status.code() == Some(0), || {
        support::cli::stderr(&output)
    })?;
    world.trajectory = Some(sandbox.trajectory("bdd.json"));
    Ok(())
}

#[when(expr = "the Rust CLI runs with task flag {string}, config task {string} and stdin {string}")]
fn run_with_task_sources(
    world: &mut ParityWorld,
    flag: String,
    config_task: String,
    stdin: String,
) -> Step {
    run_rust_cli(world, "mini_textbased", &flag, &config_task, &stdin)
}

#[when(expr = "the Rust CLI runs with config {string} and task {string}")]
fn run_with_config(world: &mut ParityWorld, config: String, task: String) -> Step {
    run_rust_cli(world, &config, &task, "", "")
}

fn message(world: &ParityWorld, index: usize) -> Result<String, String> {
    let trajectory = world.trajectory.as_ref().ok_or("no CLI run")?;
    Ok(trajectory
        .get("messages")
        .and_then(|m| m.get(index))
        .and_then(|m| m.get("content"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned())
}

#[then(expr = "the task message contains {string}")]
fn task_message(world: &mut ParityWorld, text: String) -> Step {
    let content = message(world, 1)?;
    ensure(content.contains(&text), || format!("{content:?}"))
}

#[then(expr = "the system message starts with {string}")]
fn system_message(world: &mut ParityWorld, text: String) -> Step {
    let content = message(world, 0)?;
    ensure(content.starts_with(&text), || format!("{content:?}"))
}

/// Each manifest entry must have run exactly once.
fn manifest_problems(kind: &str, manifest: &BTreeSet<String>, ran: &[String]) -> Vec<String> {
    let mut problems = Vec::new();
    let ran_set: BTreeSet<&String> = ran.iter().collect();
    for name in manifest {
        let count = ran.iter().filter(|r| *r == name).count();
        if count != 1 {
            problems.push(format!("{kind} {name} ran {count} times"));
        }
    }
    for name in ran_set {
        if !manifest.contains(name) {
            problems.push(format!("unknown {kind} {name}"));
        }
    }
    problems
}

fn main() -> std::process::ExitCode {
    if support::reference_dir().is_none() {
        eprintln!("bdd: skipped on purpose (MSWEA_RS_NO_REFERENCE=1); this is not a pass");
        return std::process::ExitCode::SUCCESS;
    }
    let features = support::crate_dir().join("tests/features");
    let writer = futures::executor::block_on(
        ParityWorld::cucumber()
            .max_concurrent_scenarios(4)
            .fail_on_skipped()
            .run(features),
    );
    let scenarios: BTreeSet<String> = support::load_scenarios()
        .iter()
        .filter_map(|s| s.get("id").and_then(Value::as_str).map(str::to_owned))
        .collect();
    let cases: BTreeSet<String> = support::cli::CASES
        .iter()
        .map(|c| c.name.to_owned())
        .collect();
    let mut problems = manifest_problems("scenario", &scenarios, &recorded(&EXECUTED));
    problems.extend(manifest_problems(
        "CLI case",
        &cases,
        &recorded(&EXECUTED_CLI),
    ));
    for problem in &problems {
        eprintln!("bdd: {problem}");
    }
    if writer.execution_has_failed() || writer.skipped_steps() > 0 || !problems.is_empty() {
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}
