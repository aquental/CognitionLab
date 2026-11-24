# P7 findings: adversarial review of config and CLI

- **Date:** 2026-11-24
- **Code reviewed:** commit `8884419` (branch `mini-swe-agent-rs/p7-config-cli`), in a detached worktree. The
  P2–P6 review fixes were merged into P7 afterwards.
- **Prompt:** `review-prompt.md` in this directory. Every reviewer also got the Rust best-practices line; Antigravity
  also got the `rust-dev` plugin line and the no-shell-commands line; Grok got a `--rules` note for headless runs.
- **Raw outputs:** `codex.md`, `grok.md`, `agy.md`, copied byte for byte from the CLI outputs (checked with `cmp`).
- **Runs:** read-only, in parallel, no permission bypass, no retry.

  | Reviewer | Command | Exit | Time |
  |---|---|---|---|
  | Codex | `codex exec -s read-only` | 0 | 201 s |
  | Antigravity | `agy --mode plan -p` | 0 | 448 s |
  | Grok | `grok --permission-mode plan --output-format plain --rules … -p` | 0 | 1159 s |

- **Fixes:** test-first. The new tests in `tests/config.rs` and `tests/cli.rs` failed or did not compile before the
  fix. A sabotage check (a CLI scenario without its `When` step) now fails the BDD run twice: the step reports "the
  CLIs did not run" and the manifest reports "ran 0 times".

Dispositions: **A** = accepted; **P** = partly accepted; **R** = refuted; **→P6** = about P6 code, handled in
`../p6/FINDINGS.md`.

## Codex (11 findings: 1 high, 7 medium, 3 low)

| # | Sev. | Finding | Disp. | Resolution |
|---|---|---|---|---|
| C1 | High | Malformed sections and class selectors become defaults; a non-string `model_name` is replaced from the environment | A | The factories' rules: `agent`/`environment` must be mappings and a present class a non-empty string; a `null` `model` section is `{}` (`get_model(config=None)`), a falsy `model_class` is the default, a truthy non-string one is unknown; a truthy non-string `model_name` is `InvalidValue`, a falsy one falls back to `MSWEA_MODEL_NAME`. Each section is taken out of the merged map once. `sections_and_class_selectors_follow_the_python_factories` |
| C2 | Medium | `run.task: null` falls through to stdin | A | `task()` tells absent from present: absent reads stdin; a string (`""` included) is the task; another type is a config error before stdin. Python renders any value; refusing it is recorded in the README |
| C3 | Medium | A deep dotted spec can overflow the stack | A | `MAX_KEY_DEPTH = 128`, `ConfigError::TooDeep`; library and CLI tests with 100,000 segments |
| C4 | Medium | `--help`/`--version` exit 2; "Saved trajectory" printed after a failure | A | clap's own exit code; the message is printed only after `run` returns `Ok`, as in Python. Tests: help and version, an unwritable destination, a template error before the loop; the default `-o` wins over a YAML `agent.output_path` |
| C5 | Medium | `.env` keeps the first duplicate | A | The file is read into a map (last assignment wins), then overlaid without overriding. `dotenv_last_assignment_wins` |
| C6 | Medium | No invalid-template parity scenario; the Python runner builds the agent outside the outcome capture | →P6 | |
| C7 | Medium | A CLI BDD case passes without running | A | `clis_agree` requires `Some(Ok(()))`; a case is recorded only after it matched; the manifest requires each case exactly once |
| C8 | Medium | Comparator: required fields missing on both sides pass; big integers vs equal floats | →P6 | |
| C9 | Low | Lookup compares raw spellings (`./default.yaml`); `MSWEA_MINI_CONFIG_PATH` is not normalized | A | The spec is normalized as `Path` does (`.` components and extra slashes dropped, `..` kept); an empty spec or override is `EmptySpec`, as `with_suffix` raises. `lookup_normalizes_the_spec_like_path` |
| C10 | Low | `openai_textbased` missing; `cost_tracking`/`set_cache_control` accept any string | A | Alias added (P8 availability error; the unknown-class message lists the aliases); `CostTracking` and `CacheControl` enums. `textbased_literals_are_checked` |
| C11 | Low | BDD lint allowances bypass the error-handling rule | P | `tests/bdd.rs` has no crate-level allows: every step returns `Result` and uses no `unwrap`, `expect`, `panic!` or indexing. `compare_case` reports a missing trajectory with the exit code and stderr. The shared harness in `tests/support/` keeps its documented, module-scoped exemptions: a broken fixture or reference environment is a setup failure |

