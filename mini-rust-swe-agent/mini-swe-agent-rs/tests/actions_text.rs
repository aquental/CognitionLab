//! Text actions (plan §3.8 "Action parsing", phase P4): `parse_regex_actions` and
//! `format_observation_messages`, compared with goldens from `parity/action_goldens.py`.

use std::fs;
use std::path::PathBuf;

use std::sync::Mutex;

use mini_swe_agent_rs::clock::{Clock, ManualClock};
use mini_swe_agent_rs::error::{AgentError, InterruptKind, StepError};
use mini_swe_agent_rs::message::{ExecOutput, Message};
use mini_swe_agent_rs::model::actions_text::{ActionRegex, ActionRegexError, TextActions};
use mini_swe_agent_rs::template::TemplateError;
use serde_json::{Map, Value, json};

/// Test helper: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::unwrap_used)]
fn golden(name: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/goldens/actions")
        .join(name);
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

const OBSERVATION: &str = "{{ output.output }}";

/// A clock that reads `start`, `start + 1`, … (the golden's patched `time.time`).
#[derive(Debug)]
struct StepClock(Mutex<f64>);

impl Clock for StepClock {
    #[allow(clippy::unwrap_used)]
    fn now(&self) -> f64 {
        let mut now = self.0.lock().unwrap();
        let value = *now;
        *now += 1.0;
        value
    }
}

/// JSON text with key order: `Value`'s `==` ignores the order of object keys.
#[allow(clippy::unwrap_used)]
fn ordered(value: &Value) -> String {
    serde_json::to_string(value).unwrap()
}

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
        assert_eq!(ordered(&actual), ordered(&case["result"]), "{name}");
    }
}

#[test]
fn observations_match_python() {
    for case in golden("observations.json") {
        let name = case["name"].as_str().unwrap();
        let actions =
            TextActions::new(r"x(.*)", "{{ error }}", case["template"].as_str().unwrap()).unwrap();
        let outputs: Vec<ExecOutput> = serde_json::from_value(case["outputs"].clone()).unwrap();
        // Python samples `time.time()` once per output, after rendering it.
        let start = case["messages"][0]["extra"]["timestamp"].as_f64().unwrap();
        let clock = StepClock(Mutex::new(start));
        let messages = actions
            .observations(&outputs, &case["template_vars"], &clock)
            .unwrap();
        assert_eq!(
            ordered(&serde_json::to_value(&messages).unwrap()),
            ordered(&case["messages"]),
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
        actions
            .observations(&[], &vars, &ManualClock::new(1.0))
            .unwrap(),
        Vec::<Message>::new()
    );
    let output = ExecOutput {
        output: "o".into(),
        returncode: 0,
        exception_info: String::new(),
        extra: None,
    };
    assert!(matches!(
        actions.observations(&[output], &vars, &ManualClock::new(1.0)),
        Err(AgentError::Template(TemplateError::DuplicateKey(key))) if key == "output"
    ));
}

#[test]
fn format_error_kwargs_cannot_replace_actions_or_error() {
    // Python's `render(actions=..., error=..., **kwargs)` raises TypeError on a repeated keyword.
    let actions = TextActions::new(r"x(.*)", "{{ error }}", OBSERVATION).unwrap();
    for key in ["actions", "error"] {
        let kwargs: Map<String, Value> =
            [(key.to_owned(), json!("override"))].into_iter().collect();
        assert!(
            matches!(
                actions.parse("no action", &kwargs),
                Err(StepError::Fatal(AgentError::Template(TemplateError::DuplicateKey(k)))) if k == key
            ),
            "{key}"
        );
        // One valid action never renders the template, in Python either.
        assert_eq!(actions.parse("xls", &kwargs).unwrap().len(), 1);
    }
}

#[test]
fn python_only_regex_semantics_are_rejected_at_load() {
    for (pattern, why) in [
        (r"(x)$", "Python's `$` also matches before a final newline"),
        (r"(?:^|x)", "can match the empty string"),
        (r"a([\S]+)b", "`\\S` inside a character class"),
    ] {
        assert!(
            matches!(
                ActionRegex::new(pattern),
                Err(ActionRegexError::Unsupported { .. })
            ),
            "{pattern}: {why}"
        );
    }
    // Escaped and in-class dollars are literal in both engines.
    assert_eq!(
        ActionRegex::new(r"\$(x)[$]").unwrap().findall("$x$"),
        vec!["x"]
    );
}

#[test]
fn non_finite_clock_readings_are_errors() {
    let actions = TextActions::new(r"x(.*)", "{{ error }}", OBSERVATION).unwrap();
    let output = ExecOutput {
        output: "o".into(),
        returncode: 0,
        exception_info: String::new(),
        extra: None,
    };
    assert!(matches!(
        actions.observations(&[output], &json!({}), &ManualClock::new(f64::NAN)),
        Err(AgentError::Internal(_))
    ));
}
