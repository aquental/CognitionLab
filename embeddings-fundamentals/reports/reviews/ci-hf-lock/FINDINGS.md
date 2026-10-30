# CI fix: model downloads under one hub cache lock — adversarial review findings

Branch `fix/serialize-model-downloads`, reviewed from base `main` (`8ed6cbe`).

**Problem.** The integration job failed on `main` after the phase 3 and phase 4 merges (runs 40852972419, 40883520031). Both runs had a cold Hugging Face cache, and in each a different EmbedAnything test failed with `Cache lock timed out`. Parallel tests loaded the same MiniLM, EmbedAnything's own hf-hub 1.0 downloads bypassed the crate's `FETCH` mutex, and hf-hub gives up on its blob lock after 10 s.

**Fix.** `hf::exclusive` runs a fetch under the `FETCH` mutex and an advisory file lock, `<hub cache>/.locks/embeddings-fundamentals.lock`, held across processes. `hf::download`, `Embedder::from_pretrained_hf` and `ClipEmbedder::new` run under it. CI also runs the lib tests with the local-engine features, which the `hf` module needs to compile.

**Reviewers.** The owner asked for Codex and Antigravity on 2026-10-30 ("o Codex voltou, pode usar ele e o Antigravity"; "pode seguir com as revisões do Codex e Antigravity"). Grok was not run; this is an owner waiver for this fix.

**Known test limit.** The integration test `embed_anything_text_loads_wait_for_the_cache_lock` checks the text adapter only. Its mutant without the lock fails the test. The same check on the image adapter cannot fail: `vision::load` also downloads `tokenizer.json` and `preprocessor_config.json` through `hf::download`, which waits for the lock, so the build waits whether or not `ClipEmbedder::new` holds it. A mutant dropping that lock survived and is recorded here.

## Codex round 1 (`codex-round1.md`, reviewed `29a8d7b`)

| ID | Reviewer | Severity | Description | Resolution / refutation | Files | Validation evidence | Reviewer confirmation | Status |
|---|---|---|---|---|---|---|---|---|
| CF-1 | Codex | medium | Every fetch needed a writable lock file, so a read-only cache that already holds the models failed before the fetch ran. Before the fix, such a cache was read without writes | Valid. `open_lock`: when the lock directory or file cannot be written (`PermissionDenied`, `ReadOnlyFilesystem`), an existing lock file is opened read-only and still locked (`flock` does not depend on the open mode). With no lock file, `fetch` runs under the in-process guard alone, because the process cannot download into that cache either. Other I/O errors still fail | `src/embedding/hf.rs` | `hf::tests::a_cache_that_cannot_be_written_is_read_without_a_lock_file_of_its_own` and `an_existing_lock_file_is_locked_even_when_it_cannot_be_written` (unix, chmod). Mutants killed: no read-only fallback, fallback skipping the existing file, `NotFound` treated as an error, no file lock | pending | fixed |
| CF-2 | Codex | low | `HfError::Lock` turned the `io::Error` into a string and dropped the source chain | Valid. `Lock { path, #[source] source: io::Error }`. The message still includes the cause, because `adapters::BuildError::Load` keeps only `to_string()` of the load error | `src/embedding/hf.rs` | `hf::tests::a_lock_failure_keeps_its_io_error`: a lock path under a regular file fails, and `source()` downcasts to `io::Error`. The previous `reason: String` version fails it. Removing only `#[source]` is an equivalent mutant: thiserror treats a field named `source` as the source anyway | pending | fixed |
