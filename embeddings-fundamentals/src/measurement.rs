//! Efficiency measurements (PRD §9.4): injectable clocks and memory probes, a measuring
//! adapter decorator, and nearest-rank latency statistics.
//!
//! Collection and computation are separate. [`MeasuredEmbedder`] records every adapter
//! call with an injected [`Clock`]; [`EmbeddingCalls::summary`] and
//! [`LatencyStats::from_samples`] turn the observations into reported values. Tests
//! inject a [`ManualClock`] and a [`FixedMemory`] probe, so expected measurements can
//! be derived by hand.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::embedding::{
    AssetRef, Capabilities, EmbeddingBatch, EmbeddingError, EmbeddingStrategy, InputKind,
    ModelInfo, TextInput, TokenCounter,
};

/// A monotonic clock: time elapsed since an arbitrary, fixed origin.
pub trait Clock: Send + Sync {
    /// Elapsed time since the clock's origin.
    fn now(&self) -> Duration;
}

/// The process's monotonic clock.
#[derive(Debug, Clone, Copy)]
pub struct SystemClock {
    origin: Instant,
}

impl SystemClock {
    /// Starts a clock at the current instant.
    #[must_use]
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.origin.elapsed()
    }
}

/// A test clock that moves only when advanced. Controlled components advance it while
/// they do their work (an adapter call, a model load), so an interval measures exactly
/// the work done inside it, and nothing when the work happened outside it. Reading the
/// clock never moves it.
#[derive(Debug, Default)]
pub struct ManualClock {
    now_micros: AtomicU64,
}

impl ManualClock {
    /// A clock at time zero.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Moves the clock forward by `by` (saturating at `u64::MAX` microseconds).
    pub fn advance(&self, by: Duration) {
        let micros = u64::try_from(by.as_micros()).unwrap_or(u64::MAX);
        self.now_micros
            .update(Ordering::SeqCst, Ordering::SeqCst, |now| {
                now.saturating_add(micros)
            });
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Duration {
        Duration::from_micros(self.now_micros.load(Ordering::SeqCst))
    }
}

/// Reads the process's peak resident memory.
pub trait MemoryProbe: Send + Sync {
    /// High-water mark of resident memory in bytes, when observable.
    fn peak_rss_bytes(&self) -> Option<u64>;
}

/// `getrusage(RUSAGE_SELF).ru_maxrss`, normalized to bytes (macOS reports bytes,
/// Linux and the BSDs report KiB).
#[derive(Debug, Clone, Copy, Default)]
pub struct RusageProbe;

impl MemoryProbe for RusageProbe {
    fn peak_rss_bytes(&self) -> Option<u64> {
        let usage =
            nix::sys::resource::getrusage(nix::sys::resource::UsageWho::RUSAGE_SELF).ok()?;
        let raw = u64::try_from(usage.max_rss()).ok()?;
        if cfg!(target_os = "macos") {
            Some(raw)
        } else {
            raw.checked_mul(1024)
        }
    }
}

/// A probe returning a fixed value (tests).
#[derive(Debug, Clone, Copy)]
pub struct FixedMemory(pub Option<u64>);

impl MemoryProbe for FixedMemory {
    fn peak_rss_bytes(&self) -> Option<u64> {
        self.0
    }
}

/// A measured value, or the honest reason it is not available.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Observation {
    /// A measured value with its unit.
    Measured {
        /// Value.
        value: f64,
        /// Unit, for example `bytes` or `ms`.
        unit: String,
        /// What exactly was measured.
        scope: String,
    },
    /// Not observable in this run.
    Unavailable {
        /// Why.
        reason: String,
    },
}

impl Observation {
    /// A value in bytes.
    #[must_use]
    pub fn bytes(value: u64, scope: &str) -> Self {
        Self::Measured {
            value: u64_f64(value),
            unit: "bytes".to_owned(),
            scope: scope.to_owned(),
        }
    }

    /// Not available, with a reason.
    #[must_use]
    pub fn unavailable(reason: &str) -> Self {
        Self::Unavailable {
            reason: reason.to_owned(),
        }
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "measurement magnitudes (bytes, counts) are far below 2^52"
)]
const fn u64_f64(n: u64) -> f64 {
    n as f64
}

