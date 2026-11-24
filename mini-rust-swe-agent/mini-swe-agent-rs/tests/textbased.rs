//! The text-based model over a canned `ChatBackend` (spec/migration-plan.md §3.8, phase P8): the
//! port of `litellm_model.py:81-106` with `litellm_textbased_model.py`, without HTTP.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::sync::{Arc, Mutex};

use mini_swe_agent_rs::clock::{Clock, ManualClock};
use mini_swe_agent_rs::config::{CostTracking, TextbasedModelConfig};
use mini_swe_agent_rs::error::{AgentError, InterruptKind, ModelError, StepError};
use mini_swe_agent_rs::host_env::HostEnv;
use mini_swe_agent_rs::message::{Message, Role};
use mini_swe_agent_rs::model::Model;
use mini_swe_agent_rs::model::openai_compat::{
    BackendError, ChatBackend, ChatRequest, ChatResponse,
};
use mini_swe_agent_rs::model::retry::backoff;
use mini_swe_agent_rs::model::stats::GlobalModelStats;
use mini_swe_agent_rs::model::textbased::TextbasedModel;
use serde_json::{Map, Value, json};

const FENCE: &str = "```mswea_bash_command";

/// A backend that replays canned replies and records every request body.
#[derive(Debug, Default)]
struct Canned {
    replies: Mutex<VecDeque<Result<Value, BackendError>>>,
    requests: Mutex<Vec<Value>>,
}

impl Canned {
    fn new(replies: Vec<Result<Value, BackendError>>) -> Arc<Self> {
        Arc::new(Self {
            replies: Mutex::new(replies.into()),
            requests: Mutex::new(Vec::new()),
        })
    }

    /// Test helper: clippy.toml exempts only `#[test]` functions.
    #[allow(clippy::unwrap_used)]
    fn requests(&self) -> Vec<Value> {
        self.requests.lock().unwrap().clone()
    }
}

impl ChatBackend for Canned {
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    fn complete(&self, request: &ChatRequest) -> Result<ChatResponse, BackendError> {
        self.requests.lock().unwrap().push(request.body());
        let reply = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("no canned reply left");
        reply.map(ChatResponse::new)
    }
}

/// A Chat Completions response with usage (10 prompt and 5 completion tokens).
fn reply(content: Value, finish_reason: &str) -> Value {
    json!({
        "id": "chatcmpl-1", "object": "chat.completion", "created": 1, "model": "test-model",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": content, "refusal": null}, "finish_reason": finish_reason}],
        "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
    })
}

fn action(command: &str) -> Value {
    json!(format!("THOUGHT: go.\n\n{FENCE}\n{command}\n```"))
}

fn host(pairs: &[(&str, &str)]) -> HostEnv {
    HostEnv::from_vars(
        pairs
            .iter()
            .map(|(k, v)| (OsString::from(k), OsString::from(v))),
    )
}

/// A registry with prices for `test-model`: 10 × 0.001 + 5 × 0.002 = 0.02 per reply.
/// Test helper: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::unwrap_used)]
fn registry(dir: &tempfile::TempDir) -> String {
    let path = dir.path().join("registry.json");
    let prices = json!({"test-model": {"input_cost_per_token": 0.001, "output_cost_per_token": 0.002, "litellm_provider": "openai", "mode": "chat"}});
    std::fs::write(&path, prices.to_string()).unwrap();
    path.to_str().unwrap().to_owned()
}

/// Test helper: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
fn config(extra: Value, env: &HostEnv) -> TextbasedModelConfig {
    let mut section = json!({"model_name": "test-model", "model_kwargs": {"drop_params": true, "temperature": 0.0}});
    for (key, value) in extra.as_object().unwrap() {
        section[key] = value.clone();
    }
    TextbasedModelConfig::from_section(&section, env).unwrap()
}

struct Setup {
    model: TextbasedModel<Arc<Canned>>,
    backend: Arc<Canned>,
    stats: Arc<GlobalModelStats>,
    clock: Arc<ManualClock>,
}

/// Test helper: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::unwrap_used)]
fn setup(replies: Vec<Result<Value, BackendError>>, extra: Value, env: &HostEnv) -> Setup {
    let backend = Canned::new(replies);
    let stats = Arc::new(GlobalModelStats::new(0.0, 0));
    let clock = Arc::new(ManualClock::new(100.0));
    let dyn_clock: Arc<dyn Clock> = clock.clone();
    let model = TextbasedModel::new(
        config(extra, env),
        backend.clone(),
        stats.clone(),
        dyn_clock,
        env,
    )
    .unwrap();
    Setup {
        model,
        backend,
        stats,
        clock,
    }
}

fn history() -> Vec<Message> {
    let extra: Map<String, Value> = [("secret".to_owned(), json!("not for the provider"))]
        .into_iter()
        .collect();
    vec![
        Message::new(Role::System, "You are a helpful assistant."),
        Message::new(Role::User, "List the files.").with_extra(extra),
    ]
}

