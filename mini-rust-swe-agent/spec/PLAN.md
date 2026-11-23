# Migration plan: mini-swe-agent Python → Rust

Analysis date: 2026-10-28. State: planning; no Rust implementation and no proven parity in this delivery.

## 1. Goal and reference

Reimplement the harness in Rust, keeping the Python code executable as the behavior oracle. The result must preserve prompts, actions, observations, limits, costs, termination, and trajectory in the cases the contract defines. The goal is not to translate classes line by line: it is to preserve behavior with clear ownership, explicit types, and a simple flow.

The analyzed code is in `../mini-swe-agent/src/minisweagent/` and declares version **2.4.6**. The local requirement in [prompt/migrate.md](prompt/migrate.md) asks only for textual shell actions, with no native tool-calling and no persistent sessions, and includes BDD/Cucumber. The current reference also contains tool-calling and the Responses API; so this requirement describes a **subset**, not the whole upstream.

**Provenance warning:** `git -C mini-swe-agent rev-parse --show-toplevel` resolves to `CognitionLab`. The observed SHA `b3cb1431177eba1289ea00ffe7c983a25db8fdad` belongs to the parent repository and **does not identify the upstream commit**. Do not declare the reference frozen by that SHA until its origin is verified.

As a provisional content identifier, reading the 205 files of the copy, excluding `.git`, `.venv`, `__pycache__`, and `.pytest_cache` components, produced the aggregate SHA-256 `1c9632a76ba2171f2f5421c51d9b64391901997e1c599963bdb937b7913f294e`. Algorithm: sort the relative paths; for each file, write `sha256(bytes)`, two spaces, the POSIX path, and a newline; concatenate these lines in UTF-8 and compute the SHA-256. This identifies the local snapshot; it does not prove upstream origin. The value was recomputed on 2026-10-28 with the same algorithm and matched (205 files). The detailed code analysis behind this plan is in [STUDY.md](STUDY.md); when either document disagrees with the code, the Python code wins and both PLAN.md and STUDY.md must be corrected.

## 2. Proposed layout

All paths below are relative to `mini-rust-swe-agent/`. The tree is planned; it does not mean the files already exist.

```text
mini-rust-swe-agent/
├── README.md                    # study, architecture, and verifiable status
├── mini-swe-agent/              # preserved Python reference
├── spec/
│   ├── PLAN.md
│   ├── STUDY.md                 # analysis of the Python code and gates P01–P18
│   ├── PRD.md                   # empty placeholder; fill in or remove
│   ├── prompt/                  # task statements (migrate.md, study.md)
│   ├── reference.json           # origin, SHA or snapshot, versions, and hashes
│   ├── parity.md               # contracts and allowed divergences
│   ├── scope.md                # coverage matrix
│   └── adr/                    # decisions with reasons and consequences
├── rust-mini-swe-agent/
│   ├── Cargo.toml
│   ├── Cargo.lock
│   ├── rust-toolchain.toml
│   ├── src/
│   │   ├── main.rs              # arguments, composition, and exit code
│   │   ├── lib.rs
│   │   ├── agent.rs
│   │   ├── model.rs
│   │   ├── environment.rs
│   │   ├── config.rs
│   │   ├── templates.rs
│   │   ├── trajectory.rs
│   │   └── error.rs
│   └── tests/
│       ├── parity.rs
│       └── bdd.rs
├── parity/
│   ├── fixtures/               # shared inputs and expected results
│   ├── python_runner.py        # imports the reference without changing it
│   ├── compare.py              # structural diff by JSON path
│   └── features/               # shared Gherkin scenarios
└── reports/                    # run evidence, no secrets
```

Start with **one Cargo package, a library and a binary**. Extract submodules when alternative implementations appear; do not start with a multi-crate workspace. The Git history stays in the parent. The Python reference must not be adjusted to make Rust pass.

## 3. Scope and milestones

| Capability | M1: offline parity | M2: usable agent | Later evolution |
|---|---|---|---|
| Traits, loop, history, errors, and limits | Required | Kept | Kept |
| Deterministic model and scripted environment | Required | Regression | Regression |
| Textual parsing, templates, configuration | Required for the documented subset | Selected textual configurations complete | Extend with fixtures |
| POSIX LocalEnvironment and trajectory 1.1 | Required | Kept | Other OSes need their own contract |
| Non-interactive CLI | Required | Documented interface | Alias compatibility when needed |
| One real textual provider | Outside the offline gate | Required, provider/model identified | Each other provider has its own gate |
| Docker | Contract and cases defined | Separate milestone M3 | Do not assume isolation from the backend's name |
| OpenRouter/Requesty/Portkey/LiteLLM | Inventoried | Only the chosen integration | Explicit adapters; no promise to replace all of LiteLLM |
| InteractiveAgent and inspector | Deferred | Deferred | M4, if included in the product |
| SWE-bench, ProgramBench, and batch runs | Deferred | Deferred | M4, evaluation and controlled concurrency |
| Singularity, Bubblewrap, Contree, SWE-ReX/Modal | Deferred | Deferred | Each backend needs its own environment and tests |
| Tool-calling, Responses API, multimodal | Excluded from the initial scope | Excluded | Extend only through an explicit scope change |
| Dynamic import of Python classes | Excluded | Registry of Rust implementations | Do not promise Python plugins in the binary |
| Tools beyond the shell and persistent sessions | Excluded | Excluded | Need a new design and a new contract |

