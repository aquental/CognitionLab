//! Combined retrieval (PRD §6.6, PLAN Phase 4): a measured combined run over a text run
//! and an image run in one store, the refusals that keep the pair comparable, the
//! guards of a combined search, and the three-mode comparison report.

use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::Arc;

use embeddings_fundamentals::adapters::BuildError;
use embeddings_fundamentals::corpus::LoadedManifest;
use embeddings_fundamentals::domain::Modality;
use embeddings_fundamentals::embedding::deterministic::{
    DeterministicBuilder, DeterministicEmbedder,
};
use embeddings_fundamentals::embedding::{EmbeddingError, EmbeddingStrategy};
use embeddings_fundamentals::evaluation::{Dataset, EvaluationMode};
use embeddings_fundamentals::experiment::{
    self, CombinedInputs, ExperimentError, ExperimentInputs, Instruments, Tools,
};
use embeddings_fundamentals::extraction::fixture::FixtureExtractor;
use embeddings_fundamentals::fusion::FusionParams;
use embeddings_fundamentals::ids::{Checksum, ExperimentId};
use embeddings_fundamentals::measurement::{FixedMemory, Hardware, ManualClock};
use embeddings_fundamentals::pipeline::{IngestSettings, NoSleep};
use embeddings_fundamentals::report::{
    CombinedResult, ComparisonReport, ConfigurationResult, ReportError,
};
use embeddings_fundamentals::retrieval::{Leg, Scope, SearchError, search_combined};
use embeddings_fundamentals::storage::{FsRecordStore, StoreError};

const LABELS: &str = "config/evaluation/labels-synthetic-v4.toml";

