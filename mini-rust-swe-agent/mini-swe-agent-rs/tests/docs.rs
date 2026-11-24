//! The P9 "done" criteria as checks (spec/migration-plan.md §4, phase P9):
//! - the README's divergence table repeats every row of plan §2.4 (divergence, Python, Rust) and
//!   cites tests and files that exist;
//! - the README records every decision of §5.2 with its options and an outcome, every phase as
//!   done with its pull request, and links to the parity evidence;
//! - every link (with its `#anchor`) and every crate path in the README, `../CLAUDE.md`,
//!   `../README.md` and `../spec/prompt/migrate.md` exists;
//! - the "nothing is built yet" claims are gone.
//!
//! The Rust-only Linux gate mounts only this crate, so the checks that need the parent directory
//! are skipped on purpose there (`MSWEA_RS_NO_REFERENCE=1`).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Test helper: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::panic)]
fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// True when the parent directory (`../CLAUDE.md`, `../mini-swe-agent/`, …) is present. A
/// missing parent is a failure unless `MSWEA_RS_NO_REFERENCE=1` opts out.
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

/// The heading level of `line` (`## x` is 2), or `None` when it is not a heading.
fn heading_level(line: &str) -> Option<usize> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    (hashes > 0 && line.chars().nth(hashes) == Some(' ')).then_some(hashes)
}

/// Each line with whether it belongs to a fenced code block (the fences included).
fn with_fences(text: &str) -> Vec<(&str, bool)> {
    let mut fenced = false;
    text.lines()
        .map(|line| {
            if line.trim_start().starts_with("```") {
                fenced = !fenced;
                return (line, true);
            }
            (line, fenced)
        })
        .collect()
}

/// The lines of the section under `heading`, up to the next heading of the same or a higher
/// level outside code blocks.
/// Test helper: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::panic)]
fn section<'a>(text: &'a str, heading: &str) -> Vec<&'a str> {
    let level = heading_level(heading).unwrap_or_else(|| panic!("not a heading: {heading}"));
    let lines = with_fences(text);
    let Some(start) = lines
        .iter()
        .position(|(line, fenced)| !fenced && line.trim_end() == heading)
    else {
        panic!("missing section {heading:?}");
    };
    lines
        .iter()
        .skip(start + 1)
        .take_while(|(line, fenced)| *fenced || heading_level(line).is_none_or(|l| l > level))
        .map(|(line, _)| *line)
        .collect()
}

/// The body rows of every Markdown table in `lines`: the header row and the separator row right
/// after it are dropped; every other row is kept, even with empty cells.
fn table_rows<'a>(lines: &[&'a str]) -> Vec<Vec<&'a str>> {
    let mut rows = Vec::new();
    let mut position = 0;
    for line in lines {
        let line = line.trim();
        if !line.starts_with('|') {
            position = 0;
            continue;
        }
        position += 1;
        if position > 2 {
            rows.push(line.trim_matches('|').split('|').map(str::trim).collect());
        }
    }
    rows
}

/// Backtick-quoted tokens of `text`.
fn code_spans(text: &str) -> impl Iterator<Item = &str> {
    text.split('`').skip(1).step_by(2)
}

/// Every `fn name` in the test sources, and every parity scenario id.
fn known_names() -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut dirs = vec![crate_dir().join("tests")];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                for line in read(&path).lines() {
                    if let Some(rest) = line.trim_start().strip_prefix("fn ") {
                        let name = rest.split(['(', '<']).next().unwrap_or_default();
                        names.insert(name.to_owned());
                    }
                }
            }
        }
    }
    let scenarios = std::fs::read_dir(crate_dir().join("parity/scenarios"));
    for entry in scenarios.into_iter().flatten().flatten() {
        if let Some(stem) = entry.path().file_stem().and_then(|s| s.to_str()) {
            names.insert(stem.to_owned());
        }
    }
    names
}

/// Paths the plan names on purpose although they no longer exist: the old plan documents that
/// decision D2 merged into `spec/migration-plan.md`.
const REMOVED: &[&str] = &["spec/PLAN.md", "spec/STUDY.md"];

