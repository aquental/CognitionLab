# PLAN — Medical Document Embedding Lab

**Source:** `spec/PRD.md` (Draft, revised after adversarial review — see §9)  
**Target:** Rust 2024 edition, single crate (`lib` + `bin`), rustc 1.99 (`rust-version = "1.99"`, pinned by `rust-toolchain.toml`)  
**Coding guidelines:** the project-local `rust-skills` skill (`/rust-skills`); §2.3 records how its rules apply here  
**Status:** Ready to implement — §0.1 lists the defaults this plan fixes where the PRD leaves a choice open.

---

## 0. PRD readiness assessment

The PRD specifies scope, architecture, record schema, metric formulas, Gherkin acceptance scenarios with milestone tags, the reliability contract, the BDD runner contract, deliverables and milestones. **Implementation can start now.** The items below are defaults this plan fixes (§0.1) or inputs only the owner can supply (§0.2).

### 0.1 Defaults fixed by this plan (override in `config/*.toml`)

| Topic | Default | Notes |
|---|---|---|
| Text model (local) | **Engine comparison (same model):** `sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2` (384-d, `BertModel`, mean pooling, cosine, L2-normalized by the adapter, 128-token limit from `sentence_bert_config.json`, revision `e8f8c211226b894fcb81acc59f3b34ba3efd5f42`) via **EmbedAnything (candle, safetensors)** in Phase 1 and **FastEmbed (ONNX, `Xenova/` export)** in Phase 2. **Model comparison (same engine):** `intfloat/multilingual-e5-base` (768-d, XLM-RoBERTa) via FastEmbed in Phase 2 | *Changed in Phase 1 after a source spike.* EmbedAnything 0.7.1's candle path accepts only `BertModel`/`JinaBert`/`ModernBert`/`Qwen3`/`Gemma3`, so it cannot load the XLM-RoBERTa E5 model. Its ONNX path (feature `ort`) pins `ort =2.0.0-rc.10`, while FastEmbed 7.5 pins `ort =2.0.0-rc.15`. Both link the native `onnxruntime` library, so Cargo cannot build them together (`--all-features` would fail). The multilingual MiniLM runs on both engines, so stage 2 still holds the model fixed while the engine and weight format change. Results stay labelled **end-to-end configuration comparisons**. |
| E5 prefixes | `query: ` / `passage: ` added **once, inside the adapter**, driven by a per-model `PrefixPolicy` | The pipeline passes raw text to the adapter (US-04 "unchanged chunks and query texts"). fastembed-rs `TextEmbedding::embed` adds **no** prefix itself (verified in fastembed-rs docs), so the FastEmbed adapter adds it for E5. MiniLM, CLIP/SigLIP, and OpenAI use `PrefixPolicy::None`. |
| Model revision | Every real-model experiment TOML pins `model_revision` (HF commit SHA, or the OpenAI model snapshot name such as `text-embedding-3-small` plus the response `model` field). Validation refuses a real adapter with no pin (PRD §6.4) | The same value feeds `ModelInfo` and `embedding_space_id`. |
| Engine ids | Canonical `embed_anything`, `fastembed`, `openai`; a `EngineId::from_display("EmbedAnything")` mapping serves the Gherkin names | BDD asserts the canonical id in records and reports. |
| Text model (remote) | OpenAI `text-embedding-3-small` (1536-d) | Price TOML: `{ currency = "USD", unit = "token", unit_quantity = 1_000_000, unit_price = <published> }`; cost = usage / unit_quantity × unit_price. A unit test with 1,000 tokens catches an error of 10^6. Each adapter declares `normalized`. OpenAI and E5 vectors are explicitly L2-normalized by the adapter (an idempotent operation), so the flag is always true by construction. |
| Text→image model | Baseline `openai/clip-vit-base-patch32` (512-d) via EmbedAnything | EmbedAnything documents CLIP and SigLIP vision support. Portuguese candidate: `google/siglip-base-patch16-256-multilingual`, **only if** a Phase 3 load spike proves EmbedAnything loads it. Otherwise English CLIP is the only image model, and weak Portuguese alignment is reported as a finding. M-CLIP is dropped because it is not a CLIP checkpoint that the engine can load. |
| Token limits | `Capabilities.max_input_tokens`, counted with the model's own `tokenizers` tokenizer (`tokenizer.json` from the pinned revision in the Hugging Face cache, which the engine fills when it loads the model): MiniLM 128, E5 512, CLIP 77, OpenAI 8191 (`cl100k_base` via `tiktoken-rs`) | Truncation records token counts, not character counts. The pipeline enforces the limit **before** the engine sees the text, so the engine's own silent tokenizer truncation never triggers. |
| Native vs OCR per page | Use native text when `pdftotext` yields ≥ 200 non-whitespace characters **and** ≥ 50 % of them are letters or digits (Unicode alphanumeric, so Portuguese diacritics count). Otherwise render and OCR | Catches empty and symbol-garbage text layers on scanned PDFs. *Phase 1 change:* the earlier token-share rule (≥ 60 % word-like tokens) misclassified pipe-separated lab tables (57 %) as garbage. The character share of such tables is 74 %, and 84–95 % on the fixture pages. Both thresholds are part of the extraction fingerprint. |
| OCR | `pdftoppm -png -singlefile -r 300 -f N -l N <pdf> <tmp>/page` (writes exactly `<tmp>/page.png`) → `tesseract <tmp>/page.png - -l por+eng --psm 6 tsv` behind an `OcrEngine` trait | Without `-png`, `pdftoppm` writes PPM, and without `-singlefile` it appends `-<page>` to the name. The parser skips the TSV header row and uses only `level = 5` (word) rows. Word boxes are converted from rendered pixels to PDF points (× 72/dpi), keeping a top-left origin. `bounding_box` stores `{x, y, w, h, unit: "pt", origin: "top-left"}`. `extraction_confidence` is the mean `conf` of words with `conf ≥ 0`, scaled to 0–1, or null when no word has `conf ≥ 0` (Tesseract uses `-1` for non-word rows). |
| Native PDF text | `pdftotext -layout -f N -l N` (poppler is installed). **Page count and order come only from `pdfinfo`** (`Pages:`); a per-page extraction that fails, or a page count that disagrees with `lopdf`, fails the source at stage `extraction`. `lopdf` is used only to read metadata for the anonymization check | `-layout` keeps each lab-table row on one line. |
| Embedded images | `pdfimages -list` first. Only rows with `type = image` are kept; `smask`, `stencil`, and images smaller than 64×64 px are dropped. A full-page raster on a page without native text is that page's OCR source, never an image record. Then `pdfimages -png -p -f N -l N` and match files by object number | A fixture PDF with a soft mask tests the filter. |
| Run modality | A run embeds one modality. A text run processes text-bearing inputs (PDFs). Image-only inputs (PNG/JPEG) are reported as `not_applicable` for a text run: they are neither failures nor successes, and they do not make the run incomplete. Embedded PDF images are ignored by text runs | Keeps text and image spaces separate (PRD §6.5) |
| Deterministic BDD extraction | BDD never shells out. `FixtureExtractor` replays `tests/fixtures/extraction/<file-stem>/p<N>.native.txt` and `p<N>.ocr.tsv`, captured once from the real tools by `cargo run --example capture_extraction`. Steps can also register pages programmatically. The TSV goes through the same parser as live OCR | PRD §5.7: deterministic checks are independent of external tools. Integration tests run the real tools |
| Modalities per input | Searchable/scanned PDF → text records only (a page rendered for OCR is not an image record). Mixed PDF → text records + one image record per embedded raster image ≥ 64×64 px. PNG/JPEG → one image record | Each fixture's `expected.toml` lists `(page, modality, count)`. |
| Vector store | Filesystem, one directory per `embedding_space_id`, JSONL per revision, exact brute-force cosine | Exact search makes BDD rankings deterministic. |
| Identifiers | See §2.1 | Deterministic; revision-scoped; modality-aware. |
| Error classes | Transient: HTTP 408/429/5xx, timeouts, connection reset, model-load I/O. Permanent: other 4xx, capability, validation, invalid vectors, input-limit | `EmbeddingError::class()` |
| Retry | `max_attempts = 3` (initial attempt included), backoff 200 ms × 2, injectable `Sleeper` | |
| Ranking ties | Single-modality search sorts by descending cosine score, then ascending `record_id` | Deterministic frozen rankings (PRD §6.6). |
| Fusion | RRF `k = 60`, `fusion_depth = all`. A combined experiment = `(text_space_id, image_space_id, k, depth)` | PRD §6.6 |
| Latency | `latency_repeats = 20` after 1 discarded warm-up query; nearest-rank median/p95 | PRD §9.4 |
| Peak RAM | `getrusage(RUSAGE_SELF).ru_maxrss` through the safe `nix::sys::resource::getrusage` wrapper, normalized to bytes (macOS reports bytes, Linux reports KiB) with `u64::try_from` | `sysinfo` reports current RSS, not the peak, so it is not used. Using the wrapper keeps the crate free of `unsafe` (§2.3). |
| Currency | USD; `estimate_status = "estimated" \| "unavailable"` | |

