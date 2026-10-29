# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Required Rust skill

Invoke `/rust-skills` before implementing, modifying, reviewing, debugging, or testing Rust code in this project. Apply its relevant guidance throughout the task, including every implementation phase and adversarial review. The skill complements the requirements and acceptance criteria in `spec/PRD.md` and `spec/PLAN.md`; it does not replace or weaken them. If the skill is unavailable, report that explicitly rather than claiming it was used.

## Current state

The specification is complete, but the code is not written yet. `src/main.rs` is a starter binary. Everything in the architecture section below is the **planned** design from `spec/PLAN.md`. Check what actually exists before assuming a module, test target, or command is there.

- `spec/PRD.md` holds the requirements and the acceptance contract (Gherkin scenarios in §5). **The Gherkin there is normative.** Feature files are copied from it, and the BDD inventory is derived from it.
- `spec/PLAN.md` holds the implementation plan: Phase 0 (harness) followed by Phases 1–4, which map to milestones `@m1`–`@m4`. It also fixes defaults (models, OCR commands, IDs, retry policy) and logs the adversarial review in §9.
- The git root is the parent directory `CognitionLab/`. This crate lives in the `embeddings-fundamentals/` subdirectory.

## Commands

Rust 2024 edition. Run all commands from this directory.

```bash
cargo build
cargo test                          # unit + doc + bdd + bdd_selftest (+ ignored integration tests)
cargo test --lib <name>             # a single unit test by name filter
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo run -- doctor                 # checks poppler + tesseract are installed
cargo test --test bdd               # Cucumber acceptance suite (harness = false)
cargo test --test bdd_selftest      # proves the runner fails on undefined/ambiguous/skipped/missing scenarios
BDD_ARGS='--name "<regex>"' cargo test --test bdd    # filtered run (the inventory check then fails by design)
RUN_INTEGRATION=1 cargo test --test integration -- --include-ignored    # real tools/engines
cargo run --example gen_fixtures    # regenerate tests/fixtures/corpus (deterministic; --rerender needs poppler)
cargo run --example capture_extraction   # recapture tests/fixtures/extraction from real poppler/tesseract
python3 scripts/sync-features.py    # regenerate tests/features/*.feature from PRD Gherkin for the acceptance milestone
cargo run -- manifest --manifest tests/fixtures/corpus/manifest.toml
cargo run --features engine-embed-anything -- ingest --manifest tests/fixtures/corpus/manifest.toml \
    --config config/experiments/baseline-text-embed-anything.toml --store store --experiment <id>
cargo run --features engine-embed-anything -- search --config config/experiments/baseline-text-embed-anything.toml \
    --store store --query "Onde está o resultado da hemoglobina?" --limit 3
# Image run (CLIP or SigLIP, `model_kind = "text_to_image"`, `[ingest] modality = "image"`):
cargo run --release --features engine-embed-anything -- experiment --manifest tests/fixtures/corpus/manifest.toml \
    --config config/experiments/image-clip-vit-b32.toml --store store --experiment <id> --report <out>.json
cargo run --release --features engine-embed-anything -- search --mode image \
    --config config/experiments/image-clip-vit-b32.toml --store store --query "Mostre a radiografia de tórax"
```

Feature files are **generated**: edit the Gherkin in `spec/PRD.md`, bump `config/acceptance.toml`, and run `scripts/sync-features.py`. Never hand-edit `us*.feature` or `rel*.feature`; only `harness.feature` is hand-written. BDD never shells out: `FixtureExtractor` replays `tests/fixtures/extraction/` (captured tool output), and the deterministic adapter supplies vectors. Use `with_keyword_vector` when a scenario needs a ranking that can be derived by hand.

The harness-less targets (`bdd`, `bdd_selftest`) **never parse argv**, so `cargo test <filter>` cannot break them. Cucumber filters go through `BDD_ARGS`. The required inventory is derived from the Gherkin blocks in `spec/PRD.md`, filtered by the `@mN` tags up to `milestone` in `config/acceptance.toml`. A scenario counts as executed only if a scenario with the same feature name, scenario name and expanded step text ran and all of its steps passed (`tests/support/inventory.rs`).

The engine adapters sit behind Cargo features (`engine-embed-anything`, `engine-fastembed`, `engine-openai`), all off by default. As a result, the BDD suite never compiles candle/ONNX or downloads models. Clippy must be run per feature set to compile the `#[cfg]` adapters.

External tools: poppler (`pdftotext`, `pdftoppm`, `pdfimages`, `pdfinfo`) and `tesseract` with `por` data (`brew install tesseract tesseract-lang`), both installed here.

Progress, blockers and review records live in `reports/progress/PROGRESS.md` and `reports/reviews/phase-N/`. Read them first when resuming phase work from `spec/prompt/execute.md`.

## Architecture (PLAN §1–§4; Phases 1–3 implemented: corpus, extraction, chunking, store, pipeline, text and image retrieval, evaluation, measurement, CLI)

