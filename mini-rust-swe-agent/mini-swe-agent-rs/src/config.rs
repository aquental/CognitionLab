//! Configuration (port of `config/__init__.py` and the layering of `run/mini.py:72-92`, plan
//! §3.10).
//!
//! - Each `-c` spec is a layer: `key.path=value` (the value parsed as JSON, else kept as a
//!   string) or a YAML file found by the lookup of `get_config_path`.
//! - An explicit `-c` list replaces the default config, which is `$MSWEA_MINI_CONFIG_PATH` or the
//!   embedded `mini_textbased.yaml`.
//! - CLI flags are one more layer, added last; a flag that was not given is not inserted.
//! - The layers merge with `recursive_merge`; each section is then deserialized into its typed
//!   config. The class keys are read first and removed, as the Python factories `pop` them.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::agent::AgentConfig;
use crate::environment::local::LocalEnvironmentConfig;
use crate::host_env::HostEnv;
use crate::merge::{MergeError, recursive_merge};
use crate::model::actions_text::DEFAULT_ACTION_REGEX;
use crate::model::deterministic::DeterministicModelConfig;
use crate::yaml::{YamlError, load_str};

/// The built-in configs, embedded in the binary: the text path only (plan §2.3).
pub const BUILTIN_CONFIGS: &[(&str, &str)] = &[
    ("default.yaml", include_str!("../config/default.yaml")),
    (
        "mini_textbased.yaml",
        include_str!("../config/mini_textbased.yaml"),
    ),
];

/// The default config when no `-c` is given (Python's is the tool-calling `mini.yaml`; plan §2.4).
pub const DEFAULT_CONFIG: &str = "mini_textbased.yaml";

/// Where a config file comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigSource {
    /// A file on disk.
    File(PathBuf),
    /// An embedded built-in config, by file name.
    Builtin(String),
}

/// Configuration errors. They are fatal before the run starts.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// A `key=value` spec has an empty key segment (`config/__init__.py:46-47`).
    #[error("Invalid config spec {spec:?}: empty config key")]
    EmptyKey {
        /// The spec.
        spec: String,
    },
    /// No lookup candidate exists (`config/__init__.py:28`).
    #[error("Could not find config file for {spec} (tried: {tried})")]
    NotFound {
        /// The spec after the suffix rule.
        spec: String,
        /// The candidates, in lookup order.
        tried: String,
    },
    /// A config file cannot be read.
    #[error("{path}: {source}")]
    Read {
        /// The file.
        path: PathBuf,
        /// The error.
        source: std::io::Error,
    },
    /// A config file is not valid YAML.
    #[error("{name}: {source}")]
    Yaml {
        /// The file or built-in name.
        name: String,
        /// The error.
        source: YamlError,
    },
    /// The layers do not merge (a layer is not a mapping).
    #[error(transparent)]
    Merge(#[from] MergeError),
    /// A section does not deserialize into its typed config.
    #[error("invalid `{section}` config: {source}")]
    Section {
        /// `agent`, `environment` or `model`.
        section: &'static str,
        /// The error.
        source: serde_json::Error,
    },
    /// No model name from `-m`, the config or `MSWEA_MODEL_NAME` (`models/__init__.py:70-79`).
    #[error(
        "No default model set. Pass -m/--model, set model.model_name, or set MSWEA_MODEL_NAME."
    )]
    NoModelName,
    /// An agent, environment or model class this port does not have.
    #[error("Unknown {kind} class: {name} (available: {available})")]
    UnknownClass {
        /// `agent`, `environment` or `model`.
        kind: &'static str,
        /// The requested class.
        name: String,
        /// What this build accepts.
        available: &'static str,
    },
    /// A model class planned for a later phase.
    #[error(
        "model class `{0}` is available from phase P8 (the text-based model); use --model-class deterministic"
    )]
    NotYetAvailable(String),
}

/// `_key_value_spec_to_nested_dict` (`config/__init__.py:31-53`).
pub fn key_value_spec(spec: &str) -> Result<Value, ConfigError> {
    let (key, raw) = spec.split_once('=').unwrap_or((spec, ""));
    let value =
        serde_json::from_str::<Value>(raw).unwrap_or_else(|_| Value::String(raw.to_owned()));
    let keys: Vec<&str> = key.split('.').collect();
    if keys.iter().any(|k| k.is_empty()) {
        return Err(ConfigError::EmptyKey {
            spec: spec.to_owned(),
        });
    }
    Ok(keys.iter().rev().fold(value, |inner, key| {
        let mut map = Map::new();
        map.insert((*key).to_owned(), inner);
        Value::Object(map)
    }))
}

