//! The differential parity harness shared by `tests/parity.rs` and `tests/bdd.rs`
//! (spec/migration-plan.md §3.15 items 4-5).
//!
//! - Scenarios are the JSON files in `parity/scenarios/`.
//! - The Python side is `parity/run_python.py`, run with the reference environment's interpreter.
//! - The Rust side builds the same agent through the library.
//! - Both sides get one explicit, credential-free environment and run each scenario in a fresh
//!   workspace at the same absolute path.
//!
//! Test support code: helpers may panic on broken fixtures or a broken reference environment.
#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

pub mod cli;
pub mod compare;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::sync::{Arc, OnceLock};

use mini_swe_agent_rs::agent::{Agent, AgentConfig, DefaultAgent};
use mini_swe_agent_rs::clock::{Clock, SystemClock};
use mini_swe_agent_rs::environment::Environment;
use mini_swe_agent_rs::environment::local::{LocalEnvironment, LocalEnvironmentConfig};
use mini_swe_agent_rs::error::{AgentError, StepError};
use mini_swe_agent_rs::host_env::HostEnv;
use mini_swe_agent_rs::merge::recursive_merge;
use mini_swe_agent_rs::message::{Action, ExecOutput, Message, Role};
use mini_swe_agent_rs::model::Model;
use mini_swe_agent_rs::model::deterministic::{DeterministicModel, DeterministicModelConfig};
use mini_swe_agent_rs::model::stats::GlobalModelStats;
use serde_json::{Map, Value, json};

use compare::{Side, compare_layers, map_exit_status, normalize_result};

pub fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn scenarios_dir() -> PathBuf {
    crate_dir().join("parity/scenarios")
}

/// Every scenario, sorted by file name. The file stem must equal the scenario id.
pub fn load_scenarios() -> Vec<Value> {
    let mut paths: Vec<PathBuf> = fs::read_dir(scenarios_dir())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    paths.sort();
    paths
        .iter()
        .map(|path| {
            let scenario: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
            let stem = path.file_stem().unwrap().to_str().unwrap();
            assert_eq!(scenario["id"], stem, "{}", path.display());
            scenario
        })
        .collect()
}

pub fn scenario(id: &str) -> Value {
    load_scenarios()
        .into_iter()
        .find(|s| s["id"] == id)
        .unwrap_or_else(|| panic!("no scenario {id}"))
}

/// The Python reference directory: `MSWEA_RS_REFERENCE_DIR`, else `../mini-swe-agent`. Its
/// `uv.lock` is ignored by the reference's own `.gitignore`, so a git worktree may need the
/// variable to point at a checkout that has it. Returns `None` only when the reference is
/// missing and `MSWEA_RS_NO_REFERENCE=1` says to skip on purpose.
pub fn reference_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("MSWEA_RS_REFERENCE_DIR")
        .map_or_else(|| crate_dir().join("../mini-swe-agent"), PathBuf::from);
    if dir.join("uv.lock").is_file() {
        return Some(dir);
    }
    assert_eq!(
        std::env::var("MSWEA_RS_NO_REFERENCE").as_deref(),
        Ok("1"),
        "the Python reference with uv.lock is missing at {}; set MSWEA_RS_REFERENCE_DIR, or \
         MSWEA_RS_NO_REFERENCE=1 to skip the parity suite on purpose",
        dir.display()
    );
    None
}

/// The reference interpreter, resolved once with `uv run --frozen` (with the developer's
/// environment, which uv needs for its cache). Scenarios then run it with the explicit one.
fn python(reference: &Path) -> &'static PathBuf {
    static PYTHON: OnceLock<PathBuf> = OnceLock::new();
    PYTHON.get_or_init(|| {
        let output = Command::new("uv")
            .args(["run", "-q", "--frozen", "--project"])
            .arg(reference)
            .args(["python", "-c", "import sys; print(sys.executable)"])
            .output()
            .expect("uv must be installed to run the parity suite");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        PathBuf::from(String::from_utf8(output.stdout).unwrap().trim())
    })
}