"Migration of the textual subset is complete" and "parity with the whole upstream" are different claims. Record each exclusion with its justification. M1 does not complete the full migration.

## 4. Step by step

### Step 0 — Pin the reference and make the Python side reproducible

1. Keep the current copy; record the inventory and hashes in `spec/reference.json`/a separate manifest.
2. If the origin is reachable, get a separate checkout, identify a verifiable upstream SHA, and compare its content with the copy. Record local changes. Do not silently replace the reference with the main branch.
3. Keep `LICENSE.md` and the attribution to the authors; document the origin of the prompts, fixtures, and adapted code.
4. Pin the Python version and resolve dependencies in a dedicated environment. `pyproject.toml` has open ranges; `pip install` alone does not give reproducibility. Export a lock/constraints file with versions and, when feasible, hashes.
5. Use a temporary directory for `MSWEA_GLOBAL_CONFIG_DIR`, set `MSWEA_SILENT_STARTUP=1`, control the environment, and use no real credentials. Importing the package creates a directory and loads the global `.env`.
6. First run selected tests for configuration, serialization, agent, and local environment. Record dependencies, OS, shell, exit code, failures, and skips. Keep apart the network, container, and `--run-fire` tests, which can cost money.

Initial commands, to run from this project's folder; this document has not run them:

```bash
python3 -m venv .venv-reference
.venv-reference/bin/python -m pip install -e './mini-swe-agent[dev]'
.venv-reference/bin/python -m pip freeze > spec/python-reference.freeze.txt
mkdir -p reports/python-config
MSWEA_GLOBAL_CONFIG_DIR="$PWD/reports/python-config" MSWEA_SILENT_STARTUP=1 \
  .venv-reference/bin/python -m pytest \
  mini-swe-agent/tests/agents/test_default.py \
  mini-swe-agent/tests/environments/test_local.py \
  mini-swe-agent/tests/config/test_init.py \
  mini-swe-agent/tests/utils/test_serialize.py
```

The freeze records the initial environment, but the editable install points at the local code: reproducibility also needs the snapshot and a lock validated in a clean install. Equivalent alternative: `uv run pytest ...` run inside `mini-swe-agent/`, which resolves `pyproject.toml`; the copy has no `uv.lock`, so exporting `uv lock`/`uv export` to `spec/` plays the same role as the freeze.

**Acceptance:** the manifest identifies the reference without confusing it with the parent's SHA; an offline set runs and its result is recorded; no change to the Python source.

### Step 1 — Write the contracts before the implementation

Create `spec/parity.md` with the inputs, outputs, state, side effects, and examples of each boundary:

| Boundary | Minimum contract |
|---|---|
| Model | A query receives the ordered history; the response includes content, actions, and cost; formats messages and observations; exports variables and configuration |
| Environment | `execute(action, cwd="")` in the Protocol; `LocalEnvironment` also accepts `timeout` as a keyword, but `DefaultAgent.execute_actions` calls `env.execute(action)` with neither cwd nor timeout, so the effective cwd is `config.cwd` or the process cwd and the timeout is `config.timeout` (30 s). A `cwd` field inside the action is not read. Returns output, returncode, exception_info, and extras, or signals a submission |
| Agent | Coordinates calls, costs, limits, messages, and termination; returns the extras of the final message |
| Configuration | YAML and `a.b=value`; parses the value as JSON or falls back to a string; recursive merge; absent is distinct from null/false/zero |
| Templates | Jinja with StrictUndefined; variable order, filters, slicing, Unicode, and whitespace preserved |
| Trajectory | Full object with `info`, `messages`, and `trajectory_format`; preserve extensible fields |
| CLI | Arguments, precedence, stdout/stderr, output path, exit code, and interruption defined |

Record the subtleties of the real code:

