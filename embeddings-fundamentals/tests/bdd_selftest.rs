//! Self-test of the BDD runner (PLAN Phase 0, PRD §5.7).
//!
//! Proves, without making the real suite red, that the runner fails on undefined,
//! ambiguous, and failing steps, on an empty or filtered required suite, on a missing
//! `Examples` row, and on a weakened step, and that it passes when the inventory is
//! complete. Each case writes its own PRD and feature files into a temporary directory.

mod support;

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context, Result};
use cucumber::{World, given, then};

use support::runner::{Suite, run_suite};

#[derive(Debug, Default, World)]
pub struct SelfTestWorld {
    value: Option<String>,
}

#[given("a defined step")]
fn defined(_: &mut SelfTestWorld) {}

#[given(expr = "the value {string}")]
fn set_value(world: &mut SelfTestWorld, value: String) {
    world.value = Some(value);
}

#[then(expr = "the value is {string}")]
fn value_is(world: &mut SelfTestWorld, expected: String) -> Result<(), String> {
    match world.value.as_deref() {
        Some(actual) if actual == expected => Ok(()),
        actual => Err(format!("expected value {expected:?}, got {actual:?}")),
    }
}

#[then("the step fails")]
fn fails(_: &mut SelfTestWorld) -> Result<(), String> {
    Err("intentional failure".to_owned())
}

#[given(regex = r"^an ambiguous step$")]
fn ambiguous_regex(_: &mut SelfTestWorld) {}

#[given(expr = "an ambiguous step")]
fn ambiguous_expr(_: &mut SelfTestWorld) {}

const PRD_BASIC: &str = r#"
```gherkin
@m1
Feature: Selftest
  Scenario: Defined
    Given the value "2"
    Then the value is "2"
```
"#;

const FEATURE_BASIC: &str = r#"
@m1
Feature: Selftest
  Scenario: Defined
    Given the value "2"
    Then the value is "2"
"#;

const PRD_OUTLINE: &str = r#"
```gherkin
Feature: Outline
  Scenario Outline: Rows
    Given the value "<v>"
    Then the value is "<v>"

    @m1
    Examples:
      | v |
      | a |
      | b |

    @m2
    Examples:
      | v |
      | c |
```
"#;

struct Case {
    name: &'static str,
    prd: &'static str,
    features: &'static [&'static str],
    milestone: u32,
    args: &'static [&'static str],
    expect_pass: bool,
}

