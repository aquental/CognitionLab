//! LocalEnvironment (plan §3.7, phase P3): the port of `tests/environments/test_local.py` plus the
//! process, submission and shell-state cases the plan adds (§3.15 item 1, traps 4-8, 24, 30).

use std::ffi::OsString;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mini_swe_agent_rs::environment::Environment;
use mini_swe_agent_rs::environment::local::{
    EnvVars, LocalEnvironment, LocalEnvironmentConfig, check_finished,
};
use mini_swe_agent_rs::error::{InterruptKind, StepError};
use mini_swe_agent_rs::host_env::HostEnv;
use mini_swe_agent_rs::message::{Action, ExecOutput};
use serde_json::json;

const MARKER: &str = "COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT";

fn host() -> Arc<HostEnv> {
    Arc::new(HostEnv::from_vars(std::env::vars_os()))
}

fn env_with(config: LocalEnvironmentConfig) -> LocalEnvironment {
    LocalEnvironment::new(config, host())
}

fn env() -> LocalEnvironment {
    env_with(LocalEnvironmentConfig::default())
}

fn cmd(command: &str) -> Action {
    Action::Command {
        command: command.to_owned(),
    }
}

/// Test helper: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::panic)]
fn run(env: &LocalEnvironment, command: &str) -> ExecOutput {
    env.execute(&cmd(command), None)
        .unwrap_or_else(|e| panic!("{command}: {e}"))
}

fn timeout_secs(seconds: u64) -> LocalEnvironmentConfig {
    LocalEnvironmentConfig {
        timeout: seconds,
        ..LocalEnvironmentConfig::default()
    }
}

fn with_env(pairs: &[(&str, &str)]) -> LocalEnvironmentConfig {
    let env: EnvVars = pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    LocalEnvironmentConfig {
        env,
        ..LocalEnvironmentConfig::default()
    }
}

// ---- Port of test_local.py ---------------------------------------------------------------------

#[test]
fn config_defaults() {
    let config = LocalEnvironmentConfig::default();
    assert_eq!(config.cwd, "");
    assert!(config.env.is_empty());
    assert_eq!(config.timeout, 30);
    let parsed: LocalEnvironmentConfig = serde_json::from_value(json!({})).unwrap();
    assert_eq!(parsed, config);
}

#[test]
fn basic_execution() {
    let result = run(&env(), "echo 'hello world'");
    assert_eq!(result.returncode, 0);
    assert!(result.output.contains("hello world"));
    assert_eq!(result.exception_info, "");
    assert_eq!(
        result.extra, None,
        "a successful output has no extra key (local.py:30)"
    );
}

#[test]
fn set_env_variables() {
    let env = env_with(with_env(&[
        ("TEST_VAR", "test_value"),
        ("ANOTHER_VAR", "another_value"),
    ]));
    assert!(run(&env, "echo $TEST_VAR").output.contains("test_value"));
    assert!(
        run(&env, "echo $TEST_VAR $ANOTHER_VAR")
            .output
            .contains("test_value another_value")
    );
}

#[test]
fn existing_env_variables() {
    // Python patches os.environ; Rust injects the host environment instead.
    let host = Arc::new(HostEnv::from_vars([
        (OsString::from("PATH"), OsString::from("/usr/bin:/bin")),
        (
            OsString::from("EXISTING_VAR"),
            OsString::from("existing_value"),
        ),
    ]));
    let env = LocalEnvironment::new(with_env(&[("NEW_VAR", "new_value")]), host);
    assert!(
        run(&env, "echo $EXISTING_VAR $NEW_VAR")
            .output
            .contains("existing_value new_value")
    );
}

#[test]
fn env_variable_override() {
    let host = Arc::new(HostEnv::from_vars([(
        OsString::from("CONFLICT_VAR"),
        OsString::from("original_value"),
    )]));
    let env = LocalEnvironment::new(with_env(&[("CONFLICT_VAR", "override_value")]), host);
    assert!(
        run(&env, "echo $CONFLICT_VAR")
            .output
            .contains("override_value")
    );
}

