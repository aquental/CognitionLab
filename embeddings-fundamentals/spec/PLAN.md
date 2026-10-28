# PLAN — Medical Document Embedding Lab

**Source:** `spec/PRD.md` (Draft, revised after adversarial review — see §9)  
**Target:** Rust 2024 edition, single crate (`lib` + `bin`), rustc 1.99  
**Status:** Ready to implement — §0.1 lists the defaults this plan fixes where the PRD leaves a choice open.

---

## 0. PRD readiness assessment

The PRD specifies scope, architecture, record schema, metric formulas, Gherkin acceptance scenarios with milestone tags, the reliability contract, the BDD runner contract, deliverables and milestones. **Implementation can start now.** The items below are defaults this plan fixes (§0.1) or inputs only the owner can supply (§0.2).

### 0.1 Defaults fixed by this plan (override in `config/*.toml`)

| Topic | Default | Notes |
|---|---|---|
| Text model (local) | `intfloat/multilingual-e5-base` (768-d, cosine, L2-normalized) via **EmbedAnything** and **FastEmbed** | Same model *name*, but different weight formats and runtimes (candle/safetensors vs ONNX). Stage 2 results are reported as **end-to-end configuration comparisons** (PRD §10), never as pure engine effects. Each adapter records `model_revision` (HF commit) and weight format. |
| E5 prefixes | `query: ` / `passage: ` added **once, inside the adapter**, driven by a per-model `PrefixPolicy` | The pipeline passes raw text to the adapter (US-04 "unchanged chunks and query texts"). fastembed-rs `TextEmbedding::embed` adds **no** prefix itself (verified in fastembed-rs docs), so the FastEmbed adapter adds it. CLIP/SigLIP and OpenAI use `PrefixPolicy::None`. |
| Model revision | Every real-model experiment TOML pins `model_revision` (HF commit SHA, or the OpenAI model snapshot name such as `text-embedding-3-small` plus the response `model` field). Validation refuses a real adapter with no pin (PRD §6.4) | The same value feeds `ModelInfo` and `embedding_space_id`. |
| Engine ids | Canonical `embed_anything`, `fastembed`, `openai`; a `EngineId::from_display("EmbedAnything")` mapping serves the Gherkin names | BDD asserts the canonical id in records and reports. |
| Text model (remote) | OpenAI `text-embedding-3-small` (1536-d) | Price TOML: `{ currency = "USD", unit = "token", unit_quantity = 1_000_000, unit_price = <published> }`; cost = usage / unit_quantity × unit_price. A unit test with 1,000 tokens catches an error of 10^6. Each adapter declares `normalized`. OpenAI and E5 vectors are explicitly L2-normalized by the adapter (an idempotent operation), so the flag is always true by construction. |
| Text→image model | Baseline `openai/clip-vit-base-patch32` (512-d) via EmbedAnything | EmbedAnything documents CLIP and SigLIP vision support. Portuguese candidate: `google/siglip-base-patch16-256-multilingual`, **only if** a Phase 3 load spike proves EmbedAnything loads it. Otherwise English CLIP is the only image model, and weak Portuguese alignment is reported as a finding. M-CLIP is dropped because it is not a CLIP checkpoint that the engine can load. |
| Token limits | `Capabilities.max_input_tokens`, counted with the model's own `tokenizers` tokenizer (`tokenizer.json` from the model repo): E5 512, CLIP 77, OpenAI 8191 (`cl100k_base` via `tiktoken-rs`) | Truncation records token counts, not character counts. |
| Native vs OCR per page | Use native text when `pdftotext` yields ≥ 200 non-whitespace chars **and** ≥ 60 % of the whitespace-separated tokens are dictionary-like (alphanumeric, with Portuguese diacritics allowed). Otherwise render and OCR | Catches the garbage text layers found on scanned PDFs. Both thresholds are versioned in the extraction fingerprint. |
| OCR | `pdftoppm -png -singlefile -r 300 -f N -l N <pdf> <tmp>/page` (writes exactly `<tmp>/page.png`) → `tesseract <tmp>/page.png - -l por+eng --psm 6 tsv` behind an `OcrEngine` trait | Without `-png`, `pdftoppm` writes PPM, and without `-singlefile` it appends `-<page>` to the name. The parser skips the TSV header row and uses only `level = 5` (word) rows. Word boxes are converted from rendered pixels to PDF points (× 72/dpi), keeping a top-left origin. `bounding_box` stores `{x, y, w, h, unit: "pt", origin: "top-left"}`. `extraction_confidence` is the mean `conf` of words with `conf ≥ 0`, scaled to 0–1, or null when no word has `conf ≥ 0` (Tesseract uses `-1` for non-word rows). |
| Native PDF text | `pdftotext -layout -f N -l N` (poppler is installed). **Page count and order come only from `pdfinfo`** (`Pages:`); a per-page extraction that fails, or a page count that disagrees with `lopdf`, fails the source at stage `extraction`. `lopdf` is used only to read metadata for the anonymization check | `-layout` keeps each lab-table row on one line. |
| Embedded images | `pdfimages -list` first. Only rows with `type = image` are kept; `smask`, `stencil`, and images smaller than 64×64 px are dropped. Then `pdfimages -png -p -f N -l N` and match files by object number | A fixture PDF with a soft mask tests the filter. |
| Modalities per input | Searchable/scanned PDF → text records only (a page rendered for OCR is not an image record). Mixed PDF → text records + one image record per embedded raster image ≥ 64×64 px. PNG/JPEG → one image record | Each fixture's `expected.toml` lists `(page, modality, count)`. |
| Vector store | Filesystem, one directory per `embedding_space_id`, JSONL per revision, exact brute-force cosine | Exact search makes BDD rankings deterministic. |
| Identifiers | See §2.1 | Deterministic; revision-scoped; modality-aware. |
| Error classes | Transient: HTTP 408/429/5xx, timeouts, connection reset, model-load I/O. Permanent: other 4xx, capability, validation, invalid vectors, input-limit | `EmbeddingError::class()` |
| Retry | `max_attempts = 3` (initial attempt included), backoff 200 ms × 2, injectable `Sleeper` | |
| Ranking ties | Single-modality search sorts by descending cosine score, then ascending `record_id` | Deterministic frozen rankings (PRD §6.6). |
| Fusion | RRF `k = 60`, `fusion_depth = all`. A combined experiment = `(text_space_id, image_space_id, k, depth)` | PRD §6.6 |
| Latency | `latency_repeats = 20` after 1 discarded warm-up query; nearest-rank median/p95 | PRD §9.4 |
| Peak RAM | `libc::getrusage(RUSAGE_SELF).ru_maxrss`, normalized to bytes (macOS reports bytes, Linux reports KiB) | `sysinfo` reports current RSS, not the peak, so it is not used. |
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
| `clap` (derive) | 4.6 | CLI |
| `thiserror` | 2 | errors |
| `toml` | 1.1 | config |
| `sha2`, `hex` | 0.11 / 0.4 | checksums, fingerprints |
| `lopdf` | 0.46 | PDF page count, info dictionary / XMP for anonymization check |
| `image` | 0.25 | decode PNG/JPEG; re-encode without metadata |
| `kamadak-exif` | 0.6 | detect EXIF before stripping |
| `tokenizers` | 0.22 | token counting for HF models |
| `tiktoken-rs` | 0.7 | token counting for OpenAI |
| `libc` | 0.2 | `getrusage` |
| `async-trait` | 0.1 | dyn-compatible async strategy trait |
| `unicode-normalization` | 0.1 | NFC for canonical hashing |
| `tracing`, `tracing-subscriber` | 0.1 / 0.3 | ID-only logs |
| `embed_anything` | 0.7 (optional) | engine 1 |
| `fastembed` | 7.5 (optional) | engine 2 |
| `reqwest` (present) | 0.13 | OpenAI (feature `engine-openai`) |
| dev: `cucumber` | 0.23, features `["output-json"]` | BDD + JSON report |
| dev: `tempfile`, `approx`, `proptest` | 3 / 0.5 / 1 | isolation, tolerance, RRF properties |
| dev: `gherkin` | (version re-exported by cucumber) | parse PRD Gherkin to build inventory |