/// A crate path quoted in a document: `mini-swe-agent-rs/…` (optionally after
/// `mini-rust-swe-agent/`), or, when `bare` (inside the crate), a path under a crate directory.
/// Paths with a glob, a placeholder or a space are left out. Returned relative to the crate.
fn crate_relative(token: &str, bare: bool) -> Option<&str> {
    let token = token.trim_end_matches(['.', ',', ':', ';', ')']);
    let token = token
        .strip_prefix("mini-rust-swe-agent/")
        .unwrap_or(token)
        .strip_prefix("mini-swe-agent-rs/")
        .or_else(|| {
            let under = ["src/", "tests/", "parity/", "examples/", "config/", "spec/"]
                .iter()
                .any(|dir| token.starts_with(dir));
            (bare && under).then_some(token)
        })?;
    let concrete = !token.contains(['*', '<', ' ', '{']) && !token.contains("pN");
    (concrete && !REMOVED.contains(&token)).then_some(token)
}

#[test]
fn readme_lists_every_divergence_of_the_plan() {
    let plan = read(&crate_dir().join("spec/migration-plan.md"));
    let readme = read(&crate_dir().join("README.md"));
    let planned = table_rows(&section(&plan, "### 2.4 Deliberate divergences"));
    let listed = table_rows(&section(&readme, "## Divergences from Python"));
    assert!(planned.len() >= 21, "{planned:?}");
    assert_eq!(listed.len(), planned.len(), "one README row per plan row");
    let known = known_names();
    for row in &planned {
        let key = row.get(..3);
        let Some(found) = listed.iter().find(|r| r.get(..3) == key) else {
            panic!("§2.4 row missing or changed in the README: {key:?}");
        };
        let checked_by = found.get(3).copied().unwrap_or_default();
        assert!(!checked_by.is_empty(), "{key:?}: no evidence");
        for token in code_spans(checked_by) {
            if let Some(path) = crate_relative(token, true) {
                assert!(
                    crate_dir().join(path).exists(),
                    "{key:?}: {path} does not exist"
                );
            } else if token.contains('_') && !token.contains(['/', ' ', '.']) {
                assert!(
                    known.contains(token),
                    "{key:?}: no test or scenario named {token}"
                );
            }
        }
    }
}

#[test]
fn readme_records_every_decision_and_its_outcome() {
    let plan = read(&crate_dir().join("spec/migration-plan.md"));
    let readme = read(&crate_dir().join("README.md"));
    let planned = table_rows(&section(&plan, "### 5.2 Open decisions"));
    assert_eq!(planned.len(), 12, "{planned:?}");
    let rows = table_rows(&section(&readme, "## Decisions"));
    assert_eq!(rows.len(), planned.len());
    for plan_row in &planned {
        let id = plan_row.first().copied().unwrap_or_default();
        let row = rows.iter().find(|r| r.first() == Some(&id));
        let Some([_, question, options, outcome]) = row.map(Vec::as_slice) else {
            panic!("{id}: expected `| id | decision | options | outcome |`, found {row:?}");
        };
        assert_eq!(
            [Some(question), Some(options)],
            [plan_row.get(1), plan_row.get(2)],
            "{id}: the decision or its options differ from the plan"
        );
        assert!(outcome.len() > 20, "{id}: no outcome");
    }
}

#[test]
fn readme_status_covers_every_phase_with_its_pull_request() {
    let readme = read(&crate_dir().join("README.md"));
    let rows = table_rows(&section(&readme, "## Status"));
    for phase in 0..=9 {
        let prefix = format!("P{phase} ");
        let row = rows
            .iter()
            .find(|r| r.first().is_some_and(|c| c.starts_with(&prefix)));
        let Some([_, state, pr]) = row.map(Vec::as_slice) else {
            panic!("P{phase}: {row:?}");
        };
        assert!(state.starts_with("Done"), "P{phase}: {state}");
        assert_eq!(*pr, format!("#{}", 21 + phase), "P{phase}");
    }
}

#[test]
fn readme_links_the_parity_evidence() {
    let readme = read(&crate_dir().join("README.md"));
    let evidence = section(&readme, "## Evidence").join("\n");
    let links = link_targets(&evidence);
    for target in [
        "parity/scenarios/",
        "parity/run_python.py",
        "tests/parity.rs",
        "tests/support/compare.rs",
        "tests/bdd.rs",
        "tests/support/cli.rs",
        "tests/text_parity.rs",
        "tests/goldens/",
        "spec/reviews/",
        "spec/reviews/p8/FINDINGS.md",
    ] {
        assert!(
            links.iter().any(|(path, _)| path == target),
            "the evidence does not link {target}"
        );
    }
}

