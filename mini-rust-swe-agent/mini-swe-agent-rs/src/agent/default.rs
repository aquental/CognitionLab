//! `DefaultAgent` (port of `agents/default.py`, plan §3.4-§3.5): a linear message history and a
//! simple loop. Interrupts carry messages into the history; fatal errors add one `exit` message
//! and are returned. The trajectory is saved after every step.

use std::error::Error as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::agent::Agent;
use crate::clock::Clock;
use crate::environment::Environment;
use crate::error::{AgentError, Interrupt, InterruptKind, StepError};
use crate::merge::recursive_merge;
use crate::message::{ExitInfo, Message, Role};
use crate::model::Model;
use crate::template::Templates;

/// `info.mini_version`: the crate version (plan §2.4).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// `info.config.agent_type`.
const AGENT_TYPE: &str = concat!(module_path!(), "::DefaultAgent");

const TRAJECTORY_FORMAT: &str = "mini-swe-agent-1.1";
const SYSTEM: &str = "system_template";
const INSTANCE: &str = "instance_template";

fn default_cost_limit() -> f64 {
    3.0
}

fn default_max_consecutive_format_errors() -> i64 {
    3
}

/// `AgentConfig` (`default.py:18-34`). Unknown keys are ignored, as pydantic does.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Template of the system message.
    pub system_template: String,
    /// Template of the first user message (the task).
    pub instance_template: String,
    /// Maximum number of model calls (`0` = none).
    #[serde(default)]
    pub step_limit: i64,
    /// Stop once the cost reaches this value (`0` = none).
    #[serde(default = "default_cost_limit")]
    pub cost_limit: f64,
    /// Wall-clock limit in whole seconds (`0` = none).
    #[serde(default)]
    pub wall_time_limit_seconds: i64,
    /// Exit after this many format errors in a row (`0` = no limit).
    #[serde(default = "default_max_consecutive_format_errors")]
    pub max_consecutive_format_errors: i64,
    /// Where to save the trajectory after every step.
    #[serde(default)]
    pub output_path: Option<PathBuf>,
}

/// The default agent over a model `M` and an environment `E`.
#[derive(Debug)]
pub struct DefaultAgent<M, E> {
    config: AgentConfig,
    templates: Templates,
    messages: Vec<Message>,
    model: M,
    env: E,
    clock: Arc<dyn Clock>,
    extra_template_vars: Map<String, Value>,
    cost: f64,
    n_calls: u64,
    n_consecutive_format_errors: i64,
    start_time: f64,
}

