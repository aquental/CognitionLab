//! Text-based actions (port of `models/utils/actions_text.py`, plan §3.8).
//!
//! The action protocol lives in the model: it finds the command in the reply with a regex, or
//! raises `FormatError`, and it formats command outputs as observation messages.

use regex::Regex;
use serde_json::{Map, Value, json};

use crate::error::{AgentError, Interrupt, InterruptKind, StepError};
use crate::message::{Action, ExecOutput, Message, Role};
use crate::pystr;
use crate::template::{TemplateError, Templates};

/// The default action regex of `LitellmTextbasedModelConfig` (`litellm_textbased_model.py:8`).
pub const DEFAULT_ACTION_REGEX: &str = r"```mswea_bash_command\s*\n(.*?)\n```";

const FORMAT_ERROR: &str = "format_error_template";
const OBSERVATION: &str = "observation_template";

/// An invalid `action_regex`.
#[derive(Debug, thiserror::Error)]
pub enum ActionRegexError {
    /// The pattern does not compile with the Rust `regex` engine (lookaround and
    /// backreferences are Python-only; risk R6).
    #[error("invalid action_regex: {0}")]
    Invalid(#[from] regex::Error),
    /// Python's `findall` returns tuples for several groups, which then fail as commands.
    #[error("action_regex has {groups} capture groups; at most one is supported")]
    TooManyGroups {
        /// Number of capture groups.
        groups: usize,
    },
}

/// An `action_regex` compiled with Python's `re.DOTALL`.
#[derive(Clone, Debug)]
pub struct ActionRegex {
    regex: Regex,
    has_group: bool,
}

impl ActionRegex {
    /// Compiles `pattern` as `re.DOTALL` does (`(?s)`).
    pub fn new(pattern: &str) -> Result<Self, ActionRegexError> {
        let regex = Regex::new(&format!("(?s){pattern}"))?;
        let groups = regex.captures_len().saturating_sub(1);
        if groups > 1 {
            return Err(ActionRegexError::TooManyGroups { groups });
        }
        Ok(Self {
            regex,
            has_group: groups == 1,
        })
    }

    /// Python's `re.findall`: the group when there is one (an unmatched group is `""`), the
    /// whole match otherwise.
    pub fn findall<'t>(&self, content: &'t str) -> Vec<&'t str> {
        if self.has_group {
            self.regex
                .captures_iter(content)
                .map(|captures| captures.get(1).map_or("", |group| group.as_str()))
                .collect()
        } else {
            self.regex
                .find_iter(content)
                .map(|found| found.as_str())
                .collect()
        }
    }
}

/// Errors building [`TextActions`].
#[derive(Debug, thiserror::Error)]
pub enum TextActionsError {
    /// Bad `action_regex`.
    #[error(transparent)]
    Regex(#[from] ActionRegexError),
    /// A template does not compile.
    #[error(transparent)]
    Template(#[from] TemplateError),
}

/// The text action protocol: regex, format-error template and observation template, compiled
/// once when the model is built.
#[derive(Debug)]
pub struct TextActions {
    regex: ActionRegex,
    templates: Templates,
}

impl TextActions {
    /// Compiles the regex and both templates; syntax errors fail here, not at render time.
    pub fn new(
        action_regex: &str,
        format_error_template: &str,
        observation_template: &str,
    ) -> Result<Self, TextActionsError> {
        let mut templates = Templates::new();
        templates.add(FORMAT_ERROR, format_error_template)?;
        templates.add(OBSERVATION, observation_template)?;
        Ok(Self {
            regex: ActionRegex::new(action_regex)?,
            templates,
        })
    }

    /// `parse_regex_actions` (`actions_text.py:15-40`): exactly one action, or a `FormatError`
    /// carrying one `user` message. `template_kwargs` (such as `finish_reason`) reach only the
    /// format-error template.
    pub fn parse(
        &self,
        content: &str,
        template_kwargs: &Map<String, Value>,
    ) -> Result<Vec<Action>, StepError> {
        let actions: Vec<&str> = self
            .regex
            .findall(content)
            .into_iter()
            .map(pystr::strip)
            .collect();
        if let [command] = actions.as_slice() {
            return Ok(vec![Action::Command {
                command: (*command).to_owned(),
            }]);
        }
        let error = format!("Expected exactly 1 action, found {}.", actions.len());
        let mut context = Map::new();
        context.insert("actions".to_owned(), json!(actions));
        context.insert("error".to_owned(), Value::String(error));
        for (key, value) in template_kwargs {
            context.insert(key.clone(), value.clone());
        }
        let rendered = self
            .templates
            .render(FORMAT_ERROR, &Value::Object(context))?;
        let mut extra = Map::new();
        extra.insert("interrupt_type".to_owned(), json!("FormatError"));
        extra.insert("n_actions".to_owned(), json!(actions.len()));
        extra.insert("model_response".to_owned(), json!(content));
        let message = Message::new(Role::User, rendered).with_extra(extra);
        Err(StepError::Interrupt(Interrupt::new(
            InterruptKind::FormatError,
            message,
            Vec::new(),
        )))
    }

    /// `format_observation_messages` (`actions_text.py:43-70`): one `user` message per output.
    /// The template sees the agent's variables plus `output`; an `output` key already in the
    /// variables is an error when an observation is rendered, as in Python.
    pub fn observations(
        &self,
        outputs: &[ExecOutput],
        template_vars: &Value,
        timestamp: f64,
    ) -> Result<Vec<Message>, AgentError> {
        let base = match template_vars {
            Value::Object(map) => map.clone(),
            Value::Null => Map::new(),
            _ => {
                return Err(AgentError::Internal(
                    "template variables are not an object".to_owned(),
                ));
            }
        };
        let mut messages = Vec::with_capacity(outputs.len());
        for output in outputs {
            if base.contains_key("output") {
                return Err(TemplateError::DuplicateKey("output".to_owned()).into());
            }
            let mut context = base.clone();
            context.insert("output".to_owned(), serde_json::to_value(output)?);
            let content = self
                .templates
                .render(OBSERVATION, &Value::Object(context))?;
            let mut extra = Map::new();
            extra.insert("raw_output".to_owned(), json!(output.output));
            extra.insert("returncode".to_owned(), json!(output.returncode));
            extra.insert("timestamp".to_owned(), json!(timestamp));
            extra.insert("exception_info".to_owned(), json!(output.exception_info));
            for (key, value) in output.extra.iter().flatten() {
                extra.insert(key.clone(), value.clone());
            }
            messages.push(Message::new(Role::User, content).with_extra(extra));
        }
        Ok(messages)
    }
}