/// One harness: a canonical base directory and the explicit environment.
pub struct Harness {
    _dir: tempfile::TempDir,
    pub base: PathBuf,
    pub env: BTreeMap<String, String>,
    reference: PathBuf,
}

impl Harness {
    /// `None` when the reference is skipped on purpose (see [`reference_dir`]).
    pub fn new() -> Option<Self> {
        let reference = reference_dir()?;
        let dir = tempfile::tempdir().unwrap();
        // Canonical, so `/private/var` and `/var` cannot differ between the two sides.
        let root = dir.path().canonicalize().unwrap();
        for sub in ["home", "tmp", "config", "base"] {
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
        Some(Self {
            base: root.join("base"),
            _dir: dir,
            env,
            reference,
        })
    }

    /// The reference virtualenv's `bin` directory (it holds the `mini` entry point).
    pub fn python_bin_dir(&self) -> PathBuf {
        python(&self.reference).parent().unwrap().to_path_buf()
    }

    /// Runs the given scenarios (all when empty) through the Python reference.
    pub fn run_python(&self, ids: &[&str]) -> Map<String, Value> {
        let out = self.base.join("python-results.json");
        let status = Command::new(python(&self.reference))
            .arg("-I")
            .arg(crate_dir().join("parity/run_python.py"))
            .arg(scenarios_dir())
            .arg(&self.base)
            .arg(&out)
            .args(ids)
            .env_clear()
            .envs(&self.env)
            .current_dir(&self.base)
            .status()
            .unwrap();
        assert!(status.success(), "parity/run_python.py failed: {status}");
        let results: Value = serde_json::from_str(&fs::read_to_string(&out).unwrap()).unwrap();
        fs::remove_file(&out).unwrap();
        match results {
            Value::Object(map) => map,
            other => panic!("unexpected Python results: {other}"),
        }
    }

    /// Runs one scenario through the Rust library, in a fresh workspace at the same path.
    pub fn run_rust(&self, scenario: &Value) -> Value {
        let workspace = fresh_workspace(&self.base, scenario);
        let mut vars = self.env.clone();
        for (key, value) in scenario["host_env"].as_object().into_iter().flatten() {
            vars.insert(key.clone(), value.as_str().unwrap().to_owned());
        }
        let host = Arc::new(HostEnv::from_vars(
            vars.into_iter()
                .map(|(k, v)| (OsString::from(k), OsString::from(v))),
        ));
        let mut result = if scenario["kind"] == "agent" {
            run_agent(scenario, &workspace, host)
        } else {
            run_environment(scenario, &workspace, host)
        };
        result["artifacts"] = snapshot_dir(&workspace);
        fs::remove_dir_all(&workspace).unwrap();
        result
    }
}

fn fresh_workspace(base: &Path, scenario: &Value) -> PathBuf {
    let workspace = base.join(scenario["id"].as_str().unwrap()).join("work");
    if workspace.exists() {
        fs::remove_dir_all(&workspace).unwrap();
    }
    fs::create_dir_all(&workspace).unwrap();
    for (name, text) in scenario["files"].as_object().into_iter().flatten() {
        let target = workspace.join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, text.as_str().unwrap()).unwrap();
    }
    workspace
}

pub fn snapshot_dir(workspace: &Path) -> Value {
    fn walk(root: &Path, dir: &Path, files: &mut BTreeMap<String, Value>) {
        let mut entries: Vec<PathBuf> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        entries.sort();
        for path in entries {
            let meta = fs::symlink_metadata(&path).unwrap();
            if meta.is_dir() {
                walk(root, &path, files);
            } else if meta.is_file() {
                let rel = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned();
                let bytes = fs::read(&path).unwrap();
                let entry = match String::from_utf8(bytes) {
                    Ok(text) => json!({"text": text}),
                    Err(e) => {
                        json!({"hex": e.into_bytes().iter().map(|b| format!("{b:02x}")).collect::<String>()})
                    }
                };
                files.insert(rel, entry);
            }
        }
    }
    let mut files = BTreeMap::new();
    walk(workspace, workspace, &mut files);
    json!(files)
}

