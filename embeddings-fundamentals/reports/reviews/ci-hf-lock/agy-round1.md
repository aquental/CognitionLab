`src/embedding/hf.rs:88-123` (`exclusive` / `exclusive_at` lock acquisition, directory `File::lock` after `FETCH.lock()`, and reverse-order local drop releasing the advisory directory lock before `_guard`)
`src/embedding/hf.rs:104-117` (unlocked fallback restricted to `PermissionDenied` and `ReadOnlyFilesystem` on `create_dir_all`, propagating all other directory creation/open/lock errors as `HfError::Lock` with `#[source]`)
`src/embedding/hf.rs:133-163`, `src/embedding/embed_anything.rs:125-134`, `src/embedding/vision.rs:318-333`, `src/embedding/fastembed.rs:112-138` (all call sites of `hf::exclusive` and `hf::download`, none nested or re-entrant)
`src/embedding/embed_anything.rs:127-147`, `src/embedding/vision.rs:313-361`, `src/embedding/fastembed.rs:198-200` (all blocking `hf::exclusive` and `hf::download` calls executed inside `tokio::task::spawn_blocking`)
`src/embedding/hf.rs:301-386`, `tests/integration.rs:343-405`, `.github/workflows/embeddings-fundamentals.yml:83-88` (unit tests for serialization, lock lifetime, read-only cache locking, uncreatable cache fallback, error source preservation, integration lock-wait test, and CI local-engine unit test step)

NO FINDINGS
