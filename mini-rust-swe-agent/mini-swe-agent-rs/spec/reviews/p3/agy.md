### Verdict

The core process runner in `LocalEnvironment` (single-thread `poll` loop, absolute monotonic deadline, merged stdout/stderr pipe, `O_NONBLOCK`, `killpg` + `child.kill()` + `wait()`, `check_finished` Python whitespace/line-break semantics) is well-structured and free of deadlocks, fd leaks, zombies, and `unsafe`/panic paths. However, **the process-group kill tests do not actually prove that `killpg` kills descendants**, and there are a few semantic/parity edge cases in `spawn_error`, `execute_with_timeout`, `uname_fields`, and unnecessary per-command allocations that should be fixed before closing P3.

---

### Findings

1. **[High] Process-group kill tests do not prove `killpg` kills descendants**
   - **Location:** [local_env.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p3/mini-rust-swe-agent/mini-swe-agent-rs/tests/local_env.rs#L245-L263), [local_env.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p3/mini-rust-swe-agent/mini-swe-agent-rs/tests/local_env.rs#L389-L401), [local_env.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p3/mini-rust-swe-agent/mini-swe-agent-rs/tests/local_env.rs#L428-L434)
   - **Problem:** None of the three timeout-kill tests actually verify that a descendant process (with a PID distinct from the direct `bash` child) was killed by `killpg`.
   - **Evidence:**
     - In `timeout_kills_child_process` (`L248-L251`), the command is `"sh -c 'echo $$ > ...; while true; do sleep 1; done'"`. Because it is a single simple command with no trailing command, `bash -c` applies its `exec` optimization and replaces itself with `sh`, so `$$` equals the direct `Child` PID. Even if `killpg` were deleted, `child.kill()` alone (`local.rs:366`) would kill `sh` and pass this test.
     - In `background_child_holding_the_pipe_is_killed_at_the_deadline` (`L389-L401`) and `child_that_closes_stdout_and_keeps_running_is_killed` (`L428-L434`), neither test records `$!` or checks `!pid_alive(pid)`. Because `timed_out` bounds the post-kill drain to `GRACE` (1 s) and both tests assert `elapsed < 5s`, both would still pass if `killpg` failed and left `sleep 30` running as an orphan.
   - **Recommended fix:**
     - In `timeout_kills_child_process`, prevent `bash`'s `exec` optimization (e.g., record both `$$` of `bash` and the child's PID, or add `; true` after `sh -c '...'`) and assert `child_pid != shell_pid` and `!pid_alive(child_pid)`.
     - In `background_child_holding_the_pipe_is_killed_at_the_deadline` and `child_that_closes_stdout_and_keeps_running_is_killed`, write `$!` (or the `sleep` PID) to a temp file and assert `!pid_alive(pid)` (with a `SIGKILL` cleanup fallback if alive).

