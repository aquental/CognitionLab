//! The `Agent` protocol (`__init__.py:71-80`, plan §3.3) and `DefaultAgent` (§3.4-§3.5).

mod default;

pub use default::{AgentConfig, DefaultAgent};

use std::path::Path;

use serde_json::{Map, Value};

use crate::error::AgentError;
use crate::message::ExitInfo;

/// An agent that runs a task to completion.
pub trait Agent {
    /// Runs the task. `extra_vars` are merged into the template variables and kept across runs,
    /// as Python's `run(task, **kwargs)` does with `|=`.
    fn run(&mut self, task: &str, extra_vars: Map<String, Value>) -> Result<ExitInfo, AgentError>;

    /// Builds the trajectory, merging `extra` last, and writes it when `path` is set
    /// (`save(path, *extra_dicts)`).
    fn save(&self, path: Option<&Path>, extra: &[Value]) -> Result<Value, AgentError>;
}
