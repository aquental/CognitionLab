//! DefaultAgent (plan §3.4-§3.5, phase P5): the port of `tests/agents/test_default.py` (text
//! model cases and the two format-error fakes) and `tests/run/test_save.py`, plus the boundary,
//! reuse, fatal-path and save cases of plan §3.15 item 1.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use mini_swe_agent_rs::agent::{Agent, AgentConfig, DefaultAgent};
use mini_swe_agent_rs::clock::{Clock, ManualClock, SystemClock};
use mini_swe_agent_rs::environment::Environment;
use mini_swe_agent_rs::environment::local::{LocalEnvironment, LocalEnvironmentConfig};
use mini_swe_agent_rs::error::{AgentError, Interrupt, InterruptKind, ModelError, StepError};
use mini_swe_agent_rs::host_env::HostEnv;
use mini_swe_agent_rs::message::{Action, ExecOutput, Message, Role};
use mini_swe_agent_rs::model::Model;
use mini_swe_agent_rs::model::deterministic::{DeterministicModel, DeterministicModelConfig};
use mini_swe_agent_rs::model::stats::GlobalModelStats;
use serde::Serialize;
use serde_json::{Map, Value, json};

const SUBMIT: &str = "echo 'COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT'";

/// Test helper: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
fn default_agent_config() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("config/default.yaml");
    let config =
        mini_swe_agent_rs::yaml::load_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    config["agent"].clone()
}

/// Test helper: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
fn agent_config(overrides: Value) -> AgentConfig {
    let mut config = default_agent_config();
    for (key, value) in overrides.as_object().unwrap() {
        config[key] = value.clone();
    }
    serde_json::from_value(config).unwrap()
}

/// `make_output` (`test_models.py:16-28`).
fn output(content: &str, commands: &[&str]) -> Value {
    let actions: Vec<Value> = commands.iter().map(|c| json!({"command": c})).collect();
    json!({"role": "assistant", "content": content, "extra": {"actions": actions, "cost": 1.0, "timestamp": 1.0}})
}

fn stats() -> Arc<GlobalModelStats> {
    Arc::new(GlobalModelStats::new(0.0, 0))
}

/// Test helper: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::unwrap_used)]
fn model(
    outputs: Vec<Value>,
    stats: &Arc<GlobalModelStats>,
    clock: &Arc<dyn Clock>,
) -> DeterministicModel {
    let config: DeterministicModelConfig =
        serde_json::from_value(json!({"outputs": outputs})).unwrap();
    DeterministicModel::new(config, stats.clone(), clock.clone()).unwrap()
}

fn local(timeout: u64) -> LocalEnvironment {
    LocalEnvironment::new(
        LocalEnvironmentConfig {
            timeout,
            ..Default::default()
        },
        Arc::new(HostEnv::from_vars(std::env::vars_os())),
    )
}

fn system_clock() -> Arc<dyn Clock> {
    Arc::new(SystemClock)
}

/// Test helper: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::unwrap_used)]
fn agent(
    outputs: Vec<Value>,
    overrides: Value,
) -> DefaultAgent<DeterministicModel, LocalEnvironment> {
    let clock = system_clock();
    DefaultAgent::new(
        agent_config(overrides),
        model(outputs, &stats(), &clock),
        local(30),
        clock,
    )
    .unwrap()
}

fn observation_text(message: &Message) -> &str {
    if message.role == Role::User && message.extra_value("returncode").is_some() {
        &message.content
    } else {
        ""
    }
}

fn submit(text: &str) -> String {
    format!("{SUBMIT}\necho '{text}'")
}

// ---- Port of test_default.py (text model) -------------------------------------------------

#[test]
fn successful_completion() {
    let mut agent = agent(
        vec![
            output("I'll echo a message", &["echo 'hello world'"]),
            output("Now finishing", &[&submit("Task completed successfully")]),
        ],
        json!({}),
    );
    let info = agent
        .run("Echo hello world then finish", Map::new())
        .unwrap();
    assert_eq!(info.exit_status, "Submitted");
    assert_eq!(info.submission, "Task completed successfully\n");
    assert_eq!(agent.n_calls(), 2);
}

#[test]
fn step_limit_enforcement() {
    let mut agent = agent(
        vec![
            output("First", &["echo 'step1'"]),
            output("Second", &["echo 'step2'"]),
        ],
        json!({"step_limit": 1}),
    );
    assert_eq!(
        agent
            .run("Run multiple commands", Map::new())
            .unwrap()
            .exit_status,
        "LimitsExceeded"
    );
    assert_eq!(agent.n_calls(), 1);
}

#[test]
fn cost_limit_enforcement() {
    let mut agent = agent(
        vec![output("Test", &["echo 'test'"])],
        json!({"cost_limit": 0.5}),
    );
    assert_eq!(
        agent
            .run("Test cost limit", Map::new())
            .unwrap()
            .exit_status,
        "LimitsExceeded"
    );
}

