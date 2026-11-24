//! The text-based model (port of `litellm_model.py:81-106` with `litellm_textbased_model.py`,
//! plan §3.8): one call through a [`ChatBackend`] with retries, the cost from a registry, global
//! stats, then the action parsed from the reply text.

use std::path::Path;
use std::sync::Arc;

use serde_json::{Map, Value, json};

use crate::clock::Clock;
use crate::config::{CostTracking, TextbasedModelConfig};
use crate::error::{AgentError, InterruptKind, ModelError, StepError};
use crate::host_env::HostEnv;
use crate::message::{ExecOutput, Message};
use crate::model::Model;
use crate::model::actions_text::TextActions;
use crate::model::cost::{Registry, completion_cost};
use crate::model::openai_compat::{ChatBackend, ChatRequest, ChatResponse, Settings};
use crate::model::retry::{attempts_from_env, retry};
use crate::model::stats::GlobalModelStats;

/// What replaces a literal `model_kwargs.api_key` wherever the config is written (plan §2.4).
pub const REDACTED: &str = "<redacted>";

/// A model that sends the history to a chat backend and parses one action from the reply.
pub struct TextbasedModel<B> {
    config: TextbasedModelConfig,
    backend: B,
    actions: TextActions,
    params: Map<String, Value>,
    registry: Option<Registry>,
    attempts: u32,
    stats: Arc<GlobalModelStats>,
    clock: Arc<dyn Clock>,
}

impl<B> std::fmt::Debug for TextbasedModel<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextbasedModel")
            .field("model_name", &self.config.model_name)
            .finish_non_exhaustive()
    }
}

impl<B: ChatBackend> TextbasedModel<B> {
    /// Builds the model: splits `model_kwargs` (reserved keys are refused), compiles the regex
    /// and templates, loads the registry when its file exists, and reads the retry setting.
    pub fn new(
        config: TextbasedModelConfig,
        backend: B,
        stats: Arc<GlobalModelStats>,
        clock: Arc<dyn Clock>,
        host: &HostEnv,
    ) -> Result<Self, ModelError> {
        let params = Settings::from_model_kwargs(&config.model_kwargs, host)?.params;
        let actions = TextActions::new(
            &config.action_regex,
            &config.format_error_template,
            &config.observation_template,
        )
        .map_err(|e| ModelError::Settings(e.to_string()))?;
        let registry = match &config.litellm_model_registry {
            Some(path) => Registry::load(Path::new(path)).map_err(ModelError::Settings)?,
            None => None,
        };
        Ok(Self {
            attempts: attempts_from_env(host)?,
            config,
            backend,
            actions,
            params,
            registry,
            stats,
            clock,
        })
    }

    /// The backend (tests inspect what it received).
    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// `_calculate_cost`: an error unless `cost_tracking` is `ignore_errors`, which gives `0.0`.
    fn cost(&self, response: &ChatResponse) -> Result<f64, ModelError> {
        match completion_cost(
            self.registry.as_ref(),
            &self.config.model_name,
            response.usage(),
        ) {
            Ok(cost) => Ok(cost),
            Err(_) if self.config.cost_tracking == CostTracking::IgnoreErrors => Ok(0.0),
            Err(reason) => Err(ModelError::CostTracking(format!(
                "Error calculating cost for model {}: {reason}, perhaps it's not registered? Set \
                 model.litellm_model_registry (or LITELLM_MODEL_REGISTRY_PATH) to a LiteLLM-format \
                 JSON file with input_cost_per_token and output_cost_per_token, or ignore this \
                 issue with cost_tracking: 'ignore_errors' in the config or \
                 MSWEA_COST_TRACKING=ignore_errors in the environment.",
                self.config.model_name
            ))),
        }
    }

    /// The config as written to trajectories and templates: a literal API key is redacted.
    fn redacted_config(&self) -> Result<Value, AgentError> {
        let mut value = serde_json::to_value(&self.config)?;
        if let Some(key) = value
            .pointer_mut("/model_kwargs/api_key")
            .filter(|k| !k.is_null())
        {
            *key = Value::String(REDACTED.to_owned());
        }
        Ok(value)
    }
}

