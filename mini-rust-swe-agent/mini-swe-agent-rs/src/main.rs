//! `mini-swe-agent-rs`: the command-line entry point (port of `run/mini.py`, plan §3.12).
//!
//! It does what Python does at import time, explicitly and in this order: snapshot the process
//! environment, resolve and create the global config dir, print the banner unless
//! `MSWEA_SILENT_STARTUP` is set, and overlay the dir's `.env` without overriding existing
//! variables (Python's order; the plan listed the banner last). Exit codes:
//! 0 when the run ends with an exit message (any status), 1 for a fatal or config error (after
//! the trajectory was saved, when there is one), 2 for usage errors.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::Context as _;
use clap::Parser;
use clap::builder::TypedValueParser as _;
use mini_swe_agent_rs::agent::{Agent, AgentConfig, DefaultAgent};
use mini_swe_agent_rs::clock::{Clock, SystemClock};
use mini_swe_agent_rs::config::{self, ModelChoice, Overrides};
use mini_swe_agent_rs::environment::local::LocalEnvironment;
use mini_swe_agent_rs::host_env::HostEnv;
use mini_swe_agent_rs::message::ExitInfo;
use mini_swe_agent_rs::model::Model;
use mini_swe_agent_rs::model::deterministic::DeterministicModel;
use mini_swe_agent_rs::model::openai_compat::{OpenAiCompatBackend, Settings};
use mini_swe_agent_rs::model::stats::GlobalModelStats;
use mini_swe_agent_rs::model::textbased::TextbasedModel;
use serde_json::Map;

/// Run mini-swe-agent-rs in your local environment.
#[derive(Debug, Parser)]
#[command(name = "mini-swe-agent-rs", version)]
struct Cli {
    /// Model to use.
    #[arg(short = 'm', long = "model")]
    model: Option<String>,
    /// Model class: `textbased` (the default; OpenAI-compatible endpoint) or `deterministic`.
    #[arg(long = "model-class")]
    model_class: Option<String>,
    /// Agent class: only `default`.
    #[arg(long = "agent-class")]
    agent_class: Option<String>,
    /// Environment class: only `local`.
    #[arg(long = "environment-class")]
    environment_class: Option<String>,
    /// Task/problem statement (else `run.task` in the config, else stdin).
    #[arg(short = 't', long = "task")]
    task: Option<String>,
    /// Interactive confirmation is not ported (D7).
    #[arg(short = 'y', long = "yolo", hide = true)]
    yolo: bool,
    /// Cost limit. Set to 0 to disable.
    #[arg(short = 'l', long = "cost-limit", value_parser = finite_f64)]
    cost_limit: Option<f64>,
    /// Config files, file names or key=value pairs; repeatable. Replaces the default config.
    #[arg(short = 'c', long = "config")]
    config: Vec<String>,
    /// Output trajectory file (default: <global config dir>/last_mini_run.traj.json).
    /// An empty value means "not given", as Python's `output or UNSET`.
    #[arg(short = 'o', long = "output", value_parser = clap::builder::OsStringValueParser::new().map(PathBuf::from))]
    output: Option<PathBuf>,
    /// Interactive exit prompts are not ported (D7).
    #[arg(long = "exit-immediately", hide = true)]
    exit_immediately: bool,
}

/// A cost limit must be a finite number (`nan` and `inf` would become JSON `null`).
fn finite_f64(text: &str) -> Result<f64, String> {
    match text.trim().parse::<f64>() {
        Ok(value) if value.is_finite() => Ok(value),
        Ok(_) => Err("must be a finite number".to_owned()),
        Err(e) => Err(e.to_string()),
    }
}

fn say(text: &str) {
    let _ = writeln!(std::io::stderr(), "{text}");
}

/// `MSWEA_GLOBAL_CONFIG_DIR`, else the platform config dir (`platformdirs.user_config_dir`).
fn global_config_dir(host: &HostEnv) -> PathBuf {
    match host
        .get("MSWEA_GLOBAL_CONFIG_DIR")
        .filter(|dir| !dir.is_empty())
    {
        Some(dir) => PathBuf::from(dir),
        None => dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("mini-swe-agent"),
    }
}

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            // `--help` and `--version` arrive here too, with exit code 0.
            let _ = error.print();
            return ExitCode::from(u8::try_from(error.exit_code()).unwrap_or(2));
        }
    };
    if cli.yolo || cli.exit_immediately {
        say(
            "error: -y/--yolo and --exit-immediately belong to the interactive agent, which is not ported (D7)",
        );
        return ExitCode::from(2);
    }
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            say(&format!("error: {error:#}"));
            ExitCode::from(1)
        }
    }
}