#[test]
fn timeout_handling_and_partial_output() {
    let clock = system_clock();
    let outputs = vec![
        output("Output then sleep", &["echo $((111*9)); sleep 10"]),
        output("Quick finish", &[&submit("recovered")]),
    ];
    let mut agent = DefaultAgent::new(
        agent_config(json!({})),
        model(outputs, &stats(), &clock),
        local(1),
        clock,
    )
    .unwrap();
    let info = agent
        .run("Test timeout with partial output", Map::new())
        .unwrap();
    assert_eq!(info.exit_status, "Submitted");
    assert_eq!(info.submission, "recovered\n");
    let timed_out: Vec<&str> = agent
        .messages()
        .iter()
        .map(observation_text)
        .filter(|t| t.contains("timed out"))
        .collect();
    assert_eq!(timed_out.len(), 1);
    assert!(timed_out[0].contains("999"));
}

#[test]
fn multiple_steps_before_completion() {
    let outputs = vec![
        output("Step 1", &["echo 'first'"]),
        output("Step 2", &["echo 'second'"]),
        output("Step 3", &["echo 'third'"]),
        output("Final step", &[&submit("completed all steps")]),
    ];
    let mut agent = agent(outputs, json!({"cost_limit": 5.0}));
    let info = agent.run("Multi-step task", Map::new()).unwrap();
    assert_eq!(
        (info.exit_status.as_str(), info.submission.as_str()),
        ("Submitted", "completed all steps\n")
    );
    assert_eq!(agent.n_calls(), 4);
}

#[test]
fn custom_config() {
    let mut agent = agent(
        vec![output("Test response", &[&submit("custom config works")])],
        json!({"system_template": "You are a test assistant.", "instance_template": "Task: {{task}}. Return bash command.", "step_limit": 2, "cost_limit": 1.0}),
    );
    let info = agent.run("Test custom config", Map::new()).unwrap();
    assert_eq!(info.submission, "custom config works\n");
    assert_eq!(agent.messages()[0].content, "You are a test assistant.");
    assert!(agent.messages()[1].content.contains("Test custom config"));
}

#[test]
fn render_template_model_stats() {
    let mut agent = agent(
        vec![
            output("Test 1", &["echo 'test1'"]),
            output("Test 2", &["echo 'test2'"]),
        ],
        json!({}),
    );
    agent.add_messages([
        Message::new(Role::System, "test"),
        Message::new(Role::User, "test"),
    ]);
    agent.query().unwrap();
    agent.query().unwrap();
    assert_eq!(
        agent
            .render_template("Calls: {{n_model_calls}}, Cost: {{model_cost}}")
            .unwrap(),
        "Calls: 2, Cost: 2.0"
    );
}

#[test]
fn messages_include_timestamps_and_history() {
    let mut agent = agent(
        vec![
            output("Response 1", &["echo 'test1'"]),
            output("Response 2", &[&submit("done")]),
        ],
        json!({}),
    );
    let info = agent.run("Track messages", Map::new()).unwrap();
    assert_eq!(info.submission, "done\n");
    let roles: Vec<Role> = agent.messages().iter().map(|m| m.role).collect();
    assert_eq!(
        roles,
        [
            Role::System,
            Role::User,
            Role::Assistant,
            Role::User,
            Role::Assistant,
            Role::Exit
        ]
    );
    for message in agent
        .messages()
        .iter()
        .filter(|m| m.extra_value("timestamp").is_some())
    {
        assert!(message.extra_value("timestamp").unwrap().is_f64());
    }
    assert!(
        agent.messages()[3].extra_value("timestamp").is_some(),
        "observations carry a timestamp"
    );
    assert_eq!(
        agent.messages()[0].extra,
        None,
        "rendered system message has no extra"
    );
}

#[test]
fn step_adds_messages() {
    let mut agent = agent(vec![output("Test command", &["echo 'hello'"])], json!({}));
    agent.add_messages([
        Message::new(Role::System, "system message"),
        Message::new(Role::User, "user message"),
    ]);
    agent.step().unwrap();
    let messages = agent.messages();
    assert_eq!(messages.len(), 4);
    assert_eq!(messages[2].role, Role::Assistant);
    assert_eq!(messages[2].actions().unwrap()[0].command(), "echo 'hello'");
    assert!(observation_text(&messages[3]).contains("returncode"));
}

#[test]
fn observations_captured() {
    let outputs = vec![
        output("Step 1", &["echo 'first'"]),
        output("Step 2", &["echo 'second'"]),
        output("Final", &[&submit("done")]),
    ];
    let mut agent = agent(outputs, json!({"cost_limit": 5.0}));
    agent.run("Multi-step task", Map::new()).unwrap();
    let observations: Vec<&str> = agent
        .messages()
        .iter()
        .map(observation_text)
        .filter(|t| !t.is_empty())
        .collect();
    assert_eq!(observations.len(), 2, "no observation after Submitted");
    assert!(observations[0].contains("first") && observations[1].contains("second"));
}

/// Wraps an environment and advances a manual clock inside `execute`, like `sleep 2` in Python.
struct ClockAdvancingEnv<E> {
    inner: E,
    clock: Arc<ManualClock>,
    seconds: f64,
}

impl<E: Environment> Environment for ClockAdvancingEnv<E> {
    type Config = E::Config;
    fn config(&self) -> &E::Config {
        self.inner.config()
    }
    fn execute(&self, action: &Action, cwd: Option<&Path>) -> Result<ExecOutput, StepError> {
        self.clock.advance(self.seconds);
        self.inner.execute(action, cwd)
    }
    fn template_vars(&self) -> Result<Value, AgentError> {
        self.inner.template_vars()
    }
}

