//! The `Model` protocol (`__init__.py:43-55`, plan §3.3).

pub mod actions_text;

use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::error::{AgentError, StepError};
use crate::message::{ExecOutput, Message, Role};

/// A language model, including how it parses actions and formats observations.
pub trait Model {
    /// The config, dumped into the trajectory.
    type Config: Serialize;

    /// The config.
    fn config(&self) -> &Self::Config;

    /// The type name written as `info.config.model_type`.
    fn type_name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    /// One model call. `&mut self` because a model may advance state (the deterministic model).
    /// Returns the assistant message, or an interrupt (`FormatError`) or a fatal error.
    fn query(&mut self, messages: &[Message]) -> Result<Message, StepError>;

    /// Builds a message; Python's `format_message(**kwargs)` returns its arguments unchanged
    /// when multimodal content is off. `extra` is passed in (not set afterwards) so an
    /// implementation sees the whole message, as `handle_uncaught_exception` passes it.
    fn format_message(
        &self,
        role: Role,
        content: String,
        extra: Option<Map<String, Value>>,
    ) -> Message {
        let message = Message::new(role, content);
        match extra {
            Some(extra) => message.with_extra(extra),
            None => message,
        }
    }

    /// Observation messages for the outputs of `message`'s actions.
    fn format_observation_messages(
        &self,
        message: &Message,
        outputs: &[ExecOutput],
        template_vars: &Value,
    ) -> Result<Vec<Message>, AgentError>;

    /// Template variables: the config, like Python's `model_dump()`.
    fn template_vars(&self) -> Result<Value, AgentError> {
        Ok(serde_json::to_value(self.config())?)
    }

    /// The model's part of the trajectory (`litellm_model.py:156-164`).
    fn serialize(&self) -> Result<Value, AgentError> {
        Ok(
            json!({"info": {"config": {"model": serde_json::to_value(self.config())?, "model_type": self.type_name()}}}),
        )
    }
}
