//! Local, opt-in tests over the owner's **real** sample exams in `tests/sample-exams/`.
//!
//! That directory is git-ignored and never leaves this machine (`CLAUDE.md`, "Real
//! sample exams"). These tests:
//! - run only with `RUN_SAMPLE_EXAMS=1` (and `--include-ignored`), never in CI;
//! - work on temporary copies and persist nothing in the repository;
//! - name a document by its index in sorted order (`exam #3`), never by its file name,
//!   and never print its text;
//! - assert properties of the pipeline on real documents, never document-specific
//!   values.
//!
//! ```bash
//! RUN_SAMPLE_EXAMS=1 cargo test --test sample_exams -- --include-ignored
//! ```

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use embeddings_fundamentals::anonymization::filename_findings;
use embeddings_fundamentals::assets::MetadataScrubber;
use embeddings_fundamentals::chunking::{ChunkError, ChunkingConfig, StructureAwareV1};
use embeddings_fundamentals::corpus::{LoadedManifest, Manifest, SourceEntry};
use embeddings_fundamentals::domain::{DocumentType, ExtractionMethod, InputType};
use embeddings_fundamentals::embedding::TokenCounter;
use embeddings_fundamentals::embedding::deterministic::{
    DeterministicEmbedder, WhitespaceTokenCounter,
};
use embeddings_fundamentals::extraction::ExtractionConfig;
use embeddings_fundamentals::extraction::images::{ImageExtractor, ImageFilter, mask_of};
use embeddings_fundamentals::extraction::poppler::{
    PopplerImageExtractor, PopplerTextExtractor, TesseractOcr,
};
use embeddings_fundamentals::ids::Checksum;
use embeddings_fundamentals::pipeline::{
    EXIT_VALIDATION, IngestSettings, NoSleep, Pipeline, ValidationError, extract_pdf,
};
use embeddings_fundamentals::storage::FsRecordStore;

/// The sample exams, sorted by file name; fails unless the opt-in is set.
#[expect(
    clippy::expect_used,
    reason = "test helper: a missing opt-in must fail"
)]
fn exams() -> Vec<PathBuf> {
    assert_eq!(
        std::env::var("RUN_SAMPLE_EXAMS").as_deref(),
        Ok("1"),
        "set RUN_SAMPLE_EXAMS=1 to run the real sample-exam tests (local only)"
    );
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/sample-exams");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("tests/sample-exams is missing (owner-supplied, local only)")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "tests/sample-exams holds no PDF");
    files
}

/// A copy of every exam in a fresh temporary corpus, with a manifest that marks each
/// source eligible but records **no** anonymization review.
#[expect(clippy::expect_used, reason = "test helper")]
fn unreviewed_corpus(files: &[PathBuf]) -> (tempfile::TempDir, LoadedManifest) {
    let dir = tempfile::tempdir().expect("tmp");
    let sources = files
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let name = path.file_name().expect("name");
            std::fs::copy(path, dir.path().join(name)).expect("copy");
            SourceEntry {
                source_id: format!("exam-{i:02}").parse().expect("id"),
                file: PathBuf::from(name),
                checksum: Checksum::of_bytes(&std::fs::read(path).expect("read")),
                document_type: DocumentType::ImagingReport,
                input_type: InputType::MixedPdf,
                eligible: true,
                required: true,
                anonymization: None,
            }
        })
        .collect();
    let manifest = Manifest {
        corpus_id: "local-sample-exams".to_owned(),
        remote_allowed: false,
        sources,
    };
    let path = dir.path().join("manifest.toml");
    std::fs::write(&path, toml::to_string(&manifest).expect("toml")).expect("write");
    let loaded = LoadedManifest::load(&path).expect("manifest");
    (dir, loaded)
}

#[test]
#[ignore = "local only: requires RUN_SAMPLE_EXAMS=1 and the owner's tests/sample-exams"]
fn every_real_exam_name_is_flagged_for_review_without_revealing_it() {
    for (i, path) in exams().iter().enumerate() {
        let name = path
            .file_name()
            .expect("name")
            .to_string_lossy()
            .into_owned();
        // Only the file name: the directories above it are the repository's own.
        let findings = filename_findings(Path::new(&name));
        assert!(
            !findings.is_empty(),
            "exam #{i}: its file name is not flagged for the anonymization review"
        );
        let words: Vec<&str> = name
            .split(|c: char| !c.is_alphabetic())
            .filter(|w| w.chars().count() >= 3)
            .collect();
        for f in &findings {
            assert!(
                words.iter().all(|w| !f.detail.contains(w)),
                "exam #{i}: a finding repeats part of the file name"
            );
        }
    }
}