#[test]
fn custom_cwd() {
    let dir = tempfile::tempdir().unwrap();
    let env = env_with(LocalEnvironmentConfig {
        cwd: dir.path().to_str().unwrap().to_owned(),
        ..Default::default()
    });
    let result = run(&env, "pwd -P");
    assert_eq!(result.returncode, 0);
    assert!(
        result
            .output
            .contains(dir.path().canonicalize().unwrap().to_str().unwrap())
    );
}

#[test]
fn cwd_parameter_override() {
    let (dir1, dir2) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let env = env_with(LocalEnvironmentConfig {
        cwd: dir1.path().to_str().unwrap().to_owned(),
        ..Default::default()
    });
    let result = env.execute(&cmd("pwd -P"), Some(dir2.path())).unwrap();
    assert!(
        result
            .output
            .contains(dir2.path().canonicalize().unwrap().to_str().unwrap())
    );
}

#[test]
fn default_cwd() {
    let result = run(&env(), "pwd -P");
    let current = std::env::current_dir().unwrap().canonicalize().unwrap();
    assert!(result.output.contains(current.to_str().unwrap()));
}

#[test]
fn command_failure() {
    let result = run(&env(), "exit 1");
    assert_eq!(result.returncode, 1);
    assert_eq!(result.output, "");
}

#[test]
fn nonexistent_command() {
    let result = run(&env(), "nonexistent_command_12345");
    assert_ne!(result.returncode, 0);
    assert!(
        result.output.contains("nonexistent_command_12345")
            || result.output.contains("command not found")
    );
}

#[test]
fn stderr_capture() {
    let result = run(&env(), "echo 'error message' >&2");
    assert_eq!(result.returncode, 0);
    assert!(result.output.contains("error message"));
}

#[test]
fn stdout_and_stderr_keep_their_order() {
    let result = run(&env(), "echo one; echo two >&2; echo three");
    assert_eq!(result.output, "one\ntwo\nthree\n");
}

#[test]
fn timeout() {
    let result = run(&env_with(timeout_secs(1)), "sleep 2");
    assert_eq!(result.returncode, -1);
    assert!(result.exception_info.contains("timed out"));
    let extra = result.extra.unwrap();
    assert_eq!(extra["exception_type"], "TimeoutExpired");
    assert_eq!(
        extra["exception"],
        "Command 'sleep 2' timed out after 1 seconds"
    );
    assert_eq!(
        result.exception_info,
        "An error occurred while executing the command: Command 'sleep 2' timed out after 1 seconds"
    );
}

/// The state of a process: running, a zombie (dead but not reaped), or gone. `kill(pid, 0)`
/// cannot tell a zombie from a running process, so read `/proc` on Linux and `ps` elsewhere.
#[derive(Debug, PartialEq, Eq)]
enum ProcState {
    Running,
    Zombie,
    Gone,
}

/// Test helper: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::unwrap_used)]
fn proc_state(pid: i32) -> ProcState {
    let proc_root = Path::new("/proc");
    let state = if proc_root.join("self").exists() {
        match std::fs::read_to_string(proc_root.join(pid.to_string()).join("stat")) {
            // The state follows the parenthesized command name.
            Ok(stat) => stat
                .rsplit_once(") ")
                .map(|(_, rest)| rest.chars().next().unwrap_or('?')),
            Err(_) => None,
        }
    } else {
        let out = std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().chars().next()
    };
    match state {
        None => ProcState::Gone,
        Some('Z') => ProcState::Zombie,
        Some(_) => ProcState::Running,
    }
}

/// Waits up to 3 s for `pid` to stop running; kills it if it is still there.
fn dead_after_wait(pid: i32) -> bool {
    for _ in 0..30 {
        if proc_state(pid) != ProcState::Running {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(pid),
        nix::sys::signal::Signal::SIGKILL,
    );
    false
}

/// A config whose `env` holds the given pid-file paths, so commands never interpolate paths.
/// Test helper: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::unwrap_used)]
fn with_pid_files(timeout: u64, files: &[(&str, &Path)]) -> LocalEnvironmentConfig {
    let env = files
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.to_str().unwrap().to_owned()))
        .collect();
    LocalEnvironmentConfig {
        timeout,
        env,
        ..Default::default()
    }
}