const CASES: &[Case] = &[
    Case {
        name: "complete inventory passes",
        prd: PRD_BASIC,
        features: &[FEATURE_BASIC],
        milestone: 1,
        args: &[],
        expect_pass: true,
    },
    Case {
        name: "milestone 0 bootstrap with no features passes",
        prd: PRD_BASIC,
        features: &[],
        milestone: 0,
        args: &[],
        expect_pass: true,
    },
    Case {
        name: "undefined step fails",
        prd: PRD_BASIC,
        features: &[r#"
Feature: Selftest
  Scenario: Defined
    Given the value "2"
    And an undefined step
    Then the value is "2"
"#],
        milestone: 1,
        args: &[],
        expect_pass: false,
    },
    Case {
        name: "ambiguous step fails",
        prd: PRD_BASIC,
        features: &[
            FEATURE_BASIC,
            r#"
Feature: Ambiguous
  Scenario: Ambiguous
    Given an ambiguous step
"#,
        ],
        milestone: 1,
        args: &[],
        expect_pass: false,
    },
    Case {
        name: "failing step fails",
        prd: PRD_BASIC,
        features: &[
            FEATURE_BASIC,
            r#"
Feature: Failing
  Scenario: Failing
    Given a defined step
    Then the step fails
"#,
        ],
        milestone: 1,
        args: &[],
        expect_pass: false,
    },
    Case {
        name: "empty required suite fails",
        prd: PRD_BASIC,
        features: &[],
        milestone: 1,
        args: &[],
        expect_pass: false,
    },
    Case {
        name: "weakened step text fails",
        prd: PRD_BASIC,
        features: &[r#"
Feature: Selftest
  Scenario: Defined
    Given the value "1"
    Then the value is "1"
"#],
        milestone: 1,
        args: &[],
        expect_pass: false,
    },
    Case {
        name: "missing Examples row fails",
        prd: PRD_OUTLINE,
        features: &[r#"
Feature: Outline
  Scenario Outline: Rows
    Given the value "<v>"
    Then the value is "<v>"

    Examples:
      | v |
      | a |
"#],
        milestone: 1,
        args: &[],
        expect_pass: false,
    },
    Case {
        name: "rows of later milestones are not yet required",
        prd: PRD_OUTLINE,
        features: &[r#"
Feature: Outline
  Scenario Outline: Rows
    Given the value "<v>"
    Then the value is "<v>"

    Examples:
      | v |
      | a |
      | b |
"#],
        milestone: 1,
        args: &[],
        expect_pass: true,
    },
    Case {
        name: "filtered run fails the inventory",
        prd: PRD_BASIC,
        features: &[FEATURE_BASIC],
        milestone: 1,
        args: &["--name", "does-not-match"],
        expect_pass: false,
    },
];

async fn run_case(case: &Case, dir: &Path) -> Result<bool> {
    let features = dir.join("features");
    fs::create_dir_all(&features).context("create features dir")?;
    for (i, text) in case.features.iter().enumerate() {
        fs::write(features.join(format!("case{i}.feature")), text).context("write feature")?;
    }
    let prd = dir.join("PRD.md");
    fs::write(&prd, case.prd).context("write PRD")?;
    let acceptance = dir.join("acceptance.toml");
    fs::write(&acceptance, format!("milestone = {}\n", case.milestone))
        .context("write acceptance")?;
    let report = run_suite::<SelfTestWorld>(Suite {
        features: &features,
        prd: &prd,
        milestone: support::inventory::acceptance_milestone(&acceptance)?,
        json_report: dir.join("cucumber.json"),
        console: false,
        cucumber_args: case.args.iter().map(|s| (*s).to_owned()).collect(),
    })
    .await?;
    println!(
        "       {}: {} required, {} executed, execution_failed={}, problems={:?}",
        case.name, report.required, report.executed, report.execution_failed, report.problems
    );
    Ok(report.passed())
}

/// Cumulative required scenario counts of the real PRD, counted by hand from PRD §5:
/// m1 = US-01 (2 rows + 3) + US-02 (2) + REL-01 (11); m2 adds US-03 (2) + US-04 (5) +
/// US-05 (3); m3 adds US-01 (3 rows) + US-02 (1) + US-03 (1 row); m4 adds US-03 (2).
const PRD_REQUIRED_PER_MILESTONE: [(u32, usize); 5] = [(0, 0), (1, 18), (2, 28), (3, 33), (4, 35)];

fn check_real_prd() -> Result<usize> {
    let prd = Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/PRD.md");
    let mut mismatches = 0;
    for (milestone, expected) in PRD_REQUIRED_PER_MILESTONE {
        let actual = support::inventory::from_prd(&prd, milestone)?.len();
        let ok = actual == expected;
        if !ok {
            mismatches += 1;
        }
        println!(
            "{} real PRD inventory at milestone {milestone}: expected {expected}, got {actual}",
            if ok { "ok  " } else { "FAIL" }
        );
    }
    Ok(mismatches)
}

#[tokio::main]
async fn main() -> Result<ExitCode> {
    let mut mismatches = check_real_prd()?;
    for case in CASES {
        let dir = tempfile::tempdir().context("create temp dir")?;
        let passed = run_case(case, dir.path()).await?;
        let ok = passed == case.expect_pass;
        if !ok {
            mismatches += 1;
        }
        println!(
            "{} {} (expected {}, got {})",
            if ok { "ok  " } else { "FAIL" },
            case.name,
            if case.expect_pass { "pass" } else { "fail" },
            if passed { "pass" } else { "fail" },
        );
    }
    println!(
        "bdd selftest: {} cases + real PRD inventory, {mismatches} mismatches",
        CASES.len()
    );
    Ok(if mismatches == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
