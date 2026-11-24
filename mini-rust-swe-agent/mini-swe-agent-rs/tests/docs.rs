//! The P9 "done" criteria as checks (spec/migration-plan.md §4, phase P9): the README lists every
//! divergence of §2.4 and every decision of §5.2 with its outcome and the status of every phase;
//! the links and crate paths in the README, `../CLAUDE.md`, `../README.md` and
//! `../spec/prompt/migrate.md` exist; and the stale "nothing is built yet" claims are gone.
//!
//! The Rust-only Linux gate mounts only this crate, so the checks of the parent files are skipped
//! on purpose there (`MSWEA_RS_NO_REFERENCE=1`).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Test helper: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::unwrap_used, clippy::panic)]
fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The lines of the section that starts with the heading `heading` (up to the next heading of
/// the same or a higher level).
/// Test helper: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::panic)]
fn section<'a>(text: &'a str, heading: &str) -> Vec<&'a str> {
    let level = heading.chars().take_while(|c| *c == '#').count();
    let mut lines = text.lines();
    if !lines.by_ref().any(|line| line.trim_end() == heading) {
        panic!("missing section {heading:?}");
    }
    lines
        .take_while(|line| {
            let hashes = line.chars().take_while(|c| *c == '#').count();
            !(hashes > 0 && hashes <= level && line.chars().nth(hashes) == Some(' '))
        })
        .collect()
}

/// The cells of the body rows of every Markdown table in `lines` (header and separator rows
/// dropped).
fn table_rows(lines: &[&str]) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut in_table = false;
    for line in lines {
        let line = line.trim();
        if !line.starts_with('|') {
            in_table = false;
            continue;
        }
        let cells: Vec<String> = line
            .trim_matches('|')
            .split('|')
            .map(|c| c.trim().to_owned())
            .collect();
        if !in_table {
            in_table = true; // the header row
            continue;
        }
        if cells
            .iter()
            .all(|c| c.chars().all(|ch| matches!(ch, '-' | ':')))
        {
            continue; // the separator row
        }
        rows.push(cells);
    }
    rows
}

fn first_cells(rows: &[Vec<String>]) -> BTreeSet<String> {
    rows.iter().filter_map(|r| r.first().cloned()).collect()
}

#[test]
fn readme_lists_every_divergence_of_the_plan() {
    let plan = read(&crate_dir().join("spec/migration-plan.md"));
    let readme = read(&crate_dir().join("README.md"));
    let planned = first_cells(&table_rows(&section(
        &plan,
        "### 2.4 Deliberate divergences",
    )));
    assert!(planned.len() >= 20, "{planned:?}");
    let listed = first_cells(&table_rows(&section(&readme, "## Divergences from Python")));
    let missing: Vec<_> = planned.difference(&listed).collect();
    assert!(
        missing.is_empty(),
        "divergences of §2.4 missing from the README: {missing:?}"
    );
}

#[test]
fn readme_records_every_decision_and_its_outcome() {
    let plan = read(&crate_dir().join("spec/migration-plan.md"));
    let readme = read(&crate_dir().join("README.md"));
    let planned = first_cells(&table_rows(&section(&plan, "### 5.2 Open decisions")));
    assert_eq!(planned.len(), 12, "{planned:?}");
    let rows = table_rows(&section(&readme, "## Decisions"));
    for id in &planned {
        let row = rows.iter().find(|r| r.first() == Some(id));
        let outcome = row
            .and_then(|r| r.last())
            .map(String::as_str)
            .unwrap_or_default();
        assert!(
            outcome.len() > 10,
            "{id}: no outcome in the README ({row:?})"
        );
    }
}

#[test]
fn readme_status_covers_every_phase() {
    let readme = read(&crate_dir().join("README.md"));
    let rows = table_rows(&section(&readme, "## Status"));
    for phase in 0..=9 {
        let prefix = format!("P{phase} ");
        let row = rows
            .iter()
            .find(|r| r.first().is_some_and(|c| c.starts_with(&prefix)));
        assert!(
            row.is_some_and(|r| r.get(1).is_some_and(|c| c.starts_with("Done"))),
            "P{phase}: {row:?}"
        );
    }
    let evidence = section(&readme, "## Evidence").join("\n");
    for needle in [
        "tests/parity.rs",
        "tests/text_parity.rs",
        "tests/support/cli.rs",
        "spec/reviews/",
    ] {
        assert!(
            evidence.contains(needle),
            "the evidence does not link {needle}"
        );
    }
}

