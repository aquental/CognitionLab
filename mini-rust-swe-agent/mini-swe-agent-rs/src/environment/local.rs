//! `LocalEnvironment`: one independent `bash -c` process per action (port of `local.py`, plan §3.7).
//!
//! - stdout and stderr share one pipe, so their order is kept (`stderr=STDOUT`);
//! - the child runs in a new process group with stdin at `/dev/null`; on timeout the whole group
//!   is killed (`killpg`) and the partial output is kept;
//! - one monotonic, absolute deadline covers both the pipe drain and the exit, as
//!   `communicate(timeout)` does; the pipe is read by a `poll` loop in the calling thread;
//! - output is decoded as UTF-8 with replacement, then universal newlines apply (`text=True`);
//! - any error becomes the `returncode: -1` output with Python's error text.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{ErrorKind, Read};
use std::os::fd::AsFd;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::fcntl::{FcntlArg, OFlag, fcntl};
use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::environment::Environment;
use crate::error::{AgentError, Interrupt, InterruptKind, StepError};
use crate::host_env::HostEnv;
use crate::merge::recursive_merge;
use crate::message::{Action, ExecOutput, Message};
use crate::pystr;

/// The submission marker (`local.py:48`).
pub const SUBMIT_MARKER: &str = "COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT";

/// The shell (decision D11: bash, not Python's `/bin/sh`).
const SHELL: &str = "/bin/bash";

/// How long to keep reading after a kill (a grandchild may hold the pipe; plan §2.2).
const GRACE: Duration = Duration::from_secs(1);

/// Config of the local environment (`LocalEnvironmentConfig`, `local.py:13-16`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalEnvironmentConfig {
    /// Working directory; `""` means the process's current directory.
    pub cwd: String,
    /// Variables added to (and overriding) the host environment.
    pub env: BTreeMap<String, String>,
    /// Command timeout in seconds.
    pub timeout: u64,
}

impl Default for LocalEnvironmentConfig {
    fn default() -> Self {
        Self {
            cwd: String::new(),
            env: BTreeMap::new(),
            timeout: 30,
        }
    }
}

/// Runs each action as its own `bash -c` process.
#[derive(Debug)]
pub struct LocalEnvironment {
    config: LocalEnvironmentConfig,
    host: Arc<HostEnv>,
    /// `platform.uname()` fields, computed once (Python caches them too).
    uname: Map<String, Value>,
}

impl LocalEnvironment {
    /// A local environment over the host environment snapshot.
    pub fn new(config: LocalEnvironmentConfig, host: Arc<HostEnv>) -> Self {
        Self {
            config,
            host,
            uname: uname_fields(),
        }
    }

    /// Runs one action with an explicit timeout (Python's `execute(..., timeout=)`).
    pub fn execute_with_timeout(
        &self,
        action: &Action,
        cwd: Option<&Path>,
        timeout: Option<Duration>,
    ) -> Result<ExecOutput, StepError> {
        let cwd = match cwd {
            Some(cwd) if !cwd.as_os_str().is_empty() => cwd.to_path_buf(),
            _ if !self.config.cwd.is_empty() => PathBuf::from(&self.config.cwd),
            _ => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        };
        let timeout = timeout.unwrap_or(Duration::from_secs(self.config.timeout));
        let output = run_command(action.command(), &cwd, self.child_env(), timeout);
        check_finished(&output).map_err(StepError::Interrupt)?;
        Ok(output)
    }

    /// `os.environ | config.env`, without an inherited `BASH_ENV` (D11: bash would source it).
    fn child_env(&self) -> Vec<(OsString, OsString)> {
        let mut vars: BTreeMap<OsString, OsString> = self
            .host
            .child_vars()
            .filter(|(name, _)| name.as_os_str() != "BASH_ENV")
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect();
        for (name, value) in &self.config.env {
            vars.insert(OsString::from(name), OsString::from(value));
        }
        vars.into_iter().collect()
    }
}

impl Environment for LocalEnvironment {
    type Config = LocalEnvironmentConfig;

    fn config(&self) -> &LocalEnvironmentConfig {
        &self.config
    }

    fn execute(&self, action: &Action, cwd: Option<&Path>) -> Result<ExecOutput, StepError> {
        self.execute_with_timeout(action, cwd, None)
    }

    /// `recursive_merge(config, platform.uname(), os.environ)` (`local.py:58-59`).
    fn template_vars(&self) -> Result<Value, AgentError> {
        let layers = [
            serde_json::to_value(&self.config)?,
            Value::Object(self.uname.clone()),
            Value::Object(self.host.template_vars()),
        ];
        Ok(Value::Object(recursive_merge(&layers)?))
    }
}

/// Raises `Submitted` when the first non-blank line of the output is the marker and the command
/// succeeded (`local.py:45-56`). Python's `lstrip`, `strip` and `splitlines` rules apply.
pub fn check_finished(output: &ExecOutput) -> Result<(), Interrupt> {
    if output.returncode != 0 {
        return Ok(());
    }
    let text = pystr::lstrip(&output.output);
    let (first, rest) = pystr::split_first_line(text);
    if first.is_empty() && rest.is_empty() {
        return Ok(());
    }
    if pystr::strip(first) != SUBMIT_MARKER {
        return Ok(());
    }
    let message = Message::exit("Submitted", rest, rest);
    Err(Interrupt::new(
        InterruptKind::Submitted,
        message,
        Vec::new(),
    ))
}

