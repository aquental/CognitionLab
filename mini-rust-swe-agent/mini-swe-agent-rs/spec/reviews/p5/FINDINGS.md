# P5 findings: adversarial review of the agent loop

- **Date:** 2026-11-24
- **Code reviewed:** commit `b7c6e8c` (branch `mini-swe-agent-rs/p5-agent-loop`, after the P0/P1 review fixes), in a
  detached worktree.
- **Prompt:** `review-prompt.md` in this directory. Every reviewer also got the Rust best-practices line; Antigravity
  also got the `rust-dev` plugin line and the no-shell-commands line; Grok got a `--rules` note for headless runs.
- **Raw outputs:** `codex.md`, `grok.md`, `agy.md`, copied byte for byte (checked with `cmp`). Grok's output starts
  with progress lines; they are kept because the copy must be exactly what the CLI printed.
- **Runs:** read-only, in parallel, no permission bypass, no retry.

  | Reviewer | Command | Exit | Time |
  |---|---|---|---|
  | Antigravity | `agy --mode plan -p` | 0 | 321 s |
  | Codex | `codex exec -s read-only` | 0 | 901 s |
  | Grok | `grok --permission-mode plan --output-format plain --rules … -p` | 0 | 2213 s (one auto-compaction) |

- **Fixes:** test-first. The new tests failed or did not compile before the fix, except
  `trajectory_is_saved_after_every_step` and `cost_equal_to_the_limit_stops_the_run`, which pin behavior that was
  already right and had no test.

Dispositions: **A** = accepted; **P** = partly accepted; **R** = refuted.

## Codex (7 findings: 2 medium, 5 low)

| # | Sev. | Finding | Disp. | Resolution |
|---|---|---|---|---|
| C1 | Medium | `run` drops the exit message's other `extra` keys | A | `ExitInfo::extra` holds the whole `extra`; `exit_status` and `submission` stay as typed fields. `run_returns_the_whole_exit_extra` (a scripted `Submitted` exit with an `artifact` key) |
| C2 | Medium | Without an output path, the whole trajectory is still serialized after every step | A | `save_configured` returns at once when `output_path` is `None` (plan §3.5); `save(None, …)` still serializes for callers. README: a serializer error is then the only observable change |
| C3 | Low | `/sleep` accepts less than Python's `float()` | A | `python_float`: Python whitespace and `_` between digits; NaN and negatives are `ValueError`, lengths too large to sleep (`inf`, `1e309`) are `OverflowError`, as `time.sleep` raises. `scripted_sleep_follows_python_float_and_time_sleep` |
| C4 | Low | One timestamp per observation batch, read before rendering | A | Fixed in P4 (`Observations::messages` reads the `Clock` once per output, after its render); the deterministic model passes its clock |
| C5 | Low | Save tests miss the save after each step and a save failure that replaces a fatal error | A | `trajectory_is_saved_after_every_step` (step 2 reads the file step 1 saved); `save_failure_replaces_a_fatal_error` (an environment that breaks the output directory, then fails) |
| C6 | Low | Every step deep-clones its observations | A | `step` and `execute_actions` return the appended slice (`&[Message]`) |
| C7 | Low | `default.rs` is over the line budget | A | README note: about 350 lines of code, and why |

## Grok (4 findings: 4 low)

| # | Sev. | Finding | Disp. | Resolution |
|---|---|---|---|---|
| G1 | Low | `/sleep` does not follow `float()` and `time.sleep` (separators, `OverflowError`) | A | Same as C3 |
| G2 | Low | `save` skips an empty path | A | The filter is gone; an empty path is `AgentError::Io` (`IsADirectory`), as Python's `Path("")` fails. `an_empty_output_path_is_an_error` |
| G3 | Low | Observations cloned; full serialization without a path | A | Same as C6 and C2 |
| G4 | Low | Over the line budget, not recorded | A | Same as C7 |

## Antigravity (7 findings: 4 medium, 3 low)

| # | Sev. | Finding | Disp. | Resolution |
|---|---|---|---|---|
| A1 | Medium | Serialization on every step without an output path | A | Same as C2 |
| A2 | Medium | `execute_actions` clones the observations | A | Same as C6 |
| A3 | Medium | `save` ignores `Some("")` | A | Same as G2 |
| A4 | Medium | Untested: save after each step, save failure over a fatal error, deterministic error paths, `cost == cost_limit` | A | Same as C5; `scripted_fatal_raise_is_the_exit_status`; the `/sleep` error kinds (C3); `cost_equal_to_the_limit_stops_the_run` |
| A5 | Low | `add_messages` is bypassed by most appends | A | Every append in `DefaultAgent` goes through `add_messages` |
| A6 | Low | Over the line budget, not recorded | A | Same as C7 |
| A7 | Low | Duplicate `tempfile` dev-dependency; an owned `String` in `from_host_env`; `write_atomic` touches the disk before serializing | A | Duplicate removed (the exact `=3.27.0` pin stays); `from_host_env` borrows; `write_atomic` serializes first |

## Cross-reviewer overlap

| Problem | Reviewers |
|---|---|
| Serialization without an output path | C2, G3, A1 |
| Observations cloned each step | C6, G3, A2 |
| Line budget not recorded | C7, G4, A6 |
| `/sleep` parsing | C3, G1 |
| Empty output path | G2, A3 |
| Save tests | C5, A4 |
