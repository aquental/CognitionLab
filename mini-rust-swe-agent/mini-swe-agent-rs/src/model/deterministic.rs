//! `DeterministicModel` (port of `test_models.py:75-130`, plan §3.8): returns canned outputs in
//! order. It is the parity oracle's model, so it never parses text.

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::clock::Clock;
use crate::error::{AgentError, ModelError, StepError};
use crate::message::{Action, ExecOutput, Message, RaiseSpec};
use crate::model::Model;
use crate::model::actions_text::Observations;
use crate::model::stats::GlobalModelStats;

/// The default observation template (`DeterministicModelConfig.observation_template`).
pub const DEFAULT_OBSERVATION_TEMPLATE: &str = "{% if output.exception_info %}<exception>{{output.exception_info}}</exception>\n{% endif %}<returncode>{{output.returncode}}</returncode>\n<output>\n{{output.output}}</output>";

fn default_model_name() -> String {
    "deterministic".to_owned()
}

fn default_cost_per_call() -> f64 {
    1.0
}

fn default_observation_template() -> String {
    DEFAULT_OBSERVATION_TEMPLATE.to_owned()
}

/// `DeterministicModelConfig`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeterministicModelConfig {
    /// The messages to return, in order. Special actions live in `extra.actions`.
    pub outputs: Vec<Message>,
    /// Model name.
    #[serde(default = "default_model_name")]
    pub model_name: String,
    /// Cost charged to the global stats per returned output.
    #[serde(default = "default_cost_per_call")]
    pub cost_per_call: f64,
    /// Observation template.
    #[serde(default = "default_observation_template")]
    pub observation_template: String,
    /// Kept for config parity; multimodal content is out of scope, so it is ignored.
    #[serde(default)]
    pub multimodal_regex: String,
}

/// Returns `config.outputs` one by one.
#[derive(Debug)]
pub struct DeterministicModel {
    config: DeterministicModelConfig,
    next: usize,
    observations: Observations,
    stats: Arc<GlobalModelStats>,
    clock: Arc<dyn Clock>,
}

impl DeterministicModel {
    /// Compiles the observation template.
    pub fn new(
        config: DeterministicModelConfig,
        stats: Arc<GlobalModelStats>,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, AgentError> {
        let observations = Observations::new(&config.observation_template)?;
        Ok(Self {
            config,
            next: 0,
            observations,
            stats,
            clock,
        })
    }

    /// `_process_test_actions` (`test_models.py:75-87`): `Ok(true)` when the output was a
    /// `/sleep` or `/warning` step and the next output should be used.
    fn process_test_actions(&self, actions: Vec<Action>) -> Result<bool, StepError> {
        for action in actions {
            match action {
                Action::Raise(RaiseSpec::Interrupt(interrupt)) => {
                    return Err(StepError::Interrupt(interrupt));
                }
                Action::Raise(RaiseSpec::Fatal { kind, message }) => {
                    return Err(ModelError::Injected { kind, message }.into());
                }
                Action::Command { command } => {
                    if let Some(rest) = command.strip_prefix("/sleep ") {
                        // Python takes `split("/sleep ")[1]`: the text up to a second occurrence.
                        let text = rest.split("/sleep ").next().unwrap_or_default();
                        self.clock.sleep(parse_sleep(text)?);
                        return Ok(true);
                    }
                    if let Some(rest) = command.strip_prefix("/warning") {
                        let text = rest.split("/warning").next().unwrap_or_default();
                        log_warning(text);
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }
}

/// `float(text)` then `time.sleep`: a negative, non-finite or unparsable value is an error.
fn parse_sleep(text: &str) -> Result<Duration, ModelError> {
    let invalid = |message: String| ModelError::Injected {
        kind: "ValueError".to_owned(),
        message,
    };
    let seconds: f64 = text
        .trim()
        .parse()
        .map_err(|_| invalid(format!("could not convert string to float: '{text}'")))?;
    Duration::try_from_secs_f64(seconds)
        .map_err(|_| invalid(format!("invalid sleep length: {seconds}")))
}

fn log_warning(text: &str) {
    use std::io::Write;
    let _ = writeln!(std::io::stderr(), "WARNING:root:{text}");
}

impl Model for DeterministicModel {
    type Config = DeterministicModelConfig;

    fn config(&self) -> &DeterministicModelConfig {
        &self.config
    }

    fn query(&mut self, _messages: &[Message]) -> Result<Message, StepError> {
        loop {
            let index = self.next;
            self.next = self.next.saturating_add(1);
            let output = self
                .config
                .outputs
                .get(index)
                .ok_or(ModelError::OutputsExhausted)?;
            // Python iterates `extra.actions` here, so malformed actions fail inside the model.
            let actions = output.actions()?;
            if self.process_test_actions(actions)? {
                continue;
            }
            self.stats.add(self.config.cost_per_call)?;
            return Ok(output.clone());
        }
    }

    fn format_observation_messages(
        &self,
        _message: &Message,
        outputs: &[ExecOutput],
        template_vars: &Value,
    ) -> Result<Vec<Message>, AgentError> {
        self.observations
            .messages(outputs, template_vars, self.clock.now())
    }
}
