//! Integration tests against real tools and engines (PLAN §5.3).
//!
//! These tests are `#[ignore]`d and also fail fast unless `RUN_INTEGRATION=1`, so the
//! default `cargo test` never depends on installed tools, downloads, or credentials.
//! A test that did not run is outstanding, never passed.

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use embeddings_fundamentals::corpus::LoadedManifest;
use embeddings_fundamentals::domain::{ExtractionMethod, SourceStatus};
use embeddings_fundamentals::embedding::deterministic::DeterministicEmbedder;
use embeddings_fundamentals::extraction::poppler::{
    PopplerImageExtractor, PopplerTextExtractor, TesseractOcr,
};
use embeddings_fundamentals::extraction::{ExtractionConfig, OcrEngine, TextExtractor};
use embeddings_fundamentals::pipeline::{IngestSettings, NoSleep, Pipeline};
use embeddings_fundamentals::storage::FsRecordStore;
use embeddings_fundamentals::tools;

fn require_integration() {
    assert!(
        std::env::var("RUN_INTEGRATION").is_ok_and(|v| v == "1"),
        "outstanding: set RUN_INTEGRATION=1 to run integration tests (not reported as passed)"
    );
}

fn corpus(file: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/corpus")
        .join(file)
}

#[tokio::test]
#[ignore = "requires RUN_INTEGRATION=1 and installed poppler + tesseract"]
async fn required_external_tools_are_installed() {
    require_integration();
    let statuses = tools::probe(tools::REQUIRED_TOOLS).await;
    let missing: Vec<_> = statuses
        .iter()
        .filter(|s| s.version.is_none())
        .map(|s| s.name)
        .collect();
    assert!(missing.is_empty(), "missing external tools: {missing:?}");
}

#[test]
#[ignore = "requires RUN_INTEGRATION=1 and installed poppler + tesseract"]
fn the_experiment_command_reports_a_failed_labeled_source_with_exit_2() {
    // Regression (C2-10): a required, labeled PDF that poppler cannot read makes the
    // ingestion incomplete; the command still writes its report, says the evaluation
    // did not run, and exits 2.
    require_integration();
    let dir = tempfile::tempdir().expect("tmp");
    let corpus_dir = dir.path().join("corpus");
    std::fs::create_dir(&corpus_dir).expect("corpus dir");
    for entry in std::fs::read_dir(corpus("")).expect("fixtures") {
        let path = entry.expect("entry").path();
        std::fs::copy(&path, corpus_dir.join(path.file_name().expect("name"))).expect("copy");
    }
    let broken = b"not a pdf\n";
    std::fs::write(corpus_dir.join("lab-searchable-001.pdf"), broken).expect("write");
    let manifest = std::fs::read_to_string(corpus("manifest.toml")).expect("manifest");
    let (head, tail) = manifest
        .split_once("file = \"lab-searchable-001.pdf\"\nchecksum = \"")
        .expect("searchable source entry");
    let (_, rest) = tail.split_once('"').expect("checksum");
    let checksum = embeddings_fundamentals::ids::Checksum::of_bytes(broken);
    std::fs::write(
        corpus_dir.join("manifest.toml"),
        format!(
            "{head}file = \"lab-searchable-001.pdf\"\nchecksum = \"{}\"{rest}",
            checksum.as_str()
        ),
    )
    .expect("manifest");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let report = dir.path().join("report.json");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_embeddings-fundamentals"))
        .current_dir(root)
        .args(["experiment", "--manifest"])
        .arg(corpus_dir.join("manifest.toml"))
        .args([
            "--config",
            "config/experiments/deterministic.toml",
            "--store",
        ])
        .arg(dir.path().join("store"))
        .args([
            "--experiment",
            "labeled-source-fails",
            "--latency-repeats",
            "1",
        ])
        .arg("--report")
        .arg(&report)
        .output()
        .expect("run the experiment command");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "stderr: {stderr}");
    assert!(stderr.contains("evaluation not run"), "stderr: {stderr}");
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report).expect("report")).expect("json");
    assert_eq!(json["run_status"], "incomplete");
    assert_eq!(json["evaluation"]["status"], "not_run");
    assert_eq!(json["reliability"]["failed_sources"], 1);
    let failed = json["ingestion"]["sources"]
        .as_array()
        .expect("sources")
        .iter()
        .find(|s| s["source_id"] == "lab-searchable-001")
        .expect("source");
    assert_eq!(failed["status"], "failed");
    assert_eq!(failed["stage"], "extraction");
}