### 0.2 Inputs only the owner can provide

1. **Anonymized corpus** (10–20 files) in `corpus/` + `corpus/manifest.toml` with anonymization-check fields (§4.4).
2. **Relevance labels** (0/1/2) for the 10 frozen queries.
3. **Extraction-fidelity sample**: about 20 lab rows from the corpus with expected name, value, unit, and reference range.
4. `remote_allowed` per corpus + `OPENAI_API_KEY` in `.env` (git-ignored).
5. `brew install tesseract tesseract-lang`.

---

## 1. Repository layout

```
embeddings-fundamentals/
├── Cargo.toml                      # lib + bin; explicit [[test]] targets (§5)
├── .gitignore                      # /target, /store, /corpus/* (except manifest), .env, /reports/tmp
├── src/
│   ├── lib.rs
│   ├── main.rs                     # parse CLI → lib::cli::run → std::process::ExitCode
│   ├── cli/                        # ingest, search, evaluate, report, validate, manifest, anonymization-check
│   ├── config/                     # TOML schemas, fingerprints, price tables, permission flags
│   ├── corpus/                     # manifest, checksum, eligibility, anonymization gate
│   ├── extraction/                 # TextExtractor, OcrEngine, ImageExtractor, MetadataScrubber
│   ├── chunking/                   # ChunkingStrategy; structure-aware-v1; TokenCounter
│   ├── embedding/                  # trait, Capabilities, ModelInfo, PrefixPolicy, validation, adapters
│   │   ├── deterministic.rs
│   │   ├── embed_anything.rs       # feature "engine-embed-anything"
│   │   ├── fastembed.rs            # feature "engine-fastembed"
│   │   └── openai.rs               # feature "engine-openai"
│   ├── storage/                    # RecordStore; FsRecordStore (atomic publish, §4.1)
│   ├── retrieval/                  # text / image / combined; space guard; revision-pinned search
│   ├── fusion/                     # RRF
│   ├── evaluation/                 # queries, labels, metrics
│   ├── measurement/                # Clock, MemoryProbe, GpuProbe, UsageSource, CostEstimator
│   ├── pipeline/                   # orchestration, retry, run status, experiment snapshots
│   ├── report/                     # JSON + Markdown comparison report
│   └── error.rs
├── tests/
│   ├── bdd.rs                      # cucumber runner + inventory check + JSON report (§5.2)
│   ├── bdd/                        # steps modules + World (mod-included by bdd.rs)
│   ├── features/                   # .feature files, grown per milestone from PRD §5
│   ├── integration.rs              # real adapters / poppler / tesseract; mod-includes tests/integration/*
│   ├── integration/
│   └── fixtures/
├── config/{experiments,prices,queries,fidelity}/
├── corpus/                         # owner-supplied (content ignored, manifest tracked)
├── store/                          # runtime (ignored)
└── reports/{bdd,integration,experiments,examples}/
```

### Dependencies

