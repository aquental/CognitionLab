# mini-swe-agent-rs

A Rust port of the [mini-swe-agent](https://github.com/SWE-agent/mini-swe-agent) harness: text-based actions,
bash only, a linear message history. The plan is [spec/migration-plan.md](spec/migration-plan.md); the Python
reference lives in [`../mini-swe-agent/`](../mini-swe-agent/) and is never edited.

## Status

| Phase | State | Pull request |
|---|---|---|
| P0 Scaffold | Done: crate, toolchain, lints, YAML loader (D5), Python baseline | stacked PR, base `main` |
| P1 Core types | Done: `Message`, `Action`, `ExecOutput`, `Interrupt`/`StepError`/`AgentError`, `recursive_merge`, `Clock`, `HostEnv`, and the `Model`/`Environment`/`Agent` traits (no implementations yet) | stacked on P0 |
| P2 Templates | Done: `src/template.rs` (minijinja 3.1, strict undefined, Python value formatting); 44 Jinja2 goldens match byte for byte | stacked on P1 |
| P3 LocalEnvironment | Done: `bash -c` per action, merged output, process-group kill, one absolute deadline (single-thread `poll` loop), Python error texts, submission check; 18 `test_local.py` functions ported plus 20 new cases | stacked on P2 |
| P4 Text actions | Done: `src/model/actions_text.rs` (`findall` group rule, `re.DOTALL`, Python `strip`, `FormatError` message, observations); 28 parser and 8 observation goldens from Python match | stacked on P3 |
| P5 Agent loop | Done: `DefaultAgent` (limits, format-error counter, fatal exit message, save after every step, atomic pretty-JSON save), `DeterministicModel` (`raise`, `/sleep` on the injected clock, `/warning`), `GlobalModelStats`; `test_default.py` text cases, the two format-error fakes and `test_save.py` ported, plus boundary, reuse and save cases | stacked on P4 |
| P6 Parity and BDD | Done: 25 JSON scenarios (19 agent, 6 environment) give equal normalized results through `parity/run_python.py` and the Rust library (`tests/parity.rs`); comparator mutation tests (`tests/comparator.rs`); Cucumber features (`tests/bdd.rs`) with one Gherkin scenario per parity scenario and a manifest check; full-suite Linux gate (`parity/Dockerfile`) | stacked on P5 |
| P7 Config and CLI | Done: `src/config.rs` (key=value and file specs, lookup and suffix rules, embedded built-ins, flags layer, class registries, `TextbasedModelConfig` with env-backed defaults) and `src/main.rs` (clap, global config dir and `.env` overlay, banner, task from `-t`/`run.task`/stdin, exit codes 0/1/2); typed dumps of both shipped YAML files equal pydantic's; Python `mini` and the Rust CLI write equal trajectories for 4 cases; `cli.feature`. **M1 reached** | stacked on P6 |

Parity is proven at the library level for the scenarios in `parity/scenarios/` (P6) and at the CLI level for the P7 gate cases (`tests/support/cli.rs`). The text-based model and its HTTP adapter are M2 (P8-P9).

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

## Evidence (P0, 2026-11-24)

| Check | Result |
|---|---|
| Python baseline: `uv run --frozen pytest tests/agents/test_default.py tests/environments/test_local.py tests/config/test_init.py tests/utils/test_serialize.py tests/run/test_local.py` in `../mini-swe-agent`, with `MSWEA_SILENT_STARTUP=1` and a temporary `MSWEA_GLOBAL_CONFIG_DIR` | 124 passed (uv 0.13.9, Python 3.15.2, macOS 27.2.1) |
| `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo build --locked`, `cargo test --locked` on macOS | Pass |
| Rust-only Linux gate (`rust:1.99.1` image): fmt, clippy, build, test | Pass |
| `cargo deny check advisories` | No advisories |
| Reference provenance (plan §1.0) | Text path identical to upstream tag `v2.4.6` (commit `a83fcae`) |

## Evidence (P6, 2026-11-24)

| Check | Result |
|---|---|
| `cargo test --locked` on macOS with the reference (all targets, including `parity` and `bdd`) | Pass: 25 of 25 scenarios equal; BDD 25 scenarios, 123 steps |
| Full-suite Linux gate (`parity/Dockerfile`) | Pass (exit 0) |
| Rust-only Linux gate | Pass; `parity` and `bdd` skipped on purpose |
| `cargo deny check advisories` | No advisories, after one recorded exception in `deny.toml`: `humantime` (unmaintained, dev-only through `cucumber`, D9) |

## Usage (M1)

The text-based model arrives in P8; until then the CLI runs the deterministic model:

```bash
cargo run --locked -- --model-class deterministic -m deterministic -c mini_textbased \
  -c 'model.outputs=[{"role":"assistant","content":"x","extra":{"actions":[{"command":"echo COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT"}]}}]' \
  -t "Say hello" -o /tmp/traj.json
```

Exit codes: 0 when the run ends with an exit message (any status, including `LimitsExceeded`), 1 for a fatal or
config error (the trajectory is saved first when the run had started), 2 for usage errors and for the
interactive-only flags `-y/--yolo` and `--exit-immediately` (D7).

**Typed config goldens.** `tests/goldens/config/*.json` are pydantic `model_dump(mode="json")` dumps of both
shipped YAML files (`parity/config_dumps.py`).

## Evidence (P7, 2026-11-24)

| Check | Result |
|---|---|
| `cargo test --locked` on macOS with the reference | Pass; BDD 3 features, 34 scenarios, 151 steps; the 4 CLI gate cases equal |
| Full-suite Linux gate (`parity/Dockerfile`), including the CLI gate against Python `mini` | Pass (exit 0) |
| Rust-only Linux gate | Pass |
| `cargo deny check advisories` | No advisories (with the recorded `deny.toml` exception) |

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
  still render in minijinja's format; no shipped template prints one.
- **LocalEnvironment (P3).** `src/environment/local.rs`. Divergences from Python (plan §2.4): `/bin/bash`
  instead of `/bin/sh` (D11), an inherited `BASH_ENV` is dropped (bash would source it), stdin is `/dev/null`, a
  process group instead of a new session, `child.kill()` after `killpg` (Python hangs if the shell left its
  group), and a 1 s bounded drain after a kill. Error texts follow CPython (`[Errno 2] No such file or directory:
  '/x'`, `Command '…' timed out after N seconds`).
- **Agent loop (P5).** `src/agent/default.rs`. `query` appends and charges the assistant message before
  `step` decodes its actions, as Python does. The deterministic model iterates `extra.actions` before charging, so
  malformed actions fail inside the model. Divergences (plan §2.4): `mini_version` is the crate version,
  `traceback` is the error's `source()` chain, the exit status of a fatal error is a stable name (D6, such as
  `OutputsExhausted` for Python's `IndexError`), the agent's templates compile at construction, and the
  trajectory is written as raw UTF-8 through a temporary file and a rename.
- **Parity harness (P6).** `tests/support/` holds the Rust runner, the test-only recording wrappers (history
  passed to each query, actions passed to `execute`) and the comparator; `parity/run_python.py` is the Python
  runner. The Python side uses a thin `DeterministicModel` subclass that turns the JSON `raise` specs into
  exceptions, and replaces the module-level `GLOBAL_MODEL_STATS` per scenario. Both sides get one explicit
  environment (`PATH`, `HOME`, `LANG=C.UTF-8`, `TMPDIR`, `MSWEA_SILENT_STARTUP`, `MSWEA_CONFIGURED`,
  `MSWEA_GLOBAL_CONFIG_DIR`, plus the scenario's variables) and a fresh workspace at the same absolute path.
  Normalized paths: message and query `extra.timestamp`, the three `*_type` paths, `info.mini_version`, and for
  a fatal exit the exit message's `content`, `exception_str` and `traceback`, with the exit status mapped by D6
  (`IndexError` → `OutputsExhausted`, plus each scenario's own map). Everything else is compared exactly; an
  integer equals a float of the same value, and integers are never compared through `f64`. A scenario also
  fails on unexpected leftover or missing scripted outputs.
- **Config and CLI (P7).** Divergences (plan §2.4): the default config is the embedded `mini_textbased.yaml`
  (Python's is the tool-calling `mini.yaml`); `--agent-class` accepts only `default` and an absent class means
  `default` (Python's CLI default is the interactive agent); `--environment-class` accepts only `local`; with no
  `-t` and no `run.task`, the task is read from stdin to EOF instead of a prompt; the setup wizard never runs; a
  `key=value` value that is not JSON for `serde_json` (`NaN`, `Infinity`) stays a string. Messages go to stderr.
- **Module layout.** The YAML loader is `src/yaml.rs`, used by `config.rs` in P7 (the plan lists only `config.rs`).
