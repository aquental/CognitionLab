//! One manual run against a real OpenAI-compatible endpoint (phase P8). Opt-in, because it needs
//! an API key and costs money:
//!
//! ```bash
//! OPENAI_API_KEY=... MSWEA_RS_REAL_MODEL=gpt-4o-mini \
//!   [MSWEA_RS_REAL_REGISTRY=registry.json] [MSWEA_RS_REAL_API_BASE=https://…/v1] \
//!   cargo test --test real_endpoint -- --ignored --nocapture
//! ```
//!
//! With `MSWEA_RS_REAL_REGISTRY` (any path; it is made absolute, as the CLI runs in a sandbox)
//! the cost comes from that LiteLLM-format file and must be positive; without it the run uses
//! `cost_tracking: ignore_errors`. The acceptance run of P8 uses a registry.

mod support;

use std::path::Path;

use support::cli::{BIN, Sandbox, stderr};

#[test]
#[ignore = "needs an API key and a real endpoint; run with --ignored"]
fn solves_a_toy_task() {
    let model = std::env::var("MSWEA_RS_REAL_MODEL").expect("set MSWEA_RS_REAL_MODEL");
    let mut sandbox = Sandbox::new();
    if let Ok(key) = std::env::var("OPENAI_API_KEY") {
        sandbox.env.insert("OPENAI_API_KEY".to_owned(), key);
    }
    let registry = std::env::var("MSWEA_RS_REAL_REGISTRY").ok().map(|path| {
        std::fs::canonicalize(&path)
            .unwrap_or_else(|e| panic!("MSWEA_RS_REAL_REGISTRY={path}: {e}"))
    });
    let mut args = vec![
        "-m".to_owned(),
        model,
        "-c".to_owned(),
        "mini_textbased".to_owned(),
        "-c".to_owned(),
        "agent.step_limit=10".to_owned(),
        "-o".to_owned(),
        sandbox.out("real.json"),
        "-t".to_owned(),
        "Create a file named hello.txt whose only content is the line: Hello from Rust".to_owned(),
    ];
    match &registry {
        Some(path) => args.extend([
            "-c".to_owned(),
            format!("model.litellm_model_registry={}", path.display()),
        ]),
        None => args.extend([
            "-c".to_owned(),
            "model.cost_tracking=ignore_errors".to_owned(),
        ]),
    }
    if let Ok(base) = std::env::var("MSWEA_RS_REAL_API_BASE") {
        args.extend([
            "-c".to_owned(),
            format!("model.model_kwargs.api_base={base}"),
        ]);
    }
    let output = sandbox.run(Path::new(BIN), &args, None);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let trajectory = sandbox.trajectory("real.json");
    assert_eq!(trajectory["info"]["exit_status"], "Submitted");
    if registry.is_some() {
        let cost = trajectory["info"]["model_stats"]["instance_cost"]
            .as_f64()
            .unwrap();
        assert!(cost > 0.0, "the registry prices the run: {cost}");
    }
    let hello = std::fs::read_to_string(sandbox.root.join("work/hello.txt")).unwrap();
    assert_eq!(hello.trim(), "Hello from Rust");
}
