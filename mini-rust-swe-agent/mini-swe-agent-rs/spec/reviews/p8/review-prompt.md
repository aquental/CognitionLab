# Adversarial code review: phase P8 of mini-swe-agent-rs

You are an adversarial reviewer. Review the **code** delivered in phase P8 of the Rust port of mini-swe-agent. The current directory is `mini-rust-swe-agent/`. **Read only files under this directory.**

Context:
- The plan is `mini-swe-agent-rs/spec/migration-plan.md`. P8 is the "P8 Real model" row of its §4 phase table. Designs: §3.6 (sync HTTP with `ureq`), §3.8 (`TextbasedModel`: config fields, settings split, request, retry, cost, global stats, parsing, format-error enrichment, success message), §3.3 (`ChatBackend`), §3.13 (no secrets in logs), §3.15 items 2 (fake HTTP server) and 4 (raw-response differential), §2.4 divergence rows, D4.
- The Python reference is `mini-swe-agent/` (read-only): `src/minisweagent/models/litellm_model.py`, `models/litellm_textbased_model.py`, `models/utils/retry.py`, `models/__init__.py` (`get_model`, `GlobalModelStats`), `models/utils/actions_text.py`, `run/hello_world.py`.
- Owner decisions not to re-argue: D1, D2, D4, D5, D9, D11.
- Earlier phases were reviewed already; their findings and dispositions are in `mini-swe-agent-rs/spec/reviews/p*/FINDINGS.md`. Do not repeat a finding that is fixed there or refuted with a reason, unless the fix or the reason is wrong.
- No real endpoint or API key is available to you; do not make network calls.

Files delivered in P8, all under `mini-swe-agent-rs/`:
- `src/model/textbased.rs`, `src/model/openai_compat.rs`, `src/model/retry.rs`, `src/model/cost.rs`, `src/model.rs`, `src/error.rs` (new `ModelError` variants), `src/config.rs` (`ModelChoice::Textbased`, Anthropic cache-control default), `src/main.rs` (generic `run_agent`), `examples/hello_world.rs`, `Cargo.toml` (`ureq`)
- `tests/textbased.rs`, `tests/fake_http.rs`, `tests/text_parity.rs`, `tests/real_endpoint.rs`, `parity/text_model_python.py`, `parity/text_model_cases.json`, README changes

What to do:
1. Check the P8 "done" criteria. Verify the model semantics against the Python code (cite file:line): retry and abort set, cost and its error path, global stats order, format-error enrichment, the success message and its `extra`, what is sent to the provider, model-name and class resolution.
2. Look for: secrets leaking (logs, trajectories, errors, `Debug`), wrong HTTP error mapping or retries that can loop or block, wire-format mistakes for real OpenAI-compatible servers (headers, URL joining, timeouts, non-UTF-8 or huge bodies, TLS setup), tests that can pass vacuously (fake server, differential projection), cost arithmetic or registry lookup errors, and anything that can panic in non-test code.
3. Judge by Rust best practices: error types, ownership and cloning, trait design, dependency choices and features.

Do not edit any file. Do not run network installs or network calls.

Output format (Markdown):
- A short verdict (1-3 sentences).
- A numbered list of findings. For each: severity (critical / high / medium / low), the file and line, the problem, evidence, and a concrete recommended fix.
- If you find nothing in a category, do not invent findings. Precision matters more than volume.