/// Test helper: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::panic)]
fn read_pid(path: &Path) -> i32 {
    for _ in 0..50 {
        if let Ok(text) = std::fs::read_to_string(path)
            && let Ok(pid) = text.trim().parse()
        {
            return pid;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("no pid in {}", path.display());
}

#[test]
fn timeout_kills_child_process() {
    // The trailing `; true` stops bash from exec'ing `sh`, so the looping process is a
    // grandchild that only `killpg` reaches (`local.py:88-91`).
    let dir = tempfile::tempdir().unwrap();
    let (shell_file, child_file) = (dir.path().join("shell.pid"), dir.path().join("child.pid"));
    let config = with_pid_files(1, &[("SHELL_PID", &shell_file), ("CHILD_PID", &child_file)]);
    let command = "echo $$ > \"$SHELL_PID\"; sh -c 'echo $$ > \"$CHILD_PID\"; while true; do sleep 1; done'; true";
    let result = run(&env_with(config), command);
    assert_eq!(result.returncode, -1);
    let (shell, child) = (read_pid(&shell_file), read_pid(&child_file));
    assert_ne!(
        shell, child,
        "the looping process must not be the direct child"
    );
    assert!(
        dead_after_wait(child),
        "grandchild {child} survived the timeout"
    );
}

#[test]
fn custom_timeout() {
    let parsed: LocalEnvironmentConfig = serde_json::from_value(json!({"timeout": 5})).unwrap();
    assert_eq!(env_with(parsed).config().timeout, 5);
}

#[test]
fn return_codes() {
    for (command, expected) in [("echo 'test'", 0), ("exit 1", 1), ("exit 42", 42)] {
        assert_eq!(run(&env(), command).returncode, expected, "{command}");
    }
}

#[test]
fn multiline_output_rust_only() {
    // `echo -e` is shell-sensitive (R11): a Rust-only bash test, never parity evidence.
    let result = run(&env(), "echo -e 'line1\\nline2\\nline3'");
    assert_eq!(result.returncode, 0);
    let lines: Vec<&str> = result.output.trim().split('\n').collect();
    assert_eq!(lines, ["line1", "line2", "line3"]);
}

#[test]
fn file_operations() {
    let dir = tempfile::tempdir().unwrap();
    let env = env_with(LocalEnvironmentConfig {
        cwd: dir.path().to_str().unwrap().to_owned(),
        ..Default::default()
    });
    assert_eq!(run(&env, "echo 'test content' > test.txt").returncode, 0);
    assert!(run(&env, "cat test.txt").output.contains("test content"));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("test.txt"))
            .unwrap()
            .trim(),
        "test content"
    );
}

#[test]
fn shell_features() {
    assert!(
        run(&env(), "echo 'hello world' | grep 'world'")
            .output
            .contains("hello world")
    );
    assert!(
        run(&env(), "echo $(echo 'nested')")
            .output
            .contains("nested")
    );
}

// ---- New process cases (plan §3.15 item 1) -----------------------------------------------------

#[test]
fn runs_bash() {
    let result = run(&env(), "echo \"bash=${BASH_VERSION:+yes}\"");
    assert_eq!(result.output, "bash=yes\n");
}

#[test]
fn universal_newlines() {
    let result = run(&env(), "printf 'a\\r\\nb\\rc\\n'");
    assert_eq!(result.output, "a\nb\nc\n");
}

#[test]
fn invalid_utf8_is_replaced() {
    let result = run(&env(), "printf 'ok\\377end'");
    assert_eq!(result.output, "ok\u{fffd}end");
}

#[test]
fn signal_exit_is_negative() {
    assert_eq!(run(&env(), "kill -9 $$").returncode, -9);
    assert_eq!(run(&env(), "kill -TERM $$").returncode, -15);
}

#[test]
fn missing_cwd_matches_python_error_text() {
    let env = env_with(LocalEnvironmentConfig {
        cwd: "/nonexistent/dir-12345".to_owned(),
        ..Default::default()
    });
    let result = run(&env, "echo hi");
    assert_eq!(result.returncode, -1);
    assert_eq!(result.output, "");
    let message = "[Errno 2] No such file or directory: '/nonexistent/dir-12345'";
    assert_eq!(
        result.exception_info,
        format!("An error occurred while executing the command: {message}")
    );
    let extra = result.extra.unwrap();
    assert_eq!(extra["exception_type"], "FileNotFoundError");
    assert_eq!(extra["exception"], message);
}