#[test]
fn wall_time_limit_enforcement() {
    let manual = Arc::new(ManualClock::new(1000.0));
    let clock: Arc<dyn Clock> = manual.clone();
    let env = ClockAdvancingEnv {
        inner: local(30),
        clock: manual,
        seconds: 2.0,
    };
    let outputs = vec![
        output("Slow command", &["true"]),
        output("Should not run", &["echo 'unreachable'"]),
    ];
    let mut agent = DefaultAgent::new(
        agent_config(json!({"wall_time_limit_seconds": 1})),
        model(outputs, &stats(), &clock),
        env,
        clock,
    )
    .unwrap();
    assert_eq!(
        agent
            .run("Test wall time limit", Map::new())
            .unwrap()
            .exit_status,
        "TimeExceeded"
    );
    assert_eq!(agent.n_calls(), 1);
}

#[test]
fn wall_time_boundaries() {
    // Trap 27: an integer limit blocks from elapsed >= limit; the truncation shows only in elapsed_seconds.
    for (elapsed, blocked, rendered) in [(0.999, false, "0"), (1.0, true, "1"), (1.999, true, "1")]
    {
        let manual = Arc::new(ManualClock::new(500.0));
        let clock: Arc<dyn Clock> = manual.clone();
        let mut agent = DefaultAgent::new(
            agent_config(json!({"wall_time_limit_seconds": 1})),
            model(vec![output("x", &["true"])], &stats(), &clock),
            local(30),
            clock,
        )
        .unwrap();
        manual.advance(elapsed);
        assert_eq!(
            agent.render_template("{{ elapsed_seconds }}").unwrap(),
            rendered,
            "{elapsed}"
        );
        let result = agent.query();
        assert_eq!(
            matches!(result, Err(StepError::Interrupt(ref i)) if i.kind() == InterruptKind::TimeExceeded),
            blocked,
            "{elapsed}"
        );
    }
    // A backward clock jump delays the limit.
    let manual = Arc::new(ManualClock::new(500.0));
    let clock: Arc<dyn Clock> = manual.clone();
    let mut agent = DefaultAgent::new(
        agent_config(json!({"wall_time_limit_seconds": 1})),
        model(vec![output("x", &["true"])], &stats(), &clock),
        local(30),
        clock,
    )
    .unwrap();
    manual.set(400.0);
    assert!(agent.query().is_ok());
}

#[test]
fn wall_time_limit_template_vars() {
    let agent = agent(
        vec![output("Test", &["echo 'test'"])],
        json!({"wall_time_limit_seconds": 3600}),
    );
    let vars = agent.template_vars().unwrap();
    assert!(vars["elapsed_seconds"].is_i64());
    assert_eq!(vars["wall_time_limit_seconds"], 3600);
    for key in [
        "n_model_calls",
        "model_cost",
        "system",
        "release",
        "machine",
        "observation_template",
        "step_limit",
    ] {
        assert!(!vars[key].is_null(), "{key}");
    }
}

#[test]
fn empty_actions_handling() {
    let mut agent = agent(
        vec![
            output("No actions here", &[]),
            output("Now with action", &[&submit("done")]),
        ],
        json!({}),
    );
    let info = agent.run("Test empty actions", Map::new()).unwrap();
    assert_eq!(
        (info.exit_status.as_str(), info.submission.as_str()),
        ("Submitted", "done\n")
    );
    assert_eq!(agent.n_calls(), 2);
}

// ---- Ports of the format-error fakes (test_default.py:463-544), on the text path ----------

#[derive(Serialize)]
struct FakeConfig {
    model_name: String,
}

/// `_FlakyToolcallModel`: `None` outputs raise a text-path `FormatError`.
struct FlakyModel {
    config: FakeConfig,
    outputs: Vec<Option<Value>>,
    index: usize,
}

/// Test helper: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::unwrap_used)]
fn format_error(extra: Value) -> StepError {
    let message = Message::new(Role::User, "No tool calls found in the response.")
        .with_extra(extra.as_object().unwrap().clone());
    StepError::Interrupt(Interrupt::new(InterruptKind::FormatError, message, vec![]))
}

/// Test helper: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::unwrap_used)]
impl Model for FlakyModel {
    type Config = FakeConfig;
    fn config(&self) -> &FakeConfig {
        &self.config
    }
    fn query(&mut self, _messages: &[Message]) -> Result<Message, StepError> {
        let next = self
            .outputs
            .get(self.index)
            .cloned()
            .ok_or(ModelError::OutputsExhausted)?;
        self.index += 1;
        match next {
            Some(value) => Ok(serde_json::from_value(value).unwrap()),
            None => Err(format_error(json!({"interrupt_type": "FormatError"}))),
        }
    }
    fn format_observation_messages(
        &self,
        _: &Message,
        outputs: &[ExecOutput],
        _: &Value,
    ) -> Result<Vec<Message>, AgentError> {
        Ok(outputs
            .iter()
            .map(|o| Message::new(Role::User, o.output.clone()))
            .collect())
    }
}

