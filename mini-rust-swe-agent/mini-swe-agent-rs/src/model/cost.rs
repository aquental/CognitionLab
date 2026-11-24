//! Cost of a completion (plan §3.8 item 4): a supported subset of LiteLLM's cost calculator.
//!
//! `prompt_tokens × input_cost_per_token + completion_tokens × output_cost_per_token`, with the
//! prices from a LiteLLM-format registry JSON (`litellm_model_registry` or
//! `LITELLM_MODEL_REGISTRY_PATH`). The model is looked up by its exact name, then without its
//! `provider/` prefix. No prices are bundled (plan §2.3).

use std::path::Path;

use serde_json::{Map, Value};

/// Model prices, by model name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Registry(Map<String, Value>);

impl Registry {
    /// Reads a registry file. Python registers the file only when it exists, so a missing file
    /// is `Ok(None)`; an unreadable or invalid one is an error.
    pub fn load(path: &Path) -> Result<Option<Self>, String> {
        if !path.is_file() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        match serde_json::from_str(&text) {
            Ok(Value::Object(map)) => Ok(Some(Self(map))),
            Ok(_) => Err(format!(
                "{}: the registry is not a JSON object",
                path.display()
            )),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    fn entry(&self, model: &str) -> Option<&Map<String, Value>> {
        self.0
            .get(model)
            .or_else(|| model.split_once('/').and_then(|(_, bare)| self.0.get(bare)))
            .and_then(Value::as_object)
    }
}

/// The cost of one completion, or why it cannot be computed. A cost that is not positive and
/// finite is an error, as `_calculate_cost` requires `cost > 0.0`.
pub fn completion_cost(
    registry: Option<&Registry>,
    model: &str,
    usage: Option<(u64, u64)>,
) -> Result<f64, String> {
    let registry = registry.ok_or("no model registry was loaded")?;
    let entry = registry
        .entry(model)
        .ok_or_else(|| format!("model {model} is not in the registry"))?;
    // A negative or non-finite price would silently lower the total, so it is refused.
    let price = |key: &str| {
        entry
            .get(key)
            .and_then(Value::as_f64)
            .ok_or_else(|| format!("the registry has no {key} for {model}"))
            .and_then(|p| {
                if p.is_finite() && p >= 0.0 {
                    Ok(p)
                } else {
                    Err(format!("the registry's {key} for {model} is invalid: {p}"))
                }
            })
    };
    let (prompt, completion) = usage.ok_or("the response has no token usage")?;
    #[allow(clippy::cast_precision_loss)]
    let cost = prompt as f64 * price("input_cost_per_token")?
        + completion as f64 * price("output_cost_per_token")?;
    if cost.is_finite() && cost > 0.0 {
        Ok(cost)
    } else {
        Err(format!("Cost must be > 0.0, got {cost}"))
    }
}
