I'll review P9 against the plan's done criteria and check every documentation claim against the code, tests, and Python reference.

The review is ready to write, but I am rechecking the cited lines so each finding points at the code that is actually there.

The README claims are in place. Next I am checking the tests and source those claims cite.

The line-budget claim needs a fresh count of the library, separate from the two modules the README already explains.

P9 meets the structural done criteria: every §2.4 row name and every D1–D12 outcome is in the crate README, and the scenario, step, golden, and findings counts match the files. Several "Checked by" citations do not prove the rows they name, the new process-group test can pass without observing that the shell left its group, and `tests/docs.rs` only compares labels, outcome length, and substrings, so those mistakes stay green. Status lines say every phase was already reviewed and that P9 changed only documentation, and the library-wide line-budget overrun is not recorded. No critical or high finding.

1. **Medium.** `mini-swe-agent-rs/README.md:166`. The hang row cites tests that never let a grandchild leave the process group.

The row says a grandchild that escaped the group makes Python's `communicate()` block, and that Rust returns partial output after a bounded grace period, checked by `background_child_with_redirected_output_survives` and `child_that_closes_stdout_and_keeps_running_is_killed`.

`background_child_with_redirected_output_survives` (`tests/local_env.rs:468-490`) expects return code 0 and a living redirected child. `child_that_closes_stdout_and_keeps_running_is_killed` (`tests/local_env.rs:494-505`) backgrounds `sleep 30` in the same group and `wait`s. The same-group pipe holder is `background_child_holding_the_pipe_is_killed_at_the_deadline` (`tests/local_env.rs:442-464`), and it is not cited. Python's second `communicate()` after `killpg` has no timeout (`mini-swe-agent/src/minisweagent/environments/local.py:89-90`). Rust always drains for `GRACE` (`src/environment/local.rs:43`, `src/environment/local.rs:442-444`). Nothing in the suite starts a grandchild, moves it to another group, and holds the pipe.

Cite a test that does that, and assert the call returns within the timeout plus the 1-second grace with partial output, and that the grandchild's process-group id differed from the shell's.

2. **Medium.** `mini-swe-agent-rs/README.md:162`. `fatal_exit_status_names_are_stable` does not check `extra.traceback`.

The row says the traceback is the error's `source()` chain and names that test. `fatal_exit_status_names_are_stable` (`tests/core_types.rs:307-329`) asserts only `exit_status()` strings. `outputs_exhausted_is_fatal_and_saved` (`tests/agent.rs:677`) asserts that the traceback key is present. The text is built by `source_chain` (`src/agent/default.rs:232`, `src/agent/default.rs:351-359`), which appends `caused by:` lines. The comparator drops fatal traceback text (`tests/support/compare.rs` normalizes it away).

Assert the `caused by:` chain on a fatal error that has a `source()`, and cite that test.

3. **Medium.** `mini-swe-agent-rs/README.md:158`. The type-name example is not the string the agent writes.

The Rust cell says `` `type_name()` strings such as `mini_swe_agent_rs::DefaultAgent` ``. The agent type is the constant `concat!(module_path!(), "::DefaultAgent")` (`src/agent/default.rs:26`), which is `mini_swe_agent_rs::agent::default::DefaultAgent`. The comparator's authorized fixture uses that full path (`tests/comparator.rs:161-162`). `Model::type_name` and `Environment::type_name` are `std::any::type_name::<Self>()` (`src/model.rs:26-28`, `src/environment.rs:22-24`), so a text-model type includes the module path and the backend type parameter. The same shortened example is in the plan (`spec/migration-plan.md:317`). `tests/docs.rs` compares only the first cell of each row, so the wrong example stays green. `save_includes_types_and_extra_dicts` (`tests/agent.rs:910-914`) only checks that the string contains `DefaultAgent`.

Put the real agent string in the README and in §2.4, and show one `type_name()` value for the model and the environment.

4. **Medium.** `mini-swe-agent-rs/README.md:169`. `textbased_literals_are_checked` does not prove the coercion half of the row.