#[tokio::test]
#[ignore = "requires RUN_INTEGRATION=1 and installed poppler + tesseract"]
async fn real_tools_select_native_then_ocr_on_the_mixed_pdf() {
    require_integration();
    let manifest = LoadedManifest::load(&corpus("manifest.toml")).expect("manifest");
    let mut manifest_one = manifest.clone();
    manifest_one
        .manifest
        .sources
        .retain(|s| s.source_id.as_str() == "us-mixed-003");
    let store_dir = tempfile::tempdir().expect("tmp");
    let store = Arc::new(FsRecordStore::open_writer(store_dir.path()).expect("store"));
    let text = PopplerTextExtractor::detect().await;
    let ocr = TesseractOcr::detect(ExtractionConfig::default()).await;
    let images = PopplerImageExtractor::detect().await;
    let embedder = Arc::new(DeterministicEmbedder::builder().build().expect("adapter"));
    let pipeline = Pipeline {
        text: &text,
        ocr: &ocr,
        images: &images,
        sleeper: &NoSleep,
        store: Arc::clone(&store),
        embedder,
    };
    let report = pipeline
        .ingest(
            &manifest_one,
            &IngestSettings::new("integration-mixed".parse().expect("id")),
        )
        .await
        .expect("ingest");
    let source = &report.sources[0];
    assert_eq!(
        source.status,
        SourceStatus::Succeeded,
        "{:?}",
        source.reason
    );
    let methods: Vec<_> = source.pages.iter().map(|p| p.method).collect();
    assert_eq!(
        methods,
        vec![ExtractionMethod::Native, ExtractionMethod::Ocr]
    );
    assert!(
        source.pages[1].confidence.is_some(),
        "OCR page reports confidence"
    );
}

#[tokio::test]
#[ignore = "requires RUN_INTEGRATION=1 and installed poppler + tesseract"]
async fn real_ocr_keeps_the_hemoglobin_row_together() {
    require_integration();
    let ocr = TesseractOcr::detect(ExtractionConfig::default()).await;
    let page = ocr
        .ocr_page(&corpus("lab-scanned-002.pdf"), NonZeroU32::MIN)
        .await
        .expect("ocr");
    let row = page
        .lines
        .iter()
        .find(|l| l.text.starts_with("Hemoglobina"))
        .expect("a line starting with Hemoglobina");
    for field in ["13,5", "g/dL", "12,0"] {
        assert!(
            row.text.contains(field),
            "OCR row {:?} lacks {field}",
            row.text
        );
    }
    let bbox = row.bounding_box.as_ref().expect("OCR line box");
    assert!(
        bbox.x > 0.0 && bbox.y > 0.0 && bbox.x + bbox.w < 595.0,
        "box in PDF points: {bbox:?}"
    );
    let text = PopplerTextExtractor::detect().await;
    assert_eq!(
        text.page_count(&corpus("lab-scanned-002.pdf"))
            .await
            .expect("pages")
            .get(),
        2
    );
}

