//! Required-scenario inventory derived from the PRD (PRD §5.7, PLAN §5.2).
//!
//! The PRD's Gherkin blocks are the source of truth. A scenario (or one `Examples`
//! row of an outline) is required when its effective milestone tag `@mN` is at most
//! the configured acceptance milestone. A required item counts as executed only when
//! the cucumber JSON report contains a scenario with the same feature name, scenario
//! name, and expanded step text, and every step of that scenario passed. Editing a
//! `.feature` file therefore cannot weaken or drop a required scenario unnoticed.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use cucumber::gherkin::{self, GherkinEnv};
use serde::Deserialize;

/// Identity of one executable scenario: feature, scenario, and expanded steps.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ScenarioKey {
    pub feature: String,
    pub scenario: String,
    pub steps: Vec<String>,
}

impl std::fmt::Display for ScenarioKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} :: {}", self.feature, self.scenario)?;
        if let Some(first) = self.steps.first() {
            write!(f, " [{first} ...]")?;
        }
        Ok(())
    }
}

/// Reads `milestone = N` from the acceptance configuration.
pub fn acceptance_milestone(path: &Path) -> Result<u32> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Acceptance {
        milestone: u32,
    }
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let acceptance: Acceptance =
        toml::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
    Ok(acceptance.milestone)
}

/// Extracts every fenced ```gherkin block from a Markdown document. An unclosed fence
/// is an error, so a truncated PRD cannot silently drop its last block.
pub fn gherkin_blocks(markdown: &str) -> Result<Vec<String>> {
    let mut blocks = Vec::new();
    let mut current: Option<Vec<&str>> = None;
    for line in markdown.lines() {
        match (&mut current, line.trim_end()) {
            (None, "```gherkin") => current = Some(Vec::new()),
            (Some(lines), "```") => {
                blocks.push(lines.join("\n") + "\n");
                current = None;
            }
            (Some(lines), _) => lines.push(line),
            (None, _) => {}
        }
    }
    if current.is_some() {
        bail!("unclosed ```gherkin fence");
    }
    Ok(blocks)
}

fn milestone_of(tags: &[String]) -> Result<Option<u32>> {
    let mut found = None;
    for tag in tags {
        let tag = tag.trim_start_matches('@');
        if let Some(n) = tag.strip_prefix('m').and_then(|n| n.parse::<u32>().ok()) {
            if found.is_some_and(|f| f != n) {
                bail!("conflicting milestone tags {tags:?}");
            }
            found = Some(n);
        }
    }
    Ok(found)
}

fn step_text(step: &gherkin::Step, row: Option<(&[String], &[String])>) -> String {
    let mut value = step.value.clone();
    if let Some((header, values)) = row {
        for (name, cell) in header.iter().zip(values) {
            value = value.replace(&format!("<{name}>"), cell);
        }
    }
    format!("{} {}", step.keyword.trim(), value)
}

/// Required scenario keys for `milestone`, derived from the PRD's Gherkin blocks.
///
/// Every PRD scenario must resolve to exactly one milestone (from the feature,
/// scenario, or `Examples` tags), otherwise the PRD itself is rejected.
pub fn from_prd(prd: &Path, milestone: u32) -> Result<BTreeSet<ScenarioKey>> {
    let markdown = fs::read_to_string(prd).with_context(|| format!("read {}", prd.display()))?;
    let blocks = gherkin_blocks(&markdown)?;
    if blocks.is_empty() {
        bail!("{} contains no gherkin blocks", prd.display());
    }
    let mut required = BTreeSet::new();
    for block in blocks {
        let feature = gherkin::Feature::parse(&block, GherkinEnv::default())
            .map_err(|e| anyhow::anyhow!("parse PRD gherkin block: {e}"))?;
        let feature_milestone = milestone_of(&feature.tags)?;
        for scenario in &feature.scenarios {
            let scenario_milestone = milestone_of(&scenario.tags)?.or(feature_milestone);
            if scenario.examples.is_empty() {
                let Some(m) = scenario_milestone else {
                    bail!("PRD scenario {:?} has no milestone tag", scenario.name);
                };
                if m <= milestone {
                    required.insert(ScenarioKey {
                        feature: feature.name.clone(),
                        scenario: scenario.name.clone(),
                        steps: scenario.steps.iter().map(|s| step_text(s, None)).collect(),
                    });
                }
                continue;
            }
            for examples in &scenario.examples {
                let Some(m) = milestone_of(&examples.tags)?.or(scenario_milestone) else {
                    bail!(
                        "PRD outline {:?} has an Examples block without milestone",
                        scenario.name
                    );
                };
                if m > milestone {
                    continue;
                }
                let Some((header, rows)) = examples
                    .table
                    .as_ref()
                    .and_then(|table| table.rows.split_first())
                else {
                    bail!(
                        "PRD outline {:?} has an Examples block without a table",
                        scenario.name
                    );
                };
                if rows.is_empty() {
                    bail!(
                        "PRD outline {:?} has an Examples block without data rows",
                        scenario.name
                    );
                }
                for row in rows {
                    required.insert(ScenarioKey {
                        feature: feature.name.clone(),
                        scenario: scenario.name.clone(),
                        steps: scenario
                            .steps
                            .iter()
                            .map(|s| step_text(s, Some((header, row))))
                            .collect(),
                    });
                }
            }
        }
    }
    Ok(required)
}

#[derive(Deserialize)]
struct JsonFeature {
    name: String,
    #[serde(default)]
    elements: Vec<JsonElement>,
}

#[derive(Deserialize)]
struct JsonElement {
    #[serde(rename = "type")]
    kind: String,
    name: String,
    #[serde(default)]
    steps: Vec<JsonStep>,
}

#[derive(Deserialize)]
struct JsonStep {
    keyword: String,
    name: String,
    result: JsonResult,
}

#[derive(Deserialize)]
struct JsonResult {
    status: String,
}

/// Executed scenarios from a cucumber JSON report, with whether every step passed.
pub fn executed(report: &Path) -> Result<BTreeMap<ScenarioKey, Vec<bool>>> {
    let text = fs::read_to_string(report).with_context(|| format!("read {}", report.display()))?;
    let features: Vec<JsonFeature> = if text.trim().is_empty() {
        Vec::new()
    } else {
        serde_json::from_str(&text).with_context(|| format!("parse {}", report.display()))?
    };
    let mut executed: BTreeMap<ScenarioKey, Vec<bool>> = BTreeMap::new();
    for feature in features {
        for element in feature
            .elements
            .into_iter()
            .filter(|e| e.kind == "scenario")
        {
            let passed = !element.steps.is_empty()
                && element.steps.iter().all(|s| s.result.status == "passed");
            let key = ScenarioKey {
                feature: feature.name.clone(),
                scenario: element.name,
                steps: element
                    .steps
                    .iter()
                    .map(|s| format!("{} {}", s.keyword.trim(), s.name))
                    .collect(),
            };
            executed.entry(key).or_default().push(passed);
        }
    }
    Ok(executed)
}

/// Problems that make a run fail even if cucumber itself reported success.
pub fn check(
    required: &BTreeSet<ScenarioKey>,
    executed: &BTreeMap<ScenarioKey, Vec<bool>>,
) -> Vec<String> {
    let mut problems = Vec::new();
    for key in required {
        match executed.get(key) {
            None => problems.push(format!("required scenario not executed: {key}")),
            Some(runs) if !runs.iter().all(|passed| *passed) => {
                problems.push(format!("required scenario did not pass: {key}"));
            }
            Some(_) => {}
        }
    }
    problems
}