Exact versions of `kamadak-exif`, `tokenizers`, `tiktoken-rs` are checked with `cargo search` in Phase 0 before pinning.

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
| `embedding_space_id` | sha256(engine, engine_version, model_id, model_revision, dims, normalized, metric, prefix policy, preprocessing) |
| `record_id` | sha256(canonical JSON `{revision_id, chunk_id, embedding_space_id}`)[..16] |

Equal dimensions never imply equal spaces (US-04).

### 2.2 Traits

```rust
pub struct Capabilities { text_documents, text_queries, images, text_to_image_compatible, batch,
                          max_input_tokens: Option<usize>, max_batch: Option<usize>, execution: Local | Remote }
pub struct ModelInfo { engine, engine_version, model_id, model_revision: Option<String>, weight_format,
                       dimensions, normalized, metric, prefix_policy, embedding_space_id }

// `async fn` in traits is not dyn-compatible, so the trait uses `#[async_trait]` (boxed futures).
// `adapters::build(&EngineConfig) -> Result<Arc<dyn EmbeddingStrategy>>` is the only factory and the
// only place behind `#[cfg(feature = ...)]`. An engine compiled without its feature returns a capability error.
#[async_trait::async_trait]
pub trait EmbeddingStrategy: Send + Sync {
    fn capabilities(&self) -> Capabilities;
    fn model_info(&self) -> ModelInfo;
    fn token_counter(&self) -> &dyn TokenCounter;
    fn input_budget(&self, kind: InputKind) -> Option<usize>; // limit minus prefix + special tokens
    async fn embed_documents(&self, raw: &[TextInput]) -> Result<EmbeddingBatch, EmbeddingError>; // adapter applies PrefixPolicy
    async fn embed_queries(&self, raw: &[TextInput]) -> Result<EmbeddingBatch, EmbeddingError>;
    async fn embed_images(&self, assets: &[AssetRef]) -> Result<EmbeddingBatch, EmbeddingError>;
}
pub fn validate_batch(batch, info) -> Result<(), VectorDefect { item_id, kind: WrongDims | NaN | Inf }>;