| Crate | Version | Purpose |
|---|---|---|
| `tokio` (present) | 1.54, add features `process`, `fs`, `sync`, `time` | runtime, async subprocesses (poppler, tesseract), `spawn_blocking`, `Semaphore`, retry sleeps |
| `clap` (derive) | 4.6 | CLI |
| `thiserror` | 2 | typed domain errors in the library (`anyhow`, already present, only in `main`/`cli`) |
| `toml` | 1.1 | config |
| `sha2`, `hex` | 0.11 / 0.4 | checksums, fingerprints |
| `lopdf` | 0.46 | PDF page count, info dictionary / XMP for anonymization check |
| `image` | 0.25 | decode PNG/JPEG; re-encode without metadata |
| `kamadak-exif` | 0.6 | detect EXIF before stripping |
| `tokenizers` | latest stable 0.x (1.0 is still a release candidate), the same major version that `embed_anything`/`fastembed` resolve to, so the crate is not built twice | token counting for HF models |
| `tiktoken-rs` | 0.12 | token counting for OpenAI |
| `nix` | 0.31, feature `resource` | safe `getrusage` wrapper (no `unsafe` in the crate) |
| `async-trait` | 0.1 | **only** for the `dyn EmbeddingStrategy` trait (§2.2) |
| `unicode-normalization` | 0.1 | NFC for canonical hashing |
| `tracing`, `tracing-subscriber` (`env-filter`) | 0.1 / 0.3 | ID-only logs; library code emits through `tracing`, and only `main` installs the subscriber |
| `embed_anything` | 0.7 (optional) | engine 1 |
| `fastembed` | 7.5 (optional) | engine 2 |
| `reqwest` (present) | 0.13 | OpenAI (feature `engine-openai`) |
| dev: `cucumber` | 0.23, features `["output-json"]` | BDD + JSON report |
| dev: `tempfile`, `approx`, `proptest` | 3 / 0.5 / 1 | isolation, tolerance, RRF properties |
| dev: `gherkin` | (version re-exported by cucumber) | parse PRD Gherkin to build inventory |

Exact versions of `kamadak-exif`, `tokenizers`, `tiktoken-rs`, `nix` are checked with `cargo search` in Phase 0 before pinning. `mockall` and `insta` are deliberately **not** added: the hand-written deterministic adapter and fault-injecting wrappers cover mocking, and golden files under `tests/fixtures/` cover snapshots.

---

## 2. Core contracts

### 2.1 Identifiers

| ID | Definition |
|---|---|
| *Encoding* | All hashes are SHA-256 over **canonical JSON** (`serde_json` with sorted keys via `BTreeMap`, no whitespace, UTF-8 NFC strings) of a typed struct, never `Debug` output or delimiter-free concatenation. Golden-value unit tests freeze each ID. |
| `extraction_fingerprint` | sha256(canonical JSON of extractor tools+versions, OCR lang/psm/dpi, native-vs-OCR thresholds) |
| `chunking_fingerprint` | sha256(strategy id + params + tokenizer id + truncation policy) |
| `revision_id` | `rev-` + sha256(source checksum ‖ extraction_fingerprint ‖ chunking_fingerprint)[..16] |
| `chunk_id` | `{source_id}-p{page}-{modality}-{index}` (e.g. `src-001-p2-text-3`, `src-001-p2-image-0`); unique within a revision |
| `embedding_space_id` | `space-` + sha256(canonical JSON of `SpaceDescriptor`: engine, engine_version, model_id, model_revision, weight_format, dims, normalized, metric, prefix policy, preprocessing)[..32]. 128 bits is ample for the handful of spaces in this lab, and the weight format is included because the same model in safetensors and ONNX form is not guaranteed to produce identical vectors |
| `record_id` | sha256(canonical JSON `{revision_id, chunk_id, embedding_space_id}`)[..16] |

Equal dimensions never imply equal spaces (US-04).

Each ID is a newtype (`SourceId`, `ChunkId`, `RevisionId`, `RecordId`, `EmbeddingSpaceId`, `ExperimentId`, `Checksum`) with a private field. IDs are created only by the hashing constructors or by `FromStr`/`TryFrom<String>` parsing that validates the format, and serde goes through `#[serde(try_from = "String", into = "String")]`. They derive `Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord`, and `Ord` is the ascending tie-break used by ranking and fusion.

### 2.2 Traits and dispatch

**Dispatch rule.** `#[async_trait]` is used on exactly **one** trait: `EmbeddingStrategy`. It needs `dyn` because the adapter is chosen from configuration at runtime, and combined mode holds two different adapters side by side. Every other async trait uses **native `async fn` in traits** with static dispatch (generics). Where the future crosses `tokio::spawn` or `spawn_blocking` boundaries, the trait declares `fn op(&self, ..) -> impl Future<Output = ..> + Send`, and implementations may still write `async fn`. Synchronous work stays in synchronous traits.

| Trait | Kind | Dispatch | Why |
|---|---|---|---|
| `EmbeddingStrategy` | async | `Arc<dyn …>` via `#[async_trait]` | runtime selection by config; heterogeneous adapters in one run |
| `TextExtractor`, `OcrEngine`, `ImageExtractor` | async (`tokio::process`) | generic, `+ Send` futures | one concrete type per binary (real tools in the CLI, fixture-backed in BDD) |
| `Sleeper` | async | generic | `TokioSleeper` in the CLI, `NoopSleeper` in tests |
| `RecordStore` | **sync** (`std::fs` + `File::sync_all` on files and directories) | generic; the pipeline calls it inside `spawn_blocking` | directory `fsync` and `rename` are blocking syscalls; `tokio::fs` exposes no directory fsync |
| `ChunkingStrategy`, `TokenCounter`, `MetadataScrubber` | sync, CPU-bound | generic; large batches run in `spawn_blocking` | pure computation |
| `Clock`, `MemoryProbe`, `GpuProbe`, `UsageSource` | sync | generic | cheap reads, replaced by fixtures in US-05 |

The pipeline is generic over the three components that tests replace: `Pipeline<'a, T: TextExtractor, O: OcrEngine, S: Sleeper>`. It holds the concrete `FsRecordStore` (fault injection is built into the store) and `Arc<dyn EmbeddingStrategy>`. *Phase 1 change:* the planned `Components` bundle with eight associated types was dropped as premature abstraction (`anti-over-abstraction`). Image, clock, and memory components are added as generic parameters when Phases 2–3 need them.

```rust
// The single dyn-compatible async trait. `async fn` in traits is not dyn-compatible,
// so this trait (and only this one) uses #[async_trait] (boxed futures).
// `adapters::build(&EngineConfig) -> Result<Arc<dyn EmbeddingStrategy>, BuildError>` is the only
// factory and the only place behind `#[cfg(feature = ...)]`. An engine compiled without its feature
// returns `BuildError::EngineNotCompiled { engine }` (features stay strictly additive).
#[async_trait::async_trait]
pub trait EmbeddingStrategy: Send + Sync {
    fn capabilities(&self) -> &Capabilities;
    fn model_info(&self) -> &ModelInfo;
    fn token_counter(&self) -> &dyn TokenCounter;
    fn input_budget(&self, kind: InputKind) -> Option<usize>; // limit minus prefix + special tokens
    async fn embed_documents(&self, raw: &[TextInput]) -> Result<EmbeddingBatch, EmbeddingError>; // adapter applies PrefixPolicy
    async fn embed_queries(&self, raw: &[TextInput]) -> Result<EmbeddingBatch, EmbeddingError>;
    async fn embed_images(&self, assets: &[AssetRef]) -> Result<EmbeddingBatch, EmbeddingError>;
}

