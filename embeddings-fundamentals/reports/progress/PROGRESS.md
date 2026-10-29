# Progress record — execution of `spec/prompt/execute.md`

This file lets work resume without repeating completed steps. Each phase lists implementation, acceptance criteria, validation evidence, reviews, findings, resolutions, and blockers.

## Decisions taken before Phase 0 (2026-10-28, owner answers)

| Topic | Decision |
|---|---|
| Tesseract | Installed (`brew install tesseract tesseract-lang`): tesseract 5.5.3 with `por` and `eng` |
| Hugging Face models | Download allowed for real integration tests (E5 base candle + ONNX, CLIP ViT-B/32, SigLIP candidate) |
| Corpus, relevance labels, fidelity sample | Not supplied. Work continues on **synthetic fixtures**. Real-corpus evaluation stays an **outstanding blocker**, never reported as done |
| OpenAI | No `OPENAI_API_KEY`. The adapter and contract tests get implemented. Live integration stays **outstanding** |
| Branches / PRs | One branch and one PR per phase, stacked: `phase-N/...` is based on `phase-(N-1)/...`, so each PR shows only its phase |

## Reviewers

| Reviewer | Command | Mode |
|---|---|---|
| Grok | `grok --permission-mode plan --no-subagents --disable-web-search --prompt-file <pkg>` (1.0.70) | Headless and read-only; the package is inlined and no tool use is allowed |
| Codex | `codex exec -s read-only -C embeddings-fundamentals <prompt>` (codex-cli 0.165.0, ChatGPT login) | A separate process with a read-only sandbox |
| Antigravity | `agy --print --mode plan` (Antigravity CLI), told to use the `rust-dev` plugin | A separate process in read-only plan mode. Added by the owner on 2026-10-29, from Phase 4, as the third reviewer after Grok and Codex (see `CLAUDE.md`) |

## Phase 0 — Scaffolding & BDD harness

**Status:** ✅ APPROVED (2026-10-28). Grok rounds 1–8 and Codex rounds 1–3 are complete; the final Grok round and the final Codex round both reported NO FINDINGS.

