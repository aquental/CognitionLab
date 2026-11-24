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
    /// The text is not valid YAML, or uses a construct outside the supported subset.
    #[error("invalid YAML: {0}")]
    Parse(String),
}

fn options() -> Options {
    let mut options = Options::default();
    options.strict_booleans = true;
    options.duplicate_keys = DuplicateKeyPolicy::LastWins;
    options
}

/// Parses one YAML document into a JSON value.
pub fn load_str(text: &str) -> Result<Value, YamlError> {
    if is_blank_document(text) {
        return Ok(Value::Null);
    }
    let mut value: Value = serde_saphyr::from_str_with_options(text, options())
        .map_err(|e| YamlError::Parse(e.to_string()))?;
    if !text.ends_with('\n') {
        remove_eof_line_break(&mut value, text);
    }
    Ok(value)
}

/// Undoes a line break the parser invents at end of input.
///
/// When a document ends inside a block scalar (`|`, `|+`) with no final newline, the
/// `saphyr` parser family adds a `\n` that is not in the text; libyaml (PyYAML) does not. The
/// shipped `default.yaml` ends this way. The affected scalar is the last leaf of the document,
/// so the fix removes one trailing `\n` from that leaf when the text's last line ends with the
/// leaf's last line. Quoted scalars are not affected: their last line ends with the quote.
fn remove_eof_line_break(value: &mut Value, text: &str) {
    let Some(leaf) = last_leaf(value) else { return };
    let Value::String(s) = leaf else { return };
    let Some(body) = s.strip_suffix('\n') else {
        return;
    };
    let last_value_line = body.rsplit('\n').next().unwrap_or(body);
    let last_text_line = text.rsplit('\n').next().unwrap_or(text);
    if !last_value_line.is_empty() && last_text_line.trim_end().ends_with(last_value_line) {
        s.pop();
    }
}

/// The value written last in document order (insertion order is kept by `preserve_order`).
fn last_leaf(value: &mut Value) -> Option<&mut Value> {
    match value {
        Value::Object(map) => map.values_mut().next_back().and_then(last_leaf),
        Value::Array(items) => items.last_mut().and_then(last_leaf),
        leaf => Some(leaf),
    }
}

/// True for a document with no content: only whitespace and comments.
fn is_blank_document(text: &str) -> bool {
    text.lines().all(|line| {
        let line = line.trim();
        line.is_empty() || line.starts_with('#')
    })
}