The pipeline runs Corpus manifest → anonymization/eligibility gate → extraction (native text or OCR, chosen per page) → structure-aware chunking → `EmbeddingStrategy` adapter → vector validation → atomic store → retrieval (text / image / RRF combined) → evaluation report.

Invariants that span several modules and are easy to break:

- **Strategy boundary.** Orchestration depends only on traits. Adapters receive **raw** text, and each adapter applies its own model prefix (E5 `query: `/`passage: `) exactly once via its `PrefixPolicy`. CLIP and OpenAI use no prefix. Extraction, chunking, and evaluation config never reach an adapter. A single feature-gated factory builds the adapters.
- **Trait dispatch (PLAN §2.2).** `#[async_trait]` is used **only** on `EmbeddingStrategy`, the one trait used as `Arc<dyn …>` (adapter chosen at runtime; combined mode holds two adapters). Every other async trait (`TextExtractor`, `OcrEngine`, `ImageExtractor`, `Sleeper`) uses native `async fn` in traits with generics, declared as `fn op(&self, ..) -> impl Future<Output = ..> + Send`. `RecordStore`, chunking, token counting, clocks, and probes are synchronous traits. The pipeline is generic over one `Components` bundle and calls blocking work (store I/O, inference, image re-encoding) through `spawn_blocking`.
- **Rust conventions (PLAN §2.3, §5.4).** These are enforced by Cargo `[lints]` and `clippy.toml`:
  - no `unwrap`/`expect`/`panic!` outside `#[test]` code. The harness-less BDD runner and its steps use `?` with `anyhow` context or `assert!` with messages;
  - `unsafe_code` is denied, and `getrusage` goes through `nix`;
  - ID newtypes and enums instead of strings, with `deny_unknown_fields` on configs;
  - float ordering with `total_cmp` and the `RecordId` tie-break;
  - `thiserror` in the library and `anyhow` only in `main`/`cli`;
  - library code only emits `tracing` events, and only `main` installs a subscriber;
  - a lint is silenced only with `#[expect(lint, reason = "...")]`, never `#[allow]`.
- **Engine ≠ model.** The canonical engine ids are `embed_anything`, `fastembed`, and `openai`. The Gherkin display names map to these ids. Real-model runs must pin `model_revision`.
- **Embedding-space identity.** `embedding_space_id` hashes engine, version, model, revision, dimensions, normalization, metric, prefix policy, and preprocessing. Equal dimensions never mean compatible spaces. A query against a mismatched space must fail before similarity search.
- **Deterministic IDs.** All hashes are SHA-256 over canonical sorted-key JSON (NFC), never `Debug` output or plain concatenation. `revision_id` = checksum + extraction fingerprint + chunking fingerprint. `chunk_id` = `{source}-p{page}-{modality}-{index}`. `record_id` = hash(revision_id, chunk_id, space). Idempotent reingestion depends on these IDs.
- **Atomic publish (PLAN §4.1).** Stage into `tmp/<uuid>`, fsync the file and its directory, rename the directory into `revisions/`, then rename the `active/<source>.json` pointer. **The pointer rename is the commit point.** Revisions are immutable and kept after replacement. Experiment snapshots are append-only and pin `(space, source, revision)`.
- **Failure semantics.** Only transient errors are retried (`max_attempts = 3`, including the first attempt). Independent sources continue after a failure. Any failed required source makes the run `Incomplete` (exit code 2). Vectors are validated (dimensions, NaN, ±inf) before staging. Empty OCR output is flagged `unreadable`. Truncation is counted in model tokens, and when truncation is disabled, oversized input fails at stage `chunking`.
- **Ranking.** Single-modality results sort by descending cosine score, with ties broken by ascending `record_id`. RRF uses `1/(60+rank)` with rank starting at 1, sums across lists, and uses the same tie-break.
- **Data handling.** `remote_allowed` defaults to false and is checked before any remote client is built or any credential is read. Logs carry IDs and counts only, never chunk text. Stored image assets are re-encoded without metadata.

## BDD contract (PRD §5.7, PLAN §5.2)

- The suite uses `cucumber` 0.23 with `output-json` (already a dev-dependency) and a `tests/bdd.rs` runner with `harness = false`. The runner calls `fail_on_skipped()`, tees a JSON writer, and must `.run(...).await`; `run_and_exit` is not used. After the run it checks executed scenarios against the inventory **parsed from `spec/PRD.md`** for milestones ≤ `config/acceptance.toml`.
- A phase adds to `tests/features/` only the scenarios and `Examples` blocks tagged with its milestone, so the suite never contains unimplemented steps.
- `#[then]` steps must compare observed vs expected values and fail on a mismatch. Deterministic BDD uses the `DeterministicEmbedder` plus fixture-backed extractors. Real-adapter evidence is separate (`tests/integration.rs`), and an unexecuted integration counts as outstanding, never as passed.
