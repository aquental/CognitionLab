//! The OpenAI-compatible Chat Completions transport (`POST {base_url}/chat/completions`, plan §3.8
//! and decision D4). One adapter covers OpenAI, OpenRouter, a LiteLLM proxy, vLLM and Ollama.
//!
//! The model talks to a [`ChatBackend`]; tests inject canned backends, and `tests/fake_http.rs`
//! checks the real wire JSON of [`OpenAiCompatBackend`].

use std::time::Duration;

use serde_json::{Map, Value};

use crate::error::ModelError;
use crate::host_env::HostEnv;

/// The default endpoint when neither `api_base` nor `base_url` is set.
pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

/// `model_kwargs` keys that are part of the request itself; setting them is a config error.
const RESERVED: &[&str] = &["model", "messages", "tools", "tool_choice", "stream", "n"];

/// `model_kwargs` keys that configure LiteLLM's client, not the provider; they are dropped.
const LITELLM_ONLY: &[&str] = &["drop_params"];

/// One Chat Completions request.
#[derive(Clone, Debug, PartialEq)]
pub struct ChatRequest {
    /// The model name.
    pub model: String,
    /// The messages, each without its `extra` key.
    pub messages: Vec<Value>,
    /// Other body parameters (`temperature`, `max_tokens`, …).
    pub params: Map<String, Value>,
}

impl ChatRequest {
    /// The JSON body: `model`, `messages`, then the parameters.
    pub fn body(&self) -> Value {
        let mut body = Map::new();
        body.insert("model".to_owned(), Value::String(self.model.clone()));
        body.insert("messages".to_owned(), Value::Array(self.messages.clone()));
        for (key, value) in &self.params {
            body.insert(key.clone(), value.clone());
        }
        Value::Object(body)
    }
}

/// A provider response, kept as the raw JSON (Python persists `response.model_dump()`).
#[derive(Clone, Debug, PartialEq)]
pub struct ChatResponse(Value);

impl ChatResponse {
    /// Wraps a response body.
    pub fn new(body: Value) -> Self {
        Self(body)
    }

    /// The raw body.
    pub fn raw(&self) -> &Value {
        &self.0
    }

    /// `choices[0]`.
    fn choice(&self) -> Option<&Value> {
        self.0.get("choices").and_then(|c| c.get(0))
    }

    /// `choices[0].message`, as an object.
    pub fn message(&self) -> Option<&Map<String, Value>> {
        self.choice()
            .and_then(|c| c.get("message"))
            .and_then(Value::as_object)
    }

    /// `choices[0].message.content`, or `None` when absent or `null`.
    pub fn content(&self) -> Option<&str> {
        self.message()
            .and_then(|m| m.get("content"))
            .and_then(Value::as_str)
    }

    /// `choices[0].finish_reason` (`null` when absent).
    pub fn finish_reason(&self) -> Value {
        self.choice()
            .and_then(|c| c.get("finish_reason"))
            .cloned()
            .unwrap_or(Value::Null)
    }

    /// `(prompt_tokens, completion_tokens)` from `usage`.
    pub fn usage(&self) -> Option<(u64, u64)> {
        let usage = self.0.get("usage")?;
        Some((
            usage.get("prompt_tokens")?.as_u64()?,
            usage.get("completion_tokens")?.as_u64()?,
        ))
    }
}

/// A failed completion. The first five never retry (`LitellmModel.abort_exceptions`).
#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    /// HTTP 401.
    #[error("authentication failed: {0}")]
    Authentication(String),
    /// HTTP 403.
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    /// HTTP 404 (an unknown model or endpoint).
    #[error("not found: {0}")]
    NotFound(String),
    /// HTTP 400 whose body says the context window was exceeded.
    #[error("context window exceeded: {0}")]
    ContextWindowExceeded(String),
    /// HTTP 400 whose body names an unsupported parameter.
    #[error("unsupported parameters: {0}")]
    UnsupportedParams(String),
    /// Any other HTTP error status (retried).
    #[error("HTTP {status}: {body}")]
    Status {
        /// The status code.
        status: u16,
        /// The response body.
        body: String,
    },
    /// The request could not be sent or the response not read (retried).
    #[error("transport error: {0}")]
    Transport(String),
    /// A 2xx response that is not JSON (retried).
    #[error("invalid response: {0}")]
    InvalidResponse(String),
}

impl BackendError {
    /// True for the errors that end the run at once.
    pub fn aborts(&self) -> bool {
        matches!(
            self,
            Self::Authentication(_)
                | Self::PermissionDenied(_)
                | Self::NotFound(_)
                | Self::ContextWindowExceeded(_)
                | Self::UnsupportedParams(_)
        )
    }

    /// The LiteLLM exception name, used as the exit status (D6).
    pub fn exit_status(&self) -> &'static str {
        match self {
            Self::Authentication(_) => "AuthenticationError",
            Self::PermissionDenied(_) => "PermissionDeniedError",
            Self::NotFound(_) => "NotFoundError",
            Self::ContextWindowExceeded(_) => "ContextWindowExceededError",
            Self::UnsupportedParams(_) => "UnsupportedParamsError",
            Self::Status { .. } => "APIError",
            Self::Transport(_) => "APIConnectionError",
            Self::InvalidResponse(_) => "APIError",
        }
    }

    /// Maps an HTTP error status and its body.
    pub fn from_status(status: u16, body: String) -> Self {
        let lower = body.to_lowercase();
        match status {
            401 => Self::Authentication(body),
            403 => Self::PermissionDenied(body),
            404 => Self::NotFound(body),
            400 if lower.contains("context_length_exceeded")
                || lower.contains("context length")
                || lower.contains("context window")
                || lower.contains("maximum context") =>
            {
                Self::ContextWindowExceeded(body)
            }
            400 if lower.contains("unsupported parameter")
                || lower.contains("unrecognized request argument")
                || lower.contains("unsupported_parameter") =>
            {
                Self::UnsupportedParams(body)
            }
            _ => Self::Status { status, body },
        }
    }
}