#[test]
fn repeated_format_errors_terminate_cleanly() {
    let model = FlakyModel {
        config: FakeConfig {
            model_name: "flaky".into(),
        },
        outputs: vec![None; 5],
        index: 0,
    };
    let mut agent = DefaultAgent::new(
        agent_config(json!({"max_consecutive_format_errors": 2})),
        model,
        local(30),
        system_clock(),
    )
    .unwrap();
    assert_eq!(
        agent
            .run("Test repeated format errors", Map::new())
            .unwrap()
            .exit_status,
        "RepeatedFormatError"
    );
    assert_eq!(agent.n_calls(), 2);
    let last_two: Vec<&str> = agent.messages()[agent.messages().len() - 2..]
        .iter()
        .map(|m| m.content.as_str())
        .collect();
    assert_eq!(
        last_two,
        [
            "No tool calls found in the response.",
            "RepeatedFormatError"
        ]
    );
}

#[test]
fn format_error_counter_resets_on_success() {
    let outputs = vec![
        None,
        Some(output("listing", &["echo hello"])),
        None,
        Some(output("done", &[&submit("ok")])),
    ];
    let model = FlakyModel {
        config: FakeConfig {
            model_name: "flaky".into(),
        },
        outputs,
        index: 0,
    };
    let mut agent = DefaultAgent::new(
        agent_config(json!({"max_consecutive_format_errors": 2})),
        model,
        local(30),
        system_clock(),
    )
    .unwrap();
    let info = agent.run("Test counter reset", Map::new()).unwrap();
    assert_eq!(
        (info.exit_status.as_str(), info.submission.as_str()),
        ("Submitted", "ok\n")
    );
}

/// `_BilledFormatErrorModel`: charges the global stats, then raises `FormatError` with `extra.cost`.
struct BilledFormatErrorModel {
    config: FakeConfig,
    stats: Arc<GlobalModelStats>,
}

impl Model for BilledFormatErrorModel {
    type Config = FakeConfig;
    fn config(&self) -> &FakeConfig {
        &self.config
    }
    fn query(&mut self, _messages: &[Message]) -> Result<Message, StepError> {
        self.stats.add(1.0)?;
        Err(format_error(
            json!({"interrupt_type": "FormatError", "cost": 1.0}),
        ))
    }
    fn format_observation_messages(
        &self,
        _: &Message,
        _: &[ExecOutput],
        _: &Value,
    ) -> Result<Vec<Message>, AgentError> {
        Ok(vec![])
    }
}

#[test]
fn format_errors_count_against_cost_limit() {
    let stats = stats();
    let model = BilledFormatErrorModel {
        config: FakeConfig {
            model_name: "billed".into(),
        },
        stats: stats.clone(),
    };
    let config = agent_config(
        json!({"cost_limit": 2.5, "step_limit": 8, "max_consecutive_format_errors": 0}),
    );
    let mut agent = DefaultAgent::new(config, model, local(30), system_clock()).unwrap();
    assert_eq!(
        agent
            .run("Test billed format errors", Map::new())
            .unwrap()
            .exit_status,
        "LimitsExceeded"
    );
    assert_eq!(agent.n_calls(), 3);
    assert_eq!(agent.cost(), 3.0);
    assert_eq!(agent.cost(), stats.cost());
}

// ---- Deterministic model details (test_models.py:75-130) ----------------------------------

#[test]
fn scripted_raise_and_sleep_and_warning() {
    let stats = stats();
    let manual = Arc::new(ManualClock::new(10.0));
    let clock: Arc<dyn Clock> = manual.clone();
    let outputs = vec![
        output("sleeping", &["/sleep 3"]),
        output("warning", &["/warning careful"]),
        json!({"role": "assistant", "content": "bad", "extra": {"actions": [{"raise": {"kind": "FormatError", "messages": [{"role": "user", "content": "scripted format error", "extra": {"cost": 0.5}}]}}]}}),
        output("done", &[&submit("fin")]),
    ];
    let mut agent = DefaultAgent::new(
        agent_config(json!({})),
        model(outputs, &stats, &clock),
        local(30),
        clock,
    )
    .unwrap();
    let info = agent.run("scripted", Map::new()).unwrap();
    assert_eq!(info.submission, "fin\n");
    assert_eq!(manual.now(), 13.0, "/sleep advances the injected clock");
    assert_eq!(
        stats.n_calls(),
        1,
        "/sleep, /warning and raise are not charged"
    );
    assert_eq!(agent.n_calls(), 2);
    assert_eq!(agent.cost(), 1.5, "format-error cost + one output");
    assert!(
        agent
            .messages()
            .iter()
            .any(|m| m.content == "scripted format error")
    );
}

#[test]
fn outputs_exhausted_is_fatal_and_saved() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested/traj.json");
    let mut agent = agent(
        vec![output("one", &["echo hi"])],
        json!({"output_path": path}),
    );
    let error = agent.run("exhaust", Map::new()).unwrap_err();
    assert!(matches!(
        error,
        AgentError::Model(ModelError::OutputsExhausted)
    ));
    let last = agent.messages().last().unwrap();
    assert_eq!(last.role, Role::Exit);
    assert_eq!(last.extra_value("exit_status").unwrap(), "OutputsExhausted");
    assert_eq!(last.extra_value("submission").unwrap(), "");
    assert_eq!(
        last.extra_value("exception_str").unwrap().as_str().unwrap(),
        last.content
    );
    assert!(last.extra_value("traceback").is_some());
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(saved["info"]["exit_status"], "OutputsExhausted");
    assert_eq!(
        saved["messages"].as_array().unwrap().len(),
        agent.messages().len()
    );
}