#[test]
fn success_builds_the_assistant_message() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(
        vec![Ok(reply(action("ls -la"), "stop"))],
        json!({"litellm_model_registry": registry(&dir)}),
        &host(&[]),
    );
    let message = s.model.query(&history()).unwrap();
    assert_eq!(message.role, Role::Assistant);
    assert!(message.content.contains("ls -la"));
    let extra = message.extra.as_ref().unwrap();
    assert_eq!(extra["actions"], json!([{"command": "ls -la"}]));
    assert_eq!(extra["cost"], json!(0.02));
    assert_eq!(extra["response"], reply(action("ls -la"), "stop"));
    assert_eq!(extra["timestamp"], json!(100.0));
    let keys: Vec<&String> = extra.keys().collect();
    assert_eq!(keys, ["actions", "response", "cost", "timestamp"]);
    // Provider fields of the reply are kept on the message.
    assert_eq!(
        serde_json::to_value(&message).unwrap()["refusal"],
        Value::Null
    );
    assert_eq!((s.stats.n_calls(), s.stats.cost()), (1, 0.02));

    let request = &s.backend.requests()[0];
    assert_eq!(request["model"], "test-model");
    assert_eq!(request["temperature"], 0.0);
    assert!(
        request.get("drop_params").is_none(),
        "LiteLLM-only keys are stripped"
    );
    assert_eq!(
        request["messages"],
        json!([
            {"role": "system", "content": "You are a helpful assistant."},
            {"role": "user", "content": "List the files."}
        ]),
        "every key but `extra` is sent"
    );
}

#[test]
fn the_previous_assistant_message_is_echoed_without_extra() {
    let dir = tempfile::tempdir().unwrap();
    let replies = vec![
        Ok(reply(action("ls"), "stop")),
        Ok(reply(action("pwd"), "stop")),
    ];
    let mut s = setup(
        replies,
        json!({"litellm_model_registry": registry(&dir)}),
        &host(&[]),
    );
    let mut messages = history();
    messages.push(s.model.query(&messages).unwrap());
    s.model.query(&messages).unwrap();
    let echoed = &s.backend.requests()[1]["messages"][2];
    assert_eq!(echoed["role"], "assistant");
    assert!(echoed.as_object().unwrap().contains_key("refusal"));
    assert!(echoed.get("extra").is_none());
}

#[test]
fn format_error_keeps_cost_and_response() {
    let dir = tempfile::tempdir().unwrap();
    for (content, n_actions) in [
        (json!("no action here"), 0),
        (json!(format!("{FENCE}\na\n```\n{FENCE}\nb\n```")), 2),
        (Value::Null, 0),
    ] {
        let raw = reply(content.clone(), "length");
        let mut s = setup(
            vec![Ok(raw.clone())],
            json!({"litellm_model_registry": registry(&dir)}),
            &host(&[]),
        );
        let Err(StepError::Interrupt(interrupt)) = s.model.query(&history()) else {
            panic!("{content}: expected a format error")
        };
        assert_eq!(interrupt.kind(), InterruptKind::FormatError);
        let extra = interrupt.head().extra.as_ref().unwrap();
        assert_eq!(extra["n_actions"], n_actions);
        assert_eq!(
            extra["cost"],
            json!(0.02),
            "the cost is persisted on a format error"
        );
        assert_eq!(
            extra["response"], raw,
            "the response is persisted on a format error"
        );
        assert_eq!(
            extra["model_response"],
            content.as_str().unwrap_or_default()
        );
        assert_eq!(s.stats.n_calls(), 1, "a format error is charged");
    }
}

#[test]
fn retry_then_success_waits_with_backoff() {
    let dir = tempfile::tempdir().unwrap();
    let replies = vec![
        Err(BackendError::Status {
            status: 500,
            body: "boom".into(),
        }),
        Err(BackendError::Transport("connection reset".into())),
        Ok(reply(action("ls"), "stop")),
    ];
    let mut s = setup(
        replies,
        json!({"litellm_model_registry": registry(&dir)}),
        &host(&[]),
    );
    s.model.query(&history()).unwrap();
    assert_eq!(s.backend.requests().len(), 3);
    assert_eq!(
        s.clock.now(),
        100.0 + 4.0 + 4.0,
        "tenacity's wait_exponential(min=4, max=60)"
    );
}

#[test]
fn abort_errors_are_never_retried() {
    for error in [
        BackendError::Authentication("bad key".into()),
        BackendError::PermissionDenied("no".into()),
        BackendError::NotFound("no model".into()),
        BackendError::ContextWindowExceeded("too long".into()),
        BackendError::UnsupportedParams("bad param".into()),
    ] {
        let mut s = setup(
            vec![Err(error), Ok(reply(action("ls"), "stop"))],
            json!({}),
            &host(&[]),
        );
        assert!(matches!(
            s.model.query(&history()),
            Err(StepError::Fatal(AgentError::Model(ModelError::Backend(_))))
        ));
        assert_eq!(s.backend.requests().len(), 1);
        assert_eq!(s.clock.now(), 100.0);
        assert_eq!(s.stats.n_calls(), 0);
    }
}

