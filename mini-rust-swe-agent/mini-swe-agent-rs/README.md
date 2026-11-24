# mini-swe-agent-rs

A Rust port of the [mini-swe-agent](https://github.com/SWE-agent/mini-swe-agent) harness: text-based actions,
bash only, a linear message history. The plan is [spec/migration-plan.md](spec/migration-plan.md); the Python
reference lives in [`../mini-swe-agent/`](../mini-swe-agent/) and is never edited.

## Status

All ten phases of the plan are done and merged into `main`: **M1** (offline parity, P0–P7) and **M2** (a usable
agent, P8–P9) are reached. Each phase was reviewed by Codex, Grok and Antigravity; the findings and what was done
about each one are in `spec/reviews/pN/FINDINGS.md`.

| Phase | State | Pull request |
|---|---|---|
| P0 Scaffold | Done: crate, toolchain, lints, YAML loader (D5), Python baseline | #21 |
| P1 Core types | Done: `Message`, `Action`, `ExecOutput`, `Interrupt`/`StepError`/`AgentError`, `recursive_merge`, `Clock`, `HostEnv`, and the `Model`/`Environment`/`Agent` traits | #22 |
| P2 Templates | Done: `src/template.rs` (minijinja 3.1, strict undefined, Python value formatting); 50 Jinja2 goldens match byte for byte | #23 |
| P3 LocalEnvironment | Done: `bash -c` per action, merged output, process-group kill, one absolute deadline (single-thread `poll` loop), Python error texts, submission check; 18 `test_local.py` functions ported plus new cases | #24 |
| P4 Text actions | Done: `src/model/actions_text.rs` (`findall` group rule, `re.DOTALL`, Python `strip`, `FormatError` message, observations); 51 parser and 15 observation goldens from Python match (key order included) | #25 |
| P5 Agent loop | Done: `DefaultAgent` (limits, format-error counter, fatal exit message, save after every step, atomic pretty-JSON save), `DeterministicModel`, `GlobalModelStats`; `test_default.py` text cases and `test_save.py` ported | #26 |
| P6 Parity and BDD | Done: 27 JSON scenarios equal through `parity/run_python.py` and the Rust library; comparator mutation tests; Cucumber features with a manifest check; full-suite Linux gate (`parity/Dockerfile`) | #27 |
| P7 Config and CLI | Done: `src/config.rs` and `src/main.rs`; typed dumps of both shipped YAML files equal pydantic's; Python `mini` and the Rust CLI write equal trajectories for 4 cases. **M1** | #28 |
| P8 Real model | Done: `TextbasedModel<B: ChatBackend>` over an OpenAI-compatible endpoint (`ureq`, rustls), tenacity's retry schedule, registry costs, `examples/hello_world.rs`; fake-server tests, the raw-response differential against Python's `LitellmTextbasedModel`, and one run against OpenAI | #29 |
| P9 Docs | Done: this README (status, usage, BDD, every divergence and decision, evidence), `tests/docs.rs` (checks those lists against the plan and the paths in the parent documents), the parent `README.md` and `CLAUDE.md` brought up to date. **M2** | #30 |

## Usage

**Provider (D4).** The text-based model speaks the OpenAI-compatible Chat Completions protocol (`POST
{api_base}/chat/completions`, bearer auth). That covers OpenAI, OpenRouter, a LiteLLM proxy, vLLM and Ollama.
`model.model_kwargs` is split: `api_base`/`base_url` (default `https://api.openai.com/v1`), `api_key` (else the
variable named by `api_key_env`, default `OPENAI_API_KEY`) and `timeout` (seconds, default 600, as LiteLLM's)
configure the transport; `drop_params` is dropped; `model`, `messages`, `tools`, `tool_choice`, `stream` and `n` are
refused; the rest (`temperature`, `max_tokens`, …) is sent in the body. `api_base` must be `http(s)://host…`,
without credentials or a fragment; its query string is kept after `/chat/completions`. Redirects are not followed
(the history never goes to another host). The proxy comes from `ALL_PROXY`, `HTTPS_PROXY` or `HTTP_PROXY` (minus
`NO_PROXY`) in the same environment the run sees, including the global `.env`.

**API key.** Put `OPENAI_API_KEY=...` in the global `.env` (macOS: `~/Library/Application Support/mini-swe-agent/.env`;
Linux: `~/.config/mini-swe-agent/.env`; or the directory in `MSWEA_GLOBAL_CONFIG_DIR`), the file Python's
`mini-extra config set` writes, or export it. Variables already set in the environment win over the file.

```bash
OPENAI_API_KEY=... cargo run --locked -- -m gpt-4o-mini -c mini_textbased \
  -c model.litellm_model_registry=registry.json -t "List the files"
cargo run --example hello_world -- -m gpt-4o-mini -t "List the files"   # MSWEA_COST_TRACKING=ignore_errors
```

Costs come from a LiteLLM-format registry file (`litellm_model_registry` or `LITELLM_MODEL_REGISTRY_PATH`; no prices
are bundled), or set `cost_tracking: ignore_errors` (`MSWEA_COST_TRACKING`) to run at cost 0.0. Retries follow
Python's: up to `MSWEA_MODEL_RETRY_STOP_AFTER_ATTEMPT` tries (default 10), waits of `clamp(2^(n-1), 4, 60)` seconds,
and never after HTTP 401, 403, 404, or a 400 that reports a context-window overflow or an unsupported parameter.

The deterministic model needs no network:

```bash
cargo run --locked -- --model-class deterministic -m deterministic -c mini_textbased \
  -c 'model.outputs=[{"role":"assistant","content":"x","extra":{"actions":[{"command":"echo COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT"}]}}]' \
  -t "Say hello" -o /tmp/traj.json
```

Exit codes: 0 when the run ends with an exit message (any status, including `LimitsExceeded`), 1 for a fatal or
config error (the trajectory is saved first when the run had started), 2 for usage errors and for the
interactive-only flags `-y/--yolo` and `--exit-immediately` (D7). `--help` and `--version` exit 0. "Saved trajectory"
is printed only after a successful run, as Python prints it after `run` returns.

**Typed config goldens.** `tests/goldens/config/*.json` are pydantic `model_dump(mode="json")` dumps of both
shipped YAML files (`parity/config_dumps.py`).

## Commands

Run inside this directory. The toolchain is pinned in `rust-toolchain.toml` (1.99.1).

```bash
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo build --locked
cargo test --locked        # unit, integration and doc tests
cargo deny check advisories
```

**Linux gate (Rust-only).** It mounts only this crate, so the check that `config/` is identical to the reference
is skipped on purpose through `MSWEA_RS_NO_REFERENCE=1`:

```bash
docker run --rm -e MSWEA_RS_NO_REFERENCE=1 -e CARGO_TARGET_DIR=/tmp/target -v "$PWD":/src -w /src rust:1.99.1 \
  sh -c 'rustup component add rustfmt clippy && cargo fmt --check && cargo clippy --all-targets --locked -- -D warnings && cargo build --locked && cargo test --locked'
```

The parity (`tests/parity.rs`) and BDD (`tests/bdd.rs`) targets print that they were skipped on purpose there.

**Parity and BDD (P6).** Both targets run every scenario of `parity/scenarios/` through the Python reference and
the Rust library and compare the normalized results. They need `uv` and the reference's `uv.lock`, which the
reference's own `.gitignore` excludes, so it exists only in a checkout where it was generated. In a git worktree,
point the harness at such a checkout:

```bash
MSWEA_RS_REFERENCE_DIR=/path/to/mini-rust-swe-agent/mini-swe-agent cargo test --locked --test parity --test bdd
```

**Linux gate (full suite).** `parity/Dockerfile` adds `uv` 0.13.9 and Python 3.15.2 to `rust:1.99.1`. The
reference is mounted read-only and copied inside the container (without its host virtualenv):

```bash
docker build -t mswea-rs-full parity
docker run --rm -v "$PWD/..":/work -v /path/to/reference/with/uv.lock:/work/mini-swe-agent:ro \
  -w /work/mini-swe-agent-rs mswea-rs-full
```

**YAML goldens.** `tests/goldens/yaml/*.json` are PyYAML `safe_load` dumps of `config/*.yaml` and
`parity/yaml_cases/*.yaml`. Regenerate them only on purpose, from the reference environment:

```bash
uv run --frozen --project ../mini-swe-agent python parity/yaml_dump.py
```

**Template goldens.** `tests/goldens/templates/cases.json` holds Jinja2 renderings of the shipped templates, the
Python class defaults and value-formatting probes. Regenerate only on purpose:

```bash
MSWEA_SILENT_STARTUP=1 MSWEA_GLOBAL_CONFIG_DIR=$(mktemp -d) uv run --frozen --project ../mini-swe-agent python parity/template_goldens.py
```

**Action goldens.** `tests/goldens/actions/*.json` come from the reference `parse_regex_actions` and
`format_observation_messages` with `time.time` fixed:

```bash
MSWEA_SILENT_STARTUP=1 MSWEA_GLOBAL_CONFIG_DIR=$(mktemp -d) uv run --frozen --project ../mini-swe-agent python parity/action_goldens.py
```

## Running the BDD features

The Gherkin features are in `tests/features/`:

| Feature | Scenarios | What each scenario does |
|---|---:|---|
| `agent_loop.feature` | 21 | Runs one agent scenario of `parity/scenarios/` through the Python reference and the Rust library, then checks the query count, the exit status and that the normalized results are equal |
| `local_environment.feature` | 6 | The same for the environment scenarios (timeouts, signals, Unicode, submission) |
| `cli.feature` | 9 | 4 scenarios run Python `mini` and the Rust CLI on the same arguments and compare exit codes and normalized trajectories; 5 check the Rust CLI alone (where the task comes from, which config is loaded) |

The runner (`tests/bdd.rs`, `harness = false`) needs the Python reference, like `tests/parity.rs`:

```bash
MSWEA_RS_REFERENCE_DIR=/path/to/mini-rust-swe-agent/mini-swe-agent cargo test --locked --test bdd
```

It runs every feature, one scenario at a time, and fails on a skipped step. After the run, every JSON scenario and
every CLI case must have been compared exactly once (the manifest check), so filtering with cucumber's `--name` or
`--tags` makes the run fail on purpose. With `MSWEA_RS_NO_REFERENCE=1` it prints that it was skipped, which is not a
pass.

## Divergences from Python

Every deliberate divergence of the plan (§2.4), with the test that pins it. `tests/docs.rs` fails if a row of
§2.4 is missing here.

| Divergence | Python | Rust | Checked by |
|---|---|---|---|
| `info.config.*_type` | `minisweagent.agents.default.DefaultAgent` | `type_name()` strings such as `mini_swe_agent_rs::DefaultAgent` | Normalized by the comparator (`tests/support/compare.rs`); `tests/comparator.rs` |
| `info.mini_version` | `2.4.6` | The crate version | Normalized by the comparator; `tests/comparator.rs` |
| Default model class | Tool-calling `LitellmModel` | Text-based model | `model_class_and_name_resolution` (`tests/config.rs`) |
| Fatal `exit_status`, `content`, `exception_str` | Python class name and `str(e)` (`RuntimeError`, `IndexError`) | A stable name per error variant and its `Display` (D6) | `fatal_exits_map_python_names_and_drop_texts` (`tests/comparator.rs`); scenarios `outputs_exhausted`, `injected_runtime_error` |
| `extra.traceback` | Python traceback | The error's `source()` chain | Normalized for fatal exits; `fatal_exit_status_names_are_stable` (`tests/core_types.rs`) |
| JSON encoding | `ensure_ascii=True` | Raw UTF-8 | Scenarios compare parsed JSON; `non_ascii_and_crlf_output`, `env_unicode_output` |
| `extra.response` | `litellm` `ModelResponse.model_dump()` | The provider's raw JSON | Left out of the raw-response projection (`tests/text_parity.rs`); the raw JSON is asserted in `tests/textbased.rs` |
| Child process session | `setsid` (no controlling terminal), stdin inherited | New process group (`process_group(0)`), **stdin `/dev/null`** | `stdin_is_at_eof`, `timeout_kills_child_process` (`tests/local_env.rs`) |
| Hang after kill when a grandchild escaped the group | `communicate()` blocks | Bounded grace period, then partial output | `background_child_with_redirected_output_survives`, `child_that_closes_stdout_and_keeps_running_is_killed` (`tests/local_env.rs`) |
| Cost | `litellm.completion_cost` (tiers, cache tokens, provider cost) | Flat per-token input and output prices from a registry file (3.8) | `cost_tracking_errors_and_ignore_errors`, `registry_prices_must_be_nonnegative` (`tests/textbased.rs`); `tests/text_parity.rs` |
| Authentication error hint | Mentions `mini-extra config set` | Mentions the API-key environment variable | `an_unreadable_abort_body_still_aborts_with_a_key_hint` (`tests/fake_http.rs`) |
| YAML 1.1 booleans (`yes`/`no`/`on`/`off` unquoted) and pydantic lax coercion | Booleans; `"3"` → 3 | YAML 1.2 (strings); strict serde types | `tests/yaml.rs`; `textbased_literals_are_checked` (`tests/config.rs`) |
| Built-in configs | All YAML in `config/`, `extra/`, `benchmarks/` | Only the text-based ones embedded; lookup order unchanged | `builtin_configs_are_findable_by_name`, `lookup_order_and_suffix_rule` (`tests/config.rs`) |
| Shell (D11) | `shell=True`: `/bin/sh -c` (bash in POSIX mode on macOS, `dash` on Debian/Ubuntu) | `/bin/bash -c` on every OS | `runs_bash` (`tests/local_env.rs`); scenarios avoid `sh`/`bash` differences |
| Startup file `BASH_ENV` (D11) | Ignored by `/bin/sh`; passed on to children | Inherited `BASH_ENV` removed from the child environment; kept if set in `config.env` | `inherited_bash_env_is_not_sourced` (`tests/local_env.rs`) |
| Default config with no `-c` | `mini.yaml` (tool-calling, `run/mini.py:22`) | Embedded `mini_textbased.yaml` | `explicit_specs_replace_the_default_and_flags_come_last` (`tests/config.rs`); parity cases always pass `-c` |
| Task when neither `-t` nor `run.task` is given | `_multiline_prompt()` (prompt_toolkit), after the setup wizard if `MSWEA_CONFIGURED` is unset | Read stdin to EOF; never a wizard | `task_order_flag_then_config_then_stdin` (`tests/cli.rs`) |
| `examples/hello_world.rs` | `hello_world.py` uses the tool-calling `LitellmModel` | `TextbasedModel` | Built and linted by `cargo clippy --all-targets` |
| Literal `model_kwargs.api_key` in the trajectory | Dumped as given | `"<redacted>"` | `a_literal_api_key_is_redacted_in_the_trajectory` (`tests/fake_http.rs`), `settings_split_and_secrets` (`tests/textbased.rs`) |
| Non-UTF-8 environment entries | Decoded with `surrogateescape`, visible to templates | Passed to the child unchanged; left out of template vars | `host_env_keeps_non_utf8_entries_for_children_only` (`tests/core_types.rs`) |
| Timeout when the shell left its process group | `communicate()` hangs after `killpg` | `child.kill()` after `killpg` | `timeout_kills_a_shell_that_left_its_process_group` (`tests/local_env.rs`) |

The phases also recorded smaller divergences, each in its design note below: the bounded drain and error texts of
`LocalEnvironment` (P3), the regex patterns refused at load (P4), the agent's raw-UTF-8 atomic save and stable
fatal names (P5), the CLI's class and `run.task` rules (P7), and the transport rules of the text-based model (P8:
one wire protocol, no redirects, the proxy from the run's environment, invalid replies retried and not billed).

## Decisions

The outcome of each decision of the plan (§5.2), as implemented. `tests/docs.rs` fails if one is missing.

| Decision | Question | Outcome |
|---|---|---|
| D1 | Crate directory name | `mini-swe-agent-rs/` (decided). The crate lives there; `../CLAUDE.md`, `../README.md` and `../spec/prompt/migrate.md` use that name (`tests/docs.rs`) |
| D2 | Which plan is authoritative | One plan (decided): `spec/migration-plan.md`; the old `spec/PLAN.md` and `spec/STUDY.md` were merged and removed |
| D3 | Text-based only, or also tool-calling with a single `bash` tool | Text only, as implemented: `TextbasedModel` parses one `mswea_bash_command` block; no tool-calling model exists |
| D4 | First real provider | OpenAI-compatible Chat Completions (`src/model/openai_compat.rs`); run against OpenAI with `gpt-4o-mini` on 2026-11-24. Native Anthropic is later work |
| D5 | YAML crate (survey of crates.io and RustSec, 2026-11-23) | `serde-saphyr` 2.2.0 (decided), `strict_booleans` and last-wins duplicate keys (`src/yaml.rs`, `tests/yaml.rs`) |
| D6 | Fatal `exit_status` names | Stable Rust variant names (`OutputsExhausted`, `GlobalLimitExceeded`, …) with the comparator's mapping table (`IndexError` → `OutputsExhausted`); provider errors use LiteLLM's names; a cost error is `RuntimeError` |
| D7 | CLI compatibility for interactive flags (`-y`, `--exit-immediately`) and an alias `mini` | Rejected: `-y/--yolo` and `--exit-immediately` exit 2 with a message; no `mini` alias (`usage_and_config_errors`, `tests/cli.rs`) |
| D8 | Ctrl-C handling | Default handling: no `SIGINT` handler; the trajectory is saved after every step, so at most the current step is lost |
| D9 | BDD with Cucumber (required by `../spec/prompt/migrate.md`) | In scope (decided): `tests/bdd.rs` with three features, one Gherkin scenario per parity scenario plus the CLI cases, and a manifest check |
| D10 | Generic vs `dyn` composition | Generics: `DefaultAgent<M, E>` and one generic `run_agent` in `main.rs` per model type; no `dyn Model` |
| D11 | Shell binary | `/bin/bash -c` (decided), recorded as a divergence; an inherited `BASH_ENV` is dropped |
| D12 | Clock for limits and timestamps | Wall clock behind the injected `Clock` (`SystemClock`, `ManualClock`); limits and timestamps use it |

## Evidence

### Summary on `main` (P9, 2026-11-24)

| Command | Result |
|---|---|
| `cargo fmt --check`; `cargo clippy --all-targets --locked -- -D warnings`; `cargo build --locked` | Pass |
| `cargo test --locked` on macOS (27.2.1) with the reference (`MSWEA_RS_REFERENCE_DIR`) | Pass: 267 tests in 23 targets (plus doc tests), 1 ignored (`real_endpoint`, opt-in); BDD 3 features, 36 scenarios, 163 steps; 27 parity scenarios, 4 CLI cases and 7 raw-response cases equal to Python |
| Full-suite Linux gate (`parity/Dockerfile`: `rust:1.99.1`, uv 0.13.9, Python 3.15.2) | Pass (exit 0) |
| Rust-only Linux gate (`MSWEA_RS_NO_REFERENCE=1`) | Pass; the reference-dependent checks print that they were skipped on purpose |
| `cargo deny check advisories` | No advisories (one recorded exception in `deny.toml`: `humantime`, dev-only through `cucumber`) |
| `cargo test --test real_endpoint -- --ignored` against OpenAI | Pass (see P8) |

Where the parity evidence lives:
- Library parity: the JSON scripts in `parity/scenarios/`, run by `parity/run_python.py` and `tests/parity.rs`
  through the comparator in `tests/support/compare.rs` (whose mutation tests are `tests/comparator.rs`).
- BDD: `tests/bdd.rs` over `tests/features/`.
- CLI parity: the cases in `tests/support/cli.rs`, compared by `tests/cli.rs` and `cli.feature`.
- Text-based model: the raw-response differential `tests/text_parity.rs` over `parity/text_model_cases.json`.
- Goldens from the reference: `tests/goldens/` (YAML, templates, actions, typed configs), with the scripts in
  `parity/` that regenerate them.
- Reviews: `spec/reviews/p0/` to `spec/reviews/p8/`; each `FINDINGS.md` records every finding, its disposition
  and the test that covers the fix.

### P0 (2026-11-24)

| Check | Result |
|---|---|
| Python baseline: `uv run --frozen pytest tests/agents/test_default.py tests/environments/test_local.py tests/config/test_init.py tests/utils/test_serialize.py tests/run/test_local.py` in `../mini-swe-agent`, with `MSWEA_SILENT_STARTUP=1` and a temporary `MSWEA_GLOBAL_CONFIG_DIR` | 124 passed (uv 0.13.9, Python 3.15.2, macOS 27.2.1) |
| `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo build --locked`, `cargo test --locked` on macOS | Pass |
| Rust-only Linux gate (`rust:1.99.1` image): fmt, clippy, build, test | Pass |
| `cargo deny check advisories` | No advisories |
| Reference provenance (plan §1.0) | Text path identical to upstream tag `v2.4.6` (commit `a83fcae`) |

### P6 (2026-11-24)

| Check | Result |
|---|---|
| `cargo test --locked` on macOS with the reference (all targets, including `parity` and `bdd`) | Pass: 27 of 27 scenarios equal; BDD 27 scenarios, 135 steps |
| Full-suite Linux gate (`parity/Dockerfile`) | Pass (exit 0) |
| Rust-only Linux gate | Pass; `parity` and `bdd` skipped on purpose |
| `cargo deny check advisories` | No advisories, after one recorded exception in `deny.toml`: `humantime` (unmaintained, dev-only through `cucumber`, D9) |

### P7 (2026-11-24)

| Check | Result |
|---|---|
| `cargo test --locked` on macOS with the reference | Pass; BDD 3 features, 36 scenarios, 163 steps; the 4 CLI gate cases equal |
| Full-suite Linux gate (`parity/Dockerfile`), including the CLI gate against Python `mini` | Pass (exit 0) |
| Rust-only Linux gate | Pass |
| `cargo deny check advisories` | No advisories (with the recorded `deny.toml` exception) |

### P8 (2026-11-24)

| Check | Result |
|---|---|
| `cargo test --locked` on macOS with the reference | Pass: `textbased` 17, `openai_compat` 7, `fake_http` 12, `text_parity` 1 (7 raw-response cases equal, outbound messages included), everything else as in P7 |
| Sabotage: the output price replaced by the input price in `cost.rs` | `text_parity` fails on 5 cases |
| Sabotage: `extra` sent to the provider; `extra.response` dropped | `text_parity` fails on each |
| Full-suite Linux gate, including `text_parity` against Python | Pass (exit 0) |
| Rust-only Linux gate, including `fake_http` | Pass |
| `cargo deny check advisories` (with `ureq` 3.5.1 and rustls) | No advisories |
| Real endpoint (`tests/real_endpoint.rs`, `--ignored`), OpenAI `gpt-4o-mini`, registry prices from LiteLLM's bundled table | Pass (6.4 s). A separate CLI run of the same task: `Submitted` after 3 calls, cost $0.00049; the key does not appear in the trajectory, stdout or stderr |

## Design notes

- **Lints.** `unsafe_code` is forbidden; `unwrap`, `expect`, `panic!`, indexing, `todo!`, `unimplemented!` and
  `dbg!` are denied for every target in `Cargo.toml`. `clippy.toml` exempts `#[test]` functions from the first
  four only (`todo!`, `unimplemented!` and `dbg!` stay denied). A test helper outside a `#[test]` function that
  needs one of them carries its own `#[allow(...)]`.
- **YAML (D5).** `serde-saphyr` 2.2.0 with `strict_booleans` (YAML 1.2: `yes`/`no`/`on`/`off` stay strings, a
  documented divergence from PyYAML) and last-wins duplicate keys (as PyYAML). Its parser family adds a `\n` to a
  block scalar that ends the file without a final newline, which both shipped configs do. `src/yaml.rs` finds
  that scalar by parsing the text again with a sentinel appended (no text heuristics) and removes the break;
  the `eof_*` goldens pin PyYAML's result, including trailing comments, aliases, folded scalars, trailing
  spaces and duplicate keys. Plain scalar keys (`42: a`) load as their JSON key text (`"42"`), which is what
  PyYAML plus `json.dumps` produce; sequence and mapping keys are an error. `yaml-rust2` and `saphyr` behave the same, so the
  fallback would not help.
- **Actions** are decoded by key presence (`src/message.rs`): a `raise` key always means a scripted raise and its
  payload must be valid, as Python checks `"raise" in action` first. An untagged serde enum would read
  `{"raise": …}` as an empty command.
- **Interrupts** hold at least one message by construction; the first one is boxed so `Result<_, StepError>`
  stays small.
- **`HostEnv`** keeps every entry for child processes, and only the UTF-8 entries for templates.
- **Templates (P2).** minijinja with `UndefinedBehavior::Strict`, no auto-escaping, and a formatter that prints
  `None`, `True`/`False` and floats the way Python's `str()` does (`1e+20`, `1e-05`, `-0.0`). Dicts and lists
  still render in minijinja's format; no shipped template prints one. The formatter applies only to `{{ … }}`
  output: values turned into strings inside an expression (`~`, `| string`, `| join`) use minijinja's format
  (`true`, `none`), where Jinja2 gives `True`, `None`. No shipped template does this; it matters only for custom
  templates. Maps keep their insertion order (`preserve_order`). Templates compile when added, so a syntax error
  fails at load; an unknown filter fails at the first render, which is when Python builds its `Template` too.
- **LocalEnvironment (P3).** `src/environment/local.rs`. Divergences from Python (plan §2.4): `/bin/bash`
  instead of `/bin/sh` (D11), an inherited `BASH_ENV` is dropped (bash would source it), stdin is `/dev/null`, a
  process group instead of a new session, `child.kill()` after `killpg` (Python hangs if the shell left its
  group), and a 1 s bounded drain after a kill. Error texts follow CPython (`[Errno 2] No such file or directory:
  '/x'`, `Command '…' timed out after N seconds`), with the platform `strerror` text. The module is over the
  plan's 250-line budget (§2.6), at about 430 lines of code: Python gets the pipe, the deadline, the process-group
  kill and the OS error texts from `subprocess`; the port writes them out (the `poll` loop, `killpg`, and CPython's
  error formatting).
- **Text actions (P4).** `action_regex` is compiled by the `regex` crate after a small rewrite: `\s` and `\S`
  get Python's whitespace set (with `\x1c`-`\x1f`) and `\Z` becomes `\z`. Patterns whose meaning cannot be kept
  are refused when the model is built: `$` outside a class (Python's also matches before a final newline), `\S`
  inside a class, lookaround and backreferences, more than one group, and any pattern that can match the empty
  string (the engines restart differently after an empty match). `\w`, `\d` and `\b` keep the `regex` crate's
  Unicode classes (risk R6). The shipped pattern is unaffected.
- **Agent loop (P5).** `src/agent/default.rs`. `query` appends and charges the assistant message before
  `step` decodes its actions, as Python does. The deterministic model iterates `extra.actions` before charging, so
  malformed actions fail inside the model. Divergences (plan §2.4): `mini_version` is the crate version,
  `traceback` is the error's `source()` chain, the exit status of a fatal error is a stable name (D6, such as
  `OutputsExhausted` for Python's `IndexError`), the agent's templates compile at construction, and the
  trajectory is written as raw UTF-8 through a temporary file and a rename. Without an `output_path`, nothing is
  serialized after a step (Python builds the dict and drops it). `run` returns the exit message's whole `extra`
  (`ExitInfo::extra`). `src/agent/default.rs` is over the plan's 250-line budget (§2.6), at about 350 lines of code:
  Python's `finally` save, its exception-to-exit-message handler and the trajectory writer are spelled out with
  explicit error paths (save replacing a fatal error, an invalid cost inside the format-error handler, the
  atomic write).
- **Parity harness (P6).** `tests/support/` holds the Rust runner, the test-only recording wrappers (history
  passed to each query, actions passed to `execute`) and the comparator; `parity/run_python.py` is the Python
  runner. The Python side uses a thin `DeterministicModel` subclass that turns the JSON `raise` specs into
  exceptions, and replaces the module-level `GLOBAL_MODEL_STATS` per scenario; it reads the reference package's
  own `default.yaml`, and the harness checks that the imported `minisweagent` lives in the reference checkout. Both
  sides get one explicit environment (`PATH`, `HOME`, `LANG=C.UTF-8`, `TMPDIR`, `MSWEA_SILENT_STARTUP`,
  `MSWEA_CONFIGURED`, `MSWEA_GLOBAL_CONFIG_DIR`, plus the scenario's variables), fresh `HOME`, `TMPDIR` and config
  directories for each scenario, and a fresh workspace at the same absolute path. A scenario with
  `"output_path": true` saves to `<base>/<id>/traj.json` (same path on both sides) and compares the file. Workspace
  snapshots include directories and symlink targets. Normalized paths: message and query `extra.timestamp` (when
  numeric), the three `*_type` paths, `info.mini_version`, and for a fatal exit the exit message's `content`,
  `exception_str` and `traceback` (when they are strings), with the exit status mapped by D6 (`IndexError` →
  `OutputsExhausted`, plus each scenario's own map). Everything else is compared exactly; every layer of the
  scenario kind must be present; an integer equals a float that is exactly that integer, and integers are never
  compared through `f64`. A scenario also fails on unexpected leftover or missing scripted outputs, on a fatal or
  saved outcome other than expected, and when the BDD manifest does not show each scenario compared exactly once.
- **Config and CLI (P7).** Divergences (plan §2.4): the default config is the embedded `mini_textbased.yaml`
  (Python's is the tool-calling `mini.yaml`); `--agent-class` accepts only `default` and an absent class means
  `default` (Python's CLI default is the interactive agent); `--environment-class` accepts only `local`; with no
  `-t` and no `run.task`, the task is read from stdin to EOF instead of a prompt; the setup wizard never runs; a
  `key=value` value that is not JSON for `serde_json` (`NaN`, `Infinity`) stays a string. Messages go to stderr.
  A present `run.task` must be a string (`""` included): Python renders any value, this port refuses other types
  instead of reading stdin. A `key=value` path deeper than 128 segments is refused (the merge recurses through
  it). `-l` must be finite. The banner is decided before `.env` is read, as in Python (the plan listed it last).
  Sections and classes follow the Python factories: `agent`/`environment` must be mappings and a present class
  must be a non-empty string; a `null` `model` section is `{}` and a falsy `model_class` is the default; a truthy
  non-string `model_name` is an error, a falsy one falls back to `MSWEA_MODEL_NAME`. The file lookup drops `.`
  components as `pathlib` does and keeps `..`. `-o ""` is "not given". `environment.env` keeps its key order.
- **Text-based model (P8).** Divergences (plan §2.4): one wire protocol (OpenAI-compatible) instead of LiteLLM's
  provider routing; cost is the registry formula only (`prompt × input + completion × output`, exact name then
  without the `provider/` prefix), with no bundled prices; `set_cache_control` is kept in the config (and set for
  Anthropic names, as `get_model` does) but no cache-control markers are sent, and Anthropic thinking blocks are not
  reordered; a literal `model_kwargs.api_key` is written as `"<redacted>"` in trajectories and template variables;
  `api_key_env` is a Rust-only setting. Error exit statuses use LiteLLM's exception names
  (`AuthenticationError`, `ContextWindowExceededError`, …); a cost error is `RuntimeError`, as in Python. The
  `response` stored in `extra` is the provider's raw JSON, where Python stores LiteLLM's normalized dump; the
  raw-response differential therefore compares outcome, content, actions, cost, `extra` keys and global stats.
  Retry warnings go to stderr as `WARNING:litellm_model:…`; neither the API key nor a request body is ever logged.
  Provider error bodies are cut to 2,000 characters, an echoed API key is replaced by `<redacted>`, and an
  authentication error names the key variable (Python suggests `mini-extra config set`). A 2xx reply without a
  first choice, with a non-assistant role or with non-text content is an invalid response: it is retried and never
  billed (LiteLLM fails while parsing such a reply, before any cost). As in Python, commands and templates see the
  whole process environment, the API key variable included (`local.py:29`, `:59`).
- **Module layout.** The YAML loader is `src/yaml.rs`, used by `config.rs` in P7 (the plan lists only `config.rs`).
