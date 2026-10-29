# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A port of [mini-swe-agent](mini-swe-agent/README.md) (Python) to Rust. The task statement is `spec/prompt/migrate.md` (in Portuguese); read it before starting phase work.

- `mini-swe-agent/` is the **Python reference implementation and the living spec**. Treat it as read-only. It has its own `AGENTS.md` (style rules for the Python code) that does not apply to the Rust crate.
- `rust-mini-swe-agent/` is where the Rust crate goes (`cargo new --bin`). **It does not exist yet.** Check what exists before assuming a module, test, or command is there.
- `spec/PLAN.md` holds the detailed plan, and the crate's `README.md` records status and design decisions. Keep both current as work lands. (`spec/PLAN.md` is referenced by the task but not yet written; `spec/migrate.md` is currently an empty directory.)
- The git root is the parent directory `CognitionLab/`. Sibling crate `../embeddings-fundamentals/` shows the user's Rust conventions (Rust 2024, toolchain pinned in `rust-toolchain.toml`, `clippy.toml` allowing unwrap/expect/panic only in tests).

## Scope rules (from the task)

- Port order: core traits (`Model`, `Environment`, `Agent`) → `LocalEnvironment` (`std::process::Command`) → `DefaultAgent` (linear history, simple loop) → deterministic mock model for parity tests → trajectory serialization (`serde_json`) → minimal CLI.
- **Only bash.** No tools besides bash, no native tool-calling (port the text-based path: `config/default.yaml` / `mini_textbased.yaml`, not `mini.yaml`), no stateful shell sessions, no extra control flow without explicit need.
- Use the `/rust-skills` skill for Rust guidance. It is installed only in `../embeddings-fundamentals/.agents/skills/rust-skills`, so it may not be invocable from here. If it is unavailable, say so rather than claiming it was used.
- Success criteria: deterministic (mocked) runs produce trajectories **structurally equivalent** to Python's; code stays readable to someone who knows the original; zero panics on normal paths (`Result`, no `unwrap`/`expect` outside tests, no unnecessary `unsafe`).

## Commands

Rust (run inside `rust-mini-swe-agent/` once it exists; toolchain is cargo 1.99.1):

```bash
cargo build
cargo test
cargo test <name_filter>                 # single test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

Python reference (run inside `mini-swe-agent/`), useful to confirm original behavior or regenerate expected trajectories:

```bash
uv run pytest tests/agents/test_default.py          # agent loop semantics
uv run pytest tests/environments/test_local.py      # LocalEnvironment semantics
uv run pytest tests/agents/test_default.py -k test_successful_completion
```

`tests/test_data/local.traj.json` (a list of messages) and `update_outputs_local.py` are existing Python fixtures for a deterministic local run, and a natural source of parity fixtures.

## Reference semantics to preserve

These are spread across `src/minisweagent/{__init__,exceptions}.py`, `agents/default.py`, `environments/local.py`, `models/test_models.py`, `models/utils/actions_text.py`, and `utils/serialize.py`.

- **Messages** are JSON objects with `role`, `content`, and optional `extra`. Roles: `system`, `user`, `assistant`, `exit`. The history is append-only, and the trajectory *is* the message list. The loop ends when the last message has role `exit`, and `run()` returns that message's `extra` (`exit_status`, `submission`).
- **Control flow uses exceptions that carry messages** (`InterruptAgentFlow` → `Submitted`, `LimitsExceeded` → `TimeExceeded`, `UserInterruption`, `FormatError`). Each one appends its messages to the history. In Rust this maps naturally to a step result or error enum that carries messages, not to panics.
  - `FormatError`: add the failed call's `extra.cost` to the agent's cost, increment the consecutive-error counter (reset after any clean step), and when it reaches `max_consecutive_format_errors` (0 = unlimited), append an `exit` message with status `RepeatedFormatError`.
  - Unexpected error: append an `exit` message (`exit_status` = error type name, plus `exception_str` and `traceback`), then propagate.
  - The trajectory is saved after **every** step (`finally`), including the failing one.
- **Limits are checked before each model query**: `step_limit` against `n_calls`, `cost_limit` against cumulative cost (`0 < limit <= value`), and `wall_time_limit_seconds`. 0 disables each one. They produce `exit` messages `LimitsExceeded` / `TimeExceeded`.
- **Action parsing (text-based)**: regex ```` ```mswea_bash_command\s*\n(.*?)\n``` ```` with DOTALL. Exactly one match is required, otherwise `FormatError` with a `user` message rendered from `format_error_template` (`extra.interrupt_type`, `n_actions`, `model_response`). An action is `{"command": ...}`.
- **LocalEnvironment**: each command runs independently through the shell (`shell=True`, i.e. `/bin/sh -c`), stderr merged into stdout, UTF-8 decoding with replacement, env = process env overlaid with `config.env`, cwd = arg or `config.cwd` or the current dir, default timeout 30s. The child runs in a **new process group, and the whole group is killed on timeout**, keeping the partial output. On any error the result is `returncode: -1` with `exception_info` and `extra.{exception_type, exception}`.
- **Submission**: if the output, after a leading `lstrip`, has `COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT` as its first line and `returncode == 0`, raise `Submitted`. The submission is the remaining lines.
- **Templates** are Jinja2 with `StrictUndefined` (an undefined variable is an error). The configs rely on filters and slicing (`length`, `[:5000]`, whitespace control). Template vars = `recursive_merge(agent config, env vars (platform.uname fields + os.environ), model config, {n_model_calls, model_cost, elapsed_seconds}, extra vars incl. task, kwargs)`. Later entries win, and nested dicts merge.
- **Observation messages** are `user` messages rendered from `observation_template` with `output` in scope. Their `extra` holds `raw_output`, `returncode`, `timestamp`, `exception_info`, plus the output's own `extra`.
- **Trajectory format** (`trajectory_format: "mini-swe-agent-1.1"`) = `recursive_merge(agent_data, model.serialize(), env.serialize())`, with `info.model_stats.{instance_cost, api_calls}`, `info.config.{agent, agent_type, model, model_type, environment, environment_type}`, `info.{mini_version, exit_status, submission}`, and `messages`. Saved as pretty JSON (indent 2), creating parent dirs. Parity checks must ignore `timestamp` values and the Python-specific `*_type` class paths.
- **DeterministicModel** returns `config.outputs` in order (`cost_per_call` default 1.0). Special actions in a canned output: `raise` (raise the given exception), `/sleep N` and `/warning msg` (side effect, then advance to the next output).