/// `Path.with_suffix(".yaml")` when the suffix is not already `.yaml`.
fn with_yaml_suffix(spec: &str) -> PathBuf {
    let path = PathBuf::from(spec);
    if path.extension().is_some_and(|ext| ext == "yaml") {
        path
    } else {
        path.with_extension("yaml")
    }
}

/// `get_config_path` (`config/__init__.py:12-28`): the path as given (relative to `cwd`), then
/// `$MSWEA_CONFIG_DIR` (default `.`), then the built-ins. The `extra/` and `benchmarks/`
/// candidates keep their place in the order but have no embedded content in v1.
pub fn config_path(spec: &str, host: &HostEnv, cwd: &Path) -> Result<ConfigSource, ConfigError> {
    let spec = with_yaml_suffix(spec);
    let config_dir = PathBuf::from(host.get("MSWEA_CONFIG_DIR").unwrap_or("."));
    let on_disk = [spec.clone(), config_dir.join(&spec)];
    let mut tried = Vec::new();
    for candidate in &on_disk {
        let absolute = cwd.join(candidate);
        tried.push(candidate.display().to_string());
        if absolute.exists() {
            return Ok(ConfigSource::File(absolute));
        }
    }
    let name = spec.to_string_lossy().into_owned();
    if BUILTIN_CONFIGS.iter().any(|(builtin, _)| *builtin == name) {
        return Ok(ConfigSource::Builtin(name));
    }
    for prefix in ["<builtin>", "<builtin>/extra", "<builtin>/benchmarks"] {
        tried.push(format!("{prefix}/{name}"));
    }
    Err(ConfigError::NotFound {
        spec: name,
        tried: tried.join(", "),
    })
}

/// The text of a config source.
pub fn read_source(source: &ConfigSource) -> Result<String, ConfigError> {
    match source {
        ConfigSource::File(path) => {
            std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
                path: path.clone(),
                source,
            })
        }
        ConfigSource::Builtin(name) => BUILTIN_CONFIGS
            .iter()
            .find(|(builtin, _)| builtin == name)
            .map(|(_, text)| (*text).to_owned())
            .ok_or_else(|| ConfigError::NotFound {
                spec: name.clone(),
                tried: format!("<builtin>/{name}"),
            }),
    }
}

fn load_source(source: &ConfigSource) -> Result<Value, ConfigError> {
    let name = match source {
        ConfigSource::File(path) => path.display().to_string(),
        ConfigSource::Builtin(name) => format!("<builtin>/{name}"),
    };
    load_str(&read_source(source)?).map_err(|source| ConfigError::Yaml { name, source })
}

/// `get_config_from_spec` (`config/__init__.py:56-61`): a spec with `=` is a key-value pair.
pub fn config_from_spec(spec: &str, host: &HostEnv, cwd: &Path) -> Result<Value, ConfigError> {
    if spec.contains('=') {
        return key_value_spec(spec);
    }
    load_source(&config_path(spec, host, cwd)?)
}

/// The CLI flags that become the last layer (`run/mini.py:74-92`). `None` means "not given".
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overrides {
    /// `-t/--task` → `run.task`.
    pub task: Option<String>,
    /// `--agent-class` → `agent.agent_class`.
    pub agent_class: Option<String>,
    /// `-l/--cost-limit` → `agent.cost_limit`.
    pub cost_limit: Option<f64>,
    /// `-o/--output` → `agent.output_path` (the CLI always sets it).
    pub output: Option<PathBuf>,
    /// `--model-class` → `model.model_class`.
    pub model_class: Option<String>,
    /// `-m/--model` → `model.model_name`.
    pub model_name: Option<String>,
    /// `--environment-class` → `environment.environment_class`.
    pub environment_class: Option<String>,
}

