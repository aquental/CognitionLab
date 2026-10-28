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

**Status:** implemented and validated; reviews pending.

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
