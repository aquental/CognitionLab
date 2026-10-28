//! Pipeline properties that the PRD scenarios observe only indirectly (always runs).

use std::path::Path;
use std::sync::Arc;

use embeddings_fundamentals::corpus::LoadedManifest;
use embeddings_fundamentals::domain::SourceStatus;
use embeddings_fundamentals::embedding::deterministic::DeterministicEmbedder;
use embeddings_fundamentals::extraction::fixture::FixtureExtractor;
use embeddings_fundamentals::pipeline::{IngestSettings, NoSleep, Pipeline};
use embeddings_fundamentals::storage::FsRecordStore;

fn fixtures() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"))
}

#[tokio::test]
async fn unchanged_reingestion_skips_extraction_and_embedding() {
    let mut manifest =
        LoadedManifest::load(&fixtures().join("corpus/manifest.toml")).expect("manifest");
    manifest
        .manifest
        .sources
        .retain(|s| s.source_id.as_str() == "lab-scanned-002");
    let extractor = FixtureExtractor::from_dir(&fixtures().join("extraction")).expect("fixtures");
    let store_dir = tempfile::tempdir().expect("tmp");
    let store = Arc::new(FsRecordStore::open(store_dir.path()).expect("store"));
    let embedder = Arc::new(DeterministicEmbedder::builder().build().expect("adapter"));
    let pipeline = Pipeline {
        text: &extractor,
        ocr: &extractor,
        sleeper: &NoSleep,
        store: Arc::clone(&store),
        embedder: embedder.clone(),
    };
    let first = pipeline
        .ingest(
            &manifest,
            &IngestSettings::new("run-1".parse().expect("id")),
        )
        .await
        .expect("run 1");
    assert_eq!(first.sources[0].status, SourceStatus::Succeeded);
    assert!(!first.sources[0].unchanged);
    let calls_after_first = embedder.calls().len();
    assert!(calls_after_first > 0);

    let second = pipeline
        .ingest(
            &manifest,
            &IngestSettings::new("run-2".parse().expect("id")),
        )
        .await
        .expect("run 2");
    let source = &second.sources[0];
    assert_eq!(source.status, SourceStatus::Succeeded);
    assert!(source.unchanged, "the active revision is current");
    assert_eq!(
        source.attempts, 0,
        "no adapter call for an unchanged revision"
    );
    assert_eq!(
        embedder.calls().len(),
        calls_after_first,
        "the adapter was not called again"
    );
    assert_eq!(source.revision_id, first.sources[0].revision_id);
}

#[tokio::test]
async fn reusing_an_experiment_id_is_refused_before_processing() {
    let mut manifest =
        LoadedManifest::load(&fixtures().join("corpus/manifest.toml")).expect("manifest");
    manifest
        .manifest
        .sources
        .retain(|s| s.source_id.as_str() == "lab-searchable-001");
    let extractor = FixtureExtractor::from_dir(&fixtures().join("extraction")).expect("fixtures");
    let store_dir = tempfile::tempdir().expect("tmp");
    let store = Arc::new(FsRecordStore::open(store_dir.path()).expect("store"));
    let embedder = Arc::new(DeterministicEmbedder::builder().build().expect("adapter"));
    let pipeline = Pipeline {
        text: &extractor,
        ocr: &extractor,
        sleeper: &NoSleep,
        store,
        embedder: embedder.clone(),
    };
    let settings = IngestSettings::new("run-1".parse().expect("id"));
    pipeline
        .ingest(&manifest, &settings)
        .await
        .expect("first run");
    let calls = embedder.calls().len();
    let refused = pipeline.ingest(&manifest, &settings).await;
    assert!(refused.is_err(), "experiment snapshots are append-only");
    assert_eq!(
        embedder.calls().len(),
        calls,
        "a refused run makes no adapter calls"
    );
}