/// A completion transport.
pub trait ChatBackend {
    /// Sends one request.
    fn complete(&self, request: &ChatRequest) -> Result<ChatResponse, BackendError>;
}

impl<B: ChatBackend + ?Sized> ChatBackend for std::sync::Arc<B> {
    fn complete(&self, request: &ChatRequest) -> Result<ChatResponse, BackendError> {
        (**self).complete(request)
    }
}

/// Where and how to send requests. The key never leaves this struct: `Debug` redacts it.
#[derive(Clone, PartialEq)]
pub struct Transport {
    /// For example `https://api.openai.com/v1`.
    pub base_url: String,
    /// The bearer token, if any.
    pub api_key: Option<String>,
    /// The whole-request timeout, if any.
    pub timeout: Option<Duration>,
}

impl std::fmt::Debug for Transport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transport")
            .field("base_url", &self.base_url)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("timeout", &self.timeout)
            .finish()
    }
}

/// `model_kwargs` split into transport settings and body parameters (plan §3.8 item 1).
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// Endpoint, key and timeout.
    pub transport: Transport,
    /// Everything else, sent in the body.
    pub params: Map<String, Value>,
}

impl Settings {
    /// Splits `model_kwargs`: `api_base`/`base_url`, `api_key` (else the variable named by
    /// `api_key_env`, default `OPENAI_API_KEY`) and `timeout` configure the transport; LiteLLM's
    /// own keys are dropped; request keys are refused; the rest goes into the body.
    pub fn from_model_kwargs(
        kwargs: &Map<String, Value>,
        host: &HostEnv,
    ) -> Result<Self, ModelError> {
        let text = |key: &str| -> Result<Option<String>, ModelError> {
            match kwargs.get(key) {
                None | Some(Value::Null) => Ok(None),
                Some(Value::String(value)) => Ok(Some(value.clone())),
                Some(other) => Err(ModelError::Settings(format!(
                    "model_kwargs.{key} must be a string, found {other}"
                ))),
            }
        };
        if let Some(key) = RESERVED.iter().find(|key| kwargs.contains_key(**key)) {
            return Err(ModelError::Settings(format!(
                "model_kwargs.{key} is set by the model itself and cannot be configured"
            )));
        }
        let base_url = match (text("api_base")?, text("base_url")?) {
            (Some(url), _) | (None, Some(url)) => url,
            (None, None) => DEFAULT_BASE_URL.to_owned(),
        };
        let key_env = text("api_key_env")?.unwrap_or_else(|| "OPENAI_API_KEY".to_owned());
        let api_key = match text("api_key")? {
            Some(key) => Some(key),
            None => host
                .get(&key_env)
                .filter(|k| !k.is_empty())
                .map(str::to_owned),
        };
        let timeout = match kwargs.get("timeout") {
            None | Some(Value::Null) => None,
            Some(value) => match value.as_f64() {
                Some(seconds) => Some(Duration::try_from_secs_f64(seconds).map_err(|_| {
                    ModelError::Settings(format!("model_kwargs.timeout is invalid: {seconds}"))
                })?),
                None => {
                    return Err(ModelError::Settings(format!(
                        "model_kwargs.timeout must be a number, found {value}"
                    )));
                }
            },
        };
        let transport_keys = ["api_base", "base_url", "api_key", "api_key_env", "timeout"];
        let params = kwargs
            .iter()
            .filter(|(key, _)| {
                !transport_keys.contains(&key.as_str()) && !LITELLM_ONLY.contains(&key.as_str())
            })
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        Ok(Self {
            transport: Transport {
                base_url,
                api_key,
                timeout,
            },
            params,
        })
    }
}

/// `POST {base_url}/chat/completions` over `ureq` (blocking, rustls; plan §3.6).
#[derive(Debug)]
pub struct OpenAiCompatBackend {
    agent: ureq::Agent,
    transport: Transport,
}

impl OpenAiCompatBackend {
    /// A backend for `transport`. Error statuses are returned, not raised, so their bodies
    /// stay readable for the error mapping.
    pub fn new(transport: Transport) -> Self {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(transport.timeout)
            .build()
            .new_agent();
        Self { agent, transport }
    }

    fn url(&self) -> String {
        format!(
            "{}/chat/completions",
            self.transport.base_url.trim_end_matches('/')
        )
    }
}

impl ChatBackend for OpenAiCompatBackend {
    fn complete(&self, request: &ChatRequest) -> Result<ChatResponse, BackendError> {
        let mut call = self
            .agent
            .post(&self.url())
            .header("Content-Type", "application/json");
        if let Some(key) = &self.transport.api_key {
            call = call.header("Authorization", &format!("Bearer {key}"));
        }
        let body = request.body().to_string();
        let mut response = call
            .send(body.as_bytes())
            .map_err(|e| BackendError::Transport(e.to_string()))?;
        let status = response.status().as_u16();
        let text = response
            .body_mut()
            .read_to_string()
            .map_err(|e| BackendError::Transport(e.to_string()))?;
        if !(200..300).contains(&status) {
            return Err(BackendError::from_status(status, text));
        }
        serde_json::from_str(&text)
            .map(ChatResponse::new)
            .map_err(|e| BackendError::InvalidResponse(e.to_string()))
    }
}