#[test]
fn cwd_that_is_a_file_is_not_a_directory_error() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let env = env_with(LocalEnvironmentConfig {
        cwd: file.path().to_str().unwrap().to_owned(),
        ..Default::default()
    });
    let result = run(&env, "echo hi");
    assert_eq!(
        result.extra.unwrap()["exception_type"],
        "NotADirectoryError"
    );
    assert!(
        result
            .exception_info
            .contains("[Errno 20] Not a directory: '")
    );
}

#[test]
fn stdin_is_at_eof() {
    let result = run(&env_with(timeout_secs(5)), "cat; echo done");
    assert_eq!(result.output, "done\n");
}

#[test]
fn background_child_holding_the_pipe_is_killed_at_the_deadline() {
    // Trap 24: `communicate()` waits for EOF, so the command ends only at the timeout.
    let dir = tempfile::tempdir().unwrap();
    let pid_file = dir.path().join("bg.pid");
    let start = Instant::now();
    let config = with_pid_files(1, &[("PID_FILE", &pid_file)]);
    let result = run(
        &env_with(config),
        "echo hi; sleep 30 & echo $! > \"$PID_FILE\"",
    );
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "{:?}",
        start.elapsed()
    );
    assert_eq!(result.returncode, -1);
    assert_eq!(result.output, "hi\n");
    assert_eq!(result.extra.unwrap()["exception_type"], "TimeoutExpired");
    let pid = read_pid(&pid_file);
    assert!(
        dead_after_wait(pid),
        "the pipe-holding sleep {pid} survived"
    );
}

#[test]
fn background_child_with_redirected_output_survives() {
    let dir = tempfile::tempdir().unwrap();
    let pid_file = dir.path().join("bg.pid");
    let start = Instant::now();
    let config = with_pid_files(5, &[("PID_FILE", &pid_file)]);
    let result = run(
        &env_with(config),
        "sleep 30 >/dev/null 2>&1 & echo $! > \"$PID_FILE\"",
    );
    assert!(start.elapsed() < Duration::from_secs(3));
    assert_eq!(result.returncode, 0);
    let pid = read_pid(&pid_file);
    std::thread::sleep(Duration::from_millis(300));
    let state = proc_state(pid);
    let _ = nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(pid),
        nix::sys::signal::Signal::SIGKILL,
    );
    assert_eq!(
        state,
        ProcState::Running,
        "the redirected background child must keep running, as in Python"
    );
}

#[test]
fn child_that_closes_stdout_and_keeps_running_is_killed() {
    let dir = tempfile::tempdir().unwrap();
    let pid_file = dir.path().join("sleep.pid");
    let start = Instant::now();
    let config = with_pid_files(1, &[("PID_FILE", &pid_file)]);
    let command = "exec >/dev/null 2>&1; sleep 30 & echo $! > \"$PID_FILE\"; wait";
    let result = run(&env_with(config), command);
    assert!(start.elapsed() < Duration::from_secs(5));
    assert_eq!(result.returncode, -1);
    assert_eq!(result.extra.unwrap()["exception_type"], "TimeoutExpired");
    let pid = read_pid(&pid_file);
    assert!(dead_after_wait(pid), "sleep {pid} survived the timeout");
}

#[test]
fn continuous_output_stops_at_the_absolute_deadline() {
    let start = Instant::now();
    let result = run(
        &env_with(timeout_secs(1)),
        "yes | head -c 200000000 | while read -r l; do echo \"$l\"; done",
    );
    let elapsed = start.elapsed();
    assert!(elapsed < Duration::from_secs(4), "{elapsed:?}");
    assert_eq!(result.returncode, -1);
    assert!(result.output.starts_with("y\n"));
}

#[test]
fn quoted_multiline_command_in_timeout_message() {
    let command = "sleep 2 # it's\necho done";
    let result = run(&env_with(timeout_secs(1)), command);
    assert_eq!(
        result.extra.unwrap()["exception"],
        format!("Command '{command}' timed out after 1 seconds")
    );
}

