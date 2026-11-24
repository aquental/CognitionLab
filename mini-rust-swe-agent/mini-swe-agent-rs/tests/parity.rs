//! Differential parity through the library (spec/migration-plan.md §3.15 item 4, phase P6):
//! every scenario in `parity/scenarios/` must give equal normalized results in Python and Rust.

mod support;

use std::collections::BTreeSet;

#[test]
fn scenario_files_are_well_formed() {
    let scenarios = support::load_scenarios();
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
    // The required set (plan §3.15 items 4-5), so a scenario cannot disappear with its Gherkin.
    let required: BTreeSet<String> = REQUIRED.iter().map(|id| (*id).to_owned()).collect();
    assert_eq!(ids, required);
}

const REQUIRED: &[&str] = &[
    "command_timeout_partial_output",
    "cost_limit",
    "env_missing_cwd",
    "env_signal_exit",
    "env_stateless_shell",
    "env_submission_marker",
    "env_timeout_background_child",
    "env_unicode_output",
    "failing_command",
    "format_errors_count_against_cost_limit",
    "format_errors_then_recovery",
    "global_call_limit",
    "injected_runtime_error",
    "invalid_template",
    "local_traj_seed",
    "long_output",
    "non_ascii_and_crlf_output",
    "outputs_exhausted",
    "repeated_format_errors",
    "save_on_failure",
    "save_with_extra_dicts",
    "signal_exit",
    "sleep_and_warning",
    "step_limit",
    "submit_after_steps",
    "submit_step1",
    "wall_time_limit",
];

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
