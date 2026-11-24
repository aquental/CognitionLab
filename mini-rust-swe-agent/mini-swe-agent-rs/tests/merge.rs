//! Port of `tests/utils/test_serialize.py` (`recursive_merge`, `utils/serialize.py:6-29`).
//!
//! Python's `UNSET` sentinel has no Rust counterpart: a value that is not set is simply absent
//! from its layer (plan §3.10 item 2), so the UNSET tests become "absent key" tests.

use mini_swe_agent_rs::merge::{MergeError, recursive_merge};
use serde_json::{Value, json};

/// Test helper: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::unwrap_used)]
fn merge(layers: &[Value]) -> Value {
    Value::Object(recursive_merge(layers).unwrap())
}

#[test]
fn empty_input() {
    assert_eq!(merge(&[]), json!({}));
}

#[test]
fn single_dictionary() {
    assert_eq!(merge(&[json!({"a": 1, "b": 2})]), json!({"a": 1, "b": 2}));
}

#[test]
fn simple_override() {
    assert_eq!(
        merge(&[json!({"a": 1, "b": 2}), json!({"b": 3, "c": 4})]),
        json!({"a": 1, "b": 3, "c": 4})
    );
}

#[test]
fn nested_dict_merge() {
    assert_eq!(
        merge(&[
            json!({"a": {"x": 1, "y": 2}, "b": 3}),
            json!({"a": {"y": 3, "z": 4}, "c": 5})
        ]),
        json!({"a": {"x": 1, "y": 3, "z": 4}, "b": 3, "c": 5})
    );
}

#[test]
fn deeply_nested_merge() {
    assert_eq!(
        merge(&[
            json!({"a": {"b": {"c": 1, "d": 2}}}),
            json!({"a": {"b": {"d": 3, "e": 4}}})
        ]),
        json!({"a": {"b": {"c": 1, "d": 3, "e": 4}}})
    );
}

#[test]
fn override_dict_with_non_dict() {
    assert_eq!(
        merge(&[json!({"a": {"x": 1, "y": 2}}), json!({"a": "string"})]),
        json!({"a": "string"})
    );
}

#[test]
fn override_non_dict_with_dict() {
    assert_eq!(
        merge(&[json!({"a": "string"}), json!({"a": {"x": 1, "y": 2}})]),
        json!({"a": {"x": 1, "y": 2}})
    );
}

#[test]
fn multiple_dictionaries() {
    assert_eq!(
        merge(&[
            json!({"a": 1, "b": 2}),
            json!({"b": 3, "c": 4}),
            json!({"c": 5, "d": 6})
        ]),
        json!({"a": 1, "b": 3, "c": 5, "d": 6})
    );
}

#[test]
fn multiple_nested_dictionaries() {
    assert_eq!(
        merge(&[
            json!({"a": {"x": 1, "y": 2}}),
            json!({"a": {"y": 3, "z": 4}}),
            json!({"a": {"z": 5, "w": 6}})
        ]),
        json!({"a": {"x": 1, "y": 3, "z": 5, "w": 6}})
    );
}

#[test]
fn mixed_value_types() {
    assert_eq!(
        merge(&[
            json!({"int": 1, "str": "hello", "list": [1, 2], "dict": {"a": 1}}),
            json!({"int": 2, "bool": true, "none": null, "dict": {"b": 2}}),
        ]),
        json!({"int": 2, "str": "hello", "list": [1, 2], "bool": true, "none": null, "dict": {"a": 1, "b": 2}})
    );
}

#[test]
fn null_value_overrides() {
    // A null *value* replaces the earlier value; only whole null *layers* are skipped.
    assert_eq!(
        merge(&[json!({"a": 1}), json!({"a": null})]),
        json!({"a": null})
    );
}

#[test]
fn list_override_not_merge() {
    assert_eq!(
        merge(&[json!({"list": [1, 2, 3]}), json!({"list": [4, 5]})]),
        json!({"list": [4, 5]})
    );
}