- The textual parser uses the regex ```` ```mswea_bash_command\s*\n(.*?)\n``` ```` with DOTALL (a newline is required after the name and before the closing fence), applies `strip()` to each capture, and requires **exactly one** capture. Null `content` becomes `""`; an empty block is still a valid capture (empty command); several lines in a block are a single command; a plain `bash` fence does not count; zero or several blocks produce `FormatError`. The Rust `regex` engine does not support lookaround or every `re` flag: a configurable `action_regex` is supported only in the common subset, and the rest is rejected explicitly.
- Limits are checked **before** the query: `0 < limit <= accumulated`. Zero disables. `n_calls` is incremented before `model.query`, and that increment survives `FormatError` and model failures: `api_calls` already counts the attempt while `instance_cost` may not yet include that response's cost. The cost is added after the response or by the format-error handler.
- **Global statistics** (`GlobalModelStats`, STUDY §8) are part of the Model contract: every successful `query()` calls `add(cost)` after computing the cost and **before** the parser; the thresholds come from `MSWEA_GLOBAL_COST_LIMIT`/`MSWEA_GLOBAL_CALL_LIMIT`, use `<` (`0 < cost_limit < total_cost`, `0 < call_limit < n_calls + 1`, so `call_limit == 1` already fails on the first response), and raise `RuntimeError`, which the agent handles as an unexpected error (diagnostic `exit` message, save, propagation). Rust receives the statistics instance by composition, never through a hidden singleton; each test scenario creates its own.
- `FormatError` preserves messages, charges the cost available in the first message it carries, increments the consecutive-error counter, and can end with `exit_status: RepeatedFormatError`, which is a **status value** in the `exit` message, not an exception class. On this path no `assistant` message is created: Rust must not synthesize one. A clean step resets this counter.
- A missing or `[]` `extra.actions` is not an error in the agent: `execute_actions` produces an empty list of outputs and calls `format_observation_messages` once; the step counts as clean and the loop continues until a limit.
- `run()` clears the messages but does not reset every counter, nor the start time set in the constructor. Test reuse of the same agent, or document a one-run-per-instance restriction; do not silently "fix" it.
- `execute_actions()` runs sequentially. A submission stops the sequence and produces no normal observation. Test this even though the textual parser normally produces a single action.
- The `finally` saves after each step attempt; unexpected errors append an `exit` message and are propagated. Failures before entering the loop (for example, the initial rendering) behave differently.
- `get_template_vars()` merges agent configuration → environment → model → counters/time → extra variables → arguments. In the local environment, the platform and the environment variables are included.
- The local shell uses `shell=True`: on POSIX the default is `/bin/sh -c`, even though the text mentions bash. Record an ADR: compatible mode reproduces `/bin/sh`; explicit bash is a separate, tested option with a declared divergence.
- `cd` and `export` do not persist between actions; file changes do persist. `stderr` is merged into stdout, invalid UTF-8 is replaced, and a timeout kills the process group on POSIX.
- Submission: `lines = output.lstrip().splitlines(keepends=True)`; it ends if `lines[0].strip() == "COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT"` and `returncode == 0`; submission = `"".join(lines[1:])`. Spaces around the marker on the first line are accepted; a different case, extra text on the line, a log line before the marker, a non-zero return code, or the marker only in the LLM's text do not submit. CRLF is preserved in the submission; a marker without a trailing newline produces an empty submission. The full matrix (8 + 2 cases) is in STUDY §5.
- **Class selection does not come from the textual YAML:** `default.yaml` and `mini_textbased.yaml` do not define `model.model_class`, and `get_model` falls back to `LitellmModel` (tool-calling). The Python oracle must instantiate `DefaultAgent`, `LitellmTextbasedModel`/`DeterministicModel`, and `LocalEnvironment` directly, or pass an explicit `model_class`. In Rust, the minimal CLI offers only the textual path, and this is a recorded decision, not parity with `mini` (STUDY §3.3).
- The configs are Pydantic with `extra='ignore'`: the YAML's `mode: confirm` is dropped by `DefaultAgent`, unknown keys appear neither in `info.config` nor in the template context; `output_path` serializes as a string or `null`. Serde must ignore unknown fields in these structs and serialize `Option<PathBuf>` as `null`.
- A background child that inherits stdout keeps the pipe open: the command only ends at the timeout, with partial output and the group killed (STUDY §4). Rust must reproduce this by reading until EOF, not until the parent shell exits.
- `DeterministicModel` has special actions `raise`, `/sleep N`, and `/warning msg` that advance to the next output without counting a call; the global cost uses `cost_per_call` and the local cost uses the message's `extra.cost` (STUDY §3.4). In JSON, `raise` becomes a tag with the exception name.
- `run()` accumulates `extra_template_vars` with `|=` and does not reset `n_consecutive_format_errors`; `save()` failures inside the `finally` replace the in-flight exception.

**Acceptance:** each rule points to a Python source/test and has an observable scenario; deliberate divergences have an ADR.

