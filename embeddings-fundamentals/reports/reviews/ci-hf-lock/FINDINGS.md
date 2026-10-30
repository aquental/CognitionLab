# CI fix: model downloads under one hub cache lock — adversarial review findings

Branch `fix/serialize-model-downloads`, reviewed from base `main` (`8ed6cbe`).

**Problem.** The integration job failed on `main` after the phase 3 and phase 4 merges (runs 40852972419, 40883520031). Both runs had a cold Hugging Face cache, and in each a different EmbedAnything test failed with `Cache lock timed out`. Parallel tests loaded the same MiniLM, EmbedAnything's own hf-hub 1.0 downloads bypassed the crate's `FETCH` mutex, and hf-hub gives up on its blob lock after 10 s.

**Fix.** `hf::exclusive` runs a fetch under the `FETCH` mutex and an advisory lock across processes. The first version locked a file, `<hub cache>/.locks/embeddings-fundamentals.lock`. Since CF-3, it locks the hub cache directory itself, opened read-only. `hf::download`, `Embedder::from_pretrained_hf` and `ClipEmbedder::new` run under it. CI also runs the lib tests with the local-engine features, which the `hf` module needs to compile.

**Reviewers.** The owner asked for Codex and Antigravity on 2026-10-30 ("o Codex voltou, pode usar ele e o Antigravity"; "pode seguir com as revisões do Codex e Antigravity"). Grok was not run; this is an owner waiver for this fix.

**Known test limit.** The integration test `embed_anything_text_loads_wait_for_the_cache_lock` checks the text adapter only. Its mutant without the lock fails the test. The same check on the image adapter cannot fail: `vision::load` also downloads `tokenizer.json` and `preprocessor_config.json` through `hf::download`, which waits for the lock, so the build waits whether or not `ClipEmbedder::new` holds it. A mutant dropping that lock survived and is recorded here.

## Codex round 1 (`codex-round1.md`, reviewed `29a8d7b`)

| ID | Reviewer | Severity | Description | Resolution / refutation | Files | Validation evidence | Reviewer confirmation | Status |
|---|---|---|---|---|---|---|---|---|
| CF-1 | Codex | medium | Every fetch needed a writable lock file, so a read-only cache that already holds the models failed before the fetch ran. Before the fix, such a cache was read without writes | Valid. `open_lock`: when the lock directory or file cannot be written (`PermissionDenied`, `ReadOnlyFilesystem`), an existing lock file is opened read-only and still locked (`flock` does not depend on the open mode). With no lock file, `fetch` runs under the in-process guard alone, because the process cannot download into that cache either. Other I/O errors still fail | `src/embedding/hf.rs` | `hf::tests::a_cache_that_cannot_be_written_is_read_without_a_lock_file_of_its_own` and `an_existing_lock_file_is_locked_even_when_it_cannot_be_written` (unix, chmod). Mutants killed: no read-only fallback, fallback skipping the existing file, `NotFound` treated as an error, no file lock | Codex round 2 (`def6ee8`): CLOSED. The fallback was then replaced for CF-3 | closed |
| CF-2 | Codex | low | `HfError::Lock` turned the `io::Error` into a string and dropped the source chain | Valid. `Lock { path, #[source] source: io::Error }`. The message still includes the cause, because `adapters::BuildError::Load` keeps only `to_string()` of the load error | `src/embedding/hf.rs` | `hf::tests::a_lock_failure_keeps_its_io_error`: a lock path under a regular file fails, and `source()` downcasts to `io::Error`. The previous `reason: String` version fails it. Removing only `#[source]` is an equivalent mutant: thiserror treats a field named `source` as the source anyway | Codex round 2 (`def6ee8`): CLOSED | closed |

## Codex round 2 (`codex-round2.md`, reviewed `def6ee8`)

CF-1 and CF-2 are confirmed CLOSED.

| ID | Reviewer | Severity | Description | Resolution / refutation | Files | Validation evidence | Reviewer confirmation | Status |
|---|---|---|---|---|---|---|---|---|
| CF-3 | Codex | medium | The CF-1 fallback treated "cannot create or open the lock file" as "cannot download". With `.locks` unwritable but the per-model directories writable, two processes would both skip the lock and download concurrently again | Valid. The lock no longer needs a file: `exclusive` takes an advisory lock (`File::lock`, `flock` on Linux and macOS) on the hub cache directory itself, opened read-only. Every process that can read the cache takes the same lock, whether or not it can write there. The only unlocked case left is a cache directory that does not exist and cannot be created (`PermissionDenied`, `ReadOnlyFilesystem`); hf-hub cannot download into it either. Any other error is `HfError::Lock`. The CF-1 behavior is kept: a read-only cache still loads | `src/embedding/hf.rs` | `hf::tests::a_cache_that_cannot_be_written_is_still_locked` (a 0o555 cache is locked against a separate open, which is what another process has), `a_cache_that_cannot_be_created_is_fetched_without_a_lock`, `the_cache_is_locked_while_fetching_and_released_after`, `a_lock_failure_keeps_its_io_error`. Mutants killed: no unlocked fallback, every create error falling back, no directory lock. One survivor, removing the in-process `FETCH` mutex: on macOS and Linux `flock` already serializes separate opens in one process. The mutex stays for platforms where `File::lock` is a per-process `fcntl` lock | Codex round 3 (`1f1172e`): CLOSED | closed |

## Codex round 3 (`codex-round3.md`, reviewed `1f1172e`)

CF-3 is confirmed CLOSED. NO FINDINGS.

## Antigravity round 1 (`agy-round1.md`, reviewed `1f1172e`)

Run read-only (`agy --mode plan --print=…`) with the `rust-dev` plugin. The prompt required a cited verdict for each area. The first call never ran: the new agy CLI read `--mode` as the `--print` prompt and exited with an argument error, so no review came back and that call is not counted.

NO FINDINGS, with cited verdicts on lock acquisition and drop order, the unlocked fallback, every call site of `hf::exclusive` and `hf::download` (re-entrancy), their blocking-pool use, and the regression tests. The citations include `fastembed.rs` lines that are not in the package, and they match the code: the review read the worktree.

## Outcome

Codex (round 3) and Antigravity (round 1) both have no unresolved finding on the final state `1f1172e`. Grok was not run (owner waiver for this fix). 3 findings, all Codex, all fixed and confirmed closed.

## CI confirmation

PR #11, run 40996386014 on `06b64c8`:

- **Cache:** the HF cache was cold ("Cache not found for input keys: hf-d73eb2d5…").
- **Results:**
  - `fmt, clippy matrix, unit + BDD`: passed.
  - Lib tests with the local engines: 171 passed.
  - Integration: 18 passed in 815 s.
- **Log:** no `Cache lock timed out` anywhere.
- **Cache save:** the job saved the HF cache.