/// Milliseconds of a duration.
#[must_use]
pub fn millis(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// Nearest-rank percentile `p` (0 < p ≤ 100) of `samples`: the value at 1-based rank
/// `ceil(p / 100 × n)` after sorting. `None` for no samples.
#[must_use]
pub fn nearest_rank(samples: &[f64], p: f64) -> Option<f64> {
    if samples.is_empty() || !(p > 0.0 && p <= 100.0) {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let n = u64_f64(u64::try_from(sorted.len()).ok()?);
    let rank = (p / 100.0 * n).ceil();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "rank is in 1..=n, a small positive integer"
    )]
    let index = (rank as usize).clamp(1, sorted.len()) - 1;
    sorted.get(index).copied()
}

/// Latency statistics over pooled warm samples.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LatencyStats {
    /// Number of samples.
    pub samples: usize,
    /// Nearest-rank median in milliseconds.
    pub median_ms: f64,
    /// Nearest-rank p95 in milliseconds.
    pub p95_ms: f64,
}

impl LatencyStats {
    /// Statistics of `samples_ms`; `None` when there are no samples.
    #[must_use]
    pub fn from_samples(samples_ms: &[f64]) -> Option<Self> {
        Some(Self {
            samples: samples_ms.len(),
            median_ms: nearest_rank(samples_ms, 50.0)?,
            p95_ms: nearest_rank(samples_ms, 95.0)?,
        })
    }
}

/// One observed adapter call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallRecord {
    /// Documents or queries.
    pub kind: InputKind,
    /// Inputs in the call.
    pub items: usize,
    /// Wall time of the call.
    pub duration: Duration,
    /// Whether the call succeeded.
    pub ok: bool,
    /// Provider usage reported by the call.
    pub usage_tokens: Option<u64>,
}

/// Shared log of adapter calls.
#[derive(Debug, Clone, Default)]
pub struct EmbeddingCalls(Arc<Mutex<Vec<CallRecord>>>);

/// Embedding throughput over a set of calls.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingSummary {
    /// Calls made, including failed attempts.
    pub calls: usize,
    /// Failed calls.
    pub failed_calls: usize,
    /// Inputs embedded by successful calls.
    pub items: usize,
    /// Total time inside the adapter, in milliseconds.
    pub total_ms: f64,
    /// Mean latency of one call (one batch), in milliseconds.
    pub mean_batch_latency_ms: f64,
    /// Inputs per second inside the adapter.
    pub items_per_second: f64,
    /// Provider usage summed over successful calls; `None` unless every successful call
    /// that embedded something reported it (a partial sum is never an estimate).
    pub usage_tokens: Option<u64>,
}

impl EmbeddingCalls {
    fn push(&self, record: CallRecord) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(record);
    }

    /// Every call observed so far.
    #[must_use]
    pub fn records(&self) -> Vec<CallRecord> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Clears the log (for example between ingestion and querying).
    pub fn clear(&self) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }

    /// Summarizes the calls of `kind`.
    #[must_use]
    pub fn summary(&self, kind: InputKind) -> EmbeddingSummary {
        let records: Vec<CallRecord> = self
            .records()
            .into_iter()
            .filter(|r| r.kind == kind)
            .collect();
        let ok: Vec<&CallRecord> = records.iter().filter(|r| r.ok).collect();
        let total: Duration = records.iter().map(|r| r.duration).sum();
        let items: usize = ok.iter().map(|r| r.items).sum();
        let calls = records.len();
        let total_ms = millis(total);
        // Usage is known only when every successful call that embedded something reported
        // it; a partial sum would present an invented total as an estimate.
        let usage_complete = ok.iter().all(|r| r.items == 0 || r.usage_tokens.is_some());
        let reported: Vec<u64> = ok.iter().filter_map(|r| r.usage_tokens).collect();
        EmbeddingSummary {
            calls,
            failed_calls: calls - ok.len(),
            items,
            total_ms,
            mean_batch_latency_ms: if calls == 0 {
                0.0
            } else {
                total_ms / u64_f64(u64::try_from(calls).unwrap_or(u64::MAX))
            },
            items_per_second: if total.is_zero() {
                0.0
            } else {
                u64_f64(u64::try_from(items).unwrap_or(u64::MAX)) / total.as_secs_f64()
            },
            usage_tokens: (usage_complete && !reported.is_empty()).then(|| reported.iter().sum()),
        }
    }
}

