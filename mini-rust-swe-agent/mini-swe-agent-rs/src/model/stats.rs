//! `GlobalModelStats` (`models/__init__.py:13-42`): process-wide cost and call counters with
//! optional limits. The only shared mutable state; `main` creates it and passes an `Arc`.

use std::sync::{Mutex, PoisonError};

use crate::error::{AgentError, ModelError};
use crate::host_env::HostEnv;

/// Global cost and call counters.
#[derive(Debug)]
pub struct GlobalModelStats {
    totals: Mutex<(f64, u64)>,
    cost_limit: f64,
    call_limit: i64,
}

impl GlobalModelStats {
    /// Counters at zero with the given limits (`0` disables a limit).
    pub fn new(cost_limit: f64, call_limit: i64) -> Self {
        Self {
            totals: Mutex::new((0.0, 0)),
            cost_limit,
            call_limit,
        }
    }

    /// Limits from `MSWEA_GLOBAL_COST_LIMIT` and `MSWEA_GLOBAL_CALL_LIMIT` (default `0`).
    /// Python fails at import on an unparsable value; this returns an error.
    pub fn from_host_env(host: &HostEnv) -> Result<Self, AgentError> {
        let read = |name: &str| host.get(name).map(str::trim).unwrap_or("0").to_owned();
        let cost = read("MSWEA_GLOBAL_COST_LIMIT");
        let calls = read("MSWEA_GLOBAL_CALL_LIMIT");
        let cost_limit = cost
            .parse::<f64>()
            .map_err(|_| AgentError::Config(format!("MSWEA_GLOBAL_COST_LIMIT={cost:?}")))?;
        let call_limit = calls
            .parse::<i64>()
            .map_err(|_| AgentError::Config(format!("MSWEA_GLOBAL_CALL_LIMIT={calls:?}")))?;
        Ok(Self::new(cost_limit, call_limit))
    }

    /// Records one call. The counters are updated first, then the limits are checked, so the
    /// failing call is counted (`add`, `models/__init__.py:25-31`). The call check is
    /// `call_limit < n_calls + 1`, so a limit of `n` fails on the `n`-th call (trap 28).
    pub fn add(&self, cost: f64) -> Result<(), ModelError> {
        let (total, calls) = {
            let mut totals = self.totals.lock().unwrap_or_else(PoisonError::into_inner);
            totals.0 += cost;
            totals.1 = totals.1.saturating_add(1);
            *totals
        };
        let calls_after = i128::from(calls) + 1;
        if (0.0 < self.cost_limit && self.cost_limit < total)
            || (0 < self.call_limit && i128::from(self.call_limit) < calls_after)
        {
            return Err(ModelError::GlobalLimitExceeded { cost: total, calls });
        }
        Ok(())
    }

    /// Total cost.
    pub fn cost(&self) -> f64 {
        self.totals.lock().unwrap_or_else(PoisonError::into_inner).0
    }

    /// Total calls.
    pub fn n_calls(&self) -> u64 {
        self.totals.lock().unwrap_or_else(PoisonError::into_inner).1
    }

    /// The cost limit (`0` = none).
    pub fn cost_limit(&self) -> f64 {
        self.cost_limit
    }

    /// The call limit (`0` = none).
    pub fn call_limit(&self) -> i64 {
        self.call_limit
    }
}