impl Overrides {
    /// The layer, with only the given flags. Python skips empty strings too (`task or UNSET`).
    pub fn layer(&self) -> Value {
        fn put(map: &mut Map<String, Value>, section: &str, key: &str, value: Value) {
            if let Value::Object(inner) = map
                .entry(section)
                .or_insert_with(|| Value::Object(Map::new()))
            {
                inner.insert(key.to_owned(), value);
            }
        }
        let text = |value: &Option<String>| {
            value
                .as_ref()
                .filter(|v| !v.is_empty())
                .map(|v| Value::String(v.clone()))
        };
        let mut layer = Map::new();
        if let Some(task) = text(&self.task) {
            put(&mut layer, "run", "task", task);
        }
        if let Some(class) = text(&self.agent_class) {
            put(&mut layer, "agent", "agent_class", class);
        }
        if let Some(limit) = self.cost_limit {
            put(&mut layer, "agent", "cost_limit", Value::from(limit));
        }
        if let Some(output) = &self.output {
            put(
                &mut layer,
                "agent",
                "output_path",
                Value::String(output.to_string_lossy().into_owned()),
            );
        }
        if let Some(class) = text(&self.model_class) {
            put(&mut layer, "model", "model_class", class);
        }
        if let Some(name) = text(&self.model_name) {
            put(&mut layer, "model", "model_name", name);
        }
        if let Some(class) = text(&self.environment_class) {
            put(&mut layer, "environment", "environment_class", class);
        }
        Value::Object(layer)
    }
}

/// Loads the specs (or the default config when `specs` is `None`), adds the flags layer, and
/// merges. An empty file is a `null` layer, which the merge skips.
pub fn merged_config(
    specs: Option<&[String]>,
    overrides: &Overrides,
    host: &HostEnv,
    cwd: &Path,
) -> Result<Map<String, Value>, ConfigError> {
    let mut layers = Vec::new();
    match specs {
        Some(specs) => {
            for spec in specs {
                layers.push(config_from_spec(spec, host, cwd)?);
            }
        }
        // Python passes `str(DEFAULT_CONFIG_FILE)` as a spec, so the override follows the
        // same lookup; the embedded default is used directly, so a file in the cwd cannot
        // shadow it.
        None => match host.get("MSWEA_MINI_CONFIG_PATH") {
            Some(path) => layers.push(config_from_spec(path, host, cwd)?),
            None => layers.push(load_source(&ConfigSource::Builtin(
                DEFAULT_CONFIG.to_owned(),
            ))?),
        },
    }
    layers.push(overrides.layer());
    Ok(recursive_merge(&layers)?)
}