pub trait TextExtractor; pub trait OcrEngine; pub trait ImageExtractor; pub trait MetadataScrubber;
pub trait ChunkingStrategy { fn id(&self) -> &str; fn chunk(&self, page, counter, policy) -> Result<Vec<Chunk>, ChunkError>; }

pub trait RecordStore {
    fn stage(&self, space, source_id, revision_id, records) -> Result<StagedRevision>;
    fn publish(&self, staged) -> Result<()>;                         // §4.1
    fn active(&self, space) -> Vec<RevisionRef>;
    fn records(&self, space, revisions: &[RevisionRef]) -> Vec<Record>; // revision-pinned reads
}
pub trait Clock; pub trait MemoryProbe; pub trait GpuProbe; pub trait UsageSource; pub trait Sleeper;
```

The pipeline depends only on these traits. Adapters never see chunking, extraction, or evaluation configuration.

---

## 3. Phases

**Gating rule:** phase *N* is complete when `cargo test --test bdd` executes every scenario and example row tagged `@m1`…`@mN` in PRD §5 (the inventory is derived from the PRD, §5.2), all of them pass, and none is skipped. Phase *N* adds to `tests/features/` only the scenarios and `Examples` blocks tagged `@mN`, copied from PRD §5, so the suite never contains unimplemented scenarios. `config/acceptance.toml` holds `milestone = N`.

### Phase 0 — Scaffolding & BDD harness

1. Convert to `lib` + `bin`. Add dependencies, Cargo features `engine-embed-anything`, `engine-fastembed`, `engine-openai` (default: none), and the `.gitignore` from §1.
2. `tests/bdd.rs` (§5.2): `World` with `TempDir` store, call logs, fault flags; tee console + JSON writer; `.run(...).await`; inventory check; explicit exit.
3. **Runner self-test** `tests/bdd_selftest.rs` (separate `[[test]]`): runs the same runner builder against `tests/fixtures/selftest/` features (one with an undefined step, one empty dir, one ambiguous step). It asserts each case yields a failure. This proves `fail_on_skipped` and empty-suite detection without making the real suite red.
4. `DeterministicEmbedder`: SHA-256-seeded normalized vectors. Configurable dims, `embedding_space_id`, capabilities, prefix policy, and execution (Local/Remote). Fault injection: `fail_n_times(class)`, `defect(kind, item)`. Records every raw input and which method was called.
5. Synthetic fixtures (generated by `tests/fixtures/gen.rs` from text, checked in with checksums). PDF fixtures are built with `lopdf` so no PDF tool is needed at test time. Each fixture has an `expected.toml`.
6. CI (§6).

**Exit:** `milestone = 0`, the inventory is empty, and the self-test passes. CI is green.

### Phase 1 — Milestone 1: Text baseline (`@m1`)

Scenarios: US-01 (text PDFs, lab row, mixed-page extraction, idempotency), US-02 (text, empty index), REL-01 (all, including the new truncation-disabled scenario and remote-permission scenario with a deterministic adapter whose `execution = Remote`).

1. `corpus::Manifest` + anonymization gate (§4.4) + checksum verification.
2. Extraction: `PopplerTextExtractor`, `TesseractOcr`, per-page selection rule (§0.1), empty OCR → `unreadable`. Fixture-backed implementations for BDD, plus an integration test that runs real poppler on the generated two-page mixed PDF and asserts `native`/`ocr` per page.
3. `StructureAwareV1` chunker: sections, paragraphs, table rows with header attached. Row detection accepts comma decimals (`13,5`), en-dash/hyphen/"a" ranges (`12,0–16,0` with U+2013, `12,0-16,0`, `12,0 a 16,0`), and both `|`-separated and whitespace-aligned columns. Unit tests use the exact PRD fixture string and real `pdftotext -layout` output from the generated fixture PDF. Splits oversized units when possible. Token-based truncation when enabled (`TruncationRecord { original_tokens, submitted_tokens }`). Counts include the adapter's prefix and special tokens: the budget is `max_input_tokens − prefix_tokens − special_tokens`, obtained from `EmbeddingStrategy::input_budget()`. Fails at stage `chunking` when truncation is disabled and an indivisible unit exceeds the limit.
4. `FsRecordStore` (§4.1) with revision-pinned reads. Append-only experiment snapshots (§4.2).
5. `pipeline::ingest`: validation first (capabilities, space, remote permission, eligibility). Then the per-source state machine `Extracted → Chunked → Embedded → Validated → Staged → Published | Failed{stage, reason, attempts}` with class-based retry. Independent sources continue. Run status `Succeeded | Incomplete`.
6. `EmbedAnythingAdapter` (E5, `PrefixPolicy::E5`).
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
2. If `revisions/<source>/<revision_id>/` already exists **and** its `records.jsonl` checksum matches, reuse it (idempotent retry). If it exists but is incomplete or mismatched (a crash between steps 3 and 4), move it to `tmp/<uuid>-stale/` first. Directory `rename` onto a non-empty directory fails on both platforms, so a leftover is never overwritten in place.
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
#[tokio::main]
async fn main() -> std::process::ExitCode {
    let json = std::fs::File::create("reports/bdd/cucumber.json").unwrap();
    let writer = LabWorld::cucumber()
        .fail_on_skipped()
        .with_writer(writer::Basic::stdout().summarized().tee(writer::Json::new(json)).normalized())
        .run("tests/features")
        .await;                                   // never drop the future
    let executed = collect_executed_scenarios("reports/bdd/cucumber.json");
    let required = inventory::from_prd("spec/PRD.md", acceptance_milestone()); // parses PRD Gherkin + @mN tags
    let missing = required.difference(&executed);
    if writer.execution_has_failed() || !missing.is_empty() || executed.is_empty() { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}
```