The row says YAML 1.1 booleans stay strings and that strict serde rejects pydantic's `"3"` → `3`, checked by `tests/yaml.rs` and `textbased_literals_are_checked`. `yaml_1_1_booleans_stay_strings` (`tests/yaml.rs:59-66`) does pin `yes`/`no`/`on`/`off` as strings. `textbased_literals_are_checked` (`tests/config.rs:571-589`) rejects the enum typos `cost_tracking: typo` and `set_cache_control: x`. No test feeds a config integer the string `"3"`.

Keep the YAML citation. Add a test that an integer field rejects `"3"`, or narrow the "Checked by" cell to the boolean behavior that is tested.

5. **Medium.** `mini-swe-agent-rs/README.md:45`. The `hello_world` command is not runnable as written.

```bash
cargo run --example hello_world -- -m gpt-4o-mini -t "List the files"   # MSWEA_COST_TRACKING=ignore_errors
```

The variable sits in a shell comment. `TextbasedModel::cost` returns an error unless `CostTracking::IgnoreErrors` is set or a registry prices the model (`src/model/textbased.rs:84-100`). No prices are bundled. The example's own comment exports the variable (`examples/hello_world.rs:6-7`). The parent README passes `-c model.cost_tracking=ignore_errors` (`README.md:182`). The paragraph under the fence (`mini-swe-agent-rs/README.md:48-49`) explains the requirement. The command a reader copies still fails.

Export `MSWEA_COST_TRACKING=ignore_errors` in the command, as the example's doc comment does.

6. **Medium.** `mini-swe-agent-rs/tests/local_env.rs:829-851`. `timeout_kills_a_shell_that_left_its_process_group` never checks that the shell left its group.

The test writes `$$`, then `exec`s Perl to `POSIX::setpgid(0, getpgrp(getppid()))`, and asserts elapsed time under 10 seconds, return code `-1`, no `setpgid:` in the output, and a dead pid. A timeout before `setpgid` still satisfies every assertion: `kill_and_reap` always calls `killpg` on the original pid and then `child.kill()` (`src/environment/local.rs:423-432`), so a dead pid does not show that `killpg` missed. The command needs `perl -MPOSIX`. On a host with bash and no Perl the assertion on return code `-1` fails the suite. There is no skip, and there is no record of the new process-group id.

Write the post-`setpgid` process-group id to a file and assert it differs from `$$` and matches the intended parent group before checking that the process died. Keep the 10-second bound.

7. **Medium.** `mini-swe-agent-rs/tests/docs.rs:60-89`, `tests/docs.rs:100-137`, and `tests/docs.rs:157-241`. The done-criteria tests can pass while the README is wrong.

- `readme_lists_every_divergence_of_the_plan` (`tests/docs.rs:79-89`) diffs the first cells only. A wrong Python cell, Rust cell, or citation still passes. Extra README rows are ignored. `planned.len() >= 20` does not lock the 21 rows in §2.4. Dropping a plan row shrinks the required set.
- `readme_records_every_decision_and_its_outcome` (`tests/docs.rs:105-108`) accepts any outcome longer than 10 characters.
- `readme_status_covers_every_phase` (`tests/docs.rs:126-137`) uses `contains` on four path substrings. Those files are not checked to exist, and the substrings are not links.
- `crate_paths` (`tests/docs.rs:164-168`) keeps a backtick token only when it starts with `mini-swe-agent-rs/`. Relative paths in the crate README are never resolved.
- `parent_documents_match_the_crate` (`tests/docs.rs:218-241`) checks markdown link targets and a fixed stale-phrase list. `](` fragments are stripped (`tests/docs.rs:148`), so a broken anchor passes. The phrase "in later stages" is not on the list.
- `table_rows` (`tests/docs.rs:60-64`) treats a row as a separator when every cell is only `-` and `:`. An empty cell makes `chars().all(...)` true, so a blank body row disappears. `first_cells` folds duplicate labels through a `BTreeSet`.

