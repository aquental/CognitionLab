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
| P5–P7 | Not started | — |

Parity with Python is **not** proven yet; that is phase P6.

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
- **Module layout.** The YAML loader is `src/yaml.rs`, used by `config.rs` in P7 (the plan lists only `config.rs`).
