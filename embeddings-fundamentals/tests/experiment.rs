//! Experiment-level properties (always runs): extraction fidelity must not depend on
//! chunking (review finding C2-6), and an incomplete ingestion whose labels cannot
//! apply is reported, not aborted (C2-10). An image run is evaluated image-only.

use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::Arc;

use embeddings_fundamentals::adapters::BuildError;
use embeddings_fundamentals::corpus::LoadedManifest;
use embeddings_fundamentals::domain::{FailureStage, Modality, RunStatus, SourceStatus};
use embeddings_fundamentals::embedding::deterministic::DeterministicEmbedder;
use embeddings_fundamentals::embedding::{EmbeddingStrategy, EngineId};
use embeddings_fundamentals::evaluation::{Dataset, EvaluationMode};
use embeddings_fundamentals::experiment::{
    self, ExperimentError, ExperimentInputs, Instruments, Tools,
};
use embeddings_fundamentals::extraction::fixture::FixtureExtractor;
use embeddings_fundamentals::fidelity::LoadedSample;
use embeddings_fundamentals::measurement::{FixedMemory, Hardware, ManualClock};
use embeddings_fundamentals::pipeline::{IngestSettings, NoSleep};
use embeddings_fundamentals::report::{ConfigurationResult, EvaluationNotRunStatus};
use embeddings_fundamentals::storage::FsRecordStore;

fn repo(path: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

#[expect(
    clippy::expect_used,
    reason = "test helper: a setup failure must abort the test"
)]
async fn run_with_budget(max_input_tokens: usize, id: &str) -> ConfigurationResult {
    // Image-only labels: text labels do not apply to truncated chunks (C2-1), and this
    // test is about extraction fidelity, not retrieval quality.
    run_experiment(
        max_input_tokens,
        id,
        "tests/fixtures/evaluation/labels-images-only.toml",
        None,
        false,
    )
    .await
    .expect("experiment")
}