### Step 2 — Create the Rust package

Run from `mini-rust-swe-agent/` when the implementation starts:

```bash
cargo new --bin --edition 2024 --vcs none --name mini-swe rust-mini-swe-agent
cd rust-mini-swe-agent
cargo build
cargo test
```

`--vcs none` avoids a nested repository. The `mini-swe` package will have the Rust import name `mini_swe`; the directory stays `rust-mini-swe-agent`. If the folder already exists, inspect its contents before using `cargo init`; do not overwrite another package. See [Cargo: creating a package](https://doc.rust-lang.org/cargo/guide/creating-a-new-project.html) and [cargo init](https://doc.rust-lang.org/cargo/commands/cargo-init.html).

1. Add `src/lib.rs`; keep `main.rs` to composition and final error handling only.
2. Pin the toolchain in `rust-toolchain.toml` with `channel = "1.99.1"` and the `rustfmt` and `clippy` components, as in `../embeddings-fundamentals/`; `cargo 1.99.1` and `rustc 1.99.1` were verified as installed on 2026-10-28. Declare `edition = "2024"`, `rust-version = "1.99"`, and `publish = false`. Copy `clippy.toml` from the sibling crate (`allow-unwrap-in-tests`, `allow-expect-in-tests`, `allow-panic-in-tests = true`) so that the "no unwrap outside tests" rule is checked by a lint, not by review.
3. Commit the executable's `Cargo.lock`. Ignore `target`, Python environments, and temporary reports without hiding fixtures.
4. Add dependencies as each step needs them: `serde`/`serde_json` (feature `preserve_order`, so key order follows insertion as in Python), `thiserror`, `clap`, `regex`; `minijinja` with `UndefinedBehavior::Strict` as the candidate template engine, validated in step 5; `serde_yaml` or a maintained successor for YAML; `nix` (feature `signal`/`process`) for `killpg` without our own `unsafe`; an HTTP library only in M2. Confirm maintenance, licenses, MSRV, and features before pinning versions. State on 2026-10-28: toolchain 1.99.1, `std::io::pipe` with `PipeWriter::try_clone`, `process_group`, and `ExitStatusExt::signal` confirmed by compiling; `minijinja` (negative slices, `length` in code points, `is defined` under `Strict`), `nix::sys::signal::killpg`, `serde_yaml`, and `cucumber` **not yet confirmed**: check them before pinning and record the versions in `spec/reference.json`.
5. Prefer synchronous execution in the first milestone, consistent with `std::process::Command` and the original loop. Use Tokio only when there is a proven need for concurrent I/O. Do not make the whole API async in advance.
6. Enable warnings and Clippy selectively; forbid our own unsafe in the core where possible. Do not copy `panic=abort` or aggressive release settings automatically: shutdown and cleanup must be verified.

**Acceptance:** build, fmt, Clippy, and basic tests pass on the pinned toolchain; the library is importable; the binary has no domain logic.

### Step 3 — Define the core types and traits

1. Create `Model`, `Environment`, and `Agent` with operations equivalent to the Python protocols. Keep the internal representation separate from the compatibility JSON.
2. Define types for message, action, execution result, final state, and statistics. Use enums for known results and `serde_json::Value`/maps only for the extensions that need them.
3. Use `&str` and slices for inputs, `&mut self` for mutable state, and single ownership of the history. Generics are enough at first; enums or trait objects can compose backends when the CLI needs dynamic selection.
4. Model submission/limits as a control result that carries messages; recoverable failures with `Result`, typed errors with `thiserror`, context at the application boundary. Do not turn every normal interruption into a fatal failure.
5. Define an injectable clock for tests that returns seconds as `f64`, with wall-clock semantics equal to `time.time()` in parity mode: `elapsed_seconds = int(now - start)` truncates toward zero, so `wall_time_limit_seconds: 1` still allows a query at 1.999 s. A monotonic clock changes terminations and prompts and stays as a product ADR outside the M1 gate. The comparator exercises the limits by advancing the injected clock, without sleeps.
6. Start a minimal scripted environment implementation to test the contract before wiring up real processes. This is test infrastructure; the full deterministic model is still in step 6.

**Acceptance:** serialization rules for the types and control results are verified; the APIs do not require cloning the history at each step; no panic on expected errors.

### Step 4 — Implement LocalEnvironment

1. Create independent processes with `std::process::Command`, with cwd and environment following the Python precedence.
2. Merge stdout/stderr at the capture point with **a single pipe**: create it with `std::io::pipe()` (stable since Rust 1.87), pass the writer as the stdout `Stdio` and a `try_clone()` of it as stderr, close the parent's copies, and read the reader until EOF in a thread. Do not concatenate two buffers at the end, since that changes the order. Read until EOF, not until `wait()` returns, to reproduce the case of a background child that holds the pipe. A minimal program with `std::io::pipe`, `try_clone`, `process_group(0)`, and `ExitStatusExt::signal` compiled and ran with `rustc 1.99.1` on 2026-10-28 (merged output in order, `code() == None`, `signal() == Some(15)` for a shell killed by SIGTERM).
3. Launch the shell with `std::os::unix::process::CommandExt::process_group(0)` and, on timeout, call `nix::sys::signal::killpg(pid, SIGKILL)` on the group; killing only the parent shell is not enough. `process_group(0)` does `setpgid` (new group, same session), while `start_new_session=True` does `setsid` (new session and group); for `killpg` on the leader the effect is the same, and the session difference (HUP, controlling terminal) is recorded as an ADR because `setsid` through `pre_exec` would require `unsafe`. The timeout is an **absolute deadline from spawn**, as in `communicate(timeout=...)`: the reader thread reads until EOF and the parent waits for the result until the deadline; never measure idle time between bytes, because a command that writes nonstop for 60 s with a 30 s timeout must expire at 30 s. After the deadline: `killpg`, `wait()`, return the partial output already read. The shell is `/bin/sh -c <command>`, as `shell=True` on POSIX.
4. Define the return code for a signal (Python gives `-N` for signal N; `ExitStatus::code()` is `None` and needs `ExitStatusExt::signal()`), process creation failure, a missing directory, timeout, and invalid UTF-8 (`String::from_utf8_lossy`, which replaces like `errors="replace"`). The error result is `returncode: -1`, `exception_info: "An error occurred while executing the command: ..."`, and `extra.{exception_type, exception}`; `exception_type` uses the Python name (`TimeoutExpired`, `FileNotFoundError`, `NotADirectoryError`) as compatibility metadata, without faking a traceback.
5. Implement the submission marker with tests for leading spaces, CRLF, a line without a newline, the marker in the middle of the output, and a non-zero return code.
6. Run test commands only in temporary directories. Define the supported OSes: Linux/macOS POSIX at first; Windows is explicitly outside this gate.

**Acceptance:** isolated `LocalEnvironment` tests (cwd/env/precedence, return code, signal, merged stream, UTF-8, marker, timeout) pass, without depending on the comparator from step 7; two background-child cases are required: (a) a child that inherits stdout → `TimeoutExpired`, partial output preserved, whole group killed; (b) a child with redirected stdio → the command returns 0 immediately and the descendant **survives**, as in the reference; no unintended persistence of cwd/env between actions; files persist as in Python.

### Step 5 — Implement DefaultAgent, configuration, and templates

1. Reproduce `run()`/`step()` as in STUDY §2: render system and instance **once, before the loop** (a failure here does not save a trajectory); then, on each iteration, `step()` = check limits → `n_calls += 1` → query → add cost and append `assistant` → execute actions in sequence → format observations; the step has these outcomes: clean (resets consecutive errors), `FormatError` (charges, counts, appends messages, and maybe `exit RepeatedFormatError`), interruption with messages (`Submitted`, `LimitsExceeded`, `TimeExceeded`: only appends), or unexpected error (appends a diagnostic `exit` and propagates); `save` always runs after the attempt, and only then is `messages[-1].role == "exit"` tested. A submission stops `execute_actions` without the normal observation.
2. Preserve the exceptional path for format errors, costs, counters, and messages; keep unexpected errors separate from normal termination.
3. Reproduce the YAML lookup and the spec merge; test false, zero, null, replaced lists, merged objects, an empty key, and unknown fields. The Rust struct defaults are Pydantic's, not the YAMLs'; no `deny_unknown_fields`:

| Field | Pydantic default | `default.yaml` | `mini_textbased.yaml` |
|---|---|---|---|
| `agent.step_limit` | 0 | 0 | 0 |
| `agent.cost_limit` | 3.0 | 0.0 (off) | 3.0 |
| `agent.wall_time_limit_seconds` | 0 | absent | absent |
| `agent.max_consecutive_format_errors` | 3 | absent | absent |
| `agent.output_path` | None | absent | absent |
| `agent.mode` | does not exist in `AgentConfig` | absent | `confirm` (ignored) |
| `environment.{cwd, env, timeout}` | `""`, `{}`, 30 | only `env` | only `env` |
| `model.action_regex` / `format_error_template` | default regex / short template | — / long template | — / long template |
| `model.cost_per_call` (deterministic) | 1.0 | — | — |

4. Validate the template engine with the real `default.yaml` and `mini_textbased.yaml` files. Features actually used: `{{ var }}`, `{% if %}`/`{% else %}`, `{% set %}`, the `is defined` test, the `in [..]` operator, the `length` filter, string slices `[:5000]`/`[-5000:]`, whitespace control `{%- -%}`, and attribute access (`output.output`). Check negative slices and code-point counting in MiniJinja; if a feature is missing, record an ADR instead of rewriting the templates. Compare prompts byte for byte, in the three contexts described in STUDY §7, each with its own golden: system/instance (the agent's full context, whose internal environment merge is config → `platform.uname()` → `os.environ` → kwargs), observation (the agent's context plus `output=`), format error (only `actions`, `error`, and `finish_reason`). Fixtures freeze `uname` and `environ` by injection; otherwise the prompt depends on the machine. Test collisions: an env var `system` overriding the platform, and an `output` key in the context, which in Python is a duplicate-argument error. `get_template_vars` uses `model_dump()` without `mode="json"`, so `output_path` reaches the template as a `Path` object, not a string.
5. Test observation truncation around 10,000 characters, including multibyte Unicode; count characters as the reference does, not bytes.
6. Cover a save failure and the precedence between the original error and the persistence error. Improvements such as atomic writes need an ADR and must preserve the defined external contract.

**Acceptance:** the loop is tested with the minimal scripted model and environment from step 3 (message sequence, costs, counters, and exit status checked against values derived by hand from the Python code); the full templates of the subset pass, with no whitespace normalization. Automatic comparison with Python is left for step 7.

### Step 6 — Deterministic model and differential oracle

1. Reuse `models/test_models.py` conceptually, keeping the runner outside the reference. `DeterministicModel` returns messages that are already structured: **on its own it does not test parsing of the textual response**. The Rust double reproduces `cost_per_call` (shared statistics) separately from `extra.cost` (agent cost), the special actions `/sleep N` and `/warning msg` (advance without counting a call), and `raise` as a JSON tag with the exception name; running out of `outputs` is an unexpected error, like the Python `IndexError`.
2. Create two sets: structured responses for the loop; raw content for the parser and the textual adapter. Also simulate an invalid billed response to check charging and persistence.
3. Define JSON fixtures with the task, config, response sequence, cost, environment outputs, clock, and expected result. Error events must be JSON tags, not serialized Python exception objects. `tests/test_data/local.traj.json` serves only as a script of responses and expected observations: it is a list of messages with prompts from an earlier version of the templates, and `update_outputs_local.py` no longer runs with the current `DeterministicModelConfig` (STUDY §10).
4. The Python runner adapts the fixture to the original components; Rust consumes the same fixture. Capture the requests sent to the model and the actions passed to the environment, in addition to the final trajectory.
5. Real-process cases use independent copies of the same initial directory. Never run Python and Rust one after the other on the same mutable directory.
6. Fail if scripted responses or outputs are left over or missing. This prevents tests that pass without running the whole scenario.
7. Shared statistics are injected per scenario; `cargo test` runs in parallel, and a process singleton would make scenarios share a budget, which Python's `reset_global_stats` has to work around. If any global fallback exists, its tests use `--test-threads=1` or `serial_test`.

**Acceptance:** a repeatable offline runner, with no shell in the purely scripted set and no network; differences reported by JSON path, with inputs and versions preserved.

### Step 7 — Serialization and output comparison

The envelope is `recursive_merge(agent_data, model.serialize(), env.serialize(), *extra_dicts)`, where `agent_data` has `info.model_stats.{instance_cost, api_calls}`, `info.config.{agent, agent_type}`, `info.mini_version`, `info.exit_status`, `info.submission` (the last two read from the last message's `extra`, `""` if absent), `messages`, and `trajectory_format: "mini-swe-agent-1.1"`; the model contributes `info.config.{model, model_type}` and the environment `info.config.{environment, environment_type}`. The configs go in through `model_dump(mode="json")` (Path → string, None → null). `save` creates the parent directories and writes `json.dumps(data, indent=2)`. Golden files come from `DefaultAgent.save()` on the identified oracle; `local.traj.json` is a list of six messages with old templates and is not usable as a golden.

The comparator will have three separate results:

1. **Protocol:** prompts, responses, actions, observations, and order match.
2. **State:** costs, number of calls, termination, submission, and intermediate snapshots match.
3. **Process/artifacts:** stdout, stderr, exit code, and file diff match according to the CLI/environment contract.

Normalization policy:

- Compare the **parsed** JSON, never the bytes: `json.dumps(indent=2)` escapes non-ASCII (`ensure_ascii=True`) and `serde_json` does not; separators and indentation can also differ without breaking the contract. Sort keys only to display the diff; keep array order.
- Freeze timestamps when possible; otherwise normalize only approved paths such as `messages[*].extra.timestamp`, preserving presence and type (float seconds, like `time.time()`; `elapsed_seconds` is an integer).
- Map temporary paths and class names in `info.config.*_type` explicitly. Compare the original values against the expected ones before mapping.
- The Rust version and the Python origin must have their own metadata; do not claim that Rust is the Python version. Record the `mini_version` mapping used for compatibility.
- Native tracebacks differ: validate presence/category and normalize only the diagnostically non-portable field. Do not drop the whole `extra`.
- Prompt, command, output, and submission strings are exact. Do not strip or remove lines to "fix" divergences.
- Cost needs an explicit policy: exact comparison in controlled fixtures and a small, justified tolerance for computed floating point; test thresholds without rounding before deciding termination.
- Absent field, null, empty string, and zero stay distinct. Unexpected JSON must fail, not be ignored.

Test the comparator itself with mutations: changing a command, a message, a cost, a missing field, or the order must fail; changing only an authorized timestamp must pass. Golden files may only be updated deliberately from the identified Python, with a review of the diff.

**Planned** interface, to implement before use:

```bash
.venv-reference/bin/python parity/python_runner.py --fixture parity/fixtures/success.json --output reports/python.json
cargo run --manifest-path rust-mini-swe-agent/Cargo.toml --bin parity-runner -- --fixture parity/fixtures/success.json --output reports/rust.json
.venv-reference/bin/python parity/compare.py reports/python.json reports/rust.json
```

`parity-runner` will be an auxiliary target, not the production interface. The comparator should return zero for equivalence, one for a difference, and another documented code for an execution error.

**Acceptance:** the comparator rejects each listed mutation (command, message, cost, missing field, order) and accepts only the authorized timestamp; at least one scripted fixture produces equivalent Python and Rust envelopes under the policy above; Rust's `*_type` and `mini_version` fields are documented with the mapping used.

### Step 8 — BDD/Cucumber and minimal CLI

Create Gherkin scenarios in `parity/features/`, run by a Rust test target with Cucumber. Use adapters for the same Python fixtures; do not duplicate expectations by hand in two suites. Confirm the Cucumber crate's API and version during implementation. BDD documents public behavior; unit tests still cover parser and merge details.

```gherkin
Feature: Completion and limits of the textual agent
  Scenario: Submission ends the run
    Given a task with deterministic responses
    And the command returns the submission marker and "resolved" with exit code zero
    When I run the scenario in the Python and Rust harnesses
    Then both end with status "Submitted"
    And the submission and the history are equivalent
    And no additional query is made

  Scenario: The limit is checked before the next query
    Given a limit of 1 call and a response that does not submit
    When I run the scenario in the Python and Rust harnesses
    Then exactly 1 query is made in each harness
    And both end with status "LimitsExceeded"
```

Add scenarios for timeout with a child, shell with no persistent state, consecutive format errors, cost at the threshold, invalid template, Unicode output, and saving on failure.

The minimal CLI accepts the task, a repeatable config, model, environment, limits, and output file. Defaults must select the non-interactive agent and the textual path. Python's `mini` uses the interactive agent by default and, without `model_class`, the tool-calling model: the comparison runner instantiates `DefaultAgent` and the textual/deterministic model directly, without going through `mini`. Document how flags/exit codes correspond and where they diverge, without promising the full `mini-extra` interface. Interruption policy: in Python, `KeyboardInterrupt` does not go through `except Exception`, but the `finally` still saves the trajectory; the Rust CLI installs a SIGINT handler that saves the current state and exits with a documented code, instead of the default abort without saving. Exit code: 0 for normal termination (any `exit_status`), non-zero for a propagated error.

**M1 acceptance:** the gates P01–P18 from STUDY §10 are closed, each pointing to the test or fixture that covers it; the required Gherkin features are the ones named above (submission, limit before the query, timeout with a child, stateless shell, consecutive format errors, cost at the threshold, invalid template, Unicode output, saving on failure), with no silent skip; the CLI produces a readable trajectory; the report states exactly what was tested.

### Step 9 — One real provider (M2)

1. Choose and record the provider, model, and API format; keep textual shell actions.
2. Implement authentication through an environment variable, payload, timeout, bounded retry, HTTP errors, empty/truncated responses, usage, and cost. Do not log tokens or sensitive payloads.
3. Use a test HTTP server and recorded/sanitized responses to compare the adapter with the chosen Python path. LiteLLM adds behavior that a plain HTTP client does not reproduce automatically.
4. Test 401/403, 429, 5xx, timeout, and invalid JSON separately. Freeze time/backoff in tests; distinguish HTTP attempts from queries counted by the agent.
5. Run an opt-in real smoke test with an explicit budget and a disposable directory. Do not require identical text from two real calls, even at temperature zero.
6. Evaluate fixed tasks with the number of samples, model version, cost, latency, and success rate. Separate model variation from harness regressions.

**M2 acceptance:** the recorded HTTP contract passes, an identified real test works, and costs are visible. This proves only the selected integration.

### Step 10 — Docker (M3) and controlled expansion (M4)

Docker has its own contract in `spec/docker.md`: image by digest, creation, cwd, `env` versus `forward_env`, arguments, interpreter (`bash -lc` in the reference), pull/exec timeout, output, and cleanup. Test an unavailable daemon, partial startup, a command with a child, timeout, interruption, and container removal. A `docker exec` client timeout does not prove that the process inside the container was terminated.

Keep tests of the scripted Docker command separate from real isolation tests. Check mounts, access to the host/socket, network, user, and resource limits; a container with broad mounts is not enough isolation on its own. Stricter profiles are documented security decisions, not assumed parity. Explicit, verifiable cleanup must be the main path; `Drop` is only a fallback, with no panic and no dependency on async in the destructor.

Only then extend providers, environments, the interactive interface, and benchmarks. For each item: contract → Python fixture → Rust adaptation → differential tests → integration test → report. Batch runs need concurrency limits, per-task isolation, coordinated writing of results, and resumption. Tool-calling/multimodal stay excluded until the scope changes.

### Step 11 — Quality, CI, and study evidence

Proposed pipeline:

```bash
cargo fmt --manifest-path rust-mini-swe-agent/Cargo.toml --check
cargo clippy --manifest-path rust-mini-swe-agent/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path rust-mini-swe-agent/Cargo.toml --locked
cargo doc --manifest-path rust-mini-swe-agent/Cargo.toml --no-deps
```

Add jobs for Python/the oracle and offline comparison, BDD, Linux/macOS, and MSRV. Docker and real providers go in separate jobs with explicit prerequisites. A suite skipped for lack of credentials/daemon is **unverified**, not passed. Publish the parity diffs as CI artifacts, with secrets removed.

Measure startup, memory, and loop throughput with a simulated model/environment in release mode; measure end-to-end latency and costs separately with a real API. Pin the hardware, versions, workload, and repetitions. Do not promise that Rust speeds up a run dominated by LLM time.

## 5. Applying rust-skills

The skill was consulted for this plan. Apply rules as needed, not every optimization automatically:

| Rules | Application in this project |
|---|---|
| `own-borrow-over-clone`, `own-slice-over-vec` | Borrow the history; move new messages; no large clones for convenience |
| `err-result-over-panic`, `err-thiserror-lib`, `err-context-chain` | Process/network/config failures with Result and the cause preserved |
| `type-no-stringly`, `pat-exhaustive-enum` | Typed states and terminations, explicit conversion to compatibility JSON |
| `serde-default-compat`, `serde-flatten` | Preserve the schema's extras and distinctions; defaults only where there is a contract |
| `proj-lib-main-split`, `proj-flat-small`, `proj-msrv-declare` | One simple package, testable lib, thin main, and reproducible toolchain |
| `test-mock-traits`, `test-integration-dir` | Scripted dependencies and end-to-end scenarios |
| `obs-structured-fields`, `obs-no-sensitive-data` | Run ID, phase, duration, and cost in structured logs, without secrets |
| `perf-profile-first`, `anti-premature-optimize` | Measure before Arc, pools, caches, or specialized collections |

Avoid `unwrap`/`expect` on normal paths; prefer safe process APIs. Do not introduce `unsafe`, excessive generics, or async just to show technical skill. If async comes in later, ensure cancellation, concurrency limits, and no locks held across await.

## 6. Completion criteria and checklist

- [ ] Python reference and environment reproducible, origin identified.
- [ ] Scope and contracts approved in the repository; exclusions transparent.
- [ ] Rust package created, build and quality checks passing.
- [ ] LocalEnvironment correct, including timeout and descendants.
- [ ] Loop, templates, parsing, costs, and limits equivalent.
- [ ] Comparator tested against real differences; normalizations restricted.
- [ ] Trajectories and intermediate snapshots equivalent in the fixtures.
- [ ] Cucumber scenarios passing along with the Rust/Python tests.
- [ ] Minimal CLI and reproducible instructions published (M1).
- [ ] Chosen real provider validated with a contract and a smoke test (M2).
- [ ] Docker validated separately or explicitly pending (M3).
- [ ] Expansions with per-component coverage, without claiming full parity (M4).
- [ ] README updated with the commands run, results, limitations, and links to evidence.

Each step can be a small PR. The description must explain the behavior change, the Python reference, and the validation. The portfolio shows engineering through the link between requirement, decision, test, and evidence; not through the number of dependencies or through results not yet measured.