### Implementation
- `lib` + `bin` split. `main.rs` is thin; the `doctor` command probes poppler and tesseract.
- Cargo: `rust-version = 1.99`, `rust-toolchain.toml`, additive features `engine-embed-anything`, `engine-fastembed`, `engine-openai` (the last now gates `reqwest`), `[lints]` (PLAN §5.4), `clippy.toml`.
- `src/ids.rs`: canonical JSON (explicit key sort, independent of serde_json's `preserve_order`; NFC strings), SHA-256 helpers, validated ID newtypes, `RevisionId`/`EmbeddingSpaceId`/`RecordId`/`ChunkId` constructors. The golden revision ID was computed independently in Python.
- `src/embedding/`: `EmbeddingStrategy` (the only `#[async_trait]` trait), `Capabilities::require`, `SpaceDescriptor` → `ModelInfo` → `EmbeddingSpaceId`, `PrefixPolicy`, `EngineId` with canonical IDs and display mapping, `ErrorClass`, `validate_batch` (item order/count, dimensions, NaN, ±inf), `l2_normalize`.
- `DeterministicEmbedder`: SHA-256-seeded normalized vectors, prefix applied once, call log (raw and submitted text), scripted transient/permanent failures, injected vector defects, frozen-vector overrides, engine label for contract tests.
- `tests/support/inventory.rs` + `runner.rs`: shared runner that tees console and the JSON writer, uses `fail_on_skipped`, never parses argv (`BDD_ARGS` instead), drops the writer to flush, and checks the PRD-derived inventory (feature + scenario + expanded steps, all steps passed).
- `tests/bdd.rs` (milestone from `config/acceptance.toml`), `tests/bdd_selftest.rs` (10 cases + real-PRD inventory counts), `tests/integration.rs` (tool probe, `RUN_INTEGRATION=1`).
- `examples/gen_fixtures.rs` → `tests/fixtures/corpus/`:
  - searchable two-page lab report;
  - the same report scanned (rendered, no text layer);
  - a mixed ultrasound report (native page with an embedded ultrasound, a 48 px logo with a soft mask, and a scanned page 2);
  - `chest-001.png` and `abd-us-001.jpg`;
  - `manifest.toml` and `expected.toml`.

  The output is deterministic.
- CI: `.github/workflows/embeddings-fundamentals.yml` (fmt, clippy feature matrix, tests, BDD artifact; local-engine integration job; optional OpenAI job that reports itself outstanding when the secret is absent).

### Acceptance criteria (PLAN Phase 0 exit)
| Criterion | Evidence |
|---|---|
| `milestone = 0`, inventory empty | `bdd inventory: milestone 0, 0 required, 0 executed` |
| Self-test passes | `bdd selftest: 10 cases + real PRD inventory, 0 mismatches` |
| CI green | Local equivalent green (see Validation). GitHub run is checked on the PR |

### Validation (local, 2026-10-28)
- `cargo fmt --check`: ok
- `cargo clippy --all-targets -- -D warnings` for none, each engine, and all features: ok
- `cargo test` (final state): 36 unit tests pass; `fixtures` 2 pass; `bdd` passes (milestone 0, 3 harness scenarios); `bdd_selftest` has 12 cases + real PRD inventory with 0 mismatches; `integration` has 1 test ignored by default (and fails as outstanding if included without `RUN_INTEGRATION=1`)
- `RUN_INTEGRATION=1 cargo test --test integration -- --include-ignored`: 1 passed (poppler 26.10.0, tesseract 5.5.3)
- Real tools on fixtures: `pdftotext -layout` returns the lab rows intact; `pdfimages -list` shows image/smask types as expected; tesseract (`por+eng`, psm 6) OCRs the scanned page and returns `12,0-16,0` with an ASCII hyphen (input for the G28 row grammar)

### rust-skills applied
`err-thiserror-lib`, `err-no-unwrap-prod` (lints), `type-newtype-ids`, `api-parse-dont-validate`, `serde-deny-unknown-fields`, `num-float-compare`, `num-cast-try-from`, `async-fn-in-trait`/`trait-object-safety` (async_trait only on the dyn strategy), `proj-lib-main-split`, `proj-feature-additive`, `obs-library-facade`, `test-proptest-properties`, `lint-workspace-lints` (crate-level), `unsafe` denied.

**Deviations from PLAN §2.3:** none. The PLAN text was updated for the self-test design and the fixture generator location.

### Reviews
Verbatim reports and the tracking table are in `reports/reviews/phase-0/` (`FINDINGS.md`).

| Reviewer | Rounds | Findings | Outcome |
|---|---|---|---|
| Grok | 8 | G0-1..G0-20 | 18 fixed. G0-18: mechanism refuted (JSON keeps `And`), coverage gap fixed. G0-20: refuted with an executable IEEE 754 proof, refutation accepted. All confirmed by Grok |
| Codex | 3 | C0-1..C0-4 | 4 fixed; Grok checked the fixes, Codex confirmed closure |

The order was respected: Grok → fixes → Codex → fixes → Grok again (Codex fixes changed reviewed contracts) → Codex. No unresolved disagreements.

CI evidence: GitHub Actions run 40642440205 on PR #3: all jobs passed, rustc 1.99.1 verified (`reports/reviews/phase-0/ci-evidence.txt`). The final state is re-verified on the PR.

### Blockers
None for Phase 0.

## Phase 1 — Milestone 1: Text baseline (`@m1`)

**Status:** ✅ APPROVED (2026-10-28, with owner waiver). Grok approved after 7 rounds (G1-1..G1-15, last round NO FINDINGS). Codex rounds 1–2 found C1-1..C1-10, all fixed; Codex round 2 closed C1-1..C1-7 and Grok round 6 confirmed C1-8..C1-10. Codex round 3 could not run (ChatGPT plan usage limit, 06:13). The owner waived it at 07:52 ("quando a quota do Codex acabar (caso atual), pode pular ele"); no Codex report was simulated, and C1-8..C1-10 carry Grok-only confirmation. See `reports/reviews/phase-1/FINDINGS.md` § Owner waiver.

### Decisions and PLAN changes, with justification
| Change | Justification |
|---|---|
| Text model: `sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2` @ `e8f8c211…` for the engine comparison; E5-base remains a model comparison on FastEmbed | EmbedAnything's candle path cannot load XLM-RoBERTa (E5). Its ONNX path pins `ort =2.0.0-rc.10` while FastEmbed pins `=2.0.0-rc.15`, and both link `onnxruntime`, so Cargo cannot build them together (source spike, PLAN §0.1) |
| Native-text rule switched to alphanumeric **character** share ≥ 50 % | The token-share rule misclassified pipe-separated lab tables (57 % word-like tokens; 74 % alphanumeric chars). A unit test caught it |
| `Pipeline<T, O, S>` instead of a `Components` bundle | Avoids premature abstraction; only the three replaced components are generic |
| Run modality: image inputs are `not_applicable` in text runs | Keeps text and image spaces separate. Images are a Milestone 3 run |
| BDD replays captured tool output (`FixtureExtractor`) | PRD §5.7: deterministic checks are independent of external tools; real tools are covered by integration tests |
| An existing revision directory is always reused, never replaced | A revision directory only appears through an atomic rename of fsync'd staging, so it is complete. It may be pinned by an older snapshot (source reverted), so replacing it would break provenance |

### Implementation
- `corpus`: manifest (`deny_unknown_fields`, `remote_allowed` defaults to false), eligibility, anonymization gate, checksum verification.
- `extraction`: page text/lines/boxes, native-vs-OCR rule, Tesseract TSV parser (word rows only, page confidence = mean `conf ≥ 0`, boxes in PDF points with a top-left origin), poppler/tesseract extractors (`kill_on_drop`, timeouts, `pdfinfo` cross-checked with lopdf), replay extractor; `examples/capture_extraction.rs`.
- `chunking`: `structure-aware-v1`. Section headings; each table row is one chunk with its section and header. Paragraphs carry their section and are split at line and word boundaries to fit the token budget. A row is truncated (and recorded) only when truncation is enabled; otherwise the source fails at the `chunking` stage.
- `record`: PRD §8 schema with full vector and provenance.
- `storage`: staging + fsync, revision rename, pointer rename as the commit point, fault injection at 3 points, retained revisions, create-only experiment snapshots, staging cleanup on open.
- `pipeline`: validation first (capability, remote permission, experiment id unused). Per source: manifest → extraction (unreadable pages fail the source) → revision (unchanged → skip) → chunking → batched embedding with class-based retries → validation → stage/publish. Run report and exit codes (2 incomplete, 3 validation).
- `retrieval`: space guard before embedding, exact cosine in f64, `total_cmp` + `record_id` ties, `Active` or `Experiment` scope.
- `adapters` (single feature-gated factory), `config` (saved experiment TOML), EmbedAnything adapter (pinned revision, HF-cache tokenizer, special tokens reserved, probe-checked dimensions, `spawn_blocking`), `anonymization` (filename patterns, PDF info/XMP keys, EXIF tags), CLI `manifest | anonymization-check | validate | ingest | search | doctor`.
- BDD: `scripts/sync-features.py` generates `tests/features/{us01,us02,rel01}.feature` verbatim from the PRD for milestone 1. The `LabWorld` gives each scenario its own store, corpus, extractor, adapter, and run history. Steps for 18 scenarios. Frozen ranking `tests/fixtures/rankings/hemoglobina.toml`, derived by hand from keyword axes.

### Acceptance criteria (PLAN Phase 1 exit)
| Criterion | Evidence |
|---|---|
| All `@m1` scenarios pass | `bdd inventory: milestone 1, 18 required, 21 executed` (18 required + 3 harness), 21 scenarios / 133 steps passed |
| Real run produces `reports/examples/record-001.json` | Produced from a real EmbedAnything run on the **synthetic** corpus (owner corpus not supplied) |

### Validation (local, 2026-10-28)
- fmt ok; clippy `-D warnings` for none, each engine, and all features: ok.
- `cargo test`: 69 unit, 2 pipeline, 2 fixtures, BDD (21 scenarios), self-test 0 mismatches.
- `RUN_INTEGRATION=1 cargo test --features engine-embed-anything --test integration -- --include-ignored`: 4 passed (tool probe; real native/OCR selection on the mixed PDF; real OCR keeps the hemoglobin row with a box in points; pinned MiniLM loads, 384-d normalized vectors, ranking sanity, budget 126).
- CLI end to end with real tools on the synthetic corpus: 10/10/8 text records (matching the hand-derived counts); images `not_applicable`; second run `unchanged` with 0 attempts.
- BDD mutation checks (each must fail a scenario):
  - removing the remote check fails 1 scenario;
  - no retries fails 2;
  - retrying permanent errors fails 1;
  - no vector validation fails 3;
  - ignoring unreadable pages fails 1;
  - ignoring the pointer fault fails 1.

  The idempotency-skip mutant is equivalent at the PRD-observable level, because the store reuses identical revisions. It is now killed by `tests/pipeline.rs`.

### rust-skills applied
`async-spawn-blocking` (store I/O, model load, inference), `async-fn-in-trait` with `+ Send` futures (extractors, sleeper), `trait-object-safety` (dyn strategy only), `num-float-compare` (`total_cmp`), `num-cast-try-from`, `serde-deny-unknown-fields`, `type-newtype-ids`, `type-enum-states`, `err-thiserror-lib`/`err-anyhow-app`, `obs-structured-fields` (ids/stages only), `anti-over-abstraction` (Components bundle dropped). Lint suppressions use `#[expect(..., reason)]` for bounded casts only.

### Outstanding (not claimed)
- Owner corpus, relevance labels, and fidelity sample: not supplied; evidence is synthetic.

## Phase 2 — Milestone 2: Engine comparison (`@m2`)

**Status:** ✅ APPROVED (2026-10-28). Implemented and merged to `main` as PR #5 at the owner's explicit request ("pare e mescle os PRs") **before** its adversarial reviews; the review fixes are on branch `phase-2/review-fixes` (from `main` @ `44cf64a`; review diff base `3cdb355`, merge of PR #4). Grok rounds 1–9 and Codex rounds 1–6 are complete; at `e0b873e` the final Grok round (9) and the final Codex round (6) both reported NO FINDINGS. 30 findings (G2-1..G2-9, C2-1..C2-21), all fixed with regression tests; see `reports/reviews/phase-2/FINDINGS.md`.

**Intermittent `WriterBusy` — fixed (commit `ff4fde9`).** Cause confirmed by reproduction: `flock` belongs to the open file description, so a child spawned concurrently by any thread holds a copy of `.writer.lock` until its `exec` and keeps the lock alive after the writer drops it. A stress loop reproduced it (26 of 4000 opens). Fix: a POSIX record lock (`fcntl F_SETLK`, per-process, never inherited) plus an in-process registry of held roots, claimed before the lock file is opened (record locks do not exclude writers within one process, and closing any descriptor of the file would drop the lock). Regression tests: `writer_lock_is_released_on_drop_while_children_are_spawned` (time-based, 4 spawner threads; against the `main` code it fails 5/5 with 318–426 `WriterBusy`, against the fix it passes 5/5) and `writer_lock_excludes_another_process` (a re-executed lock-holder child; the parent must get `WriterBusy`). Full unit suite: 0 failures in 15 consecutive runs (previously about 1 in 10).

### Decisions and PLAN changes, with justification
| Change | Justification |
|---|---|
| `embedding/local.rs`: one shared contract core (`LocalTextAdapter<B: TextBackend>`) for EmbedAnything and FastEmbed | Limits, prefix (applied once), blocking pool, and normalization live in one place and are unit-tested with a recording backend, without compiling an engine (PLAN Phase 2 step 1 "exact string sent to the backend") |
| FastEmbed loads pinned files through `hf-hub` 0.5 + `try_new_from_user_defined` | The FastEmbed catalogue loader downloads `main`, which cannot be pinned. Downloads are serialized in-process and retry hf-hub's 5 s lock timeout (a real parallel-download failure was observed in the integration tests) |
| Chunking fingerprint uses the tokenizer's content id (`hf-tokenizer:sha256:…`, `tiktoken:cl100k_base`, `whitespace`) instead of the model id | Chunk boundaries depend on the tokenizer and budget, not on the model name. Consequence: the Xenova MiniLM `tokenizer.json` differs byte-wise from the sentence-transformers one, so the two MiniLM runs have different chunking fingerprints even though the chunks are identical |
| RoBERTa-family position limit = `max_position_embeddings − pad_token_id − 1` | E5-base (XLM-R) declares 514 positions but accepts 512 tokens |
| OpenAI pin = model snapshot name; every response's `model` must equal it | The API has no commit hashes |
| Retry classes for OpenAI: 408, 429, 5xx, timeouts, connection errors are transient | PLAN §0.1 |
| `adapters::build_contract_test`: deterministic stand-in under the engine's identity | PRD §5.4 contract tests run the real orchestration without models or providers |
| Contract runs share `max_input_tokens = 128` | The scenario fixes the pipeline configuration; the chunking budget must not vary with the stand-in |
| Cold start = fresh query-side adapter load + the first frozen query (the discarded warm-up) | PRD §9.4 definition; ingestion model load is reported separately |
| Labels key on `chunk_id` (space-independent); image items are labeled now | Labels stay frozen when image retrieval is evaluated (Milestone 3). Chunk ids `chest-001-p1-image-0`, `abd-us-001-p1-image-0`, and `us-mixed-003-p1-image-0` are therefore a commitment for Phase 3 |

### Implementation
- Adapters: FastEmbed (`engine-fastembed`, MiniLM ONNX and E5-base configs), OpenAI (`engine-openai`, reqwest, `cl100k_base` budget, key read only after the remote check, `.env.example`), shared `hf.rs`.
- Pipeline: `IngestSettings.modality`; image runs check text-to-image and image capabilities before anything else (no extraction, adapter call, or store write).
- Retrieval: `search_vector` with the space guard before any read; dimension check.
- `evaluation`: frozen `queries-v1` (10 queries) and `labels-synthetic-v1`; P@3/Hit@3/nDCG@3 per PRD §5.3; checksums; evaluation fingerprint; unpadded results with notes.
- `measurement`: `Clock`/`ManualClock`, `MemoryProbe`/`RusageProbe` (`nix`), nearest-rank stats, `MeasuredEmbedder` decorator, hardware detection. `cost`: versioned price tables; null with a reason when usage or price is missing. `fidelity`: field-level accuracy per extraction method. Store: index size of the active revisions.
- `experiment::run` and `report` (JSON + Markdown, "end-to-end configuration comparison", refuses non-comparable inputs). CLI: `experiment`, `report`; `search` accepts `--manifest` for a remote engine's permission check.
- BDD milestone 2: `us03` (text-only + low relevance), `us04` (3 engines, image rejection, separate spaces), `us05` (3 scenarios). Fixtures: keyword-vector index (`tests/fixtures/evaluation/keywords.toml`), hand-derived ranks, metrics precomputed by `scripts/precompute-metrics.py`, controlled measurements (`tests/fixtures/measurements/expected.toml`), test price table.

### Acceptance criteria (PLAN Phase 2 exit)
| Criterion | Evidence |
|---|---|
| All `@m1` + `@m2` pass | `bdd inventory: milestone 2, 28 required, 31 executed`; 31 scenarios / 191 steps passed |
| Three text configurations evaluated | `reports/phase-2/comparison.md`: EmbedAnything MiniLM, FastEmbed MiniLM, FastEmbed E5-base, real runs on the **synthetic** corpus |
| Integration report exists, or remote marked outstanding | 9 real integration tests pass (local); OpenAI live run **outstanding** |

### Validation (local, 2026-10-28)
- fmt ok; clippy `-D warnings` for none, each engine, and all features: ok.
- `cargo test`: 101 unit, 8 pipeline, 2 fixtures, BDD 31 scenarios, self-test 0 mismatches. **Intermittent failure observed (not fixed):** `storage::tests::abandoned_staging_is_removed_on_open_and_never_visible` failed with `WriterBusy` on reopen in 2 of about 12 full `cargo test --lib` runs. It never failed alone (6 of 6 passed). Unconfirmed hypothesis: a child process spawned concurrently by another test inherits the `.writer.lock` descriptor, so the `flock` outlives the dropped store. A real writer could hit the same race. `cargo test --features engine-openai --lib embedding::openai`: 5 passed (local HTTP server).
- `RUN_INTEGRATION=1 cargo test --features engine-embed-anything,engine-fastembed --test integration -- --include-ignored`: 9 passed, including FastEmbed MiniLM and E5 loads, cross-engine MiniLM cosine > 0.99, and the E5 prefix budget.
- BDD mutation checks (each fails at least one scenario): linear nDCG gain; relevance threshold 2; no text-to-image check; no space guard in `search_vector`; unscaled price; provider memory not reported; cold first query dropped; chunking fingerprint keyed by model (3 fail); token usage off by one (2 fail).
- Real runs (release build, Apple M3 Pro, CPU): macro P@3 / Hit@3 / nDCG@3 = 0.333 / 0.500 / 0.409 (both MiniLM engines, identical rankings) and 0.433 / 0.700 / 0.540 (E5-base). Regenerated on 2026-10-28 after the Grok and Codex fixes (labels v2 with anchors, per-run hardware, fidelity over extraction output, cold start = first load + first batch of the fresh process): identical retrieval metrics and fidelity. **Hardware note:** the original Phase 2 figures were produced on another machine (Apple M3 Pro, 12 CPUs); the regenerated evidence comes from an Apple M3 (8 CPUs, 16 GiB), which each result now records (C2-3). **Cold-start note:** two consecutive regenerations on this machine gave 12.5 / 26.7 / 56.1 s and then 1.6 / 4.9 / 10.5 s (EmbedAnything MiniLM / FastEmbed MiniLM / FastEmbed E5-base): cold start depends on the OS file cache of the model files, so it is reported as a single indicative run, never compared statistically. Fidelity on the synthetic sample: native 1.000, OCR 0.688 (en dashes read as hyphens, `/mm³` read as `/mms` and `/mm?`, missing accents).

### rust-skills applied
`trait-object-safety` (only `EmbeddingStrategy` is dyn; the `MeasuredEmbedder` decorator implements it), `trait-dyn-vs-generic` (`LocalTextAdapter<B>` static over the backend), `async-spawn-blocking` (inference, downloads, store reads), `async-async-fn-bounds` (`experiment::run` takes `impl AsyncFn`), `num-float-compare`, `num-cast-try-from`, `serde-deny-unknown-fields`, `type-newtype-ids` (`QueryId`), `err-thiserror-lib`, `api-must-use`. Lint suppressions use `#[expect(..., reason)]`.

### Outstanding (not claimed)
- ~~Intermittent `WriterBusy` on store reopen~~: fixed, see the status paragraph above.
- ~~Reviews~~: complete, Phase 2 approved (see Status).
- Integration after the fixes: `RUN_INTEGRATION=1 cargo test --features engine-embed-anything,engine-fastembed --test integration -- --include-ignored`: 10 passed after the Codex round-2 fixes, including the new CLI exit-2 test for C2-10 (2026-10-28). The OpenAI live test is not compiled without `engine-openai` and stays outstanding.
- OpenAI live integration: no `OPENAI_API_KEY` and no owner authorization for remote processing.
- Owner corpus, relevance labels, and fidelity sample; all evidence is synthetic.
- The price in `config/prices/openai-embeddings-2026-10.toml` must be verified against the published price before an OpenAI estimate is reported.

## Phase 3 — Milestone 3: Image retrieval (`@m3`)

**Status:** ✅ APPROVED (2026-10-29). Branch `phase-3/image-retrieval` (from `3294dbd`, the approved Phase 2 head, merged to `main` with PR #6). Review diff base `3294dbd`. Grok rounds 1–7 and Codex rounds 1–4 are complete; the final Grok round (7, `af10c26`) and the final Codex round (4, `9196029`) both reported NO FINDINGS. 13 findings (S3-1, G3-1..G3-5, C3-1..C3-7): G3-4 refuted with evidence and hardened, the rest fixed with regression tests; see `reports/reviews/phase-3/FINDINGS.md`.

### Decisions and PLAN changes, with justification
| Decision | Justification |
|---|---|
| Both CLIP ViT-B/32 (`openai/clip-vit-base-patch32` @ `3d74acf9…`, 512-d) and SigLIP base-patch16-256-multilingual (`google/siglip-base-patch16-256-multilingual` @ `8952a4ea…`, 768-d) are evaluated | Spike (`reports/spikes/phase3-models.md`): both load through EmbedAnything 0.7.1's Rust API, so the multilingual candidate qualifies under PLAN §0.1 |
| The adapter uses only the engine's weights (`ClipEmbedder::new` + `get_text_features`/`get_image_features`); preprocessing, tokenization, padding, and normalization are the adapter's own (`embedding::preprocess`, `embedding::vision`) | EmbedAnything 0.7.1 hardcodes CLIP-224 sizes with SigLIP normalization for both models and panics on over-limit text and undecodable images (spike defects 1–2). Preprocessing follows each model's pinned `preprocessor_config.json` and enters the space descriptor |
| `[engine] model_kind = "text" \| "text_to_image"` (default text) and `[ingest] modality = "text" \| "image"` (default text) | One modality per run (PLAN §0.1); engine and model stay separate choices. Only `embed_anything` has a cross-modal adapter; `fastembed`/`openai` with `text_to_image` are refused (exit 3) |
| Image runs: `pdfimages -list` rows of type `image` with both sides ≥ 64 px; images on pages that need OCR are page scans and never become image records; a PDF without a qualifying image is `not_applicable` | Keeps logos, masks, and scans out of the image index (PLAN §0.1 "Embedded images") |
| Assets are decoded, masked by their PDF soft or explicit mask (the `smask`/`mask` row `pdfimages -list` gives right after the image; its gray value is the opacity, scaled to the image size), turned upright by their EXIF orientation, composited over white when they have alpha, and re-encoded as RGB8 PNG (`MetadataScrubber`, id `decode-mask-orient-flatten-white-rgb8-png-v3`), stored content-addressed at `store/assets/<sha256>.png` | PRD §8: no EXIF/XMP/IPTC/ICC/text chunk survives by construction; masks, orientation, and transparency change what an image shows, so they are applied before being dropped (C3-2, C3-3, C3-6) |
| Labels v4 (`config/evaluation/labels-synthetic-v4.toml`): v2 judgments plus, for image labels, an anchor naming the original (`file` or `file#page=N&image=num`) and the SHA-256 of the stored asset (`asset_checksum`) | An image label must judge the same original and the same pixels after re-extraction (C3-5); v2 stays for the Phase 2 evidence |
| Image embedding calls are measured as the indexed (document) side | They are the ingestion batches of an image run; the measuring decorator passed them through unmeasured before |
| The comparison report gains a `Mode` column (text-only / image-only) | Results of different retrieval modes must not be read as one ranking |
| Image experiments are run without `--fidelity` | Fidelity measures text extraction, which an image run does not use |

### Implementation
- `extraction::images` (`ImageInfo`, `ImageFilter`, `parse_list`, `ImageExtractor`), `PopplerImageExtractor` (`pdfimages -list`, `pdfimages -png -p -f N -l N`), fixture replay (`images.list`, `img-p<page>-<pos>.png`), `capture_extraction --images`.
- `assets` (`MetadataScrubber`), store `put_asset` / `asset_path` / `read_asset` (atomic write, strict 64-hex URI validation, integrity check, abandoned temp cleanup).
- Pipeline image runs: image-run extraction and preparation fingerprints, `ExtractionMethod::{EmbeddedImage, ImageFile}`, `ChunkKind::Image`, `RecordMetadata.original_asset` and `generated_caption` (never embedded), `chunk_text = null`.
- `embedding::preprocess` (HF `CLIPImageProcessor`/`SiglipImageProcessor` semantics: shortest-edge or exact resize, bicubic Catmull-Rom or bilinear, center crop, rescale, mean/std; checked against the exact pinned configs), `embedding::vision` (`VisionAdapter`: text queries + images, no document text, over-limit queries refused with `InputTooLong`, inference on the blocking pool with a panic mapped to a permanent error, probe-checked dimensions), shared `hf::download`.
- Retrieval: `search_images` (requires text-to-image capability; filters image records). Evaluation: `check_image_labels`, `evaluate_images` (image-only). `experiment::run` evaluates image-only after an image run. CLI: `search --mode text|image` (a model without the capability is a validation refusal, exit 3).
- Configs: `image-clip-vit-b32.toml`, `image-siglip-multilingual.toml`, `image-deterministic.toml`.
- BDD milestone 3: US-01 mixed PDF/PNG/JPEG, US-02 image query, US-03 image-only example. Fixtures: `image-keywords.toml`, hand-derived `image-only.ranks.toml`, precomputed `image-only.toml`.

### Acceptance criteria (PLAN Phase 3 exit)
| Criterion | Evidence |
|---|---|
| All `@m1`–`@m3` pass | `bdd inventory: milestone 3, 33 required, 36 executed`; 36 scenarios / 226 steps passed; self-test 12 cases, 0 mismatches |
| Image-only report for ≥ 1 model | `reports/phase-3/comparison.md` (+ JSON): CLIP ViT-B/32 and SigLIP-multilingual, real runs on the **synthetic** corpus |

### Validation (local, 2026-10-28)
- fmt ok; clippy `-D warnings` for none, each engine, and all features: ok.
- `cargo test`: 149 unit (incl. the C3-2/C3-3/C3-6 scrubber, C3-4 scratch-cleanup and both cancellation windows, mask association, and C3-5 label regressions), 15 pipeline (incl. the G3-1, G3-5, C3-1, and C3-6 regressions), 7 experiment (new: an image run is evaluated image-only through the text-query path, metrics equal the hand-derived fixture), 3 evidence, 2 fixtures, BDD 36 scenarios, self-test 0 mismatches.
- `RUN_INTEGRATION=1 cargo test --release --features engine-embed-anything,engine-fastembed --test integration -- --include-ignored`: 15 passed, including the masked-PDF checks (the installed `pdfimages` gives the captured `tests/fixtures/pdf-masks` list and pixels, C3-6; a real recapture of that PDF replays with its masks, C3-7), the new real image run (`pdfimages`: embedded page-1 image kept, page-2 scan dropped, stored asset equals the scrubbed extraction) and CLIP / SigLIP loads (dimensions, normalization, batch = single, over-limit query refused, document text refused). The OpenAI live test stays outstanding.
- Real runs (release, Apple M3, 8 CPUs, CPU inference): macro P@3 / Hit@3 / nDCG@3 = 0.167 / 0.300 / 0.241 (CLIP) and 0.167 / 0.300 / 0.270 (SigLIP). Identical metrics in four runs, the last after the Codex round 2 fixes (scrubber v3, labels v4). SigLIP ranks the chest image first for q07 ("Mostre a radiografia de tórax presente no conjunto de exames."); CLIP ranks it third. **Load note:** the last run had moderate unrelated load (load average about 6 on 8 logical CPUs): warm median 35 ms (CLIP) and 62 ms (SigLIP). The earlier runs carried heavy load (load average 25–39, other desktop applications), with warm medians of 0.78–1.8 s (CLIP) and 1.9–3.3 s (SigLIP). Latencies are therefore indicative only and not comparable with the Phase 2 text runs.
- Spike re-run with the corrected preprocessing: recorded in `reports/spikes/phase3-models.md` next to the original numbers.

### rust-skills applied
`async-spawn-blocking` (model load, downloads, inference, image re-encoding), `err-result-over-panic` (engine panics become `JoinError` → permanent error; over-limit text refused before the engine), `api-parse-dont-validate` (`ImagePreprocessing::from_json`, asset URI validation), `type-enum-states` (`ModelKind`, `Modality`, `SearchMode`), `serde-default-compat` / `serde-skip-empty` (new config and metadata fields keep old files and records valid), `num-cast-try-from`, `num-float-compare`. Lint suppressions use `#[expect(..., reason)]`.

### Outstanding (not claimed)
- ~~Adversarial reviews~~: complete, Phase 3 approved (see Status).
- Owner corpus, relevance labels, and fidelity sample; all evidence is synthetic.
- Image latency on an idle host (the last run had a load average of about 6 on 8 CPUs).
- PDF soft masks with `/Matte` or a non-default `/Decode` (not visible in `pdfimages` output; C3-6 residual).

## Phase 4 — Milestone 4: Combined retrieval & publication (`@m4`)

**Status:** 🔄 IMPLEMENTED AND VALIDATED; adversarial reviews in progress (Grok → Codex → Antigravity). Branch `phase-4/combined-retrieval` (from `e1abe6f`, `main` after PR #8 merged Phase 3). Review diff base `e1abe6f`.

### Decisions and PLAN changes, with justification
| Decision | Justification |
|---|---|
| `fusion::rrf` over item keys: score = Σ 1/(k + rank), ranks from 1, each list's top `depth` (default `"all"`), a repeated item counts once at its best rank, contributions summed in ascending rank order, ties broken by ascending key (`record_id`) | PRD §5.3/§6.6. Summing in rank order makes two items with the same ranks in different lists bit-identical, so the tie-break, not float rounding, decides them |
| `FusionParams { method = "rrf", k = 60, depth = "all" \| n }` is a versioned config value. It is recorded in every combined evaluation (`EvaluationReport.fusion`) and combined result, and fingerprinted in the combined snapshot. It is not part of the evaluation fingerprint | PRD §6.6 requires depth to be recorded. The evaluation fingerprint covers the frozen dataset and metrics, which must stay equal across the three modes for them to be comparable |
| Combined search runs each leg in its own space with its own query model (`Leg { space, embedder }`). Both spaces and the image leg's text-to-image capability are checked before any store read or query embedding. The legs run concurrently (`tokio::try_join!`) | PRD §6.6: each leg's query vector must match its own index space; unsupported combinations are refused before processing |
| A combined result keeps its fused rank and score (`score` as `f32`, the exact `fusion.fused_score` as `f64`) and its rank and cosine score in each leg (`fusion.text`, `fusion.image`) | Traceability: a fused ranking must show where each item came from, without comparing raw scores |
| A combined configuration file (`config/experiments/combined-*.toml`) names a text configuration and an image configuration plus `[fusion]`. Loading checks the roles: the text leg must be a text model over a text run, the image leg a cross-modal model over an image run (exit 3 otherwise) | PRD §6.6: a combined configuration is an ordered pair of spaces plus fusion parameters |
| `combine` ingests nothing. It loads the snapshots of a text run and an image run, refuses a pair that indexed another corpus or was evaluated with other frozen queries or labels, and refuses a leg whose snapshot pins nothing in its model's space. It then saves a create-only snapshot pinning both legs' revisions and measures fused retrieval over it | Reuses the evidence of the single-mode runs instead of re-ingesting, and the combined id alone reproduces the run (`search --mode combined --experiment <id>`) |
| Combined cold start = both model loads + the first fused query in a fresh process; warm latency covers both legs and the fusion; index size is the sum of both legs' active indexes; query cost is reported per leg | PRD §9.4 definitions applied to a two-model configuration; costs of different providers are not summed into one number |
| `ComparisonReport::with_combined` adds combined results to a report. It refuses another corpus, query set, labels, evaluation fingerprint, or machine, and refuses a leg experiment the report shows in another space. The extraction-fingerprint rule now compares only configurations in the same mode | A text run and an image run extract different things, so their extraction fingerprints differ by design. The old rule refused every three-mode report |
| BDD combined example: both fixture indexes in one store. The hand-derived `combined.ranks.toml` pins the record id at each rank, because every fused tie joins a text and an image result and the record-id tie-break decides it | The labels by rank depend on hashes, which are inputs of the derivation, not outputs of the code under test. Pinning them turns a fingerprint change into a named failure instead of a silent re-derivation |

### Implementation
- `src/fusion.rs`: `rrf`, `FusionParams`, `FusionMethod`, `FusionDepth` (`"all"` or a positive count), unit tests and proptests:
  - overlapping items accumulate;
  - descending score with key tie-break;
  - scaling raw scores leaves the fusion unchanged;
  - list order does not matter.
- `src/retrieval.rs`: `FusionTrace`, `LegTrace`, `SearchHit.fusion`, `fuse`, `Leg`, `search_combined`.
- `src/evaluation.rs`: `evaluate_combined` (checks text labels against the text index and image labels against the image index); `ResultEntry.fusion`; `EvaluationReport.fusion`.
- `src/experiment.rs`: `run_combined`, `CombinedInputs`, `ExperimentError::Combination`.
- `src/report.rs`:
  - result types: `CombinedResult`, `LegConfiguration`, `CombinedStart`, `CombinedMeasurements`, `CombinedCosts`;
  - `with_combined`;
  - Markdown sections: "Combined configurations", "Retrieval modes compared", "Combined retrieval against its legs", "Combined efficiency", and per-query fused top 3 with leg ranks.
- `src/config.rs`: `CombinedConfigFile`, `CombinedConfig::load` with role checks.
- `src/cli.rs`: `combine`, `search --mode combined`, `report --combined`. Remote permission is checked for every remote leg.
- Configs: `combined-e5-base-siglip.toml`, `combined-minilm-clip.toml`, `combined-deterministic.toml`.
- BDD milestone 4: `us03` Combined example and "Fuse rankings without comparing incompatible raw scores". Fixtures: `combined.ranks.toml` (hand-derived, with record ids) and `combined.toml` (`scripts/precompute-metrics.py`).
- Tests: `tests/combined.rs` (4) covers:
  - a combined run pins both legs and matches the hand-derived metrics;
  - the refusals: swapped legs, another corpus, other labels, a missing leg, a reused id;
  - search guards before any read or embedding;
  - the three-mode report and its refusals.

  The integration test `the_cli_runs_combined_retrieval_over_real_text_and_image_runs` runs real MiniLM + CLIP through the CLI.
- Documentation: `reports/README.md` (reproduction), `reports/ACCEPTANCE.md` (PRD §12/§13 → evidence), `README.md` (results, status), `CLAUDE.md` (current state, commands, review rule).

### Acceptance criteria (PLAN Phase 4 exit)
| Criterion | Evidence |
|---|---|
| All `@m1`–`@m4` pass | `bdd inventory: milestone 4, 35 required, 38 executed`; 38 scenarios / 241 steps passed |
| `fusion::rrf` with the specified score, order, tie-break, and proptests | `fusion::tests` (9); BDD "Fuse rankings without comparing incompatible raw scores" |
| `retrieval::combined` | `search_combined`; `tests/combined.rs`; real CLI integration |
| Final comparison report, reproduction, saved configs, limitations | [`reports/phase-4/comparison.md`](../phase-4/comparison.md); [`reports/README.md`](../README.md); `config/experiments/`; the limitations section of the report |
| PRD §12 deliverables and §13 criteria walked through with evidence links | [`reports/ACCEPTANCE.md`](../ACCEPTANCE.md) |

### Validation (local, 2026-10-29)
- fmt ok; clippy `-D warnings` for none, each engine, and all features: ok.
- `cargo test`: 160 unit (incl. 9 fusion and 2 combined-config tests), 15 pipeline, 7 experiment, 4 combined, 3 evidence, 2 fixtures, BDD milestone 4 (38 scenarios), self-test 0 mismatches.
- Mutants caught:
  - reversing the RRF key tie-break fails 3 fusion unit tests, the BDD combined example and fusion scenario, and the combined run test;
  - dropping the combined corpus check fails `a_combined_run_refuses_legs_that_cannot_be_paired`.
- `RUN_INTEGRATION=1 cargo test --release --features engine-embed-anything,engine-fastembed --test integration -- --include-ignored`: **16 passed**, including the new real combined CLI chain. The OpenAI live test stays outstanding.
- Real runs, `reports/phase-4/` (release, Apple M3, 8 CPUs, CPU inference, load average 3–5).
  - Text and image metrics are identical to Phases 2 and 3:
    - MiniLM (both engines): 0.333 / 0.500 / 0.409;
    - E5-base: 0.433 / 0.700 / 0.540;
    - CLIP: 0.167 / 0.300 / 0.241;
    - SigLIP: 0.167 / 0.300 / 0.270.
  - Combined:
    - E5-base + SigLIP: 0.367 / **0.800** / 0.508, warm median 91 ms;
    - MiniLM + CLIP: 0.300 / 0.600 / 0.388, warm median 41 ms.
  - Fusion raises Hit@3 above every leg. It lowers P@3 and nDCG@3 below the text leg, because irrelevant images displace relevant text chunks at ranks 2–3 on the laboratory queries; the report's limitations explain this per query.

### rust-skills applied
- `num-float-compare`: `total_cmp`, and float ordering in tests through `total_cmp`.
- `num-cast-try-from`: `#[expect]` with reasons on the bounded casts.
- `async-join-parallel`/`async-try-join`: the two legs run concurrently.
- `async-spawn-blocking`: snapshot load and save, and index size, on the blocking pool.
- `type-enum-states`: `FusionMethod`, `FusionDepth`, `SearchMode::Combined`.
- `serde-try-from-validate`: `FusionDepth` from `"all"` or a positive count.
- `serde-deny-unknown-fields`: fusion and combined configs.
- `serde-default-compat` / `serde-skip-empty`: new optional report fields keep the Phase 2/3 JSON readable.
- `api-parse-dont-validate`: `CombinedConfig::load` checks roles at the boundary.
- `test-proptest-properties`.

PLAN §2.3 deviations: none.

### Outstanding (not claimed)
- Adversarial reviews (Grok → Codex → Antigravity).
- Owner corpus, relevance labels, and fidelity sample; all evidence is synthetic.
- Live OpenAI run (key and `remote_allowed`).
- Latency on an idle host.

