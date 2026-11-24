//! The OpenAI-compatible backend against a fake HTTP server (spec/migration-plan.md §3.15 item 2,
//! phase P8): the wire JSON built from the shipped `mini_textbased.yaml`, the auth header, the
//! error mapping, and an end-to-end CLI run that must not leak the API key.

mod support;

use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use mini_swe_agent_rs::clock::{Clock, ManualClock};
use mini_swe_agent_rs::config::{self, ModelChoice};
use mini_swe_agent_rs::error::{AgentError, ModelError, StepError};
use mini_swe_agent_rs::host_env::HostEnv;
use mini_swe_agent_rs::message::{Message, Role};
use mini_swe_agent_rs::model::Model;
use mini_swe_agent_rs::model::openai_compat::{BackendError, OpenAiCompatBackend, Settings};
use mini_swe_agent_rs::model::stats::GlobalModelStats;
use mini_swe_agent_rs::model::textbased::TextbasedModel;
use serde_json::{Value, json};
use support::cli::{BIN, Sandbox, stderr};

const KEY: &str = "sk-sentinel-0123456789";

/// One captured request.
#[derive(Clone, Debug)]
struct Request {
    line: String,
    headers: Vec<(String, String)>,
    body: Value,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// A server that answers each connection with the next canned `(status, body)`.
struct FakeServer {
    url: String,
    requests: Arc<Mutex<Vec<Request>>>,
    handle: Option<JoinHandle<()>>,
}

impl FakeServer {
    /// Test helper: clippy.toml exempts only `#[test]` functions.
    #[allow(clippy::unwrap_used)]
    fn start(replies: Vec<(u16, Value)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = requests.clone();
        let handle = std::thread::spawn(move || {
            for (status, body) in replies {
                let Ok((stream, _)) = listener.accept() else {
                    return;
                };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let mut headers = Vec::new();
                loop {
                    let mut header = String::new();
                    reader.read_line(&mut header).unwrap();
                    let header = header.trim_end();
                    if header.is_empty() {
                        break;
                    }
                    let (name, value) = header.split_once(':').unwrap();
                    headers.push((name.trim().to_owned(), value.trim().to_owned()));
                }
                let length: usize = headers
                    .iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case("content-length"))
                    .map_or(0, |(_, v)| v.parse().unwrap());
                let mut raw = vec![0; length];
                reader.read_exact(&mut raw).unwrap();
                seen.lock().unwrap().push(Request {
                    line: line.trim_end().to_owned(),
                    headers,
                    body: serde_json::from_slice(&raw).unwrap(),
                });
                let text = body.to_string();
                let mut stream = stream;
                write!(
                    stream,
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
                    text.len()
                )
                .unwrap();
            }
        });
        Self {
            url,
            requests,
            handle: Some(handle),
        }
    }

    /// Test helper: clippy.toml exempts only `#[test]` functions.
    #[allow(clippy::unwrap_used)]
    fn requests(&mut self) -> Vec<Request> {
        if let Some(handle) = self.handle.take() {
            handle.join().unwrap();
        }
        self.requests.lock().unwrap().clone()
    }
}

fn reply(content: &str) -> Value {
    json!({
        "id": "chatcmpl-1", "object": "chat.completion", "created": 1, "model": "test-model",
        "choices": [{"index": 0, "finish_reason": "stop", "message": {
            "role": "assistant", "content": content, "refusal": null, "annotations": []
        }}],
        "usage": {"prompt_tokens": 3, "completion_tokens": 2, "total_tokens": 5}
    })
}

fn action(command: &str) -> String {
    format!("THOUGHT: go.\n\n```mswea_bash_command\n{command}\n```")
}

fn host(pairs: &[(&str, &str)]) -> HostEnv {
    HostEnv::from_vars(
        pairs
            .iter()
            .map(|(k, v)| (OsString::from(k), OsString::from(v))),
    )
}

/// The shipped `mini_textbased.yaml` model section, pointed at `url`, through the config path.
/// Test helper: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
fn shipped_model(url: &str, env: &HostEnv) -> TextbasedModel<OpenAiCompatBackend> {
    let source = config::ConfigSource::Builtin("mini_textbased.yaml".to_owned());
    let mut raw =
        mini_swe_agent_rs::yaml::load_str(&config::read_source(&source).unwrap()).unwrap();
    raw["model"]["model_name"] = json!("test-model");
    raw["model"]["model_kwargs"]["api_base"] = json!(url);
    raw["model"]["cost_tracking"] = json!("ignore_errors");
    let ModelChoice::Textbased(model_config) = config::model_choice(&raw["model"], env).unwrap()
    else {
        panic!("the shipped config selects the text-based model")
    };
    let settings = Settings::from_model_kwargs(&model_config.model_kwargs, env).unwrap();
    let clock: Arc<dyn Clock> = Arc::new(ManualClock::new(0.0));
    let stats = Arc::new(GlobalModelStats::new(0.0, 0));
    TextbasedModel::new(
        model_config,
        OpenAiCompatBackend::new(settings.transport),
        stats,
        clock,
        env,
    )
    .unwrap()
}

