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

/// The whole-request deadline when `model_kwargs.timeout` is not set (LiteLLM's default).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(600);

/// At most this much of an error response is read.
const ERROR_BODY_BYTES: u64 = 64 * 1024;

/// At most this many characters of a provider body go into an error message.
const ERROR_TEXT_CHARS: usize = 2_000;

/// What replaces the API key wherever a provider echoes it back.
const REDACTED: &str = "<redacted>";

/// Proxy variables, in the order of ureq's `Proxy::try_from_env`.
const PROXY_VARS: &[&str] = &[
    "ALL_PROXY",
    "all_proxy",
    "HTTPS_PROXY",
    "https_proxy",
    "HTTP_PROXY",
    "http_proxy",
];

/// Lower-case markers of a 400 that reports a context-window overflow.
const CONTEXT_MARKERS: &[&str] = &[
    "context_length_exceeded",
    "context length",
    "context window",
    "maximum context",
    "prompt is too long",
    "input is too long",
    "reduce the length of the messages",
];

/// Lower-case markers of a 400 that reports an unsupported request parameter.
const UNSUPPORTED_MARKERS: &[&str] = &[
    "unsupported parameter",
    "unsupported_parameter",
    "unknown parameter",
    "unknown_parameter",
    "unrecognized request argument",
    "extra inputs are not permitted",
];

/// `model_kwargs` keys that are part of the request itself; setting them is a config error.
const RESERVED: &[&str] = &["model", "messages", "tools", "tool_choice", "stream", "n"];

/// `model_kwargs` keys that configure LiteLLM's client, not the provider; they are dropped.
const LITELLM_ONLY: &[&str] = &["drop_params"];

/// One Chat Completions request. It serializes as the JSON body: `model`, `messages`, then
/// the parameters.
#[derive(Clone, Debug, PartialEq)]
pub struct ChatRequest {
    /// The model name.
    pub model: String,
    /// The messages, each without its `extra` key.
    pub messages: Vec<Value>,
    /// Other body parameters (`temperature`, `max_tokens`, …).
    pub params: Map<String, Value>,
}