/// `platform.uname()` fields. `processor` comes from `uname -p`, with `unknown` read as `""`.
fn uname_fields() -> Map<String, Value> {
    let mut fields = Map::new();
    let info = nix::sys::utsname::uname().ok();
    let text = |value: Option<&std::ffi::OsStr>| {
        Value::String(
            value
                .map(|v| v.to_string_lossy().into_owned())
                .unwrap_or_default(),
        )
    };
    fields.insert(
        "system".to_owned(),
        text(info.as_ref().map(|u| u.sysname())),
    );
    fields.insert("node".to_owned(), text(info.as_ref().map(|u| u.nodename())));
    fields.insert(
        "release".to_owned(),
        text(info.as_ref().map(|u| u.release())),
    );
    fields.insert(
        "version".to_owned(),
        text(info.as_ref().map(|u| u.version())),
    );
    fields.insert(
        "machine".to_owned(),
        text(info.as_ref().map(|u| u.machine())),
    );
    let processor = Command::new("uname")
        .arg("-p")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .filter(|p| p != "unknown")
        .unwrap_or_default();
    fields.insert("processor".to_owned(), Value::String(processor));
    fields
}

/// Runs `command` and never fails: errors become the `returncode: -1` output.
fn run_command(
    command: &str,
    cwd: &Path,
    env: Vec<(OsString, OsString)>,
    timeout: Duration,
) -> ExecOutput {
    let (mut reader, writer) = match std::io::pipe() {
        Ok(pair) => pair,
        Err(e) => return error_output(Vec::new(), os_error(&e, None)),
    };
    let stderr = match writer.try_clone() {
        Ok(stderr) => stderr,
        Err(e) => return error_output(Vec::new(), os_error(&e, None)),
    };
    let mut builder = Command::new(SHELL);
    builder
        .arg("-c")
        .arg(command)
        .current_dir(cwd)
        .env_clear()
        .envs(env)
        .stdin(Stdio::null())
        .stdout(writer)
        .stderr(stderr)
        .process_group(0);
    let spawned = builder.spawn();
    // The parent's copies of the write end must close, or EOF never arrives.
    drop(builder);
    let mut child = match spawned {
        Ok(child) => child,
        Err(e) => return error_output(Vec::new(), spawn_error(&e, cwd)),
    };
    let deadline = Instant::now().checked_add(timeout);
    let mut buffer = Vec::new();

    if let Err(e) = set_nonblocking(&reader) {
        kill_and_reap(&mut child);
        return error_output(buffer, os_error(&std::io::Error::from(e), None));
    }
    match drain(&mut reader, &mut buffer, deadline) {
        Ok(Drain::Eof) => {}
        Ok(Drain::Deadline) => return timed_out(&mut child, &mut reader, buffer, command, timeout),
        Err(e) => {
            kill_and_reap(&mut child);
            return error_output(buffer, os_error(&e, None));
        }
    }
    // EOF: wait for the exit under the same deadline (the child may close stdout and keep running).
    let mut pause = Duration::from_millis(1);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let returncode = status
                    .code()
                    .or_else(|| status.signal().map(|s| -s))
                    .unwrap_or(-1);
                return ExecOutput {
                    output: decode(&buffer),
                    returncode,
                    exception_info: String::new(),
                    extra: None,
                };
            }
            Ok(None) => {
                let left = remaining(deadline);
                if left == Some(Duration::ZERO) {
                    return timed_out(&mut child, &mut reader, buffer, command, timeout);
                }
                std::thread::sleep(left.map_or(pause, |r| r.min(pause)));
                pause = (pause * 2).min(Duration::from_millis(20));
            }
            Err(e) => {
                kill_and_reap(&mut child);
                return error_output(buffer, os_error(&e, None));
            }
        }
    }
}

enum Drain {
    Eof,
    Deadline,
}

/// Reads the pipe until EOF or the deadline. The deadline is checked on every iteration, so a
/// continuously readable pipe cannot extend it.
fn drain(
    reader: &mut std::io::PipeReader,
    buffer: &mut Vec<u8>,
    deadline: Option<Instant>,
) -> std::io::Result<Drain> {
    let mut chunk = [0_u8; 64 * 1024];
    loop {
        let left = remaining(deadline);
        if left == Some(Duration::ZERO) {
            return Ok(Drain::Deadline);
        }
        let timeout = match left {
            // Round up to whole milliseconds so a sub-millisecond wait does not spin.
            Some(r) => {
                PollTimeout::try_from(r.as_millis().saturating_add(1).min(i32::MAX as u128) as i32)
                    .unwrap_or(PollTimeout::MAX)
            }
            None => PollTimeout::NONE,
        };
        let mut fds = [PollFd::new(reader.as_fd(), PollFlags::POLLIN)];
        match poll(&mut fds, timeout) {
            Ok(0) | Err(Errno::EINTR) => continue,
            Ok(_) => {}
            Err(e) => return Err(e.into()),
        }
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => return Ok(Drain::Eof),
                Ok(n) => buffer.extend_from_slice(chunk.get(..n).unwrap_or_default()),
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
            if remaining(deadline) == Some(Duration::ZERO) {
                return Ok(Drain::Deadline);
            }
        }
    }
}

