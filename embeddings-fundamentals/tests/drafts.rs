//! Drafts of the owner inputs (`TESTS.md`): the manifest draft reproduces the measured
//! fields of the synthetic corpus manifest and stays refused until reviewed, and the
//! label draft pools candidates whose anchors and asset checksums the evaluator
//! accepts once every judgment is filled in.

use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::Arc;

use embeddings_fundamentals::corpus::{LoadedManifest, Manifest};
use embeddings_fundamentals::domain::Modality;
use embeddings_fundamentals::drafts::{self, PoolRun};
use embeddings_fundamentals::embedding::EmbeddingStrategy;
use embeddings_fundamentals::embedding::deterministic::{
    DeterministicBuilder, DeterministicEmbedder,
};
use embeddings_fundamentals::evaluation::{
    Dataset, Frozen, LabelSet, check_image_labels, check_text_labels, evaluate_images,
    evaluate_text, load_queries,
};
use embeddings_fundamentals::extraction::ExtractionConfig;
use embeddings_fundamentals::extraction::fixture::FixtureExtractor;
use embeddings_fundamentals::extraction::images::ImageFilter;
use embeddings_fundamentals::ids::{Checksum, ExperimentId};
use embeddings_fundamentals::pipeline::{IngestSettings, NoSleep, Pipeline};
use embeddings_fundamentals::retrieval::Scope;
use embeddings_fundamentals::storage::FsRecordStore;

