//! Pipeline properties that the PRD scenarios observe only indirectly (always runs).

use std::path::Path;
use std::sync::Arc;

use embeddings_fundamentals::anonymization::image_findings;
use embeddings_fundamentals::corpus::LoadedManifest;
use embeddings_fundamentals::domain::{ExtractionMethod, Modality, SourceStatus};
use embeddings_fundamentals::embedding::EmbeddingStrategy;
use embeddings_fundamentals::embedding::deterministic::DeterministicEmbedder;
use embeddings_fundamentals::embedding::deterministic::Method;
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
    let store = Arc::new(FsRecordStore::open_writer(store_dir.path()).expect("store"));
    let embedder = Arc::new(DeterministicEmbedder::builder().build().expect("adapter"));
    let pipeline = Pipeline {
        text: &extractor,
        ocr: &extractor,
        images: &extractor,
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
    let store = Arc::new(FsRecordStore::open_writer(store_dir.path()).expect("store"));
    let embedder = Arc::new(DeterministicEmbedder::builder().build().expect("adapter"));
    let pipeline = Pipeline {
        text: &extractor,
        ocr: &extractor,
        images: &extractor,
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
    let store = Arc::new(FsRecordStore::open_writer(store_dir.path()).expect("store"));
    let embedder = Arc::new(DeterministicEmbedder::builder().build().expect("adapter"));
    let first = Pipeline {
        text: &good,
        ocr: &good,
        images: &good,
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
        images: &broken,
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
    let store = Arc::new(FsRecordStore::open_writer(store_dir.path()).expect("store"));
    let embedder = Arc::new(DeterministicEmbedder::builder().build().expect("adapter"));
    let pipeline = Pipeline {
        text: &extractor,
        ocr: &extractor,
        images: &extractor,
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

#[tokio::test]
async fn a_damaged_active_revision_is_rebuilt_on_the_next_ingestion() {
    let mut manifest =
        LoadedManifest::load(&fixtures().join("corpus/manifest.toml")).expect("manifest");
    manifest
        .manifest
        .sources
        .retain(|s| s.source_id.as_str() == "lab-searchable-001");
    let extractor = FixtureExtractor::from_dir(&fixtures().join("extraction")).expect("fixtures");
    let store_dir = tempfile::tempdir().expect("tmp");
    let store = Arc::new(FsRecordStore::open_writer(store_dir.path()).expect("store"));
    let embedder = Arc::new(DeterministicEmbedder::builder().build().expect("adapter"));
    let pipeline = Pipeline {
        text: &extractor,
        ocr: &extractor,
        images: &extractor,
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
    let revision = first.sources[0].revision_id.clone().expect("revision");
    let space = first.space.clone();
    let records = store_dir
        .path()
        .join(space.as_str())
        .join("revisions/lab-searchable-001")
        .join(revision.as_str())
        .join("records.jsonl");
    let full = std::fs::read(&records).expect("records");
    let first_line = [full.split(|b| *b == b'\n').next().expect("line"), b"\n"].concat();
    let damages: [Option<Vec<u8>>; 4] = [
        None,
        Some(b"not json\n".to_vec()),
        Some(Vec::new()),
        Some(first_line),
    ];
    for (run, damage) in damages.into_iter().enumerate() {
        match &damage {
            None => std::fs::remove_file(&records).expect("delete records"),
            Some(bytes) => std::fs::write(&records, bytes).expect("damage records"),
        }
        let id = format!("run-{}", run + 2);
        let report = pipeline
            .ingest(&manifest, &IngestSettings::new(id.parse().expect("id")))
            .await
            .expect("run");
        let source = &report.sources[0];
        assert_eq!(
            source.status,
            SourceStatus::Succeeded,
            "{:?}",
            source.reason
        );
        assert!(!source.unchanged, "a damaged revision is not 'unchanged'");
        assert_eq!(source.records_committed, 10, "the revision was rebuilt");
        assert_eq!(store.active_records(&space).expect("readable").len(), 10);
    }
    // After healing, an intact revision is recognised as unchanged again.
    let report = pipeline
        .ingest(
            &manifest,
            &IngestSettings::new("run-9".parse().expect("id")),
        )
        .await
        .expect("run");
    assert!(report.sources[0].unchanged);
}

#[tokio::test]
async fn ineligible_sources_refuse_the_run_before_any_processing() {
    use embeddings_fundamentals::pipeline::{EXIT_VALIDATION, ValidationError};
    for required in [true, false] {
        let mut manifest =
            LoadedManifest::load(&fixtures().join("corpus/manifest.toml")).expect("manifest");
        manifest.manifest.sources.retain(|s| s.input_type.is_pdf());
        // One source without a recorded anonymization review.
        manifest.manifest.sources[1].anonymization = None;
        manifest.manifest.sources[1].required = required;
        let extractor =
            FixtureExtractor::from_dir(&fixtures().join("extraction")).expect("fixtures");
        let store_dir = tempfile::tempdir().expect("tmp");
        let store = Arc::new(FsRecordStore::open_writer(store_dir.path()).expect("store"));
        let embedder = Arc::new(DeterministicEmbedder::builder().build().expect("adapter"));
        let pipeline = Pipeline {
            text: &extractor,
            ocr: &extractor,
            images: &extractor,
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
        assert!(
            matches!(err, ValidationError::Ineligible(ref s) if s.len() == 1),
            "{err}"
        );
        assert_eq!(err.exit_code(), EXIT_VALIDATION);
        assert!(
            embedder.calls().is_empty(),
            "no adapter call (required={required})"
        );
        assert!(
            extractor.ocr_calls().is_empty(),
            "no extraction (required={required})"
        );
        assert!(
            store.spaces().expect("spaces").is_empty(),
            "nothing published (required={required})"
        );
    }
}

#[tokio::test]
async fn search_refuses_a_damaged_revision_in_both_scopes() {
    use embeddings_fundamentals::retrieval::{Scope, SearchError, search_text};
    let mut manifest =
        LoadedManifest::load(&fixtures().join("corpus/manifest.toml")).expect("manifest");
    manifest
        .manifest
        .sources
        .retain(|s| s.source_id.as_str() == "lab-searchable-001");
    let extractor = FixtureExtractor::from_dir(&fixtures().join("extraction")).expect("fixtures");
    let store_dir = tempfile::tempdir().expect("tmp");
    let store = Arc::new(FsRecordStore::open_writer(store_dir.path()).expect("store"));
    let embedder = Arc::new(DeterministicEmbedder::builder().build().expect("adapter"));
    let pipeline = Pipeline {
        text: &extractor,
        ocr: &extractor,
        images: &extractor,
        sleeper: &NoSleep,
        store: Arc::clone(&store),
        embedder: embedder.clone(),
    };
    let report = pipeline
        .ingest(
            &manifest,
            &IngestSettings::new("run-1".parse().expect("id")),
        )
        .await
        .expect("ingest");
    let revision = report.sources[0].revision_id.clone().expect("revision");
    let records = store_dir
        .path()
        .join(report.space.as_str())
        .join("revisions/lab-searchable-001")
        .join(revision.as_str())
        .join("records.jsonl");
    let full = std::fs::read(&records).expect("records");
    let first_line = [full.split(|b| *b == b'\n').next().expect("line"), b"\n"].concat();
    let snapshot = store
        .load_experiment(&"run-1".parse().expect("id"))
        .expect("snapshot");
    for damaged in [Vec::new(), first_line] {
        std::fs::write(&records, &damaged).expect("damage");
        for scope in [Scope::Active, Scope::Experiment(&snapshot)] {
            let outcome =
                search_text(&store, &report.space, &*embedder, "hemoglobina", 3, scope).await;
            assert!(
                matches!(outcome, Err(SearchError::Store(_))),
                "damaged revision must not be served"
            );
        }
    }
}

#[tokio::test]
async fn search_vector_refuses_non_finite_query_vectors_before_scoring() {
    // Regression (C2-5): the matching-space vector path checked space and size only.
    use embeddings_fundamentals::embedding::EmbeddingStrategy as _;
    use embeddings_fundamentals::retrieval::{Scope, SearchError, search_vector};
    let mut manifest =
        LoadedManifest::load(&fixtures().join("corpus/manifest.toml")).expect("manifest");
    manifest
        .manifest
        .sources
        .retain(|s| s.source_id.as_str() == "lab-searchable-001");
    let extractor = FixtureExtractor::from_dir(&fixtures().join("extraction")).expect("fixtures");
    let store_dir = tempfile::tempdir().expect("tmp");
    let store = Arc::new(FsRecordStore::open_writer(store_dir.path()).expect("store"));
    let embedder = Arc::new(DeterministicEmbedder::builder().build().expect("adapter"));
    let pipeline = Pipeline {
        text: &extractor,
        ocr: &extractor,
        images: &extractor,
        sleeper: &NoSleep,
        store: Arc::clone(&store),
        embedder: embedder.clone(),
    };
    pipeline
        .ingest(
            &manifest,
            &IngestSettings::new("run-1".parse().expect("id")),
        )
        .await
        .expect("ingest");
    let info = embedder.model_info();
    let space = info.embedding_space_id.clone();
    let dims = info.dimensions.get();
    let mut good = vec![0.0_f32; dims];
    good[0] = 1.0;
    let hits = search_vector(&store, &space, &space, &good, 3, Scope::Active)
        .await
        .expect("a finite vector searches");
    assert_eq!(hits.len(), 3);
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut vector = good.clone();
        vector[dims - 1] = bad;
        let outcome = search_vector(&store, &space, &space, &vector, 3, Scope::Active).await;
        assert!(
            matches!(outcome, Err(SearchError::InvalidQueryVector(_))),
            "{bad}: {outcome:?}"
        );
    }
}

#[tokio::test]
async fn an_image_run_embeds_image_files_and_qualifying_pdf_images() {
    let manifest =
        LoadedManifest::load(&fixtures().join("corpus/manifest.toml")).expect("manifest");
    let extractor = FixtureExtractor::from_dir(&fixtures().join("extraction")).expect("fixtures");
    let store_dir = tempfile::tempdir().expect("tmp");
    let store = Arc::new(FsRecordStore::open_writer(store_dir.path()).expect("store"));
    let embedder = Arc::new(
        DeterministicEmbedder::builder()
            .model_id("deterministic-image-v1")
            .text_to_image()
            .build()
            .expect("adapter"),
    );
    let pipeline = Pipeline {
        text: &extractor,
        ocr: &extractor,
        images: &extractor,
        sleeper: &NoSleep,
        store: Arc::clone(&store),
        embedder: embedder.clone(),
    };
    let mut settings = IngestSettings::new("images-1".parse().expect("id"));
    settings.modality = Modality::Image;
    let report = pipeline.ingest(&manifest, &settings).await.expect("run");
    let status = |id: &str| {
        report
            .source(&id.parse().expect("id"))
            .map(|s| s.status)
            .expect("source")
    };
    // The searchable report has no images; the scanned report's rasters are its OCR
    // sources; the mixed PDF keeps its page-1 ultrasound, not the logo or the page-2 scan.
    assert_eq!(status("lab-searchable-001"), SourceStatus::NotApplicable);
    assert_eq!(status("lab-scanned-002"), SourceStatus::NotApplicable);
    for id in ["us-mixed-003", "chest-001", "abd-us-001"] {
        assert_eq!(status(id), SourceStatus::Succeeded, "{id}: {report:?}");
    }
    let mut records = store
        .active_records(&embedder.model_info().embedding_space_id)
        .expect("records");
    records.sort_by(|a, b| a.chunk_id.cmp(&b.chunk_id));
    let ids: Vec<&str> = records.iter().map(|r| r.chunk_id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "abd-us-001-p1-image-0",
            "chest-001-p1-image-0",
            "us-mixed-003-p1-image-0"
        ]
    );
    for record in &records {
        assert_eq!(record.modality, Modality::Image);
        assert_eq!(record.chunk_text, None);
        let uri = record.asset_uri.as_deref().expect("asset uri");
        let path = store.asset_path(uri).expect("path");
        store.read_asset(uri).expect("intact asset");
        assert_eq!(image_findings(&path), vec![], "{uri} carries metadata");
        let original = record.metadata.original_asset.as_ref().expect("original");
        let expected = if record.source_id.as_str() == "us-mixed-003" {
            (ExtractionMethod::EmbeddedImage, Some(0), (256, 192))
        } else {
            (
                ExtractionMethod::ImageFile,
                None,
                (original.width, original.height),
            )
        };
        assert_eq!(
            (
                record.metadata.extraction_method,
                original.pdf_image_num,
                (original.width, original.height)
            ),
            expected
        );
    }
    let calls = embedder.calls();
    assert!(
        calls.iter().all(|c| c.method == Method::Images),
        "{calls:?}"
    );
    // Unchanged reingestion re-embeds nothing.
    let before = embedder.calls().len();
    settings.experiment_id = "images-2".parse().expect("id");
    let again = pipeline.ingest(&manifest, &settings).await.expect("run");
    assert!(
        again
            .sources
            .iter()
            .filter(|s| s.status == SourceStatus::Succeeded)
            .all(|s| s.unchanged)
    );
    assert_eq!(embedder.calls().len(), before);
}

#[tokio::test]
async fn an_image_run_needs_a_cross_modal_adapter() {
    let manifest =
        LoadedManifest::load(&fixtures().join("corpus/manifest.toml")).expect("manifest");
    let extractor = FixtureExtractor::from_dir(&fixtures().join("extraction")).expect("fixtures");
    let store_dir = tempfile::tempdir().expect("tmp");
    let store = Arc::new(FsRecordStore::open_writer(store_dir.path()).expect("store"));
    let text_only = Arc::new(DeterministicEmbedder::builder().build().expect("adapter"));
    let pipeline = Pipeline {
        text: &extractor,
        ocr: &extractor,
        images: &extractor,
        sleeper: &NoSleep,
        store,
        embedder: text_only.clone(),
    };
    let mut settings = IngestSettings::new("images-refused".parse().expect("id"));
    settings.modality = Modality::Image;
    assert!(pipeline.ingest(&manifest, &settings).await.is_err());
    assert!(text_only.calls().is_empty());
    assert_eq!(extractor.extraction_calls(), 0);
}

#[tokio::test]
async fn an_image_reprocess_that_keeps_nothing_deactivates_the_old_revision() {
    // Regression (G3-1): a reprocess whose filter keeps no image must not leave the
    // previous image revision answering searches.
    let manifest =
        LoadedManifest::load(&fixtures().join("corpus/manifest.toml")).expect("manifest");
    let mut manifest_one = manifest.clone();
    manifest_one
        .manifest
        .sources
        .retain(|s| s.source_id.as_str() == "us-mixed-003");
    let extractor = FixtureExtractor::from_dir(&fixtures().join("extraction")).expect("fixtures");
    let store_dir = tempfile::tempdir().expect("tmp");
    let store = Arc::new(FsRecordStore::open_writer(store_dir.path()).expect("store"));
    let embedder = Arc::new(
        DeterministicEmbedder::builder()
            .model_id("deterministic-image-v1")
            .text_to_image()
            .build()
            .expect("adapter"),
    );
    let space = embedder.model_info().embedding_space_id.clone();
    let pipeline = Pipeline {
        text: &extractor,
        ocr: &extractor,
        images: &extractor,
        sleeper: &NoSleep,
        store: Arc::clone(&store),
        embedder,
    };
    let source = "us-mixed-003".parse().expect("id");
    let mut settings = IngestSettings::new("keep-image".parse().expect("id"));
    settings.modality = Modality::Image;
    let first = pipeline
        .ingest(&manifest_one, &settings)
        .await
        .expect("run");
    assert_eq!(
        first.source(&source).map(|s| s.status),
        Some(SourceStatus::Succeeded)
    );
    let published = store.active_pointer(&space, &source).expect("pointer");
    assert!(published.is_some(), "the first run publishes the image");

    // The 256x192 image no longer passes a 512 px minimum.
    settings.experiment_id = "keep-nothing".parse().expect("id");
    settings.images.min_side_px = 512;
    let second = pipeline
        .ingest(&manifest_one, &settings)
        .await
        .expect("run");
    let report = second.source(&source).expect("source");
    assert_eq!(report.status, SourceStatus::NotApplicable);
    assert_eq!(report.revision_id, None, "no revision was published");
    assert!(
        report
            .reason
            .as_deref()
            .is_some_and(|r| r.contains("deactivated")),
        "{:?}",
        report.reason
    );
    assert_eq!(
        store.active_pointer(&space, &source).expect("pointer"),
        None
    );
    assert_eq!(store.active_records(&space).expect("records"), vec![]);
    // The earlier experiment still reads the revision it pinned.
    let snapshot = store
        .load_experiment(&"keep-image".parse().expect("id"))
        .expect("snapshot");
    let pinned = store.records(&snapshot.revisions).expect("pinned records");
    assert_eq!(pinned.len(), 1);
    // A further run stays not applicable and has nothing left to deactivate.
    settings.experiment_id = "keep-nothing-again".parse().expect("id");
    let third = pipeline
        .ingest(&manifest_one, &settings)
        .await
        .expect("run");
    let report = third.source(&source).expect("source");
    assert_eq!(report.status, SourceStatus::NotApplicable);
    assert!(
        report
            .reason
            .as_deref()
            .is_some_and(|r| !r.contains("deactivated")),
        "{:?}",
        report.reason
    );
}