#[tokio::test]
#[ignore = "local only: requires RUN_SAMPLE_EXAMS=1 and the owner's tests/sample-exams"]
async fn the_gate_refuses_every_unreviewed_real_exam_before_any_processing() {
    let files = exams();
    let (_corpus, manifest) = unreviewed_corpus(&files);
    let store_dir = tempfile::tempdir().expect("tmp");
    let store = Arc::new(FsRecordStore::open_writer(store_dir.path()).expect("store"));
    let embedder = Arc::new(DeterministicEmbedder::builder().build().expect("adapter"));
    let text = PopplerTextExtractor::detect().await;
    let ocr = TesseractOcr::detect(ExtractionConfig::default()).await;
    let images = PopplerImageExtractor::detect().await;
    let pipeline = Pipeline {
        text: &text,
        ocr: &ocr,
        images: &images,
        sleeper: &NoSleep,
        store: Arc::clone(&store),
        embedder: Arc::clone(&embedder)
            as Arc<dyn embeddings_fundamentals::embedding::EmbeddingStrategy>,
    };
    let outcome = pipeline
        .ingest(
            &manifest,
            &IngestSettings::new("sample-exams".parse().expect("id")),
        )
        .await;
    let Err(error) = outcome else {
        panic!("an unreviewed real corpus was ingested");
    };
    assert_eq!(error.exit_code(), EXIT_VALIDATION);
    let ValidationError::Ineligible(refused) = error else {
        panic!("refused for another reason than eligibility");
    };
    let refused: BTreeSet<String> = refused.iter().map(|(id, _)| id.to_string()).collect();
    let all: BTreeSet<String> = (0..files.len()).map(|i| format!("exam-{i:02}")).collect();
    assert_eq!(refused, all, "every unreviewed exam is refused");
    assert!(embedder.calls().is_empty(), "nothing was embedded");
    assert!(
        store.spaces().expect("spaces").is_empty(),
        "nothing was written to the store"
    );
}

/// Visible-text tokens of a page, for coverage checks.
fn tokens(text: &str) -> BTreeSet<String> {
    text.split_whitespace().map(str::to_owned).collect()
}

