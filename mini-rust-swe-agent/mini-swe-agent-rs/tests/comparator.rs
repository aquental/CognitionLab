//! Self-tests of the parity comparator (spec/migration-plan.md §3.15 item 4, gates P15-P16):
//! changing anything but an authorized path must fail; authorized changes must pass.
//! Pure Rust: these run in the Rust-only gate too.

mod support;

use serde_json::{Map, Value, json};
use support::compare::{Side, compare_layers, first_difference, normalize_result};

/// A named edit of a sample result.
type Edit = Box<dyn FnOnce(&mut Value)>;

fn sample() -> Value {
    json!({
        "trajectory": {
            "info": {
                "model_stats": {"instance_cost": 2.0, "api_calls": 2},
                "config": {
                    "agent": {"step_limit": 0}, "agent_type": "minisweagent.agents.default.DefaultAgent",
                    "model": {"outputs": [{"role": "assistant", "content": "x", "timestamp": 5}]},
                    "model_type": "x.DeterministicModel",
                    "environment": {"cwd": "/w"}, "environment_type": "x.LocalEnvironment"
                },
                "mini_version": "2.4.6",
                "exit_status": "Submitted",
                "submission": "done\n"
            },
            "messages": [
                {"role": "system", "content": "s"},
                {"role": "assistant", "content": "a", "extra": {"actions": [{"command": "ls"}], "cost": 1.0, "timestamp": 1700000000.5}},
                {"role": "user", "content": "<returncode>0</returncode>", "extra": {"raw_output": "", "returncode": 0, "timestamp": 1700000001.5, "exception_info": ""}},
                {"role": "exit", "content": "done\n", "extra": {"exit_status": "Submitted", "submission": "done\n"}}
            ],
            "trajectory_format": "mini-swe-agent-1.1"
        },
        "queries": [[{"role": "system", "content": "s"}], [{"role": "system", "content": "s"}, {"role": "assistant", "content": "a", "extra": {"timestamp": 1.0}}]],
        "actions": [{"command": "ls"}],
        "state": {"returned": {"exit_status": "Submitted", "submission": "done\n"}, "error": null, "n_calls": 2, "cost": 2.0, "global_cost": 2.0, "global_calls": 2, "remaining_outputs": 0},
        "artifacts": {"a.txt": {"text": "x"}}
    })
}

fn equal_after_normalizing(python: Value, rust: Value) -> bool {
    let (mut python, mut rust) = (python, rust);
    normalize_result(&mut python, Side::Python, &Map::new());
    normalize_result(&mut rust, Side::Rust, &Map::new());
    compare_layers("agent", &python, &rust).is_ok()
}

fn mutated(edit: impl FnOnce(&mut Value)) -> Value {
    let mut value = sample();
    edit(&mut value);
    value
}

#[test]
fn identical_results_are_equal() {
    assert!(equal_after_normalizing(sample(), sample()));
}

#[test]
fn unauthorized_changes_fail() {
    let mutations: Vec<(&str, Edit)> = vec![
        (
            "command",
            Box::new(|v| v["actions"][0]["command"] = json!("ls -la")),
        ),
        (
            "message content",
            Box::new(|v| v["trajectory"]["messages"][0]["content"] = json!("S")),
        ),
        ("cost", Box::new(|v| v["state"]["cost"] = json!(2.0000001))),
        (
            "message order",
            Box::new(|v| {
                v["trajectory"]["messages"]
                    .as_array_mut()
                    .unwrap()
                    .swap(1, 2)
            }),
        ),
        (
            "removed field",
            Box::new(|v| {
                v["trajectory"]["messages"][2]["extra"]
                    .as_object_mut()
                    .unwrap()
                    .remove("exception_info");
            }),
        ),
        (
            "null vs empty",
            Box::new(|v| v["trajectory"]["info"]["submission"] = Value::Null),
        ),
        (
            "nested timestamp in fixture data",
            Box::new(|v| {
                v["trajectory"]["info"]["config"]["model"]["outputs"][0]["timestamp"] = json!(6)
            }),
        ),
        (
            "timestamp inside actions",
            Box::new(|v| {
                v["trajectory"]["messages"][1]["extra"]["actions"][0]["timestamp"] = json!(1)
            }),
        ),
        (
            "query history",
            Box::new(|v| {
                v["queries"]
                    .as_array_mut()
                    .unwrap()
                    .pop()
                    .map(|_| ())
                    .unwrap_or_default()
            }),
        ),
        (
            "artifact",
            Box::new(|v| v["artifacts"]["a.txt"]["text"] = json!("y")),
        ),
        (
            "exit status",
            Box::new(|v| v["state"]["returned"]["exit_status"] = json!("LimitsExceeded")),
        ),
        (
            "leftover outputs",
            Box::new(|v| v["state"]["remaining_outputs"] = json!(1)),
        ),
        (
            "string vs number",
            Box::new(|v| v["state"]["n_calls"] = json!("2")),
        ),
        (
            "whitespace in output",
            Box::new(|v| v["trajectory"]["messages"][3]["content"] = json!("done")),
        ),
    ];
    for (name, edit) in mutations {
        assert!(
            !equal_after_normalizing(sample(), mutated(edit)),
            "{name} must fail"
        );
    }
}

