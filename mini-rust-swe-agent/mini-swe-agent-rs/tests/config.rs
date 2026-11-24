//! Configuration (spec/migration-plan.md §3.10, phase P7): the port of
//! `tests/config/test_init.py`, the lookup and layering rules, the class registries and the typed
//! dumps of both shipped YAML files against pydantic's `model_dump(mode="json")`.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use mini_swe_agent_rs::config::{
    self, ConfigError, ConfigSource, ModelChoice, Overrides, TextbasedModelConfig,
};
use mini_swe_agent_rs::host_env::HostEnv;
use serde_json::{Value, json};

fn host(pairs: &[(&str, &str)]) -> HostEnv {
    HostEnv::from_vars(
        pairs
            .iter()
            .map(|(k, v)| (OsString::from(k), OsString::from(v))),
    )
}

/// Test helper: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::unwrap_used)]
fn kv(spec: &str) -> Value {
    config::key_value_spec(spec).unwrap()
}

// ---- Port of TestKeyValueSpecToNestedDict -----------------------------------------------------

// `-3.14` is the Python test's value, not an approximation of pi.
#[allow(clippy::approx_constant)]
#[test]
fn key_value_specs() {
    let cases = [
        ("key=value", json!({"key": "value"})),
        ("key=\"value\"", json!({"key": "value"})),
        ("agent.mode=yolo", json!({"agent": {"mode": "yolo"}})),
        ("a.b.c=value", json!({"a": {"b": {"c": "value"}}})),
        (
            "a.b.c.d.e=value",
            json!({"a": {"b": {"c": {"d": {"e": "value"}}}}}),
        ),
        ("count=42", json!({"count": 42})),
        ("value=-10", json!({"value": -10})),
        ("temperature=0.7", json!({"temperature": 0.7})),
        ("value=-3.14", json!({"value": -3.14})),
        ("enabled=true", json!({"enabled": true})),
        ("enabled=false", json!({"enabled": false})),
        ("value=null", json!({"value": null})),
        ("tags=[\"a\",\"b\",\"c\"]", json!({"tags": ["a", "b", "c"]})),
        ("numbers=[1,2,3]", json!({"numbers": [1, 2, 3]})),
        (
            "config={\"key\":\"value\"}",
            json!({"config": {"key": "value"}}),
        ),
        (
            "model.max_tokens=1000",
            json!({"model": {"max_tokens": 1000}}),
        ),
        (
            "model.temperature=0.5",
            json!({"model": {"temperature": 0.5}}),
        ),
        ("agent.verbose=true", json!({"agent": {"verbose": true}})),
        (
            "model.model_name=anthropic/claude-sonnet-4-5-20250929",
            json!({"model": {"model_name": "anthropic/claude-sonnet-4-5-20250929"}}),
        ),
        ("key=\"\"", json!({"key": ""})),
        ("message=\"hello world\"", json!({"message": "hello world"})),
        ("value=0", json!({"value": 0})),
        ("a=b=c", json!({"a": "b=c"})),
    ];
    for (spec, expected) in cases {
        assert_eq!(kv(spec), expected, "{spec}");
    }
    // `0.0` stays a float, as `json.loads` keeps it.
    assert!(kv("value=0.0")["value"].is_f64());
}

#[test]
fn empty_key_segments_are_errors() {
    for spec in ["=value", "model..name=value", "model.=value"] {
        let error = config::key_value_spec(spec).unwrap_err();
        assert!(matches!(error, ConfigError::EmptyKey { .. }), "{spec}");
        assert!(error.to_string().contains("empty config key"), "{error}");
    }
}

// ---- Port of TestGetConfigFromSpec and the lookup rules ---------------------------------------

#[test]
fn spec_with_equals_sign_is_a_key_value_pair() {
    let empty = host(&[]);
    let cwd = std::env::current_dir().unwrap();
    assert_eq!(
        config::config_from_spec("agent.mode=yolo", &empty, &cwd).unwrap(),
        json!({"agent": {"mode": "yolo"}})
    );
    assert_eq!(
        config::config_from_spec("model.max_tokens=500", &empty, &cwd).unwrap(),
        json!({"model": {"max_tokens": 500}})
    );
}

#[test]
fn yaml_files_load_as_utf8() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.yaml");
    fs::write(&file, "key: value\nnumber: 42").unwrap();
    let loaded = config::config_from_spec(file.to_str().unwrap(), &host(&[]), dir.path()).unwrap();
    assert_eq!(loaded, json!({"key": "value", "number": 42}));
    fs::write(&file, "system_template: a — b").unwrap();
    let loaded = config::config_from_spec(file.to_str().unwrap(), &host(&[]), dir.path()).unwrap();
    assert_eq!(
        loaded["system_template"].as_str().unwrap().chars().count(),
        5
    );
}

#[test]
fn builtin_configs_are_findable_by_name() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["default", "mini_textbased"] {
        let source = config::config_path(name, &host(&[]), dir.path()).unwrap();
        assert_eq!(source, ConfigSource::Builtin(format!("{name}.yaml")));
        assert_eq!(
            config::read_source(&source).unwrap(),
            fs::read_to_string(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("config/{name}.yaml"))
            )
            .unwrap()
        );
    }
}