#[test]
fn empty_nested_dicts() {
    assert_eq!(
        merge(&[json!({"a": {}}), json!({"a": {"x": 1}})]),
        json!({"a": {"x": 1}})
    );
    assert_eq!(
        merge(&[json!({"a": {"x": 1}}), json!({"a": {}})]),
        json!({"a": {"x": 1}})
    );
}

#[test]
fn complex_nested_structure() {
    let first = json!({"config": {"server": {"host": "localhost", "port": 8080}, "debug": true}, "data": [1, 2, 3]});
    let second = json!({"config": {"server": {"port": 9090, "ssl": true}, "log_level": "info"}, "extra": "value"});
    assert_eq!(
        merge(&[first, second]),
        json!({
            "config": {"server": {"host": "localhost", "port": 9090, "ssl": true}, "debug": true, "log_level": "info"},
            "data": [1, 2, 3],
            "extra": "value",
        })
    );
}

#[test]
fn original_layers_unchanged() {
    let first = json!({"a": {"x": 1}});
    let second = json!({"a": {"y": 2}});
    let layers = [first.clone(), second.clone()];
    let _ = merge(&layers);
    assert_eq!(layers, [first, second]);
}

#[test]
fn null_layers_skipped() {
    assert_eq!(merge(&[Value::Null]), json!({}));
    assert_eq!(
        merge(&[json!({"a": 1}), Value::Null, json!({"b": 2})]),
        json!({"a": 1, "b": 2})
    );
}

#[test]
fn absent_keys_leave_earlier_values() {
    // Python: recursive_merge({"a": 1}, {"a": UNSET, "b": 2}) == {"a": 1, "b": 2}
    assert_eq!(
        merge(&[json!({"a": 1}), json!({"b": 2})]),
        json!({"a": 1, "b": 2})
    );
    // Python: {"run": {"task": UNSET}} merges to {"run": {}}; Rust builds the layer without the key.
    assert_eq!(
        merge(&[json!({"a": 1}), json!({"run": {}})]),
        json!({"a": 1, "run": {}})
    );
}

#[test]
fn later_wins_keeps_first_insertion_order() {
    // Unsorted keys at both levels, so a sorted map would fail.
    let merged = merge(&[
        json!({"z": 1, "m": {"y": 1, "b": 2}, "a": 2}),
        json!({"m": {"c": 3, "y": 4}, "q": 5, "z": 6}),
    ]);
    let keys: Vec<&String> = merged.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["z", "m", "a", "q"]);
    let inner: Vec<&String> = merged["m"].as_object().unwrap().keys().collect();
    assert_eq!(inner, ["y", "b", "c"]);
    assert_eq!(
        merged,
        json!({"z": 6, "m": {"y": 4, "b": 2, "c": 3}, "a": 2, "q": 5})
    );
}

#[test]
fn nested_absent_key_keeps_sibling() {
    // Python: recursive_merge({"a": {"x": 1, "y": 2}}, {"a": {"y": UNSET, "z": 3}, "b": UNSET, "c": 4})
    //   == {"a": {"x": 1, "y": 2, "z": 3}, "c": 4}
    assert_eq!(
        merge(&[
            json!({"a": {"x": 1, "y": 2}}),
            json!({"a": {"z": 3}, "c": 4})
        ]),
        json!({"a": {"x": 1, "y": 2, "z": 3}, "c": 4})
    );
}

#[test]
fn falsy_values_override() {
    assert_eq!(
        merge(&[
            json!({"a": true, "b": 5, "c": "x", "d": [1], "e": {"k": 1}}),
            json!({"a": false, "b": 0, "c": "", "d": [], "e": null})
        ]),
        json!({"a": false, "b": 0, "c": "", "d": [], "e": null})
    );
}

#[test]
fn non_object_layer_is_an_error() {
    assert!(matches!(
        recursive_merge(&[json!([1, 2])]),
        Err(MergeError::NotAnObject { index: 0, .. })
    ));
    assert!(matches!(
        recursive_merge(&[json!({}), json!("x")]),
        Err(MergeError::NotAnObject { index: 1, .. })
    ));
}
