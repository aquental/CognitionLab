//! The `mini-swe-agent-rs` binary (spec/migration-plan.md §3.12, phase P7): exit codes, task
//! order, the global config dir and `.env`, rejected interactive flags, and the P7 gate: the
//! Python `mini` CLI and the Rust CLI write equal trajectories.

mod support;

use std::fs;
use std::path::Path;

use support::cli::{BIN, CASES, SUBMIT, Sandbox, base_args, compare_case, stderr};

// ---- Rust-only behavior ------------------------------------------------------------------------

#[test]
fn submit_exits_zero_and_saves() {
    let sandbox = Sandbox::new();
    let mut args = base_args(&sandbox, &["echo hi", SUBMIT], "t.json");
    args.extend(["-t".into(), "say hi".into()]);
    let output = sandbox.run(Path::new(BIN), &args, None);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let trajectory = sandbox.trajectory("t.json");
    assert_eq!(trajectory["info"]["exit_status"], "Submitted");
    assert!(
        trajectory["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("say hi")
    );
}

#[test]
fn limits_exit_zero_and_fatal_errors_exit_one_after_saving() {
    let sandbox = Sandbox::new();
    let mut args = base_args(&sandbox, &["echo 1", "echo 2"], "limit.json");
    args.extend(["-t".into(), "t".into(), "-l".into(), "1".into()]);
    let output = sandbox.run(Path::new(BIN), &args, None);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        sandbox.trajectory("limit.json")["info"]["exit_status"],
        "LimitsExceeded"
    );

    let mut args = base_args(&sandbox, &["echo 1"], "fatal.json");
    args.extend(["-t".into(), "t".into()]);
    let output = sandbox.run(Path::new(BIN), &args, None);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert_eq!(
        sandbox.trajectory("fatal.json")["info"]["exit_status"],
        "OutputsExhausted"
    );
}

#[test]
fn task_order_flag_then_config_then_stdin() {
    let sandbox = Sandbox::new();
    let task_of = |name: &str| {
        sandbox.trajectory(name)["messages"][1]["content"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let mut args = base_args(&sandbox, &[SUBMIT], "a.json");
    args.extend([
        "-c".into(),
        "run.task=from config".into(),
        "-t".into(),
        "from flag".into(),
    ]);
    assert_eq!(
        sandbox
            .run(Path::new(BIN), &args, Some("from stdin"))
            .status
            .code(),
        Some(0)
    );
    assert!(task_of("a.json").contains("from flag"));

    let mut args = base_args(&sandbox, &[SUBMIT], "b.json");
    args.extend(["-c".into(), "run.task=from config".into()]);
    assert_eq!(
        sandbox
            .run(Path::new(BIN), &args, Some("from stdin"))
            .status
            .code(),
        Some(0)
    );
    assert!(task_of("b.json").contains("from config"));

    let args = base_args(&sandbox, &[SUBMIT], "c.json");
    assert_eq!(
        sandbox
            .run(Path::new(BIN), &args, Some("from stdin\n"))
            .status
            .code(),
        Some(0)
    );
    assert!(task_of("c.json").contains("from stdin\n"));
}

#[test]
fn output_defaults_to_the_global_config_dir() {
    let sandbox = Sandbox::new();
    let args: Vec<String> = base_args(&sandbox, &[SUBMIT], "unused.json")
        .into_iter()
        .take(8)
        .collect();
    let mut args = args;
    args.extend(["-t".into(), "t".into()]);
    let output = sandbox.run(Path::new(BIN), &args, None);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let saved = sandbox.root.join("config/last_mini_run.traj.json");
    assert!(saved.is_file(), "{}", saved.display());
}

#[test]
fn global_env_file_is_an_overlay() {
    let sandbox = Sandbox::new();
    fs::write(
        sandbox.root.join("config/.env"),
        "MSWEA_MODEL_NAME=deterministic\nLANG=from-dotenv\n",
    )
    .unwrap();
    let mut args: Vec<String> = base_args(&sandbox, &["printf '%s' \"$LANG\"", SUBMIT], "env.json");
    // Drop `-m deterministic`: the name must come from the .env file.
    args.drain(2..4);
    args.extend(["-t".into(), "t".into()]);
    let output = sandbox.run(Path::new(BIN), &args, None);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let trajectory = sandbox.trajectory("env.json");
    assert_eq!(
        trajectory["info"]["config"]["model"]["model_name"],
        "deterministic"
    );
    assert_eq!(
        trajectory["messages"][3]["extra"]["raw_output"], "C.UTF-8",
        "existing variables win"
    );

    // A missing .env is an empty overlay, and the config dir is created.
    let mut sandbox = Sandbox::new();
    let fresh = sandbox.root.join("fresh-config");
    sandbox.env.insert(
        "MSWEA_GLOBAL_CONFIG_DIR".into(),
        fresh.to_str().unwrap().into(),
    );
    let mut args = base_args(&sandbox, &[SUBMIT], "x.json");
    args.extend(["-t".into(), "t".into()]);
    assert_eq!(
        sandbox.run(Path::new(BIN), &args, None).status.code(),
        Some(0)
    );
    assert!(fresh.is_dir());
}

#[test]
fn banner_unless_silent() {
    let mut sandbox = Sandbox::new();
    let mut args = base_args(&sandbox, &[SUBMIT], "b.json");
    args.extend(["-t".into(), "t".into()]);
    assert!(
        !stderr(&sandbox.run(Path::new(BIN), &args, None)).contains("This is mini-swe-agent-rs")
    );
    sandbox.env.remove("MSWEA_SILENT_STARTUP");
    let loud = stderr(&sandbox.run(Path::new(BIN), &args, None));
    assert!(loud.contains("This is mini-swe-agent-rs version"), "{loud}");
}

#[test]
fn usage_and_config_errors() {
    let sandbox = Sandbox::new();
    for flag in ["-y", "--yolo", "--exit-immediately"] {
        let output = sandbox.rust(&[flag, "-t", "t"]);
        assert_eq!(output.status.code(), Some(2), "{flag}");
        assert!(
            stderr(&output).contains("interactive"),
            "{flag}: {}",
            stderr(&output)
        );
    }
    assert_eq!(sandbox.rust(&["--no-such-flag"]).status.code(), Some(2));
    let output = sandbox.rust(&["-m", "gpt-x", "-t", "t"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("P8"), "{}", stderr(&output));
    let output = sandbox.rust(&[
        "--model-class",
        "deterministic",
        "-m",
        "d",
        "--environment-class",
        "docker",
        "-t",
        "t",
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("docker"), "{}", stderr(&output));
    let output = sandbox.rust(&["-c", "missing-config", "-t", "t"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("Could not find config file"),
        "{}",
        stderr(&output)
    );
}

// ---- The P7 gate: Python `mini` and the Rust CLI -------------------------------------------------

#[test]
fn python_and_rust_clis_write_equal_trajectories() {
    let Some(harness) = support::Harness::new() else {
        eprintln!("skipped on purpose: MSWEA_RS_NO_REFERENCE=1");
        return;
    };
    let mini = harness.python_bin_dir().join("mini");
    let failures: Vec<String> = CASES
        .iter()
        .filter_map(|case| {
            compare_case(&mini, case)
                .err()
                .map(|e| format!("--- {}\n{e}", case.name))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