#[test]
fn inherited_bash_env_is_not_sourced() {
    let dir = tempfile::tempdir().unwrap();
    let rc = dir.path().join("rc.sh");
    std::fs::write(&rc, "echo SOURCED\n").unwrap();
    let mut vars: Vec<(OsString, OsString)> = std::env::vars_os().collect();
    vars.push((OsString::from("BASH_ENV"), rc.clone().into_os_string()));
    let host = Arc::new(HostEnv::from_vars(vars));
    let inherited = LocalEnvironment::new(LocalEnvironmentConfig::default(), host.clone());
    assert_eq!(run(&inherited, "echo cmd").output, "cmd\n");
    let configured = LocalEnvironment::new(with_env(&[("BASH_ENV", rc.to_str().unwrap())]), host);
    assert_eq!(run(&configured, "echo cmd").output, "SOURCED\ncmd\n");
}

// ---- Shell state (trap 24) ---------------------------------------------------------------------

#[test]
fn shell_state_does_not_persist_but_files_do() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    let env = env_with(LocalEnvironmentConfig {
        cwd: dir.path().to_str().unwrap().to_owned(),
        ..Default::default()
    });
    let root = dir.path().canonicalize().unwrap();
    run(&env, "cd sub; export STUDY_X=abc; printf x > f");
    let result = run(
        &env,
        "pwd -P; printf '%s\\n' \"${STUDY_X-unset}\"; cat sub/f",
    );
    assert_eq!(result.output, format!("{}\nunset\nx", root.display()));
    assert_eq!(
        run(&env, "cd sub && pwd -P").output,
        format!("{}\n", root.join("sub").display())
    );
}

// ---- Submission (trap 30) ----------------------------------------------------------------------

fn output(text: &str, returncode: i32) -> ExecOutput {
    ExecOutput {
        output: text.to_owned(),
        returncode,
        exception_info: String::new(),
        extra: None,
    }
}

/// Test helper: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::unwrap_used)]
fn submission(text: &str, returncode: i32) -> Option<String> {
    match check_finished(&output(text, returncode)) {
        Ok(()) => None,
        Err(interrupt) => {
            assert_eq!(interrupt.kind(), InterruptKind::Submitted);
            let info = interrupt.head().exit_info().unwrap();
            assert_eq!(info.exit_status, "Submitted");
            assert_eq!(interrupt.head().content, info.submission);
            Some(info.submission)
        }
    }
}

#[test]
fn submission_matrix() {
    assert_eq!(submission(&format!("{MARKER}\n"), 0).as_deref(), Some(""));
    assert_eq!(
        submission(&format!("\n\n  {MARKER}\nrest"), 0).as_deref(),
        Some("rest")
    );
    assert_eq!(
        submission(&format!("  {MARKER}  \nx\n"), 0).as_deref(),
        Some("x\n")
    );
    assert_eq!(submission(&format!("log line\n{MARKER}\n"), 0), None);
    assert_eq!(submission(&format!("{}\n", MARKER.to_lowercase()), 0), None);
    assert_eq!(submission(&format!("{MARKER} done\n"), 0), None);
    assert_eq!(submission(&format!("{MARKER}\n"), 1), None);
    assert_eq!(
        submission(&format!("{MARKER}\na\nb\n"), 0).as_deref(),
        Some("a\nb\n")
    );
    assert_eq!(
        submission(&format!("{MARKER}\r\nx\r\n"), 0).as_deref(),
        Some("x\r\n")
    );
    assert_eq!(submission(MARKER, 0).as_deref(), Some(""));
    assert_eq!(submission("", 0), None);
}

#[test]
fn submission_uses_python_whitespace_and_line_breaks() {
    // str.lstrip/strip treat \x1c-\x1f as whitespace; splitlines breaks on \x0b, \x0c, \x1c-\x1e, \x85,  ,  .
    assert_eq!(
        submission(&format!("\x1f{MARKER}\nx"), 0).as_deref(),
        Some("x")
    );
    for sep in [
        "\x0b", "\x0c", "\x1c", "\x1d", "\x1e", "\u{85}", "\u{2028}", "\u{2029}",
    ] {
        assert_eq!(
            submission(&format!("{MARKER}{sep}tail"), 0).as_deref(),
            Some("tail"),
            "{sep:?}"
        );
    }
}

