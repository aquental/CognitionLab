//! YAML subset tests (spec/migration-plan.md §3.10 item 5, decision D5).
//!
//! Goldens in `tests/goldens/yaml/` come from PyYAML's `safe_load` through `parity/yaml_dump.py`.

// Test crate: helpers outside `#[test]` functions may panic on broken fixtures (clippy.toml covers only `#[test]`).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::fs;
use std::path::{Path, PathBuf};

use mini_swe_agent_rs::yaml::{YamlError, load_str};
use serde_json::{Value, json};

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn assert_matches_golden(source: &Path) {
    let stem = source.file_stem().unwrap().to_str().unwrap();
    let golden_path = crate_dir()
        .join("tests/goldens/yaml")
        .join(format!("{stem}.json"));
    let golden: Value = serde_json::from_str(&fs::read_to_string(&golden_path).unwrap()).unwrap();
    let loaded = load_str(&fs::read_to_string(source).unwrap())
        .unwrap_or_else(|e| panic!("{}: {e}", source.display()));
    assert_eq!(loaded, golden, "{} differs from PyYAML", source.display());
}

#[test]
fn shipped_configs_match_pyyaml() {
    for name in ["default.yaml", "mini_textbased.yaml"] {
        assert_matches_golden(&crate_dir().join("config").join(name));
    }
}

#[test]
fn subset_cases_match_pyyaml() {
    for name in [
        "scalars.yaml",
        "block_scalars.yaml",
        "duplicates.yaml",
        "aliases.yaml",
        "empty.yaml",
        "comment_only.yaml",
        "eof_clip.yaml",
        "eof_keep.yaml",
        "eof_strip.yaml",
        "eof_quoted.yaml",
        "eof_nested.yaml",
        "eof_plain.yaml",
    ] {
        assert_matches_golden(&crate_dir().join("parity/yaml_cases").join(name));
    }
}

#[test]
fn trailing_dot_numbers_are_floats() {
    let value = load_str("a: 3.\nb: 0.\n").unwrap();
    assert!(value["a"].is_f64(), "{value}");
    assert_eq!(value["a"].as_f64(), Some(3.0));
    assert!(value["b"].is_f64(), "{value}");
}

#[test]
fn yaml_1_1_booleans_stay_strings() {
    // Deliberate divergence from PyYAML (plan §2.4): YAML 1.2 core schema, no `yes`/`no`/`on`/`off`.
    let value = load_str("a: yes\nb: no\nc: on\nd: off\ne: Yes\nf: OFF\n").unwrap();
    assert_eq!(
        value,
        json!({"a": "yes", "b": "no", "c": "on", "d": "off", "e": "Yes", "f": "OFF"})
    );
}

#[test]
fn empty_and_comment_only_documents_are_null() {
    assert_eq!(load_str("").unwrap(), Value::Null);
    assert_eq!(load_str("# nothing\n").unwrap(), Value::Null);
}

#[test]
fn invalid_yaml_is_an_error() {
    assert!(matches!(
        load_str("a: [unclosed\n"),
        Err(YamlError::Parse(_))
    ));
}

#[test]
fn non_scalar_keys_are_an_error() {
    assert!(load_str("? [a, b]\n: c\n").is_err());
}

#[test]
fn alias_expansion_is_bounded() {
    // "Billion laughs": must fail fast instead of exhausting memory.
    let mut text = String::from("a0: &a0 [x, x, x, x, x, x, x, x, x, x]\n");
    for i in 1..10 {
        let prev = i - 1;
        text.push_str(&format!(
            "a{i}: &a{i} [*a{prev}, *a{prev}, *a{prev}, *a{prev}, *a{prev}, *a{prev}, *a{prev}, *a{prev}, *a{prev}, *a{prev}]\n"
        ));
    }
    assert!(load_str(&text).is_err());
}

/// The copies in `config/` must stay byte-identical to the Python reference.
/// The Rust-only Linux gate mounts only this crate; it sets `MSWEA_RS_NO_REFERENCE=1` to skip
/// this one check explicitly (never silently).
#[test]
fn shipped_configs_are_identical_to_the_reference() {
    let reference = crate_dir().join("../mini-swe-agent/src/minisweagent/config");
    if !reference.is_dir() {
        assert_eq!(
            std::env::var("MSWEA_RS_NO_REFERENCE").as_deref(),
            Ok("1"),
            "reference directory {} is missing; set MSWEA_RS_NO_REFERENCE=1 to skip on purpose",
            reference.display()
        );
        eprintln!("skipped on purpose: MSWEA_RS_NO_REFERENCE=1");
        return;
    }
    for name in ["default.yaml", "mini_textbased.yaml"] {
        assert_eq!(
            fs::read(crate_dir().join("config").join(name)).unwrap(),
            fs::read(reference.join(name)).unwrap(),
            "config/{name} differs from the reference"
        );
    }
}