/// Decorator that times every adapter call. It reports the wrapped adapter's model
/// information unchanged, so records and spaces are identical with or without it.
///
/// Generic over the clock (static dispatch, PLAN §2.2); it is itself an
/// [`EmbeddingStrategy`] so it can stand in for the selected `dyn` adapter.
pub struct MeasuredEmbedder<C> {
    inner: Arc<dyn EmbeddingStrategy>,
    clock: Arc<C>,
    calls: EmbeddingCalls,
}

impl<C: Clock> MeasuredEmbedder<C> {
    /// Wraps `inner`, reading `clock` around each call.
    #[must_use]
    pub fn new(inner: Arc<dyn EmbeddingStrategy>, clock: Arc<C>) -> Self {
        Self {
            inner,
            clock,
            calls: EmbeddingCalls::default(),
        }
    }

    /// The shared call log.
    #[must_use]
    pub fn calls(&self) -> EmbeddingCalls {
        self.calls.clone()
    }

    fn observe(
        &self,
        kind: InputKind,
        items: usize,
        start: Duration,
        outcome: &Result<EmbeddingBatch, EmbeddingError>,
    ) {
        let duration = self.clock.now().saturating_sub(start);
        self.calls.push(CallRecord {
            kind,
            items,
            duration,
            ok: outcome.is_ok(),
            usage_tokens: outcome
                .as_ref()
                .ok()
                .and_then(|b| b.usage)
                .map(|u| u.tokens),
        });
    }
}

impl<C> std::fmt::Debug for MeasuredEmbedder<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MeasuredEmbedder")
            .field("model", self.inner.model_info())
            .finish_non_exhaustive()
    }
}

#[async_trait::async_trait]
impl<C: Clock + 'static> EmbeddingStrategy for MeasuredEmbedder<C> {
    fn capabilities(&self) -> &Capabilities {
        self.inner.capabilities()
    }

    fn model_info(&self) -> &ModelInfo {
        self.inner.model_info()
    }

    fn token_counter(&self) -> &dyn TokenCounter {
        self.inner.token_counter()
    }

    fn input_budget(&self, kind: InputKind) -> Option<usize> {
        self.inner.input_budget(kind)
    }

    async fn embed_documents(&self, raw: &[TextInput]) -> Result<EmbeddingBatch, EmbeddingError> {
        let start = self.clock.now();
        let outcome = self.inner.embed_documents(raw).await;
        self.observe(InputKind::Document, raw.len(), start, &outcome);
        outcome
    }

    async fn embed_queries(&self, raw: &[TextInput]) -> Result<EmbeddingBatch, EmbeddingError> {
        let start = self.clock.now();
        let outcome = self.inner.embed_queries(raw).await;
        self.observe(InputKind::Query, raw.len(), start, &outcome);
        outcome
    }

    async fn embed_images(&self, assets: &[AssetRef]) -> Result<EmbeddingBatch, EmbeddingError> {
        // Image runs arrive with Milestone 3; they are passed through unmeasured.
        self.inner.embed_images(assets).await
    }
}

/// Collected hardware facts for reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hardware {
    /// Operating system.
    pub os: String,
    /// CPU architecture.
    pub arch: String,
    /// CPU model, when known.
    pub cpu: Option<String>,
    /// Logical CPUs available to the process.
    pub logical_cpus: Option<usize>,
    /// Physical memory in bytes, when known.
    pub memory_bytes: Option<u64>,
    /// Accelerator used for inference.
    pub accelerator: String,
}

/// Upper bound on one hardware probe subprocess (`sysctl`).
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Runs `program args` without blocking a runtime worker and returns its trimmed
/// stdout, or `None` when it fails, prints nothing, or exceeds `limit` (the child is
/// killed when the timeout drops it).
async fn probe_output(program: &str, args: &[&str], limit: std::time::Duration) -> Option<String> {
    let child = tokio::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .output();
    let output = tokio::time::timeout(limit, child).await.ok()?.ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|s| !s.is_empty())
}

