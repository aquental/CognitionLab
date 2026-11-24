//! Text actions (plan §3.8 "Action parsing", phase P4): `parse_regex_actions` and
//! `format_observation_messages`, compared with goldens from `parity/action_goldens.py`.

// Test crate: helpers outside `#[test]` functions may panic on broken fixtures.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::fs;
use std::path::PathBuf;

use mini_swe_agent_rs::error::{AgentError, InterruptKind, StepError};
use mini_swe_agent_rs::message::{ExecOutput, Message};
use mini_swe_agent_rs::model::actions_text::{ActionRegex, ActionRegexError, TextActions};
use mini_swe_agent_rs::template::TemplateError;
use serde_json::{Map, Value, json};

fn golden(name: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/goldens/actions")
        .join(name);
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

const OBSERVATION: &str = "{{ output.output }}";

#[test]
fn parse_matches_python() {
    let cases = golden("parse.json");
    assert!(cases.len() >= 25);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let actions = TextActions::new(
            case["action_regex"].as_str().unwrap(),
            case["format_error_template"].as_str().unwrap(),
            OBSERVATION,
        )
        .unwrap();
        let kwargs = case["template_kwargs"].as_object().unwrap();
        let result = actions.parse(case["content"].as_str().unwrap(), kwargs);
        let actual = match result {
            Ok(actions) => {
                json!({"actions": actions.iter().map(|a| a.to_value()).collect::<Vec<_>>()})
            }
            Err(StepError::Interrupt(interrupt)) => {
                assert_eq!(interrupt.kind(), InterruptKind::FormatError, "{name}");
                json!({"format_error": interrupt.into_messages()})
            }
            Err(StepError::Fatal(e)) => panic!("{name}: {e}"),
        };
        assert_eq!(actual, case["result"], "{name}");
    }
}

#[test]
fn observations_match_python() {
    for case in golden("observations.json") {
        let name = case["name"].as_str().unwrap();
        let actions =
            TextActions::new(r"x(.*)", "{{ error }}", case["template"].as_str().unwrap()).unwrap();
        let outputs: Vec<ExecOutput> = serde_json::from_value(case["outputs"].clone()).unwrap();
        let messages = actions
            .observations(
                &outputs,
                &case["template_vars"],
                case["timestamp"].as_f64().unwrap(),
            )
            .unwrap();
        assert_eq!(
            serde_json::to_value(&messages).unwrap(),
            case["messages"],
            "{name}"
        );
    }
}

#[test]
fn regex_with_two_groups_is_rejected_at_load() {
    assert!(matches!(
        ActionRegex::new(r"(a)(b)"),
        Err(ActionRegexError::TooManyGroups { groups: 2 })
    ));
    assert!(
        matches!(
            ActionRegex::new(r"(?<=a)b"),
            Err(ActionRegexError::Invalid(_))
        ),
        "lookaround is Python-only (R6)"
    );
}

#[test]
fn template_syntax_errors_fail_at_load() {
    assert!(TextActions::new(r"x(.*)", "{% if %}", OBSERVATION).is_err());
    assert!(TextActions::new(r"x(.*)", "{{ error }}", "{% for %}").is_err());
}

#[test]
fn format_error_template_failure_is_fatal() {
    // Trap 26: if the format-error template cannot render, no FormatError exists.
    let actions = TextActions::new(r"x(.*)", "{{ undefined_var }}", OBSERVATION).unwrap();
    let result = actions.parse("no action", &Map::new());
    assert!(
        matches!(result, Err(StepError::Fatal(AgentError::Template(_)))),
        "{result:?}"
    );
}

#[test]
fn output_key_collision_is_lazy() {
    // Trap 25: Python raises TypeError only when an observation is rendered.
    let actions = TextActions::new(r"x(.*)", "{{ error }}", OBSERVATION).unwrap();
    let vars = json!({"output": "shadow"});
    assert_eq!(
        actions.observations(&[], &vars, 1.0).unwrap(),
        Vec::<Message>::new()
    );
    let output = ExecOutput {
        output: "o".into(),
        returncode: 0,
        exception_info: String::new(),
        extra: None,
    };
    assert!(matches!(
        actions.observations(&[output], &vars, 1.0),
        Err(AgentError::Template(TemplateError::DuplicateKey(key))) if key == "output"
    ));
}
