//! Experiment-level properties (always runs): extraction fidelity must not depend on
//! chunking (review finding C2-6).

use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use embeddings_fundamentals::adapters::BuildError;
use embeddings_fundamentals::corpus::LoadedManifest;
use embeddings_fundamentals::embedding::EmbeddingStrategy;
use embeddings_fundamentals::embedding::deterministic::DeterministicEmbedder;
use embeddings_fundamentals::evaluation::Dataset;
use embeddings_fundamentals::experiment::{self, ExperimentInputs, Instruments, Tools};
use embeddings_fundamentals::extraction::fixture::FixtureExtractor;
use embeddings_fundamentals::fidelity::LoadedSample;
use embeddings_fundamentals::measurement::{FixedMemory, Hardware, ManualClock};
use embeddings_fundamentals::pipeline::{IngestSettings, NoSleep};
use embeddings_fundamentals::report::ConfigurationResult;
use embeddings_fundamentals::storage::FsRecordStore;

fn repo(path: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

#[expect(
    clippy::expect_used,
    reason = "test helper: a setup failure must abort the test"
)]
async fn run_with_budget(max_input_tokens: usize, id: &str) -> ConfigurationResult {
    let manifest =
        LoadedManifest::load(&repo("tests/fixtures/corpus/manifest.toml")).expect("manifest");
    let extractor =
        FixtureExtractor::from_dir(&repo("tests/fixtures/extraction")).expect("fixtures");
    let store_dir = tempfile::tempdir().expect("tmp");
    let store = Arc::new(FsRecordStore::open_writer(store_dir.path()).expect("store"));
    let dataset = Dataset::load(
        &repo("config/evaluation/queries-v1.toml"),
        // Image-only labels: text labels do not apply to truncated chunks (C2-1), and
        // this test is about extraction fidelity, not retrieval quality.
        &repo("tests/fixtures/evaluation/labels-images-only.toml"),
    )
    .expect("dataset");
    let sample =
        LoadedSample::load(&repo("config/fidelity/sample-synthetic-v1.toml")).expect("sample");
    let mut settings = IngestSettings::new(id.parse().expect("id"));
    settings.chunking.truncation_enabled = true;
    let template = DeterministicEmbedder::builder().build().expect("adapter");
    let mut capabilities = template.capabilities().clone();
    capabilities.max_input_tokens = NonZeroUsize::new(max_input_tokens);
    let build = async || -> Result<Arc<dyn EmbeddingStrategy>, BuildError> {
        let adapter = DeterministicEmbedder::builder()
            .capabilities(capabilities.clone())
            .build()
            .expect("adapter");
        let adapter: Arc<dyn EmbeddingStrategy> = Arc::new(adapter);
        Ok(adapter)
    };
    let instruments = Instruments {
        clock: Arc::new(ManualClock::new(Duration::from_millis(1))),
        memory: FixedMemory(Some(1)),
        hardware: Hardware::detect(),
    };
    experiment::run(
        Tools {
            text: &extractor,
            ocr: &extractor,
            sleeper: &NoSleep,
        },
        &store,
        build,
        &instruments,
        ExperimentInputs {
            manifest: &manifest,
            settings: &settings,
            dataset: &dataset,
            latency_repeats: NonZeroUsize::MIN,
            price: None,
            fidelity: Some(&sample),
        },
    )
    .await
    .expect("experiment")
}

#[tokio::test]
async fn extraction_fidelity_is_independent_of_the_chunking_budget() {
    let wide = run_with_budget(512, "fidelity-wide").await;
    let narrow = run_with_budget(12, "fidelity-narrow").await;
    assert_eq!(
        wide.reliability.truncations, 0,
        "the wide budget truncates nothing"
    );
    assert!(
        narrow.reliability.truncations > 0,
        "the narrow budget must truncate rows for this test to mean anything"
    );
    assert_ne!(wide.fingerprints.chunking, narrow.fingerprints.chunking);
    let (wide, narrow) = (
        wide.fidelity.expect("fidelity"),
        narrow.fidelity.expect("fidelity"),
    );
    assert_eq!(
        wide.overall, narrow.overall,
        "extraction fidelity changed with the chunking budget"
    );
    assert_eq!(wide.by_field, narrow.by_field);
    assert_eq!(wide.by_method, narrow.by_method);
    assert!(wide.overall.correct > 0, "the sample must match something");
}