#[test]
fn submission_through_the_environment() {
    let result = env().execute(&cmd(&format!("printf '{MARKER}\\r\\nx\\r\\n'")), None);
    let Err(StepError::Interrupt(interrupt)) = result else {
        panic!("{result:?}")
    };
    assert_eq!(
        interrupt.head().exit_info().unwrap().submission,
        "x\n",
        "universal newlines apply first"
    );
    // A failing command never submits.
    assert!(
        env()
            .execute(&cmd(&format!("echo {MARKER}; exit 3")), None)
            .is_ok()
    );
}

// ---- Template vars and serialization -----------------------------------------------------------

#[test]
fn template_vars_merge_config_uname_and_host_env() {
    let host = Arc::new(HostEnv::from_vars([
        (OsString::from("HOME_TEST"), OsString::from("h")),
        (OsString::from("system"), OsString::from("from-env")),
    ]));
    let env = LocalEnvironment::new(with_env(&[("A", "1")]), host);
    let vars = env.template_vars().unwrap();
    assert_eq!(vars["cwd"], "");
    assert_eq!(vars["timeout"], 30);
    assert_eq!(vars["env"], json!({"A": "1"}));
    assert_eq!(vars["HOME_TEST"], "h");
    assert_eq!(
        vars["system"], "from-env",
        "os.environ wins over platform.uname (local.py:59)"
    );
    for key in ["node", "release", "version", "machine", "processor"] {
        assert!(vars[key].is_string(), "{key}");
    }
    assert_ne!(vars["processor"], "unknown");
}

#[test]
fn serialize_dumps_the_config() {
    let serialized = env_with(with_env(&[("A", "1")])).serialize().unwrap();
    assert_eq!(
        serialized["info"]["config"]["environment"],
        json!({"cwd": "", "env": {"A": "1"}, "timeout": 30})
    );
    assert!(
        serialized["info"]["config"]["environment_type"]
            .as_str()
            .unwrap()
            .ends_with("LocalEnvironment")
    );
}

// ---- Review fixes (P3) -------------------------------------------------------------------------

#[test]
fn unsearchable_cwd_names_the_cwd() {
    use std::os::unix::fs::PermissionsExt;
    let uid = std::process::Command::new("id").arg("-u").output().unwrap();
    if String::from_utf8_lossy(&uid.stdout).trim() == "0" {
        eprintln!("skipped: root can enter a directory without search permission");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let locked = dir.path().join("locked");
    std::fs::create_dir(&locked).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o600)).unwrap();
    let env = env_with(LocalEnvironmentConfig {
        cwd: locked.to_str().unwrap().to_owned(),
        ..Default::default()
    });
    let result = run(&env, "echo hi");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(
        result.extra.as_ref().unwrap()["exception_type"],
        "PermissionError"
    );
    let expected = format!("[Errno 13] Permission denied: '{}'", locked.display());
    assert_eq!(result.extra.unwrap()["exception"], expected.as_str());
}

#[test]
fn os_error_text_is_the_platform_strerror() {
    // A symlink loop as cwd: CPython prints `strerror(ELOOP)`, "Too many levels of symbolic links".
    let dir = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(dir.path().join("b"), dir.path().join("a")).unwrap();
    std::os::unix::fs::symlink(dir.path().join("a"), dir.path().join("b")).unwrap();
    let cwd = dir.path().join("a");
    let env = env_with(LocalEnvironmentConfig {
        cwd: cwd.to_str().unwrap().to_owned(),
        ..Default::default()
    });
    let result = run(&env, "echo hi");
    let extra = result.extra.unwrap();
    assert_eq!(extra["exception_type"], "OSError");
    let message = extra["exception"].as_str().unwrap();
    assert!(
        message.contains("] Too many levels of symbolic links: '"),
        "{message}"
    );
    assert!(!message.contains("os error"), "{message}");
}

