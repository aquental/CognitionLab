# Handoff — Medical Document Embedding Lab

Updated: 2026-10-29. Use this file to resume `spec/prompt/execute.md` in a new session.

## Owner's standing instructions
- Run `spec/prompt/execute.md`, phases 0 → 4 in order. Each phase gets **its own branch and PR** before the next phase starts. Implement every phase.
- Reply to the owner in Portuguese.
- Push and PR creation are authorized, one per phase. A merge needs the owner's explicit authorization; the only one standing is the merge order below.
- **Merge order (owner, 2026-10-29):** at the end, once their review loops approve, merge the Phase 4 PR first and then the `anonimizador` PR. The `anonimizador/scaffold` branch depends on Phase 4. Open or retarget its PR against `main` only after Phase 4 merges, so its diff shows only the subproject.
- Review loop for each phase: implementation → validation → **Grok** review → fixes → Grok confirmation → **Codex** review → fixes → Codex confirmation. If Codex fixes change reviewed behavior, run Grok again and then Codex again. Each finding must be fixed or refuted with evidence, and the reviewer must confirm.
- **Codex waiver:** when Codex fails with the ChatGPT usage-quota error, skip it. Record the skip as an owner waiver in `PROGRESS.md` and in the phase's `FINDINGS.md`. Never simulate a review. Grok is always required.
- Never invent results, reviewer reports, credentials, or remote authorization. Never send secrets or medical documents to reviewers.
- Invoke `/rust-skills` before any Rust work (see `CLAUDE.md`).
- Commit attribution: `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. PR bodies end with `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.

## State (updated 2026-10-29)
| Phase | Branch | PR | Status |
|---|---|---|---|
| 0–3 | — | #3–#8 | Approved and merged to `main` (Phase 3 via PR #8, `e1abe6f`) |
| 4 Combined + publication (`@m4`) | `phase-4/combined-retrieval` | see `PROGRESS.md` | **Approved.** Grok rounds 1–7, Codex rounds 1–3, Antigravity rounds 1–2; each final round reported NO FINDINGS. 14 findings, all closed (G4-5 moved to `anonimizador`). To be merged first, per the merge order |
| `anonimizador` stage 0 | `anonimizador/scaffold` | after Phase 4 merges | Holds the verifier moved out of Phase 4 (G4-5 and the verifier part of G4-6 fixed there). Its own review loop (Grok → Codex → Antigravity, told the code is Python and to judge it by Python best practices) runs next |

`REVISION.md` lists every reviewer finding per phase with its resolution. Details live in `reports/progress/PROGRESS.md` (read it first) and `reports/reviews/phase-N/`.

## Next steps
1. Merge the Phase 4 PR.
2. Review `anonimizador` (stage 0) with Grok → Codex → Antigravity. Reports go to `../anonimizador/reports/reviews/stage-0/`. Then open its PR against `main` and merge it.
3. **Intermittent test failure, not fixed.** `storage::tests::abandoned_staging_is_removed_on_open_and_never_visible` fails with `WriterBusy` on reopen in about 1 of 10 full `cargo test --lib` runs, and never when run alone.
   - Hypothesis (unconfirmed): a process spawned concurrently by another test (`tools`/`poppler` `Command`, or `sysctl` in `Hardware::detect`) inherits the `.writer.lock` descriptor between fork and exec, so the `flock` outlives the dropped store. A real writer could hit the same race.
   - Next: reproduce it, confirm the cause, then fix it. Candidate fixes are an `O_CLOEXEC` audit or a lock that is not inherited, for example `fcntl` OFD locks or a retry with a bounded wait. Add a regression test.
4. Owner inputs (below): build the owner corpus with `TESTS.md` and `../anonimizador`, then rerun the experiments.

## Review commands
- Packages: `scripts/review-package.sh <phase> <base> <Reviewer> <out> reports/reviews/phase-N/FINDINGS.md`, built in a clean worktree of the committed head (it has no `tests/sample-exams/`). Grep each package for secrets, and count exam file-name words (lengths only) before sending.
- Grok: `timeout 3600 grok --permission-mode plan --no-subagents --disable-web-search --prompt-file <pkg>`.
- Codex: `codex exec -s read-only -C . --skip-git-repo-check -o <out> - < <pkg>`. On the quota error, apply the waiver.
- Antigravity: `agy --mode plan --sandbox --print-timeout 3600s --print "<prompt>"`, with the package placed in the worktree as `.review/agy-roundN-package.md`, and the prompt naming the `rust-dev` plugin.

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
