### Verdict

Phase P7 delivers a clean, well-structured config and CLI layer with zero panics in `main.rs` and strong parity coverage against Python's `mini` CLI, but it has **two high-severity bugs** (`--help`/`--version` exiting with code 2, and non-object sections / non-string `*_class` and `run.task` values being silently ignored) plus several medium-severity issues in config validation, `.env` overlay precedence, trajectory-saved reporting, and test/clippy discipline in the `harness = false` BDD target. Address the high and medium items before treating M1 as complete.

---

### Findings

1. **[High] `--help` and `--version` exit with code 2 instead of 0**
   - **Location:** [src/main.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p7/mini-rust-swe-agent/mini-swe-agent-rs/src/main.rs#L78-L84)
   - **Problem:** `Cli::try_parse()` returns `Err(error)` not only for usage errors, but also for `-h`/`--help` (`ErrorKind::DisplayHelp`) and `-V`/`--version` (`ErrorKind::DisplayVersion`), for which `clap` sets `error.exit_code() == 0`. `main()` unconditionally returns `ExitCode::from(2)` on every `Err(error)`, breaking standard CLI behavior.
   - **Evidence:**
     ```rust
     let cli = match Cli::try_parse() {
         Ok(cli) => cli,
         Err(error) => {
             let _ = error.print();
             return ExitCode::from(2);
         }
     };
     ```
     Neither [tests/cli.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p7/mini-rust-swe-agent/mini-swe-agent-rs/tests/cli.rs) nor [tests/features/cli.feature](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p7/mini-rust-swe-agent/mini-swe-agent-rs/tests/features/cli.feature) tests `--help` or `--version`.
   - **Recommended fix:** Return `ExitCode::from(u8::try_from(error.exit_code()).unwrap_or(2))` (or call `error.exit()`), and add tests in `tests/cli.rs` asserting exit code `0` for `--help` and `--version`.

2. **[High] Non-object config sections and non-string `*_class` / `model_name` / `run.task` values are silently ignored**
   - **Location:** [src/config.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p7/mini-rust-swe-agent/mini-swe-agent-rs/src/config.rs#L301-L317), [src/config.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p7/mini-rust-swe-agent/mini-swe-agent-rs/src/config.rs#L367-L377), [src/config.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p7/mini-rust-swe-agent/mini-swe-agent-rs/src/config.rs#L455-L456)
   - **Problem:** Four places silently swallow wrong JSON types instead of failing with a config error:
     1. `take_class` (`src/config.rs:311`) and `TextbasedModelConfig::from_section` (`src/config.rs:456`) do `section.as_object().cloned().unwrap_or_default()`. Passing `-c environment=42` or `-c environment=null` silently produces an empty `{}` and succeeds with default `LocalEnvironmentConfig`, whereas Python fails (`environments/__init__.py:32`, `agents/__init__.py:27`).
     2. `take_class` (`src/config.rs:312-315`) does `.and_then(|v| v.as_str().map(str::to_owned))`. Passing `-c agent.agent_class=123` or `-c model.model_class=true` (parsed as JSON number/bool by `key_value_spec`) removes the key, returns `None`, and silently selects the default class, whereas Python raises `ValueError: Unknown ...` (`agents/__init__.py:15-22`, `environments/__init__.py:20-27`, `models/__init__.py:100-108`).
     3. `config::task` (`src/config.rs:301-307`) uses `.and_then(Value::as_str)`. Passing `-c run.task=42` (e.g. an issue number, parsed as JSON `42`) returns `None`, causing `main.rs:148-159` to block waiting for stdin instead of using the config value or reporting a type error (`run/mini.py:94` checks `is UNSET`).
     4. `model_choice` (`src/config.rs:367-377`) ignores a non-string `model.model_name` and silently falls back to `MSWEA_MODEL_NAME` or `NoModelName`.
   - **Recommended fix:** Return a `ConfigError` when a section is not a `Value::Object`, or when `*_class`, `model.model_name`, or `run.task` is present with a non-string JSON value (treating `null` on `model_class` as absent if matching Python's falsy check in `models/__init__.py:100`).

3. **[Medium] `main.rs` prints `Saved trajectory to '...'` even when saving failed or never happened**
   - **Location:** [src/main.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p7/mini-rust-swe-agent/mini-swe-agent-rs/src/main.rs#L173-L188)
   - **Problem:** `say(&format!("Saved trajectory to '{}'", path.display()))` runs unconditionally after `agent.run(...)`, regardless of whether `result` is `Ok` or `Err`. If `agent.run` fails before the loop (e.g., a pre-loop template render error, which never saves per plan §1.3 item 2 / §3.5) or if `save()` itself fails with an `Io` error (e.g., unwritable path), the CLI still prints `Saved trajectory to '...'` right before `error: ...`.
   - **Evidence:** Python (`mini-swe-agent/src/minisweagent/run/mini.py:102-104`) only prints `"Saved trajectory to ..."` after `agent.run(run_task)` returns normally.
   - **Recommended fix:** Print `Saved trajectory to '...'` only when `result.is_ok()` (or, if you want to report it on fatal in-loop errors as well, only when `!matches!(result, Err(AgentError::Io { .. }))` and `path.is_file()`).

4. **[Medium] `TextbasedModelConfig` leaves `cost_tracking` and `set_cache_control` stringly-typed**
   - **Location:** [src/config.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p7/mini-rust-swe-agent/mini-swe-agent-rs/src/config.rs#L433-L437), [src/config.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p7/mini-rust-swe-agent/mini-swe-agent-rs/src/config.rs#L455-L471)
   - **Problem:** Python's `LitellmModelConfig` (`mini-swe-agent/src/minisweagent/models/litellm_model.py:34-36`) constrains these fields with `Literal["default_end"] | None` and `Literal["default", "ignore_errors"]`. In Rust, both are plain `String` / `Option<String>`, so invalid values (including `MSWEA_COST_TRACKING=""` or `cost_tracking: "typo"`) silently pass deserialization. This also goes against plan §3.16 (`type-no-stringly`) and Rust type-driven design best practices.
   - **Recommended fix:** Replace `String` with `#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)] #[serde(rename_all = "snake_case")]` enums (`CostTracking { Default, IgnoreErrors }` and `CacheControl { DefaultEnd }`).

5. **[Medium] Non-finite `--cost-limit` (`NaN`, `inf`) silently turns into `null` in `Overrides::layer`**
   - **Location:** [src/config.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p7/mini-rust-swe-agent/mini-swe-agent-rs/src/config.rs#L247-L249), [src/main.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p7/mini-rust-swe-agent/mini-swe-agent-rs/src/main.rs#L47-L48)
   - **Problem:** `clap` parses `f64` via `str::parse`, which accepts `"NaN"`, `"inf"`, and `"-inf"`. In `Overrides::layer`, `Value::from(limit)` converts any non-finite `f64` into `Value::Null`, overwriting the YAML `cost_limit` with `null` and causing a misleading deserialization error (`invalid 'agent' config: invalid type: null, expected f64`).
   - **Recommended fix:** Add a `value_parser` on `Cli::cost_limit` that rejects non-finite floats (exiting with code 2 and a clear message), and validate finiteness in `Overrides` if called from library code.

6. **[Medium] `HostEnv::overlay_missing` uses first-wins instead of last-wins for duplicate keys in `.env`**
   - **Location:** [src/host_env.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p7/mini-rust-swe-agent/mini-swe-agent-rs/src/host_env.rs#L35-L41), [src/main.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p7/mini-rust-swe-agent/mini-swe-agent-rs/src/main.rs#L105-L111)
   - **Problem:** `dotenvy::from_path_iter` yields duplicate keys from `.env` in file order. `overlay_missing` inserts directly into `self.vars` with `.entry(...).or_insert_with(...)`, so the first line in `.env` inserts the key and any later redefinition in the same `.env` file is ignored. In Python, `dotenv.load_dotenv` parses `.env` into a dict (last definition wins) and only skips keys already present in `os.environ` before loading.
   - **Recommended fix:** Collect the `.env` pairs into a `BTreeMap<String, String>` (last-wins) before calling `host.overlay_missing(...)`.

7. **[Medium] Empty config spec (`-c ""` or `MSWEA_MINI_CONFIG_PATH=""`) resolves to `cwd` and fails with `Is a directory`**
   - **Location:** [src/config.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p7/mini-rust-swe-agent/mini-swe-agent-rs/src/config.rs#L131-L154), [src/config.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p7/mini-rust-swe-agent/mini-swe-agent-rs/src/config.rs#L289-L294)
   - **Problem:** `PathBuf::from("").with_extension("yaml")` is a no-op returning `""`. Then `cwd.join("")` equals `cwd`, `absolute.exists()` is `true` because `cwd` is a directory, and `read_source` fails with `EISDIR` (`Is a directory`). In Python (`config/__init__.py:16`), `Path("").with_suffix(".yaml")` immediately fails (`ValueError: PurePosixPath('.') has an empty name`).
   - **Recommended fix:** Reject empty file specs before lookup (returning `ConfigError::NotFound` or a dedicated invalid-spec error), and consider checking `absolute.is_file()` or rejecting directories cleanly.

8. **[Medium] Clippy exemptions in `tests/bdd.rs` and `tests/support/` violate plan §3.5, and `tests/support/cli.rs` has diagnostic and fragility gaps**
   - **Location:** [tests/bdd.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p7/mini-rust-swe-agent/mini-swe-agent-rs/tests/bdd.rs#L8-L14), [tests/bdd.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p7/mini-rust-swe-agent/mini-swe-agent-rs/tests/bdd.rs#L155-L161), [tests/support/cli.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p7/mini-rust-swe-agent/mini-swe-agent-rs/tests/support/cli.rs#L24), [tests/support/cli.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p7/mini-rust-swe-agent/mini-swe-agent-rs/tests/support/cli.rs#L64-L67), [tests/support/cli.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p7/mini-rust-swe-agent/mini-swe-agent-rs/tests/support/cli.rs#L165-L183)
   - **Problem:**
     - Plan §3.5 explicitly requires: *"`tests/bdd.rs` has `harness = false`, so `cfg(test)` is not set there: its step functions return `Result` and use no `unwrap`, `expect` or indexing, and shared helpers in `tests/support/` follow the same rule."* Instead, `tests/bdd.rs:9-14` and `tests/support/mod.rs:11-17` add blanket `#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]`.
     - In `tests/bdd.rs:155-161`, `clis_agree` writes `if let Some(Err(problem)) = &world.cli_result`, which silently passes if `world.cli_result` is `None`.
     - In `tests/support/cli.rs:173-183`, `sandbox.trajectory("traj.json")` is read immediately via `.unwrap()` before checking exit codes or reporting `stderr`, so if either CLI fails before writing a trajectory, the test panics with `ENOENT` and hides the actual CLI error output.
     - In `tests/support/cli.rs:165-170`, deciding whether to pass `-t` checks `case.extra.contains(&"run.task=Read the task from the config")` — a hardcoded string that will silently break if another `run.task=...` case is added or edited.
     - In `tests/support/cli.rs:64-66`, `input.write_all(...).unwrap()` will panic with `BrokenPipe` if non-empty `stdin` is passed to a command that exits before reading stdin.
   - **Recommended fix:** Have `tests/bdd.rs` steps return `Result<(), String>` and remove the blanket `#![allow(...)]` (and match `Some(Ok(()))` in `clis_agree`); in `compare_case`, check exit codes and include `stderr` before reading `traj.json`, replace the hardcoded string check with `task: Option<&'static str>` on `Case`, and ignore `io::ErrorKind::BrokenPipe` on `stdin.write_all`.

9. **[Low] Missing `"openai_textbased"` model class alias specified in plan §3.8**
   - **Location:** [src/config.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p7/mini-rust-swe-agent/mini-swe-agent-rs/src/config.rs#L383-L390)
   - **Problem:** Plan §3.8 specifies the model class registry as `"deterministic"` and `"textbased"` with aliases `"litellm_textbased"` and `"openai_textbased"`. `model_choice` omits `"openai_textbased"`, returning `ConfigError::UnknownClass` instead of `ConfigError::NotYetAvailable`.
   - **Recommended fix:** Add `"openai_textbased"` to the `NotYetAvailable` match arm in `src/config.rs:384-388`.

10. **[Low] `main.rs` flattens all errors into `Result<(), String>` instead of using `anyhow::Result`**
    - **Location:** [src/main.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p7/mini-rust-swe-agent/mini-swe-agent-rs/src/main.rs#L100-L189)
    - **Problem:** `run()` uses `Result<(), String>` with 13 manual `.map_err(...)` conversions, discarding error source chains. Plan §3.5, §3.14, §3.16 and Rust CLI error-handling best practices specify `anyhow::Result` in `main.rs` with `.with_context(...)`.
    - **Recommended fix:** Either adopt `anyhow` in `main.rs` (formatting errors with `{err:#}` to preserve cause chains) or document the intentional omission of `anyhow` in the README/plan.

11. **[Low] Minor lookup/startup edge cases: `./default` builtin lookup and `.env` vs banner ordering**
    - **Location:** [src/config.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p7/mini-rust-swe-agent/mini-swe-agent-rs/src/config.rs#L155-L158), [src/main.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p7/mini-rust-swe-agent/mini-swe-agent-rs/src/main.rs#L105-L118)
    - **Problem:**
      - `config_path("./default", ...)` compares `BUILTIN_CONFIGS` against `"./default.yaml"` without normalizing leading `./`, whereas Python's `builtin_config_dir / Path("./default.yaml")` (`config/__init__.py:20`) resolves to `default.yaml`.
      - `main.rs:105-118` loads `.env` *before* checking `MSWEA_SILENT_STARTUP` for the banner, whereas Python (`mini-swe-agent/src/minisweagent/__init__.py:30-36`) prints the banner before `dotenv.load_dotenv`. (This matches plan §3.12, so it only needs a note if strict parity is intended.)
    - **Recommended fix:** Strip leading `./` (or normalize relative components) before checking `BUILTIN_CONFIGS` in `config_path`.