2. **[Medium] `spawn_error` misattributes `EACCES` on an unsearchable `cwd` to `/bin/bash`**
   - **Location:** [local.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p3/mini-rust-swe-agent/mini-swe-agent-rs/src/environment/local.rs#L419-L425)
   - **Problem:** `spawn_error` checks `let cwd_is_bad = !cwd.is_dir();` to decide whether to name `cwd` or `/bin/bash` in the CPython-formatted error message. If `cwd` is an existing directory without execute/search permission (e.g., `chmod 0600`), `stat(cwd)` succeeds so `cwd.is_dir()` is `true`, even though `chdir(cwd)` fails with `EACCES`. Rust then reports `[Errno 13] Permission denied: '/bin/bash'` instead of CPython's `[Errno 13] Permission denied: '<cwd>'`.
   - **Evidence:** `local.rs:422-424`:
     ```rust
     let cwd_is_bad = !cwd.is_dir();
     let filename = if cwd_is_bad { cwd } else { Path::new(SHELL) };
     os_error(error, Some(filename))
     ```
     In CPython `subprocess.Popen._execute_child`, `chdir(cwd)` sets `err_filename = cwd` when `chdir` fails.
   - **Recommended fix:** Also treat `cwd` as bad when it cannot be entered, e.g., `!cwd.is_dir() || std::fs::metadata(cwd.join(".")).is_err()` (or `nix::unistd::access(cwd, AccessFlags::X_OK).is_err()`), and add a test with a `0o600` temp directory (skipped if running as root).

3. **[Medium] `timed_out` truncates sub-second `Duration` values in `execute_with_timeout`**
   - **Location:** [local.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p3/mini-rust-swe-agent/mini-swe-agent-rs/src/environment/local.rs#L86-L97), [local.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p3/mini-rust-swe-agent/mini-swe-agent-rs/src/environment/local.rs#L380-L384)
   - **Problem:** The public method `LocalEnvironment::execute_with_timeout` accepts `timeout: Option<Duration>`, but `timed_out` formats the error message using `timeout.as_secs()`. Any sub-second or fractional duration (such as `Duration::from_millis(500)` or `Duration::from_millis(1500)`) is silently truncated to `0 seconds` or `1 seconds`, contradicting CPython's `TimeoutExpired.__str__` (`%s` formatting of the timeout value). In addition, `execute_with_timeout(..., Some(...))` is never directly tested in `tests/local_env.rs`.
   - **Evidence:** `local.rs:380-383`:
     ```rust
     let message = format!(
         "Command '{command}' timed out after {} seconds",
         timeout.as_secs()
     );
     ```
   - **Recommended fix:** Format whole seconds with `timeout.as_secs()` (matching Python's integer timeout) and fractional durations with `timeout.as_secs_f64()` when `timeout.subsec_nanos() != 0` (or narrow the public API parameter to `Option<u64>` seconds to match `LocalEnvironmentConfig.timeout` and `local.py:24`). Add a test exercising `execute_with_timeout` with an explicit `Some(...)` override.

4. **[Medium] `uname_fields` ignores `uname -p` exit status and respawns `uname` per `LocalEnvironment::new`**
   - **Location:** [local.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p3/mini-rust-swe-agent/mini-swe-agent-rs/src/environment/local.rs#L71-L82), [local.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p3/mini-rust-swe-agent/mini-swe-agent-rs/src/environment/local.rs#L208-L217)
   - **Problem:** `uname_fields()` accepts `out.stdout` even when `uname -p` exits with a non-zero status (e.g., minimal/BusyBox `uname` where `-p` is unsupported). CPython's `platform._syscmd_uname` checks `rc == 0` before using stdout. Additionally, `uname_fields()` spawns `uname -p` on every `LocalEnvironment::new()` call rather than caching it once process-wide like Python's `platform.uname()`.
   - **Evidence:** `local.rs:208-216`:
     ```rust
     let processor = Command::new("uname")
         .arg("-p")
         .stdin(Stdio::null())
         .stderr(Stdio::null())
         .output()
         .ok()
         .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
         .filter(|p| p != "unknown")
         .unwrap_or_default();
     ```
   - **Recommended fix:** Add `.filter(|out| out.status.success())` before reading `out.stdout`, and cache the computed `uname` map in a `static UNAME: OnceLock<Map<String, Value>>` so `LocalEnvironment::new` does not spawn a subprocess on every construction.

5. **[Low] Unnecessary per-command allocations and clones in `child_env`, `execute_with_timeout`, and `error_output`**
   - **Location:** [local.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p3/mini-rust-swe-agent/mini-swe-agent-rs/src/environment/local.rs#L92-L96), [local.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p3/mini-rust-swe-agent/mini-swe-agent-rs/src/environment/local.rs#L104-L115), [local.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p3/mini-rust-swe-agent/mini-swe-agent-rs/src/environment/local.rs#L392-L397)
   - **Problem:**
     - `child_env` clones every host and config environment key and value into a `BTreeMap<OsString, OsString>`, then collects it into a `Vec<(OsString, OsString)>` on every command, even though both `self.host` and `self.config.env` outlive `run_command`.
     - `execute_with_timeout` allocates a `PathBuf` even when borrowing `cwd` or `self.config.cwd`.
     - `error_output` constructs a temporary `serde_json::Value` via `json!(...)` and then clones its inner `Map` (`.as_object().cloned().unwrap_or_default()`), cloning both strings a second time.
   - **Recommended fix:**
     - Return `BTreeMap<&OsStr, &OsStr>` (or `impl IntoIterator<Item = (&OsStr, &OsStr)>`) from `child_env` and pass it directly to `builder.envs(...)`, eliminating all `OsString` allocations per command.
     - Use `Cow<'_, Path>` for `cwd`.
     - Build the `extra` `Map` directly with `Map::from_iter([("exception_type".to_owned(), Value::String(exception_type)), ("exception".to_owned(), Value::String(message))])`.

6. **[Low] `os_error` uses `nix::errno::Errno::desc()` instead of platform `strerror`**
   - **Location:** [local.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p3/mini-rust-swe-agent/mini-swe-agent-rs/src/environment/local.rs#L428-L446)
   - **Problem:** `Errno::desc()` is a hardcoded Linux-derived static table inside `nix` rather than the platform's `strerror(3)` that CPython uses. While common spawn errors (`ENOENT`, `EACCES`, `ENOTDIR`, `EISDIR`) happen to match, other OS errors diverge on both Linux and macOS (e.g., `ENOMEM` is `"Out of memory"` in `nix` vs. `"Cannot allocate memory"` in `strerror`; `EAGAIN` is `"Try again"` vs. `"Resource temporarily unavailable"`).
   - **Recommended fix:** Either note this minor divergence for non-common errnos, or strip the `" (os error N)"` suffix from `error.to_string()` (which `std::io::Error` formats via the platform's `strerror_r`) when `raw_os_error()` is `Some(code)`.

7. **[Low] Unquoted temp-file paths in `tests/local_env.rs` shell commands**
   - **Location:** [local_env.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p3/mini-rust-swe-agent/mini-swe-agent-rs/tests/local_env.rs#L248-L251), [local_env.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p3/mini-rust-swe-agent/mini-swe-agent-rs/tests/local_env.rs#L408-L411)
   - **Problem:** `pid_file.display()` is interpolated unquoted into shell commands (`echo $$ > {}` and `echo $! > {}`), which will break if `TMPDIR` contains whitespace or shell metacharacters. (Compare `test_local.py:159`, which uses `shlex.quote`.)
   - **Recommended fix:** Pass the path via `config.env` (e.g., `PID_FILE`) and reference `"$PID_FILE"` in the shell command, or shell-quote the path.

8. **[Low] `local.rs` exceeds the plan's 250-line simplicity budget without recording the cause in `README.md`**
   - **Location:** [local.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p3/mini-rust-swe-agent/mini-swe-agent-rs/src/environment/local.rs#L1-L482), [README.md](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p3/mini-rust-swe-agent/mini-swe-agent-rs/README.md#L86-L90)
   - **Problem:** Plan §2.6 sets a budget of `environment/local.rs` ≤ 250 lines excluding tests and doc comments, and states: *"If a module goes over budget, the cause must be written down in the README"*. Excluding `#[cfg(test)]`, doc comments, and blank lines, `local.rs` is ~410 lines (and ~345 lines even after `is_python_space`/`split_first_line`/`python_repr` move to `pystr.rs` in P4), and `README.md` does not record the budget overrun.
   - **Recommended fix:** Add a sentence to the `LocalEnvironment (P3)` note in `README.md` documenting the overrun and its cause (explicit pipe/poll/process-group management and CPython error formatting that Python gets from `subprocess`).
