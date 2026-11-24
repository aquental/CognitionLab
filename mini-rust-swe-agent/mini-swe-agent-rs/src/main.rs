//! `mini-swe-agent-rs`: the command-line entry point (port of `run/mini.py`, plan §3.12).
//!
//! It does what Python does at import time, explicitly and in this order: snapshot the process
//! environment, resolve and create the global config dir, overlay its `.env` without overriding
//! existing variables, and print the banner unless `MSWEA_SILENT_STARTUP` is set. Exit codes:
//! 0 when the run ends with an exit message (any status), 1 for a fatal or config error (after
//! the trajectory was saved, when there is one), 2 for usage errors.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use clap::Parser;
use mini_swe_agent_rs::agent::{Agent, DefaultAgent};
use mini_swe_agent_rs::clock::{Clock, SystemClock};
use mini_swe_agent_rs::config::{self, ModelChoice, Overrides};
use mini_swe_agent_rs::environment::local::LocalEnvironment;
use mini_swe_agent_rs::host_env::HostEnv;
use mini_swe_agent_rs::model::deterministic::DeterministicModel;
use mini_swe_agent_rs::model::stats::GlobalModelStats;
use serde_json::Map;

/// Run mini-swe-agent-rs in your local environment.
#[derive(Debug, Parser)]
#[command(name = "mini-swe-agent-rs", version)]
struct Cli {
    /// Model to use.
    #[arg(short = 'm', long = "model")]
    model: Option<String>,
    /// Model class: `deterministic` (the text-based model arrives in P8).
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
    #[arg(short = 'l', long = "cost-limit")]
    cost_limit: Option<f64>,
    /// Config files, file names or key=value pairs; repeatable. Replaces the default config.
    #[arg(short = 'c', long = "config")]
    config: Vec<String>,
    /// Output trajectory file (default: <global config dir>/last_mini_run.traj.json).
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,
    /// Interactive exit prompts are not ported (D7).
    #[arg(long = "exit-immediately", hide = true)]
    exit_immediately: bool,
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
            let _ = error.print();
            return ExitCode::from(2);
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
        Err(message) => {
            say(&format!("error: {message}"));
            ExitCode::from(1)
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    let mut host = HostEnv::from_vars(std::env::vars_os());
    let config_dir = global_config_dir(&host);
    std::fs::create_dir_all(&config_dir).map_err(|e| format!("{}: {e}", config_dir.display()))?;
    let env_file = config_dir.join(".env");
    if env_file.is_file() {
        let pairs = dotenvy::from_path_iter(&env_file)
            .map_err(|e| format!("{}: {e}", env_file.display()))?
            .collect::<Result<Vec<(String, String)>, _>>()
            .map_err(|e| format!("{}: {e}", env_file.display()))?;
        host.overlay_missing(pairs);
    }
    if host.get("MSWEA_SILENT_STARTUP").is_none_or(str::is_empty) {
        say(&format!(
            "This is mini-swe-agent-rs version {}.\nLoading global config from '{}'",
            env!("CARGO_PKG_VERSION"),
            env_file.display()
        ));
    }
    let host = Arc::new(host);
    let cwd = std::env::current_dir().map_err(|e| format!("current directory: {e}"))?;

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
    let merged =
        config::merged_config(specs, &overrides, &host, &cwd).map_err(|e| e.to_string())?;
    let section = |name: &str| {
        merged
            .get(name)
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}))
    };

    let agent_config = config::agent_config(&section("agent")).map_err(|e| e.to_string())?;
    let env_config =
        config::environment_config(&section("environment")).map_err(|e| e.to_string())?;
    let model_choice = config::model_choice(&section("model"), &host).map_err(|e| e.to_string())?;
    let task = match config::task(&merged) {
        Some(task) => task,
        None => {
            // Python prompts for a multi-line task; this port reads stdin to EOF (plan §2.4).
            say("What do you want to do? (reading the task from stdin until EOF)");
            let mut task = String::new();
            std::io::stdin()
                .read_to_string(&mut task)
                .map_err(|e| format!("stdin: {e}"))?;
            task
        }
    };

    let stats = Arc::new(GlobalModelStats::from_host_env(&host).map_err(|e| e.to_string())?);
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
    let result = match model_choice {
        ModelChoice::Deterministic(model_config) => {
            let model = DeterministicModel::new(model_config, stats, clock.clone())
                .map_err(|e| e.to_string())?;
            let mut agent =
                DefaultAgent::new(agent_config, model, env, clock).map_err(|e| e.to_string())?;
            agent.run(&task, Map::new())
        }
    };
    if let Some(path) = output_path {
        say(&format!("Saved trajectory to '{}'", path.display()));
    }
    result
        .map(|info| say(&format!("Exit status: {}", info.exit_status)))
        .map_err(|e| e.to_string())
}
