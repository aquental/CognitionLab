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

**Status:** implemented and validated. Reviews in progress.

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
- `cargo test`: 27 unit tests pass; `bdd` passes (milestone 0); `bdd_selftest` has 0 mismatches; `integration` has 1 test ignored by default
- `RUN_INTEGRATION=1 cargo test --test integration -- --include-ignored`: 1 passed (poppler 26.10.0, tesseract 5.5.3)
- Real tools on fixtures: `pdftotext -layout` returns the lab rows intact; `pdfimages -list` shows image/smask types as expected; tesseract (`por+eng`, psm 6) OCRs the scanned page and returns `12,0-16,0` with an ASCII hyphen (input for the G28 row grammar)

### rust-skills applied
`err-thiserror-lib`, `err-no-unwrap-prod` (lints), `type-newtype-ids`, `api-parse-dont-validate`, `serde-deny-unknown-fields`, `num-float-compare`, `num-cast-try-from`, `async-fn-in-trait`/`trait-object-safety` (async_trait only on the dyn strategy), `proj-lib-main-split`, `proj-feature-additive`, `obs-library-facade`, `test-proptest-properties`, `lint-workspace-lints` (crate-level), `unsafe` denied.

**Deviations from PLAN §2.3:** none. The PLAN text was updated for the self-test design and the fixture generator location.

### Reviews
See `reports/reviews/phase-0/`.

### Blockers
None for Phase 0.