#[test]
fn authorized_changes_pass() {
    let changes: Vec<(&str, Edit)> = vec![
        (
            "message timestamp",
            Box::new(|v| v["trajectory"]["messages"][1]["extra"]["timestamp"] = json!(1.0)),
        ),
        (
            "query timestamp",
            Box::new(|v| v["queries"][1][1]["extra"]["timestamp"] = json!(99.0)),
        ),
        (
            "types and version",
            Box::new(|v| {
                v["trajectory"]["info"]["config"]["agent_type"] =
                    json!("mini_swe_agent_rs::agent::default::DefaultAgent");
                v["trajectory"]["info"]["config"]["model_type"] =
                    json!("mini_swe_agent_rs::model::deterministic::DeterministicModel");
                v["trajectory"]["info"]["config"]["environment_type"] =
                    json!("mini_swe_agent_rs::environment::local::LocalEnvironment");
                v["trajectory"]["info"]["mini_version"] = json!("0.1.0");
            }),
        ),
        ("2 vs 2.0", Box::new(|v| v["state"]["cost"] = json!(2))),
        (
            "key order",
            Box::new(|v| {
                let state = v["state"].as_object().unwrap();
                let reversed: Map<String, Value> = state
                    .iter()
                    .rev()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                v["state"] = Value::Object(reversed);
            }),
        ),
    ];
    for (name, edit) in changes {
        assert!(
            equal_after_normalizing(sample(), mutated(edit)),
            "{name} must pass"
        );
    }
}

#[test]
fn fatal_exits_map_python_names_and_drop_texts() {
    let fatal = |status: &str, text: &str| {
        json!({
            "trajectory": {"info": {"exit_status": status}, "messages": [
                {"role": "exit", "content": text, "extra": {"exit_status": status, "submission": "", "exception_str": text, "traceback": "tb"}}
            ]},
            "state": {"returned": null, "error": status},
            "queries": [], "actions": [], "artifacts": {}
        })
    };
    assert!(equal_after_normalizing(
        fatal("IndexError", "list index out of range"),
        fatal(
            "OutputsExhausted",
            "the deterministic model has no outputs left"
        )
    ));
    assert!(!equal_after_normalizing(
        fatal("IndexError", "x"),
        fatal("GlobalLimitExceeded", "x")
    ));
    // The D6 map applies to the Python side only.
    assert!(!equal_after_normalizing(
        fatal("OutputsExhausted", "x"),
        fatal("IndexError", "x")
    ));
    // A non-fatal exit keeps its content.
    let mut python = sample();
    let mut rust = sample();
    rust["trajectory"]["messages"][3]["content"] = json!("other");
    normalize_result(&mut python, Side::Python, &Map::new());
    normalize_result(&mut rust, Side::Rust, &Map::new());
    assert!(compare_layers("agent", &python, &rust).is_err());
}

#[test]
fn integers_are_never_compared_through_f64() {
    let big = 9_007_199_254_740_993_u64; // 2^53 + 1, which f64 rounds to 2^53
    assert!(first_difference(&json!(big), &json!(big - 1)).is_some());
    assert!(first_difference(&json!(big), &json!(9_007_199_254_740_992.0)).is_some());
    assert!(first_difference(&json!(-3), &json!(-3.0)).is_none());
    // Above 2^53, an integer and a float that is exactly that integer are equal.
    let exact = 18_014_398_509_481_984_u64; // 2^54
    assert!(first_difference(&json!(exact), &json!(18_014_398_509_481_984.0)).is_none());
    assert!(
        first_difference(&json!(-(1_i64 << 60)), &json!(-1_152_921_504_606_846_976.0)).is_none()
    );
    assert!(first_difference(&json!(1), &json!(1.5)).is_some());
    assert!(first_difference(&json!(1), &json!(true)).is_some());
}

#[test]
fn required_layers_cannot_be_missing_on_both_sides() {
    for key in ["trajectory", "queries", "actions", "state", "artifacts"] {
        let mut python = sample();
        let mut rust = sample();
        python.as_object_mut().unwrap().remove(key);
        rust.as_object_mut().unwrap().remove(key);
        assert!(compare_layers("agent", &python, &rust).is_err(), "{key}");
    }
    let empty = json!({});
    assert!(compare_layers("environment", &empty, &empty).is_err());
    assert!(
        compare_layers(
            "environment",
            &json!({"results": [], "artifacts": {}}),
            &json!({"results": [], "artifacts": {}})
        )
        .is_ok()
    );
    // An unexpected top-level key is a difference too.
    let mut extra = sample();
    extra["surprise"] = json!(1);
    assert!(compare_layers("agent", &extra, &extra).is_err());
}

#[test]
fn normalization_keeps_types_and_presence() {
    // A timestamp that is not a number is not normalized away.
    assert!(!equal_after_normalizing(
        sample(),
        mutated(|v| v["trajectory"]["messages"][1]["extra"]["timestamp"] = Value::Null)
    ));
    // A fatal exit message without `content` differs from one with it.
    let fatal = |content: Option<&str>| {
        let mut exit = json!({"role": "exit", "extra": {"exit_status": "X", "submission": "", "exception_str": "e", "traceback": "t"}});
        if let Some(text) = content {
            exit["content"] = json!(text);
        }
        let mut value = sample();
        value["trajectory"]["messages"] = json!([exit]);
        value["state"]["error"] = json!("X");
        value
    };
    assert!(equal_after_normalizing(fatal(Some("a")), fatal(Some("b"))));
    assert!(!equal_after_normalizing(fatal(Some("a")), fatal(None)));
}
