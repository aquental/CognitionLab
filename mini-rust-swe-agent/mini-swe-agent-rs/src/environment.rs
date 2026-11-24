//! The `Environment` protocol (`__init__.py:58-68`, plan §3.3).

use std::path::Path;

use serde::Serialize;
use serde_json::{Value, json};

use crate::error::{AgentError, StepError};
use crate::message::{Action, ExecOutput};

/// Where commands run. It also owns the submission signal (`Submitted`).
pub trait Environment {
    /// The config, dumped into the trajectory.
    type Config: Serialize;

    /// The config.
    fn config(&self) -> &Self::Config;

    /// The type name written as `info.config.environment_type`.
    fn type_name(&self) -> &'static str
    where
        Self: Sized,
    {
        std::any::type_name::<Self>()
    }

    /// Runs one action. Returns its output, or the `Submitted` interrupt.
    fn execute(&self, action: &Action, cwd: Option<&Path>) -> Result<ExecOutput, StepError>;

    /// Template variables. There is no default: the local environment adds `uname` fields and
    /// the host environment, which a config-only default would leave undefined.
    fn template_vars(&self) -> Result<Value, AgentError>;

    /// The environment's part of the trajectory.
    fn serialize(&self) -> Result<Value, AgentError>
    where
        Self: Sized,
    {
        Ok(
            json!({"info": {"config": {"environment": serde_json::to_value(self.config())?, "environment_type": self.type_name()}}}),
        )
    }
}