fn default_config() -> Value {
    mini_swe_agent_rs::yaml::load_str(
        &fs::read_to_string(crate_dir().join("config/default.yaml")).unwrap(),
    )
    .unwrap()
}

/// Records the history passed to each agent-level query (a test-only wrapper).
pub struct RecordingModel<M> {
    pub inner: M,
    queries: Rc<RefCell<Vec<Value>>>,
}

impl<M: Model> Model for RecordingModel<M> {
    type Config = M::Config;
    fn config(&self) -> &M::Config {
        self.inner.config()
    }
    fn type_name(&self) -> &'static str {
        self.inner.type_name()
    }
    fn query(&mut self, messages: &[Message]) -> Result<Message, StepError> {
        self.queries
            .borrow_mut()
            .push(serde_json::to_value(messages).unwrap());
        self.inner.query(messages)
    }
    fn format_message(
        &self,
        role: Role,
        content: String,
        extra: Option<Map<String, Value>>,
    ) -> Message {
        self.inner.format_message(role, content, extra)
    }
    fn format_observation_messages(
        &self,
        message: &Message,
        outputs: &[ExecOutput],
        template_vars: &Value,
    ) -> Result<Vec<Message>, AgentError> {
        self.inner
            .format_observation_messages(message, outputs, template_vars)
    }
    fn template_vars(&self) -> Result<Value, AgentError> {
        self.inner.template_vars()
    }
    fn serialize(&self) -> Result<Value, AgentError> {
        self.inner.serialize()
    }
}

/// Records each action passed to `execute` (a test-only wrapper).
pub struct RecordingEnvironment<E> {
    inner: E,
    actions: Rc<RefCell<Vec<Value>>>,
}

impl<E: Environment> Environment for RecordingEnvironment<E> {
    type Config = E::Config;
    fn config(&self) -> &E::Config {
        self.inner.config()
    }
    fn type_name(&self) -> &'static str {
        self.inner.type_name()
    }
    fn execute(&self, action: &Action, cwd: Option<&Path>) -> Result<ExecOutput, StepError> {
        self.actions.borrow_mut().push(action.to_value());
        self.inner.execute(action, cwd)
    }
    fn template_vars(&self) -> Result<Value, AgentError> {
        self.inner.template_vars()
    }
    fn serialize(&self) -> Result<Value, AgentError> {
        self.inner.serialize()
    }
}

fn run_agent(scenario: &Value, workspace: &Path, host: Arc<HostEnv>) -> Value {
    let empty = json!({});
    let merged =
        recursive_merge([&default_config(), scenario.get("config").unwrap_or(&empty)]).unwrap();
    let config = Value::Object(merged);
    let stats = Arc::new(GlobalModelStats::from_host_env(&host).unwrap());
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);

    let mut model_config = config["model"].clone();
    model_config["outputs"] = scenario["outputs"].clone();
    let model_config: DeterministicModelConfig = serde_json::from_value(model_config).unwrap();
    let n_outputs = model_config.outputs.len();
    let model = DeterministicModel::new(model_config, stats.clone(), clock.clone()).unwrap();

    let mut env_config = config["environment"].clone();
    env_config["cwd"] = json!(workspace.to_str().unwrap());
    let env = LocalEnvironment::new(
        serde_json::from_value::<LocalEnvironmentConfig>(env_config).unwrap(),
        host,
    );

    let queries = Rc::new(RefCell::new(Vec::new()));
    let actions = Rc::new(RefCell::new(Vec::new()));
    let agent_config: AgentConfig = serde_json::from_value(config["agent"].clone()).unwrap();
    let mut agent = DefaultAgent::new(
        agent_config,
        RecordingModel {
            inner: model,
            queries: queries.clone(),
        },
        RecordingEnvironment {
            inner: env,
            actions: actions.clone(),
        },
        clock,
    )
    .unwrap();
    let (returned, error) = match agent.run(scenario["task"].as_str().unwrap(), Map::new()) {
        // Python's `run` returns the exit message's `extra` dict.
        Ok(info) => (Value::Object(info.extra), Value::Null),
        Err(e) => (Value::Null, json!(e.exit_status())),
    };
    let save_extra: Vec<Value> = scenario["save_extra"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let trajectory = agent.save(None, &save_extra).unwrap();
    let (n_calls, cost) = (agent.n_calls(), agent.cost());
    let (model, _env) = agent.into_parts();
    let remaining =
        i64::try_from(n_outputs).unwrap() - i64::try_from(model.inner.outputs_consumed()).unwrap();
    let queries = queries.borrow().clone();
    let actions = actions.borrow().clone();
    json!({
        "trajectory": trajectory,
        "queries": queries,
        "actions": actions,
        "state": {
            "returned": returned,
            "error": error,
            "n_calls": n_calls,
            "cost": cost,
            "global_cost": stats.cost(),
            "global_calls": stats.n_calls(),
            "remaining_outputs": remaining,
        },
    })
}