// Native async fn in traits, static dispatch, explicit Send futures.
pub trait OcrEngine: Send + Sync {
    fn ocr_page(&self, page_png: &Path) -> impl Future<Output = Result<PageText, ExtractionError>> + Send;
}
pub trait TextExtractor: Send + Sync {
    fn page_count(&self, pdf: &Path) -> impl Future<Output = Result<NonZeroU32, ExtractionError>> + Send;
    fn extract_page(&self, pdf: &Path, page: NonZeroU32) -> impl Future<Output = Result<PageText, ExtractionError>> + Send;
}
pub trait ImageExtractor: Send + Sync {
    fn extract_images(&self, pdf: &Path, page: NonZeroU32) -> impl Future<Output = Result<Vec<AssetRef>, ExtractionError>> + Send;
}
pub trait Sleeper: Send + Sync {
    fn sleep(&self, d: Duration) -> impl Future<Output = ()> + Send;
}

// Synchronous traits.
pub trait RecordStore: Send + Sync + 'static {
    fn stage(&self, space: &EmbeddingSpaceId, source: &SourceId, rev: &RevisionId, records: &[Record]) -> Result<StagedRevision, StoreError>;
    fn publish(&self, staged: StagedRevision) -> Result<(), StoreError>;   // §4.1; consumes the staged handle
    fn active(&self, space: &EmbeddingSpaceId) -> Result<Vec<RevisionRef>, StoreError>;
    fn records(&self, space: &EmbeddingSpaceId, revs: &[RevisionRef]) -> Result<Vec<Record>, StoreError>; // revision-pinned reads
}
pub trait ChunkingStrategy: Send + Sync {
    fn id(&self) -> &str;
    fn chunk(&self, page: &PageText, counter: &dyn TokenCounter, policy: &TruncationPolicy) -> Result<Vec<Chunk>, ChunkError>;
}
pub trait TokenCounter: Send + Sync { fn count(&self, text: &str) -> usize; }
pub trait Clock: Send + Sync { fn now(&self) -> Instant; }
pub trait MemoryProbe: Send + Sync { fn peak_rss_bytes(&self) -> Option<u64>; }

pub fn validate_batch(batch: &EmbeddingBatch, info: &ModelInfo) -> Result<(), VectorDefect>; // WrongDims | NaN | Inf, with item id
```

`Capabilities` and `ModelInfo` are returned by reference: they are computed once when the adapter is built and never change. `StagedRevision` is consumed by `publish`, so the type system prevents publishing the same staged revision twice.

### 2.3 Rust engineering conventions (from `rust-skills`)

These rules apply to every phase. Reviews (execute prompt §7) check them.

| Area | Convention | Rules |
|---|---|---|
| Errors | One `thiserror` enum per module boundary (`ExtractionError`, `ChunkError`, `EmbeddingError`, `StoreError`, `ValidationError`, `EvalError`), each with `#[source]` chains and lowercase messages without trailing punctuation. `EmbeddingError::class() -> ErrorClass`. `anyhow` only in `main`/`cli`, with `.context(..)`. Messages carry IDs and stages, **never** chunk text or credentials. Fallible public fns document `# Errors`. | `err-thiserror-lib`, `err-anyhow-app`, `err-source-chain`, `err-lowercase-msg`, `err-doc-errors`, `obs-no-sensitive-data` |
| No panics in production | `unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!` are denied in non-test code by lints (§5.4). `#[test]` functions and `#[cfg(test)]` modules may use `expect("<what failed>")` with a useful message (`clippy.toml`). Harness-less targets (`bdd`, `bdd_selftest`) and step modules are **not** treated as test code by clippy, so they use `?` with `anyhow` context, `Result`-returning steps, and `assert!` with messages. | `err-no-unwrap-prod`, `anti-unwrap-abuse`, `anti-panic-expected` |
| Unsafe | `unsafe_code = "deny"` crate-wide. `getrusage` goes through `nix`. If a future need is unavoidable, it goes in one module with `#[expect(unsafe_code, reason = "...")]` and `// SAFETY:` comments. | `unsafe-safety-comment`, `unsafe-minimize-scope` |
| Types | ID newtypes (§2.1). Enums instead of strings for `Modality`, `ExtractionMethod`, `EngineId`, `ErrorClass`, `SourceStatus`, `RunStatus`, `FailureStage`, `EstimateStatus`, `PrefixPolicy`. Page numbers are `NonZeroU32`. Config and manifest structs use `#[serde(deny_unknown_fields)]` and `rename_all = "snake_case"`, so a typo in a TOML key is an error, not a silent default. | `type-newtype-ids`, `type-no-stringly`, `type-enum-states`, `num-nonzero`, `serde-deny-unknown-fields`, `api-parse-dont-validate` |
| Floats & numbers | Vectors are `Vec<f32>`, validated finite before storage. Ranking sorts with `f32::total_cmp` (descending) then `RecordId` (ascending), never `partial_cmp().unwrap()`. Metric tests compare with `approx` (1e-6). Narrowing casts use `TryFrom`; `as` is allowed only after an explicit range check, with a comment. | `num-float-compare`, `num-cast-try-from` |
| Async | Tokio multi-thread runtime in `main`. Subprocesses via `tokio::process::Command` with `kill_on_drop(true)` and a timeout. CPU-bound inference (candle, ONNX), image re-encoding, and store I/O run in `spawn_blocking`. FastEmbed's `&mut self` model sits behind a `std::sync::Mutex` that is locked **only inside** the blocking closure, never across `.await`. Source concurrency is bounded by a `Semaphore` (`--jobs`, default 1 so timing measurements stay reproducible). | `async-spawn-blocking`, `async-no-lock-await`, `async-fn-in-trait`, `trait-object-safety` |
| Ownership | Adapters borrow inputs (`&[TextInput]`). Records move into `stage`. `Arc` is used only for the shared adapter and store handles. Prefer `&str`/`&[T]`/`&Path` parameters, and accept the occasional `clone` of small IDs instead of introducing lifetimes. | `own-borrow-over-clone`, `own-slice-over-vec`, `own-arc-shared`, `anti-clone-excessive` |
| Observability | The library emits `tracing` events with structured fields (`source_id`, `stage`, `attempt`, `tokens`). `#[instrument(skip_all, fields(..))]` is used so arguments holding text are never recorded. Only `main` installs `tracing_subscriber` with `EnvFilter`. | `obs-library-facade`, `obs-structured-fields`, `obs-instrument-spans` |
| Features | Engine features only **add** adapters and their dependencies (`dep:` syntax). There are no mutually exclusive features. `unexpected_cfgs` stays on. | `proj-feature-additive`, `lint-cfg-check` |
| Structure | `main.rs` only parses arguments, installs tracing, and maps `Result` to `ExitCode`. Logic lives in `lib.rs` modules, organized by feature (§1). Internal items are `pub(crate)`. `#[non_exhaustive]` is **not** used, because the crate is not published and exhaustive matches catch new variants. | `proj-lib-main-split`, `proj-mod-by-feature`, `proj-pub-crate-internal`, `pat-exhaustive-enum` |
| Tests | Unit tests in `#[cfg(test)] mod tests` beside the code, with descriptive names in arrange/act/assert shape. `#[tokio::test]` for async. `proptest` for RRF, metrics, ID stability, and truncation budgets. | `test-cfg-test-module`, `test-descriptive-names`, `test-proptest-properties`, `test-tokio-async` |
| Suppressions | No `#[allow]` to silence a finding. Where a lint is genuinely wrong for one item, use `#[expect(lint, reason = "...")]` so the suppression fails once it becomes stale. | `lint-*` |

