//! The CLI sandbox and the P7 gate cases, shared by `tests/cli.rs` and `tests/bdd.rs`.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::{Map, Value, json};

use super::compare::{Side, first_difference, normalize_trajectory};

pub const BIN: &str = env!("CARGO_BIN_EXE_mini-swe-agent-rs");
pub const SUBMIT: &str = "echo COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT";

#[derive(Debug)]
pub struct Sandbox {
    _dir: tempfile::TempDir,
    pub root: PathBuf,
    pub env: BTreeMap<String, String>,
}

/// Test helpers: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::unwrap_used)]
impl Sandbox {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        for sub in ["home", "tmp", "config", "work", "out"] {
            fs::create_dir_all(root.join(sub)).unwrap();
        }
        let path = |sub: &str| root.join(sub).to_str().unwrap().to_owned();
        let env = BTreeMap::from([
            (
                "PATH".to_owned(),
                "/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin".to_owned(),
            ),
            ("HOME".to_owned(), path("home")),
            ("LANG".to_owned(), "C.UTF-8".to_owned()),
            ("TMPDIR".to_owned(), path("tmp")),
            ("MSWEA_SILENT_STARTUP".to_owned(), "1".to_owned()),
            ("MSWEA_CONFIGURED".to_owned(), "true".to_owned()),
            ("MSWEA_GLOBAL_CONFIG_DIR".to_owned(), path("config")),
        ]);
        Self {
            _dir: dir,
            root,
            env,
        }
    }

    pub fn run(&self, program: &Path, args: &[String], stdin: Option<&str>) -> Output {
        let mut child = Command::new(program)
            .args(args)
            .env_clear()
            .envs(&self.env)
            .current_dir(self.root.join("work"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        input
            .write_all(stdin.unwrap_or_default().as_bytes())
            .unwrap();
        drop(input);
        child.wait_with_output().unwrap()
    }

    pub fn rust(&self, args: &[&str]) -> Output {
        let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
        self.run(Path::new(BIN), &args, None)
    }

    pub fn trajectory(&self, name: &str) -> Value {
        serde_json::from_str(&fs::read_to_string(self.root.join("out").join(name)).unwrap())
            .unwrap()
    }

    pub fn out(&self, name: &str) -> String {
        self.root
            .join("out")
            .join(name)
            .to_str()
            .unwrap()
            .to_owned()
    }
}

pub fn outputs(commands: &[&str]) -> String {
    let outputs: Vec<Value> = commands
        .iter()
        .map(|c| json!({"role": "assistant", "content": format!("```mswea_bash_command\n{c}\n```"), "extra": {"actions": [{"command": c}], "cost": 1.0}}))
        .collect();
    format!("model.outputs={}", Value::Array(outputs))
}

pub fn base_args(sandbox: &Sandbox, commands: &[&str], output: &str) -> Vec<String> {
    vec![
        "--model-class".into(),
        "deterministic".into(),
        "-m".into(),
        "deterministic".into(),
        "-c".into(),
        "mini_textbased".into(),
        "-c".into(),
        outputs(commands),
        "-o".into(),
        sandbox.out(output),
    ]
}

pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// One P7 gate case: the same arguments for Python `mini` and the Rust CLI.
#[derive(Debug)]
pub struct Case {
    pub name: &'static str,
    pub commands: &'static [&'static str],
    pub extra: &'static [&'static str],
    /// The task comes from `run.task` in `extra`, so no `-t` is passed.
    pub task_in_config: bool,
    pub exit_code: i32,
    pub exit_status: &'static str,
}

pub const CASES: &[Case] = &[
    Case {
        name: "submit",
        commands: &["echo hello > hello.txt && cat hello.txt", SUBMIT],
        extra: &[],
        task_in_config: false,
        exit_code: 0,
        exit_status: "Submitted",
    },
    Case {
        name: "cost_limit_flag",
        commands: &["echo 1", "echo 2", "echo 3"],
        extra: &["-l", "2"],
        task_in_config: false,
        exit_code: 0,
        exit_status: "LimitsExceeded",
    },
    Case {
        name: "task_from_config",
        commands: &["pwd", SUBMIT],
        extra: &["-c", "run.task=Read the task from the config"],
        task_in_config: true,
        exit_code: 0,
        exit_status: "Submitted",
    },
    Case {
        name: "outputs_exhausted",
        commands: &["echo only"],
        extra: &[],
        task_in_config: false,
        exit_code: 1,
        exit_status: "OutputsExhausted",
    },
];

/// Runs one case through Python `mini` (at `mini`) and the Rust CLI, one after the other in a
/// fresh workspace at the same path, and compares exit codes, normalized trajectories and files.
pub fn compare_case(mini: &Path, case: &Case) -> Result<(), String> {
    let sandbox = Sandbox::new();
    let mut args = vec!["--agent-class".to_owned(), "default".to_owned()];
    args.extend(base_args(&sandbox, case.commands, "traj.json"));
    if !case.task_in_config {
        args.extend(["-t".into(), format!("CLI parity case {}", case.name)]);
    }
    args.extend(case.extra.iter().map(|a| (*a).to_owned()));

    let saved = |side: &str, output: &Output| {
        if sandbox.root.join("out/traj.json").is_file() {
            Ok(sandbox.trajectory("traj.json"))
        } else {
            Err(format!(
                "{side} wrote no trajectory (exit {:?}): {}",
                output.status.code(),
                stderr(output)
            ))
        }
    };
    let python = sandbox.run(mini, &args, None);
    let python_trajectory = saved("python", &python)?;
    let python_files = super::snapshot_dir(&sandbox.root.join("work"));
    fs::remove_dir_all(sandbox.root.join("work")).unwrap();
    fs::remove_file(sandbox.root.join("out/traj.json")).unwrap();
    fs::create_dir_all(sandbox.root.join("work")).unwrap();

    let rust_args: Vec<String> = args.iter().skip(2).cloned().collect();
    let rust = sandbox.run(Path::new(BIN), &rust_args, None);
    let rust_trajectory = saved("rust", &rust)?;
    let rust_files = super::snapshot_dir(&sandbox.root.join("work"));

    let mut problems = Vec::new();
    for (side, output) in [("python", &python), ("rust", &rust)] {
        if output.status.code() != Some(case.exit_code) {
            problems.push(format!(
                "{side} exit code {:?}: {}",
                output.status.code(),
                stderr(output)
            ));
        }
    }
    let fatal = case.exit_code == 1;
    let map = Map::new();
    let (mut left, mut right) = (python_trajectory, rust_trajectory);
    normalize_trajectory(&mut left, fatal, Side::Python, &map);
    normalize_trajectory(&mut right, fatal, Side::Rust, &map);
    if right["info"]["exit_status"] != case.exit_status {
        problems.push(format!("rust exit status {}", right["info"]["exit_status"]));
    }
    if let Some(diff) = first_difference(&left, &right) {
        problems.push(format!("trajectory: {diff}"));
    }
    if let Some(diff) = first_difference(&python_files, &rust_files) {
        problems.push(format!("workspace: {diff}"));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("\n"))
    }
}