#[test]
fn lookup_order_and_suffix_rule() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().join("cwd");
    let config_dir = dir.path().join("configs");
    fs::create_dir_all(&cwd).unwrap();
    fs::create_dir_all(&config_dir).unwrap();
    let env = host(&[("MSWEA_CONFIG_DIR", config_dir.to_str().unwrap())]);

    // The suffix is replaced by `.yaml`: `mine.yml` and `mine` both look for `mine.yaml`.
    fs::write(config_dir.join("mine.yaml"), "a: 1").unwrap();
    for spec in ["mine", "mine.yml"] {
        assert_eq!(
            config::config_path(spec, &env, &cwd).unwrap(),
            ConfigSource::File(config_dir.join("mine.yaml")),
            "{spec}"
        );
    }
    // The path as given (relative to the cwd) comes first.
    fs::write(cwd.join("mine.yaml"), "a: 2").unwrap();
    assert_eq!(
        config::config_path("mine", &env, &cwd).unwrap(),
        ConfigSource::File(cwd.join("mine.yaml"))
    );
    // MSWEA_CONFIG_DIR beats the built-ins.
    fs::write(config_dir.join("default.yaml"), "a: 3").unwrap();
    assert_eq!(
        config::config_path("default", &env, &cwd).unwrap(),
        ConfigSource::File(config_dir.join("default.yaml"))
    );
    // Without MSWEA_CONFIG_DIR, the second candidate is `./<spec>` (the cwd).
    let other = dir.path().join("other");
    fs::create_dir_all(&other).unwrap();
    assert_eq!(
        config::config_path("default", &host(&[]), &other).unwrap(),
        ConfigSource::Builtin("default.yaml".to_owned())
    );
}

#[test]
fn missing_config_lists_the_candidates() {
    let dir = tempfile::tempdir().unwrap();
    let error = config::config_path("nope", &host(&[]), dir.path()).unwrap_err();
    let text = error.to_string();
    assert!(
        text.starts_with("Could not find config file for nope.yaml"),
        "{text}"
    );
    assert!(
        text.contains("extra/nope.yaml") && text.contains("benchmarks/nope.yaml"),
        "{text}"
    );
}

#[test]
fn empty_config_file_is_a_skipped_layer() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("empty.yaml"), "# nothing\n").unwrap();
    let specs = vec!["empty".to_owned(), "agent.step_limit=3".to_owned()];
    let merged =
        config::merged_config(Some(&specs), &Overrides::default(), &host(&[]), dir.path()).unwrap();
    assert_eq!(merged["agent"]["step_limit"], 3);
}

// ---- Layering -----------------------------------------------------------------------------------

#[test]
fn explicit_specs_replace_the_default_and_flags_come_last() {
    let dir = tempfile::tempdir().unwrap();
    let overrides = Overrides {
        task: Some("from flag".into()),
        cost_limit: Some(0.0),
        output: Some(PathBuf::from("/tmp/out.json")),
        model_name: Some("deterministic".into()),
        model_class: Some("deterministic".into()),
        agent_class: None,
        environment_class: None,
    };
    let specs = vec![
        "run.task=from config".to_owned(),
        "agent.cost_limit=5".to_owned(),
    ];
    let merged = config::merged_config(Some(&specs), &overrides, &host(&[]), dir.path()).unwrap();
    assert_eq!(merged["run"]["task"], "from flag");
    assert_eq!(
        merged["agent"]["cost_limit"], 0.0,
        "`-l 0` is given, so it is inserted"
    );
    assert_eq!(merged["agent"]["output_path"], "/tmp/out.json");
    assert!(
        merged["agent"].get("agent_class").is_none(),
        "a flag not given is not inserted"
    );
    assert!(
        merged
            .get("environment")
            .is_none_or(|e| e.get("environment_class").is_none())
    );
    assert!(
        merged["agent"].get("system_template").is_none(),
        "the default config is replaced"
    );

    let defaulted =
        config::merged_config(None, &Overrides::default(), &host(&[]), dir.path()).unwrap();
    assert!(
        defaulted["agent"]["system_template"].is_string(),
        "embedded mini_textbased.yaml"
    );
    assert_eq!(defaulted["agent"]["cost_limit"], 3.0);
}

#[test]
fn mini_config_path_overrides_the_default() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("custom.yaml");
    fs::write(&file, "agent:\n  step_limit: 9\n").unwrap();
    let env = host(&[("MSWEA_MINI_CONFIG_PATH", file.to_str().unwrap())]);
    let merged = config::merged_config(None, &Overrides::default(), &env, dir.path()).unwrap();
    assert_eq!(merged["agent"], json!({"step_limit": 9}));
}

#[test]
fn task_comes_from_the_merged_config() {
    let merged = json!({"run": {"task": "x"}});
    assert_eq!(
        config::task(merged.as_object().unwrap()),
        Some("x".to_owned())
    );
    assert_eq!(config::task(json!({}).as_object().unwrap()), None);
}

