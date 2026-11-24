//! One manual run against a real OpenAI-compatible endpoint (phase P8). Opt-in, because it needs
//! an API key and costs money:
//!
//! ```bash
//! OPENAI_API_KEY=... MSWEA_RS_REAL_MODEL=gpt-4o-mini \
//!   [MSWEA_RS_REAL_REGISTRY=registry.json] [MSWEA_RS_REAL_API_BASE=https://…/v1] \
//!   cargo test --test real_endpoint -- --ignored --nocapture
//! ```
//!
//! With `MSWEA_RS_REAL_REGISTRY` the cost comes from that LiteLLM-format file; without it the run
//! uses `cost_tracking: ignore_errors`.

mod support;

use std::path::Path;

use support::cli::{BIN, Sandbox, stderr};

#[test]
#[ignore = "needs an API key and a real endpoint; run with --ignored"]
fn solves_a_toy_task() {
    let model = std::env::var("MSWEA_RS_REAL_MODEL").expect("set MSWEA_RS_REAL_MODEL");
    let mut sandbox = Sandbox::new();
    for name in ["OPENAI_API_KEY", "LITELLM_MODEL_REGISTRY_PATH"] {
        if let Ok(value) = std::env::var(name) {
            sandbox.env.insert(name.to_owned(), value);
        }
    }
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
    match std::env::var("MSWEA_RS_REAL_REGISTRY") {
        Ok(path) => args.extend([
            "-c".to_owned(),
            format!("model.litellm_model_registry={path}"),
        ]),
        Err(_) => args.extend([
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
    assert_eq!(
        sandbox.trajectory("real.json")["info"]["exit_status"],
        "Submitted"
    );
    let hello = std::fs::read_to_string(sandbox.root.join("work/hello.txt")).unwrap();
    assert_eq!(hello.trim(), "Hello from Rust");
}
