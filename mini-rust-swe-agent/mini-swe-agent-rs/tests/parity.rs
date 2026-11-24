//! Differential parity through the library (spec/migration-plan.md §3.15 item 4, phase P6):
//! every scenario in `parity/scenarios/` must give equal normalized results in Python and Rust.

mod support;

use std::collections::BTreeSet;

#[test]
fn scenario_files_are_well_formed() {
    let scenarios = support::load_scenarios();
    assert!(scenarios.len() >= 20, "{} scenarios", scenarios.len());
    let mut ids = BTreeSet::new();
    for scenario in &scenarios {
        let id = scenario["id"].as_str().unwrap();
        assert!(ids.insert(id.to_owned()), "duplicate id {id}");
        assert!(scenario["description"].is_string(), "{id}");
        match scenario["kind"].as_str() {
            Some("agent") => {
                assert!(scenario["task"].is_string(), "{id}");
                assert!(scenario["outputs"].is_array(), "{id}");
                assert!(scenario["expect"]["exit_status"].is_string(), "{id}");
                assert!(scenario["expect"]["queries"].is_u64(), "{id}");
            }
            Some("environment") => assert!(scenario["commands"].is_array(), "{id}"),
            other => panic!("{id}: unknown kind {other:?}"),
        }
    }
}

#[test]
fn python_and_rust_agree_on_every_scenario() {
    let Some(harness) = support::Harness::new() else {
        eprintln!("skipped on purpose: MSWEA_RS_NO_REFERENCE=1");
        return;
    };
    let mut python = harness.run_python(&[]);
    let mut failures = Vec::new();
    let scenarios = support::load_scenarios();
    for scenario in &scenarios {
        let id = scenario["id"].as_str().unwrap();
        let python_result = python
            .remove(id)
            .unwrap_or_else(|| panic!("no Python result for {id}"));
        let rust_result = harness.run_rust(scenario);
        if let Err(problem) = support::check(scenario, python_result, rust_result) {
            failures.push(format!("--- {id}\n{problem}"));
        }
    }
    assert!(
        python.is_empty(),
        "Python ran unknown scenarios: {:?}",
        python.keys()
    );
    assert!(
        failures.is_empty(),
        "{} of {} scenarios differ:\n{}",
        failures.len(),
        scenarios.len(),
        failures.join("\n")
    );
}