fn repo(path: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
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

#[expect(clippy::expect_used, reason = "test helper")]
fn keyword_builder(file: &str) -> DeterministicBuilder {
    let keywords: KeywordFile =
        toml::from_str(&std::fs::read_to_string(repo(file)).expect("read")).expect("parse");
    let mut builder = DeterministicEmbedder::builder()
        .dimensions(NonZeroUsize::new(keywords.dimensions).expect("dims"));
    for k in &keywords.keywords {
        builder = builder.with_keyword_vector(&k.keyword, &k.components);
    }
    builder
}

/// The keyword index of the text-only BDD fixture.
fn text_builder() -> DeterministicBuilder {
    keyword_builder("tests/fixtures/evaluation/keywords.toml")
}

/// The keyword index of the image-only BDD fixture, in its own cross-modal space.
fn image_builder() -> DeterministicBuilder {
    keyword_builder("tests/fixtures/evaluation/image-keywords.toml")
        .model_id("deterministic-image-v1")
        .text_to_image()
}

#[expect(clippy::expect_used, reason = "test helper")]
fn build(builder: &DeterministicBuilder) -> Result<Arc<dyn EmbeddingStrategy>, BuildError> {
    let adapter: Arc<dyn EmbeddingStrategy> = Arc::new(builder.clone().build().expect("adapter"));
    Ok(adapter)
}

/// A store with a text run and an image run of the synthetic corpus, both evaluated
/// with the frozen dataset.
struct Fixture {
    _dir: tempfile::TempDir,
    store: Arc<FsRecordStore>,
    manifest: LoadedManifest,
    dataset: Dataset,
    text: ConfigurationResult,
    image: ConfigurationResult,
}

fn instruments() -> Instruments<ManualClock, FixedMemory> {
    Instruments {
        clock: Arc::new(ManualClock::new()),
        memory: FixedMemory(Some(1)),
        hardware: Hardware {
            os: "test-os".into(),
            arch: "test-arch".into(),
            cpu: None,
            logical_cpus: None,
            memory_bytes: None,
            accelerator: "none".into(),
        },
    }
}

#[expect(clippy::expect_used, reason = "test helper")]
async fn fixture() -> Fixture {
    let manifest =
        LoadedManifest::load(&repo("tests/fixtures/corpus/manifest.toml")).expect("manifest");
    let extractor =
        FixtureExtractor::from_dir(&repo("tests/fixtures/extraction")).expect("fixtures");
    let dir = tempfile::tempdir().expect("tmp");
    let store = Arc::new(FsRecordStore::open_writer(dir.path()).expect("store"));
    let dataset =
        Dataset::load(&repo("config/evaluation/queries-v1.toml"), &repo(LABELS)).expect("dataset");
    let mut runs = Vec::new();
    for (id, modality, builder) in [
        ("text-run", Modality::Text, text_builder()),
        ("image-run", Modality::Image, image_builder()),
    ] {
        let mut settings = IngestSettings::new(id.parse().expect("id"));
        settings.modality = modality;
        let result = experiment::run(
            Tools {
                text: &extractor,
                ocr: &extractor,
                images: &extractor,
                sleeper: &NoSleep,
            },
            &store,
            async || build(&builder),
            &instruments(),
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
        .expect(id);
        runs.push(result);
    }
    let image = runs.pop().expect("image run");
    let text = runs.pop().expect("text run");
    Fixture {
        _dir: dir,
        store,
        manifest,
        dataset,
        text,
        image,
    }
}

async fn combine(
    f: &Fixture,
    id: &str,
    legs: (&str, &str),
    dataset: &Dataset,
    corpus: &Checksum,
) -> Result<CombinedResult, ExperimentError> {
    let (id, text, image): (ExperimentId, ExperimentId, ExperimentId) = (
        id.parse()
            .map_err(|e| ExperimentError::Combination(format!("{e}")))?,
        legs.0
            .parse()
            .map_err(|e| ExperimentError::Combination(format!("{e}")))?,
        legs.1
            .parse()
            .map_err(|e| ExperimentError::Combination(format!("{e}")))?,
    );
    let (text_builder, image_builder) = (text_builder(), image_builder());
    experiment::run_combined(
        &f.store,
        async || build(&text_builder),
        async || build(&image_builder),
        &instruments(),
        CombinedInputs {
            experiment_id: &id,
            text_experiment: &text,
            image_experiment: &image,
            corpus_checksum: corpus,
            dataset,
            fusion: FusionParams::default(),
            latency_repeats: NonZeroUsize::MIN.saturating_add(1),
            text_price: None,
            image_price: None,
        },
    )
    .await
}

#[tokio::test]
async fn a_combined_run_fuses_the_text_and_image_runs_and_pins_both() {
    let f = fixture().await;
    let corpus = f.manifest.manifest_checksum.clone();
    let result = combine(
        &f,
        "combined-run",
        ("text-run", "image-run"),
        &f.dataset,
        &corpus,
    )
    .await
    .expect("combined run");

    assert_eq!(result.evaluation.mode, EvaluationMode::Combined);
    assert_eq!(result.fusion, FusionParams::default());
    assert_eq!(result.evaluation.fusion, Some(FusionParams::default()));
    assert_eq!(result.text.embedding_space_id, f.text.embedding_space_id);
    assert_eq!(result.image.embedding_space_id, f.image.embedding_space_id);
    assert_eq!(
        result.evaluation.spaces,
        [
            f.text.embedding_space_id.clone(),
            f.image.embedding_space_id.clone()
        ]
    );
    assert_eq!(result.measurements.warm_query_latency.samples, 20);
    for query in &result.evaluation.queries {
        assert_eq!(query.results.len(), 3, "{}", query.query_id);
        for r in &query.results {
            let trace = r.fusion.as_ref().expect("fused results carry their legs");
            assert!(
                trace.text.is_some() != trace.image.is_some(),
                "a text record and an image record are distinct items"
            );
            assert_eq!(
                trace.text.is_some(),
                r.modality == Modality::Text,
                "{}",
                r.chunk_id
            );
        }
    }
    // Same indexes as the BDD combined example, so the same hand-derived metrics.
    let expected: MacroMetrics = toml::from_str(
        &std::fs::read_to_string(repo("tests/fixtures/metrics/combined.toml")).expect("read"),
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

    // The combined snapshot pins exactly the legs' revisions in the legs' spaces.
    let snapshot = f
        .store
        .load_experiment(&"combined-run".parse().expect("id"))
        .expect("combined snapshot");
    let leg_revisions = |id: &str, space| {
        f.store
            .load_experiment(&id.parse().expect("id"))
            .expect("leg snapshot")
            .revisions
            .into_iter()
            .filter(|r| &r.space == space)
            .collect::<Vec<_>>()
    };
    let mut expected_revisions = leg_revisions("text-run", &f.text.embedding_space_id);
    expected_revisions.extend(leg_revisions("image-run", &f.image.embedding_space_id));
    assert_eq!(snapshot.revisions, expected_revisions);
    assert!(snapshot.fingerprints.contains_key("fusion"));
    assert_eq!(snapshot.corpus_checksum, corpus);

    // A combined search by the combined id returns the evaluated ranking.
    let text_adapter = text_builder().build().expect("text");
    let image_adapter = image_builder().build().expect("image");
    let first = &f.dataset.queries.value.queries[0];
    let hits = search_combined(
        &f.store,
        Leg {
            space: &f.text.embedding_space_id,
            embedder: &text_adapter,
        },
        Leg {
            space: &f.image.embedding_space_id,
            embedder: &image_adapter,
        },
        FusionParams::default(),
        &first.text,
        3,
        Scope::Experiment(&snapshot),
    )
    .await
    .expect("search");
    let ids: Vec<_> = hits.iter().map(|h| h.record_id.clone()).collect();
    let evaluated: Vec<_> = result.evaluation.queries[0]
        .results
        .iter()
        .map(|r| r.record_id.clone())
        .collect();
    assert_eq!(ids, evaluated);
}

#[tokio::test]
async fn a_combined_run_refuses_legs_that_cannot_be_paired() {
    let f = fixture().await;
    let corpus = f.manifest.manifest_checksum.clone();
    // Swapped legs: the image run pins nothing in the text model's space.
    let swapped = combine(
        &f,
        "swapped",
        ("image-run", "text-run"),
        &f.dataset,
        &corpus,
    )
    .await;
    assert!(
        matches!(swapped, Err(ExperimentError::Combination(ref m)) if m.contains("pins no revision")),
        "{swapped:?}"
    );
    // Another corpus.
    let other = Checksum::of_bytes(b"another manifest");
    let corpus_mismatch = combine(
        &f,
        "other-corpus",
        ("text-run", "image-run"),
        &f.dataset,
        &other,
    )
    .await;
    assert!(
        matches!(corpus_mismatch, Err(ExperimentError::Combination(ref m)) if m.contains("indexed corpus")),
        "{corpus_mismatch:?}"
    );
    // Other frozen labels than the leg runs were evaluated with.
    let other_labels = Dataset::load(
        &repo("config/evaluation/queries-v1.toml"),
        &repo("tests/fixtures/evaluation/labels-images-only.toml"),
    )
    .expect("dataset");
    let labels_mismatch = combine(
        &f,
        "other-labels",
        ("text-run", "image-run"),
        &other_labels,
        &corpus,
    )
    .await;
    assert!(
        matches!(labels_mismatch, Err(ExperimentError::Combination(ref m)) if m.contains("not evaluated")),
        "{labels_mismatch:?}"
    );
    // A missing leg run.
    let missing = combine(
        &f,
        "missing",
        ("text-run", "no-such-run"),
        &f.dataset,
        &corpus,
    )
    .await;
    assert!(
        matches!(missing, Err(ExperimentError::Store(_))),
        "{missing:?}"
    );
    // None of the refusals left a snapshot behind.
    for id in ["swapped", "other-corpus", "other-labels", "missing"] {
        assert!(
            !f.store
                .experiment_exists(&id.parse().expect("id"))
                .expect("exists"),
            "{id}"
        );
    }
    // Combined ids are create-only, like every experiment snapshot.
    combine(&f, "once", ("text-run", "image-run"), &f.dataset, &corpus)
        .await
        .expect("first");
    let again = combine(&f, "once", ("text-run", "image-run"), &f.dataset, &corpus).await;
    assert!(
        matches!(
            again,
            Err(ExperimentError::Store(StoreError::ExperimentExists(_)))
        ),
        "{again:?}"
    );
}

#[tokio::test]
async fn a_combined_search_checks_both_legs_before_reading_or_embedding() {
    let f = fixture().await;
    let text_adapter = text_builder().build().expect("text");
    let image_adapter = image_builder().build().expect("image");
    let query = "radiografia";
    // The image leg's index searched with the text model: refused by space.
    let mismatch = search_combined(
        &f.store,
        Leg {
            space: &f.text.embedding_space_id,
            embedder: &text_adapter,
        },
        Leg {
            space: &f.image.embedding_space_id,
            embedder: &text_adapter,
        },
        FusionParams::default(),
        query,
        3,
        Scope::Active,
    )
    .await;
    assert!(
        matches!(mismatch, Err(SearchError::SpaceMismatch { .. })),
        "{mismatch:?}"
    );
    // A text model as the image leg, in its own space: no text-to-image capability.
    let unsupported = search_combined(
        &f.store,
        Leg {
            space: &f.text.embedding_space_id,
            embedder: &text_adapter,
        },
        Leg {
            space: &f.text.embedding_space_id,
            embedder: &text_adapter,
        },
        FusionParams::default(),
        query,
        3,
        Scope::Active,
    )
    .await;
    assert!(
        matches!(
            unsupported,
            Err(SearchError::Embedding(EmbeddingError::Unsupported { .. }))
        ),
        "{unsupported:?}"
    );
    assert!(
        text_adapter.calls().is_empty() && image_adapter.calls().is_empty(),
        "no query was embedded before the refusals"
    );
}

#[tokio::test]
async fn the_comparison_report_sets_combined_retrieval_beside_its_legs() {
    let f = fixture().await;
    let corpus = f.manifest.manifest_checksum.clone();
    let combined = combine(
        &f,
        "combined-run",
        ("text-run", "image-run"),
        &f.dataset,
        &corpus,
    )
    .await
    .expect("combined run");
    let report = ComparisonReport::new(vec![f.text.clone(), f.image.clone()], vec![])
        .expect("text and image runs are comparable")
        .with_combined(vec![combined.clone()])
        .expect("combined run is comparable");
    let md = report.to_markdown();
    for section in [
        "## Combined configurations",
        "## Retrieval modes compared",
        "## Combined retrieval against its legs",
        "## Combined efficiency",
        "### combined-run (combined)",
        "| rrf | 60 | all |",
    ] {
        assert!(md.contains(section), "missing {section:?}");
    }
    let delta = combined.evaluation.metrics.ndcg_at_3 - f.text.evaluation.metrics.ndcg_at_3;
    let row = md
        .lines()
        .find(|l| l.starts_with("| combined-run | text | text-run |"))
        .expect("text-leg row");
    assert!(
        row.contains(&format!("{delta:+.4}")),
        "the text-leg nDCG delta is reported: {row}"
    );
    // The JSON report reads back into the same report (combined results included).
    let json = serde_json::to_string(&report).expect("json");
    let parsed: ComparisonReport = serde_json::from_str(&json).expect("parse");
    assert_eq!(parsed.configurations.len(), 2);
    assert_eq!(parsed.combined.len(), 1);
    assert_eq!(
        parsed.combined[0].experiment_id,
        report.combined[0].experiment_id
    );
    assert_eq!(parsed.combined[0].fusion, FusionParams::default());

    // Refusals: another corpus, and a leg run reported in another space.
    let mut other_corpus = combined.clone();
    other_corpus.corpus_checksum = Checksum::of_bytes(b"another manifest");
    let refused = ComparisonReport::new(vec![f.text.clone(), f.image.clone()], vec![])
        .expect("report")
        .with_combined(vec![other_corpus]);
    assert!(
        matches!(
            refused,
            Err(ReportError::NotComparable {
                what: "corpus checksum",
                ..
            })
        ),
        "{refused:?}"
    );
    let mut wrong_leg = combined;
    wrong_leg.text.embedding_space_id = f.image.embedding_space_id.clone();
    let refused = ComparisonReport::new(vec![f.text.clone(), f.image.clone()], vec![])
        .expect("report")
        .with_combined(vec![wrong_leg]);
    assert!(
        matches!(refused, Err(ReportError::LegMismatch { leg: "text", .. })),
        "{refused:?}"
    );
}