fn repo(path: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

#[expect(clippy::expect_used, reason = "test helper")]
fn fixtures() -> FixtureExtractor {
    FixtureExtractor::from_dir(&repo("tests/fixtures/extraction")).expect("fixtures")
}

#[tokio::test]
async fn the_manifest_draft_measures_the_synthetic_corpus_and_is_refused_until_reviewed() {
    let dir = repo("tests/fixtures/corpus");
    let files = drafts::corpus_files(&dir).expect("files");
    let extractor = fixtures();
    let draft = drafts::draft_manifest(
        &extractor,
        &extractor,
        &dir,
        &files,
        &ExtractionConfig::default(),
        &ImageFilter::default(),
        "synthetic-fixtures-v1",
    )
    .await
    .expect("draft");

    // Checksums, source ids, and the measured input types match the reviewed manifest.
    let committed = LoadedManifest::load(&dir.join("manifest.toml")).expect("manifest");
    let mut expected: Vec<_> = committed
        .manifest
        .sources
        .iter()
        .map(|s| {
            (
                s.source_id.to_string(),
                s.file.display().to_string(),
                s.checksum.to_string(),
                s.input_type,
            )
        })
        .collect();
    expected.sort_by(|a, b| a.1.cmp(&b.1));
    let drafted: Vec<_> = draft
        .sources
        .iter()
        .map(|s| {
            (
                s.source_id.to_string(),
                s.file.clone(),
                s.checksum.to_string(),
                s.input_type,
            )
        })
        .collect();
    assert_eq!(drafted, expected);
    assert!(draft.sources.iter().all(|s| !s.name_flagged()));

    // The rendered draft parses, but the gate refuses every source: no review yet.
    let rendered = draft.render();
    let parsed: Manifest = toml::from_str(&rendered).expect("draft parses");
    assert!(!parsed.remote_allowed);
    let loaded = LoadedManifest::from_manifest(parsed, &dir, rendered.as_bytes()).expect("load");
    for source in &loaded.manifest.sources {
        assert!(
            loaded.verify(source).is_err(),
            "{} must be refused",
            source.source_id
        );
    }

    // Recording the review is the only edit needed for every source to pass the gate.
    let reviewed = rendered
        .replace("# anonymization = ", "anonymization = ")
        .replace("<name>", "owner")
        .replace("<YYYY-MM-DD>", "2026-10-29");
    let parsed: Manifest = toml::from_str(&reviewed).expect("reviewed draft parses");
    let loaded = LoadedManifest::from_manifest(parsed, &dir, reviewed.as_bytes()).expect("load");
    for source in &loaded.manifest.sources {
        loaded
            .verify(source)
            .expect("reviewed source passes the gate");
    }
}

#[tokio::test]
async fn a_flagged_file_name_never_becomes_a_source_id() {
    let dir = tempfile::tempdir().expect("tmp");
    std::fs::copy(
        repo("tests/fixtures/corpus/chest-001.png"),
        dir.path().join("FULANO DE TAL.png"),
    )
    .expect("copy");
    std::fs::copy(
        repo("tests/fixtures/corpus/chest-001.png"),
        dir.path().join(".hidden.png"),
    )
    .expect("copy");
    std::fs::write(dir.path().join("notes.txt"), "not a corpus file").expect("write");
    let files = drafts::corpus_files(dir.path()).expect("files");
    assert_eq!(files, ["FULANO DE TAL.png"]);
    let extractor = fixtures();
    let draft = drafts::draft_manifest(
        &extractor,
        &extractor,
        dir.path(),
        &files,
        &ExtractionConfig::default(),
        &ImageFilter::default(),
        "flagged",
    )
    .await
    .expect("draft");
    let source = &draft.sources[0];
    assert!(source.name_flagged());
    assert_eq!(source.source_id.as_str(), "source-001");
    // Findings name patterns, never the matched text.
    assert!(source.findings.iter().all(|f| !f.detail.contains("FULANO")));
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

#[expect(clippy::expect_used, reason = "test helper")]
async fn ingest(
    store: &Arc<FsRecordStore>,
    manifest: &LoadedManifest,
    id: &str,
    modality: Modality,
    embedder: &Arc<dyn EmbeddingStrategy>,
) {
    let extractor = fixtures();
    let mut settings = IngestSettings::new(id.parse().expect("id"));
    settings.modality = modality;
    let pipeline = Pipeline {
        text: &extractor,
        ocr: &extractor,
        images: &extractor,
        sleeper: &NoSleep,
        store: Arc::clone(store),
        embedder: Arc::clone(embedder),
    };
    pipeline.ingest(manifest, &settings).await.expect("ingest");
}

#[tokio::test]
async fn a_filled_label_draft_applies_to_the_runs_it_was_pooled_from() {
    let manifest =
        LoadedManifest::load(&repo("tests/fixtures/corpus/manifest.toml")).expect("manifest");
    let dir = tempfile::tempdir().expect("tmp");
    let store = Arc::new(FsRecordStore::open_writer(dir.path()).expect("store"));
    let text: Arc<dyn EmbeddingStrategy> = Arc::new(
        keyword_builder("tests/fixtures/evaluation/keywords.toml")
            .build()
            .expect("text model"),
    );
    let image: Arc<dyn EmbeddingStrategy> = Arc::new(
        keyword_builder("tests/fixtures/evaluation/image-keywords.toml")
            .model_id("deterministic-image-v1")
            .text_to_image()
            .build()
            .expect("image model"),
    );
    ingest(&store, &manifest, "text-run", Modality::Text, &text).await;
    ingest(&store, &manifest, "image-run", Modality::Image, &image).await;
    let (text_id, image_id): (ExperimentId, ExperimentId) = (
        "text-run".parse().expect("id"),
        "image-run".parse().expect("id"),
    );
    let text_snapshot = store.load_experiment(&text_id).expect("text snapshot");
    let image_snapshot = store.load_experiment(&image_id).expect("image snapshot");
    let (text_space, image_space) = (
        text.model_info().embedding_space_id.clone(),
        image.model_info().embedding_space_id.clone(),
    );
    let queries = load_queries(&repo("config/evaluation/queries-v1.toml")).expect("queries");
    let depth = 4;
    let draft = drafts::draft_labels(
        &store,
        &queries.value,
        &[
            PoolRun {
                experiment: &text_id,
                modality: Modality::Text,
                space: &text_space,
                embedder: &*text,
                scope: Scope::Experiment(&text_snapshot),
            },
            PoolRun {
                experiment: &image_id,
                modality: Modality::Image,
                space: &image_space,
                embedder: &*image,
                scope: Scope::Experiment(&image_snapshot),
            },
        ],
        depth,
        "synthetic",
    )
    .await
    .expect("draft");

    assert_eq!(draft.queries.len(), queries.value.queries.len());
    for q in &draft.queries {
        assert!(!q.candidates.is_empty(), "{} pooled nothing", q.id);
        let ids: std::collections::BTreeSet<_> = q.candidates.iter().map(|c| &c.chunk_id).collect();
        assert_eq!(
            ids.len(),
            q.candidates.len(),
            "{} repeats a candidate",
            q.id
        );
        assert!(q.candidates.len() <= 2 * depth);
        for c in &q.candidates {
            assert!(
                c.found_by
                    .iter()
                    .all(|(_, rank)| (1..=depth).contains(rank))
            );
            assert!(c.conflicts.is_empty());
            if c.modality == Modality::Image {
                assert!(c.original.is_some() && c.asset_checksum.is_some());
            }
        }
    }

    // Unjudged, the draft does not load as a label file.
    let rendered = draft.render();
    assert!(toml::from_str::<LabelSet>(&rendered).is_err());

    // Judged, it loads, and every anchor and asset checksum applies to its run.
    let judged = rendered
        .replace("label = \"TODO\"  # 0, 1, or 2", "label = 1")
        .replace("note = \"TODO\"", "note = \"judged\"");
    let labels: LabelSet = toml::from_str(&judged).expect("judged draft parses");
    assert_eq!(labels.corpus, "synthetic");
    assert_eq!(labels.query_set, queries.value.version);
    let dataset = Dataset::new(
        queries.clone(),
        Frozen {
            value: labels,
            checksum: Checksum::of_bytes(judged.as_bytes()),
        },
    )
    .expect("dataset");
    let text_records = store
        .records(&text_snapshot.revisions)
        .expect("text records");
    let image_records = store
        .records(&image_snapshot.revisions)
        .expect("image records");
    check_text_labels(&dataset, &text_records).expect("text labels apply");
    check_image_labels(&dataset, &image_records).expect("image labels apply");
    let text_report = evaluate_text(
        &store,
        &text_space,
        &*text,
        &dataset,
        Scope::Experiment(&text_snapshot),
    )
    .await
    .expect("text evaluation");
    let image_report = evaluate_images(
        &store,
        &image_space,
        &*image,
        &dataset,
        Scope::Experiment(&image_snapshot),
    )
    .await
    .expect("image evaluation");
    // Every top-3 hit was pooled (depth 4 covers the top 3), so every one is judged.
    for report in [&text_report, &image_report] {
        for entry in &report.queries {
            assert!(
                entry.results.iter().all(|r| r.labeled),
                "{}",
                entry.query_id
            );
        }
    }
}
