//! The simplest use of the library (port of `run/hello_world.py`): one agent with the
//! `default.yaml` agent section, a local environment, and the text-based model over an
//! OpenAI-compatible endpoint (Python's example uses the tool-calling `LitellmModel`; plan §2.4).
//!
//! ```bash
//! OPENAI_API_KEY=... MSWEA_COST_TRACKING=ignore_errors \
//!   cargo run --example hello_world -- -m gpt-4o-mini -t "List the files in this directory"
//! ```

use std::sync::Arc;

use anyhow::Context as _;
use clap::Parser;
use mini_swe_agent_rs::agent::{Agent, AgentConfig, DefaultAgent};
use mini_swe_agent_rs::clock::{Clock, SystemClock};
use mini_swe_agent_rs::config::{self, ConfigSource, TextbasedModelConfig};
use mini_swe_agent_rs::environment::local::{LocalEnvironment, LocalEnvironmentConfig};
use mini_swe_agent_rs::host_env::HostEnv;
use mini_swe_agent_rs::model::openai_compat::{OpenAiCompatBackend, Settings};
use mini_swe_agent_rs::model::stats::GlobalModelStats;
use mini_swe_agent_rs::model::textbased::TextbasedModel;
use serde_json::{Map, json};

#[derive(Debug, Parser)]
struct Args {
    /// Task/problem statement.
    #[arg(short = 't', long = "task")]
    task: String,
    /// Model name (defaults to `MSWEA_MODEL_NAME`).
    #[arg(short = 'm', long = "model", env = "MSWEA_MODEL_NAME")]
    model: String,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let host = Arc::new(HostEnv::from_vars(std::env::vars_os()));
    let defaults = config::read_source(&ConfigSource::Builtin("default.yaml".to_owned()))?;
    let defaults = mini_swe_agent_rs::yaml::load_str(&defaults)?;
    let agent_config: AgentConfig = serde_json::from_value(
        defaults
            .get("agent")
            .cloned()
            .context("default.yaml has no agent section")?,
    )?;
    let model_config =
        TextbasedModelConfig::from_section(&json!({"model_name": args.model}), &host)?;
    let backend = OpenAiCompatBackend::new(
        Settings::from_model_kwargs(&model_config.model_kwargs, &host)?.transport,
    );
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let stats = Arc::new(GlobalModelStats::from_host_env(&host)?);
    let model = TextbasedModel::new(model_config, backend, stats, clock.clone(), &host)?;
    let env = LocalEnvironment::new(LocalEnvironmentConfig::default(), host);
    let mut agent = DefaultAgent::new(agent_config, model, env, clock)?;
    let info = agent.run(&args.task, Map::new())?;
    println!("{}: {}", info.exit_status, info.submission);
    Ok(())
}
