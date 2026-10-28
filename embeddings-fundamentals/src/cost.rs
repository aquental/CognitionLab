//! Provider cost estimates (PRD §5.5, §11).
//!
//! Prices are recorded per unit quantity exactly as published (for example USD per
//! 1,000,000 tokens), and the estimate is `usage / unit_quantity × unit_price`. A
//! missing usage figure or price yields a null cost with the reason, never zero.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::domain::Execution;

/// A versioned provider price table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PriceTable {
    /// Version label.
    pub version: String,
    /// Provider.
    pub provider: String,
    /// Model the price applies to.
    pub model: String,
    /// ISO currency code.
    pub currency: String,
    /// Billing unit, for example `token`.
    pub unit: String,
    /// Units the price is quoted for, for example 1,000,000.
    pub unit_quantity: u64,
    /// Price of `unit_quantity` units.
    pub unit_price: f64,
    /// Where the price was taken from.
    pub source: String,
}

/// Price table errors.
#[derive(Debug, thiserror::Error)]
pub enum PriceError {
    /// The file cannot be read.
    #[error("cannot read price table {path}: {source}")]
    Read {
        /// Path.
        path: String,
        /// I/O error.
        #[source]
        source: std::io::Error,
    },
    /// The file is invalid.
    #[error("invalid price table {path}: {reason}")]
    Invalid {
        /// Path.
        path: String,
        /// Reason.
        reason: String,
    },
}

impl PriceTable {
    /// Loads and validates a price table (positive unit quantity, finite non-negative
    /// price).
    ///
    /// # Errors
    ///
    /// Returns [`PriceError`] for unreadable or invalid files.
    pub fn load(path: &Path) -> Result<Self, PriceError> {
        let shown = path.display().to_string();
        let text = std::fs::read_to_string(path).map_err(|source| PriceError::Read {
            path: shown.clone(),
            source,
        })?;
        let table: Self = toml::from_str(&text).map_err(|e| PriceError::Invalid {
            path: shown.clone(),
            reason: e.to_string(),
        })?;
        if table.unit_quantity == 0 || !table.unit_price.is_finite() || table.unit_price < 0.0 {
            return Err(PriceError::Invalid {
                path: shown,
                reason: "unit_quantity must be positive and unit_price finite and >= 0".into(),
            });
        }
        Ok(table)
    }
}

/// Whether a cost could be estimated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EstimateStatus {
    /// Estimated from reported usage and a price table.
    Estimated,
    /// Usage or price is missing; the cost is unknown.
    Unavailable,
    /// A local engine: no provider is billed.
    NotApplicable,
}

/// A provider cost estimate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostEstimate {
    /// Status.
    pub estimate_status: EstimateStatus,
    /// Estimated cost; `None` unless estimated (unknown is never zero).
    pub estimated_cost: Option<f64>,
    /// Why the cost is not estimated.
    pub reason: Option<String>,
    /// Currency of the price table.
    pub currency: Option<String>,
    /// Price table version.
    pub price_version: Option<String>,
    /// Billing unit.
    pub usage_unit: Option<String>,
    /// Units reported by the provider.
    pub usage: Option<u64>,
    /// Units the price is quoted for.
    pub unit_quantity: Option<u64>,
    /// Price of `unit_quantity` units.
    pub unit_price: Option<f64>,
}

#[expect(
    clippy::cast_precision_loss,
    reason = "token counts are far below 2^52, so the conversion is exact"
)]
const fn exact_f64(n: u64) -> f64 {
    n as f64
}

/// Estimates the provider cost of `usage` units for an engine that runs at
/// `execution`.
#[must_use]
pub fn estimate(
    execution: Execution,
    usage: Option<u64>,
    price: Option<&PriceTable>,
) -> CostEstimate {
    let empty = |status, reason: &str| CostEstimate {
        estimate_status: status,
        estimated_cost: None,
        reason: Some(reason.to_owned()),
        currency: price.map(|p| p.currency.clone()),
        price_version: price.map(|p| p.version.clone()),
        usage_unit: price.map(|p| p.unit.clone()),
        usage,
        unit_quantity: price.map(|p| p.unit_quantity),
        unit_price: price.map(|p| p.unit_price),
    };
    if execution == Execution::Local {
        return empty(
            EstimateStatus::NotApplicable,
            "local engine: no provider is billed",
        );
    }
    match (usage, price) {
        (None, _) => empty(
            EstimateStatus::Unavailable,
            "missing provider usage: the provider did not report billed units",
        ),
        (_, None) => empty(
            EstimateStatus::Unavailable,
            "missing pricing data: no price table for this provider and model",
        ),
        (Some(units), Some(table)) => CostEstimate {
            estimate_status: EstimateStatus::Estimated,
            estimated_cost: Some(
                exact_f64(units) / exact_f64(table.unit_quantity) * table.unit_price,
            ),
            reason: None,
            currency: Some(table.currency.clone()),
            price_version: Some(table.version.clone()),
            usage_unit: Some(table.unit.clone()),
            usage: Some(units),
            unit_quantity: Some(table.unit_quantity),
            unit_price: Some(table.unit_price),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    fn table() -> PriceTable {
        PriceTable {
            version: "p1".into(),
            provider: "openai".into(),
            model: "text-embedding-3-small".into(),
            currency: "USD".into(),
            unit: "token".into(),
            unit_quantity: 1_000_000,
            unit_price: 0.02,
            source: "test".into(),
        }
    }

    #[test]
    fn a_thousand_tokens_cost_a_millionth_of_the_quoted_price_times_a_thousand() {
        let cost = estimate(Execution::Remote, Some(1_000), Some(&table()));
        assert_eq!(cost.estimate_status, EstimateStatus::Estimated);
        // 1,000 / 1,000,000 × 0.02 = 0.00002; a missing scale would give 20.
        assert_abs_diff_eq!(
            cost.estimated_cost.expect("cost"),
            0.000_02,
            epsilon = 1e-15
        );
    }

    #[test]
    fn unknown_costs_are_null_with_a_reason_never_zero() {
        for cost in [
            estimate(Execution::Remote, None, Some(&table())),
            estimate(Execution::Remote, Some(10), None),
        ] {
            assert_eq!(cost.estimate_status, EstimateStatus::Unavailable);
            assert_eq!(cost.estimated_cost, None);
            assert!(
                cost.reason
                    .as_deref()
                    .is_some_and(|r| r.starts_with("missing"))
            );
        }
        let local = estimate(Execution::Local, None, None);
        assert_eq!(local.estimate_status, EstimateStatus::NotApplicable);
        assert_eq!(local.estimated_cost, None);
    }

    #[test]
    fn the_checked_in_price_table_loads() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("config/prices/openai-embeddings-2026-10.toml");
        let table = PriceTable::load(&path).expect("price table");
        assert_eq!(table.unit_quantity, 1_000_000);
    }
}