#[test]
fn global_call_limit_fails_on_the_first_response() {
    // Trap 28: with MSWEA_GLOBAL_CALL_LIMIT=1, `1 < 1 + 1` fails the first call.
    let clock = system_clock();
    let stats = Arc::new(GlobalModelStats::new(0.0, 1));
    let mut agent = DefaultAgent::new(
        agent_config(json!({})),
        model(vec![output("x", &["true"])], &stats, &clock),
        local(30),
        clock,
    )
    .unwrap();
    let error = agent.run("global", Map::new()).unwrap_err();
    assert!(matches!(
        error,
        AgentError::Model(ModelError::GlobalLimitExceeded { calls: 1, .. })
    ));
    assert_eq!(agent.n_calls(), 1);
    assert_eq!(
        agent.cost(),
        0.0,
        "the rejected response is not charged to the agent"
    );
    assert!(agent.messages().iter().all(|m| m.role != Role::Assistant));
    assert_eq!(
        agent.messages().last().unwrap().content,
        "Global cost/call limit exceeded: $1.0000 / 1"
    );
}

#[test]
fn global_stats_boundaries() {
    let at_limit = GlobalModelStats::new(2.0, 0);
    assert!(at_limit.add(1.0).is_ok());
    assert!(
        at_limit.add(1.0).is_ok(),
        "cost equal to the limit does not trip (strict <)"
    );
    assert!(at_limit.add(0.01).is_err());
    let calls = GlobalModelStats::new(0.0, 2);
    assert!(calls.add(0.0).is_ok());
    assert!(
        calls.add(0.0).is_err(),
        "the second call trips a limit of 2"
    );
    let host = HostEnv::from_vars([
        ("MSWEA_GLOBAL_COST_LIMIT".into(), "1.5".into()),
        ("MSWEA_GLOBAL_CALL_LIMIT".into(), "4".into()),
    ]);
    let parsed = GlobalModelStats::from_host_env(&host).unwrap();
    assert_eq!((parsed.cost_limit(), parsed.call_limit()), (1.5, 4));
    assert!(
        GlobalModelStats::from_host_env(&HostEnv::from_vars([(
            "MSWEA_GLOBAL_CALL_LIMIT".into(),
            "x".into()
        )]))
        .is_err()
    );
}

#[test]
fn malformed_actions_fail_inside_the_deterministic_model() {
    // Python's `_process_test_actions` iterates `extra.actions` before charging, so a string
    // fails in the model (AttributeError) and nothing is appended or charged.
    let mut agent = agent(
        vec![
            json!({"role": "assistant", "content": "bad", "extra": {"actions": "ls", "cost": 1.0}}),
        ],
        json!({}),
    );
    let error = agent.run("bad actions", Map::new()).unwrap_err();
    assert!(matches!(error, AgentError::Action(_)), "{error:?}");
    assert_eq!((agent.n_calls(), agent.cost()), (1, 0.0));
    let roles: Vec<Role> = agent.messages().iter().map(|m| m.role).collect();
    assert_eq!(roles, [Role::System, Role::User, Role::Exit]);
}

#[test]
fn malformed_actions_fail_after_the_assistant_is_saved() {
    // A model that returns the message: query() appends and charges it, then step() decodes
    // the actions (default.py:130-157), so the assistant message stays in the history.
    let bad =
        json!({"role": "assistant", "content": "bad", "extra": {"actions": "ls", "cost": 1.0}});
    let model = FlakyModel {
        config: FakeConfig {
            model_name: "flaky".into(),
        },
        outputs: vec![Some(bad)],
        index: 0,
    };
    let mut agent =
        DefaultAgent::new(agent_config(json!({})), model, local(30), system_clock()).unwrap();
    let error = agent.run("bad actions", Map::new()).unwrap_err();
    assert!(matches!(error, AgentError::Action(_)), "{error:?}");
    assert_eq!(agent.cost(), 1.0);
    let roles: Vec<Role> = agent.messages().iter().map(|m| m.role).collect();
    assert_eq!(
        roles,
        [Role::System, Role::User, Role::Assistant, Role::Exit]
    );
}

#[test]
fn submitted_aborts_the_remaining_actions() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("ran");
    let mut agent = agent(
        vec![output(
            "two",
            &[&submit("x"), &format!("touch {}", marker.display())],
        )],
        json!({}),
    );
    assert_eq!(
        agent.run("abort", Map::new()).unwrap().exit_status,
        "Submitted"
    );
    assert!(!marker.exists());
}

// ---- Reuse, render failure, save --------------------------------------------------------------