#[expect(
    clippy::expect_used,
    reason = "test helper: a setup failure must abort the test"
)]
async fn run_experiment(
    max_input_tokens: usize,
    id: &str,
    labels: &str,
    failing_stem: Option<&str>,
    fail_query_side: bool,
) -> Result<ConfigurationResult, ExperimentError> {
    let manifest =
        LoadedManifest::load(&repo("tests/fixtures/corpus/manifest.toml")).expect("manifest");
    let extractor =
        FixtureExtractor::from_dir(&repo("tests/fixtures/extraction")).expect("fixtures");
    if let Some(stem) = failing_stem {
        extractor.fail_file(stem, "injected extraction failure");
    }
    let store_dir = tempfile::tempdir().expect("tmp");
    let store = Arc::new(FsRecordStore::open_writer(store_dir.path()).expect("store"));
    let dataset =
        Dataset::load(&repo("config/evaluation/queries-v1.toml"), &repo(labels)).expect("dataset");
    let sample =
        LoadedSample::load(&repo("config/fidelity/sample-synthetic-v1.toml")).expect("sample");
    let mut settings = IngestSettings::new(id.parse().expect("id"));
    settings.chunking.truncation_enabled = true;
    let template = DeterministicEmbedder::builder().build().expect("adapter");
    let mut capabilities = template.capabilities().clone();
    capabilities.max_input_tokens = NonZeroUsize::new(max_input_tokens);
    let builds = std::sync::atomic::AtomicUsize::new(0);
    let build = async || -> Result<Arc<dyn EmbeddingStrategy>, BuildError> {
        let n = builds.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if fail_query_side && n > 0 {
            return Err(BuildError::Load {
                engine: EngineId::Deterministic,
                reason: "injected query-side load failure".into(),
            });
        }
        let adapter = DeterministicEmbedder::builder()
            .capabilities(capabilities.clone())
            .build()
            .expect("adapter");
        let adapter: Arc<dyn EmbeddingStrategy> = Arc::new(adapter);
        Ok(adapter)
    };
    let instruments = Instruments {
        clock: Arc::new(ManualClock::new()),
        memory: FixedMemory(Some(1)),
        hardware: Hardware::detect().await,
    };
    experiment::run(
        Tools {
            text: &extractor,
            ocr: &extractor,
            images: &extractor,
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

#[tokio::test]
async fn a_failed_labeled_source_is_reported_as_incomplete_without_an_evaluation() {
    let outcome = run_experiment(
        512,
        "labeled-source-fails",
        "config/evaluation/labels-synthetic-v4.toml",
        Some("lab-searchable-001"),
        false,
    )
    .await;
    let Err(ExperimentError::EvaluationNotRun(incomplete)) = outcome else {
        panic!("expected EvaluationNotRun, got {outcome:?}");
    };
    assert_eq!(incomplete.run_status, RunStatus::Incomplete);
    assert_eq!(incomplete.evaluation.status, EvaluationNotRunStatus::NotRun);
    assert!(
        incomplete
            .evaluation
            .reason
            .contains("lab-searchable-001-p1-text-"),
        "the reason names the inapplicable label: {}",
        incomplete.evaluation.reason
    );
    assert_eq!(incomplete.reliability.failed_sources, 1);
    let failed: Vec<_> = incomplete
        .ingestion
        .sources
        .iter()
        .filter(|s| s.status == SourceStatus::Failed)
        .collect();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].source_id.as_str(), "lab-searchable-001");
    assert_eq!(failed[0].stage, Some(FailureStage::Extraction));
    // The persisted form round-trips and says the evaluation did not run.
    let json = serde_json::to_value(&*incomplete).expect("json");
    assert_eq!(json["evaluation"]["status"], "not_run");
    assert_eq!(json["run_status"], "incomplete");
}

#[tokio::test]
async fn a_failed_unlabeled_source_still_completes_the_evaluation() {
    // The image-only labels never point into the searchable PDF, so its failure leaves
    // the evaluation applicable; the result still says the ingestion was incomplete.
    let result = run_experiment(
        512,
        "unlabeled-source-fails",
        "tests/fixtures/evaluation/labels-images-only.toml",
        Some("lab-searchable-001"),
        false,
    )
    .await
    .expect("experiment");
    assert_eq!(result.run_status, RunStatus::Incomplete);
    assert_eq!(result.reliability.failed_sources, 1);
}

#[tokio::test]
async fn an_incomplete_run_is_reported_even_when_the_query_side_would_fail() {
    // Regression (G2-9): the label check runs before the query-side adapter load and
    // the warm-up query, so their failure cannot hide the incomplete-run report.
    let outcome = run_experiment(
        512,
        "query-side-fails",
        "config/evaluation/labels-synthetic-v4.toml",
        Some("lab-searchable-001"),
        true,
    )
    .await;
    assert!(
        matches!(outcome, Err(ExperimentError::EvaluationNotRun(_))),
        "expected EvaluationNotRun, got {outcome:?}"
    );
}

#[tokio::test]
async fn an_incomplete_run_with_applicable_labels_is_reported_when_the_query_side_fails() {
    // Regression (C2-14): image-only labels still apply after the searchable PDF fails,
    // so the label check passes; the query-side adapter then fails to load. The
    // ingestion outcome must still be reported.
    let outcome = run_experiment(
        512,
        "unlabeled-source-and-query-side-fail",
        "tests/fixtures/evaluation/labels-images-only.toml",
        Some("lab-searchable-001"),
        true,
    )
    .await;
    let Err(ExperimentError::EvaluationNotRun(incomplete)) = outcome else {
        panic!("expected EvaluationNotRun, got {outcome:?}");
    };
    assert_eq!(incomplete.reliability.failed_sources, 1);
    assert!(
        incomplete
            .evaluation
            .reason
            .contains("injected query-side load failure"),
        "{}",
        incomplete.evaluation.reason
    );
}

#[tokio::test]
async fn a_complete_run_whose_query_side_fails_is_an_error() {
    let outcome = run_experiment(
        512,
        "complete-query-side-fails",
        "tests/fixtures/evaluation/labels-images-only.toml",
        None,
        true,
    )
    .await;
    assert!(
        matches!(outcome, Err(ExperimentError::Build(_))),
        "a complete ingestion has nothing incomplete to report: {outcome:?}"
    );
}

#[derive(serde::Deserialize)]
struct KeywordFile {
    dimensions: usize,
    keywords: Vec<Keyword>,
}

#[derive(serde::Deserialize)]
struct Keyword {
    keyword: String,
    components: Vec<(usize, f32)>,
}

#[derive(serde::Deserialize)]
struct MacroMetrics {
    #[serde(rename = "macro")]
    macro_avg: Metrics,
}

#[derive(serde::Deserialize)]
#[expect(
    clippy::struct_field_names,
    reason = "the names are the metric keys of the fixture"
)]
struct Metrics {
    precision_at_3: f64,
    hit_at_3: f64,
    ndcg_at_3: f64,
}

