# AGENTS.md — Embeddings Fundamentals (Medical Document Embedding Lab)

This file provides authoritative reference instructions for AI coding agents working in `embeddings-fundamentals/`.

## Required Rust Plugin (`rust-dev`)

Always use the **`rust-dev`** plugin when implementing, modifying, refactoring, reviewing, debugging, or testing Rust code in this project (and in any Rust project):

- **Rule**: Follow the `rust-dev` global rule (`~/.gemini/config/plugins/rust-dev/rules/AGENTS.md`) alongside `spec/PLAN.md` §2.2–§2.3. Where a generic guideline and `spec/PLAN.md` §2.3 differ, follow `spec/PLAN.md` and document the reason.
- **Skills** (read the relevant `SKILL.md` before starting the task):
  - `rust-async-and-errors`: Tokio async/await discipline, `spawn_blocking`, avoiding locks across `.await`, `thiserror` vs. `anyhow`, and `tracing`.
  - `rust-ownership-and-perf`: Ownership, borrowing, lifetimes, avoiding unnecessary `.clone()`, type-driven design (newtypes/enums), and `unsafe` auditing.
  - `rust-cargo-quality`: Static analysis, `cargo check`, `cargo clippy --all-targets -- -D warnings` across the feature matrix, `cargo fmt --check`, and `cargo test`.
- Never claim a skill or plugin was applied if it was not loaded and followed.

## 1. Project Overview & Current State

**Medical Document Embedding Lab** is an experimental Rust 2024 project (`rustc 1.99`, pinned via `rust-toolchain.toml`) for reproducible extraction, OCR, structure-aware chunking, multimodal embedding generation, revisioned vector storage, retrieval (text-only, image-only, and RRF-fused combined), and evaluation on Portuguese medical documents.

- **Workspace location**: `embeddings-fundamentals/` inside the `CognitionLab/` repository. Run all `cargo` and helper commands from `embeddings-fundamentals/`.
- **Sources of truth**:
  - `spec/PRD.md` — Product requirements, record schema (§8), metric formulas (§5.3, §9.4), data handling (§11), deliverables (§12), acceptance criteria (§13), and **normative Gherkin scenarios** (§5).
  - `spec/PLAN.md` — Architecture, core contracts (§2.1–§2.3), reliability and atomic storage (§4), testing layers (§5), CI (§6), and adversarial review log (§9).
  - `spec/prompt/execute.md` — Phase execution workflow, engineering rules, and mandatory sequential adversarial review protocol.
  - `reports/progress/PROGRESS.md` & `reports/ACCEPTANCE.md` — Actual implementation status, phase decisions, validation evidence, and outstanding blockers.
- **Implementation status**: Phases 0 through 4 (`milestone = 4` in `config/acceptance.toml`) are implemented in `src/` and `tests/`, with 35 required PRD scenarios (38 total with harness scenarios) passing on the **synthetic fixture corpus** (`tests/fixtures/corpus/`).
  - **Outstanding owner inputs** (never claim these as completed): real anonymized owner corpus (`corpus/`), frozen relevance labels and extraction-fidelity sample for that corpus, and live OpenAI runs (`OPENAI_API_KEY` + `remote_allowed = true`).

---

## 2. Repository Structure