#[cfg(feature = "engine-embed-anything")]
#[tokio::test]
#[ignore = "requires RUN_INTEGRATION=1 and downloads the pinned model"]
async fn embed_anything_adapter_loads_the_pinned_model_and_ranks_sensibly() {
    use embeddings_fundamentals::config::ExperimentConfig;
    use embeddings_fundamentals::embedding::TextInput;
    require_integration();
    let config_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("config/experiments/baseline-text-embed-anything.toml");
    let config = ExperimentConfig::load(&config_path).expect("config");
    let adapter = embeddings_fundamentals::adapters::build(&config.engine)
        .await
        .expect("load model");
    let info = adapter.model_info();
    assert_eq!(info.dimensions.get(), 384);
    assert_eq!(
        info.model_revision.as_deref(),
        config.engine.model_revision.as_deref()
    );
    let docs = [
        ("doc-hb", "Hemograma\nHemoglobina | 13,5 | g/dL | 12,0–16,0"),
        (
            "doc-gl",
            "Bioquímica\nGlicemia de jejum | 92 | mg/dL | 70–99",
        ),
    ];
    let inputs: Vec<TextInput> = docs
        .iter()
        .map(|(id, text)| TextInput {
            id: id.parse().expect("id"),
            text: (*text).to_owned(),
        })
        .collect();
    let batch = adapter
        .embed_documents(&inputs)
        .await
        .expect("embed documents");
    let query = adapter
        .embed_queries(&[TextInput {
            id: "q".parse().expect("id"),
            text: "Onde está o resultado da hemoglobina?".into(),
        }])
        .await
        .expect("embed query");
    let q = &query.embeddings[0].vector;
    let score = |v: &[f32]| v.iter().zip(q).map(|(a, b)| a * b).sum::<f32>();
    for e in &batch.embeddings {
        let norm: f32 = e.vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-4, "vectors are normalized");
    }
    assert!(
        score(&batch.embeddings[0].vector) > score(&batch.embeddings[1].vector),
        "hemoglobin row should outrank glycemia for a hemoglobin query"
    );
    let budget = adapter.input_budget(embeddings_fundamentals::embedding::InputKind::Document);
    assert_eq!(budget, Some(126), "128-token limit minus [CLS] and [SEP]");
    // The counter reports full lengths (tokenizer.json truncates at 128 by itself).
    let long = format!("Hemoglobina | 13,5 | g/dL | {}", "palavra ".repeat(300));
    let tokens = adapter.token_counter().count(&long);
    assert!(
        tokens > 300,
        "counted {tokens} tokens: the tokenizer's own truncation leaked into counting"
    );
    let too_long = TextInput {
        id: "long".parse().expect("id"),
        text: long,
    };
    assert!(
        adapter.embed_documents(&[too_long]).await.is_err(),
        "over-budget input is rejected"
    );
}

#[cfg(feature = "engine-embed-anything")]
#[tokio::test]
#[ignore = "requires RUN_INTEGRATION=1 and downloads the pinned model"]
async fn embed_anything_honours_the_configured_prefix_policy() {
    use embeddings_fundamentals::config::ExperimentConfig;
    use embeddings_fundamentals::embedding::{InputKind, PrefixPolicy};
    require_integration();
    let config_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("config/experiments/baseline-text-embed-anything.toml");
    let none = ExperimentConfig::load(&config_path).expect("config").engine;
    let mut e5 = none.clone();
    e5.prefix_policy = PrefixPolicy::E5;
    let a = embeddings_fundamentals::adapters::build(&none)
        .await
        .expect("none");
    let b = embeddings_fundamentals::adapters::build(&e5)
        .await
        .expect("e5");
    assert_eq!(b.model_info().prefix_policy, PrefixPolicy::E5);
    assert_ne!(
        a.model_info().embedding_space_id,
        b.model_info().embedding_space_id,
        "prefix changes the space"
    );
    let (budget_a, budget_b) = (
        a.input_budget(InputKind::Document),
        b.input_budget(InputKind::Document),
    );
    assert!(
        budget_b < budget_a,
        "the passage prefix consumes budget: {budget_a:?} vs {budget_b:?}"
    );
}