fn run(cli: Cli) -> anyhow::Result<()> {
    let mut host = HostEnv::from_vars(std::env::vars_os());
    let config_dir = global_config_dir(&host);
    std::fs::create_dir_all(&config_dir)
        .with_context(|| format!("creating {}", config_dir.display()))?;
    let env_file = config_dir.join(".env");
    // Python prints the banner before `load_dotenv`, so `.env` cannot silence it.
    if host.get("MSWEA_SILENT_STARTUP").is_none_or(str::is_empty) {
        say(&format!(
            "This is mini-swe-agent-rs version {}.\nLoading global config from '{}'",
            env!("CARGO_PKG_VERSION"),
            env_file.display()
        ));
    }
    if env_file.is_file() {
        // `load_dotenv` reads the whole file first (the last assignment of a name wins), then
        // skips the names the process already has.
        let pairs: BTreeMap<String, String> = dotenvy::from_path_iter(&env_file)
            .and_then(|iter| iter.collect())
            .with_context(|| format!("reading {}", env_file.display()))?;
        host.overlay_missing(pairs);
    }
    let host = Arc::new(host);
    let cwd = std::env::current_dir().context("current directory")?;

    let overrides = Overrides {
        task: cli.task,
        agent_class: cli.agent_class,
        cost_limit: cli.cost_limit,
        output: Some(
            cli.output
                .unwrap_or_else(|| config_dir.join("last_mini_run.traj.json")),
        ),
        model_class: cli.model_class,
        model_name: cli.model,
        environment_class: cli.environment_class,
    };
    let specs = (!cli.config.is_empty()).then_some(cli.config.as_slice());
    let mut merged = config::merged_config(specs, &overrides, &host, &cwd)?;
    let task = config::task(&merged)?;
    // Each section is taken once; an absent one is `{}` (`config.get(name, {})`).
    let mut section = |name: &str| merged.remove(name).unwrap_or_else(|| serde_json::json!({}));
    let model_choice = config::model_choice(&section("model"), &host)?;
    let env_config = config::environment_config(&section("environment"))?;
    let agent_config = config::agent_config(&section("agent"))?;
    let task = match task {
        Some(task) => task,
        None => {
            // Python prompts for a multi-line task; this port reads stdin to EOF (plan §2.4).
            say("What do you want to do? (reading the task from stdin until EOF)");
            let mut task = String::new();
            std::io::stdin()
                .read_to_string(&mut task)
                .context("reading the task from stdin")?;
            task
        }
    };

    let stats = Arc::new(GlobalModelStats::from_host_env(&host)?);
    if (stats.cost_limit() > 0.0 || stats.call_limit() > 0)
        && host.get("MSWEA_SILENT_STARTUP").is_none_or(str::is_empty)
    {
        say(&format!(
            "Global cost/call limit: ${:.4} / {}",
            stats.cost_limit(),
            stats.call_limit()
        ));
    }
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let env = LocalEnvironment::new(env_config, host.clone());
    let output_path = agent_config.output_path.clone();
    // One generic run per model type (plan §3.3: static dispatch).
    let result = match model_choice {
        ModelChoice::Deterministic(model_config) => {
            let model = DeterministicModel::new(model_config, stats, clock.clone())?;
            run_agent(agent_config, model, env, clock, &task)?
        }
        ModelChoice::Textbased(model_config) => {
            let settings = Settings::from_model_kwargs(&model_config.model_kwargs, &host)?;
            let backend = OpenAiCompatBackend::new(settings.transport);
            let model = TextbasedModel::new(model_config, backend, stats, clock.clone(), &host)?;
            run_agent(agent_config, model, env, clock, &task)?
        }
    };
    // Python prints this only after `run` returned.
    if let Some(path) = output_path {
        say(&format!("Saved trajectory to '{}'", path.display()));
    }
    say(&format!("Exit status: {}", result.exit_status));
    Ok(())
}

fn run_agent<M: Model>(
    config: AgentConfig,
    model: M,
    env: LocalEnvironment,
    clock: Arc<dyn Clock>,
    task: &str,
) -> anyhow::Result<ExitInfo> {
    let mut agent = DefaultAgent::new(config, model, env, clock)?;
    Ok(agent.run(task, Map::new())?)
}