The `#[allow(clippy::unwrap_used, clippy::panic)]` helpers match the crate rule that only `#[test]` functions are exempt. The gap is the assertions.

Compare all four cells of each §2.4 row, compare each decision outcome to a fixture or to a substantial expected sentence, and resolve every evidence path the README names. Add the stale sentences from findings 8 and 9 to the parent-document check.

8. **Medium.** `mini-swe-agent-rs/spec/migration-plan.md:6` and `mini-swe-agent-rs/spec/migration-plan.md:1007`. The status text says P9 was already reviewed and that P9 touched only docs.

Line 6 says each phase's review is in `reviews/pN/FINDINGS.md`. The crate README repeats that for all ten phases (`mini-swe-agent-rs/README.md:9-11`). The parent README does too (`README.md:5-8`). §6.1 is titled "Implementation reviews (P0–P8)" (`spec/migration-plan.md:1005`) and says "P9 changes only documentation and the checks in `tests/docs.rs`" (`spec/migration-plan.md:1007`). P9 also added `timeout_kills_a_shell_that_left_its_process_group`. `spec/reviews/p9/` contains `agy.md`, `codex.md`, and `review-prompt.md`, and no `FINDINGS.md`.

Say the reviews on record are P0–P8. Mention the new `local_env` test in §6.1. Point the "each phase was reviewed" sentences at P0–P8 until a P9 `FINDINGS.md` exists.

9. **Medium.** `README.md:12`. The parent README still says the implementation is future work.

Lines 5–8 say the port is implemented and that all ten phases are done. Line 12 says the study contributes the analysis, the design, and, "in later stages, the Rust implementation and its validation." `tests/docs.rs:229-235` does not list that phrase. Line 160 still says the parity suite "will have" its own policy. That sentence is the smaller stale claim.

Rewrite line 12 so it matches the status block. Update line 160 to point at the comparator that exists.

10. **Medium.** `mini-swe-agent-rs/spec/migration-plan.md:357-359`. The README records two module overruns and omits the library budget.

§2.6 sets the library at 1,800 lines, excluding tests and doc comments, and says a module that goes over must have its cause written in the README. The design notes explain `local.rs` (about 430 versus 250) and `agent/default.rs` (about 350 versus 250) (`mini-swe-agent-rs/README.md:299-302`, `mini-swe-agent-rs/README.md:316-319`). Across the 22 files under `src/` other than `main.rs`, after dropping `#[cfg(test)]` bodies and doc comments, the count is 3,407 lines of code, 378 blank lines, and 34 other comments: 3,819 lines against a budget of 1,800.

Add the library total and the cause, in the same place as the two module notes.

11. **Low.** `mini-swe-agent-rs/README.md:35-36`. The proxy sentence lists only the uppercase variables.

The code reads `ALL_PROXY`, `all_proxy`, `HTTPS_PROXY`, `https_proxy`, `HTTP_PROXY`, and `http_proxy` (`src/model/openai_compat.rs:30-37`), and it honors `NO_PROXY` and `no_proxy` (`src/model/openai_compat.rs:417`). The documented set is a subset of the set the code uses.

Name the lowercase variables and `no_proxy` in that sentence.

12. **Low.** `mini-swe-agent-rs/README.md:211`. "267 tests in 23 targets (plus doc tests)" does not match the crate layout.

`tests/` has 19 integration-test binaries (`bdd` is `harness = false` in `Cargo.toml:51-53`). `cargo test` adds the library harness and, when the binary has a harness, `src/main.rs`. That is 20 or 21 summaries. Doc-tests are already set aside by the parenthetical. Examples are not part of `cargo test` as written. There is one example, `examples/hello_world.rs`. The "267" and "1 ignored" figures agree with a count of `#[test]` functions and the single `#[ignore]` on `tests/real_endpoint.rs`. This suite was not re-run here: the crate pins Rust 1.99.1, and a fetch was out of scope.

State the target count from `cargo test --locked -- --list`, or name the harnesses.

The historical "124 pytest passed" figure, the Linux-gate exit codes, and the P8 OpenAI run were not re-executed.
