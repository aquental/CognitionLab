//! Retries of a model call (port of `models/utils/retry.py`: tenacity's `stop_after_attempt` and
//! `wait_exponential(multiplier=1, min=4, max=60)`, plan §3.8 item 3).

use std::time::Duration;

use crate::clock::Clock;
use crate::error::ModelError;
use crate::host_env::HostEnv;
use crate::model::openai_compat::BackendError;

/// Attempts when `MSWEA_MODEL_RETRY_STOP_AFTER_ATTEMPT` is not set.
pub const DEFAULT_ATTEMPTS: u32 = 10;

/// `MSWEA_MODEL_RETRY_STOP_AFTER_ATTEMPT` (default 10). Python's `int()` fails on bad text; so
/// does this, when the model is built. At least one attempt is always made.
pub fn attempts_from_env(host: &HostEnv) -> Result<u32, ModelError> {
    match host.get("MSWEA_MODEL_RETRY_STOP_AFTER_ATTEMPT") {
        None => Ok(DEFAULT_ATTEMPTS),
        Some(text) => text.trim().parse::<i64>().map_or_else(
            |_| {
                Err(ModelError::Settings(format!(
                    "MSWEA_MODEL_RETRY_STOP_AFTER_ATTEMPT={text:?} is not an integer"
                )))
            },
            |n| Ok(u32::try_from(n.max(1)).unwrap_or(u32::MAX)),
        ),
    }
}

/// The wait after the `failures`-th failed attempt: `clamp(2^(failures - 1), 4, 60)` seconds.
pub fn backoff(failures: u32) -> Duration {
    let exponent = failures.saturating_sub(1);
    let seconds = 1u64.checked_shl(exponent).unwrap_or(u64::MAX);
    Duration::from_secs(seconds.clamp(4, 60))
}

/// Calls `call` until it succeeds, fails with an aborting error, or `attempts` calls failed.
/// Each retry is logged as a warning, then the clock sleeps.
pub fn retry<T>(
    attempts: u32,
    clock: &dyn Clock,
    mut call: impl FnMut() -> Result<T, BackendError>,
) -> Result<T, BackendError> {
    let mut failures = 0u32;
    loop {
        match call() {
            Ok(value) => return Ok(value),
            Err(error) => {
                failures = failures.saturating_add(1);
                if error.aborts() || failures >= attempts {
                    return Err(error);
                }
                let wait = backoff(failures);
                log_warning(&format!(
                    "Retrying the model call in {} seconds as it raised {}: {error}",
                    wait.as_secs(),
                    error.exit_status()
                ));
                clock.sleep(wait);
            }
        }
    }
}

/// Python's `before_sleep_log(logger, WARNING)` on the `litellm_model` logger, to stderr.
fn log_warning(text: &str) {
    use std::io::Write;
    let _ = writeln!(std::io::stderr(), "WARNING:litellm_model:{text}");
}