#[tokio::test]
#[ignore = "local only: requires RUN_SAMPLE_EXAMS=1 and the owner's tests/sample-exams"]
async fn real_exams_extract_deterministically_and_chunk_within_the_invariants() {
    let config = ExtractionConfig::default();
    let text = PopplerTextExtractor::detect().await;
    let ocr = TesseractOcr::detect(config.clone()).await;
    let counter = WhitespaceTokenCounter;
    let unlimited = StructureAwareV1::new(ChunkingConfig::default()).expect("chunker");
    let truncating = StructureAwareV1::new(ChunkingConfig {
        truncation_enabled: true,
        ..ChunkingConfig::default()
    })
    .expect("chunker");
    let (mut native_pages, mut ocr_pages, mut chunks_seen) = (0, 0, 0);
    for (i, path) in exams().iter().enumerate() {
        let pages = extract_pdf(&text, &ocr, path, &config, |_| {})
            .await
            .unwrap_or_else(|e| panic!("exam #{i}: extraction failed: {e:?}"));
        assert!(!pages.is_empty(), "exam #{i}: no page");
        // Reproducibility: a second extraction gives the same pages.
        let again = extract_pdf(&text, &ocr, path, &config, |_| {})
            .await
            .expect("second extraction");
        for (p, (a, b)) in pages.iter().zip(&again).enumerate() {
            assert!(
                a.method == b.method && a.text() == b.text(),
                "exam #{i} page {}: extraction is not deterministic",
                p + 1
            );
        }
        for (p, page) in pages.iter().enumerate() {
            let at = format!("exam #{i} page {}", p + 1);
            assert_eq!(page.page.get() as usize, p + 1, "{at}: page order");
            match page.method {
                ExtractionMethod::Native => {
                    native_pages += 1;
                    assert!(
                        config.native_text_is_usable(&page.text()),
                        "{at}: an unusable text layer was kept instead of OCR"
                    );
                }
                ExtractionMethod::Ocr => ocr_pages += 1,
                other => panic!("{at}: unexpected extraction method {other:?}"),
            }

            // Without a budget nothing is dropped: every token of the page reaches a chunk.
            let chunks = unlimited.chunk(page, &counter, None).expect("unlimited");
            let mut covered = BTreeSet::new();
            for (n, c) in chunks.iter().enumerate() {
                assert_eq!(c.index as usize, n, "{at}: chunk indices are contiguous");
                assert_eq!(c.page, page.page, "{at}: chunk page");
                assert_eq!(c.extraction_method, page.method, "{at}: chunk provenance");
                assert!(!c.text.trim().is_empty(), "{at}: empty chunk");
                assert!(c.truncation.is_none(), "{at}: truncated without a budget");
                covered.extend(tokens(&c.text));
            }
            let missing = tokens(&page.text()).difference(&covered).count();
            assert_eq!(missing, 0, "{at}: {missing} page tokens reached no chunk");
            chunks_seen += chunks.len();

            for budget in [16, 64, 512] {
                // With truncation every chunk fits, and a cut is always recorded.
                for c in truncating
                    .chunk(page, &counter, Some(budget))
                    .expect("truncating chunker never refuses")
                {
                    let n = counter.count(&c.text);
                    assert!(n <= budget, "{at}: {n} tokens over a {budget} budget");
                    if let Some(t) = c.truncation {
                        assert!(
                            t.original_tokens > budget && t.submitted_tokens == n,
                            "{at}: inconsistent truncation record"
                        );
                    }
                }
                // Without truncation an over-budget unit is refused, never emitted.
                match unlimited.chunk(page, &counter, Some(budget)) {
                    Ok(chunks) => assert!(
                        chunks.iter().all(|c| counter.count(&c.text) <= budget),
                        "{at}: a chunk over the {budget} budget was emitted"
                    ),
                    Err(ChunkError::InputTooLong { tokens, limit, .. }) => assert!(
                        tokens > limit && limit == budget,
                        "{at}: inconsistent refusal"
                    ),
                    Err(other) => panic!("{at}: unexpected chunking error {other:?}"),
                }
            }
        }
    }
    eprintln!(
        "sample exams: {native_pages} native pages, {ocr_pages} OCR pages, {chunks_seen} chunks"
    );
    assert!(native_pages > 0, "no real page used its text layer");
}

/// PNG chunk types of `png`.
#[expect(clippy::expect_used, reason = "test helper")]
fn png_chunk_types(png: &[u8]) -> Vec<String> {
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"), "not a PNG");
    let mut types = Vec::new();
    let mut at = 8;
    while at + 8 <= png.len() {
        let len = u32::from_be_bytes(png[at..at + 4].try_into().expect("length")) as usize;
        types.push(String::from_utf8_lossy(&png[at + 4..at + 8]).into_owned());
        at += 12 + len;
    }
    types
}

