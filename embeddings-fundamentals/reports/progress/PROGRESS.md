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

**Status:** ✅ APPROVED (2026-10-29). Grok rounds 1–7, Codex rounds 1–3, and Antigravity rounds 1–2 (with the `rust-dev` plugin) are complete. The final round of each reviewer (Grok 7 at `d0d99e8`, Codex 3 at `6d42294`, Antigravity 2 at `1349cc5`) reported NO FINDINGS. 14 findings (S4-1, S4-2, G4-1..G4-6, C4-1..C4-5, A4-1): G4-5 moved to `anonimizador` with the verifier and fixed there, the rest fixed here with regression tests; see `reports/reviews/phase-4/FINDINGS.md`. The per-phase review changelog is `REVISION.md`. Branch `phase-4/combined-retrieval` (from `e1abe6f`, `main` after PR #8 merged Phase 3). Review diff base `e1abe6f`.

### Decisions and PLAN changes, with justification
| Decision | Justification |
|---|---|
| `fusion::rrf` over item keys: score = Σ 1/(k + rank), ranks from 1, each list's top `depth` (default `"all"`), a repeated item counts once at its best rank, contributions summed in ascending rank order, ties broken by ascending key (`record_id`) | PRD §5.3/§6.6. Summing in rank order makes two items with the same ranks in different lists bit-identical, so the tie-break, not float rounding, decides them |
| `FusionParams { method = "rrf", k = 60, depth = "all" \| n }` is a versioned config value. It is recorded in every combined evaluation (`EvaluationReport.fusion`) and combined result, and fingerprinted in the combined snapshot. It is not part of the evaluation fingerprint | PRD §6.6 requires depth to be recorded. The evaluation fingerprint covers the frozen dataset and metrics, which must stay equal across the three modes for them to be comparable |
| Combined search runs each leg in its own space with its own query model (`Leg { space, embedder }`). Both spaces and the image leg's text-to-image capability are checked before any store read or query embedding. The legs run concurrently (`tokio::try_join!`) | PRD §6.6: each leg's query vector must match its own index space; unsupported combinations are refused before processing |
| A combined result keeps its fused rank and score (`score` as `f32`, the exact `fusion.fused_score` as `f64`) and its rank and cosine score in each leg (`fusion.text`, `fusion.image`) | Traceability: a fused ranking must show where each item came from, without comparing raw scores |
| A combined configuration file (`config/experiments/combined-*.toml`) names a text configuration and an image configuration plus `[fusion]`. Loading checks the roles: the text leg must be a text model over a text run, the image leg a cross-modal model over an image run (exit 3 otherwise) | PRD §6.6: a combined configuration is an ordered pair of spaces plus fusion parameters |
| `combine` ingests nothing. It loads the snapshots of a text run and an image run, refuses a pair that indexed another corpus or was evaluated with other frozen queries or labels, and refuses a leg whose snapshot pins nothing in its model's space. It then saves a create-only snapshot pinning both legs' revisions and measures fused retrieval over it | Reuses the evidence of the single-mode runs instead of re-ingesting, and the combined id alone reproduces the run (`search --mode combined --experiment <id>`) |
| Combined cold start = both model loads + the first fused query in a fresh process; warm latency covers both legs and the fusion; index size is the size of the revisions the combined snapshot pins (Grok G4-3); query cost is reported per leg | PRD §9.4 definitions applied to a two-model configuration; costs of different providers are not summed into one number |
| `ComparisonReport::with_combined` adds combined results to a report. It refuses another corpus, query set, labels, evaluation fingerprint, or machine, and refuses a leg experiment the report shows in another space. The extraction-fingerprint rule now compares only configurations in the same mode | A text run and an image run extract different things, so their extraction fingerprints differ by design. The old rule refused every three-mode report |
| A combined search scoped to an experiment refuses a snapshot that is not the combined run of its legs and fusion parameters: fusion fingerprint and a pinned revision in each leg's space (`check_combined_snapshot`, exit 3). The CLI reads the snapshot and checks the fusion fingerprint before loading any model (Grok G4-1, G4-2) | The combined id must reproduce the run it names; a different `k` or a missing leg would otherwise change or silently shrink the fused ranking |
| Owner's real sample exams (`tests/sample-exams/`, git-ignored, added 2026-10-29) enter only local, opt-in, property-based tests (`tests/sample_exams.rs`, `RUN_SAMPLE_EXAMS=1`). Messages name a document by index, never by file name, and nothing derived from them is committed or sent to a reviewer; reviewers run from a clean worktree of the committed branch (`CLAUDE.md`) | PRD §11: real documents without an anonymization review must not be indexed, reported, or shared. The tests check the pipeline's behavior on real layouts without persisting anything |
| `anonymization::filename_findings` also flags upper-case word sequences (with one-letter particles) and every free-form file name (anything beyond lower-case ASCII letters, digits, `-`, `_`, `.`) | Found by the real exams: none of the 12 real file names, mostly upper-case personal names, was flagged. A finding only asks the reviewer to look, so a conservative rule is safe. The blocking gate is unchanged: without a recorded review a source is refused |
| Chunking rules `structure-aware-v1/r2` (`chunking::RULES_ID`, part of the chunking fingerprint): a table header that no row uses (a multi-column line without digits followed by text, a heading, a blank line, another header, or the end of the page) is kept as a paragraph | Found by the real exams: 24 such lines on 9 of the 12 documents were silently dropped from every chunk. The rules id enters the fingerprint, so revisions chunked by the old rules are not reused as "unchanged". The synthetic corpus has no such line: its chunks are unchanged, but every text revision id (and so every text record id) changed |
| `pipeline::extract_pdf`: the per-page native/OCR selection as a public function, used by ingestion and by the real-exam tests | The tests exercise the same selection as ingestion without a manifest, and without bypassing the gate through `Pipeline::extract_source` |
| Owner-requested corpus tools (2026-10-29). `draft-manifest` measures a corpus directory: checksums, pages with a usable text layer (same rule as ingestion), and kept images. It guesses `input_type`, writes no anonymization review, refuses flagged file names because the manifest is tracked, and never uses a flagged name as a `source_id`. `draft-labels` pools the top `--depth` results of every run for every frozen query, with anchors, asset checksums, and `label = "TODO"`, which does not parse until it is judged. `scripts/anonymization-verify.py` counts the owner's terms in the name, metadata, text layer, OCR, embedded images, and raw bytes, and prints counts only. `TESTS.md` documents anonymization, building the corpus, and what an automatic anonymizer would need | The owner corpus is the main outstanding input. The drafts remove the mechanical work (checksums, ids, anchors, asset checksums) without ever recording a review or a judgment. The verifier is a separate Python script that checks independently of the pipeline code. It catches the classic failed redaction, a box drawn over text that stays in the file |
| `extract_pdf` reports each page through an `on_page` callback as it is extracted, and its error names the failing page (Grok G4-4) | A failed source must still show how far extraction got. The refactor in `b14c7d1` had moved page reports after full success |
| Codex round 1 (C4-1..C4-5). The snapshot records the run status, and `combine` refuses an incomplete or unrecorded leg. The remote permission check is bound to the corpus actually searched: every searched snapshot must have indexed `--manifest`, and an active scope is refused for a remote engine. Results and combined legs record `revisions_fingerprint`, and a report pairs a leg with its reported run only over the same revisions. Draft errors name no path. The README review status is accurate | PLAN §4.3 (incomplete runs never look complete), PRD §11 (per-corpus remote permission), traceability of "combined against its legs", and no identifying data in messages |
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
- Tests: `tests/combined.rs` (7) covers:
  - a combined run pins both legs and matches the hand-derived metrics;
  - the refusals: swapped legs, another corpus, other labels, a missing leg, a reused id;
  - search guards before any read or embedding;
  - the three-mode report and its refusals;
  - a combined search scoped to an experiment must be that combined run (G4-1);
  - the snapshot is read before any model loads (G4-2);
  - the index size is that of the pinned revisions (G4-3).

  The integration test `the_cli_runs_combined_retrieval_over_real_text_and_image_runs` runs real MiniLM + CLIP through the CLI.
- Documentation: `reports/README.md` (reproduction), `reports/ACCEPTANCE.md` (PRD §12/§13 → evidence), `README.md` (results, status), `CLAUDE.md` (current state, commands, review rule), `TESTS.md` (anonymization, owner corpus, automatic anonymizer design).
- Corpus tools (owner request):
  - `src/drafts.rs` with the `draft-manifest` and `draft-labels` CLI commands;
  - `scripts/anonymization-verify.py` and `scripts/make-covered-name-pdf.py`, which writes `tests/fixtures/anonymization/covered-name.pdf`.

  At the owner's request, these two scripts and the fixture moved to the sibling Python project `../anonimizador` (branch `anonimizador/scaffold`), where the verifier became `anonimizador verify`. Its integration test moved with it, into `anonimizador/tests/`.

  Tests: `tests/drafts.rs` (3), `drafts::tests` (5), and 2 integration tests.

### Acceptance criteria (PLAN Phase 4 exit)
| Criterion | Evidence |
|---|---|
| All `@m1`–`@m4` pass | `bdd inventory: milestone 4, 35 required, 38 executed`; 38 scenarios / 241 steps passed |
| `fusion::rrf` with the specified score, order, tie-break, and proptests | `fusion::tests` (9); BDD "Fuse rankings without comparing incompatible raw scores" |
| `retrieval::combined` | `search_combined`; `tests/combined.rs`; real CLI integration |
| Final comparison report, reproduction, saved configs, limitations | [`reports/phase-4/comparison.md`](../phase-4/comparison.md); [`reports/README.md`](../README.md); `config/experiments/`; the limitations section of the report |
| PRD §12 deliverables and §13 criteria walked through with evidence links | [`reports/ACCEPTANCE.md`](../ACCEPTANCE.md) |

### Validation (local, 2026-10-29)
- **Final validation at approval** (head `1349cc5` plus documentation): fmt and clippy `-D warnings` for every feature set ok; `cargo test`: 166 unit, 17 pipeline, 8 combined, 7 experiment, 4 drafts, 3 evidence, 2 fixtures, BDD milestone 4 (38/38, 35 required). `RUN_INTEGRATION=1` integration: **17 passed**. `RUN_SAMPLE_EXAMS=1` real exams (local only): **4 passed**. The items below record the validation at earlier review rounds.
- fmt ok; clippy `-D warnings` for none, each engine, and all features: ok.
- `cargo test`: 166 unit (incl. 9 fusion, 2 combined-config, and 5 draft tests), 16 pipeline (incl. the G4-4 regression), 7 experiment, 7 combined (incl. the G4-1/G4-2/G4-3 regressions), 3 drafts, 3 evidence, 2 fixtures, BDD milestone 4 (38 scenarios), self-test 0 mismatches.
- Mutants caught (each regression test also fails on its mutant: dropping the snapshot check, building a model before reading the snapshot, summing active instead of pinned revisions):
  - reversing the RRF key tie-break fails 3 fusion unit tests, the BDD combined example and fusion scenario, and the combined run test;
  - dropping the combined corpus check fails `a_combined_run_refuses_legs_that_cannot_be_paired`.
- `RUN_INTEGRATION=1 cargo test --release --features engine-embed-anything,engine-fastembed --test integration -- --include-ignored`: **18 passed**, including the real combined CLI chain, the anonymization verifier on the covered-name fixture, and `draft-manifest` with real poppler. After the verifier moved to `../anonimizador`, 17 remain here. The OpenAI live test stays outstanding.
- `RUN_SAMPLE_EXAMS=1 cargo test --release --test sample_exams -- --include-ignored` (local only, real exams): **4 passed**.
  - Every unreviewed exam is refused before any processing.
  - Every file name is flagged, and no finding reveals it.
  - Coverage: 12 documents, 45 pages (39 native, 6 OCR), 739 chunks. Extraction is deterministic; no token is lost without a budget; budgets 16/64/512 are respected with and without truncation.
  - Images: 348 listed, 94 kept, 27 prepared, all PNGs with only IHDR/IDAT/IEND.
- Synthetic regressions for the two defects:
  - `anonymization::tests::identifier_like_filenames_are_flagged`;
  - `chunking::tests::a_table_header_no_row_uses_is_kept_as_text` (a mutant dropping unused headers fails it).
- Real runs, `reports/phase-4/` (release, Apple M3, 8 CPUs, CPU inference, load average 4–11; regenerated after the Codex round 1 fixes, which record run status and revisions).
  - Text and image metrics are identical to Phases 2 and 3:
    - MiniLM (both engines): 0.333 / 0.500 / 0.409;
    - E5-base: 0.433 / 0.700 / 0.540;
    - CLIP: 0.167 / 0.300 / 0.241;
    - SigLIP: 0.167 / 0.300 / 0.270.
  - Combined:
    - E5-base + SigLIP: 0.400 / **0.800** / 0.545, warm median 88 ms (0.367 / 0.800 / 0.508 before the chunking-rules revision r2);
    - MiniLM + CLIP: 0.267 / 0.600 / 0.340, warm median 40 ms (0.300 / 0.600 / 0.388 before r2).
  - Fusion raises Hit@3 above every leg.
  - Its P@3 and nDCG@3 are within tie-break noise. Every fused tie joins a text and an image item, and the `record_id` tie-break (PRD §6.6) decides it. The r2 rules changed every text record id without changing any chunk, and that alone moved both combined configurations.
  - The report's limitations explain this per query. A relevance-aware cross-leg tie rule would be a PRD change and is not made here.

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
- Owner corpus, relevance labels, and fidelity sample; all evidence is synthetic.
- Live OpenAI run (key and `remote_allowed`).
- Latency on an idle host.


## Classifier (`spec/CLASSIFIER.md`) — Phase 0: preparation

Branch `embeddings/classifier`, on top of the spec branch (PR #13). The owner asked, on 2026-10-31:
- implement the phases in order, with TDD;
- after each phase, run an adversarial review by Antigravity and then by Codex (Codex only after 03:30);
- validate and fix every finding;
- record each finding in `reports/reviews/classifier/classifier-findings.md`;
- move on to the next phase after 3 cycles if only low-severity findings remain.

### Implementation
- **Dependencies:** `unicode-normalization` pinned to `=0.1.24`, which has the Unicode 16.0.0 tables of the vectors; 0.1.26 had Unicode 18. `pdf-extract` and `stringprep` accept any `0.1`. Also added: `fancy-regex = "0.19.2"` and the feature `classifier-jev = ["dep:reqwest"]`.
- **Vectors:** `tests/fixtures/classifier/vetores.json` is TestoDoc's file (sha256 `f14676c7…db22e`), with its origin in the README next to it. A count-only check found no word used as a person's name that matches the exam file names.
- **`src/classifier/`:** `categorias.rs` holds the nine types, the question and the versioned constants. Its texts were generated from the vectors, not retyped. `tipos.rs` holds the types of §5.2–§5.5, `Classificacao` included.
- **`tests/classifier_vectors.rs`:**
  - the sha256 pin;
  - the partition of the vectors' sections into covered, pending (a list that only shrinks) and excluded;
  - `meta`, `constantes` and `criterio` checked against the Rust constants;
  - a guard that the normalization tables are Unicode 16.0.0.

### Validation (local, 2026-10-31)
- **Checks:** `cargo fmt --check`; `cargo clippy --all-targets -- -D warnings`, with and without `classifier-jev`; `cargo test` (172 lib tests, all test targets, BDD exit 0).
- **Mutants:** four tried, all detected:
  - option order in `TODAS`;
  - `TOLERANCIA_LIMIAR`;
  - slug of `Outro`;
  - one character of a description.

### Reviews
See `reports/reviews/classifier/classifier-findings.md`.

- **Antigravity, round 1:** 4 findings.
  - A0-1 (medium): `Classificacao` was missing; fixed.
  - A0-2 (medium): a covered section could go untested; fixed with a registry of checks.
  - A0-3 (low): key sets and deserialization were not tested; fixed.
  - A0-4 (low): open; it waits for the owner to push the TestoDoc rename commit.
- **Validation of the fixes:** `cargo fmt --check`; clippy with and without `classifier-jev`; 10 mutants tried, all detected.
- **Antigravity, round 2:** it confirmed that A0-1, A0-2 and A0-3 are closed, and gave NO FINDINGS.
- **Codex, round 1** (03:35 to 10:03): it agreed with those closures and found two low findings, both fixed:
  - C0-1: the doc of `margem` was wrong;
  - C0-2: the evidence lacked `cargo test --features classifier-jev`. The five §14.8 commands now pass at `76b5eba` and at `08c9cbf`.

## Classifier — Phase 1: primitives and Unicode

### Implementation
- **Python character classes (`scripts/gen-pyclasses.py` → `src/classifier/pyclasses.rs`).**
  - The script runs under Python 3.14 (UCD 16.0.0) and stops on any other UCD.
  - It writes, as sorted ranges or maps:
    - the `re` classes `\d`, `\w`, `\s`, `[^\W\d_]` and `[^\W_]`;
    - the `str` predicates `isupper`, `islower`, `isdigit`, `isdecimal`, `isspace` and the Cased property;
    - the titlecase set, category `Lt` (what CPython's `str.isupper` tests; not `str.istitle`);
    - the full `lower`, `upper` and `title` mappings.
  - The file carries the generator's sha256. `tests/classifier_pyclasses.rs` checks it on every run.
  - Two opt-in tests (`RUN_PYTHON_DIFF=1 PYTHON314=…`) regenerate the file and compare it byte for byte, and compare the primitives with Python on 2 000 random strings.
- **`texto.rs`:**
  - the predicates, and `lower`, `upper`, `title` and `str_isupper` with Python's semantics;
  - `strip` and `split` with Python's whitespace;
  - `compor` (NFC plus every `splitlines` break turned into LF);
  - `dobrar` (per code point: the first code point of its NFD decomposition, lowered when Python's `lower` gives one code point; the same number of code points);
  - `MapaOffsets`.
- **`canonico.rs`:** `json_canonico`, equal to `json.dumps(sort_keys=True, separators=(",", ":"), ensure_ascii=False)`, without NFC; it refuses floats. `digest` is the sha256 of that output.
- **`listas.rs`:** the 13 lists of the vectors, in their order.
- **`mascaramento::partes_de_nome`** and **`estado::termos_permitidos`.**
- **Vectors:** `listas`, `dobrar`, `compor`, `partes_de_nome`, `termos_permitidos` and `json_canonico` moved to the check registry. A section with no cases, or a list with a value that is not a string, fails.
- **Spec:**
  - §6.3 now says the titlecase table is category `Lt`; TDD found this.
  - It also says that no table of assigned code points is generated, because the pin of `unicode-normalization` holds.

### Validation (local, 2026-10-31)
- **Checks:**
  - `cargo fmt --check`;
  - clippy with and without `classifier-jev`;
  - `cargo test` (246 tests, BDD exit 0);
  - `RUN_PYTHON_DIFF=1 PYTHON314=/opt/homebrew/bin/python3.14 cargo test --test classifier_pyclasses -- --include-ignored` (3 tests).
- **§14.4 tests in this phase:** `ª` and `ǆ` in `title`, `ß` in `upper`, `İ` in `lower`, initial versus string `isupper`, marks and `‿` outside `\w`, `²`/`½`/`Ⅻ` in `[^\W\d_]`, U+001C–U+001F in `\s`, `\x1f` outside `compor`, and U+A7CE, a letter only from Unicode 17 on, seen as unassigned.
  - The regex items (`İ`/`ı` under IGNORECASE) belong to Phase 2 (`pyre.rs`), and so does `extract_patient_hints`.
  - Mutant: `is_upper` through Rust's `char::is_uppercase`, which has Unicode 17 tables; it is detected.

### Reviews
- **Antigravity:** not run. The run at 02:56 hit the individual quota (429, reset in about 124 h). The step stays open until the reset or a written waiver from the owner.
- **Codex, round 1:** two medium findings, both fixed:
  - C1-1: the final sigma in `lower` and `title`;
  - C1-2: Python's float `repr` in `json_canonico`, with a new float differential against Python.
- **Owner decision (10:30):** Codex only while Antigravity has no quota; Antigravity reviews this phase after its quota resets.

## Classifier — Phase 2: masking

### Implementation
- **`pyre.rs`: Python `re` patterns translated to `fancy-regex`.** Each pattern is kept as its Python text. A translator covers the subset TestoDoc uses and refuses the rest:
  - `\w`, `\d`, `\s` and their negations become classes generated from Python 3.14, inside and outside brackets;
  - `\b` becomes lookarounds over Python's `\w`;
  - without MULTILINE, `$` becomes `(?=\n?\z)`;
  - under IGNORECASE, `İ`/`ı` are added to every literal `i`/`I` and to every class that holds `i` or `I`, ranges included; `(?i:…)` and `(?-i:…)` are tracked per group.
  - `search(s, pos, endpos)` cuts at `endpos` but not at `pos`, so lookbehind and `\b` see before it. `match` is an anchored search, and `fullmatch` compiles `\A(?:…)\z`.
- **`mascaramento/`:** the five layers, `_substituir`, the marker collapse, `conferir` and `nomes_dos_rotulos`. Positions are code points: a search on the folded text is mapped back through two `MapaOffsets`. The Python texts of the regexes are built as `mascaramento.py` builds them. A regex failure is an error (`ErroMascaramento`), never a silent pass.
- **`indicios.rs`:** the `nome` field of `extract_patient_hints`, and `_nomes_dos_indicios`.
- **Vectors:** `regexes`, `remover_dados_pessoais` (126), `conferir` (12), `nomes_dos_rotulos` and `nomes_dos_indicios` moved to the check registry. Only `montar_estado`, `decidir_classificacao` and `validar_respostas` are still pending.

### Validation (local, 2026-10-31)
- **§14.8:** fmt, clippy with and without `classifier-jev`, and `cargo test` with and without the feature: 267 tests, all pass.
- **Regex differential** (`tests/classifier_pyre.rs`, opt-in): the 22 vector regexes against Python 3.14's `re`, on 1 510 texts. It compares `finditer`, `search(s, pos, endpos)` and, as TestoDoc uses them, the 40-code-point windows and `match` at every position. Mutants detected: `İ`/`ı` dropped (in literals, in classes, or both); Rust's `\b`; Rust's `$`.
- **Masking differential** (`tests/classifier_differential.rs`, opt-in, `RUN_DIFFERENTIAL=1 TESTODOC_DIR=…`): 3 000 synthetic texts through TestoDoc's own Python, through `uv` with Python 3.14, as TestoDoc's vector generator does.
  - It compares `remover_dados_pessoais` (text, counts and a second pass), `conferir`, `nomes_dos_rotulos` and `_nomes_dos_indicios`. All match.
  - A second test checks the sha256 of the three TestoDoc files and finds each helper regex in them.
- **Mutants:** 17 tried on the layers.
  - 9 were detected by the vectors and 1 by the masking differential (the 40-code-point window).
  - 6 were detected by unit tests whose expected values come from running TestoDoc on the same input:
    - name of 2 to 6 words;
    - two words that are not particles;
    - literals of 3 code points or more;
    - the order `(−length, text)` of the literals;
    - marker merge inside a value;
    - `title()` of an all-caps hint name.
  - Two of those six cannot be seen through the public functions, only by a unit test:
    - the marker merge, because the final collapse gives the same text;
    - `title()` in `_nomes_dos_indicios`, because `dobrar` undoes the case.
  - 1 was not detected and is equivalent: the column loop's advance after a lone label skips a line that is never a label. The order of name parts inside the `\b(?:…)\b` regex is also equivalent: two parts never match at the same start.
- **Properties (§14.2):**
  - Idempotence does **not** hold in TestoDoc. The differential found 65 non-idempotent texts in 5 000 (spec §0.4, F9). The port follows TestoDoc: the differential compares the second pass, and a test pins the example.
  - The marker property holds on separated pieces (256 cases). A known name glued to a digit escapes, in TestoDoc too (F10, a privacy gap reported to the owner).

### DV10: a known name glued to digits or `_` (owner decision, 2026-10-31)
- **Owner:** "não seja compatível com o Python, corrija no Rust".
- **The leak, confirmed with TestoDoc's own code.** These texts kept the whole name, and `conferir` found nothing:
  - `"Ref 12345Zanoni Prado"`, glued to a number that is not a pattern;
  - `"arquivo_Zanoni_Prado_eco"`, between underscores;
  - `"Zanoni2 Prado3"`, followed by a digit.
- **Fix.** Each known-name part is now bounded by non-letters (`(?<![^\W\d_])…(?![^\W\d_])`) instead of `\b`. Layer 2 and `conferir` share the pattern, so the fix covers both.
  - The vectors are unchanged: they have no such case.
  - The port carries its own masking version, `MASCARAMENTO_VERSAO_PORTE = "mascaramento-5+cognitionlab-1"`.
  - The spec records it as DV10 (§13), in F10 (§0.4), in §2.2, in §5.1, in §14.2 and in invariant 3.
- **Tests.**
  - A unit test written first (red, then green) covers the three cases, and `ana` inside `Banana` stays.
  - The marker proptest now also glues the pieces, with `""` and `"_"` separators.
  - The masking differential applies the same change on the Python side, so every other behaviour is still compared with TestoDoc. Without that change it fails, which shows that glued cases come up.


### Reviews
- **Pending.** Codex hit its usage limit at 11:22, before this phase's round 1 and before Phase 1's round 2; nothing was sent. Grok has no balance (402), and Antigravity is out until about 2026-11-05.
- **Owner decision (11:30):** go on with the next phases and run the pending reviews in phase order when Codex returns (2026-11-04 03:38), then Antigravity's.
- **Owner decision (11:35):** review suspended until the owner resumes it. Pending:
  - Codex: Phase 1 round 2 (closure of C1-1, C1-2), then Phase 2 and each later phase;
  - Antigravity: Phases 1 and 2 and each later phase.
- The acceptance at `c99abcc` passed in a clean worktree: the five §14.8 commands (268 tests each run) and the three differentials.

## Classifier — Phase 3: state and decision

### Implementation
- **`estado.rs`: `montar_estado`, following TD §6.**
  - Pages are composed (NFC) and selected by mode before empty pages are dropped.
  - The known names are `nomes_conhecidos`, then the hint names and the label names of the whole text.
  - Each page is masked, then `conferir` runs on the whole masked page, then the page is cut at the page and total limits, dropping a broken marker. A page left empty by the cut ends the loop.
  - The other strings of the state (file terms, title terms, extension, mode) are checked one by one. Origins are sorted.
- **`decisao.rs`:**
  - `validar_respostas` takes the single `tipo_documento` key. Missing or null probabilities become one-hot on the choice. Keys must be options, values finite and in [0, 1], and the sum within 1e-3 of 1. The values are read as Python's `float()` reads them, and the sum is Python's compensated `sum()`.
  - `decidir_classificacao` sorts with `total_cmp`. The margin is the top-two difference, never below 0. A confidence equal to the threshold is `ok`.
- **`tipos.rs`:** `EstadoEnviado::para_value()` (canonical form, for the digest) and `json_fio()` (wire JSON in TestoDoc's key order).
- **Vectors:** `montar_estado` (21), `validar_respostas` (12) and `decidir_classificacao` (7) moved to the check registry. `PENDING` is now empty. Only the two documented exclusions remain (`referencia_pseudonima`, `bloco_tipo_detectado`).

### Validation (local, 2026-10-31)
- **§14.8:** fmt, clippy with and without `classifier-jev`, and `cargo test` with and without the feature: 279 tests, all pass.
- **TestoDoc differential** (`tests/classifier_differential.rs`): each of the 3 000 cases now also carries a synthetic document. It has 1 to 4 pages with random origins, a random mode, file name, title and extension. The test compares `montar_estado` with TestoDoc's: state, digest, counts, block, origins, useful characters and type term. All match, with DV10 applied on both sides.
- **Mutants on `montar_estado`:**
  - Detected by the vectors:
    - the cut not setting `truncado`;
    - the broken marker kept;
    - extension dots kept;
    - label words counted as useful characters;
    - generic terms counted as type terms;
    - the other strings not checked;
    - the mode ignored.
  - Two survived the vectors and are now detected by unit tests whose expected values come from running TestoDoc on the same input:
    - `continue` instead of `break` after a page left empty by the cut. TestoDoc sends 10 pages and 59 991 code points, with `truncado`.
    - Unsorted origins.
  - In `decidir_classificacao`, replacing `max(0.0, d)` by `d` is equivalent: `d` is never negative after the descending sort.
- **Timing (§15, `tests/classifier_medicao.rs`, opt-in, release).** Recorded, not gated:
  - The 21 vector cases take 401 ms, including the one-time regex compile.
  - A synthetic document of 200 pages (627 504 code points) takes 90 ms in mode `1`, 100 ms in mode `2` and 300 ms in mode `todas`. Mode `todas` sends 21 pages and 60 000 code points, cut.

### Reviews
- Suspended by the owner (11:35). This phase joins the pending list: Codex and Antigravity, in phase order, when the owner resumes the review.

## Classifier — Phase 4: Jev adapter and `classificar`

### Implementation
- **`provedor.rs`** (always compiled, no I/O):
  - the neutral contract: `CriterioEscolha` and `CRITERIO_TIPO_DOCUMENTO`, `RequestNeutro` (`alvo_versao` = first 16 hex digits of the digest, `politica_versao` = `MASCARAMENTO_VERSAO_PORTE`), `RespostaProvedor`, and the `RlcdProvider` trait (native `async fn`, generics, no `dyn`);
  - `Avaliador`: the deadline wraps the wait for a slot and the call, as in TestoDoc. There is no retry, and the latency is measured from the start of the wait;
  - `classificar(prep, limiar, envio)` in TD §9.1 order: no content, then privacy block (with the `conferencia_<camada>` counts), then authorization, then the call and the decision. `Envio` has three variants (`NaoPedido`, `NaoAutorizado`, `Autorizado`), and only the authorized one can reach the provider.
- **`jev.rs`** (feature `classifier-jev`):
  - The request is `POST {base}/v1/systemone`. Its body is written by hand in the SDK's key order, with `criteria` in the order of `Categoria::TODAS`. The headers are `Authorization: Bearer`, `Content-Type`, `Accept` and `User-Agent`.
  - The model must match TestoDoc's `MODELO_JEV_RE` after `strip()`, with Python's `\d`.
  - The base URL must be `https`, or `http` on `127.0.0.1` or `localhost`, without credentials, query or fragment.
  - Status and transport map to categories as §8.5–§8.6 say. An error body is never read.
  - A 200 is decoded in two steps: the envelope (`answers` as a map, `usage` required), then the conversion to the neutral format and `validar_respostas`. Each case keeps the metadata §8.4 lists, and `confianca_provedor` is kept only with a valid answer.
  - The key is redacted in `Debug`, and it is never in errors or logs.
- **Types:** `Classificacao.model` is now `Option<String>`. It is `None` when sending is not authorized, since `RLCD_MODEL` is then never read (spec §5.5 updated).
- **Spec:**
  - §5.6 now describes `Avaliador` and `Envio`.
  - §8.5: a cut 200 body is marked `is_decode()` by `reqwest`, not `is_body()`. Any `bytes()` error other than a timeout is therefore `rede`, as the SDK's `httpx` read errors are.

### Validation (local, 2026-10-31)
- **§14.8:** fmt, clippy with and without `classifier-jev`, `cargo test` (286 tests) and `cargo test --features classifier-jev` (304 tests): all pass.
- **Unit tests:**
  - `classificar` and `Avaliador` against a test double: no call without content or with a block; `preparado` with and without the not-authorized reason; the decision; the error path; the deadline including the wait for the slot; the minimal state keys against `CHAVES_PROIBIDAS_NO_ESTADO`.
  - `jev`:
    - the model, with expected values from TestoDoc's `modelo_rlcd_fixo`;
    - the endpoint and the status table;
    - the redacted key;
    - the body's order and escapes;
    - every invalid answer and envelope listed in §14.5.
- **Local server** (`tests/classifier_jev.rs`: a `TcpListener` on `127.0.0.1`, no network):
  - the exact body and headers, a missing request id (DV2), and an invalid 200 that keeps its request id (DV8);
  - the 11 statuses, a server past the deadline, the wait for a slot inside the deadline, a closed port, a cut body, and a `reqwest` timeout mapped to `timeout`;
  - logs: a global subscriber captures every log line of the test binary, and none has the key, the error body or the exam text.
- **SDK differential** (`classifier_differential.rs` with `--features classifier-jev`): for each of the 3 000 synthetic documents, the request body is compared byte for byte with the one that typesafe-sdk 0.7.4 builds through TestoDoc's own path (`minimizar_estado`, `sdk.Choice`, `prepare_system_one`). All match. Nothing is sent.
- **Mutants:** 30 tried on `provedor.rs` and `jev.rs`.
  - 28 are detected by the unit tests or the local server, among them: the order of `criteria`, `state`/`model`, `instructions`/`criteria`; every check of `classificar`; the deadline; one more slot; the latency; each metadata field; each status and transport class; each header; the model `strip()`; the local-only `http`.
  - 2 survived the first run:
    - a `type` other than `choice` with an otherwise valid shape is now detected, by a new case;
    - a non-number `confidence` was an equivalent mutant: `as_f64()` already refuses a string. The redundant filter was removed, and a case pins the behaviour.

### Reviews
- Suspended by the owner. Phase 4 joins the pending list.
- Not run: the live Jev test (§14.6) is optional and is for the owner to run.

## Classifier — Phase 5: `classify` CLI

### Implementation
- **`src/classify.rs`**, with the `classify` subcommand wired in `cli.rs`:
  - `executar(args, fonte, ambiente)` is the whole command except printing. It follows §10.1 in order:
    1. mode and threshold, with Python's `float()`;
    2. the file and its format;
    3. the R-P1 double opt-in. Only then are `RLCD_MODEL`, the key (`RLCD_API_KEY`, else `TYPESAFE_API_KEY`), `RLCD_BASE_URL` and `RLCD_TIMEOUT_S` read and validated;
    4. the tools;
    5. the extraction, with the §10.2 OCR rule and the §10.4 visual page;
    6. `montar_estado` and `classificar`.
  - The extraction is a `Fonte` trait: `FontePoppler` (poppler + tesseract with `por`) in the binary, a double in the tests. The environment is a lookup function. In the binary it reads process variables first, then a `.env` read by `dotenvy::dotenv_iter` without changing the environment.
  - `LinhaCli` holds the §10.3 keys in order, and the error envelope fills only what the stop established. `--mostrar-estado` prints the state in wire order, only when it is safe. The readable mode prints the same fields. Every stderr message is a fixed phrase per reason, with no path and no error text (DV9).
  - A binary built without `classifier-jev` refuses authorized sending with code 5 and reads no sending variable (spec §12).
- **`TesseractOcr::ocr_image`:** OCR of a standalone image, outside the `OcrEngine` trait (§10.2).
- **Fixtures** (`tests/fixtures/classifier/cli/`, from `examples/gen_classifier_fixtures.rs`; invented text and names): a Holter PDF with native text, the A9 echocardiogram `eco_mapa.pdf`, a scanned PDF and a PNG.

### Finding while running the real exams locally (§14.7): the backtracking limit
- **Symptom.** The local, no-send run over the owner's sample exams failed with `erro_interno` on 10 of the 12 PDFs. The diagnosis printed only exam indices and error categories, never content.
- **Cause.** `fancy-regex` stops a search after 1 000 000 backtracking steps. Some layer-1 patterns cost a quadratic number of steps on long whitespace runs, which `pdftotext -layout` puts between columns. 3 000 spaces were enough. Python's `re` has no step limit: TestoDoc takes 1.8 s on those 3 000 spaces and leaves the text unchanged.
- **Fix.** Every regex is now compiled with `backtrack_limit(usize::MAX)`, so a search takes as long as Python's would. A regression test written first (red, then green) runs three whitespace-heavy texts and checks them against TestoDoc's outputs: unchanged text, no counts, no names, empty `conferir`. Removing the fix makes it fail again. The synthetic vectors and the differential never had such runs. Spec §6.4 now records this.
- **After the fix,** all 12 exams classify locally without sending. The states are `preparado`, `sem_conteudo` or `bloqueado_privacidade`, and every limit holds. The output has no file name and no `estado_preparado`. The run takes 12.9 s in release.

### Validation (local, 2026-10-31)
- **§14.8:** fmt, clippy with and without `classifier-jev`, `cargo test` (304 tests) and `cargo test --features classifier-jev` (324 tests): all pass.
- **In process** (`tests/classifier_cli.rs`, with the extraction double):
  - every exit code: 0, 2, 3, 4, 5 and 6, plus 1 in the unit tests;
  - the double opt-in: with `CLASSIFIER_REMOTE_ALLOWED` missing, `0`, `true`, ` 1` or `1 `, nothing `RLCD_*` and no key is even looked up;
  - the step-3 validations and the binary without the adapter;
  - the tools check: tesseract is checked lazily, at the first PDF page that needs OCR;
  - the OCR and visual-page rules on selected pages only, an OCR failure that keeps the page, and the join of native and OCR text;
  - A9 without `estado_preparado`, and no state in `sem_conteudo` or before the classification;
  - the key order and the complete envelope;
  - no path or text in either output mode;
  - with the local server: `ok` (0) and a provider error (4, `envio_realizado`).
- **Binary** (`tests/classifier_cli_bin.rs`):
  - Always: the stops before the extraction print exactly one JSON line, with the right code. Neither stdout nor stderr has the path, in JSON or readable mode.
  - With `RUN_INTEGRATION=1` and real poppler and tesseract:
    - the four fixtures give `preparado` (`nativo` or `ocr_local`);
    - `--enviar` with `CLASSIFIER_REMOTE_ALLOWED=0` and an invalid model gives `preparado`, code 0;
    - `--mostrar-estado` on the Holter shows the labelled name masked;
    - A9 gives code 6 without the state;
    - with the feature and a local server, the binary gives `ok` with `tipo = holter`.
- **Unit tests:** the `pdfimages -list` area (images and stencils, not masks; a zero resolution is unreadable), the `pdfinfo` page size, and the exit codes.
- **Mutants:** 18 tried on `classify.rs`.
  - 17 are detected; 2 of those 17 needed a stronger test, written as the mutants showed it: OCR outside the selected pages, and a blank image text (`total_paginas` 0, as in TestoDoc).
  - 1 is equivalent: showing the state despite a block. `classificar` already returns `bloqueado_privacidade`, which is not among the states that show it. The check stays as defense, mirroring TP.

### Reviews
- Suspended by the owner. Phase 5 joins the pending list.

## Classifier — Phase 6: the anonymizer's index (Python, `../anonimizador`)

### Implementation
- **`anonimizador/classifier.py`:** runs `<binary> classify --json [--enviar] <copy>` as a child process, without a shell. Its stdin is closed, its stderr is discarded, and the timeout is `2 * (RLCD_TIMEOUT_S + 5) + 5` seconds.
  - **Accepted output:** exactly one UTF-8 JSON object whose state is known and whose `codigo_saida` equals the exit code and fits the state.
  - **Index cells:** as in §11.2. The weights use three decimals, highest first, with ties in the classifier's order. A reason or error category must be a snake_case word.
  - **Failures:** a timeout, a child that cannot start, or anything else is `error: classifier failed`.
- **`batch.py`:**
  - `start(..., classify=True)` creates `<run>/classifier/` and adds it to the git probes.
  - After each exam, a copy whose re-verification is `ok` is classified, and its valid JSON line is kept with `write_new`.
  - The index gains the two columns and the header line from the first valid line, saying whether sending was requested. Without `--classifier`, the index is byte-for-byte as before.
- **CLI:** `--classifier BINARY` and `--classify-remote`, which needs `--classifier`. The child still requires `CLASSIFIER_REMOTE_ALLOWED=1`.
- **Spec §11.1:** fixed a contradiction with the §11.2 table. A valid CLI error (codes 1, 2, 5) shows its reason. An exit code that differs from the line or does not fit its state is a failure.

### Validation (local, 2026-10-31)
- **Checks:** `uv run ruff check .`, `uv run ruff format --check .` and `uv run pytest`: 215 passed, 2 skipped (the real-exam opt-ins). This includes the 13 new tests of `tests/test_classifier.py`, which use fake binaries (scripts that print fixed lines). The new tests cover:
  - every state;
  - every malformed output;
  - the tie order;
  - the timeout formula;
  - `--enviar` only with `--classify-remote`;
  - a hung, missing or non-executable binary;
  - the header without the binary's path;
  - the git probe of `classifier/`;
  - end to end with poppler and tesseract: classified copies with kept and linked lines, a copy that fails verification never classified, a failing classifier that leaves exit 0 and keeps nothing, and the index unchanged without `--classifier`.
- **Mutants:** 11 tried on `classifier.py` and `batch.py`.
  - All are detected.
  - Two of them needed a new test, written as the mutants showed it: an exit code valid for the state but not the line's, and the git probe of `classifier/`.
- **Smoke test, with no sending:** the real binary (`--features classifier-jev`) ran over two synthetic fixtures (PDF and PNG), with the outputs in the scratchpad. The index showed the columns and the header line (`model none`, `sending not requested`), and each copy appeared as `not sent`. The two JSON lines were in `classifier/`.
- **Not run:** the owner's run with sending (`--classify-remote` with `CLASSIFIER_REMOTE_ALLOWED=1` over the anonymized sample exams). It is the owner's to run (§20, Phase 6), and nothing from it is committed.

### Reviews
- Suspended by the owner. Phase 6 joins the pending list. When the review resumes, it follows the anonymizer's stage review, with a Python reviewer prompt.

## Classifier — real sending to Jev (2026-10-31)

### Implementation
- **The key in `.env`:**
  - `.env.example` now has `TYPESAFE_API_KEY=`, the optional `RLCD_*` settings with their defaults, and a note that the authorization never goes there.
  - The binary reads the process environment, then the `.env` from the working directory up, then the crate root's `.env` (`classify::ambiente_em_camadas`). As a result, `anonimizador batch --classifier`, run from its own directory, finds the key.
- **The authorization is per run:** `CLASSIFIER_REMOTE_ALLOWED` is read from the process only, never from a `.env` (`SO_DO_PROCESSO`). Spec §12 records this.
- **Live test** (`tests/classifier_live.rs`, spec §14.6): it sends three synthetic documents (echocardiogram with strain, Holter, blood test) to the real Jev. It needs `RUN_JEV_LIVE=1` and `CLASSIFIER_REMOTE_ALLOWED=1`, with the key in the environment or `.env`. It checks the shape of each answer, not the exact type:
  - the state is `ok` or `incerto`;
  - the probabilities sum to 1;
  - the confidence is the probability of the chosen type;
  - the echoed model is an exact version;
  - token usage is reported.

### Validation
- fmt, clippy with and without `classifier-jev`, `cargo test` (306 tests) and `cargo test --features classifier-jev` (326 tests): all pass.
- With `RUN_INTEGRATION=1`: a `.env` that sets `CLASSIFIER_REMOTE_ALLOWED=1` leaves the run `preparado`. A key and a base URL from the working directory's `.env` reach the local server, and the result is `ok`.
- Mutant: dropping the process-only rule is detected by the unit test and by the binary test.
- **Not run: the live test itself.** The local `.env` has no TypeSafe key yet. Once the owner adds it, the command is in `CLAUDE.md`.