fn remaining(deadline: Option<Instant>) -> Option<Duration> {
    deadline.map(|d| d.saturating_duration_since(Instant::now()))
}

fn set_nonblocking(reader: &std::io::PipeReader) -> nix::Result<()> {
    let flags = OFlag::from_bits_truncate(fcntl(reader, FcntlArg::F_GETFL)?);
    fcntl(reader, FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK)).map(|_| ())
}

/// Kills the process group and the shell itself, then reaps the shell.
fn kill_and_reap(child: &mut Child) {
    if let Ok(pid) = i32::try_from(child.id()) {
        match killpg(Pid::from_raw(pid), Signal::SIGKILL) {
            Ok(()) | Err(Errno::ESRCH) => {}
            Err(e) => log_warning(&format!("killpg({pid}) failed: {e}")),
        }
    }
    // The shell may have left its group; Python would hang here (plan §2.4).
    let _ = child.kill();
    let _ = child.wait();
}

fn timed_out(
    child: &mut Child,
    reader: &mut std::io::PipeReader,
    mut buffer: Vec<u8>,
    command: &str,
    timeout: Duration,
) -> ExecOutput {
    kill_and_reap(child);
    // Keep what is already readable, for a bounded time; then our read end is dropped.
    let _ = drain(reader, &mut buffer, Instant::now().checked_add(GRACE));
    let message = format!(
        "Command '{command}' timed out after {} seconds",
        timeout.as_secs()
    );
    error_output(buffer, ("TimeoutExpired".to_owned(), message))
}

fn error_output(buffer: Vec<u8>, (exception_type, message): (String, String)) -> ExecOutput {
    ExecOutput {
        output: decode(&buffer),
        returncode: -1,
        exception_info: format!("An error occurred while executing the command: {message}"),
        extra: Some(
            json!({"exception_type": exception_type, "exception": message})
                .as_object()
                .cloned()
                .unwrap_or_default(),
        ),
    }
}

/// UTF-8 with replacement, then universal newlines (`\r\n` and lone `\r` become `\n`).
fn decode(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            out.push('\n');
        } else {
            out.push(c);
        }
    }
    out
}

/// The error a failed spawn reports. A missing or non-directory cwd names the cwd, as
/// CPython's `Popen` does; anything else names the shell.
fn spawn_error(error: &std::io::Error, cwd: &Path) -> (String, String) {
    let cwd_is_bad = !cwd.is_dir();
    let filename = if cwd_is_bad { cwd } else { Path::new(SHELL) };
    os_error(error, Some(filename))
}

/// CPython's exception type and `str(e)` for an OS error.
fn os_error(error: &std::io::Error, filename: Option<&Path>) -> (String, String) {
    let Some(code) = error.raw_os_error() else {
        return ("OSError".to_owned(), error.to_string());
    };
    let errno = Errno::from_raw(code);
    let exception_type = match errno {
        Errno::ENOENT => "FileNotFoundError",
        Errno::EACCES | Errno::EPERM => "PermissionError",
        Errno::ENOTDIR => "NotADirectoryError",
        Errno::EISDIR => "IsADirectoryError",
        _ => "OSError",
    };
    let mut message = format!("[Errno {code}] {}", errno.desc());
    if let Some(filename) = filename {
        message.push_str(": ");
        message.push_str(&python_repr(&filename.to_string_lossy()));
    }
    (exception_type.to_owned(), message)
}

/// Python's `repr(str)` for the common cases: single quotes unless the text has a single quote
/// and no double quote; backslash escapes for quotes, `\\`, `\n`, `\r`, `\t` and control bytes.
fn python_repr(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(text.len() + 2);
    out.push(quote);
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02x}", c as u32))
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

fn log_warning(message: &str) {
    use std::io::Write;
    let _ = writeln!(std::io::stderr(), "warning: {message}");
}

#[cfg(test)]
mod tests {
    use super::{decode, python_repr};

    #[test]
    fn decode_translates_newlines_and_replaces_bad_bytes() {
        assert_eq!(decode(b"a\r\nb\rc\n\xff"), "a\nb\nc\n\u{fffd}");
        assert_eq!(decode(b"\r\r\n"), "\n\n");
    }

    #[test]
    fn repr_like_python() {
        assert_eq!(python_repr("/x"), "'/x'");
        assert_eq!(python_repr("it's"), "\"it's\"");
        assert_eq!(python_repr("a'b\"c"), "'a\\'b\"c'");
        assert_eq!(python_repr("a\nb"), "'a\\nb'");
    }
}