```text
embeddings-fundamentals/
├── Cargo.toml                  # Single crate (lib + bin), Rust 2024, strict [lints]
├── clippy.toml                 # Allows unwrap/expect/panic only inside #[test] code
├── rust-toolchain.toml         # Pins rustc 1.99
├── config/
│   ├── acceptance.toml         # Current required BDD milestone (milestone = 4)
│   ├── evaluation/             # Frozen Portuguese queries (queries-v1.toml) & labels
│   ├── experiments/            # Saved single-mode and combined experiment TOMLs
│   ├── fidelity/               # Extraction-fidelity sample definitions
│   └── prices/                 # Versioned remote provider price tables
├── src/
│   ├── lib.rs                  # Library crate root
│   ├── main.rs                 # Thin binary entrypoint (CLI parse -> tracing -> ExitCode)
│   ├── cli.rs                  # Subcommands: doctor, manifest, anonymization-check, validate, ingest, search, experiment, combine, report
│   ├── ids.rs                  # Canonical-JSON SHA-256 hashing & validated ID newtypes
│   ├── domain.rs               # Closed-set domain enums (Modality, EngineId, ExtractionMethod, etc.)
│   ├── config.rs               # TOML schemas (deny_unknown_fields), fingerprints
│   ├── corpus.rs               # Manifest loading, checksums, blocking anonymization gate
│   ├── anonymization.rs        # Filename pattern checks, PDF info/XMP & EXIF inspection
│   ├── assets.rs               # MetadataScrubber (decode, PDF mask, EXIF orient, flatten to RGB8 PNG)
│   ├── extraction/             # TextExtractor, OcrEngine, ImageExtractor, poppler/tesseract & fixture replay
│   ├── chunking.rs             # StructureAwareV1 chunker, lab-table row grammar, token budgets
│   ├── embedding/              # EmbeddingStrategy trait, LocalTextAdapter, VisionAdapter, Deterministic, EmbedAnything, FastEmbed, OpenAI
│   ├── adapters.rs             # Single feature-gated adapter factory
│   ├── record.rs               # Indexed record schema (PRD §8)
│   ├── storage.rs              # FsRecordStore: staging, fsync, revision seal, atomic pointer publish, fcntl writer lock
│   ├── pipeline.rs             # Pre-flight validation, per-source state machine, bounded retries
│   ├── retrieval.rs            # Exact cosine search, embedding-space guard, text/image/combined modes
│   ├── fusion.rs               # Reciprocal Rank Fusion (RRF, k = 60)
│   ├── evaluation.rs           # Precision@3, Hit@3, nDCG@3, anchored image label checks
│   ├── fidelity.rs             # Field-level lab row extraction fidelity evaluator
│   ├── measurement.rs          # Clock, RusageProbe (nix getrusage), cold/warm latency stats
│   ├── cost.rs                 # Scaled provider cost estimator
│   ├── experiment.rs           # End-to-end experiment runner (ingest + evaluate + measure)
│   ├── report.rs               # JSON and Markdown comparison report generator
│   └── tools.rs                # External binary probes (pdftotext, pdftoppm, pdfimages, pdfinfo, tesseract)
├── tests/
│   ├── bdd.rs                  # Cucumber runner (harness = false) + PRD inventory check
│   ├── bdd_selftest.rs         # Proves runner fails on skipped/undefined/ambiguous/missing scenarios
│   ├── integration.rs          # Real poppler/tesseract and local model tests (RUN_INTEGRATION=1)
│   ├── pipeline.rs             # Idempotency, retries, and snapshot property tests
│   ├── experiment.rs           # Experiment evaluation & failure tests
│   ├── combined.rs             # Combined retrieval & RRF tests
│   ├── evidence.rs             # Committed report & artifact verification tests
│   ├── fixtures.rs             # Deterministic fixture generation checks
│   ├── bdd/                    # Cucumber World and step definitions
│   ├── support/                # Shared BDD runner and PRD Gherkin inventory parser
│   ├── features/               # Generated Gherkin .feature files (DO NOT hand-edit us*/rel*)
│   └── fixtures/               # Synthetic corpus, captured tool outputs, frozen rankings & metrics
├── scripts/
│   ├── sync-features.py        # Generates tests/features/*.feature from spec/PRD.md for acceptance.toml
│   ├── precompute-metrics.py   # Independently precomputes expected evaluation metrics
│   └── review-package.sh       # Builds adversarial review packages
└── reports/                    # Progress log, phase comparison reports, spikes, and adversarial reviews
```

---