#[test]
fn reuse_keeps_counters_and_format_error_state() {
    let model = FlakyModel {
        config: FakeConfig {
            model_name: "flaky".into(),
        },
        outputs: vec![
            None,
            None,
            None,
            Some(output("ok", &["echo a"])),
            None,
            Some(output("done", &[&submit("y")])),
        ],
        index: 0,
    };
    let mut agent = DefaultAgent::new(
        agent_config(json!({"max_consecutive_format_errors": 2})),
        model,
        local(30),
        system_clock(),
    )
    .unwrap();
    let mut first_vars = Map::new();
    first_vars.insert("old".into(), json!("kept"));
    assert_eq!(
        agent.run("first", first_vars).unwrap().exit_status,
        "RepeatedFormatError"
    );
    // Trap 19: the counter is still at the limit, so the next format error exits at once.
    assert_eq!(
        agent.run("second", Map::new()).unwrap().exit_status,
        "RepeatedFormatError"
    );
    assert_eq!(agent.n_calls(), 3);
    assert_eq!(
        agent.render_template("{{ old }} {{ task }}").unwrap(),
        "kept second"
    );
    // A clean step clears it: ok, then one error, then submit.
    assert_eq!(
        agent.run("third", Map::new()).unwrap().exit_status,
        "Submitted"
    );
    assert_eq!(
        agent.messages().len(),
        2 + 2 + 1 + 2,
        "history reset per run"
    );
}

#[test]
fn render_failure_before_the_loop_is_not_saved() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("traj.json");
    let mut agent = agent(
        vec![output("x", &["true"])],
        json!({"instance_template": "{{ undefined_variable }}", "output_path": path}),
    );
    assert!(matches!(
        agent.run("t", Map::new()),
        Err(AgentError::Template(_))
    ));
    assert!(
        agent.messages().is_empty(),
        "both initial messages are built before either is appended"
    );
    assert!(!path.exists());
}

#[test]
fn save_failure_replaces_the_result() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let path = file.path().join("traj.json"); // a path under a regular file
    let mut agent = agent(
        vec![output("done", &[&submit("x")])],
        json!({"output_path": path}),
    );
    assert!(matches!(
        agent.run("t", Map::new()),
        Err(AgentError::Io { .. })
    ));
}

#[test]
fn save_includes_types_and_extra_dicts() {
    // Port of tests/run/test_save.py.
    let mut agent = agent(vec![output("echo 'test'", &[])], json!({}));
    agent.add_messages([
        Message::new(Role::System, "test system message"),
        Message::new(Role::User, "test user message"),
    ]);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test_trajectory.json");
    agent
        .save(
            Some(&path),
            &[json!({"info": {"exit_status": "Submitted", "submission": "test result"}})],
        )
        .unwrap();
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let config = &saved["info"]["config"];
    assert!(
        config["agent_type"]
            .as_str()
            .unwrap()
            .contains("DefaultAgent")
    );
    assert!(
        config["model_type"]
            .as_str()
            .unwrap()
            .ends_with("DeterministicModel")
    );
    assert!(
        config["environment_type"]
            .as_str()
            .unwrap()
            .ends_with("LocalEnvironment")
    );
    assert_eq!(saved["info"]["exit_status"], "Submitted");
    assert_eq!(saved["info"]["submission"], "test result");
    assert_eq!(saved["trajectory_format"], "mini-swe-agent-1.1");
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .starts_with("{\n  \""),
        "pretty JSON with 2-space indent"
    );
    let leftovers: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
    assert_eq!(leftovers.len(), 1, "no temporary file left behind");
}

#[test]
fn serialize_structure() {
    let mut agent = agent(vec![output("x", &[&submit("s")])], json!({"step_limit": 7}));
    agent.run("serialize", Map::new()).unwrap();
    let data = agent.save(None, &[]).unwrap();
    assert_eq!(
        data["info"]["model_stats"],
        json!({"instance_cost": 1.0, "api_calls": 1})
    );
    assert_eq!(data["info"]["config"]["agent"]["step_limit"], 7);
    assert_eq!(data["info"]["config"]["agent"]["output_path"], Value::Null);
    assert_eq!(
        data["info"]["config"]["model"]["model_name"],
        "deterministic"
    );
    assert_eq!(data["info"]["config"]["environment"]["timeout"], 30);
    assert_eq!(data["info"]["exit_status"], "Submitted");
    assert_eq!(data["info"]["submission"], "s\n");
    assert_eq!(data["messages"].as_array().unwrap().len(), 4);
    assert!(data["info"]["mini_version"].is_string());
}

#[test]
fn unknown_agent_keys_are_ignored() {
    let config: AgentConfig = serde_json::from_value(
        json!({"system_template": "s", "instance_template": "i", "mode": "confirm"}),
    )
    .unwrap();
    assert_eq!(config.cost_limit, 3.0);
    assert_eq!(config.max_consecutive_format_errors, 3);
    assert!(
        serde_json::from_value::<AgentConfig>(json!({"system_template": "s"})).is_err(),
        "instance_template is required"
    );
}

// ---- P1 review fixes: in-order actions and invalid costs ----------------------------------

#[test]
fn scripted_raise_wins_over_a_later_malformed_action() {
    // Python's _process_test_actions raises at the first `raise` before it sees entry 1.
    let outputs = vec![
        json!({"role": "assistant", "content": "x", "extra": {"actions": [
            {"raise": {"kind": "FormatError", "messages": [{"role": "user", "content": "scripted"}]}},
            5
        ]}}),
        output("done", &[&submit("ok")]),
    ];
    let mut agent = agent(outputs, json!({}));
    assert_eq!(
        agent.run("order", Map::new()).unwrap().exit_status,
        "Submitted"
    );
    assert!(agent.messages().iter().any(|m| m.content == "scripted"));
}