/// A history message as sent to the provider: `role`, `content` and the provider fields, in
/// that order. `extra` (raw responses, command outputs) is never serialized.
fn api_message(message: &Message) -> Result<Value, AgentError> {
    let mut map = Map::new();
    map.insert("role".to_owned(), serde_json::to_value(message.role)?);
    map.insert("content".to_owned(), Value::String(message.content.clone()));
    for (key, value) in message.provider_fields() {
        map.insert(key.clone(), value.clone());
    }
    Ok(Value::Object(map))
}

impl<B: ChatBackend> Model for TextbasedModel<B> {
    type Config = TextbasedModelConfig;

    fn config(&self) -> &TextbasedModelConfig {
        &self.config
    }

    fn query(&mut self, messages: &[Message]) -> Result<Message, StepError> {
        let request = ChatRequest {
            model: self.config.model_name.clone(),
            messages: messages.iter().map(api_message).collect::<Result<_, _>>()?,
            params: self.params.clone(),
        };
        let backend = &self.backend;
        // An invalid completion is retried inside the same budget, before any billing.
        let response = retry(self.attempts, self.clock.as_ref(), || {
            backend.complete(&request).and_then(ChatResponse::validated)
        })
        .map_err(ModelError::Backend)?;
        let cost = self.cost(&response)?;
        self.stats.add(cost)?;
        let mut kwargs = Map::new();
        kwargs.insert("finish_reason".to_owned(), response.finish_reason());
        let content = response.content().unwrap_or_default();
        let actions = match self.actions.parse(content, &kwargs) {
            Ok(actions) => actions,
            // The response and its cost are persisted on a format error (`litellm_model.py:90-98`).
            Err(StepError::Interrupt(mut interrupt))
                if interrupt.kind() == InterruptKind::FormatError =>
            {
                let head = interrupt.head_mut();
                let extra = head.extra.get_or_insert_with(Map::new);
                extra.insert("cost".to_owned(), json!(cost));
                extra.insert("response".to_owned(), response.raw().clone());
                return Err(StepError::Interrupt(interrupt));
            }
            Err(other) => return Err(other),
        };
        let mut message = response.message().cloned().unwrap_or_default();
        // LiteLLM's `Message` defaults the role to `assistant`.
        message
            .entry("role")
            .or_insert_with(|| Value::String("assistant".to_owned()));
        message.insert("content".to_owned(), Value::String(content.to_owned()));
        let timestamp = serde_json::Number::from_f64(self.clock.now()).ok_or_else(|| {
            AgentError::Internal("the clock returned a non-finite time".to_owned())
        })?;
        let mut extra = Map::new();
        extra.insert(
            "actions".to_owned(),
            Value::Array(actions.iter().map(|a| a.to_value()).collect()),
        );
        extra.insert("response".to_owned(), response.raw().clone());
        extra.insert("cost".to_owned(), json!(cost));
        extra.insert("timestamp".to_owned(), Value::Number(timestamp));
        message.insert("extra".to_owned(), Value::Object(extra));
        serde_json::from_value(Value::Object(message)).map_err(|e| {
            AgentError::Internal(format!("the provider message is not a valid message: {e}")).into()
        })
    }

    fn format_observation_messages(
        &self,
        _message: &Message,
        outputs: &[ExecOutput],
        template_vars: &Value,
    ) -> Result<Vec<Message>, AgentError> {
        self.actions
            .observations(outputs, template_vars, self.clock.as_ref())
    }

    fn template_vars(&self) -> Result<Value, AgentError> {
        self.redacted_config()
    }

    fn serialize(&self) -> Result<Value, AgentError> {
        Ok(
            json!({"info": {"config": {"model": self.redacted_config()?, "model_type": self.type_name()}}}),
        )
    }
}