#[cfg(feature = "engine-embed-anything")]
#[tokio::test]
#[ignore = "requires RUN_INTEGRATION=1 and downloads the pinned model"]
async fn embed_anything_always_enforces_a_finite_limit_within_the_model() {
    use embeddings_fundamentals::config::ExperimentConfig;
    use std::num::NonZeroUsize;
    require_integration();
    let config_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("config/experiments/baseline-text-embed-anything.toml");
    let base = ExperimentConfig::load(&config_path).expect("config").engine;
    let mut unset = base.clone();
    unset.max_input_tokens = None;
    let adapter = embeddings_fundamentals::adapters::build(&unset)
        .await
        .expect("load");
    // MiniLM's config.json: max_position_embeddings = 512.
    assert_eq!(
        adapter.capabilities().max_input_tokens,
        NonZeroUsize::new(512),
        "falls back to the model limit"
    );
    let mut oversized = base;
    oversized.max_input_tokens = NonZeroUsize::new(1000);
    let refused = embeddings_fundamentals::adapters::build(&oversized).await;
    assert!(
        refused.is_err(),
        "a limit above the model's would allow silent engine truncation"
    );
}

#[cfg(any(feature = "engine-fastembed", feature = "engine-openai"))]
#[expect(
    clippy::expect_used,
    reason = "test helper: a broken fixture must fail the calling test"
)]
fn experiment(file: &str) -> embeddings_fundamentals::config::EngineConfig {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("config/experiments")
        .join(file);
    embeddings_fundamentals::config::ExperimentConfig::load(&path)
        .expect("config")
        .engine
}

#[cfg(any(feature = "engine-fastembed", feature = "engine-openai"))]
#[expect(
    clippy::expect_used,
    reason = "test helper: a broken fixture must fail the calling test"
)]
async fn hemoglobin_scores(
    adapter: &dyn embeddings_fundamentals::embedding::EmbeddingStrategy,
) -> (Vec<Vec<f32>>, f32, f32) {
    use embeddings_fundamentals::embedding::TextInput;
    let docs = [
        ("doc-hb", "Hemograma\nHemoglobina | 13,5 | g/dL | 12,0–16,0"),
        (
            "doc-gl",
            "Bioquímica\nGlicemia de jejum | 92 | mg/dL | 70–99",
        ),
    ];
    let inputs: Vec<TextInput> = docs
        .iter()
        .map(|(id, text)| TextInput {
            id: id.parse().expect("id"),
            text: (*text).to_owned(),
        })
        .collect();
    let batch = adapter.embed_documents(&inputs).await.expect("documents");
    let query = adapter
        .embed_queries(&[TextInput {
            id: "q".parse().expect("id"),
            text: "Onde está o resultado da hemoglobina?".into(),
        }])
        .await
        .expect("query");
    let q = &query.embeddings[0].vector;
    let score = |v: &[f32]| v.iter().zip(q).map(|(a, b)| a * b).sum::<f32>();
    let vectors: Vec<Vec<f32>> = batch.embeddings.into_iter().map(|e| e.vector).collect();
    for v in &vectors {
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-4, "vectors are normalized");
    }
    let (hb, gl) = (score(&vectors[0]), score(&vectors[1]));
    (vectors, hb, gl)
}

#[cfg(feature = "engine-fastembed")]
#[tokio::test]
#[ignore = "requires RUN_INTEGRATION=1 and downloads the pinned ONNX model"]
async fn fastembed_minilm_loads_the_pinned_onnx_export_and_ranks_sensibly() {
    use embeddings_fundamentals::embedding::{EngineId, InputKind};
    require_integration();
    let config = experiment("text-fastembed-minilm.toml");
    let adapter = embeddings_fundamentals::adapters::build(&config)
        .await
        .expect("load model");
    let info = adapter.model_info();
    assert_eq!(info.engine, EngineId::FastEmbed);
    assert_eq!(info.dimensions.get(), 384);
    assert_eq!(info.weight_format.as_deref(), Some("onnx"));
    assert_eq!(info.model_revision, config.model_revision);
    assert_eq!(adapter.input_budget(InputKind::Document), Some(126));
    let (_, hb, gl) = hemoglobin_scores(adapter.as_ref()).await;
    assert!(hb > gl, "hemoglobin row outranks glycemia ({hb} vs {gl})");
}