/// `run.task` of the merged config, if any.
pub fn task(config: &Map<String, Value>) -> Option<String> {
    config
        .get("run")
        .and_then(|run| run.get("task"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// A section as a mutable object with its class key removed.
fn take_class(section: &Value, key: &str) -> (Map<String, Value>, Option<String>) {
    let mut map = section.as_object().cloned().unwrap_or_default();
    let class = map
        .remove(key)
        .and_then(|v| v.as_str().map(str::to_owned))
        .filter(|v| !v.is_empty());
    (map, class)
}

fn typed<T: serde::de::DeserializeOwned>(
    section: &'static str,
    map: Map<String, Value>,
) -> Result<T, ConfigError> {
    serde_json::from_value(Value::Object(map))
        .map_err(|source| ConfigError::Section { section, source })
}

/// `get_agent` (`agents/__init__.py:26-29`): only `default` exists in this port. Python's CLI
/// default is `interactive` (out of scope, D7), so an absent class means `default`.
pub fn agent_config(section: &Value) -> Result<AgentConfig, ConfigError> {
    let (map, class) = take_class(section, "agent_class");
    match class.as_deref() {
        None | Some("default" | "minisweagent.agents.default.DefaultAgent") => typed("agent", map),
        Some(other) => Err(ConfigError::UnknownClass {
            kind: "agent",
            name: other.to_owned(),
            available: "default",
        }),
    }
}

/// `get_environment` (`environments/__init__.py:31-34`): only `local` exists in this port.
pub fn environment_config(section: &Value) -> Result<LocalEnvironmentConfig, ConfigError> {
    let (map, class) = take_class(section, "environment_class");
    match class.as_deref() {
        None | Some("local" | "minisweagent.environments.local.LocalEnvironment") => {
            typed("environment", map)
        }
        Some(other) => Err(ConfigError::UnknownClass {
            kind: "environment",
            name: other.to_owned(),
            available: "local",
        }),
    }
}

/// The model the config asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum ModelChoice {
    /// `--model-class deterministic`.
    Deterministic(DeterministicModelConfig),
}

/// `get_model` (`models/__init__.py:43-60`): the model name is resolved from the section, then
/// `MSWEA_MODEL_NAME`; the class key is removed. The text-based model arrives in P8.
pub fn model_choice(section: &Value, host: &HostEnv) -> Result<ModelChoice, ConfigError> {
    let (mut map, class) = take_class(section, "model_class");
    let name = map
        .get("model_name")
        .and_then(Value::as_str)
        .filter(|n| !n.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            host.get("MSWEA_MODEL_NAME")
                .filter(|n| !n.is_empty())
                .map(str::to_owned)
        })
        .ok_or(ConfigError::NoModelName)?;
    map.insert("model_name".to_owned(), Value::String(name));
    match class.as_deref() {
        Some("deterministic" | "minisweagent.models.test_models.DeterministicModel") => {
            Ok(ModelChoice::Deterministic(typed("model", map)?))
        }
        None
        | Some(
            "textbased"
            | "litellm_textbased"
            | "minisweagent.models.litellm_textbased_model.LitellmTextbasedModel",
        ) => Err(ConfigError::NotYetAvailable(
            class.unwrap_or_else(|| "textbased".to_owned()),
        )),
        Some(other) => Err(ConfigError::UnknownClass {
            kind: "model",
            name: other.to_owned(),
            available: "deterministic (textbased from P8)",
        }),
    }
}

fn default_model_kwargs() -> Map<String, Value> {
    Map::new()
}

fn default_cost_tracking() -> String {
    "default".to_owned()
}

fn default_format_error_template() -> String {
    "Please always provide EXACTLY ONE action in triple backticks, found {{actions|length}} actions.".to_owned()
}

fn default_observation_template() -> String {
    crate::model::deterministic::DEFAULT_OBSERVATION_TEMPLATE.to_owned()
}

fn default_action_regex() -> String {
    DEFAULT_ACTION_REGEX.to_owned()
}

/// `LitellmTextbasedModelConfig` (`litellm_model.py:25-46`, `litellm_textbased_model.py:7-14`),
/// so its typed dump can be compared with pydantic's before the model exists (P8). Field order
/// follows `model_dump`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextbasedModelConfig {
    /// Model name (required).
    pub model_name: String,
    /// Extra arguments for the provider API.
    #[serde(default = "default_model_kwargs")]
    pub model_kwargs: Map<String, Value>,
    /// Model registry path; defaults to `$LITELLM_MODEL_REGISTRY_PATH`.
    #[serde(default)]
    pub litellm_model_registry: Option<String>,
    /// Cache-control mode (kept, ignored on the text path).
    #[serde(default)]
    pub set_cache_control: Option<String>,
    /// `default` or `ignore_errors`; defaults to `$MSWEA_COST_TRACKING`, else `default`.
    #[serde(default = "default_cost_tracking")]
    pub cost_tracking: String,
    /// Template for a reply without exactly one action.
    #[serde(default = "default_format_error_template")]
    pub format_error_template: String,
    /// Template of an observation.
    #[serde(default = "default_observation_template")]
    pub observation_template: String,
    /// Kept for config parity; multimodal content is out of scope.
    #[serde(default)]
    pub multimodal_regex: String,
    /// The regex that finds the action.
    #[serde(default = "default_action_regex")]
    pub action_regex: String,
}

impl TextbasedModelConfig {
    /// Deserializes the section. The two environment-backed defaults are read from `host` when
    /// the key is absent, as pydantic reads `os.getenv` for a missing field.
    pub fn from_section(section: &Value, host: &HostEnv) -> Result<Self, ConfigError> {
        let mut map = section.as_object().cloned().unwrap_or_default();
        if !map.contains_key("litellm_model_registry")
            && let Some(path) = host.get("LITELLM_MODEL_REGISTRY_PATH")
        {
            map.insert(
                "litellm_model_registry".to_owned(),
                Value::String(path.to_owned()),
            );
        }
        if !map.contains_key("cost_tracking")
            && let Some(mode) = host.get("MSWEA_COST_TRACKING")
        {
            map.insert("cost_tracking".to_owned(), Value::String(mode.to_owned()));
        }
        typed("model", map)
    }
}