impl<M: Model, E: Environment> DefaultAgent<M, E> {
    /// Builds the agent and compiles its two templates. The wall-time limit counts from here,
    /// as `_start_time` does.
    pub fn new(
        config: AgentConfig,
        model: M,
        env: E,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, AgentError> {
        let mut templates = Templates::new();
        templates.add(SYSTEM, &config.system_template)?;
        templates.add(INSTANCE, &config.instance_template)?;
        let start_time = clock.now();
        Ok(Self {
            config,
            templates,
            messages: Vec::new(),
            model,
            env,
            clock,
            extra_template_vars: Map::new(),
            cost: 0.0,
            n_calls: 0,
            n_consecutive_format_errors: 0,
            start_time,
        })
    }

    /// The config.
    pub fn config(&self) -> &AgentConfig {
        &self.config
    }

    /// The history.
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// The agent's cost.
    pub fn cost(&self) -> f64 {
        self.cost
    }

    /// The number of model calls.
    pub fn n_calls(&self) -> u64 {
        self.n_calls
    }

    /// Gives the model and the environment back.
    pub fn into_parts(self) -> (M, E) {
        (self.model, self.env)
    }

    /// Appends messages and returns the appended slice.
    pub fn add_messages(&mut self, messages: impl IntoIterator<Item = Message>) -> &[Message] {
        let start = self.messages.len();
        self.messages.extend(messages);
        self.messages.get(start..).unwrap_or_default()
    }

    /// `int(time.time() - self._start_time)`: truncated toward zero.
    fn elapsed_seconds(&self) -> i64 {
        // `as` saturates and maps NaN to 0, which is the intent here.
        #[allow(clippy::cast_possible_truncation)]
        let elapsed = (self.clock.now() - self.start_time).trunc() as i64;
        elapsed
    }

    /// `get_template_vars` (`default.py:51-64`): agent config, environment, model, counters,
    /// extra vars; later layers win.
    pub fn template_vars(&self) -> Result<Value, AgentError> {
        let counters = json!({
            "n_model_calls": self.n_calls,
            "model_cost": self.cost,
            "elapsed_seconds": self.elapsed_seconds(),
        });
        let layers = [
            serde_json::to_value(&self.config)?,
            self.env.template_vars()?,
            self.model.template_vars()?,
            counters,
            Value::Object(self.extra_template_vars.clone()),
        ];
        Ok(Value::Object(recursive_merge(&layers)?))
    }

    /// Renders a template string with the current variables (`_render_template`).
    pub fn render_template(&self, template: &str) -> Result<String, AgentError> {
        Ok(crate::template::render_once(
            template,
            &self.template_vars()?,
        )?)
    }

    fn render(&self, name: &str) -> Result<String, AgentError> {
        Ok(self.templates.render(name, &self.template_vars()?)?)
    }

    /// One step: query, then execute the actions of the new assistant message.
    pub fn step(&mut self) -> Result<Vec<Message>, StepError> {
        self.query()?;
        self.execute_actions()
    }

    /// Checks the limits, calls the model, charges the cost and appends the assistant message
    /// (`default.py:130-152`).
    pub fn query(&mut self) -> Result<(), StepError> {
        let over_steps = 0 < self.config.step_limit
            && u64::try_from(self.config.step_limit).is_ok_and(|limit| limit <= self.n_calls);
        let over_cost = 0.0 < self.config.cost_limit && self.config.cost_limit <= self.cost;
        if over_steps || over_cost {
            return Err(Interrupt::limits_exceeded().into());
        }
        let limit = self.config.wall_time_limit_seconds;
        if 0 < limit && limit <= self.elapsed_seconds() {
            return Err(Interrupt::time_exceeded().into());
        }
        self.n_calls = self.n_calls.saturating_add(1);
        let message = self.model.query(&self.messages)?;
        self.cost += message.cost()?;
        self.messages.push(message);
        Ok(())
    }

    /// Runs the actions of the last message and appends the observations
    /// (`default.py:154-157`). A `Submitted` interrupt stops the remaining actions.
    pub fn execute_actions(&mut self) -> Result<Vec<Message>, StepError> {
        let Some(message) = self.messages.last() else {
            return Err(
                AgentError::Internal("execute_actions without a message".to_owned()).into(),
            );
        };
        // One action at a time, as Python iterates them: a Submitted interrupt ends the step
        // before a later entry is even decoded.
        let mut outputs = Vec::new();
        for action in message.action_iter() {
            outputs.push(self.env.execute(&action?, None)?);
        }
        let template_vars = self.template_vars()?;
        let observations =
            self.model
                .format_observation_messages(message, &outputs, &template_vars)?;
        Ok(self.add_messages(observations).to_vec())
    }

    /// `handle_uncaught_exception` (`default.py:74-86`).
    fn add_fatal_exit(&mut self, error: &AgentError) {
        let text = error.to_string();
        let mut extra = Map::new();
        extra.insert("exit_status".to_owned(), json!(error.exit_status()));
        extra.insert("submission".to_owned(), json!(""));
        extra.insert("exception_str".to_owned(), json!(text));
        extra.insert("traceback".to_owned(), json!(source_chain(error)));
        let message = self.model.format_message(Role::Exit, text, Some(extra));
        self.messages.push(message);
    }

    /// A `FormatError`: charge its cost, count it, append its messages, and exit with
    /// `RepeatedFormatError` once the limit is reached (`default.py:101-116`). An invalid cost
    /// fails before anything is appended, as Python's `+=` raises inside the `except` block.
    fn handle_format_error(&mut self, interrupt: Interrupt) -> Result<(), AgentError> {
        self.cost += interrupt.head().cost()?;
        self.n_consecutive_format_errors = self.n_consecutive_format_errors.saturating_add(1);
        let limit = self.config.max_consecutive_format_errors;
        self.messages.extend(interrupt.into_messages());
        if 0 < limit && limit <= self.n_consecutive_format_errors {
            self.messages.push(Message::exit(
                "RepeatedFormatError",
                "RepeatedFormatError",
                "",
            ));
        }
        Ok(())
    }

    /// `serialize(*extra_dicts)` (`default.py:159-180`).
    pub fn serialize(&self, extra: &[Value]) -> Result<Value, AgentError> {
        let last_extra = |key: &str| {
            self.messages
                .last()
                .and_then(|m| m.extra_value(key))
                .cloned()
                .unwrap_or_else(|| json!(""))
        };
        let agent_data = json!({
            "info": {
                "model_stats": {"instance_cost": self.cost, "api_calls": self.n_calls},
                "config": {"agent": serde_json::to_value(&self.config)?, "agent_type": AGENT_TYPE},
                "mini_version": VERSION,
                "exit_status": last_extra("exit_status"),
                "submission": last_extra("submission"),
            },
            "messages": serde_json::to_value(&self.messages)?,
            "trajectory_format": TRAJECTORY_FORMAT,
        });
        let mut layers = vec![agent_data, self.model.serialize()?, self.env.serialize()?];
        layers.extend(extra.iter().cloned());
        Ok(Value::Object(recursive_merge(&layers)?))
    }

    fn save_configured(&self) -> Result<(), AgentError> {
        self.save(self.config.output_path.as_deref(), &[])
            .map(|_| ())
    }
}

impl<M: Model, E: Environment> Agent for DefaultAgent<M, E> {
    /// The loop of `default.py:88-128`.
    fn run(&mut self, task: &str, extra_vars: Map<String, Value>) -> Result<ExitInfo, AgentError> {
        self.extra_template_vars
            .insert("task".to_owned(), json!(task));
        self.extra_template_vars.extend(extra_vars);
        self.messages.clear();
        // Both messages are built before either is appended.
        let system = self
            .model
            .format_message(Role::System, self.render(SYSTEM)?, None);
        let instance = self
            .model
            .format_message(Role::User, self.render(INSTANCE)?, None);
        self.messages.extend([system, instance]);
        loop {
            match self.step() {
                Ok(_) => self.n_consecutive_format_errors = 0,
                Err(StepError::Interrupt(interrupt))
                    if interrupt.kind() == InterruptKind::FormatError =>
                {
                    if let Err(error) = self.handle_format_error(interrupt) {
                        // Raised inside `except FormatError`: no exit message, but `finally` saves.
                        self.save_configured()?;
                        return Err(error);
                    }
                }
                Err(StepError::Interrupt(interrupt)) => {
                    self.messages.extend(interrupt.into_messages())
                }
                Err(StepError::Fatal(error)) => {
                    self.add_fatal_exit(&error);
                    // Python's `finally` saves; a failing save replaces the original error.
                    self.save_configured()?;
                    return Err(error);
                }
            }
            self.save_configured()?;
            if self.messages.last().is_some_and(|m| m.role == Role::Exit) {
                break;
            }
        }
        Ok(self
            .messages
            .last()
            .and_then(Message::exit_info)
            .unwrap_or_default())
    }

    /// `save(path, *extra_dicts)`: pretty JSON with a 2-space indent, written atomically
    /// through a temporary file in the destination directory.
    fn save(&self, path: Option<&Path>, extra: &[Value]) -> Result<Value, AgentError> {
        let data = self.serialize(extra)?;
        if let Some(path) = path.filter(|p| !p.as_os_str().is_empty()) {
            write_atomic(path, &data)?;
        }
        Ok(data)
    }
}

/// The error and its `source()` chain, one per line (Python writes a traceback; plan §2.4).
fn source_chain(error: &AgentError) -> String {
    let mut text = format!("{error}");
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(&format!("\ncaused by: {cause}"));
        source = cause.source();
    }
    text
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> AgentError + '_ {
    move |source| AgentError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn write_atomic(path: &Path, data: &Value) -> Result<(), AgentError> {
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    std::fs::create_dir_all(parent).map_err(io(parent))?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(io(parent))?;
    let text = serde_json::to_string_pretty(data)?;
    file.write_all(text.as_bytes()).map_err(io(file.path()))?;
    file.persist(path).map_err(|e| AgentError::Io {
        path: path.to_path_buf(),
        source: e.error,
    })?;
    Ok(())
}