The pipeline depends only on these traits. Adapters never see chunking, extraction, or evaluation configuration.

---

## 3. Phases

**Gating rule:** phase *N* is complete when `cargo test --test bdd` executes every scenario and example row tagged `@m1`…`@mN` in PRD §5 (the inventory is derived from the PRD, §5.2), all of them pass, and none is skipped. Phase *N* adds to `tests/features/` only the scenarios and `Examples` blocks tagged `@mN`, copied from PRD §5, so the suite never contains unimplemented scenarios. `config/acceptance.toml` holds `milestone = N`.

### Phase 0 — Scaffolding & BDD harness

1. Convert to `lib` + `bin` (`main.rs` stays thin, §2.3). Add dependencies, additive Cargo features `engine-embed-anything`, `engine-fastembed`, `engine-openai` (default: none), `rust-version`, `rust-toolchain.toml`, the `[lints]` tables and `clippy.toml` from §5.4, and the `.gitignore` from §1.
2. `tests/bdd.rs` (§5.2): `World` with `TempDir` store, call logs, fault flags; tee console + JSON writer; `.run(...).await`; inventory check; explicit exit.
3. **Runner self-test** `tests/bdd_selftest.rs` (separate `[[test]]`): runs the same runner (`tests/support/runner.rs`) against per-case PRD and feature files that it writes into temporary directories. The cases cover an undefined step, an ambiguous step, a failing step, an empty required suite, a filtered run, a weakened step, a missing `Examples` row, the milestone-0 bootstrap, and a complete suite. It asserts the expected pass or fail for each, and also checks the real PRD inventory counts per milestone (0/18/28/33/35). This proves `fail_on_skipped` and empty-suite detection without making the real suite red.
4. `DeterministicEmbedder`: SHA-256-seeded normalized vectors. Configurable dims, `embedding_space_id`, capabilities, prefix policy, and execution (Local/Remote). Fault injection: `fail_n_times(class)`, `defect(kind, item)`. Records every raw input and which method was called.
5. Synthetic fixtures, generated by `examples/gen_fixtures.rs` (`cargo run --example gen_fixtures`) and checked in under `tests/fixtures/corpus/` with `manifest.toml` checksums and `expected.toml`. PDFs are built with `lopdf`, and scanned pages are produced by rendering a generated text page with `pdftoppm` **at generation time only**, so tests need no PDF tool. Generation is deterministic (byte-identical output).
6. CI (§6).

**Exit:** `milestone = 0`, the inventory is empty, and the self-test passes. CI is green.

### Phase 1 — Milestone 1: Text baseline (`@m1`)

Scenarios: US-01 (text PDFs, lab row, mixed-page extraction, idempotency), US-02 (text, empty index), REL-01 (all, including the new truncation-disabled scenario and remote-permission scenario with a deterministic adapter whose `execution = Remote`).

1. `corpus::Manifest` + anonymization gate (§4.4) + checksum verification.
2. Extraction: `PopplerTextExtractor`, `TesseractOcr`, per-page selection rule (§0.1), empty OCR → `unreadable`. Fixture-backed implementations for BDD, plus an integration test that runs real poppler on the generated two-page mixed PDF and asserts `native`/`ocr` per page.
3. `StructureAwareV1` chunker: sections, paragraphs, table rows with header attached. Row detection accepts comma decimals (`13,5`), en-dash/hyphen/"a" ranges (`12,0–16,0` with U+2013, `12,0-16,0`, `12,0 a 16,0`), and both `|`-separated and whitespace-aligned columns. Unit tests use the exact PRD fixture string and real `pdftotext -layout` output from the generated fixture PDF. Splits oversized units when possible. Token-based truncation when enabled (`TruncationRecord { original_tokens, submitted_tokens }`). Counts include the adapter's prefix and special tokens: the budget is `max_input_tokens − prefix_tokens − special_tokens`, obtained from `EmbeddingStrategy::input_budget()`. Fails at stage `chunking` when truncation is disabled and an indivisible unit exceeds the limit.
4. `FsRecordStore` (§4.1) with revision-pinned reads. Append-only experiment snapshots (§4.2).
5. `pipeline::ingest`: validation first (capabilities, space, remote permission, eligibility). Then the per-source state machine `Extracted → Chunked → Embedded → Validated → Staged → Published | Failed{stage, reason, attempts}` with class-based retry. Independent sources continue. Run status `Succeeded | Incomplete`.
6. `EmbedAnythingAdapter` (multilingual MiniLM via candle, `PrefixPolicy::None`, pinned revision; the token counter loads `tokenizer.json` from the HF cache). The CPU-bound `embed` runs in `spawn_blocking`.
7. `retrieval::text_search` with the space guard (mismatch error **before** similarity), top-k, and `--experiment <id>` to search the revisions a saved experiment used.
8. CLI exit codes: 0 succeeded, 2 incomplete, 3 validation refused, 1 internal error.

**Exit:** all `@m1` pass. A real run over the owner corpus (when supplied) produces `reports/examples/record-001.json` (Deliverable 4).

### Phase 2 — Milestone 2: Engine comparison (`@m2`)

Scenarios: US-03 (text-only example, low relevance), US-04 (all), US-05 (all).