#[test]
fn submitted_wins_over_a_later_malformed_action() {
    // DefaultAgent runs actions in order; Submitted aborts before entry 1 is decoded.
    let reply = json!({"role": "assistant", "content": "x", "extra": {"actions": [{"command": submit("s")}, 5]}});
    let model = FlakyModel {
        config: FakeConfig {
            model_name: "flaky".into(),
        },
        outputs: vec![Some(reply)],
        index: 0,
    };
    let mut agent =
        DefaultAgent::new(agent_config(json!({})), model, local(30), system_clock()).unwrap();
    let info = agent.run("order", Map::new()).unwrap();
    assert_eq!(
        (info.exit_status.as_str(), info.submission.as_str()),
        ("Submitted", "s\n")
    );
}

#[test]
fn invalid_cost_is_fatal_before_the_message_is_appended() {
    // Python: `self.cost += "x"` raises TypeError before add_messages.
    let mut agent = agent(
        vec![json!({"role": "assistant", "content": "x", "extra": {"actions": [], "cost": "x"}})],
        json!({}),
    );
    let error = agent.run("cost", Map::new()).unwrap_err();
    assert!(matches!(error, AgentError::Cost(_)), "{error:?}");
    let roles: Vec<Role> = agent.messages().iter().map(|m| m.role).collect();
    assert_eq!(roles, [Role::System, Role::User, Role::Exit]);
    assert_eq!(
        agent.messages()[2].extra_value("exit_status").unwrap(),
        "InvalidCost"
    );
}

/// Raises FormatError whose `extra.cost` is not a number.
struct BadCostFormatErrorModel {
    config: FakeConfig,
}

impl Model for BadCostFormatErrorModel {
    type Config = FakeConfig;
    fn config(&self) -> &FakeConfig {
        &self.config
    }
    fn query(&mut self, _messages: &[Message]) -> Result<Message, StepError> {
        Err(format_error(
            json!({"interrupt_type": "FormatError", "cost": null}),
        ))
    }
    fn format_observation_messages(
        &self,
        _: &Message,
        _: &[ExecOutput],
        _: &Value,
    ) -> Result<Vec<Message>, AgentError> {
        Ok(vec![])
    }
}

#[test]
fn invalid_format_error_cost_propagates_without_an_exit_message() {
    // Python raises TypeError inside `except FormatError`, which the sibling `except Exception`
    // does not catch: no exit message, but `finally` still saves.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("traj.json");
    let model = BadCostFormatErrorModel {
        config: FakeConfig {
            model_name: "bad".into(),
        },
    };
    let mut agent = DefaultAgent::new(
        agent_config(json!({"output_path": path})),
        model,
        local(30),
        system_clock(),
    )
    .unwrap();
    assert!(matches!(
        agent.run("cost", Map::new()),
        Err(AgentError::Cost(_))
    ));
    let roles: Vec<Role> = agent.messages().iter().map(|m| m.role).collect();
    assert_eq!(roles, [Role::System, Role::User]);
    assert!(path.exists());
}

// ---- Review fixes (P5) -------------------------------------------------------------------------

#[test]
fn run_returns_the_whole_exit_extra() {
    // Python returns `self.messages[-1].get("extra", {})`, with any extra keys.
    let exit = json!({"role": "exit", "content": "done", "extra": {"exit_status": "Submitted", "submission": "done", "artifact": "patch.diff"}});
    let raise = json!({"role": "assistant", "content": "x", "extra": {"actions": [{"raise": {"kind": "Submitted", "messages": [exit]}}]}});
    let mut agent = agent(vec![raise], json!({}));
    let info = agent.run("t", Map::new()).unwrap();
    assert_eq!(info.exit_status, "Submitted");
    assert_eq!(info.extra["artifact"], "patch.diff");
    let last = agent.messages().last().unwrap();
    assert_eq!(Value::Object(info.extra), json!(last.extra));
}

#[test]
fn trajectory_is_saved_after_every_step() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("traj.json");
    let cat = format!("cat '{}'", path.display());
    let mut agent = agent(
        vec![
            output("one", &["echo first"]),
            output("two", &[&cat]),
            output("done", &[&submit("x")]),
        ],
        json!({"output_path": path}),
    );
    agent.run("t", Map::new()).unwrap();
    // Step 2 reads the file that step 1 saved.
    let observed = observation_text(&agent.messages()[5]);
    assert!(observed.contains("echo first"), "{observed}");
}

/// Replaces the output directory with a file, then fails: the save in `finally` must fail too.
struct BreakingEnv {
    inner: LocalEnvironment,
    dir: PathBuf,
}

impl Environment for BreakingEnv {
    type Config = LocalEnvironmentConfig;
    fn config(&self) -> &LocalEnvironmentConfig {
        self.inner.config()
    }
    #[allow(clippy::unwrap_used)]
    fn execute(&self, _action: &Action, _cwd: Option<&Path>) -> Result<ExecOutput, StepError> {
        std::fs::remove_dir_all(&self.dir).unwrap();
        std::fs::write(&self.dir, "not a directory").unwrap();
        Err(AgentError::Internal("boom".to_owned()).into())
    }
    fn template_vars(&self) -> Result<Value, AgentError> {
        self.inner.template_vars()
    }
}

