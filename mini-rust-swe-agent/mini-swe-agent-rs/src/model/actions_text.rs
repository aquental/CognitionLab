//! Text-based actions (port of `models/utils/actions_text.py`, plan §3.8).
//!
//! The action protocol lives in the model: it finds the command in the reply with a regex, or
//! raises `FormatError`, and it formats command outputs as observation messages.

use regex::{Regex, RegexBuilder};
use serde_json::{Map, Value, json};

use crate::clock::Clock;
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
    /// The pattern compiles, but Python's `re` would match differently.
    #[error("unsupported action_regex: {reason}")]
    Unsupported {
        /// What differs from Python's `re`.
        reason: &'static str,
    },
}

/// Python's `\s` for `str` patterns: Unicode whitespace plus `\x1c`..`\x1f` (`str.isspace`).
const PY_SPACE: &str = r"\s\x1c-\x1f";

/// Rewrites the Python-only parts of a pattern for the `regex` crate: `\s` and `\S` get Python's
/// whitespace set and `\Z` becomes `\z`. Forms that cannot be rewritten are refused: `$` outside a
/// class (Python's also matches before a final newline) and `\S` inside a class.
fn translate(pattern: &str) -> Result<String, ActionRegexError> {
    let mut out = String::with_capacity(pattern.len() + 16);
    let mut chars = pattern.chars().peekable();
    // Inside `[...]`; `class_start` is true right after `[` or `[^`, where `]` is literal.
    let (mut in_class, mut class_start) = (false, false);
    while let Some(c) = chars.next() {
        let at_start = std::mem::take(&mut class_start);
        match c {
            '\\' => match chars.next() {
                Some('s') if in_class => out.push_str(PY_SPACE),
                Some('s') => out.push_str(&format!("[{PY_SPACE}]")),
                Some('S') if in_class => {
                    return Err(ActionRegexError::Unsupported {
                        reason: "`\\S` inside a character class",
                    });
                }
                Some('S') => out.push_str(&format!("[^{PY_SPACE}]")),
                Some('Z') if !in_class => out.push_str(r"\z"),
                Some(next) => {
                    out.push('\\');
                    out.push(next);
                }
                None => out.push('\\'),
            },
            '[' if !in_class => {
                in_class = true;
                class_start = true;
                out.push('[');
                if chars.peek() == Some(&'^') {
                    chars.next();
                    out.push('^');
                }
            }
            ']' if in_class && !at_start => {
                in_class = false;
                out.push(']');
            }
            '$' if !in_class => {
                return Err(ActionRegexError::Unsupported {
                    reason: "Python's `$` also matches before a final newline; use `\\Z`",
                });
            }
            _ => out.push(c),
        }
    }
    Ok(out)
}

/// An `action_regex` compiled with Python's `re.DOTALL`.
#[derive(Clone, Debug)]
pub struct ActionRegex {
    regex: Regex,
    has_group: bool,
}

impl ActionRegex {
    /// Compiles `pattern` as `re.DOTALL` does, after rewriting `\s`, `\S` and `\Z` to Python's
    /// meaning. A pattern that can match the empty string is refused: Python's `findall` and the
    /// `regex` crate disagree on where the next match may start after an empty one.
    ///
    /// Other engine differences remain (risk R6): `\w`, `\d` and `\b` use the `regex` crate's
    /// Unicode classes (a combining mark is a word character here, not in Python), and
    /// lookaround and backreferences do not compile.
    pub fn new(pattern: &str) -> Result<Self, ActionRegexError> {
        let translated = translate(pattern)?;
        let regex = RegexBuilder::new(&translated)
            .dot_matches_new_line(true)
            .build()?;
        if Self::can_match_empty(&translated) {
            return Err(ActionRegexError::Unsupported {
                reason: "the pattern can match the empty string",
            });
        }
        let groups = regex.captures_len().saturating_sub(1);
        if groups > 1 {
            return Err(ActionRegexError::TooManyGroups { groups });
        }
        Ok(Self {
            regex,
            has_group: groups == 1,
        })
    }

    /// True when some input has an empty match (the pattern's minimum length is zero).
    fn can_match_empty(pattern: &str) -> bool {
        regex_syntax::ParserBuilder::new()
            .dot_matches_new_line(true)
            .build()
            .parse(pattern)
            .is_ok_and(|hir| hir.properties().minimum_len() == Some(0))
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
    observations: Observations,
}

/// The observation template, compiled once (`format_observation_messages`).
#[derive(Debug)]
pub struct Observations {
    templates: Templates,
}

impl Observations {
    /// Compiles the observation template.
    pub fn new(observation_template: &str) -> Result<Self, TemplateError> {
        let mut templates = Templates::new();
        templates.add(OBSERVATION, observation_template)?;
        Ok(Self { templates })
    }

    /// `format_observation_messages` (`actions_text.py:43-70`): one `user` message per output.
    /// The template sees the agent's variables plus `output`; an `output` key already in the
    /// variables is an error when an observation is rendered, as in Python. The clock is read
    /// once per output, after its template renders (`time.time()` in the message literal).
    pub fn messages(
        &self,
        outputs: &[ExecOutput],
        template_vars: &Value,
        clock: &dyn Clock,
    ) -> Result<Vec<Message>, AgentError> {
        let vars = match template_vars {
            Value::Object(map) => Some(map),
            Value::Null => None,
            _ => {
                return Err(AgentError::Internal(
                    "template variables are not an object".to_owned(),
                ));
            }
        };
        if outputs.is_empty() {
            return Ok(Vec::new());
        }
        if vars.is_some_and(|vars| vars.contains_key("output")) {
            return Err(TemplateError::DuplicateKey("output".to_owned()).into());
        }
        // One copy of the variables; only `output` changes between renders.
        let mut context = Value::Object(vars.cloned().unwrap_or_default());
        let mut messages = Vec::with_capacity(outputs.len());
        for output in outputs {
            if let Value::Object(map) = &mut context {
                map.insert("output".to_owned(), serde_json::to_value(output)?);
            }
            let content = self.templates.render(OBSERVATION, &context)?;
            let timestamp = serde_json::Number::from_f64(clock.now()).ok_or_else(|| {
                AgentError::Internal("the clock returned a non-finite time".to_owned())
            })?;
            let mut extra = Map::new();
            extra.insert("raw_output".to_owned(), json!(output.output));
            extra.insert("returncode".to_owned(), json!(output.returncode));
            extra.insert("timestamp".to_owned(), Value::Number(timestamp));
            extra.insert("exception_info".to_owned(), json!(output.exception_info));
            for (key, value) in output.extra.iter().flatten() {
                extra.insert(key.clone(), value.clone());
            }
            messages.push(Message::new(Role::User, content).with_extra(extra));
        }
        Ok(messages)
    }
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
        let observations = Observations::new(observation_template)?;
        Ok(Self {
            regex: ActionRegex::new(action_regex)?,
            templates,
            observations,
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
        // Python's `render(actions=..., error=..., **kwargs)` raises TypeError on a repeated name.
        if let Some(key) = ["actions", "error"]
            .into_iter()
            .find(|key| template_kwargs.contains_key(*key))
        {
            return Err(StepError::Fatal(
                TemplateError::DuplicateKey(key.to_owned()).into(),
            ));
        }
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

    /// Observation messages for `outputs` (see [`Observations::messages`]).
    pub fn observations(
        &self,
        outputs: &[ExecOutput],
        template_vars: &Value,
        clock: &dyn Clock,
    ) -> Result<Vec<Message>, AgentError> {
        self.observations.messages(outputs, template_vars, clock)
    }
}
