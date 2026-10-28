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
    /// When set, cucumber itself (not only the inventory) must report this outcome.
    expect_execution_failed: Option<bool>,
}

const CASES: &[Case] = &[
    Case {
        name: "complete inventory passes",
        prd: PRD_BASIC,
        features: &[FEATURE_BASIC],
        milestone: 1,
        args: &[],
        expect_pass: true,
        expect_execution_failed: Some(false),
    },
    Case {
        name: "required scenario with And/But steps passes (keywords kept verbatim)",
        prd: r#"
```gherkin
@m1
Feature: Conjunctions
  Scenario: And and But
    Given the value "2"
    And a defined step
    Then the value is "2"
    But the value is "2"
```
"#,
        features: &[r#"
Feature: Conjunctions
  Scenario: And and But
    Given the value "2"
    And a defined step
    Then the value is "2"
    But the value is "2"
"#],
        milestone: 1,
        args: &[],
        expect_pass: true,
        expect_execution_failed: Some(false),
    },
    Case {
        name: "milestone 0 bootstrap with no features passes",
        prd: PRD_BASIC,
        features: &[],
        milestone: 0,
        args: &[],
        expect_pass: true,
        expect_execution_failed: Some(false),
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
        expect_execution_failed: Some(true),
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
        expect_execution_failed: Some(true),
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
        expect_execution_failed: Some(true),
    },
    Case {
        name: "skipped step with PRD-identical text fails through fail_on_skipped",
        prd: r#"
```gherkin
@m1
Feature: Skipping
  Scenario: Skipped
    Given the value "2"
    And a step nobody defined
    Then the value is "2"
```
"#,
        features: &[r#"
Feature: Skipping
  Scenario: Skipped
    Given the value "2"
    And a step nobody defined
    Then the value is "2"
"#],
        milestone: 1,
        args: &[],
        expect_pass: false,
        expect_execution_failed: Some(true),
    },
    Case {
        name: "empty required suite fails",
        prd: PRD_BASIC,
        features: &[],
        milestone: 1,
        args: &[],
        expect_pass: false,
        expect_execution_failed: None,
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
        expect_execution_failed: None,
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
        expect_execution_failed: None,
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
        expect_execution_failed: Some(false),
    },
    Case {
        name: "filtered run fails the inventory",
        prd: PRD_BASIC,
        features: &[FEATURE_BASIC],
        milestone: 1,
        args: &["--name", "does-not-match"],
        expect_pass: false,
        expect_execution_failed: None,
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
        fingerprinted: vec![("acceptance", acceptance.clone())],
    })
    .await?;
    let fingerprints: std::collections::BTreeMap<String, String> = serde_json::from_str(
        &fs::read_to_string(dir.join(support::runner::FINGERPRINTS_FILE))
            .context("read fingerprints")?,
    )
    .context("parse fingerprints")?;
    let expected_prd = embeddings_fundamentals::ids::Checksum::of_bytes(case.prd.as_bytes());
    let expected_acceptance =
        embeddings_fundamentals::ids::Checksum::of_bytes(&fs::read(&acceptance)?);
    if fingerprints.get("prd").map(String::as_str) != Some(expected_prd.as_str())
        || fingerprints.get("acceptance").map(String::as_str) != Some(expected_acceptance.as_str())
        || fingerprints
            .keys()
            .filter(|k| k.starts_with("feature:"))
            .count()
            != case.features.len()
    {
        println!(
            "       {}: fingerprints do not match the inputs: {fingerprints:?}",
            case.name
        );
        return Ok(!case.expect_pass);
    }
    if let Some(expected) = case.expect_execution_failed
        && report.execution_failed != expected
    {
        println!(
            "       {}: cucumber execution_failed={} but expected {expected}",
            case.name, report.execution_failed
        );
        return Ok(!case.expect_pass);
    }
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

fn check_inventory_rejects_empty_examples() -> Result<usize> {
    let dir = tempfile::tempdir().context("temp dir")?;
    let prd = dir.path().join("PRD.md");
    fs::write(
        &prd,
        "```gherkin\nFeature: Empty\n  Scenario Outline: Rows\n    Given the value \"<v>\"\n\n    @m1\n    Examples:\n      | v |\n```\n",
    )
    .context("write PRD")?;
    let rejected = support::inventory::from_prd(&prd, 1).is_err();
    println!(
        "{} inventory rejects an Examples block without data rows",
        if rejected { "ok  " } else { "FAIL" }
    );
    Ok(usize::from(!rejected))
}

fn check_inventory_rejects_unclosed_fence() -> Result<usize> {
    let dir = tempfile::tempdir().context("temp dir")?;
    let prd = dir.path().join("PRD.md");
    let text = format!(
        "{PRD_BASIC}\n```gherkin\n@m1\nFeature: Truncated\n  Scenario: Lost\n    Given a defined step\n"
    );
    fs::write(&prd, text).context("write PRD")?;
    let rejected = support::inventory::from_prd(&prd, 1).is_err();
    println!(
        "{} inventory rejects an unclosed gherkin fence after a closed one",
        if rejected { "ok  " } else { "FAIL" }
    );
    Ok(usize::from(!rejected))
}

fn check_split_args() -> usize {
    let cases: [(&str, Option<Vec<&str>>); 4] = [
        (
            r#"--name "Hemoglobina 13,5""#,
            Some(vec!["--name", "Hemoglobina 13,5"]),
        ),
        (
            "--tags '@m1 and @us01'",
            Some(vec!["--tags", "@m1 and @us01"]),
        ),
        ("  -n  plain  ", Some(vec!["-n", "plain"])),
        (r#"--name "unterminated"#, None),
    ];
    let mut mismatches = 0;
    for (input, expected) in cases {
        let actual = support::runner::split_args(input).ok();
        let expected: Option<Vec<String>> =
            expected.map(|v| v.into_iter().map(str::to_owned).collect());
        let ok = actual == expected;
        if !ok {
            mismatches += 1;
        }
        println!(
            "{} split_args({input:?}) = {actual:?}",
            if ok { "ok  " } else { "FAIL" }
        );
    }
    mismatches
}

#[tokio::main]
async fn main() -> Result<ExitCode> {
    let mut mismatches = check_real_prd()?
        + check_inventory_rejects_empty_examples()?
        + check_inventory_rejects_unclosed_fence()?
        + check_split_args();
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