/// Markdown link targets in `text`, without URLs, anchors-only links and `#fragments`.
fn link_targets(text: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("](") {
        rest = rest.get(start + 2..).unwrap_or_default();
        let Some(end) = rest.find(')') else { break };
        let target = rest.get(..end).unwrap_or_default();
        let target = target.split('#').next().unwrap_or_default().trim();
        if !target.is_empty() && !target.contains("://") && !target.starts_with("mailto:") {
            targets.push(target.to_owned());
        }
        rest = rest.get(end..).unwrap_or_default();
    }
    targets
}

/// Backtick-quoted crate paths in `text` (`mini-swe-agent-rs/…`), resolved in this crate.
fn crate_paths(text: &str) -> Vec<PathBuf> {
    text.split('`')
        .skip(1)
        .step_by(2)
        .filter_map(|token| {
            let token = token.trim_end_matches(['.', ',', ':', ';']);
            let relative = token
                .strip_prefix("mini-rust-swe-agent/")
                .unwrap_or(token)
                .strip_prefix("mini-swe-agent-rs/")?;
            (!relative.contains(['*', '<', ' '])).then(|| crate_dir().join(relative))
        })
        .collect()
}

/// True when the parent directory (`../CLAUDE.md`, `../mini-swe-agent/`, …) is present. The
/// Rust-only Linux gate mounts only this crate and opts out with `MSWEA_RS_NO_REFERENCE=1`; a
/// missing parent without that opt-out is a failure.
/// Test helper: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::panic)]
fn parent_mounted() -> bool {
    if crate_dir().join("../spec/prompt/migrate.md").is_file() {
        return true;
    }
    if std::env::var("MSWEA_RS_NO_REFERENCE").as_deref() != Ok("1") {
        panic!(
            "../spec/prompt/migrate.md is missing; set MSWEA_RS_NO_REFERENCE=1 to skip on purpose"
        );
    }
    eprintln!("skipped on purpose: the parent directory is not mounted");
    false
}

/// Links and crate paths of `file` that do not exist. Links that leave the crate's directory
/// are checked only when `outside` is true.
fn missing_paths(file: &Path, outside: bool) -> Vec<String> {
    let text = read(file);
    let dir = file.parent().unwrap_or(Path::new("."));
    let mut missing: Vec<String> = link_targets(&text)
        .into_iter()
        .filter(|target| outside || !target.starts_with("../"))
        .filter(|target| !dir.join(target).exists())
        .map(|target| format!("{}: link {target}", file.display()))
        .collect();
    missing.extend(
        crate_paths(&text)
            .into_iter()
            .filter(|path| !path.exists())
            .map(|path| format!("{}: path {}", file.display(), path.display())),
    );
    missing
}

#[test]
fn crate_readme_links_exist() {
    let missing = missing_paths(&crate_dir().join("README.md"), parent_mounted());
    assert!(missing.is_empty(), "{missing:#?}");
}

#[test]
fn parent_documents_match_the_crate() {
    if !parent_mounted() {
        return;
    }
    let base = crate_dir().join("..");
    let mut missing = Vec::new();
    for file in ["CLAUDE.md", "README.md", "spec/prompt/migrate.md"] {
        missing.extend(missing_paths(&base.join(file), true));
    }
    assert!(missing.is_empty(), "{missing:#?}");
    // Claims from before the crate existed.
    let stale = [
        ("CLAUDE.md", "not scaffolded"),
        ("CLAUDE.md", "So far it holds only"),
        ("CLAUDE.md", "(in Portuguese)"),
        ("README.md", "has not been created yet"),
        ("README.md", "How to start the implementation"),
    ];
    for (file, phrase) in stale {
        assert!(
            !read(&base.join(file)).contains(phrase),
            "../{file} still says {phrase:?}"
        );
    }
    let plan = read(&crate_dir().join("spec/migration-plan.md"));
    assert!(
        !plan.contains("No Rust code exists yet"),
        "the plan's status line is stale"
    );
}