#[cfg(all(feature = "engine-fastembed", feature = "engine-embed-anything"))]
#[tokio::test]
#[ignore = "requires RUN_INTEGRATION=1 and downloads both pinned models"]
async fn the_same_minilm_gives_near_identical_vectors_on_both_engines() {
    require_integration();
    let onnx = embeddings_fundamentals::adapters::build(&experiment("text-fastembed-minilm.toml"))
        .await
        .expect("fastembed");
    let candle =
        embeddings_fundamentals::adapters::build(&experiment("baseline-text-embed-anything.toml"))
            .await
            .expect("embed_anything");
    assert_ne!(
        onnx.model_info().embedding_space_id,
        candle.model_info().embedding_space_id,
        "different engines are different spaces even for the same model"
    );
    let (a, _, _) = hemoglobin_scores(onnx.as_ref()).await;
    let (b, _, _) = hemoglobin_scores(candle.as_ref()).await;
    for (x, y) in a.iter().zip(&b) {
        let cosine: f32 = x.iter().zip(y).map(|(p, q)| p * q).sum();
        assert!(cosine > 0.99, "cross-engine cosine {cosine}");
    }
}

#[cfg(feature = "engine-fastembed")]
#[tokio::test]
#[ignore = "requires RUN_INTEGRATION=1 and downloads the pinned ONNX model"]
async fn fastembed_e5_applies_its_prefixes_within_the_model_limit() {
    use embeddings_fundamentals::embedding::{InputKind, PrefixPolicy};
    use std::num::NonZeroUsize;
    require_integration();
    let config = experiment("text-fastembed-e5-base.toml");
    let adapter = embeddings_fundamentals::adapters::build(&config)
        .await
        .expect("load model");
    let info = adapter.model_info();
    assert_eq!(info.dimensions.get(), 768);
    assert_eq!(info.prefix_policy, PrefixPolicy::E5);
    let counter = adapter.token_counter();
    let doc = adapter.input_budget(InputKind::Document).expect("budget");
    let query = adapter.input_budget(InputKind::Query).expect("budget");
    assert_eq!(doc, 512 - 2 - counter.count("passage: "));
    assert_eq!(query, 512 - 2 - counter.count("query: "));
    let (_, hb, gl) = hemoglobin_scores(adapter.as_ref()).await;
    assert!(hb > gl, "hemoglobin row outranks glycemia ({hb} vs {gl})");
    let mut oversized = config;
    oversized.max_input_tokens = NonZeroUsize::new(513);
    assert!(
        embeddings_fundamentals::adapters::build(&oversized)
            .await
            .is_err(),
        "513 is above XLM-RoBERTa's 512 usable positions"
    );
}

#[cfg(feature = "engine-openai")]
#[tokio::test]
#[ignore = "requires RUN_INTEGRATION=1, OPENAI_API_KEY, and owner permission for remote processing"]
async fn openai_live_embeddings_rank_sensibly() {
    require_integration();
    assert!(
        std::env::var("OPENAI_REMOTE_ALLOWED").is_ok_and(|v| v == "1"),
        "outstanding: the owner has not authorized remote processing (OPENAI_REMOTE_ALLOWED=1)"
    );
    let adapter = embeddings_fundamentals::adapters::build(&experiment("text-openai-3-small.toml"))
        .await
        .expect("adapter (OPENAI_API_KEY)");
    assert_eq!(adapter.model_info().dimensions.get(), 1536);
    let (_, hb, gl) = hemoglobin_scores(adapter.as_ref()).await;
    assert!(hb > gl, "hemoglobin row outranks glycemia ({hb} vs {gl})");
}

