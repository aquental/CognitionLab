//! The injected wall clock (plan §2.2, decision D12).
//!
//! Python reads `time.time()` for limits and timestamps. `SystemClock` keeps those wall-clock
//! semantics; `ManualClock` lets tests set the time. Subprocess timeouts do not use this clock:
//! they use a monotonic `Instant` (plan §3.7).

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// A source of wall-clock time in seconds since the Unix epoch.
pub trait Clock: Send + Sync {
    /// The current time, like Python's `time.time()`.
    fn now(&self) -> f64;
}

/// The real clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> f64 {
        // A clock before 1970 reads as a negative offset, as `time.time()` would.
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(elapsed) => elapsed.as_secs_f64(),
            Err(before) => -before.duration().as_secs_f64(),
        }
    }
}

/// A clock that tests set by hand. Public on purpose: integration tests compile the library
/// without `cfg(test)`.
#[derive(Debug)]
pub struct ManualClock {
    bits: AtomicU64,
}

impl ManualClock {
    /// A clock that reads `now` until changed.
    pub fn new(now: f64) -> Self {
        Self {
            bits: AtomicU64::new(now.to_bits()),
        }
    }

    /// Sets the time.
    pub fn set(&self, now: f64) {
        self.bits.store(now.to_bits(), Ordering::SeqCst);
    }

    /// Moves the time by `seconds` (negative values move it back). One atomic
    /// read-modify-write, so concurrent advances are never lost.
    pub fn advance(&self, seconds: f64) {
        self.bits
            .update(Ordering::SeqCst, Ordering::SeqCst, |bits| {
                (f64::from_bits(bits) + seconds).to_bits()
            });
    }
}

impl Clock for ManualClock {
    fn now(&self) -> f64 {
        f64::from_bits(self.bits.load(Ordering::SeqCst))
    }
}
