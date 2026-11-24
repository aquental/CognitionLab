//! YAML loading for config files (decision D5: `serde-saphyr`).
//!
//! The supported subset (spec/migration-plan.md §3.10 item 5):
//! - YAML 1.2 core schema: `yes`, `no`, `on`, `off` stay strings (a deliberate divergence from
//!   PyYAML's YAML 1.1 booleans, plan §2.4);
//! - `3.` and `0.` are floats, as in the shipped configs;
//! - duplicate keys keep the **last** value, as PyYAML's `safe_load` does;
//! - aliases and `<<` merge keys resolve; alias expansion is bounded by the default budget;
//! - an empty or comment-only document is `null`, which the config merge skips.

use serde_json::Value;
use serde_saphyr::{DuplicateKeyPolicy, Options};

/// Errors from [`load_str`].
#[derive(Debug, thiserror::Error)]
pub enum YamlError {
    /// The text is not valid YAML, uses a construct outside the supported subset, or exceeds
    /// the parser's resource budget.
    #[error("invalid YAML: {0}")]
    Parse(#[source] Box<serde_saphyr::Error>),
}

/// Appended to a text with no final newline to find the scalar that ends at end of input.
const EOF_SENTINEL: &str = "__MSWEA_EOF_SENTINEL__";

fn options() -> Options {
    let mut options = Options::default();
    options.strict_booleans = true;
    options.duplicate_keys = DuplicateKeyPolicy::LastWins;
    options
}

fn parse(text: &str) -> Result<Value, YamlError> {
    serde_saphyr::from_str_with_options(text, options()).map_err(|e| YamlError::Parse(Box::new(e)))
}

/// Parses one YAML document into a JSON value.
pub fn load_str(text: &str) -> Result<Value, YamlError> {
    let mut value = parse(text)?;
    if !text.is_empty() && !text.ends_with('\n') {
        remove_eof_line_break(&mut value, text);
    }
    Ok(value)
}

/// Undoes a line break the parser invents at end of input.
///
/// When a document ends inside a clip or keep block scalar (`|`, `>`, `|+`, `>+`) with no final
/// newline, the `saphyr` parser family adds a `\n` that is not in the text; libyaml (PyYAML) does
/// not. Both shipped configs end this way. To find that scalar without guessing, the text is
/// parsed again with a sentinel appended to its last line: the one string that then ends with
/// the sentinel plus `\n` is the scalar the invented break belongs to. One trailing `\n` is
/// removed at the same path in the real value. When the last line is a comment, a plain or
/// quoted scalar, or anything else, no string ends that way and nothing changes.
fn remove_eof_line_break(value: &mut Value, text: &str) {
    let Ok(probe) = parse(&format!("{text}{EOF_SENTINEL}")) else {
        return;
    };
    let mut path = Vec::new();
    if !find_sentinel(&probe, &mut path) {
        return;
    }
    if let Some(Value::String(s)) = lookup_mut(value, &path)
        && s.ends_with('\n')
    {
        s.pop();
    }
}

enum Step {
    Key(String),
    Index(usize),
}

/// Depth-first search for the string that ends with the sentinel and a line break.
fn find_sentinel(value: &Value, path: &mut Vec<Step>) -> bool {
    match value {
        Value::String(s) => s
            .strip_suffix('\n')
            .is_some_and(|body| body.ends_with(EOF_SENTINEL)),
        Value::Object(map) => map.iter().any(|(key, child)| {
            path.push(Step::Key(key.clone()));
            find_sentinel(child, path) || {
                path.pop();
                false
            }
        }),
        Value::Array(items) => items.iter().enumerate().any(|(index, child)| {
            path.push(Step::Index(index));
            find_sentinel(child, path) || {
                path.pop();
                false
            }
        }),
        _ => false,
    }
}

fn lookup_mut<'v>(value: &'v mut Value, path: &[Step]) -> Option<&'v mut Value> {
    path.iter().try_fold(value, |current, step| match step {
        Step::Key(key) => current.get_mut(key.as_str()),
        Step::Index(index) => current.get_mut(*index),
    })
}