#[tokio::test]
#[ignore = "requires RUN_INTEGRATION=1 and installed poppler + tesseract"]
async fn a_real_image_run_keeps_embedded_images_and_drops_page_scans() {
    use embeddings_fundamentals::assets::MetadataScrubber;
    use embeddings_fundamentals::domain::Modality;
    use embeddings_fundamentals::extraction::images::ImageExtractor;
    require_integration();
    let manifest = LoadedManifest::load(&corpus("manifest.toml")).expect("manifest");
    let store_dir = tempfile::tempdir().expect("tmp");
    let store = Arc::new(FsRecordStore::open_writer(store_dir.path()).expect("store"));
    let text = PopplerTextExtractor::detect().await;
    let ocr = TesseractOcr::detect(ExtractionConfig::default()).await;
    let images = PopplerImageExtractor::detect().await;
    let embedder = Arc::new(
        DeterministicEmbedder::builder()
            .text_to_image()
            .build()
            .expect("adapter"),
    );
    let space = embeddings_fundamentals::embedding::EmbeddingStrategy::model_info(&*embedder)
        .embedding_space_id
        .clone();
    let pipeline = Pipeline {
        text: &text,
        ocr: &ocr,
        images: &images,
        sleeper: &NoSleep,
        store: Arc::clone(&store),
        embedder,
    };
    let mut settings = IngestSettings::new("integration-images".parse().expect("id"));
    settings.modality = Modality::Image;
    let report = pipeline.ingest(&manifest, &settings).await.expect("ingest");
    let status = |id: &str| {
        report
            .sources
            .iter()
            .find(|s| s.source_id.as_str() == id)
            .map(|s| (s.status, s.records_committed))
    };
    // The image files and the embedded page-1 image of the mixed PDF become records; the
    // page-2 raster of the mixed PDF is its OCR scan, never an image record.
    assert_eq!(status("chest-001"), Some((SourceStatus::Succeeded, 1)));
    assert_eq!(status("abd-us-001"), Some((SourceStatus::Succeeded, 1)));
    assert_eq!(status("us-mixed-003"), Some((SourceStatus::Succeeded, 1)));
    assert_eq!(
        status("lab-searchable-001").map(|s| s.0),
        Some(SourceStatus::NotApplicable)
    );
    let records = store.active_records(&space).expect("records");
    let embedded = records
        .iter()
        .find(|r| r.source_id.as_str() == "us-mixed-003")
        .expect("embedded image record");
    assert_eq!(embedded.metadata.page_number, 1);
    assert_eq!(embedded.chunk_text, None);
    let original = embedded
        .metadata
        .original_asset
        .as_ref()
        .expect("original asset");
    // The stored asset is the metadata-free re-encoding of what pdfimages extracted.
    let pdf = corpus("us-mixed-003.pdf");
    let listed = images.list_images(&pdf).await.expect("list");
    let image = listed
        .iter()
        .find(|i| Some(i.num) == original.pdf_image_num)
        .expect("listed image");
    let bytes = images.extract_image(&pdf, image).await.expect("extract");
    let expected = MetadataScrubber.scrub(&bytes).expect("scrub");
    let stored = store
        .read_asset(embedded.asset_uri.as_deref().expect("asset uri"))
        .expect("asset");
    assert_eq!(stored, expected.png);
    assert_eq!(
        (original.width, original.height),
        (expected.width, expected.height)
    );
}

