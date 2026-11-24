//! Raw-response differential (spec/migration-plan.md §3.15 item 4, phase P8): Python's real
//! `LitellmTextbasedModel` with `litellm.completion` patched (`parity/text_model_python.py`)
//! against Rust's `TextbasedModel` with a canned backend, on the cases of
//! `parity/text_model_cases.json`. The deterministic model ships ready actions, so this is what
//! proves the text path: billing, parsing, format-error enrichment and global accounting.

mod support;

use std::process::Command;
use std::sync::{Arc, Mutex};

use mini_swe_agent_rs::clock::{Clock, ManualClock};
use mini_swe_agent_rs::config::{self, ModelChoice};
use mini_swe_agent_rs::error::{AgentError, StepError};
use mini_swe_agent_rs::host_env::HostEnv;
use mini_swe_agent_rs::message::Message;
use mini_swe_agent_rs::model::Model;
use mini_swe_agent_rs::model::openai_compat::{
    BackendError, ChatBackend, ChatRequest, ChatResponse,
};
use mini_swe_agent_rs::model::stats::GlobalModelStats;
use mini_swe_agent_rs::model::textbased::TextbasedModel;
use serde_json::{Value, json};
use support::compare::first_difference;

/// Returns one canned response.
#[derive(Debug)]
struct Once(Mutex<Option<Value>>);

impl ChatBackend for Once {
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    fn complete(&self, _request: &ChatRequest) -> Result<ChatResponse, BackendError> {
        Ok(ChatResponse::new(
            self.0.lock().unwrap().take().expect("one call per case"),
        ))
    }
}

/// The Rust projection of one case, with the same keys as the Python runner's.
/// Test helper: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
fn rust_case(case: &Value, registry: &str, messages: &[Message]) -> Value {
    let source = config::ConfigSource::Builtin("mini_textbased.yaml".to_owned());
    let raw = mini_swe_agent_rs::yaml::load_str(&config::read_source(&source).unwrap()).unwrap();
    let mut section = raw["model"].clone();
    section["model_name"] = case["model_name"].clone();
    section["litellm_model_registry"] = json!(registry);
    section["cost_tracking"] = case
        .get("cost_tracking")
        .cloned()
        .unwrap_or(json!("default"));
    let host = HostEnv::from_vars(std::iter::empty());
    let ModelChoice::Textbased(model_config) = config::model_choice(&section, &host).unwrap()
    else {
        panic!("text-based")
    };
    let stats = Arc::new(GlobalModelStats::new(0.0, 0));
    let clock: Arc<dyn Clock> = Arc::new(ManualClock::new(0.0));
    let backend = Once(Mutex::new(Some(case["response"].clone())));
    let mut model =
        TextbasedModel::new(model_config, backend, stats.clone(), clock, &host).unwrap();
    let keys = |extra: &serde_json::Map<String, Value>| json!(extra.keys().collect::<Vec<_>>());
    let mut result = match model.query(messages) {
        Ok(message) => {
            let extra = message.extra.clone().unwrap();
            json!({"kind": "ok", "role": message.role, "content": message.content, "actions": extra["actions"], "cost": extra["cost"], "extra_keys": keys(&extra)})
        }
        Err(StepError::Interrupt(interrupt)) => {
            let head = interrupt.head();
            let extra = head.extra.clone().unwrap();
            json!({"kind": "format_error", "role": head.role, "content": head.content, "interrupt_type": extra["interrupt_type"], "n_actions": extra["n_actions"], "model_response": extra["model_response"], "cost": extra["cost"], "extra_keys": keys(&extra)})
        }
        Err(StepError::Fatal(AgentError::Model(error))) => {
            json!({"kind": "error", "type": error.exit_status()})
        }
        Err(StepError::Fatal(other)) => panic!("{other}"),
    };
    result["stats"] = json!({"calls": stats.n_calls(), "cost": stats.cost()});
    result
}

#[test]
#[allow(clippy::indexing_slicing)]
fn python_and_rust_text_models_agree_on_raw_responses() {
    let Some(harness) = support::Harness::new() else {
        eprintln!("skipped on purpose: MSWEA_RS_NO_REFERENCE=1");
        return;
    };
    let cases_file = support::crate_dir().join("parity/text_model_cases.json");
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(&cases_file).unwrap()).unwrap();
    let out = harness.base.join("text-python.json");
    let status = Command::new(harness.python_bin_dir().join("python"))
        .arg("-I")
        .arg(support::crate_dir().join("parity/text_model_python.py"))
        .arg(&cases_file)
        .arg(&out)
        .env_clear()
        .envs(&harness.env)
        .current_dir(&harness.base)
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(
        status.success(),
        "parity/text_model_python.py failed: {status}"
    );
    let python: Value = serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();

    let registry = harness.base.join("registry.json");
    std::fs::write(&registry, doc["registry"].to_string()).unwrap();
    let messages: Vec<Message> = serde_json::from_value(doc["messages"].clone()).unwrap();
    let mut failures = Vec::new();
    let cases = doc["cases"].as_array().unwrap();
    assert!(cases.len() >= 6);
    for case in cases {
        let id = case["id"].as_str().unwrap();
        let rust = rust_case(case, registry.to_str().unwrap(), &messages);
        if let Some(diff) = first_difference(&python[id], &rust) {
            failures.push(format!("{id}: {diff}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    // The set covers every outcome.
    let kinds: std::collections::BTreeSet<&str> = python
        .as_object()
        .unwrap()
        .values()
        .filter_map(|r| r["kind"].as_str())
        .collect();
    assert_eq!(
        kinds.into_iter().collect::<Vec<_>>(),
        ["error", "format_error", "ok"]
    );
}