// ---- Class registries --------------------------------------------------------------------------

fn section(value: Value) -> Value {
    value
}

#[test]
fn model_class_and_name_resolution() {
    let env = host(&[]);
    let choice = config::model_choice(
        &section(
            json!({"model_class": "deterministic", "model_name": "deterministic", "outputs": []}),
        ),
        &env,
    )
    .unwrap();
    let ModelChoice::Deterministic(config) = choice;
    assert_eq!(config.model_name, "deterministic");

    // MSWEA_MODEL_NAME is the last source of the model name.
    let env_named = host(&[("MSWEA_MODEL_NAME", "from-env")]);
    let ModelChoice::Deterministic(config) = config::model_choice(
        &json!({"model_class": "deterministic", "outputs": []}),
        &env_named,
    )
    .unwrap();
    assert_eq!(config.model_name, "from-env");

    assert!(matches!(
        config::model_choice(
            &json!({"model_class": "deterministic", "outputs": []}),
            &env
        ),
        Err(ConfigError::NoModelName)
    ));
    for class in [None, Some("textbased"), Some("litellm_textbased")] {
        let mut model = json!({"model_name": "gpt-x"});
        if let Some(class) = class {
            model["model_class"] = json!(class);
        }
        let error = config::model_choice(&model, &env).unwrap_err();
        assert!(
            matches!(error, ConfigError::NotYetAvailable(_)),
            "{class:?}: {error}"
        );
        assert!(error.to_string().contains("P8"), "{error}");
    }
    assert!(matches!(
        config::model_choice(&json!({"model_name": "m", "model_class": "litellm"}), &env),
        Err(ConfigError::UnknownClass { .. })
    ));
}

#[test]
fn environment_and_agent_classes() {
    let env =
        config::environment_config(&json!({"environment_class": "local", "timeout": 5})).unwrap();
    assert_eq!(env.timeout, 5);
    assert!(config::environment_config(&json!({})).is_ok());
    assert!(matches!(
        config::environment_config(&json!({"environment_class": "docker"})),
        Err(ConfigError::UnknownClass { .. })
    ));
    let agent = json!({"system_template": "s", "instance_template": "i", "agent_class": "default", "mode": "confirm"});
    assert_eq!(config::agent_config(&agent).unwrap().cost_limit, 3.0);
    assert!(
        config::agent_config(&json!({"system_template": "s", "instance_template": "i"})).is_ok()
    );
    for class in ["interactive", "nope"] {
        let mut agent = agent.clone();
        agent["agent_class"] = json!(class);
        assert!(
            matches!(
                config::agent_config(&agent),
                Err(ConfigError::UnknownClass { .. })
            ),
            "{class}"
        );
    }
    assert!(matches!(
        config::agent_config(&json!({"system_template": "s"})),
        Err(ConfigError::Section { .. })
    ));
}

// ---- Typed dumps against pydantic ------------------------------------------------------------

/// Test helper: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::unwrap_used)]
fn golden(name: &str) -> Value {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/goldens/config/{name}.json"));
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn typed_dumps_match_pydantic() {
    for name in ["default", "mini_textbased"] {
        let source = ConfigSource::Builtin(format!("{name}.yaml"));
        let raw =
            mini_swe_agent_rs::yaml::load_str(&config::read_source(&source).unwrap()).unwrap();
        let expected = golden(name);
        let agent = config::agent_config(&raw["agent"]).unwrap();
        assert_eq!(
            serde_json::to_value(agent).unwrap(),
            expected["agent"],
            "{name} agent"
        );
        let env = config::environment_config(&raw["environment"]).unwrap();
        assert_eq!(
            serde_json::to_value(env).unwrap(),
            expected["environment"],
            "{name} environment"
        );
        let mut model = raw["model"].clone();
        model["model_name"] = json!("test-model");
        let model = TextbasedModelConfig::from_section(&model, &host(&[])).unwrap();
        assert_eq!(
            serde_json::to_value(model).unwrap(),
            expected["model"],
            "{name} model"
        );
    }
}

#[test]
fn textbased_env_backed_defaults() {
    let env = host(&[
        ("LITELLM_MODEL_REGISTRY_PATH", "/r.json"),
        ("MSWEA_COST_TRACKING", "ignore_errors"),
    ]);
    let model = TextbasedModelConfig::from_section(&json!({"model_name": "m"}), &env).unwrap();
    assert_eq!(model.litellm_model_registry.as_deref(), Some("/r.json"));
    assert_eq!(model.cost_tracking, "ignore_errors");
    let explicit =
        json!({"model_name": "m", "cost_tracking": "default", "litellm_model_registry": null});
    let model = TextbasedModelConfig::from_section(&explicit, &env).unwrap();
    assert_eq!(
        model.cost_tracking, "default",
        "a value in the config wins over the environment"
    );
    assert_eq!(model.litellm_model_registry, None);
    assert!(
        TextbasedModelConfig::from_section(&json!({}), &host(&[])).is_err(),
        "model_name is required"
    );
}