#[cfg(feature = "engine-embed-anything")]
#[expect(
    clippy::expect_used,
    reason = "test helper: a broken fixture or model must fail the calling test"
)]
async fn cross_modal_scores(
    config_file: &str,
    expected_dims: usize,
) -> std::collections::BTreeMap<(String, String), f32> {
    use embeddings_fundamentals::config::ExperimentConfig;
    use embeddings_fundamentals::embedding::{
        AssetRef, Capability, EmbeddingError, InputKind, TextInput,
    };
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("config/experiments")
        .join(config_file);
    let config = ExperimentConfig::load(&path).expect("config");
    let adapter = embeddings_fundamentals::adapters::build(&config.engine)
        .await
        .expect("load model");
    let info = adapter.model_info();
    assert_eq!(info.dimensions.get(), expected_dims);
    assert_eq!(info.model_revision, config.engine.model_revision);
    let caps = adapter.capabilities();
    assert!(caps.images && caps.text_queries && caps.text_to_image_compatible);
    assert!(!caps.text_documents);
    let doc = TextInput {
        id: "doc".parse().expect("id"),
        text: "Hemoglobina".into(),
    };
    assert!(
        matches!(
            adapter.embed_documents(&[doc]).await,
            Err(EmbeddingError::Unsupported {
                capability: Capability::TextDocuments,
                ..
            })
        ),
        "a cross-modal model embeds no document text"
    );
    let images = ["chest-001.png", "abd-us-001.jpg"];
    let assets: Vec<AssetRef> = images
        .iter()
        .enumerate()
        .map(|(i, file)| AssetRef {
            id: format!("img-{i}").parse().expect("id"),
            path: corpus(file),
        })
        .collect();
    let batch = adapter.embed_images(&assets).await.expect("embed images");
    // A batch gives the same vectors as single calls.
    let single = adapter
        .embed_images(&assets[1..])
        .await
        .expect("embed one image");
    let drift = batch.embeddings[1]
        .vector
        .iter()
        .zip(&single.embeddings[0].vector)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0_f32, f32::max);
    assert!(drift < 1e-4, "batch and single vectors differ by {drift}");
    let queries = [
        "Mostre a radiografia de tórax",
        "Encontre as imagens de ultrassonografia abdominal",
        "a chest x-ray",
        "an abdominal ultrasound image",
    ];
    let inputs: Vec<TextInput> = queries
        .iter()
        .enumerate()
        .map(|(i, text)| TextInput {
            id: format!("q-{i}").parse().expect("id"),
            text: (*text).to_owned(),
        })
        .collect();
    let embedded = adapter.embed_queries(&inputs).await.expect("embed queries");
    for e in batch.embeddings.iter().chain(&embedded.embeddings) {
        assert_eq!(e.vector.len(), expected_dims);
        let norm: f32 = e.vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-4, "vectors are normalized");
    }
    // A query above the text position limit is refused, never truncated or a panic.
    let budget = adapter.input_budget(InputKind::Query).expect("budget");
    let long = TextInput {
        id: "long".parse().expect("id"),
        text: "radiografia ".repeat(budget + 10),
    };
    assert!(
        matches!(
            adapter.embed_queries(&[long]).await,
            Err(EmbeddingError::InputTooLong { .. })
        ),
        "an over-limit query is refused"
    );
    let mut scores = std::collections::BTreeMap::new();
    for (q, query) in embedded.embeddings.iter().zip(queries) {
        for (image, file) in batch.embeddings.iter().zip(images) {
            let score = q.vector.iter().zip(&image.vector).map(|(a, b)| a * b).sum();
            scores.insert((query.to_owned(), file.to_owned()), score);
        }
    }
    for ((query, file), score) in &scores {
        eprintln!("{config_file}\t{query}\t{file}\t{score:.4}");
    }
    scores
}

#[cfg(feature = "engine-embed-anything")]
#[tokio::test]
#[ignore = "requires RUN_INTEGRATION=1 and downloads the pinned CLIP checkpoint"]
async fn clip_loads_the_pinned_checkpoint_and_embeds_queries_and_images() {
    require_integration();
    let scores = cross_modal_scores("image-clip-vit-b32.toml", 512).await;
    let s = |q: &str, f: &str| scores[&(q.to_owned(), f.to_owned())];
    assert!(
        s("a chest x-ray", "chest-001.png") > s("a chest x-ray", "abd-us-001.jpg"),
        "CLIP ranks the chest image first for an English chest query"
    );
}

#[cfg(feature = "engine-embed-anything")]
#[tokio::test]
#[ignore = "requires RUN_INTEGRATION=1 and downloads the pinned SigLIP checkpoint"]
async fn siglip_loads_the_pinned_checkpoint_and_embeds_queries_and_images() {
    require_integration();
    let scores = cross_modal_scores("image-siglip-multilingual.toml", 768).await;
    let s = |q: &str, f: &str| scores[&(q.to_owned(), f.to_owned())];
    let chest = "Mostre a radiografia de tórax";
    assert!(
        s(chest, "chest-001.png") > s(chest, "abd-us-001.jpg"),
        "SigLIP ranks the chest image first for the Portuguese chest query"
    );
}
