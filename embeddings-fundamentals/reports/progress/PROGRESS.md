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

**Status:** implemented; merged to `main` as PR #5 at the owner's explicit request (2026-10-28, "pare e mescle os PRs"), **before** its adversarial reviews. Phase 2 is **not approved yet**. Review fixes go on branch `phase-2/review-fixes` (from `main` @ `44cf64a`); the review diff base is `3cdb355` (merge of PR #4, end of Phase 1).

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
- Reviews in progress: Grok rounds 1–3 (G2-1..G2-6, round 3 NO FINDINGS), Codex round 1 (C2-1..C2-9), Grok round 4 NO FINDINGS, Codex round 2 (C2-2..C2-9 CLOSED; C2-1 reopened; new C2-10..C2-12, all fixed). Grok round 5 raised G2-7..G2-9 (fixed); Grok round 6 NO FINDINGS; Codex round 3 closed C2-1, C2-10..C2-12 and raised C2-13..C2-16 (fixed); Grok round 7 then Codex round 4 confirm. See `reports/reviews/phase-2/FINDINGS.md`.
- Integration after the fixes: `RUN_INTEGRATION=1 cargo test --features engine-embed-anything,engine-fastembed --test integration -- --include-ignored`: 10 passed after the Codex round-2 fixes, including the new CLI exit-2 test for C2-10 (2026-10-28). The OpenAI live test is not compiled without `engine-openai` and stays outstanding.
- OpenAI live integration: no `OPENAI_API_KEY` and no owner authorization for remote processing.
- Owner corpus, relevance labels, and fidelity sample; all evidence is synthetic.
- The price in `config/prices/openai-embeddings-2026-10.toml` must be verified against the published price before an OpenAI estimate is reported.
