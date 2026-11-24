//! The parity comparator (spec/migration-plan.md §3.15 item 4).
//!
//! Normalization is a fixed list of named JSON paths; everything else is compared exactly.
//! Equality ignores key order and treats an integer and a float with the same value as equal
//! (as `json.loads` does), but compares integers as integers, never through `f64`.

// Pure comparator logic: the support module's test exemptions do not apply here.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use serde_json::{Map, Value};

/// D6: Python exception names that always map to a Rust exit status.
pub const PYTHON_EXIT_STATUS: &[(&str, &str)] = &[("IndexError", "OutputsExhausted")];

const TIMESTAMP: &str = "<timestamp>";
const IGNORED: &str = "<ignored>";

/// Which side a result comes from: only Python names are mapped through D6.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Python,
    Rust,
}

/// Maps a Python exit status to the Rust name (global table plus the scenario's own map).
pub fn map_exit_status(name: &str, scenario_map: &Map<String, Value>) -> String {
    if let Some(mapped) = scenario_map.get(name).and_then(Value::as_str) {
        return mapped.to_owned();
    }
    PYTHON_EXIT_STATUS
        .iter()
        .find(|(python, _)| *python == name)
        .map_or_else(|| name.to_owned(), |(_, rust)| (*rust).to_owned())
}

/// `messages[*].extra.timestamp`: a numeric value is replaced, the key kept. Any other type is
/// left as it is, so it still differs from a real timestamp.
fn normalize_message(message: &mut Value) {
    if let Some(timestamp) = message
        .pointer_mut("/extra/timestamp")
        .filter(|t| t.is_number())
    {
        *timestamp = Value::String(TIMESTAMP.to_owned());
    }
}

/// Replaces a string value with `IGNORED`; a missing key or another type stays as it is.
fn ignore_text(slot: Option<&mut Value>) {
    if let Some(slot) = slot.filter(|v| v.is_string()) {
        *slot = Value::String(IGNORED.to_owned());
    }
}

fn map_in_place(value: Option<&mut Value>, side: Side, scenario_map: &Map<String, Value>) {
    if side == Side::Rust {
        return;
    }
    if let Some(slot) = value
        && let Some(name) = slot.as_str()
    {
        *slot = Value::String(map_exit_status(name, scenario_map));
    }
}

/// Normalizes a trajectory. `fatal` is true when the run ended with a propagated error.
pub fn normalize_trajectory(
    trajectory: &mut Value,
    fatal: bool,
    side: Side,
    scenario_map: &Map<String, Value>,
) {
    for key in ["agent_type", "model_type", "environment_type"] {
        if let Some(slot) = trajectory.pointer_mut(&format!("/info/config/{key}")) {
            *slot = Value::String(IGNORED.to_owned());
        }
    }
    if let Some(slot) = trajectory.pointer_mut("/info/mini_version") {
        *slot = Value::String(IGNORED.to_owned());
    }
    if let Some(Value::Array(messages)) = trajectory.get_mut("messages") {
        messages.iter_mut().for_each(normalize_message);
        if fatal
            && let Some(exit) = messages
                .last_mut()
                .filter(|m| m.get("role").and_then(Value::as_str) == Some("exit"))
        {
            ignore_text(exit.get_mut("content"));
            if let Some(Value::Object(extra)) = exit.get_mut("extra") {
                ignore_text(extra.get_mut("exception_str"));
                ignore_text(extra.get_mut("traceback"));
                map_in_place(extra.get_mut("exit_status"), side, scenario_map);
            }
        }
    }
    if fatal {
        map_in_place(
            trajectory.pointer_mut("/info/exit_status"),
            side,
            scenario_map,
        );
    }
}

/// Normalizes one harness result (agent or environment scenario) in place.
pub fn normalize_result(result: &mut Value, side: Side, scenario_map: &Map<String, Value>) {
    let fatal = result.pointer("/state/error").is_some_and(|e| !e.is_null());
    for key in ["trajectory", "saved"] {
        if let Some(trajectory) = result.get_mut(key).filter(|t| t.is_object()) {
            normalize_trajectory(trajectory, fatal, side, scenario_map);
        }
    }
    if let Some(Value::Array(queries)) = result.get_mut("queries") {
        for query in queries {
            if let Value::Array(messages) = query {
                messages.iter_mut().for_each(normalize_message);
            }
        }
    }
    map_in_place(result.pointer_mut("/state/error"), side, scenario_map);
}

