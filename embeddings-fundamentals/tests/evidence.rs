//! Guards the committed experiment evidence under `reports/`: every published result must
//! parse as the current report types and follow the current measurement definitions, so
//! a change to a definition cannot leave stale numbers published (review finding G2-6).

use std::path::Path;

use embeddings_fundamentals::domain::Execution;
use embeddings_fundamentals::measurement::Observation;
use embeddings_fundamentals::report::{ComparisonReport, ConfigurationResult};

fn check(c: &ConfigurationResult) -> Result<(), String> {
    let m = &c.measurements;
    let id = &c.experiment_id;
    // PRD §9.4: cold start = first load of the fresh process + its first operation.
    if (m.cold_start.model_load_ms - m.ingestion_model_load_ms).abs() > 1e-9 {
        return Err(format!(
            "{id}: cold-start load {} is not the process's first load {}",
            m.cold_start.model_load_ms, m.ingestion_model_load_ms
        ));
    }
    if let (Some(total), Some(first)) = (m.cold_start.total_ms, m.cold_start.first_operation_ms) {
        if (total - (m.cold_start.model_load_ms + first)).abs() > 1e-9 {
            return Err(format!(
                "{id}: cold total {total} is not load + first operation"
            ));
        }
    }
    let q = &m.query_side_start;
    if (q.total_ms - (q.model_load_ms + q.first_query_ms)).abs() > 1e-9 {
        return Err(format!("{id}: query-side total"));
    }
    let Observation::Unavailable { reason } = &m.gpu_memory else {
        return Err(format!(
            "{id}: CPU-only evidence must report GPU memory as unavailable"
        ));
    };
    let expected = match c.execution {
        Execution::Local => "local CPU",
        Execution::Remote => "remote provider",
    };
    if !reason.contains(expected) {
        return Err(format!("{id}: GPU reason {reason:?}"));
    }
    Ok(())
}

#[test]
fn published_phase_2_evidence_matches_the_current_definitions() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("reports/phase-2");
    let mut results = 0;
    for entry in std::fs::read_dir(&dir).expect("reports/phase-2") {
        let path = entry.expect("entry").path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_owned();
        if !name.ends_with(".json") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read evidence");
        if name == "comparison.json" {
            let report: ComparisonReport =
                serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(
                !report.configurations.is_empty(),
                "{name}: no configurations"
            );
            for c in &report.configurations {
                check(c).unwrap_or_else(|e| panic!("{name}: {e}"));
            }
        } else {
            let result: ConfigurationResult =
                serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
            check(&result).unwrap_or_else(|e| panic!("{name}: {e}"));
            results += 1;
        }
    }
    assert_eq!(results, 3, "three published text configurations");
}