#[test]
fn retries_stop_after_the_configured_attempts() {
    let env = host(&[("MSWEA_MODEL_RETRY_STOP_AFTER_ATTEMPT", "2")]);
    let replies = vec![
        Err(BackendError::Status {
            status: 503,
            body: String::new(),
        }),
        Err(BackendError::Status {
            status: 503,
            body: String::new(),
        }),
        Ok(reply(action("ls"), "stop")),
    ];
    let mut s = setup(replies, json!({}), &env);
    assert!(s.model.query(&history()).is_err());
    assert_eq!(s.backend.requests().len(), 2);
}

#[test]
fn backoff_follows_tenacity() {
    let seconds: Vec<u64> = [1, 2, 3, 4, 5, 6, 7, 8, 64, u32::MAX]
        .iter()
        .map(|n| backoff(*n).as_secs())
        .collect();
    assert_eq!(seconds, [4, 4, 4, 8, 16, 32, 60, 60, 60, 60]);
}

#[test]
fn cost_tracking_errors_and_ignore_errors() {
    let dir = tempfile::tempdir().unwrap();
    // No registry: the cost cannot be computed, which is fatal and charges nothing.
    let mut s = setup(vec![Ok(reply(action("ls"), "stop"))], json!({}), &host(&[]));
    let error = s.model.query(&history()).unwrap_err();
    let StepError::Fatal(AgentError::Model(error)) = error else {
        panic!("{error:?}")
    };
    assert!(matches!(error, ModelError::CostTracking(_)));
    assert_eq!(error.exit_status(), "RuntimeError");
    assert!(
        error.to_string().contains("ignore_errors"),
        "the message says how to proceed"
    );
    assert_eq!(s.stats.n_calls(), 0);

    // ignore_errors: cost 0.0, and the call still goes through `add(0.0)`.
    let mut s = setup(
        vec![Ok(reply(action("ls"), "stop"))],
        json!({"cost_tracking": "ignore_errors"}),
        &host(&[]),
    );
    let message = s.model.query(&history()).unwrap();
    assert_eq!(message.extra.unwrap()["cost"], json!(0.0));
    assert_eq!((s.stats.n_calls(), s.stats.cost()), (1, 0.0));

    // Missing usage and a provider prefix.
    let mut no_usage = reply(action("ls"), "stop");
    no_usage.as_object_mut().unwrap().remove("usage");
    let mut s = setup(
        vec![Ok(no_usage)],
        json!({"litellm_model_registry": registry(&dir)}),
        &host(&[]),
    );
    assert!(s.model.query(&history()).is_err());
    let mut s = setup(
        vec![Ok(reply(action("ls"), "stop"))],
        json!({"model_name": "openai/test-model", "litellm_model_registry": registry(&dir)}),
        &host(&[]),
    );
    assert_eq!(
        s.model.query(&history()).unwrap().extra.unwrap()["cost"],
        json!(0.02)
    );
}

#[test]
fn global_limits_apply_after_counting() {
    let backend = Canned::new(vec![Ok(reply(action("ls"), "stop"))]);
    let stats = Arc::new(GlobalModelStats::new(0.0, 1));
    let clock: Arc<dyn Clock> = Arc::new(ManualClock::new(0.0));
    let env = host(&[]);
    let mut model = TextbasedModel::new(
        config(json!({"cost_tracking": "ignore_errors"}), &env),
        backend,
        stats.clone(),
        clock,
        &env,
    )
    .unwrap();
    assert!(matches!(
        model.query(&history()),
        Err(StepError::Fatal(AgentError::Model(
            ModelError::GlobalLimitExceeded { .. }
        )))
    ));
    assert_eq!(stats.n_calls(), 1);
}

#[test]
fn settings_split_and_secrets() {
    let env = host(&[("MY_KEY", "sk-sentinel-123")]);
    for reserved in ["model", "messages", "tools", "tool_choice", "stream", "n"] {
        let section = json!({"model_name": "m", "model_kwargs": {reserved: 1}});
        let config = TextbasedModelConfig::from_section(&section, &env).unwrap();
        let clock: Arc<dyn Clock> = Arc::new(ManualClock::new(0.0));
        let built = TextbasedModel::new(
            config,
            Canned::new(vec![]),
            Arc::new(GlobalModelStats::new(0.0, 0)),
            clock,
            &env,
        );
        assert!(matches!(built, Err(ModelError::Settings(_))), "{reserved}");
    }
    // A literal key is redacted wherever the config is written.
    let mut s = setup(
        vec![],
        json!({"model_kwargs": {"api_key": "sk-sentinel-123", "api_base": "http://x/v1"}}),
        &env,
    );
    let dumped =
        s.model.serialize().unwrap().to_string() + &s.model.template_vars().unwrap().to_string();
    assert!(!dumped.contains("sk-sentinel-123"), "{dumped}");
    assert!(dumped.contains("<redacted>"));
    let _ = &mut s;
}

#[test]
fn cost_tracking_is_typed() {
    let env = host(&[("MSWEA_COST_TRACKING", "ignore_errors")]);
    assert_eq!(
        config(json!({}), &env).cost_tracking,
        CostTracking::IgnoreErrors
    );
}
