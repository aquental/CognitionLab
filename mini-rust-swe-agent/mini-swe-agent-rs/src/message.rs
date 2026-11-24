//! Messages, actions and command outputs (plan §3.2).
//!
//! A message is `role` + `content` + an optional `extra` object, plus any provider fields
//! (`tool_calls`, …). Whether `extra` is present matters for trajectory parity, so it is an
//! `Option` that is skipped when absent and kept when empty.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::error::{Interrupt, InterruptKind};

/// The role of a message on the text path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// The system prompt.
    System,
    /// The task, observations and format errors.
    User,
    /// A model reply.
    Assistant,
    /// The final message of a run.
    Exit,
}

/// One message of the history (the trajectory *is* the history).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// Who wrote it.
    pub role: Role,
    /// Its text.
    pub content: String,
    /// Provider fields such as `tool_calls`; never `role`, `content` or `extra`.
    #[serde(flatten)]
    provider: Map<String, Value>,
    /// Metadata: actions, cost, timestamps, exit status, …
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<Map<String, Value>>,
}

/// Rejected provider field name.
#[derive(Debug, thiserror::Error)]
#[error("`{0}` is not a provider field")]
pub struct ReservedKey(pub String);

impl Message {
    /// A message with no `extra` key, like the rendered system and task messages.
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            provider: Map::new(),
            extra: None,
        }
    }

    /// The same message with `extra` set.
    #[must_use]
    pub fn with_extra(mut self, extra: Map<String, Value>) -> Self {
        self.extra = Some(extra);
        self
    }

    /// An `exit` message: `extra = {exit_status, submission}` (`default.py:105-147`, `local.py:50-56`).
    pub fn exit(
        exit_status: &str,
        content: impl Into<String>,
        submission: impl Into<String>,
    ) -> Self {
        let mut extra = Map::new();
        extra.insert(
            "exit_status".to_owned(),
            Value::String(exit_status.to_owned()),
        );
        extra.insert("submission".to_owned(), Value::String(submission.into()));
        Self::new(Role::Exit, content).with_extra(extra)
    }

    /// Sets a provider field. `role`, `content` and `extra` are refused, so the message can
    /// never serialize a duplicate key.
    pub fn set_provider_field(&mut self, key: &str, value: Value) -> Result<(), ReservedKey> {
        if matches!(key, "role" | "content" | "extra") {
            return Err(ReservedKey(key.to_owned()));
        }
        self.provider.insert(key.to_owned(), value);
        Ok(())
    }

    /// Provider fields, in insertion order.
    pub fn provider_fields(&self) -> &Map<String, Value> {
        &self.provider
    }

    /// `extra.cost`, or `0.0` when missing (`default.py:150`).
    pub fn cost(&self) -> f64 {
        self.extra_value("cost")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
    }

    /// One `extra` field.
    pub fn extra_value(&self, key: &str) -> Option<&Value> {
        self.extra.as_ref().and_then(|extra| extra.get(key))
    }

    /// Decodes `extra.actions`; missing means no actions (`default.py:156`).
    pub fn actions(&self) -> Result<Vec<Action>, ActionError> {
        match self.extra_value("actions") {
            None => Ok(Vec::new()),
            Some(Value::Array(items)) => items
                .iter()
                .enumerate()
                .map(|(index, item)| Action::decode(index, item))
                .collect(),
            Some(_) => Err(ActionError::NotAList),
        }
    }

    /// `exit_status` and `submission` of an `exit` message; missing fields read as `""`.
    pub fn exit_info(&self) -> Option<ExitInfo> {
        if self.role != Role::Exit {
            return None;
        }
        let text = |key| {
            self.extra_value(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        Some(ExitInfo {
            exit_status: text("exit_status"),
            submission: text("submission"),
        })
    }
}

/// What `run` returns: the last message's status and submission.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExitInfo {
    /// For example `Submitted` or `LimitsExceeded`.
    pub exit_status: String,
    /// The submitted text, or `""`.
    pub submission: String,
}