The JSON file is dropped (and flushed) before `main` returns. The inventory comes from the PRD and cannot be shrunk by editing a test file. Exact writer combinator names are confirmed against cucumber 0.23 docs in Phase 0.

### 5.3 Layers

| Layer | Trigger |
|---|---|
| Unit tests beside the code (fingerprints, IDs, chunker, token truncation, RRF, metrics, retry, cost scaling, `ru_maxrss` normalization) | always |
| BDD (`bdd`) + runner self-test (`bdd_selftest`) — fixtures only, no network, no model download | always, gating |
| Integration (`integration`) — real adapters, poppler, tesseract; results to `reports/integration/`, never merged into BDD evidence; unexecuted tests are reported as **outstanding**, not as passed | `RUN_INTEGRATION=1` + features + credentials |

---

## 6. CI (`.github/workflows/ci.yml`)

1. `cargo fmt --check`
2. `cargo clippy --all-targets -D warnings` **for each feature set**: none, each engine alone, `--all-features`. This compiles every `#[cfg]` adapter.
3. `cargo test` (unit + `bdd` + `bdd_selftest`) with no engine features. This step gates the merge.
4. Upload `reports/bdd/cucumber.json` + corpus/config fingerprints.
5. **Required job (local engines, no secrets):** cache the HF models pinned in `config/experiments/*.toml`, then run `RUN_INTEGRATION=1 cargo test --features engine-embed-anything,engine-fastembed --test integration -- --include-ignored`. Phase 2 cannot exit on the deterministic adapter alone.
6. Optional job (OpenAI secret): `RUN_INTEGRATION=1 cargo test --features engine-openai --test integration -- --include-ignored`, then upload `reports/integration/`. If it is not run, OpenAI stays an **outstanding** acceptance item.

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