#[tokio::test]
async fn an_image_run_is_evaluated_image_only_through_the_text_query_path() {
    let manifest =
        LoadedManifest::load(&repo("tests/fixtures/corpus/manifest.toml")).expect("manifest");
    let extractor =
        FixtureExtractor::from_dir(&repo("tests/fixtures/extraction")).expect("fixtures");
    let store_dir = tempfile::tempdir().expect("tmp");
    let store = Arc::new(FsRecordStore::open_writer(store_dir.path()).expect("store"));
    let dataset = Dataset::load(
        &repo("config/evaluation/queries-v1.toml"),
        &repo("config/evaluation/labels-synthetic-v4.toml"),
    )
    .expect("dataset");
    let mut settings = IngestSettings::new("image-run".parse().expect("id"));
    settings.modality = Modality::Image;
    let keywords: KeywordFile = toml::from_str(
        &std::fs::read_to_string(repo("tests/fixtures/evaluation/image-keywords.toml"))
            .expect("read"),
    )
    .expect("parse");
    let mut builder = DeterministicEmbedder::builder()
        .model_id("deterministic-image-v1")
        .dimensions(NonZeroUsize::new(keywords.dimensions).expect("dims"))
        .text_to_image();
    for k in &keywords.keywords {
        builder = builder.with_keyword_vector(&k.keyword, &k.components);
    }
    let build = async || -> Result<Arc<dyn EmbeddingStrategy>, BuildError> {
        let adapter: Arc<dyn EmbeddingStrategy> =
            Arc::new(builder.clone().build().expect("adapter"));
        Ok(adapter)
    };
    let instruments = Instruments {
        clock: Arc::new(ManualClock::new()),
        memory: FixedMemory(Some(1)),
        hardware: Hardware::detect().await,
    };
    let result = experiment::run(
        Tools {
            text: &extractor,
            ocr: &extractor,
            images: &extractor,
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
            fidelity: None,
        },
    )
    .await
    .expect("experiment");

    assert_eq!(result.run_status, RunStatus::Succeeded);
    assert_eq!(result.evaluation.mode, EvaluationMode::ImageOnly);
    // The three synthetic images are the indexed items, embedded through embed_images
    // and measured as the ingestion side.
    assert_eq!(result.measurements.embedding.items, 3);
    for query in &result.evaluation.queries {
        assert_eq!(query.results.len(), 3, "{}", query.query_id);
        assert!(
            query.results.iter().all(|r| r.modality == Modality::Image
                && r.chunk_text.is_none()
                && r.asset_uri.is_some()),
            "{}: image-only results are image records",
            query.query_id
        );
    }
    let expected: MacroMetrics = toml::from_str(
        &std::fs::read_to_string(repo("tests/fixtures/metrics/image-only.toml")).expect("read"),
    )
    .expect("parse");
    let m = &result.evaluation.metrics;
    for (name, got, want) in [
        ("P@3", m.precision_at_3, expected.macro_avg.precision_at_3),
        ("Hit@3", m.hit_at_3, expected.macro_avg.hit_at_3),
        ("nDCG@3", m.ndcg_at_3, expected.macro_avg.ndcg_at_3),
    ] {
        assert!((got - want).abs() < 1e-9, "{name}: {got} vs {want}");
    }
}
