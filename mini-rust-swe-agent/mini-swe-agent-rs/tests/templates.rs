//! Template goldens (plan §3.9, phase P2): Rust must render every shipped text-based template,
//! the Python class defaults and the formatting probes byte for byte like Jinja2.
//! Goldens come from `parity/template_goldens.py`.

use std::fs;
use std::path::PathBuf;

use mini_swe_agent_rs::template::{TemplateError, Templates, render_once};
use mini_swe_agent_rs::yaml::load_str;
use serde_json::{Value, json};

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Test helper: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::unwrap_used)]
fn cases() -> Vec<Value> {
    let text = fs::read_to_string(crate_dir().join("tests/goldens/templates/cases.json")).unwrap();
    serde_json::from_str::<Vec<Value>>(&text).unwrap()
}

#[test]
fn renders_every_golden_byte_for_byte() {
    let cases = cases();
    assert!(cases.len() >= 40, "only {} cases", cases.len());
    let mut failures = Vec::new();
    for case in &cases {
        let name = case["name"].as_str().unwrap();
        let rendered = render_once(case["template"].as_str().unwrap(), &case["context"])
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        if rendered != case["expected"].as_str().unwrap() {
            failures.push(name.to_owned());
        }
    }
    assert!(failures.is_empty(), "differ from Jinja2: {failures:?}");
}

#[test]
fn config_goldens_use_the_shipped_templates() {
    let mut seen = std::collections::BTreeSet::new();
    for case in cases() {
        let name = case["name"].as_str().unwrap();
        let Some(rest) = name.strip_prefix("config:") else {
            continue;
        };
        let mut parts = rest.split(':');
        let (file, path) = (parts.next().unwrap(), parts.next().unwrap());
        let config =
            load_str(&fs::read_to_string(crate_dir().join(format!("config/{file}.yaml"))).unwrap())
                .unwrap();
        let (section, key) = path.split_once('.').unwrap();
        assert_eq!(config[section][key], case["template"], "{name}");
        seen.insert(format!("{file}:{path}"));
    }
    // Every template of both shipped configs has goldens.
    for file in ["default", "mini_textbased"] {
        for path in [
            "agent.system_template",
            "agent.instance_template",
            "model.observation_template",
            "model.format_error_template",
        ] {
            assert!(
                seen.contains(&format!("{file}:{path}")),
                "no golden for {file}:{path}"
            );
        }
    }
}

#[test]
fn compiled_templates_render_many_times() {
    let mut templates = Templates::new();
    templates.add("greet", "Hello {{ name }}!").unwrap();
    for name in ["a", "b"] {
        assert_eq!(
            templates.render("greet", &json!({"name": name})).unwrap(),
            format!("Hello {name}!")
        );
    }
    assert!(matches!(
        templates.render("missing", &json!({})),
        Err(TemplateError::Unknown(_))
    ));
}

#[test]
fn undefined_variables_are_errors() {
    assert!(matches!(
        render_once("{{ missing }}", &json!({})),
        Err(TemplateError::Render { .. })
    ));
    assert!(render_once("{{ output.missing.deeper }}", &json!({"output": {}})).is_err());
    // Jinja2's StrictUndefined also refuses truthiness checks.
    assert!(render_once("{% if missing %}x{% endif %}", &json!({})).is_err());
}

#[test]
fn defined_tests_work_on_missing_variables() {
    assert_eq!(
        render_once(
            "{% if missing is defined %}a{% else %}b{% endif %}",
            &json!({})
        )
        .unwrap(),
        "b"
    );
}

#[test]
fn syntax_errors_fail_at_compile_time() {
    let mut templates = Templates::new();
    assert!(matches!(
        templates.add("bad", "{% if %}"),
        Err(TemplateError::Syntax { .. })
    ));
}

#[test]
fn trailing_newline_is_dropped_like_jinja2() {
    assert_eq!(render_once("x\n", &json!({})).unwrap(), "x");
    assert_eq!(render_once("x\n\n", &json!({})).unwrap(), "x\n");
}

#[test]
fn html_is_never_escaped() {
    let html = json!({"s": "<a href=\"x\">&</a>"});
    assert_eq!(
        render_once("{{ s }}", &html).unwrap(),
        "<a href=\"x\">&</a>"
    );
    // minijinja escapes `.html` and `.xml` names by default; the port never does.
    let mut templates = Templates::new();
    for name in ["obs.html", "obs.xml", "obs.htm"] {
        templates.add(name, "{{ s }}").unwrap();
        assert_eq!(
            templates.render(name, &html).unwrap(),
            "<a href=\"x\">&</a>",
            "{name}"
        );
    }
}

#[test]
fn context_must_be_an_object() {
    // Python's `render(**context)` needs a mapping.
    for context in [json!(null), json!([1]), json!("x"), json!(3)] {
        assert!(
            matches!(
                render_once("static", &context),
                Err(TemplateError::ContextNotObject { .. })
            ),
            "{context}"
        );
    }
}
