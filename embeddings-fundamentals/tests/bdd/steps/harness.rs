//! Steps for `harness.feature`: per-scenario isolation of the store, call log, and
//! fault plan. These scenarios are not PRD requirements; they keep the harness honest.

use std::fs;

use anyhow::{Context, Result, ensure};
use cucumber::{given, then, when};
use embeddings_fundamentals::embedding::{EmbeddingStrategy, ErrorClass, TextInput};

use crate::LabWorld;

fn parse_class(class: &str) -> Result<ErrorClass> {
    match class {
        "transient" => Ok(ErrorClass::Transient),
        "permanent" => Ok(ErrorClass::Permanent),
        other => anyhow::bail!("unknown error class {other:?}"),
    }
}

fn store_entries(world: &LabWorld) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(world.store_dir.path()).context("read scenario store")? {
        let name = entry
            .context("read store entry")?
            .file_name()
            .to_string_lossy()
            .into_owned();
        // Dot-files (`.writer.lock`) belong to the store itself, not to the scenario.
        if !name.starts_with('.') {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

#[given("a fresh scenario world")]
fn fresh_world(world: &mut LabWorld) -> Result<()> {
    ensure!(
        world.store_dir.path().is_dir(),
        "scenario store directory does not exist"
    );
    Ok(())
}

#[given(expr = "the adapter fails the next {int} call(s) with a {string} error")]
fn adapter_fails_next(world: &mut LabWorld, count: u32, class: String) -> Result<()> {
    world.embedder.fail_next(count, parse_class(&class)?);
    Ok(())
}

#[when("the scenario writes a marker file into its store")]
fn write_marker(world: &mut LabWorld) -> Result<()> {
    fs::write(world.store_dir.path().join("marker"), b"harness").context("write marker")
}

#[when("the scenario embeds one document through the deterministic adapter")]
async fn embed_one(world: &mut LabWorld) -> Result<()> {
    let input = TextInput {
        id: "harness-doc".parse()?,
        text: "Hemoglobina 13,5 g/dL".to_owned(),
    };
    let outcome = world.embedder.embed_documents(&[input]).await;
    world.last_embedding = Some(outcome.map(|_| ()).map_err(|e| e.class()));
    Ok(())
}

#[then("the store contains only the marker file")]
fn store_has_only_marker(world: &mut LabWorld) -> Result<()> {
    let entries = store_entries(world)?;
    ensure!(
        entries == ["marker"],
        "expected only the marker file, found {entries:?}"
    );
    Ok(())
}

#[then("the store is empty")]
fn store_is_empty(world: &mut LabWorld) -> Result<()> {
    let entries = store_entries(world)?;
    ensure!(
        entries.is_empty(),
        "expected an empty store, found {entries:?}"
    );
    Ok(())
}

#[then(expr = "the adapter call log has exactly {int} call(s)")]
fn call_log_len(world: &mut LabWorld, expected: usize) -> Result<()> {
    let actual = world.embedder.calls().len();
    ensure!(
        actual == expected,
        "expected {expected} adapter calls, observed {actual}"
    );
    Ok(())
}

#[then("the adapter call log is empty")]
fn call_log_empty(world: &mut LabWorld) -> Result<()> {
    call_log_len(world, 0)
}

#[then(expr = "the last embedding attempt failed with class {string}")]
fn last_attempt_failed(world: &mut LabWorld, class: String) -> Result<()> {
    let expected = parse_class(&class)?;
    match world.last_embedding {
        Some(Err(actual)) if actual == expected => Ok(()),
        other => anyhow::bail!("expected a {class} failure, observed {other:?}"),
    }
}

#[then("the last embedding attempt succeeded")]
fn last_attempt_succeeded(world: &mut LabWorld) -> Result<()> {
    ensure!(
        matches!(world.last_embedding, Some(Ok(()))),
        "expected a successful embedding, observed {:?}",
        world.last_embedding
    );
    Ok(())
}