/// One action of an assistant message.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// A shell command (a missing `command` key means `""`, as `local.py:26` reads it).
    Command {
        /// The command text.
        command: String,
    },
    /// A scripted exception, accepted only by the deterministic model (`test_models.py:75-87`).
    Raise(RaiseSpec),
}

/// What a scripted `raise` action raises.
#[derive(Clone, Debug, PartialEq)]
pub enum RaiseSpec {
    /// An agent-flow interrupt with its messages, such as `FormatError`.
    Interrupt(Interrupt),
    /// Any other exception, which the agent treats as fatal.
    Fatal {
        /// The Python exception name, such as `RuntimeError`.
        kind: String,
        /// Its message.
        message: String,
    },
}

/// An action that cannot be decoded.
#[derive(Debug, thiserror::Error)]
pub enum ActionError {
    /// `extra.actions` is not a list.
    #[error("extra.actions is not a list")]
    NotAList,
    /// One entry is invalid.
    #[error("action {index}: {reason}")]
    Invalid {
        /// Position in the list.
        index: usize,
        /// What is wrong.
        reason: String,
    },
}

impl Action {
    /// Decodes one action **by key presence**: a `raise` key always means [`Action::Raise`] and
    /// its payload must be valid, as Python checks `"raise" in action` first.
    fn decode(index: usize, value: &Value) -> Result<Self, ActionError> {
        let invalid = |reason: String| ActionError::Invalid { index, reason };
        let Value::Object(map) = value else {
            return Err(invalid("not an object".to_owned()));
        };
        if let Some(spec) = map.get("raise") {
            return RaiseSpec::decode(spec).map(Action::Raise).map_err(invalid);
        }
        match map.get("command") {
            None => Ok(Action::Command {
                command: String::new(),
            }),
            Some(Value::String(command)) => Ok(Action::Command {
                command: command.clone(),
            }),
            Some(_) => Err(invalid("`command` is not a string".to_owned())),
        }
    }

    /// The command text; `""` for a scripted raise.
    pub fn command(&self) -> &str {
        match self {
            Action::Command { command } => command,
            Action::Raise(_) => "",
        }
    }

    /// The JSON form of this action.
    pub fn to_value(&self) -> Value {
        match self {
            Action::Command { command } => json!({ "command": command }),
            Action::Raise(spec) => json!({ "raise": spec.to_value() }),
        }
    }
}

impl RaiseSpec {
    fn decode(value: &Value) -> Result<Self, String> {
        let Value::Object(map) = value else {
            return Err("`raise` is not an object".to_owned());
        };
        let Some(kind) = map.get("kind").and_then(Value::as_str) else {
            return Err("`raise.kind` is missing".to_owned());
        };
        if let Some(interrupt_kind) = InterruptKind::from_name(kind) {
            let messages: Vec<Message> = match map.get("messages") {
                Some(messages) => serde_json::from_value(messages.clone())
                    .map_err(|e| format!("`raise.messages`: {e}"))?,
                None => return Err(format!("`raise.messages` is required for {kind}")),
            };
            return Interrupt::from_messages(interrupt_kind, messages)
                .map(RaiseSpec::Interrupt)
                .ok_or_else(|| format!("{kind} needs at least one message"));
        }
        let message = map
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        Ok(RaiseSpec::Fatal {
            kind: kind.to_owned(),
            message,
        })
    }

    fn to_value(&self) -> Value {
        match self {
            RaiseSpec::Interrupt(interrupt) => {
                json!({ "kind": interrupt.kind().name(), "messages": interrupt.messages().collect::<Vec<_>>() })
            }
            RaiseSpec::Fatal { kind, message } => json!({ "kind": kind, "message": message }),
        }
    }
}

/// The result of running one command (`local.py:24-43`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExecOutput {
    /// Merged stdout and stderr.
    pub output: String,
    /// Exit code; `-signal` for a signal; `-1` on errors.
    pub returncode: i32,
    /// `""` on success; the error text otherwise.
    pub exception_info: String,
    /// `exception_type` and `exception` on errors; absent on success.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<Map<String, Value>>,
}