#[tokio::test]
#[ignore = "local only: requires RUN_SAMPLE_EXAMS=1 and the owner's tests/sample-exams"]
async fn real_exam_images_are_filtered_and_stored_without_metadata() {
    let extractor = PopplerImageExtractor::detect().await;
    let filter = ImageFilter::default();
    let (mut listed_total, mut kept_total, mut prepared_total) = (0, 0, 0);
    for (i, path) in exams().iter().enumerate() {
        let listed = extractor
            .list_images(path)
            .await
            .unwrap_or_else(|e| panic!("exam #{i}: pdfimages -list failed: {e:?}"));
        listed_total += listed.len();
        let kept: Vec<_> = listed.iter().filter(|info| filter.keeps(info)).collect();
        kept_total += kept.len();
        for info in &kept {
            assert!(
                info.width >= filter.min_side_px && info.height >= filter.min_side_px,
                "exam #{i}: a small image passed the filter"
            );
        }
        // Prepare up to three kept images per exam, with their masks, as ingestion does.
        for info in kept.iter().take(3) {
            let at = format!("exam #{i} page {} image {}", info.page, info.num);
            let bytes = extractor
                .extract_image(path, info)
                .await
                .unwrap_or_else(|e| panic!("{at}: extraction failed: {e:?}"));
            let mask = match mask_of(&listed, info) {
                Some(mask) => Some(
                    extractor
                        .extract_image(path, mask)
                        .await
                        .unwrap_or_else(|e| panic!("{at}: mask extraction failed: {e:?}")),
                ),
                None => None,
            };
            let prepared = MetadataScrubber
                .scrub_masked(&bytes, mask.as_deref())
                .unwrap_or_else(|e| panic!("{at}: preparation failed: {e:?}"));
            assert!(
                prepared.width > 0 && prepared.height > 0,
                "{at}: empty image"
            );
            assert_eq!(
                prepared.checksum,
                Checksum::of_bytes(&prepared.png),
                "{at}: checksum"
            );
            let types = png_chunk_types(&prepared.png);
            assert!(
                types
                    .iter()
                    .all(|t| matches!(t.as_str(), "IHDR" | "IDAT" | "IEND")),
                "{at}: the stored PNG keeps chunks {types:?}"
            );
            image::load_from_memory(&prepared.png)
                .unwrap_or_else(|e| panic!("{at}: the stored PNG does not decode: {e}"));
            prepared_total += 1;
        }
    }
    eprintln!(
        "sample exams: {listed_total} listed images, {kept_total} kept, {prepared_total} prepared"
    );
    assert!(prepared_total > 0, "no real image was prepared");
}

/// The classifier's `classify` over a temporary copy of each real exam, **never** with
/// sending (spec/CLASSIFIER.md §14.7, R-P4): no `--enviar`, and an environment that
/// answers nothing, so no key, model or authorization can be read. Only properties
/// are asserted, and a document is named by its index.
#[tokio::test]
#[ignore = "local only: requires RUN_SAMPLE_EXAMS=1 and the owner's tests/sample-exams"]
async fn real_exams_are_classified_locally_without_sending_and_without_text() {
    use embeddings_fundamentals::classifier::{EstadoClassificacao, LIMITE_TOTAL};
    use embeddings_fundamentals::classify::{ClassifyArgs, FontePoppler, executar, texto_de_saida};

    let files = exams();
    let fonte = FontePoppler::detect().await;
    let nada = |_: &str| None;
    let mut por_estado = std::collections::BTreeMap::<String, usize>::new();
    for (i, path) in files.iter().enumerate() {
        let dir = tempfile::tempdir().expect("tmp");
        let copia = dir.path().join(path.file_name().expect("name"));
        std::fs::copy(path, &copia).expect("copy");
        let args = ClassifyArgs {
            arquivo: copia.clone(),
            paginas: None,
            json: true,
            enviar: false,
            limiar: None,
            titulo: None,
            nomes_paciente: Vec::new(),
            mostrar_estado: false,
        };
        let linha = executar(&args, &fonte, &nada).await;
        assert!(
            matches!(
                linha.estado,
                EstadoClassificacao::Preparado
                    | EstadoClassificacao::SemConteudo
                    | EstadoClassificacao::BloqueadoPrivacidade
            ),
            "exam #{i}: unexpected state {:?} (motivo {:?})",
            linha.estado,
            linha.motivo_codigo
        );
        assert!(!linha.envio_realizado, "exam #{i}");
        assert!(linha.model.is_none(), "exam #{i}");
        assert!(linha.paginas_enviadas.unwrap_or(0) <= 2, "exam #{i}");
        assert!(
            linha.caracteres_enviados.unwrap_or(0) <= LIMITE_TOTAL,
            "exam #{i}"
        );
        assert!(linha.estado_preparado.is_none(), "exam #{i}");
        let stem = copia
            .file_stem()
            .expect("stem")
            .to_string_lossy()
            .into_owned();
        for json in [true, false] {
            let saida = texto_de_saida(&linha, json).expect("serialize");
            assert!(
                !saida.contains(&stem),
                "exam #{i}: the output names the file"
            );
        }
        *por_estado.entry(format!("{:?}", linha.estado)).or_default() += 1;
    }
    eprintln!(
        "classified {} exams locally, without sending: {por_estado:?}",
        files.len()
    );
}