#[tokio::test]
async fn unchanged_sources_are_recognised_before_extraction() {
    let mut manifest =
        LoadedManifest::load(&fixtures().join("corpus/manifest.toml")).expect("manifest");
    manifest
        .manifest
        .sources
        .retain(|s| s.source_id.as_str() == "lab-scanned-002");
    let good = FixtureExtractor::from_dir(&fixtures().join("extraction")).expect("fixtures");
    let store_dir = tempfile::tempdir().expect("tmp");
    let store = Arc::new(FsRecordStore::open(store_dir.path()).expect("store"));
    let embedder = Arc::new(DeterministicEmbedder::builder().build().expect("adapter"));
    let first = Pipeline {
        text: &good,
        ocr: &good,
        sleeper: &NoSleep,
        store: Arc::clone(&store),
        embedder: embedder.clone(),
    };
    first
        .ingest(
            &manifest,
            &IngestSettings::new("run-1".parse().expect("id")),
        )
        .await
        .expect("run 1");
    let calls = embedder.calls().len();

    // OCR is now broken; the engine ids (and so the fingerprints) are unchanged.
    let broken = FixtureExtractor::from_dir(&fixtures().join("extraction")).expect("fixtures");
    broken.fail_file("lab-scanned-002", "tesseract crashed");
    let second = Pipeline {
        text: &broken,
        ocr: &broken,
        sleeper: &NoSleep,
        store,
        embedder: embedder.clone(),
    };
    let report = second
        .ingest(
            &manifest,
            &IngestSettings::new("run-2".parse().expect("id")),
        )
        .await
        .expect("run 2");
    let source = &report.sources[0];
    assert_eq!(
        source.status,
        SourceStatus::Succeeded,
        "{:?}",
        source.reason
    );
    assert!(source.unchanged);
    assert!(source.pages.is_empty(), "no page was extracted");
    assert!(broken.ocr_calls().is_empty(), "OCR was not invoked");
    assert_eq!(embedder.calls().len(), calls);
}

#[tokio::test]
async fn an_unreadable_experiment_file_is_refused_before_processing() {
    use embeddings_fundamentals::pipeline::EXIT_VALIDATION;
    let mut manifest =
        LoadedManifest::load(&fixtures().join("corpus/manifest.toml")).expect("manifest");
    manifest
        .manifest
        .sources
        .retain(|s| s.source_id.as_str() == "lab-searchable-001");
    let extractor = FixtureExtractor::from_dir(&fixtures().join("extraction")).expect("fixtures");
    let store_dir = tempfile::tempdir().expect("tmp");
    std::fs::create_dir_all(store_dir.path().join("experiments")).expect("dir");
    std::fs::write(
        store_dir.path().join("experiments/run-1.json"),
        b"{\"experiment_id\": \"run-1\", \"revis",
    )
    .expect("corrupt");
    let store = Arc::new(FsRecordStore::open(store_dir.path()).expect("store"));
    let embedder = Arc::new(DeterministicEmbedder::builder().build().expect("adapter"));
    let pipeline = Pipeline {
        text: &extractor,
        ocr: &extractor,
        sleeper: &NoSleep,
        store: Arc::clone(&store),
        embedder: embedder.clone(),
    };
    let err = pipeline
        .ingest(
            &manifest,
            &IngestSettings::new("run-1".parse().expect("id")),
        )
        .await
        .expect_err("refused");
    assert_eq!(err.exit_code(), EXIT_VALIDATION, "{err}");
    assert!(embedder.calls().is_empty(), "no adapter call");
    assert!(
        store.spaces().expect("spaces").is_empty(),
        "nothing was published"
    );
}

#[test]
fn store_failures_are_internal_errors_not_refusals() {
    use embeddings_fundamentals::pipeline::ValidationError;
    use embeddings_fundamentals::storage::StoreError;
    let err = ValidationError::Store(StoreError::InjectedFault(
        embeddings_fundamentals::storage::FaultPoint::BeforeStage,
    ));
    assert_eq!(err.exit_code(), 1);
}
