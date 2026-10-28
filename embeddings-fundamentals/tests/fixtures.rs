//! Integrity of the checked-in synthetic fixture corpus (always runs).
//!
//! Every `[[sources]]` file in `tests/fixtures/corpus/manifest.toml` must hash to its
//! recorded checksum, and `expected.toml` must describe exactly the manifest sources.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, ensure};
use embeddings_fundamentals::ids::Checksum;
use serde::Deserialize;

#[derive(Deserialize)]
struct Manifest {
    sources: Vec<Source>,
}

#[derive(Deserialize)]
struct Source {
    source_id: String,
    file: String,
    checksum: String,
}

#[derive(Deserialize)]
struct Expected {
    sources: Vec<ExpectedSource>,
}

#[derive(Deserialize)]
struct ExpectedSource {
    source_id: String,
}

fn corpus() -> &'static Path {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/corpus"
    ))
}

fn manifest() -> Result<Manifest> {
    let text = fs::read_to_string(corpus().join("manifest.toml")).context("read manifest.toml")?;
    toml::from_str(&text).context("parse manifest.toml")
}

#[test]
fn every_fixture_matches_its_manifest_checksum() -> Result<()> {
    let manifest = manifest()?;
    ensure!(!manifest.sources.is_empty(), "manifest lists no sources");
    for source in &manifest.sources {
        let bytes = fs::read(corpus().join(&source.file))
            .with_context(|| format!("read fixture {}", source.file))?;
        ensure!(
            Checksum::of_bytes(&bytes).as_str() == source.checksum,
            "checksum mismatch for {} ({}): regenerate with `cargo run --example gen_fixtures`",
            source.source_id,
            source.file
        );
    }
    Ok(())
}

#[test]
fn expected_behaviour_covers_exactly_the_manifest_sources() -> Result<()> {
    let manifest: BTreeSet<String> = manifest()?
        .sources
        .into_iter()
        .map(|s| s.source_id)
        .collect();
    let text = fs::read_to_string(corpus().join("expected.toml")).context("read expected.toml")?;
    let expected: Expected = toml::from_str(&text).context("parse expected.toml")?;
    let expected: BTreeSet<String> = expected.sources.into_iter().map(|s| s.source_id).collect();
    ensure!(
        manifest == expected,
        "expected.toml and manifest.toml list different sources"
    );
    Ok(())
}