1. `FastEmbedAdapter` (E5 ONNX, adds its own prefix) and `OpenAiAdapter` (raw text, usage tokens, HTTP-status error class). Each adapter has a unit test asserting the exact string sent to the backend (prefix applied once).
2. Spaces: `store/<embedding_space_id>/`.
3. `evaluation`: frozen queries + labels (checksums in report), metrics exactly per PRD §5.3, expected values precomputed by hand and checked in under `tests/fixtures/metrics/`.
4. `measurement`: cold/warm per PRD §9.4, `ru_maxrss` peak, GPU memory number or `"unavailable"`, index size on disk, ingestion duration, time/page, chunks/s, batch latency, model-load time.
5. `CostEstimator`: `usage / unit_quantity × unit_price`. Null with a reason when usage or price is missing. Remote inference memory `"unavailable"`.
6. Extraction-fidelity evaluator over `config/fidelity/sample-v1.toml`: field-level accuracy per extraction configuration.
7. `report`: JSON + Markdown with hardware, corpus checksum, batch size, engine, model, weight format, all fingerprints, fidelity table, and the "end-to-end configuration comparison" wording.
8. Integration tests in `tests/integration.rs` (§5.3).

**Exit:** all `@m1`+`@m2` pass. Three text configurations evaluated. The integration report exists (or remote is marked outstanding).

### Phase 3 — Milestone 3: Image retrieval (`@m3`)

Scenarios: US-01 (mixed PDF, PNG, JPEG), US-02 (image), US-03 (image-only example).

1. **Spike first:** confirm EmbedAnything 0.7 loads CLIP ViT-B/32 and the multilingual SigLIP candidate through the Rust API. Record the result in `reports/spikes/phase3-models.md`.
2. `ImageExtractor` (`pdfimages -png -p`) + PNG/JPEG ingestion. Assets are re-encoded without EXIF/XMP (`MetadataScrubber`) into `store/assets/<sha256>.png`, and `asset_uri` points there with the original path in metadata. Image records keep `chunk_text = null`. Generated captions, if any, go only to `metadata.generated_caption = { text, generated: true, model }` and are never embedded or used as query text.
3. `embed_images()` + text-query path for the cross-modal model. `text_to_image_compatible = true` only for cross-modal models.
4. `retrieval::image_search` via `embed_queries`, asserted from the adapter call log.

**Exit:** all `@m1`–`@m3` pass. Image-only report for ≥ 1 model.

### Phase 4 — Milestone 4: Combined retrieval & publication (`@m4`)

Scenarios: US-03 (combined example, RRF).

1. `fusion::rrf(k, depth)`: score = Σ 1/(k + rank) with rank from 1. Sort by descending score, ties by ascending `record_id`. Proptest: overlapping items accumulate, and scaling raw scores leaves the result unchanged.
2. `retrieval::combined`.
3. Final comparison report, `reports/README.md` (reproduction), saved experiment configs, limitations. Walk through the PRD §12 deliverables and PRD §13 acceptance criteria with evidence links.

---

## 4. Reliability & data-handling design

### 4.1 Atomic publish (POSIX, macOS + Linux)

```
store/<space>/
  revisions/<source_id>/<revision_id>/records.jsonl   # immutable once published
  tmp/<uuid>/                                         # per-attempt staging, never read by search
  active/<source_id>.json                             # {"revision_id": ...}
```

1. Write `tmp/<uuid>/records.jsonl` → `fsync(file)` → `fsync(tmp/<uuid>)`.
2. Every revision carries a seal, `revision.json`, holding the checksum and record count of its `records.jsonl`. The seal is written and fsync'd in staging before the rename. If `revisions/<source>/<revision_id>/` already exists and its records match the seal (checksum, count, every line parses), it is **reused as-is**. A truncated file, even one that still parses, fails the seal. The new staging is discarded, the report says `reused_revision` with 0 records written, and the existing records keep the experiment id of the run that produced them. The directory only appears through an atomic rename of fsync'd staging, so it is complete. It may also be pinned by an older snapshot (a source reverted to an earlier revision), so it is never replaced. A damaged directory (missing or corrupt `records.jsonl`) is moved to `tmp/<stage>.stale/` and the staging is installed in its place, so a retry heals it. *Phase 1 change from the checksum-match rule:* records embed the producing `experiment_id`, so a re-run never has an identical checksum.
3. `rename(tmp/<uuid>, revisions/<source>/<revision_id>)` → `fsync(revisions/<source>)`.
4. Write `active/<source>.json.tmp-<uuid>` → `fsync` → `rename` to `active/<source>.json` → `fsync(active/)`. **This rename is the commit point.**
5. Search reads only revisions named by `active/` or by an experiment snapshot. A revision directory without a pointer is never visible.
6. `tmp/` is garbage-collected at startup. Replaced revisions are **kept** (PRD §8).

Fault-injection hooks (`FaultyStore`) can fail before steps 1, 3, and 4, which covers REL-01 scenario 1 and the retry-after-leftover path.

### 4.2 Idempotency & experiment provenance

- Before embedding, compute `revision_id`. If `active/<source>.json` already points to it in this space, skip with status `succeeded (unchanged)`. Changing extraction or chunking changes `revision_id`, so the source is reprocessed.
- Every evaluation writes `experiments/<experiment_id>.json` (create-only, never rewritten) with `[(space, source_id, revision_id)]`, config fingerprints, and query/label checksums. `search --experiment` and report regeneration use that list.

### 4.3 Validation & status

- Vector validation runs on the whole source batch before staging. The REL-01 invalid-vector steps drive `ingest` against a live `FsRecordStore` and assert that the `active/` pointer and `revisions/` are unchanged, rather than calling `validate_batch` alone. One defect aborts the revision and reports the `item_id` and the defect.
- Any failed required source → `Incomplete`, exit 2.
- Logs carry IDs, stages, and token counts only, never chunk text.

### 4.4 Anonymization gate (PRD §11)

`corpus/manifest.toml` per source: `anonymization = { filename = "ok", metadata = "ok", text = "ok", pixels = "ok", reviewer = "...", date = "..." }`. `cli anonymization-check` reports PDF info dictionary/XMP keys and image EXIF/IPTC/XMP to help the reviewer fill these in. The pipeline refuses a source with any missing check (validation error, exit 3). The CLI flags identifier-like filenames (CPF/RG patterns, dates of birth, capitalized name pairs) and fails closed until the reviewer acknowledges them. Stored image assets are always re-encoded without metadata. Records get `anonymization_status = "verified"` only from this gate.