/// Markdown link targets of `text` as (path, anchor), without URLs and `mailto:` links. An
/// anchor-only link has an empty path.
fn link_targets(text: &str) -> Vec<(String, Option<String>)> {
    let mut targets = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("](") {
        rest = rest.get(start + 2..).unwrap_or_default();
        let Some(end) = rest.find(')') else { break };
        let target = rest.get(..end).unwrap_or_default().trim();
        if !target.contains("://") && !target.starts_with("mailto:") {
            let (path, anchor) = match target.split_once('#') {
                Some((path, anchor)) => (path, Some(anchor.to_owned())),
                None => (target, None),
            };
            targets.push((path.to_owned(), anchor));
        }
        rest = rest.get(end..).unwrap_or_default();
    }
    targets
}

/// GitHub's anchor for a heading: lower case, punctuation dropped, spaces as hyphens.
fn slug(heading: &str) -> String {
    heading
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

/// The anchors of the headings of a Markdown file.
fn anchors(path: &Path) -> BTreeSet<String> {
    with_fences(&read(path))
        .into_iter()
        .filter(|(_, fenced)| !fenced)
        .filter_map(|(line, _)| {
            heading_level(line).map(|level| slug(line.get(level + 1..).unwrap_or_default()))
        })
        .collect()
}

/// Crate paths written in plain text (`mini-swe-agent-rs/…`, optionally after
/// `mini-rust-swe-agent/`).
/// Test helper: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::unwrap_used)]
fn plain_crate_paths(text: &str) -> Vec<String> {
    let pattern =
        regex::Regex::new(r"(?:mini-rust-swe-agent/)?mini-swe-agent-rs/[A-Za-z0-9_./-]*").unwrap();
    pattern
        .find_iter(text)
        .filter_map(|m| crate_relative(m.as_str(), false).map(str::to_owned))
        .collect()
}

/// Checks every link (and its anchor) and every crate path of `file`. Inside the crate
/// (`in_crate`), paths under a crate directory count too, and links that leave the crate are
/// checked only when the parent is mounted. Returns the problems and how many references were
/// checked.
fn check_references(file: &Path, in_crate: bool, outside: bool) -> (Vec<String>, usize) {
    let text = read(file);
    let dir = file.parent().unwrap_or(Path::new("."));
    let mut problems = Vec::new();
    let mut checked = 0;
    for (path, anchor) in link_targets(&text) {
        if !outside && path.starts_with("../") {
            continue;
        }
        checked += 1;
        let target = if path.is_empty() {
            file.to_path_buf()
        } else {
            dir.join(&path)
        };
        if !target.exists() {
            problems.push(format!("{}: link {path}", file.display()));
        } else if let Some(anchor) = anchor
            && target.extension().is_some_and(|e| e == "md")
            && !anchors(&target).contains(&anchor)
        {
            problems.push(format!("{}: anchor {path}#{anchor}", file.display()));
        }
    }
    let mut paths: Vec<String> = code_spans(&text)
        .filter_map(|token| crate_relative(token, in_crate))
        .map(str::to_owned)
        .collect();
    paths.extend(plain_crate_paths(&text));
    for path in paths {
        checked += 1;
        if !crate_dir().join(&path).exists() {
            problems.push(format!("{}: path {path}", file.display()));
        }
    }
    (problems, checked)
}

#[test]
fn crate_readme_references_exist() {
    let (problems, checked) =
        check_references(&crate_dir().join("README.md"), true, parent_mounted());
    assert!(problems.is_empty(), "{problems:#?}");
    assert!(checked >= 40, "only {checked} references were checked");
}

#[test]
fn the_plan_status_is_current() {
    let plan = read(&crate_dir().join("spec/migration-plan.md"));
    assert!(
        !plan.contains("No Rust code exists yet"),
        "the plan's status line is stale"
    );
}

#[test]
fn parent_documents_match_the_crate() {
    if !parent_mounted() {
        return;
    }
    let base = crate_dir().join("..");
    for file in ["CLAUDE.md", "README.md", "spec/prompt/migrate.md"] {
        let (problems, checked) = check_references(&base.join(file), false, true);
        assert!(problems.is_empty(), "{problems:#?}");
        assert!(checked > 0, "../{file}: no crate reference was checked");
    }
    // Claims from before the crate existed.
    let stale = [
        ("CLAUDE.md", "not scaffolded"),
        ("CLAUDE.md", "So far it holds only"),
        ("CLAUDE.md", "(in Portuguese)"),
        ("README.md", "has not been created yet"),
        ("README.md", "How to start the implementation"),
        ("README.md", "in later stages"),
        ("README.md", "will have its own strict policy"),
    ];
    for (file, phrase) in stale {
        assert!(
            !read(&base.join(file)).contains(phrase),
            "../{file} still says {phrase:?}"
        );
    }
}