## 3. Essential Commands

Run all commands from `embeddings-fundamentals/`:

### Build, Format, Lint & Unit/BDD Tests
```bash
# Format check
cargo fmt --check

# Clippy across the full additive feature matrix (required before committing)
for f in "" engine-embed-anything engine-fastembed engine-openai engine-embed-anything,engine-fastembed,engine-openai; do
  cargo clippy --all-targets ${f:+--features $f} -- -D warnings
done

# Default test suite (unit, pipeline, experiment, combined, fixtures, BDD, bdd_selftest)
cargo test

# Run a single unit test
cargo test --lib <test_name_filter>

# OpenAI adapter unit/contract tests (uses local HTTP test server)
cargo test --features engine-openai --lib embedding::openai
```

### BDD Acceptance Suite & Feature Sync
```bash
# Run the Cucumber BDD suite and PRD inventory check
cargo test --test bdd

# Run the BDD runner self-test suite
cargo test --test bdd_selftest

# Filter BDD scenarios during debugging (note: inventory check will fail by design on filtered runs)
BDD_ARGS='--name "<regex>"' cargo test --test bdd

# Regenerate tests/features/*.feature after editing Gherkin in spec/PRD.md or bumping config/acceptance.toml
python3 scripts/sync-features.py
```

### Integration Tests & External Tools
```bash
# Check installed external tools (poppler & tesseract with por+eng)
cargo run -- doctor

# Run real tool & local model integration tests (requires poppler, tesseract, HF cache)
RUN_INTEGRATION=1 cargo test --release --features engine-embed-anything,engine-fastembed --test integration -- --include-ignored

# Regenerate synthetic fixture corpus or recapture extraction outputs
cargo run --example gen_fixtures
cargo run --example capture_extraction
```

### CLI Operations
```bash
# Validate manifest and anonymization gate
cargo run -- manifest --manifest tests/fixtures/corpus/manifest.toml
cargo run -- anonymization-check --manifest tests/fixtures/corpus/manifest.toml

# Ingest and search (text baseline)
cargo run --features engine-embed-anything -- ingest \
  --manifest tests/fixtures/corpus/manifest.toml \
  --config config/experiments/baseline-text-embed-anything.toml \
  --store store --experiment demo-text-1

cargo run --features engine-embed-anything -- search --mode text \
  --config config/experiments/baseline-text-embed-anything.toml \
  --store store --query "Onde está o resultado da hemoglobina?" --limit 3
```
See `reports/README.md` for full release-mode `experiment`, `combine`, and `report` reproduction commands.

---

## 4. Architectural & Domain Invariants

Any code change must preserve these cross-cutting contracts (defined in `spec/PRD.md` and `spec/PLAN.md` §2–§4):

1. **Strategy Boundary & Prefix Policy**:
   - Pipeline orchestration depends only on traits and never leaks extraction, chunking, or evaluation config into an embedding adapter.
   - Adapters receive **raw** text (`&[TextInput]`). Model-specific prefixes (such as `query: ` and `passage: ` for E5) are applied **exactly once inside the adapter** via `PrefixPolicy`. MiniLM, CLIP, SigLIP, and OpenAI use `PrefixPolicy::None`.
   - Token budgets (`EmbeddingStrategy::input_budget`) subtract prefix tokens and special tokens from `max_input_tokens` so the pipeline truncates or rejects oversized chunks *before* calling the engine.

2. **Engine vs. Model & Embedding-Space Isolation**:
   - Canonical engine IDs are `embed_anything`, `fastembed`, and `openai` (mapped from Gherkin display names `EmbedAnything`, `FastEmbed`, `OpenAI`).
   - Real-model configs must pin an immutable `model_revision` (validation fails with exit code `3` if omitted).
   - `embedding_space_id` hashes the canonical JSON of `SpaceDescriptor` (engine, engine version, model ID, model revision, weight format, dimensions, normalization, metric, prefix policy, image preprocessing).
   - Equal vector dimensions **never** imply compatibility. Retrieval must check `embedding_space_id` and fail with a space-mismatch error *before* computing cosine similarity.