/// A JSON integer as `i128` (covers both `i64` and `u64`), or `None` for a float.
fn integer(n: &serde_json::Number) -> Option<i128> {
    n.as_i64()
        .map(i128::from)
        .or_else(|| n.as_u64().map(i128::from))
}

/// A float that is exactly an integer, as `i128`; `None` otherwise.
fn integral(x: f64) -> Option<i128> {
    // 2^127 bounds `i128`; every float of that size or less is an exact integer value.
    const LIMIT: f64 = 170_141_183_460_469_231_731_687_303_715_884_105_728.0;
    // `as` saturates, and its value is used only when `x` is an integer in range.
    (x.is_finite() && x.fract() == 0.0 && x.abs() < LIMIT).then_some(x as i128)
}

/// Integers compare exactly; an integer equals a float only when the float is exactly that
/// integer; floats compare as floats.
fn numbers_equal(a: &serde_json::Number, b: &serde_json::Number) -> bool {
    match (integer(a), integer(b)) {
        (Some(x), Some(y)) => x == y,
        (Some(x), None) => b.as_f64().and_then(integral) == Some(x),
        (None, Some(y)) => a.as_f64().and_then(integral) == Some(y),
        (None, None) => a.as_f64() == b.as_f64(),
    }
}

/// The first difference between `left` and `right`, as a JSON path, or `None` when equal.
pub fn first_difference(left: &Value, right: &Value) -> Option<String> {
    diff_at("$", left, right)
}

fn diff_at(path: &str, left: &Value, right: &Value) -> Option<String> {
    match (left, right) {
        (Value::Number(a), Value::Number(b)) => {
            (!numbers_equal(a, b)).then(|| format!("{path}: {a} != {b}"))
        }
        (Value::Object(a), Value::Object(b)) => {
            for (key, value) in a {
                let child = format!("{path}.{key}");
                match b.get(key) {
                    None => return Some(format!("{child}: missing on the right")),
                    Some(other) => {
                        if let Some(found) = diff_at(&child, value, other) {
                            return Some(found);
                        }
                    }
                }
            }
            b.keys()
                .find(|key| !a.contains_key(*key))
                .map(|key| format!("{path}.{key}: missing on the left"))
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                return Some(format!("{path}: {} items != {} items", a.len(), b.len()));
            }
            a.iter()
                .zip(b)
                .enumerate()
                .find_map(|(index, (x, y))| diff_at(&format!("{path}[{index}]"), x, y))
        }
        _ => (left != right).then(|| format!("{path}: {} != {}", short(left), short(right))),
    }
}

fn short(value: &Value) -> String {
    let text = value.to_string();
    if text.chars().count() > 200 {
        format!("{}…", text.chars().take(200).collect::<String>())
    } else {
        text
    }
}

/// Compares two normalized results layer by layer: protocol, state, artifacts. Every key of the
/// scenario kind is required on both sides (`saved` is optional), and no other key may appear.
pub fn compare_layers(kind: &str, python: &Value, rust: &Value) -> Result<(), String> {
    let layers: &[(&str, &[&str])] = match kind {
        "agent" => &[
            ("protocol", &["queries", "actions"]),
            ("state", &["state", "trajectory"]),
            ("artifacts", &["artifacts"]),
        ],
        "environment" => &[("protocol", &["results"]), ("artifacts", &["artifacts"])],
        other => return Err(format!("unknown scenario kind {other:?}")),
    };
    let mut problems = Vec::new();
    for (side, result) in [("python", python), ("rust", rust)] {
        let Some(map) = result.as_object() else {
            problems.push(format!("{side}: the result is not an object"));
            continue;
        };
        for key in map.keys() {
            let known =
                key == "saved" || layers.iter().any(|(_, keys)| keys.contains(&key.as_str()));
            if !known {
                problems.push(format!("{side}: unexpected key {key}"));
            }
        }
    }
    let optional: &[(&str, &[&str])] = &[("state", &["saved"])];
    for (layer, keys, required) in layers
        .iter()
        .map(|(l, k)| (l, k, true))
        .chain(optional.iter().map(|(l, k)| (l, k, false)))
    {
        for key in *keys {
            let (left, right) = (python.get(*key), rust.get(*key));
            let found = match (left, right) {
                (None, None) if required => Some("missing on both sides".to_owned()),
                (None, None) => None,
                (Some(l), Some(r)) => first_difference(l, r),
                _ => Some("present on one side only".to_owned()),
            };
            if let Some(found) = found {
                problems.push(format!("{layer} layer, {key}: {found}"));
            }
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("\n"))
    }
}