fn run_environment(scenario: &Value, workspace: &Path, host: Arc<HostEnv>) -> Value {
    let empty = json!({});
    let config = recursive_merge([
        &json!({"cwd": workspace.to_str().unwrap()}),
        scenario.get("config").unwrap_or(&empty),
    ])
    .unwrap();
    let env = LocalEnvironment::new(serde_json::from_value(Value::Object(config)).unwrap(), host);
    let results: Vec<Value> = scenario["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|command| {
            let action = Action::Command {
                command: command.as_str().unwrap().to_owned(),
            };
            match env.execute(&action, None) {
                Ok(output) => json!({"output": output}),
                Err(StepError::Interrupt(interrupt)) => json!({"submitted": interrupt.head()}),
                Err(StepError::Fatal(e)) => panic!("{}: {e}", scenario["id"]),
            }
        })
        .collect();
    json!({"results": results})
}

/// The exit status a normalized result ended with: the returned status, or the mapped error.
pub fn exit_status(result: &Value) -> Option<String> {
    result
        .pointer("/state/returned/exit_status")
        .or_else(|| result.pointer("/state/error"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

pub fn query_count(result: &Value) -> usize {
    result["queries"].as_array().map_or(0, Vec::len)
}

/// Normalizes both results, compares them layer by layer, and checks the scenario's
/// expectations on both sides. Returns every problem found.
pub fn check(scenario: &Value, mut python: Value, mut rust: Value) -> Result<(), String> {
    let map = scenario["exit_status_map"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    normalize_result(&mut python, Side::Python, &map);
    normalize_result(&mut rust, Side::Rust, &map);
    let mut problems = Vec::new();
    if let Err(diff) = compare_layers(&python, &rust) {
        problems.push(diff);
    }
    if scenario["kind"] == "agent" {
        let expect = &scenario["expect"];
        for (side, result) in [("python", &python), ("rust", &rust)] {
            let status = exit_status(result);
            if status.as_deref() != expect["exit_status"].as_str() {
                problems.push(format!(
                    "{side}: exit status {status:?}, expected {}",
                    expect["exit_status"]
                ));
            }
            let queries = query_count(result);
            if Some(queries as u64) != expect["queries"].as_u64() {
                problems.push(format!(
                    "{side}: {queries} queries, expected {}",
                    expect["queries"]
                ));
            }
            let remaining = result
                .pointer("/state/remaining_outputs")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let exhausted = expect["exit_status"] == "OutputsExhausted";
            let allowed = if exhausted {
                remaining == -1
            } else if scenario["allow_leftover"] == true {
                remaining > 0
            } else {
                remaining == 0
            };
            if !allowed {
                problems.push(format!("{side}: {remaining} scripted outputs left over"));
            }
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("\n"))
    }
}

/// The Python names of an error mapped to Rust (re-exported for steps).
pub fn mapped(name: &str, scenario: &Value) -> String {
    map_exit_status(
        name,
        &scenario["exit_status_map"]
            .as_object()
            .cloned()
            .unwrap_or_default(),
    )
}
