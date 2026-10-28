//! Shared Cucumber runner used by the `bdd` suite and its self-test (PLAN §5.2).

use std::fmt::Debug;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Parser;
use cucumber::codegen::WorldInventory;
use cucumber::{World, WriterExt, cli, writer};

use embeddings_fundamentals::ids::Checksum;

use super::inventory;

/// Inputs of one suite run.
pub struct Suite<'a> {
    /// Directory of `.feature` files to execute.
    pub features: &'a Path,
    /// PRD whose Gherkin blocks define the required inventory.
    pub prd: &'a Path,
    /// Acceptance milestone: scenarios tagged `@m1`..`@mN` are required.
    pub milestone: u32,
    /// Where the cucumber JSON report is written.
    pub json_report: PathBuf,
    /// Print the human-readable report to stdout.
    pub console: bool,
    /// Cucumber CLI arguments (`--name`, `--tags`, ...); argv is never parsed, so
    /// `cargo test <filter>` cannot break the harness-less targets.
    pub cucumber_args: Vec<String>,
    /// Inputs whose digests are published beside the JSON report (PRD §5.7), for
    /// example the corpus manifest and the acceptance configuration.
    pub fingerprinted: Vec<(&'static str, PathBuf)>,
}

/// File name of the fingerprint record written next to the cucumber JSON report.
pub const FINGERPRINTS_FILE: &str = "fingerprints.json";

/// Writes `fingerprints.json`: the SHA-256 of the PRD, every `.feature` file, and each
/// fingerprinted input, so a published BDD result is tied to the exact inputs it ran on.
fn write_fingerprints(suite: &Suite<'_>) -> Result<()> {
    let mut digests = std::collections::BTreeMap::new();
    let mut add = |name: String, path: &Path| -> Result<()> {
        let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
        digests.insert(name, Checksum::of_bytes(&bytes).to_string());
        Ok(())
    };
    add("prd".to_owned(), suite.prd)?;
    for (name, path) in &suite.fingerprinted {
        add((*name).to_owned(), path)?;
    }
    let mut features: Vec<PathBuf> = std::fs::read_dir(suite.features)
        .with_context(|| format!("read {}", suite.features.display()))?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<std::io::Result<_>>()?;
    features.retain(|p| p.extension().is_some_and(|ext| ext == "feature"));
    features.sort();
    for path in &features {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        add(format!("feature:{name}"), path)?;
    }
    let target = suite.json_report.with_file_name(FINGERPRINTS_FILE);
    let json = serde_json::to_string_pretty(&digests).context("serialize fingerprints")?;
    std::fs::write(&target, json).with_context(|| format!("write {}", target.display()))
}

/// Outcome of a suite run.
#[derive(Debug)]
pub struct Report {
    /// Cucumber reported a failed, skipped, ambiguous, or unparsable scenario.
    pub execution_failed: bool,
    /// Number of required scenario keys.
    pub required: usize,
    /// Number of executed scenario keys.
    pub executed: usize,
    /// Inventory problems (missing or failing required scenarios, empty suite).
    pub problems: Vec<String>,
}

impl Report {
    /// The suite passed only if cucumber succeeded and the inventory is complete.
    pub fn passed(&self) -> bool {
        !self.execution_failed && self.problems.is_empty()
    }
}

/// Runs the features with `fail_on_skipped`, writes the JSON report, flushes it, and
/// checks the executed scenarios against the PRD inventory.
pub async fn run_suite<W>(suite: Suite<'_>) -> Result<Report>
where
    W: World + WorldInventory + Debug,
{
    if let Some(parent) = suite.json_report.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let json = File::create(&suite.json_report)
        .with_context(|| format!("create {}", suite.json_report.display()))?;
    let console: Box<dyn io::Write> = if suite.console {
        Box::new(io::stdout())
    } else {
        Box::new(io::sink())
    };
    let args = std::iter::once("bdd".to_owned()).chain(suite.cucumber_args.iter().cloned());
    let opts: cli::Opts<_, _, _, cli::Empty> =
        cli::Opts::try_parse_from(args).context("parse cucumber arguments")?;

    let writer = W::cucumber()
        .with_writer(
            writer::Basic::raw(console, writer::Coloring::Never, writer::Verbosity::Default)
                .summarized()
                .tee::<W, _>(writer::Json::for_tee(json))
                .normalized(),
        )
        .fail_on_skipped()
        .with_cli(opts)
        .run(suite.features)
        .await;
    let execution_failed = cucumber::writer::Stats::execution_has_failed(&writer);
    // Dropping the writer drops the JSON output file, which flushes it.
    drop(writer);

    write_fingerprints(&suite)?;
    let required = inventory::from_prd(suite.prd, suite.milestone)?;
    let executed = inventory::executed(&suite.json_report)?;
    let mut problems = inventory::check(&required, &executed);
    if !required.is_empty() && executed.is_empty() {
        problems.push("required suite is empty: no scenario was executed".to_owned());
    }
    Ok(Report {
        execution_failed,
        required: required.len(),
        executed: executed.len(),
        problems,
    })
}

/// Splits a `BDD_ARGS` string into arguments, honouring single and double quotes so
/// `--name "Hemoglobina 13,5"` stays one regex. Backslash escapes the next character.
pub fn split_args(input: &str) -> Result<Vec<String>> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_arg = false;
    let mut quote: Option<char> = None;
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (_, '\\') => {
                let escaped = chars.next().context("dangling backslash in BDD_ARGS")?;
                current.push(escaped);
                in_arg = true;
            }
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => current.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                in_arg = true;
            }
            (None, c) if c.is_whitespace() => {
                if in_arg {
                    args.push(std::mem::take(&mut current));
                    in_arg = false;
                }
            }
            (None, c) => {
                current.push(c);
                in_arg = true;
            }
        }
    }
    anyhow::ensure!(quote.is_none(), "unterminated quote in BDD_ARGS");
    if in_arg {
        args.push(current);
    }
    Ok(args)
}