impl serde::Serialize for ChatRequest {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.params.len().saturating_add(2)))?;
        map.serialize_entry("model", &self.model)?;
        map.serialize_entry("messages", &self.messages)?;
        for (key, value) in &self.params {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

impl ChatRequest {
    /// The JSON body as a value (tests inspect it).
    pub fn body(&self) -> Value {
        serde_json::to_value(self).unwrap_or_default()
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

    /// `(prompt_tokens, completion_tokens)` from `usage`. Like pydantic's lax `int`, an
    /// integral float or a base-10 string is accepted.
    pub fn usage(&self) -> Option<(u64, u64)> {
        let usage = self.0.get("usage")?;
        Some((
            tokens(usage.get("prompt_tokens")?)?,
            tokens(usage.get("completion_tokens")?)?,
        ))
    }

    /// Checks the completion shape: a first choice whose message is from the assistant (or has
    /// no role) and whose content is text, `null` or absent. Anything else is an invalid
    /// response, retried like a transport error and never billed.
    pub fn validated(self) -> Result<Self, BackendError> {
        let invalid = |what: &str| Err(BackendError::InvalidResponse(what.to_owned()));
        let Some(choice) = self
            .0
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|c| c.first())
        else {
            return invalid("the response has no choices");
        };
        let Some(message) = choice.get("message").and_then(Value::as_object) else {
            return invalid("choices[0] has no message object");
        };
        match message.get("role") {
            None | Some(Value::Null) => {}
            Some(Value::String(role)) if role == "assistant" => {}
            Some(_) => return invalid("choices[0].message.role is not assistant"),
        }
        match message.get("content") {
            None | Some(Value::Null | Value::String(_)) => Ok(self),
            Some(_) => invalid("choices[0].message.content is not text"),
        }
    }
}

/// A token count: a non-negative integer, an integral float, or a base-10 string.
fn tokens(value: &Value) -> Option<u64> {
    match value {
        Value::Number(n) => n.as_u64().or_else(|| {
            let f = n.as_f64()?;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            (f.is_finite() && f >= 0.0 && f.fract() == 0.0 && f < 1.8e19).then_some(f as u64)
        }),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

/// At most [`ERROR_TEXT_CHARS`] characters of `text`.
fn bounded(text: String) -> String {
    match text.char_indices().nth(ERROR_TEXT_CHARS) {
        Some(_) => {
            let mut short: String = text.chars().take(ERROR_TEXT_CHARS).collect();
            short.push_str(&format!(" … ({} bytes in all)", text.len()));
            short
        }
        None => text,
    }
}

/// The part of an error body that classifies it: the `error` object's `code`, `type` and
/// `message` (or the top-level ones, as vLLM sends), else the whole body.
fn error_signal(body: &str) -> String {
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(body) else {
        return body.to_lowercase();
    };
    let fields = match map.get("error") {
        Some(Value::Object(error)) => error,
        Some(Value::String(text)) => return text.to_lowercase(),
        _ => &map,
    };
    ["code", "type", "message"]
        .iter()
        .filter_map(|key| fields.get(*key).and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
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
    /// A 3xx: redirects are not followed, so the history never goes to another host.
    #[error(
        "HTTP {status}: redirects are not followed; set model_kwargs.api_base to the final endpoint"
    )]
    Redirect {
        /// The status code.
        status: u16,
    },
    /// The request could not be sent or the response not read (retried).
    #[error("transport error: {0}")]
    Transport(String),
    /// A 2xx response that is not a valid completion (retried).
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
                | Self::Redirect { .. }
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
            Self::Status { .. } | Self::Redirect { .. } => "APIError",
            Self::Transport(_) => "APIConnectionError",
            Self::InvalidResponse(_) => "APIError",
        }
    }

    /// Maps an HTTP error status and its body (kept to [`ERROR_TEXT_CHARS`] characters). The
    /// status alone decides 3xx, 401, 403 and 404; a 400 is classified by its error fields.
    pub fn from_status(status: u16, body: String) -> Self {
        let signal = error_signal(&body);
        let has = |markers: &[&str]| markers.iter().any(|m| signal.contains(m));
        let body = bounded(body);
        match status {
            300..=399 => Self::Redirect { status },
            401 => Self::Authentication(body),
            403 => Self::PermissionDenied(body),
            404 => Self::NotFound(body),
            400 if has(CONTEXT_MARKERS) => Self::ContextWindowExceeded(body),
            400 if has(UNSUPPORTED_MARKERS) => Self::UnsupportedParams(body),
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

/// Where and how to send requests. The key and the proxy (which may hold credentials) never
/// leave this struct: `Debug` redacts them.
#[derive(Clone, PartialEq)]
pub struct Transport {
    /// A validated `http(s)://host[:port][/path][?query]`, for example
    /// `https://api.openai.com/v1`.
    pub base_url: String,
    /// The bearer token, if any.
    pub api_key: Option<String>,
    /// The variable the key is read from when `model_kwargs.api_key` is unset; named in the
    /// hint of an authentication error.
    pub key_env: String,
    /// The whole-request deadline.
    pub timeout: Duration,
    /// The proxy URI, from the host environment (`ALL_PROXY`, `HTTPS_PROXY`, `HTTP_PROXY`,
    /// minus `NO_PROXY` matches).
    pub proxy: Option<String>,
}

impl std::fmt::Debug for Transport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transport")
            .field("base_url", &self.base_url)
            .field("api_key", &self.api_key.as_ref().map(|_| REDACTED))
            .field("key_env", &self.key_env)
            .field("timeout", &self.timeout)
            .field("proxy", &self.proxy.as_ref().map(|_| "<configured>"))
            .finish()
    }
}

impl Transport {
    /// `{base path}/chat/completions`, with the base URL's query string kept.
    pub fn endpoint(&self) -> String {
        let (path, query) = match self.base_url.split_once('?') {
            Some((path, query)) => (path, Some(query)),
            None => (self.base_url.as_str(), None),
        };
        let mut url = format!("{}/chat/completions", path.trim_end_matches('/'));
        if let Some(query) = query {
            url.push('?');
            url.push_str(query);
        }
        url
    }

    /// `text` with every occurrence of the API key replaced.
    fn redact(&self, text: String) -> String {
        match &self.api_key {
            Some(key) if text.contains(key.as_str()) => text.replace(key.as_str(), REDACTED),
            _ => text,
        }
    }
}

/// The host of a validated base URL, for `NO_PROXY` matching.
fn url_host(base_url: &str) -> &str {
    let rest = base_url
        .strip_prefix("https://")
        .or_else(|| base_url.strip_prefix("http://"))
        .unwrap_or(base_url);
    let authority = rest.split(['/', '?']).next().unwrap_or_default();
    match authority.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or_default(),
        None => authority.split(':').next().unwrap_or_default(),
    }
}

/// Checks `http(s)://host…`: no credentials, no fragment, a non-empty host. The URL is not
/// echoed, as it may hold a secret.
fn validate_base_url(url: &str) -> Result<(), ModelError> {
    let error = |why: &str| Err(ModelError::Settings(format!("model_kwargs.api_base {why}")));
    let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    else {
        return error("must start with http:// or https://");
    };
    if url.contains('#') {
        return error("must not have a fragment");
    }
    let authority = rest.split(['/', '?']).next().unwrap_or_default();
    if authority.contains('@') {
        return error("must not contain credentials; use model_kwargs.api_key or api_key_env");
    }
    if url_host(url).is_empty() {
        return error("has no host");
    }
    Ok(())
}