impl Hardware {
    /// Combines two descriptions of the machine a measurement ran on. The stable
    /// identity (OS, architecture, logical CPUs, accelerator) must be equal; a probed
    /// field (`cpu`, `memory_bytes`) that one side could not read is unknown, not
    /// different, and is taken from the other side. Returns `None` when the two
    /// describe different machines.
    #[must_use]
    pub fn merge(&self, other: &Self) -> Option<Self> {
        fn probed<T: Clone + PartialEq>(a: Option<&T>, b: Option<&T>) -> Option<Option<T>> {
            match (a, b) {
                (Some(a), Some(b)) if a != b => None,
                (a, b) => Some(a.or(b).cloned()),
            }
        }
        let same = self.os == other.os
            && self.arch == other.arch
            && self.logical_cpus == other.logical_cpus
            && self.accelerator == other.accelerator;
        same.then_some(())?;
        Some(Self {
            cpu: probed(self.cpu.as_ref(), other.cpu.as_ref())?,
            memory_bytes: probed(self.memory_bytes.as_ref(), other.memory_bytes.as_ref())?,
            ..self.clone()
        })
    }

    /// Detects the current machine (best effort; unknown fields stay `None`). Engines
    /// in this build run on the CPU. Probes run as async subprocesses with a timeout,
    /// and `/proc` is read through `tokio::fs`.
    pub async fn detect() -> Self {
        let sysctl = async |key: &str| probe_output("sysctl", &["-n", key], PROBE_TIMEOUT).await;
        let proc_field = async |file: &str, key: &str| {
            tokio::fs::read_to_string(file).await.ok().and_then(|t| {
                t.lines()
                    .find(|l| l.starts_with(key))
                    .and_then(|l| l.split_once(':'))
                    .map(|(_, v)| v.trim().to_owned())
            })
        };
        let (cpu, memory_bytes) = if cfg!(target_os = "macos") {
            (
                sysctl("machdep.cpu.brand_string").await,
                sysctl("hw.memsize").await.and_then(|m| m.parse().ok()),
            )
        } else {
            (
                proc_field("/proc/cpuinfo", "model name").await,
                proc_field("/proc/meminfo", "MemTotal")
                    .await
                    .and_then(|m| m.trim_end_matches("kB").trim().parse::<u64>().ok())
                    .and_then(|kib| kib.checked_mul(1024)),
            )
        };
        Self {
            os: std::env::consts::OS.to_owned(),
            arch: std::env::consts::ARCH.to_owned(),
            cpu,
            logical_cpus: std::thread::available_parallelism()
                .ok()
                .map(std::num::NonZeroUsize::get),
            memory_bytes,
            accelerator: "cpu".to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedding::deterministic::DeterministicEmbedder;

    #[test]
    fn a_probe_field_that_could_not_be_read_is_unknown_not_different() {
        // Regression (G2-8): a timed-out `sysctl` leaves `cpu`/`memory_bytes` unset;
        // that must not make two runs on the same machine incomparable.
        let full = Hardware {
            os: "macos".into(),
            arch: "aarch64".into(),
            cpu: Some("Apple M3".into()),
            logical_cpus: Some(8),
            memory_bytes: Some(16),
            accelerator: "cpu".into(),
        };
        let probe_timed_out = Hardware {
            cpu: None,
            memory_bytes: None,
            ..full.clone()
        };
        assert_eq!(probe_timed_out.merge(&full), Some(full.clone()));
        assert_eq!(full.merge(&probe_timed_out), Some(full.clone()));
        let other_cpu = Hardware {
            cpu: Some("Apple M3 Pro".into()),
            ..full.clone()
        };
        assert_eq!(full.merge(&other_cpu), None);
        let other_cores = Hardware {
            logical_cpus: Some(12),
            ..probe_timed_out.clone()
        };
        assert_eq!(full.merge(&other_cores), None);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_stalled_hardware_probe_times_out_without_blocking_the_runtime() {
        // Regression (C2-12): a probe that never answers must not stall the single
        // runtime worker, and detection gives up at its timeout.
        let limit = std::time::Duration::from_millis(200);
        let started = std::time::Instant::now();
        let ticks = async {
            let mut n = 0_u32;
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(20));
            loop {
                interval.tick().await;
                n += 1;
                if n == 5 {
                    break n;
                }
            }
        };
        let (output, n) = tokio::join!(probe_output("sleep", &["30"], limit), ticks);
        assert_eq!(output, None, "a timed-out probe reports nothing");
        assert_eq!(n, 5, "the concurrent timer kept running");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "detection must stop at its timeout, took {:?}",
            started.elapsed()
        );
        // Answering probes get the production limit, so a loaded test machine cannot
        // turn a slow process start into a false timeout.
        assert_eq!(
            probe_output("sh", &["-c", "echo ' x '"], PROBE_TIMEOUT)
                .await
                .as_deref(),
            Some("x")
        );
        assert_eq!(
            probe_output("sh", &["-c", "exit 1"], PROBE_TIMEOUT).await,
            None
        );
    }
    use approx::assert_abs_diff_eq;

    #[test]
    fn nearest_rank_matches_the_definition() {
        let samples: Vec<f64> = (1..=20).map(f64::from).collect();
        // n = 20: median rank ceil(10) = 10, p95 rank ceil(19) = 19.
        assert_eq!(nearest_rank(&samples, 50.0), Some(10.0));
        assert_eq!(nearest_rank(&samples, 95.0), Some(19.0));
        let odd = [5.0, 1.0, 3.0];
        assert_eq!(nearest_rank(&odd, 50.0), Some(3.0), "rank ceil(1.5) = 2");
        assert_eq!(nearest_rank(&odd, 95.0), Some(5.0));
        assert_eq!(nearest_rank(&[], 50.0), None);
        assert_eq!(nearest_rank(&odd, 0.0), None);
    }

    fn call(items: usize, ok: bool, usage: Option<u64>) -> CallRecord {
        CallRecord {
            kind: InputKind::Document,
            items,
            duration: Duration::from_millis(1),
            ok,
            usage_tokens: usage,
        }
    }

    #[test]
    fn usage_is_unknown_when_any_successful_call_omits_it() {
        // Regression (G2-1): one billed batch without usage next to one with usage.
        let calls = EmbeddingCalls::default();
        calls.push(call(5, true, Some(100)));
        calls.push(call(5, true, None));
        assert_eq!(calls.summary(InputKind::Document).usage_tokens, None);

        // Failed calls and empty calls do not make usage unknown.
        let calls = EmbeddingCalls::default();
        calls.push(call(5, true, Some(100)));
        calls.push(call(5, false, None));
        calls.push(call(0, true, None));
        calls.push(call(3, true, Some(20)));
        assert_eq!(calls.summary(InputKind::Document).usage_tokens, Some(120));

        // No call reported usage at all (local engines).
        let calls = EmbeddingCalls::default();
        calls.push(call(5, true, None));
        assert_eq!(calls.summary(InputKind::Document).usage_tokens, None);
    }

    #[test]
    fn manual_clock_moves_only_when_advanced() {
        let clock = ManualClock::new();
        let start = clock.now();
        assert_eq!(clock.now(), start, "reading never moves the clock");
        clock.advance(Duration::from_millis(10));
        assert_eq!(clock.now() - start, Duration::from_millis(10));
    }

    #[tokio::test]
    async fn the_decorator_times_calls_and_keeps_the_space() {
        // The adapter's work advances the clock (5 ms per call); a decorator that read
        // its start after the inner call returned would measure 0 ms (C2-18).
        let clock = Arc::new(ManualClock::new());
        let inner: Arc<dyn EmbeddingStrategy> = Arc::new(
            DeterministicEmbedder::builder()
                .work(
                    Arc::clone(&clock),
                    crate::embedding::deterministic::WorkCost {
                        per_call: Duration::from_millis(5),
                        per_item: Duration::ZERO,
                    },
                )
                .build()
                .expect("adapter"),
        );
        let measured = MeasuredEmbedder::new(Arc::clone(&inner), clock);
        assert_eq!(measured.model_info(), inner.model_info());
        let input = |id: &str| TextInput {
            id: id.parse().expect("id"),
            text: "dois tokens".into(),
        };
        measured
            .embed_documents(&[input("a"), input("b")])
            .await
            .expect("docs");
        measured.embed_queries(&[input("q")]).await.expect("query");
        let docs = measured.calls().summary(InputKind::Document);
        assert_eq!((docs.calls, docs.items), (1, 2));
        assert_abs_diff_eq!(docs.total_ms, 5.0, epsilon = 1e-9);
        assert_abs_diff_eq!(docs.items_per_second, 400.0, epsilon = 1e-9);
        assert_eq!(
            docs.usage_tokens,
            Some(4),
            "whitespace tokens of both inputs"
        );
        assert_eq!(measured.calls().summary(InputKind::Query).calls, 1);
    }

    #[test]
    fn peak_rss_is_observable_here() {
        let peak = RusageProbe.peak_rss_bytes().expect("getrusage");
        assert!(
            peak > 1024 * 1024,
            "a test process uses more than 1 MiB: {peak}"
        );
    }
}
