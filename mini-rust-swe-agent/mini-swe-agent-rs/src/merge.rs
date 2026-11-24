//! `recursive_merge`, the port of `utils/serialize.py:6-29`.
//!
//! Later layers win; nested objects merge; lists and scalars are replaced. A `null` layer is
//! skipped (Python skips `None` dictionaries). Python's `UNSET` sentinel has no counterpart: a
//! value that is not set is absent from its layer.

use serde_json::{Map, Value};

/// Errors from [`recursive_merge`].
#[derive(Debug, thiserror::Error)]
pub enum MergeError {
    /// A layer is neither an object nor `null`.
    #[error("merge layer {index} is not an object (found {found})")]
    NotAnObject {
        /// Position of the layer in the input.
        index: usize,
        /// JSON type of the layer.
        found: &'static str,
    },
}

/// Merges the layers into one object, later layers taking precedence.
pub fn recursive_merge<'a>(
    layers: impl IntoIterator<Item = &'a Value>,
) -> Result<Map<String, Value>, MergeError> {
    let mut result = Map::new();
    for (index, layer) in layers.into_iter().enumerate() {
        match layer {
            Value::Null => {}
            Value::Object(map) => merge_into(&mut result, map),
            other => {
                return Err(MergeError::NotAnObject {
                    index,
                    found: type_name(other),
                });
            }
        }
    }
    Ok(result)
}

fn merge_into(target: &mut Map<String, Value>, layer: &Map<String, Value>) {
    for (key, value) in layer {
        match (target.get_mut(key), value) {
            (Some(Value::Object(existing)), Value::Object(incoming)) => {
                merge_into(existing, incoming)
            }
            _ => {
                target.insert(key.clone(), value.clone());
            }
        }
    }
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