`remote_allowed` defaults to `false`. `validate()` checks it **before** `adapters::build` runs, so the OpenAI client is never constructed and `OPENAI_API_KEY` is never read for a corpus that is not permitted (REL-01).

Version control: `.env`, `store/`, corpus content, and `reports/tmp/` (raw per-run outputs) are ignored. Only reviewed final evidence under `reports/experiments/` and `reports/examples/`, produced from a gate-passed corpus, is committed.

---

## 5. Testing layers

### 5.1 Cargo test targets

```toml
[[test]]
name = "bdd"
path = "tests/bdd.rs"
harness = false

[[test]]
name = "bdd_selftest"
path = "tests/bdd_selftest.rs"
harness = false

[[test]]
name = "integration"
path = "tests/integration.rs"      # mod-includes tests/integration/*.rs; each test is #[cfg(feature = "...")] + #[ignore]
```

### 5.2 BDD runner (`tests/bdd.rs`)

```rust
// harness = false: this is a plain `main`, which clippy does not treat as test code, so it uses
// `?` + anyhow context instead of `expect`. Step functions return `Result<(), StepError>` or use
// `assert!`/`assert_eq!` with explicit messages.
#[tokio::main]
async fn main() -> anyhow::Result<ExitCode> {
    std::fs::create_dir_all("reports/bdd").context("create reports/bdd")?;
    let json = std::fs::File::create("reports/bdd/cucumber.json").context("create cucumber.json")?;
    let writer = LabWorld::cucumber()
        .fail_on_skipped()
        .with_writer(writer::Basic::stdout().summarized().tee(writer::Json::new(json)).normalized())
        .run("tests/features")
        .await;                                   // never drop the future
    let failed = writer.execution_has_failed();
    drop(writer);                                 // flush the JSON file before reading it back
    let executed = inventory::executed("reports/bdd/cucumber.json")?;
    let milestone = inventory::acceptance_milestone("config/acceptance.toml")?;
    let required = inventory::from_prd("spec/PRD.md", milestone)?; // PRD Gherkin + @mN tags
    let missing: Vec<_> = required.difference(&executed).collect();
    if failed || !missing.is_empty() || (!required.is_empty() && executed.is_empty()) {
        eprintln!("bdd: failed={failed} missing={missing:?}");
        return Ok(ExitCode::FAILURE);
    }
    Ok(ExitCode::SUCCESS)
}
```

The writer, and with it the JSON file, is dropped and flushed explicitly before the inventory reads the report back. The inventory comes from the PRD and cannot be shrunk by editing a test file. Exact writer combinator names are confirmed against cucumber 0.23 docs in Phase 0.

### 5.3 Layers

| Layer | Trigger |
|---|---|
| Unit tests beside the code (fingerprints, IDs, chunker, token truncation, RRF, metrics, retry, cost scaling, `ru_maxrss` normalization) | always |
| BDD (`bdd`) + runner self-test (`bdd_selftest`) — fixtures only, no network, no model download | always, gating |
| Integration (`integration`) — real adapters, poppler, tesseract; results to `reports/integration/`, never merged into BDD evidence; unexecuted tests are reported as **outstanding**, not as passed | `RUN_INTEGRATION=1` + features + credentials |

### 5.4 Lint configuration (Phase 0)

```toml
# Cargo.toml
[lints.rust]
unsafe_code = "deny"
unexpected_cfgs = "warn"

[lints.clippy]
all = { level = "warn", priority = -1 }
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
todo = "deny"
unimplemented = "deny"
dbg_macro = "deny"
float_cmp = "deny"
cast_possible_truncation = "warn"
cast_sign_loss = "warn"
cast_precision_loss = "warn"
```

```toml
# clippy.toml
allow-unwrap-in-tests = true
allow-expect-in-tests = true
allow-panic-in-tests = true
```

CI runs clippy with `-D warnings`, so every `warn` above blocks a merge.

---

## 6. CI (`.github/workflows/ci.yml`)

1. `cargo fmt --check`
2. `cargo clippy --all-targets -- -D warnings` (lints from §5.4) **for each feature set**: none, each engine alone, `--all-features`. This compiles every `#[cfg]` adapter.
3. `cargo test` (unit + doc + `fixtures` + `bdd` + `bdd_selftest`) with no engine features. This step gates the merge.
   - The fixture corpus is regenerated into a temporary directory from the checked-in rasters and diffed against `tests/fixtures/corpus/` (no poppler needed).
   - Measurements reported as evidence are produced with `--release`, and the report records the build profile.
4. Upload `reports/bdd/cucumber.json` + corpus/config fingerprints.
5. **Required job (local engines, no secrets):** cache the HF models pinned in `config/experiments/*.toml`, then run `RUN_INTEGRATION=1 cargo test --features engine-embed-anything,engine-fastembed --test integration -- --include-ignored`. Phase 2 cannot exit on the deterministic adapter alone.
6. OpenAI job, added in Phase 2 together with the first OpenAI integration test. It is gated by the repository variable `RUN_OPENAI_INTEGRATION == 'true'` at job level, so without credentials GitHub shows it as **skipped**, never as a green pass. It runs only the OpenAI tests (`--features engine-openai ... openai`). If it does not run, OpenAI stays an **outstanding** acceptance item.

---

## 7. Owner actions required

- [ ] Corpus + `corpus/manifest.toml` including anonymization fields and `remote_allowed`
- [ ] Relevance labels for the 10 frozen queries
- [ ] Extraction-fidelity sample (≈ 20 lab rows)
- [ ] `brew install tesseract tesseract-lang`
- [ ] `OPENAI_API_KEY` in `.env`
- [ ] Confirm or change the model defaults in §0.1

## 8. Risks & mitigations

| Risk | Mitigation |
|---|---|
| `embed_anything` build time / platform issues | Feature-gated; BDD never compiles it; CI clippy matrix catches breakage |
| Multilingual SigLIP not loadable | Phase 3 spike; fall back to English CLIP and report it |
| E5 weights differ between candle and ONNX | Record weight format and revision; use the end-to-end comparison wording |
| OCR quality on lab tables | `-layout`, row regex, fidelity sample reported |
| Real-embedding nondeterminism | Documented tolerance; BDD uses the deterministic adapter only |

---

## 9. Adversarial review log

Reviewer: Grok (grok CLI 1.0.70, headless, read-only). Findings were verified by Claude, and the resolutions are reflected in this plan and in the PRD.