#[test]
fn explicit_timeout_override() {
    let env = env_with(timeout_secs(1));
    // Python's `timeout or self.config.timeout`: zero falls back to the config.
    let quick = env
        .execute_with_timeout(&cmd("echo ok"), None, Some(Duration::ZERO))
        .unwrap();
    assert_eq!(quick.returncode, 0);
    let result = env
        .execute_with_timeout(&cmd("sleep 2"), None, Some(Duration::from_millis(500)))
        .unwrap();
    assert_eq!(
        result.extra.unwrap()["exception"],
        "Command 'sleep 2' timed out after 0.5 seconds"
    );
}

#[test]
fn submission_matrix_through_execute() {
    // The text reaches the command through an environment variable, so no quoting is needed.
    let cases: &[(&str, i32, Option<&str>)] = &[
        ("{M}\n", 0, Some("")),
        ("\n\n  {M}\nrest", 0, Some("rest")),
        ("  {M}  \nx\n", 0, Some("x\n")),
        ("log line\n{M}\n", 0, None),
        ("{m}\n", 0, None),
        ("{M} done\n", 0, None),
        ("{M}\n", 1, None),
        ("{M}\na\nb\n", 0, Some("a\nb\n")),
        ("{M}", 0, Some("")),
        ("", 0, None),
        ("\x1f{M}\nx", 0, Some("x")),
        ("{M}\u{2028}tail", 0, Some("tail")),
    ];
    for (template, code, expected) in cases {
        let text = template
            .replace("{M}", MARKER)
            .replace("{m}", &MARKER.to_lowercase());
        let config = LocalEnvironmentConfig {
            env: [("TEXT".to_owned(), text.clone())].into_iter().collect(),
            ..Default::default()
        };
        let result =
            env_with(config).execute(&cmd(&format!("printf '%s' \"$TEXT\"; exit {code}")), None);
        let submitted = match result {
            Ok(_) => None,
            Err(StepError::Interrupt(interrupt)) => {
                Some(interrupt.head().exit_info().unwrap().submission)
            }
            Err(e) => panic!("{text:?}: {e}"),
        };
        assert_eq!(submitted.as_deref(), *expected, "{text:?} exit {code}");
    }
}

#[test]
fn independent_workspace_copies_do_not_share_files() {
    let (one, two) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    for dir in [&one, &two] {
        std::fs::write(dir.path().join("seed.txt"), "seed\n").unwrap();
    }
    let in_dir = |dir: &tempfile::TempDir| {
        env_with(LocalEnvironmentConfig {
            cwd: dir.path().to_str().unwrap().to_owned(),
            ..Default::default()
        })
    };
    run(
        &in_dir(&one),
        "cd . && export X=1 && printf one > written.txt",
    );
    assert_eq!(run(&in_dir(&one), "cat written.txt").output, "one");
    let other = run(
        &in_dir(&two),
        "ls; cat written.txt 2>&1; printf '%s' \"${X-unset}\"",
    );
    assert_eq!(
        other.output,
        "seed.txt\ncat: written.txt: No such file or directory\nunset"
    );
}

#[test]
fn timeout_kills_a_shell_that_left_its_process_group() {
    // Plan §2.4: after `killpg`, Python's `communicate()` hangs when the shell moved to another
    // process group. Here the shell's process becomes a `perl` that joins the test runner's
    // group, so `killpg` of the child's own group misses it; the port also kills the child.
    let dir = tempfile::tempdir().unwrap();
    let shell_file = dir.path().join("shell.pid");
    let config = with_pid_files(1, &[("SHELL_PID", &shell_file)]);
    let command = "echo $$ > \"$SHELL_PID\"; exec perl -MPOSIX -e 'POSIX::setpgid(0, getpgrp(getppid())) or die qq(setpgid: $!); sleep 30'";
    let start = Instant::now();
    let result = run(&env_with(config), command);
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "{:?}",
        start.elapsed()
    );
    assert_eq!(result.returncode, -1, "{}", result.output);
    assert!(!result.output.contains("setpgid:"), "{}", result.output);
    let shell = read_pid(&shell_file);
    assert!(
        dead_after_wait(shell),
        "the shell {shell} survived the timeout"
    );
}