#[test]
fn save_failure_replaces_a_fatal_error() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("out");
    std::fs::create_dir(&dir).unwrap();
    let clock = system_clock();
    let env = BreakingEnv {
        inner: local(30),
        dir: dir.clone(),
    };
    let mut agent = DefaultAgent::new(
        agent_config(json!({"output_path": dir.join("traj.json")})),
        model(vec![output("one", &["echo hi"])], &stats(), &clock),
        env,
        clock,
    )
    .unwrap();
    let error = agent.run("t", Map::new()).unwrap_err();
    assert!(matches!(error, AgentError::Io { .. }), "{error:?}");
    assert_eq!(agent.messages().last().unwrap().role, Role::Exit);
}

#[test]
fn scripted_fatal_raise_is_the_exit_status() {
    let raise = json!({"role": "assistant", "content": "x", "extra": {"actions": [{"raise": {"kind": "RuntimeError", "message": "boom"}}]}});
    let mut agent = agent(vec![raise], json!({}));
    let error = agent.run("t", Map::new()).unwrap_err();
    assert!(
        matches!(&error, AgentError::Model(ModelError::Injected { kind, .. }) if kind == "RuntimeError")
    );
    assert_eq!(
        agent
            .messages()
            .last()
            .unwrap()
            .extra_value("exit_status")
            .unwrap(),
        "RuntimeError"
    );
}

/// Runs `/sleep <text>` then a submission on a manual clock; the error kind, or `None`.
/// Test helper: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::unwrap_used, clippy::panic)]
fn sleep_result(text: &str) -> (Option<String>, f64) {
    let manual = Arc::new(ManualClock::new(0.0));
    let clock: Arc<dyn Clock> = manual.clone();
    let sleep = json!({"role": "assistant", "content": "s", "extra": {"actions": [{"command": format!("/sleep {text}")}]}});
    let mut agent = DefaultAgent::new(
        agent_config(json!({})),
        model(
            vec![sleep, output("done", &[&submit("x")])],
            &stats(),
            &clock,
        ),
        local(30),
        clock,
    )
    .unwrap();
    let kind = match agent.run("t", Map::new()) {
        Ok(_) => None,
        Err(AgentError::Model(ModelError::Injected { kind, .. })) => Some(kind),
        Err(e) => panic!("{text}: {e}"),
    };
    (kind, manual.now())
}

#[test]
fn scripted_sleep_follows_python_float_and_time_sleep() {
    for (text, seconds) in [
        ("0.0_1", 0.01),
        ("1_0", 10.0),
        (" 2 ", 2.0),
        ("1e1", 10.0),
        ("0", 0.0),
    ] {
        let (kind, now) = sleep_result(text);
        assert_eq!(kind, None, "{text}");
        assert!((now - seconds).abs() < 1e-9, "{text}: {now}");
    }
    for (text, kind) in [
        ("abc", "ValueError"),
        ("1__0", "ValueError"),
        ("_1", "ValueError"),
        ("0x10", "ValueError"),
        ("-1", "ValueError"),
        ("nan", "ValueError"),
        ("inf", "OverflowError"),
        ("1e309", "OverflowError"),
    ] {
        assert_eq!(sleep_result(text).0.as_deref(), Some(kind), "{text}");
    }
}

#[test]
fn cost_equal_to_the_limit_stops_the_run() {
    let mut agent = agent(
        vec![output("one", &["echo 1"]), output("two", &["echo 2"])],
        json!({"cost_limit": 1.0}),
    );
    assert_eq!(
        agent.run("t", Map::new()).unwrap().exit_status,
        "LimitsExceeded"
    );
    assert_eq!(agent.n_calls(), 1);
}

#[test]
fn an_empty_output_path_is_an_error() {
    // Python's `Path("")` is `.`, and writing it fails with IsADirectoryError.
    let mut agent = agent(
        vec![output("done", &[&submit("x")])],
        json!({"output_path": ""}),
    );
    assert!(matches!(
        agent.run("t", Map::new()),
        Err(AgentError::Io { .. })
    ));
}

#[test]
fn fatal_traceback_is_the_error_source_chain() {
    // Plan §2.4: `extra.traceback` is the error's `source()` chain, not a Python traceback.
    let clock = system_clock();
    let config: DeterministicModelConfig = serde_json::from_value(json!({
        "outputs": [output("x", &["echo hi"])],
        "observation_template": "{{ no_such_variable }}"
    }))
    .unwrap();
    let model = DeterministicModel::new(config, stats(), clock.clone()).unwrap();
    let mut agent = DefaultAgent::new(agent_config(json!({})), model, local(30), clock).unwrap();
    let error = agent.run("t", Map::new()).unwrap_err();
    assert!(matches!(error, AgentError::Template(_)), "{error}");
    let last = agent.messages().last().unwrap();
    let traceback = last.extra_value("traceback").unwrap().as_str().unwrap();
    let exception = last.extra_value("exception_str").unwrap().as_str().unwrap();
    assert!(traceback.starts_with(exception), "{traceback}");
    assert!(traceback.contains("\ncaused by: "), "{traceback}");
}

#[test]
fn a_multimodal_regex_is_refused() {
    // Multimodal content is out of scope (plan §2.3); a non-empty pattern would be ignored.
    let config: DeterministicModelConfig =
        serde_json::from_value(json!({"outputs": [], "multimodal_regex": "<img>"})).unwrap();
    let built = DeterministicModel::new(config, stats(), system_clock());
    assert!(matches!(built, Err(AgentError::Config(_))), "{built:?}");
}
