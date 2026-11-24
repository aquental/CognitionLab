# P9 findings: adversarial review of the documentation

- **Date:** 2026-11-24
- **Code reviewed:** commit `328be80` (branch `mini-swe-agent-rs/p9-docs`, PR #30), in a detached worktree.
- **Prompt:** `review-prompt.md` in this directory. Every reviewer also got the Rust best-practices line. Antigravity
  also got the `rust-dev` plugin line and the no-shell-commands line. Grok got a `--rules` note for headless runs.
- **Raw outputs:** `codex.md`, `grok.md`, `agy.md`, copied byte for byte from the CLI outputs (checked with `cmp`).
  `grok.md` starts with four progress lines, which are part of the CLI output.
- **Runs:** read-only, in parallel, no permission bypass, no retry.

  | Reviewer | Command | Exit | Time |
  |---|---|---|---|
  | Codex | `codex exec -s read-only` | 0 | 249 s |
  | Antigravity | `agy --mode plan -p` | 0 | 374 s |
  | Grok | `grok --permission-mode plan --output-format plain --rules … -p` | 0 | 1478 s |

- **Process note.** Grok's review was not fully independent. The Codex and Antigravity outputs were copied into this
  directory while Grok was still running, and Grok saw them: its finding G8 names `agy.md` and `codex.md`. Its
  findings are its own, but some may have been prompted by what it read. From now on, outputs are copied only after
  every run has finished.
- **Fixes:** test-first.
  - Product fixes (P8 code): an explicit `null` provider role, and an ignored `multimodal_regex`. The new tests
    failed before the fix.
  - New evidence tests for divergences that had none or had the wrong one:
    - `timeout_returns_while_an_escaped_grandchild_holds_the_pipe`;
    - `fatal_traceback_is_the_error_source_chain`;
    - `lax_coercion_is_refused`.
  - The rewritten group test asserts that the shell's process really left its group before the deadline. Without
    `child.kill()` it hangs for 30 s.
  - `tests/docs.rs` was rewritten. Three sabotage checks now fail it:
    - a changed Python cell in the divergence table;
    - a cited test name that does not exist;
    - a broken `#anchor` in `../README.md`.
- **Gates after the fixes:** all pass.
  - `cargo test --locked` on macOS with the reference: 275 libtest tests and 1 ignored; BDD 36 scenarios and 163
    steps.
  - The full-suite Linux gate.
  - The Rust-only Linux gate. The two process-group tests run there too, with the image's `perl`.
  - clippy `-D warnings` on all targets.
  - `cargo deny check advisories`.

Dispositions:
- **A** = accepted.
- **P** = partly accepted.
- **R** = refuted.

## Codex (9 findings: 6 medium, 3 low)

| # | Sev. | Finding | Disp. | Resolution |
|---|---|---|---|---|
| C1 | Medium | The parity evidence is named but unlinked, and the real-endpoint command is incomplete | A | The evidence list is now Markdown links to the scenarios, runners, comparator, BDD, CLI cases, differential, goldens and reviews. `readme_links_the_parity_evidence` requires them, and `crate_readme_references_exist` resolves every link. The summary row gives the full command (`OPENAI_API_KEY`, `MSWEA_RS_REAL_MODEL`, `MSWEA_RS_REAL_REGISTRY`) and links the P8 findings |
| C2 | Medium | The subprocess divergence describes the wrong Python behavior and cites tests for a different case | A | Verified: Python's shell is a session leader (`start_new_session=True`, `local.py:84`), so it cannot change group. The §2.4 row now says "Cannot happen" for Python; the Rust cell, §3.7 and the P3 note say that `child.kill()` covers the port's group-leader shell. The escaped-descendant row now cites `timeout_returns_while_an_escaped_grandchild_holds_the_pipe`. In that test, a `perl` grandchild moves to its own group and keeps stdout open. The call returns after the timeout plus the 1 s grace period (between 2 s and 8 s) with the partial output, and the grandchild is still alive, as with Python's `killpg` |
| C3 | Medium | The new timeout test can pass before the process changes groups | P | The `perl` now prints `moved to <pgid>` (autoflushed) only after `setpgid` succeeds. The test asserts that group differs from the shell's pid, and also asserts `TimeoutExpired`, the 10 s bound and a dead process. The timeout is 2 s. `perl` is required and checked first, with a clear message; it is on macOS and in the gates' `rust` image, and bash cannot change a group by itself. No separate supervisor: the command ends by itself after 30 s, and the elapsed time is asserted |
| C4 | Medium | An explicit null provider role can be billed and then fail outside retry | A | P8 bug. A `null` role now becomes `assistant`, as a missing one does (LiteLLM's default), before the message is built. Test: `a_null_role_means_assistant_and_is_billed_once`, which failed with `Internal("… invalid type: null …")` before the fix |
| C5 | Medium | The documentation checks accept missing outcomes, evidence links, and crate paths | A | `tests/docs.rs` now checks the following. Divergences: each README row repeats the plan's first three cells exactly, the row counts match, and every test name, scenario id and path cited under "Checked by" exists. Decisions: four cells, with the decision and options equal to the plan's. Status: each phase is "Done" with its PR number. Evidence: links. Every link (with its anchor) and every crate path in the README, plus plain-text crate paths in the parent documents, must exist, with a minimum count so a check cannot pass with nothing in it. The table parser drops only the header and separator rows and borrows the cells |
| C6 | Medium | Usage gives misleading instructions for cost tracking and trajectory saves | A | The `hello_world` command sets `OPENAI_API_KEY` and `MSWEA_COST_TRACKING=ignore_errors` before `cargo run`. The exit-code text now says that 1 covers a config error, a fatal error or a failed save. It also says the CLI always has an output path and saves after every step, so a fatal error is in the trajectory unless the save is what failed |
| C7 | Low | D8 omits child survival, and the provider notes omit an accepted-but-ignored setting | A | D8's outcome now says that a command still running at Ctrl-C keeps running, as in Python. `multimodal_regex` was accepted and ignored by both models. Now a non-empty value is refused when the model is built (`a_multimodal_regex_is_refused` in `tests/textbased.rs` and `tests/agent.rs`), and the README and plan §2.3 record it |
| C8 | Low | Several evidence labels overstate their assertions | A | The type path in §2.4 and the README is now `mini_swe_agent_rs::agent::default::DefaultAgent`, plus `std::any::type_name` for the model and the environment. The traceback row cites the new `fatal_traceback_is_the_error_source_chain` (it starts with `exception_str` and has a `caused by:` line). The `extra.response` row names the exact projection. The coercion row cites the new `lax_coercion_is_refused` (`"3"` for an integer, `"1.5"` for a float, `3` for a string) |
| C9 | Low | The status and aggregate test count need reconciliation | A | The summary separates the libtest harnesses (275 passed, 1 ignored, no doc tests) from the BDD runner (3 features, 36 scenarios, 163 steps). The "every phase was reviewed" claims are true with this file. Plan §6.1 now covers P0–P9 and names what P9 added beyond documentation |

## Grok (12 findings: 10 medium, 2 low)

| # | Sev. | Finding | Disp. | Resolution |
|---|---|---|---|---|
| G1 | Medium | The hang row cites tests that never let a grandchild leave the process group | A | As C2 |
| G2 | Medium | `fatal_exit_status_names_are_stable` does not check `extra.traceback` | A | As C8 |
| G3 | Medium | The type-name example is not the string the agent writes | A | As C8; corrected in the plan and the README |
| G4 | Medium | `textbased_literals_are_checked` does not prove the coercion half of the row | A | As C8 (`lax_coercion_is_refused`); the boolean half cites `yaml_1_1_booleans_stay_strings` |
| G5 | Medium | The `hello_world` command is not runnable as written | A | As C6 |
| G6 | Medium | `timeout_kills_a_shell_that_left_its_process_group` never checks that the shell left its group | A | As C3 |
| G7 | Medium | The done-criteria tests can pass while the README is wrong | A | As C5. Also: duplicate rows are no longer folded, a blank row is no longer dropped, anchors are checked, and the stale-phrase list includes "in later stages" and "will have its own strict policy" |
| G8 | Medium | The status text says P9 was already reviewed and that P9 touched only docs | A | As C9 |
| G9 | Medium | The parent README still says the implementation is future work | A | Both sentences rewritten. The parity-policy sentence now links the comparator and says what it normalizes. Both phrases are in the stale list |
| G10 | Medium | The README records two module overruns and omits the library budget | A | New design note "Size (plan §2.6)": about 3,400 lines of code (non-blank lines without comments, tests and `main.rs`) against 1,800, with the causes per module. The `local.rs` and `agent/default.rs` notes now use the same count (about 460 and 310) |
| G11 | Low | The proxy sentence lists only the uppercase variables | A | The sentence names both cases, the order, and `NO_PROXY`/`no_proxy` |
| G12 | Low | "267 tests in 23 targets (plus doc tests)" does not match the crate layout | A | As C9 |

## Antigravity (8 findings: 2 high, 4 medium, 2 low)

| # | Sev. | Finding | Disp. | Resolution |
|---|---|---|---|---|
| A1 | High | The divergence table cites tests that do not prove the claimed behavior | A | As C2 (escaped grandchild) and C8 (traceback, coercion), each with a new test |
| A2 | High | `tests/docs.rs` path and link checks pass vacuously for `../spec/prompt/migrate.md` and skip nearly all paths in the crate README | A | Plain-text crate paths (`mini-swe-agent-rs/…`) are now extracted from the parent documents, so `migrate.md`'s two paths are checked. Backtick paths under `src/`, `tests/`, `parity/`, `examples/`, `config/` and `spec/` are resolved in the crate README. Each parent document must yield at least one checked reference, and the README at least 40. Paths with globs or placeholders are skipped, and so are the two removed plan files that D2 names on purpose |
| A3 | Medium | `timeout_kills_a_shell_that_left_its_process_group` can pass vacuously under load and depends on `perl` | P | As C3 |
| A4 | Medium | The whole-library budget (≤ 1,800 lines) is exceeded without the required README explanation | A | As G10 |
| A5 | Medium | The `hello_world` and `real_endpoint` commands in the README fail if run as written | A | As C6 and C1 |
| A6 | Medium | Contradictory claims about P9's scope and review coverage | A | As C9 |
| A7 | Low | Stale list of skipped Rust-only gate targets, and a contradictory `set_cache_control` scope note | A | The README lists every check that is skipped on purpose without the reference or the parent directory. Plan §2.3 now says the Anthropic `set_cache_control` default is kept in the config, with no markers sent, and that a non-empty `multimodal_regex` is refused |
| A8 | Low | Lint hygiene, fenced-block brittleness, and an unnecessary skip in `tests/docs.rs` | A | The unused `unwrap_used` allow is gone. `section()` ignores headings inside fenced code blocks. The plan staleness check is its own crate-local test (`the_plan_status_is_current`), so it runs in the Rust-only gate |
