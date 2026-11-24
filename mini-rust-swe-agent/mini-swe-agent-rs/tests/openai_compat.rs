//! Transport settings, HTTP error mapping and response validation of the OpenAI-compatible
//! backend (phase P8; the review fixes recorded in spec/reviews/p8/FINDINGS.md).

use std::ffi::OsString;
use std::time::Duration;

use mini_swe_agent_rs::error::ModelError;
use mini_swe_agent_rs::host_env::HostEnv;
use mini_swe_agent_rs::model::openai_compat::{
    BackendError, ChatResponse, DEFAULT_TIMEOUT, Settings,
};
use serde_json::{Map, Value, json};

const KEY: &str = "sk-sentinel-0123456789";

fn host(pairs: &[(&str, &str)]) -> HostEnv {
    HostEnv::from_vars(
        pairs
            .iter()
            .map(|(k, v)| (OsString::from(k), OsString::from(v))),
    )
}

/// Test helper: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::unwrap_used)]
fn kwargs(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

fn settings(value: Value, env: &HostEnv) -> Result<Settings, ModelError> {
    Settings::from_model_kwargs(&kwargs(value), env)
}

#[test]
fn timeout_defaults_and_validation() {
    let env = host(&[]);
    let default = settings(json!({}), &env).unwrap();
    assert_eq!(default.transport.timeout, DEFAULT_TIMEOUT);
    assert_eq!(DEFAULT_TIMEOUT, Duration::from_secs(600));
    let set = settings(json!({"timeout": 2.5}), &env).unwrap();
    assert_eq!(set.transport.timeout, Duration::from_millis(2500));
    for bad in [json!(0), json!(0.0), json!(-1), json!("10"), json!(1e300)] {
        assert!(
            matches!(
                settings(json!({ "timeout": bad }), &env),
                Err(ModelError::Settings(_))
            ),
            "{bad}"
        );
    }
}

#[test]
fn api_key_sources_and_validation() {
    let env = host(&[("OPENAI_API_KEY", KEY), ("OTHER_KEY", "sk-other")]);
    let from_env = settings(json!({}), &env).unwrap().transport;
    assert_eq!(from_env.api_key.as_deref(), Some(KEY));
    assert_eq!(from_env.key_env, "OPENAI_API_KEY");
    let other = settings(json!({"api_key_env": "OTHER_KEY"}), &env)
        .unwrap()
        .transport;
    assert_eq!(
        (other.api_key.as_deref(), other.key_env.as_str()),
        (Some("sk-other"), "OTHER_KEY")
    );
    // An empty literal key is unset, as an empty variable is.
    let empty = settings(json!({"api_key": ""}), &env).unwrap().transport;
    assert_eq!(empty.api_key.as_deref(), Some(KEY));
    // Errors about the key never print it.
    for bad in [json!(987_654_321), json!("sk-bad\r\nX-Injected: 1")] {
        let error = settings(json!({ "api_key": bad }), &env).unwrap_err();
        let text = error.to_string();
        assert!(matches!(error, ModelError::Settings(_)), "{text}");
        assert!(
            !text.contains("987654321") && !text.contains("sk-bad"),
            "{text}"
        );
    }
    let debug = format!(
        "{:?}",
        settings(json!({"api_key": KEY}), &env).unwrap().transport
    );
    assert!(!debug.contains(KEY), "{debug}");
}

#[test]
fn endpoint_is_built_and_validated_once() {
    let env = host(&[]);
    let endpoint =
        |base: &str| settings(json!({ "api_base": base }), &env).map(|s| s.transport.endpoint());
    assert_eq!(
        endpoint("https://api.example.com/v1").unwrap(),
        "https://api.example.com/v1/chat/completions"
    );
    assert_eq!(
        endpoint("https://api.example.com/v1/").unwrap(),
        "https://api.example.com/v1/chat/completions"
    );
    assert_eq!(
        endpoint("https://host/openai/v1?api-version=2024-06-01").unwrap(),
        "https://host/openai/v1/chat/completions?api-version=2024-06-01"
    );
    assert_eq!(
        settings(json!({}), &env).unwrap().transport.endpoint(),
        "https://api.openai.com/v1/chat/completions"
    );
    for bad in [
        "",
        "api.example.com/v1",
        "ftp://host/v1",
        "http://",
        "http:///v1",
        "https://user:secret@host/v1",
        "https://host/v1#frag",
    ] {
        let error = endpoint(bad).unwrap_err();
        assert!(matches!(error, ModelError::Settings(_)), "{bad}");
        assert!(!error.to_string().contains("secret"), "{error}");
    }
}

#[test]
fn proxy_comes_from_the_host_env() {
    let base = json!({"api_base": "https://api.example.com/v1"});
    let proxy = |pairs: &[(&str, &str)]| {
        settings(base.clone(), &host(pairs))
            .unwrap()
            .transport
            .proxy
    };
    assert_eq!(proxy(&[]), None);
    assert_eq!(
        proxy(&[("HTTPS_PROXY", "http://proxy:3128")]).as_deref(),
        Some("http://proxy:3128")
    );
    assert_eq!(
        proxy(&[("https_proxy", "http://lower:3128")]).as_deref(),
        Some("http://lower:3128")
    );
    // ALL_PROXY wins, as in ureq's `Proxy::try_from_env`.
    assert_eq!(
        proxy(&[("HTTP_PROXY", "http://b:1"), ("ALL_PROXY", "http://a:1")]).as_deref(),
        Some("http://a:1")
    );
    for no_proxy in [
        "api.example.com",
        ".example.com",
        "example.com",
        "*",
        "x, api.example.com",
    ] {
        assert_eq!(
            proxy(&[("HTTPS_PROXY", "http://proxy:3128"), ("NO_PROXY", no_proxy)]),
            None,
            "{no_proxy}"
        );
    }
    assert_eq!(
        proxy(&[
            ("HTTPS_PROXY", "http://proxy:3128"),
            ("no_proxy", "other.com")
        ])
        .as_deref(),
        Some("http://proxy:3128")
    );
    let debug = format!(
        "{:?}",
        settings(
            base.clone(),
            &host(&[("HTTPS_PROXY", "http://user:pw@proxy:3128")])
        )
        .unwrap()
        .transport
    );
    assert!(!debug.contains("pw@"), "{debug}");
}

#[test]
fn http_errors_map_from_structured_bodies() {
    let classify = |status: u16, body: Value| BackendError::from_status(status, body.to_string());
    let anthropic = json!({"type": "error", "error": {"type": "invalid_request_error", "message": "prompt is too long: 250000 tokens > 200000 maximum"}});
    assert!(matches!(
        classify(400, anthropic),
        BackendError::ContextWindowExceeded(_)
    ));
    let openai = json!({"error": {"message": "This model's maximum context length is 8192 tokens.", "code": "context_length_exceeded"}});
    assert!(matches!(
        classify(400, openai),
        BackendError::ContextWindowExceeded(_)
    ));
    let unknown = json!({"error": {"message": "Unknown parameter: 'foo'.", "type": "invalid_request_error", "code": "unknown_parameter"}});
    assert!(matches!(
        classify(400, unknown),
        BackendError::UnsupportedParams(_)
    ));
    let vllm = json!({"object": "error", "message": "Extra inputs are not permitted", "type": "BadRequestError"});
    assert!(matches!(
        classify(400, vllm),
        BackendError::UnsupportedParams(_)
    ));
    // Only the error fields of a JSON body decide, not text quoted elsewhere.
    let quoted = json!({"error": {"message": "Invalid image URL"}, "echo": "the context window"});
    assert!(matches!(
        classify(400, quoted),
        BackendError::Status { status: 400, .. }
    ));
    let redirect = BackendError::from_status(307, String::new());
    assert!(redirect.aborts(), "a redirect is never followed or retried");
    for status in [401, 403, 404] {
        assert!(BackendError::from_status(status, String::new()).aborts());
    }
    assert!(!BackendError::from_status(429, String::new()).aborts());
    // Bodies are bounded in messages (2,000 characters plus a short suffix).
    let long = BackendError::from_status(500, "é".repeat(10_000)).to_string();
    assert!(long.chars().count() < 2_100, "{}", long.chars().count());
    assert!(long.ends_with("(20000 bytes in all)"), "{long}");
}

#[test]
fn completions_are_validated() {
    let ok = |message: Value| {
        ChatResponse::new(json!({"choices": [{"message": message, "finish_reason": "stop"}]}))
            .validated()
    };
    assert!(ok(json!({"role": "assistant", "content": "x"})).is_ok());
    assert!(ok(json!({"role": "assistant", "content": null})).is_ok());
    assert!(ok(json!({"role": "assistant"})).is_ok());
    assert!(ok(json!({"content": "no role"})).is_ok());
    for bad in [
        json!({"role": "user", "content": "x"}),
        json!({"role": "assistant", "content": [{"type": "text", "text": "x"}]}),
        json!("not an object"),
    ] {
        assert!(
            matches!(ok(bad.clone()), Err(BackendError::InvalidResponse(_))),
            "{bad}"
        );
    }
    for body in [
        json!({"choices": []}),
        json!({"error": {"message": "x"}}),
        json!([1]),
        json!({"choices": [{}]}),
    ] {
        assert!(
            matches!(
                ChatResponse::new(body.clone()).validated(),
                Err(BackendError::InvalidResponse(_))
            ),
            "{body}"
        );
    }
}

#[test]
fn usage_accepts_integral_numbers_and_digit_strings() {
    let usage = |usage: Value| ChatResponse::new(json!({"usage": usage})).usage();
    assert_eq!(
        usage(json!({"prompt_tokens": 10, "completion_tokens": 5})),
        Some((10, 5))
    );
    assert_eq!(
        usage(json!({"prompt_tokens": 10.0, "completion_tokens": "5"})),
        Some((10, 5))
    );
    assert_eq!(
        usage(json!({"prompt_tokens": 10.5, "completion_tokens": 5})),
        None
    );
    assert_eq!(
        usage(json!({"prompt_tokens": -1, "completion_tokens": 5})),
        None
    );
    assert_eq!(
        usage(json!({"prompt_tokens": "ten", "completion_tokens": 5})),
        None
    );
}