#[test]
fn request_from_the_shipped_config() {
    let mut server = FakeServer::start(vec![
        (200, reply(&action("ls"))),
        (200, reply(&action("pwd"))),
    ]);
    let env = host(&[("OPENAI_API_KEY", KEY)]);
    let mut model = shipped_model(&server.url, &env);
    let mut messages = vec![
        Message::new(Role::System, "system"),
        Message::new(Role::User, "task"),
    ];
    messages.push(model.query(&messages).unwrap());
    model.query(&messages).unwrap();

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    let first = &requests[0];
    assert_eq!(first.line, "POST /v1/chat/completions HTTP/1.1");
    assert_eq!(
        first.header("authorization"),
        Some(format!("Bearer {KEY}").as_str())
    );
    assert!(
        first
            .header("content-type")
            .is_some_and(|v| v.starts_with("application/json"))
    );
    assert_eq!(first.body["model"], "test-model");
    assert_eq!(
        first.body["messages"],
        json!([{"role": "system", "content": "system"}, {"role": "user", "content": "task"}])
    );
    assert!(first.body.get("drop_params").is_none(), "{}", first.body);
    // The provider fields of the previous assistant message are echoed, without `extra`.
    let echoed = &requests[1].body["messages"][2];
    assert_eq!(echoed["refusal"], Value::Null);
    assert_eq!(echoed["annotations"], json!([]));
    assert!(echoed.get("extra").is_none());
}

#[test]
fn error_statuses_map_and_abort() {
    let context = json!({"error": {"message": "This model's maximum context length is 8192 tokens.", "type": "invalid_request_error", "code": "context_length_exceeded"}});
    for (status, body, expected) in [
        (400, context, "ContextWindowExceededError"),
        (
            401,
            json!({"error": {"message": "Incorrect API key"}}),
            "AuthenticationError",
        ),
        (
            404,
            json!({"error": {"message": "model not found"}}),
            "NotFoundError",
        ),
    ] {
        let mut server = FakeServer::start(vec![(status, body)]);
        let mut model = shipped_model(&server.url, &host(&[("OPENAI_API_KEY", KEY)]));
        let result = model.query(&[Message::new(Role::User, "x")]);
        let Err(StepError::Fatal(AgentError::Model(ModelError::Backend(error)))) = result else {
            panic!("{status}: {result:?}")
        };
        assert_eq!(error.exit_status(), expected);
        assert!(matches!(
            error,
            BackendError::ContextWindowExceeded(_)
                | BackendError::Authentication(_)
                | BackendError::NotFound(_)
        ));
        assert_eq!(server.requests().len(), 1, "{status} is never retried");
    }
}

#[test]
fn cli_run_against_the_fake_server_keeps_the_key_secret() {
    let mut server = FakeServer::start(vec![(
        200,
        reply(&action("echo COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT")),
    )]);
    let mut sandbox = Sandbox::new();
    sandbox.env.insert("OPENAI_API_KEY".into(), KEY.into());
    let args: Vec<String> = [
        "-m",
        "test-model",
        "-c",
        "mini_textbased",
        "-c",
        &format!("model.model_kwargs.api_base={}", server.url),
        "-c",
        "model.cost_tracking=ignore_errors",
        "-t",
        "Say hi",
        "-o",
        &sandbox.out("traj.json"),
    ]
    .iter()
    .map(|a| (*a).to_owned())
    .collect();
    let output = sandbox.run(Path::new(BIN), &args, None);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let trajectory = std::fs::read_to_string(sandbox.root.join("out/traj.json")).unwrap();
    assert!(trajectory.contains("\"exit_status\": \"Submitted\""));
    for (place, text) in [
        ("stderr", stderr(&output)),
        (
            "stdout",
            String::from_utf8_lossy(&output.stdout).into_owned(),
        ),
        ("trajectory", trajectory),
    ] {
        assert!(!text.contains(KEY), "the API key leaked into {place}");
    }
    let requests = server.requests();
    assert_eq!(
        requests[0].header("authorization"),
        Some(format!("Bearer {KEY}").as_str())
    );
}

#[test]
fn a_literal_api_key_is_redacted_in_the_trajectory() {
    let mut server = FakeServer::start(vec![(
        200,
        reply(&action("echo COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT")),
    )]);
    let sandbox = Sandbox::new();
    let args: Vec<String> = [
        "-m",
        "test-model",
        "-c",
        "mini_textbased",
        "-c",
        &format!("model.model_kwargs.api_base={}", server.url),
        "-c",
        &format!("model.model_kwargs.api_key={KEY}"),
        "-c",
        "model.cost_tracking=ignore_errors",
        "-t",
        "Say hi",
        "-o",
        &sandbox.out("traj.json"),
    ]
    .iter()
    .map(|a| (*a).to_owned())
    .collect();
    let output = sandbox.run(Path::new(BIN), &args, None);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let trajectory = std::fs::read_to_string(sandbox.root.join("out/traj.json")).unwrap();
    assert!(!trajectory.contains(KEY));
    let saved: Value = serde_json::from_str(&trajectory).unwrap();
    assert_eq!(
        saved["info"]["config"]["model"]["model_kwargs"]["api_key"],
        "<redacted>"
    );
    assert_eq!(
        server.requests()[0].header("authorization"),
        Some(format!("Bearer {KEY}").as_str())
    );
}