## Grok (11 findings: 8 medium, 3 low)

| # | Sev. | Finding | Disp. | Resolution |
|---|---|---|---|---|
| G1 | Medium | `--help`/`--version` exit 2 | A | Same as C4 |
| G2 | Medium | "Saved trajectory" can be false | A | Same as C4 |
| G3 | Medium | A present non-string `run.task` reads stdin | A | Same as C2 |
| G4 | Medium | A present non-string `model_name` is replaced from the environment | A | Same as C1 |
| G5 | Medium | `./default` and `foo/../default` miss the built-ins | P | `./default` is found (C9). `foo/../default` is not: `pathlib` keeps `..` and the built-in dir has no `foo/`, so Python's `exists()` is false too (tested) |
| G6 | Medium | `openai_textbased` missing | A | Same as C10 |
| G7 | Medium | The CLI gate can pass without comparing (opt-out; `None` result) | P | `None` now fails (C7). The opt-out stays: `MSWEA_RS_NO_REFERENCE=1` is the explicit switch of the Rust-only gate, it prints that the suite was skipped ("this is not a pass"), and the README never records a skipped suite as passed. Without the switch and without the reference, the harness fails |
| G8 | Medium | `harness = false` target disables the denies | P | Same as C11 |
| G9 | Low | `environment.env` is a `BTreeMap`, so the typed dump sorts keys; the test ignores key order | A | `EnvVars`: an ordered `dict[str, str]` (a repeated name keeps its first position); the typed-dump test compares JSON text |
| G10 | Low | `-l nan`/`-l inf` become `null` | A | `-l` must be finite (usage error, exit 2) |
| G11 | Low | `-o ""` is inserted | A | An empty `-o` is "not given" (`output or UNSET`); clap accepts the empty value. `empty_output_is_unset_and_the_default_beats_yaml` |

## Antigravity (11 findings: 2 high, 6 medium, 3 low)

| # | Sev. | Finding | Disp. | Resolution |
|---|---|---|---|---|
| A1 | High | `--help`/`--version` exit 2 | A | Same as C4 |
| A2 | High | Non-object sections and non-string class, `model_name` and `run.task` values are ignored | A | Same as C1 and C2 |
| A3 | Medium | "Saved trajectory" printed when the save failed or never happened | A | Same as C4 |
| A4 | Medium | `cost_tracking` and `set_cache_control` are stringly typed | A | Same as C10; `MSWEA_COST_TRACKING=""` is now an error, as pydantic's `Literal` check |
| A5 | Medium | Non-finite `--cost-limit` | A | Same as G10 |
| A6 | Medium | `.env` duplicates: first wins | A | Same as C5 |
| A7 | Medium | An empty spec resolves to the cwd | A | Same as C9 |
| A8 | Medium | Clippy exemptions in the BDD target; `compare_case` diagnostics and fragility | P | Same as C11; `Case::task_in_config` replaces the string check. A broken pipe on stdin is not handled specially: both CLIs read stdin only when no task is given, and every gate case gives one |
| A9 | Low | `openai_textbased` missing | A | Same as C10 |
| A10 | Low | `main.rs` flattens errors into `String` | A | `anyhow` in `main.rs` only (plan §3.5), with `{:#}` so the cause chain is printed |
| A11 | Low | `./default` lookup; banner printed after `.env` | A | Lookup: C9. Banner: decided before `.env` is read, as `__init__.py:30-36` does (the plan listed it last). `banner_is_decided_before_the_env_file` |

## Cross-reviewer overlap

| Problem | Reviewers |
|---|---|
| `--help`/`--version` exit code | C4, G1, A1 |
| "Saved trajectory" after a failure | C4, G2, A3 |
| Non-string `run.task` | C2, G3, A2 |
| Sections, classes and `model_name` types | C1, G4, A2 |
| `openai_textbased` alias | C10, G6, A9 |
| BDD lint exemptions | C11, G8, A8 |
| Built-in lookup spelling | C9, G5, A7, A11 |
| `.env` duplicates | C5, A6 |
| Non-finite `-l` | G10, A5 |
| Typed literals | C10, A4 |
| CLI gate can pass without running | C7, G7 |