3. **Deterministic Canonical IDs**:
   - All fingerprints and IDs (`extraction_fingerprint`, `chunking_fingerprint`, `revision_id`, `embedding_space_id`, `record_id`) are SHA-256 hashes over **canonical sorted-key JSON with UTF-8 NFC strings** (`src/ids.rs`), never `Debug` formatting or unseparated string concatenation.
   - `chunk_id` follows `{source_id}-p{page}-{modality}-{index}` (e.g., `chest-001-p1-image-0`). Frozen relevance labels key on `chunk_id`.

4. **Atomic Storage & Revision Provenance (`FsRecordStore`)**:
   - Staging writes to `tmp/<uuid>/records.jsonl` + `revision.json` seal, `fsync`s file and directory, atomically renames into `revisions/<source_id>/<revision_id>/`, and then atomically renames `active/<source_id>.json`. **The pointer rename is the commit point.**
   - Failed replacements leave the previous active revision intact and searchable.
   - Committed revisions are immutable and retained after replacement so append-only experiment snapshots (`experiments/<experiment_id>.json`) can always re-read the exact revisions they used.
   - Writers acquire a per-process POSIX record lock (`fcntl F_SETLK` + in-process registry) on `.writer.lock` so concurrent subprocess spawns (`fork`/`exec`) cannot inherit and leak an `flock`.

5. **Failure Semantics & Exit Codes**:
   - Only `ErrorClass::Transient` errors (HTTP 408/429/5xx, timeouts, connection resets, model-load I/O) are retried up to `max_attempts = 3` (initial attempt included). `ErrorClass::Permanent` fails immediately.
   - Independent sources continue when one source fails, but any failed required source marks the overall run `Incomplete`.
   - Vectors are validated for expected count, dimensions, and finite values (`!is_nan() && is_finite()`) before staging.
   - Empty OCR output on a required scanned page flags the page `unreadable` and marks the run `Incomplete`.
   - CLI exit codes: `0` = succeeded, `1` = internal error, `2` (`EXIT_INCOMPLETE`) = incomplete run, `3` (`EXIT_VALIDATION`) = validation refused (ineligible corpus, missing remote permission, unpinned model, unsupported capability).

6. **Ranking & Fusion**:
   - Single-modality search orders results by descending cosine similarity (`f32::total_cmp`) and breaks ties by ascending `RecordId`.
   - Combined retrieval uses Reciprocal Rank Fusion (`RRF`, `k = 60`, 1-based ranks): $\text{score}(d) = \sum \frac{1}{60 + \text{rank}(d)}$, sorted descending with ascending `RecordId` tie-break. Raw similarity scores from different spaces are never compared directly.

7. **Data Handling & Privacy Gate**:
   - Every source in `manifest.toml` must pass the blocking anonymization gate (`filename`, `metadata`, `text`, `pixels`, `reviewer`, `date`).
   - `remote_allowed` defaults to `false` and is checked during validation **before** any remote adapter is built or `OPENAI_API_KEY` is read.
   - Stored image assets are scrubbed and re-encoded as metadata-free RGB8 PNGs (`store/assets/<sha256>.png`).
   - Operational logs (`tracing`) and error messages must record only IDs, stages, attempts, and token counts—**never** document text, chunk text, or API credentials.

---

## 5. Rust Engineering Conventions

Enforced by `Cargo.toml` `[lints]`, `clippy.toml`, and `spec/PLAN.md` §2.2–§2.3:

- **No Panics in Production**: `unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented`, `dbg_macro`, and `float_cmp` are `deny` lints across the crate.
  - Regular `#[test]` functions and `#[cfg(test)]` modules may use `.expect("descriptive reason")`.
  - Harness-less test binaries (`tests/bdd.rs`, `tests/bdd_selftest.rs`, and modules under `tests/bdd/` and `tests/support/`) are **not** compiled as `#[cfg(test)]` by Clippy: use `?` with `anyhow::Context`, `Result`-returning steps, and `assert!` / `assert_eq!` with explicit messages.
- **No `unsafe`**: `unsafe_code = "deny"` crate-wide. Process memory high-water mark (`ru_maxrss`) uses the safe `nix::sys::resource::getrusage` wrapper.
- **Lint Suppressions**: Never use `#[allow(...)]`. When a lint suppression is genuinely required on a narrow item (such as a range-checked numeric cast), use `#[expect(clippy::..., reason = "...")]`.
- **Error Handling**:
  - Library modules define typed `thiserror` enums (`ExtractionError`, `ChunkError`, `EmbeddingError`, `StoreError`, `ValidationError`, `EvalError`) with `#[source]` chains and lowercase messages without trailing punctuation.
  - `anyhow` is restricted to `main.rs`, `cli.rs`, and test harnesses.
- **Trait Dispatch & Async Discipline**:
  - `#[async_trait]` is used **exclusively** on `EmbeddingStrategy` (the only trait used via `Arc<dyn EmbeddingStrategy>`).
  - All other async traits (`TextExtractor`, `OcrEngine`, `ImageExtractor`, `Sleeper`) use native `async fn` in traits (`-> impl Future<Output = ...> + Send`) with static dispatch.
  - Synchronous traits (`RecordStore`, `ChunkingStrategy`, `TokenCounter`, `Clock`, `MemoryProbe`) remain synchronous; CPU-heavy inference, image decoding/encoding, and blocking filesystem syncs run inside `tokio::task::spawn_blocking`.
  - Never hold a `std::sync::Mutex` lock across an `.await` point.
  - External subprocesses (`tokio::process::Command`) must set `kill_on_drop(true)` and enforce a timeout.
- **Type Safety & Serde**:
  - Wrap identifiers in validated newtypes (`SourceId`, `ChunkId`, `RevisionId`, `RecordId`, `EmbeddingSpaceId`, `ExperimentId`, `QueryId`, `Checksum`) and page numbers in `NonZeroU32`.
  - Use `#[serde(deny_unknown_fields)]` on all configuration and manifest structs so typos in TOML files fail fast.
- **Additive Features**:
  - Cargo features (`engine-embed-anything`, `engine-fastembed`, `engine-openai`) are strictly additive (`dep:` syntax) and off by default. Note that `embed_anything` must **not** enable its `ort` feature because `fastembed` links a different `ort` RC version.

---

## 6. BDD & Adversarial Review Workflow

- **Generated Feature Files**: `tests/features/us*.feature` and `tests/features/rel01.feature` are generated verbatim from the Gherkin blocks in `spec/PRD.md` by `python3 scripts/sync-features.py` according to the milestone in `config/acceptance.toml`. Never edit those `.feature` files by hand (only `tests/features/harness.feature` is hand-maintained).
- **Inventory Verification**: `tests/bdd.rs` runs `fail_on_skipped()`, flushes `reports/bdd/cucumber.json`, and verifies that every scenario and `Examples` row tagged `@m1`..`@mN` in `spec/PRD.md` executed and passed.
- **Deterministic vs. Live Evidence**: BDD tests never access the network, download models, or shell out to poppler/tesseract; they use `DeterministicEmbedder` and `FixtureExtractor`. Real tool and model runs live in `tests/integration.rs` (`RUN_INTEGRATION=1`).
- **Sequential Adversarial Reviews**: Per `CLAUDE.md` and `spec/prompt/execute.md` §7, each phase undergoes sequential external adversarial reviews (**Grok → Codex → Antigravity**), never in parallel, with raw reports and finding resolution tables recorded in `reports/reviews/phase-N/`.
