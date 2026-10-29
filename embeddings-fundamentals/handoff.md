# Handoff — Medical Document Embedding Lab

Updated: 2026-10-29. Use this file to resume `spec/prompt/execute.md` in a new session.

## Owner's standing instructions
- Run `spec/prompt/execute.md`, phases 0 → 4 in order. Each phase gets **its own branch and PR** before the next phase starts. Implement every phase.
- Reply to the owner in Portuguese.
- Merges are **not** authorized. Push and PR creation are authorized, one per phase.
- Review loop for each phase: implementation → validation → **Grok** review → fixes → Grok confirmation → **Codex** review → fixes → Codex confirmation. If Codex fixes change reviewed behavior, run Grok again and then Codex again. Each finding must be fixed or refuted with evidence, and the reviewer must confirm.
- **Codex waiver:** when Codex fails with the ChatGPT usage-quota error, skip it. Record the skip as an owner waiver in `PROGRESS.md` and in the phase's `FINDINGS.md`. Never simulate a review. Grok is always required.
- Never invent results, reviewer reports, credentials, or remote authorization. Never send secrets or medical documents to reviewers.
- Invoke `/rust-skills` before any Rust work (see `CLAUDE.md`).
- Commit attribution: `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. PR bodies end with `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.

## State (updated 2026-10-29)
| Phase | Branch | PR | Status |
|---|---|---|---|
| 0–3 | — | #3–#8 | Approved and merged to `main` (Phase 3 via PR #8, `e1abe6f`) |
| 4 Combined + publication (`@m4`) | `phase-4/combined-retrieval` | — | Implemented and validated; reviews Grok → Codex → Antigravity in progress |

The review loop now has a third reviewer, Antigravity (`agy`), after Grok and Codex (see `CLAUDE.md`).

Details live in `reports/progress/PROGRESS.md` (read it first) and `reports/reviews/phase-N/`.

## Next steps
0. **Intermittent test failure, not fixed.** `storage::tests::abandoned_staging_is_removed_on_open_and_never_visible` fails with `WriterBusy` on reopen in about 1 of 10 full `cargo test --lib` runs, and never when run alone.
   - Hypothesis (unconfirmed): a process spawned concurrently by another test (`tools`/`poppler` `Command`, or `sysctl` in `Hardware::detect`) inherits the `.writer.lock` descriptor between fork and exec, so the `flock` outlives the dropped store. A real writer could hit the same race.
   - Next: reproduce it, confirm the cause, then fix it. Candidate fixes are an `O_CLOEXEC` audit or a lock that is not inherited, for example `fcntl` OFD locks or a retry with a bounded wait. Add a regression test.
1. **Phase 2 Grok review.** Build the package with `scripts/review-package.sh 2 phase-1/text-baseline Grok <out> [history]`. Run Grok headless and read-only, in the background, with docs inlined and no tools:
   `timeout 3600 grok --permission-mode plan --no-subagents --disable-web-search --prompt-file <pkg>`
   The method is also in memory `grok-adversarial-review`. Store the verbatim output as `reports/reviews/phase-2/grok-roundN.md`. Track findings in `reports/reviews/phase-2/FINDINGS.md`, using the Phase 1 table format.
2. Fix the findings and add regression tests. Run Grok confirmation rounds until a round reports NO FINDINGS.
3. **Codex review:** `codex exec -s read-only -C . --skip-git-repo-check -o reports/reviews/phase-2/codex-roundN.md - < <pkg>`. On the quota error, apply the waiver.
4. Mark Phase 2 approved in `PROGRESS.md`, confirm CI is green, and mark the PR ready.
5. Start Phase 3 on `phase-3/image-retrieval`, branched from `phase-2/engine-comparison`:
   - CLIP (`openai/clip-vit-base-patch32`) via EmbedAnything. Run a load spike for SigLIP-multilingual first (PLAN §0.1).
   - Image records and `pdfimages` filtering. Metadata-free re-encoded assets.
   - Image chunk ids **must** be `chest-001-p1-image-0`, `abd-us-001-p1-image-0`, and `us-mixed-003-p1-image-0`, because the frozen labels already use them.
   - Replace `ValidationError::ModalityNotImplemented` with a real image run.
   - Run the `@m3` scenarios (bump `config/acceptance.toml`, then run `scripts/sync-features.py`).
6. Phase 4: RRF (`k = 60`) and the combined mode, then final evidence, the README, and the acceptance mapping.

## Validation commands
- Full local check: fmt, clippy for each feature set, and the tests. The script lives in the session scratchpad and is not committed. It runs:
  - `cargo fmt --check`
  - `cargo clippy --all-targets [--features X | --all-features] -- -D warnings` for none, each engine, and all features
  - `cargo test`
- Run the full check **before every commit**. CI previously failed on fmt.
- OpenAI contract tests: `cargo test --features engine-openai --lib embedding::openai`
- Real integrations: `RUN_INTEGRATION=1 cargo test --features engine-embed-anything,engine-fastembed --test integration -- --include-ignored`. These need poppler, `tesseract-lang`, and the HF cache, all installed here.
- Measured runs, from a release build with `--features engine-embed-anything,engine-fastembed`:
  `embeddings-fundamentals experiment --manifest tests/fixtures/corpus/manifest.toml --config config/experiments/<cfg>.toml --store <dir> --experiment <id> --fidelity config/fidelity/sample-synthetic-v1.toml --report reports/phase-2/<id>.json`
  Then combine them with `embeddings-fundamentals report --result … --json … --markdown … --note …`.

## Outstanding owner inputs (never claim them done)
- Anonymized owner corpus, relevance labels for the 10 frozen queries, and a fidelity sample. All current evidence is synthetic.
- OpenAI live run: needs `OPENAI_API_KEY` in `.env` and `remote_allowed = true` from the owner. The live test is `openai_live_embeddings_rank_sensibly` and additionally needs `OPENAI_REMOTE_ALLOWED=1`.
- Verify the price in `config/prices/openai-embeddings-2026-10.toml` against the published price.

## Gotchas
- Feature files are generated. Never hand-edit `us*.feature` or `rel*.feature`.
- The two MiniLM exports (sentence-transformers and Xenova) have byte-different `tokenizer.json` files, so their chunking fingerprints differ. The chunks themselves are identical.
- hf-hub gives up on a held blob lock after about 5 s. FastEmbed downloads are serialized and retried in `src/embedding/fastembed.rs`.
- `ort` conflict: EmbedAnything must stay **without** its `ort` feature, because FastEmbed pins `ort =2.0.0-rc.15`.
- Clippy denies `expect` in test helpers that are not `#[test]` functions. Use `#[expect(clippy::expect_used, reason = …)]`.