| ID | Sev. | Finding | Verdict | Resolution (where) |
|---|---|---|---|---|
| G1 | Critical | Verbatim features + `fail_on_skipped` conflict with per-phase green gates; US-02/US-03 mix milestones in one feature | Valid | `@m1`–`@m4` tags on scenarios and on split `Examples` blocks (PRD §5); features grow per phase; gate = inventory ≤ current milestone (PLAN §3); runner self-test replaces "CI red" (PLAN Phase 0) |
| G2 | Critical | Revision key omits extraction/chunking fingerprints; experiments cannot re-read their revision | Valid | `revision_id` = checksum + extraction + chunking fingerprints; immutable retained revisions; append-only experiment snapshots; `search --experiment` (PRD §8, PLAN §2.1, §4.2) |
| G3 | High | `chunk_id`/`record_id` collide across modality and revisions; `experiment_id` semantics | Valid | Modality in `chunk_id`; `record_id` includes `revision_id`; `experiment_id` = producing run, immutable (PRD §8, PLAN §2.1) |
| G4 | High | Directory `rename` onto non-empty dir fails; missing parent-dir `fsync` | Valid | Per-attempt `tmp/<uuid>`, leftover reuse or quarantine, file + dir `fsync`, pointer rename is the commit point (PLAN §4.1) |
| G5 | High | E5 prefix double-applied / applied to CLIP; "true engine comparison" claim | Partly valid. fastembed-rs does **not** auto-prefix (verified in fastembed-rs docs) | `PrefixPolicy` per model, applied once inside the adapter; CLIP and OpenAI get none; end-to-end comparison wording (PLAN §0.1) |
| G6 / G30 | High | Truncation by unspecified length unit; no behaviour when disabled; prefix overflows the budget | Valid | Token counts via model tokenizer; budget minus prefix and special tokens; new REL-01 scenario "truncation disabled" fails at `chunking` (PRD §5.6, §6.3; PLAN Phase 1) |
| G7 | High | `tests/integration/` not a target; `run_and_exit` writes no report; missing `.await`; hand-edited inventory | Valid | Explicit `[[test]]` targets; JSON tee writer + `run().await`; inventory parsed from the PRD inside `bdd`; clippy per feature set (PRD §5.7, PLAN §5–6) |
| G8 | High | RRF scenario ambiguous ("1 divided by 60 plus rank"), no overlapping item, sort direction unstated | Valid. Original order A,C,B,D was arithmetically correct | Rewritten as `1 / (60 + rank)`, overlap item E, explicit scores, order `E,A,C,B,D` descending (PRD §5.3) |
| G9 / G16 | High | `.env` not ignored; manifest ignored; no anonymization checks; `remote_allowed` not default-deny; reports with chunk text tracked | Valid | `.gitignore` fixed; blocking anonymization gate; `remote_allowed=false` default, checked before client build; only reviewed evidence committed (PRD §11, PLAN §1, §4.4) |
| G10 | Medium | Hit@3 per-query rule undefined | Valid | Exact per-query definitions for P@3, Hit@3, nDCG@3 (incl. IDCG) + macro mean (PRD §5.3) |
| G11 | Medium | Fusion depth, latency N, p95 method, cold/warm undefined | Valid | `fusion_depth`, `latency_repeats=20`, nearest-rank, cold/warm definitions (PRD §6.6, §9.4) |
| G12 / G21 | Medium | Native-vs-OCR threshold undefined; `pdftoppm` writes PPM; TSV `-1`; modalities per fixture | Valid | 200 chars + 60 % token-quality rule in fingerprint; `-png -singlefile`; confidence = mean of `conf ≥ 0`; modality table; real-poppler integration test (PLAN §0.1, Phase 1) |
| G13 | Medium | `sysinfo` gives current RSS, not peak | Valid | `getrusage ru_maxrss`, normalized for macOS bytes / Linux KiB (PRD §9.4, PLAN §0.1) |
| G14 | Medium | M-CLIP not loadable as a CLIP checkpoint | Valid | Dropped; multilingual SigLIP candidate behind a Phase 3 load spike; EmbedAnything documents CLIP and SigLIP (PLAN §0.1, Phase 3) |
| G15 / G17 | Medium/High | Extraction fidelity and GPU memory missing from report | Valid | Frozen fidelity sample + per-field accuracy; GPU number or `"unavailable"` (PRD §9.4, PLAN Phase 2) |
| G16(cost) / G24 | Medium | Price per 1M tokens not scaled; OpenAI normalization unstated | Valid | `unit_quantity` in price table and Gherkin; adapter L2-normalizes explicitly (PRD §5.5, §11; PLAN §0.1) |
| G18 | High | Model revision optional → unpinned spaces | Valid | Mandatory pinned revision for real runs (PRD §6.4, PLAN §0.1) |
| G19 | High | `pdfimages` emits soft masks/stencils as images | Valid | `pdfimages -list` filter `type = image`, ≥ 64×64 px, mask fixture (PLAN §0.1) |
| G20 | High | `pdftoppm` filename, TSV header, bbox coordinate space | Valid | `-singlefile`, header skip, `level=5` rows, boxes in PDF points top-left (PLAN §0.1) |
| G22 | Medium | `async fn` traits not dyn-compatible | Valid | `async-trait` + single feature-gated factory (PLAN §2.2) |
| G23 | Medium | Engine id spelling inconsistent; real local adapters never run in CI | Valid | Canonical ids + display mapping; required local-engine integration job; OpenAI outstanding if not run (PRD §6.4, PLAN §6) |
| G25 | Medium | Generated captions have no field; could leak query terms | Valid | `metadata.generated_caption`, never embedded; image fixtures have no captions (PRD §8, PLAN Phase 3) |
| G26 | Medium | Invalid-vector scenario only calls `validate_batch` | Valid | Scenario now ingests against a live store and asserts the pointer is unchanged (PRD §5.6, PLAN §4.3) |
| G27 | Medium | No cosine tie-break; combined mode has two spaces | Valid | Tie-break by `record_id`; combined config = space pair + `k` + depth (PRD §6.6, PLAN §0.1) |
| G28 | Medium | Comma decimals and en dash break row regex | Valid | Row grammar covers `,` decimals, U+2013, `a` ranges; tests use exact fixture string + real `pdftotext` (PLAN Phase 1) |
| G29 | Medium | `lopdf` and poppler page counts may disagree | Valid | `pdfinfo` is the single source of truth; mismatch fails the source (PLAN §0.1) |
| G31 | Low | Hash preimage encoding unspecified | Valid | Canonical sorted-key JSON + NFC + golden tests (PLAN §2.1) |