/// The proxy for `host`: the first valid of [`PROXY_VARS`] in the host environment, unless
/// `NO_PROXY` matches (`*`, the host, or a parent domain with or without a leading dot).
fn proxy_for(env: &HostEnv, host: &str) -> Option<String> {
    let host = host.to_lowercase();
    let no_proxy = env.get("NO_PROXY").or_else(|| env.get("no_proxy"));
    let bypass = no_proxy.is_some_and(|list| {
        list.split(',').map(str::trim).any(|entry| {
            let entry = entry.trim_start_matches("*.").trim_start_matches('.');
            let entry = entry.to_lowercase();
            entry == "*"
                || (!entry.is_empty() && (host == entry || host.ends_with(&format!(".{entry}"))))
        })
    });
    if bypass {
        return None;
    }
    PROXY_VARS
        .iter()
        .filter_map(|name| env.get(name))
        .find(|uri| !uri.is_empty() && ureq::Proxy::new(uri).is_ok())
        .map(str::to_owned)
}

/// A key as a header value: control characters (a trailing newline from a secret file, say)
/// would break the request, so they are refused, naming only where the key came from.
fn checked_key(key: String, source: &str) -> Result<String, ModelError> {
    if key.chars().any(char::is_control) {
        return Err(ModelError::Settings(format!(
            "the API key from {source} contains control characters"
        )));
    }
    Ok(key)
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
        validate_base_url(&base_url)?;
        let key_env = text("api_key_env")?.unwrap_or_else(|| "OPENAI_API_KEY".to_owned());
        // The key's value is never put in an error.
        let literal = match kwargs.get("api_key") {
            None | Some(Value::Null) => None,
            Some(Value::String(key)) if key.is_empty() => None,
            Some(Value::String(key)) => Some(checked_key(key.clone(), "model_kwargs.api_key")?),
            Some(_) => {
                return Err(ModelError::Settings(
                    "model_kwargs.api_key must be a string".to_owned(),
                ));
            }
        };
        let api_key = match literal {
            Some(key) => Some(key),
            None => match host.get(&key_env).filter(|k| !k.is_empty()) {
                Some(key) => Some(checked_key(key.to_owned(), &key_env)?),
                None => None,
            },
        };
        let timeout = match kwargs.get("timeout") {
            None | Some(Value::Null) => DEFAULT_TIMEOUT,
            Some(value) => value
                .as_f64()
                .filter(|seconds| *seconds > 0.0)
                .and_then(|seconds| Duration::try_from_secs_f64(seconds).ok())
                .ok_or_else(|| {
                    ModelError::Settings(format!(
                        "model_kwargs.timeout must be a positive number of seconds, found {value}"
                    ))
                })?,
        };
        let proxy = proxy_for(host, url_host(&base_url));
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
                key_env,
                timeout,
                proxy,
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
    /// stay readable for the error mapping; redirects are not followed; the proxy is the one
    /// the transport resolved from the host environment, never the process environment.
    pub fn new(transport: Transport) -> Self {
        let proxy = transport
            .proxy
            .as_deref()
            .and_then(|uri| ureq::Proxy::new(uri).ok());
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(transport.timeout))
            .max_redirects(0)
            .proxy(proxy)
            .build()
            .new_agent();
        Self { agent, transport }
    }

    /// The error for a non-2xx response: a bounded, lossy read of the body (an unreadable
    /// body is empty, so the status still decides), the key redacted, and a hint on 401.
    fn status_error(&self, status: u16, body: &mut ureq::Body) -> BackendError {
        let bytes = body
            .with_config()
            .limit(ERROR_BODY_BYTES)
            .read_to_vec()
            .unwrap_or_default();
        let text = self
            .transport
            .redact(String::from_utf8_lossy(&bytes).into_owned());
        match BackendError::from_status(status, text) {
            BackendError::Authentication(mut message) => {
                message.push_str(&format!(
                    " Set the {} environment variable (for example in the global .env file) or \
                     model_kwargs.api_key.",
                    self.transport.key_env
                ));
                BackendError::Authentication(message)
            }
            other => other,
        }
    }
}

impl ChatBackend for OpenAiCompatBackend {
    fn complete(&self, request: &ChatRequest) -> Result<ChatResponse, BackendError> {
        let mut call = self
            .agent
            .post(&self.transport.endpoint())
            .header("Content-Type", "application/json");
        if let Some(key) = &self.transport.api_key {
            call = call.header("Authorization", &format!("Bearer {key}"));
        }
        let body =
            serde_json::to_vec(request).map_err(|e| BackendError::Transport(e.to_string()))?;
        let mut response = call
            .send(body.as_slice())
            .map_err(|e| BackendError::Transport(self.transport.redact(e.to_string())))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(self.status_error(status, response.body_mut()));
        }
        let text = response
            .body_mut()
            .read_to_string()
            .map_err(|e| BackendError::Transport(e.to_string()))?;
        serde_json::from_str(&text)
            .map(ChatResponse::new)
            .map_err(|e| BackendError::InvalidResponse(e.to_string()))
    }
}
