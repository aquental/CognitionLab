# P3 findings: adversarial review of LocalEnvironment

- **Date:** 2026-11-24
- **Code reviewed:** commit `93c01ab` (branch `mini-swe-agent-rs/p3-local-environment`, after the P0/P1 review
  fixes), in a detached worktree.
- **Prompt:** `review-prompt.md` in this directory. Every reviewer also got the Rust best-practices line; Antigravity
  also got the `rust-dev` plugin line and the no-shell-commands line; Grok got a `--rules` note for headless runs.
- **Raw outputs:** `codex.md`, `grok.md`, `agy.md`, copied byte for byte (checked with `cmp`).
- **Runs:** read-only, in parallel, no permission bypass, no retry.

  | Reviewer | Command | Exit | Time |
  |---|---|---|---|
  | Codex | `codex exec -s read-only` | 0 | 183 s |
  | Antigravity | `agy --mode plan -p` | 0 | 265 s |
  | Grok | `grok --permission-mode plan --output-format plain --rules … -p` | 0 | 1371 s |

- **Fixes:** test-first. The new tests for the cwd error, the `strerror` text and the timeout override failed before
  the fix. The rewritten kill tests passed on the old code: they show that `killpg` already reaches descendants,
  which the old tests did not prove.

Dispositions: **A** = accepted; **P** = partly accepted; **R** = refuted.

## Codex (6 findings: 1 high, 2 medium, 3 low)

| # | Sev. | Finding | Disp. | Resolution |
|---|---|---|---|---|
| C1 | High | `kill_and_reap` can block in `wait()` before the grace drain | R | Python does the same: after a timeout, `subprocess.run` kills and then calls a blocking `process.wait()`. `SIGKILL` cannot be caught or ignored. A process in uninterruptible I/O cannot be reaped by any user-space code, and returning without reaping would leak a zombie. The direct child is in our process group and gets `SIGKILL` from both `killpg` and `child.kill()` |
| C2 | Medium | The deadline can be bypassed: `checked_add` overflow means no deadline; `try_wait` runs before the deadline check after EOF | R | Overflow needs a timeout of about 584 billion years (`u64` seconds), and Python would also wait that long. The order matches CPython: `Popen._wait` calls `_try_wait(WNOHANG)` before it compares the remaining time, so a child that has already exited is reported as exited |
| C3 | Medium | Process tests can misclassify zombies and miss a surviving descendant | A | `proc_state` reads `/proc/<pid>/stat` on Linux and `ps -o stat=` elsewhere, so it tells running, zombie and gone apart. The kill tests record a pid that is not the direct child (`; true` stops bash from exec'ing) and check that it stops running. The pipe-holding and closed-stdout tests record `$!` and check the `sleep`. The survival test requires `Running` |
| C4 | Low | An explicit zero timeout times out at once; sub-second timeouts print `0 seconds` | A | A zero override falls back to the config (`timeout or self.config.timeout`); the message prints `0.5` for 500 ms (`%s` of a float); `explicit_timeout_override` test |
| C5 | Low | A cwd without search permission is reported against `/bin/bash` | A | The cwd is named when `metadata(cwd/".")` fails (missing, not a directory, no search permission, symlink loop), as CPython does after `chdir` fails; `unsearchable_cwd_names_the_cwd` (skipped as root) |
| C6 | Low | The submission matrix does not go through `execute`; no independent workspace copies | A | `submission_matrix_through_execute` (12 cases, text passed through an env variable); `independent_workspace_copies_do_not_share_files` |

## Grok (5 findings: 1 high, 1 medium, 3 low)

| # | Sev. | Finding | Disp. | Resolution |
|---|---|---|---|---|
| G1 | High | Kill tests never observe a descendant | A | Same as C3 |
| G2 | Medium | Unsearchable cwd attributed to `/bin/bash` | A | Same as C5 |
| G3 | Low | `Errno::desc()` is not the platform `strerror` | A | The text comes from `std::io::Error::from_raw_os_error` without the ` (os error N)` suffix; `os_error_text_is_the_platform_strerror` (a symlink-loop cwd: `Too many levels of symbolic links`) |
| G4 | Low | `local.rs` is over the 250-line budget without a README note | A | README note: about 430 lines, and why |
| G5 | Low | Pid-file paths interpolated unquoted into `sh -c` | A | Paths go through `config.env` (`"$PID_FILE"`) |

## Antigravity (8 findings: 1 high, 3 medium, 4 low)

| # | Sev. | Finding | Disp. | Resolution |
|---|---|---|---|---|
| A1 | High | Kill tests do not prove `killpg` kills descendants | A | Same as C3 |
| A2 | Medium | Unsearchable cwd attributed to `/bin/bash` | A | Same as C5 |
| A3 | Medium | Sub-second timeouts truncated; the override is untested | A | Same as C4 |
| A4 | Medium | `uname -p` exit status ignored; `uname` spawned for each environment | A | Only a successful `uname -p` counts (CPython checks `rc == 0`); the fields are computed once per process (`OnceLock`) |
| A5 | Low | Allocations per command in `child_env`, `cwd`, `error_output` | P | `error_output` builds its map directly. `child_env` and `cwd` are kept: one small map per command is negligible next to the `fork`/`exec` of `bash` |
| A6 | Low | `Errno::desc()` instead of `strerror` | A | Same as G3 |
| A7 | Low | Unquoted temp paths in tests | A | Same as G5 |
| A8 | Low | Line budget not recorded | A | Same as G4 |

## Also fixed

- `nix` is pinned exactly (`=0.31.3`), like the other dependencies (an earlier review noted that it was the only
  dependency without `=`).

## Cross-reviewer overlap

| Problem | Reviewers |
|---|---|
| Kill tests do not prove a descendant dies | C3, G1, A1 |
| Unsearchable cwd blamed on `/bin/bash` | C5, G2, A2 |
| Timeout override: zero and sub-second values | C4, A3 |
| `strerror` text | G3, A6 |
| Unquoted paths in tests | G5, A7 |
| Line budget | G4, A8 |
