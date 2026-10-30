ID: CF-1
Severity: medium
File: src/embedding/hf.rs:101
Description: Every fetch now requires a writable lock file, including reads of fully cached models. A prepopulated, read-only Hugging Face cache fails before the fetch closure runs.
Impact: Local engines cannot load cached models from read-only mounts or shared caches where the caller lacks write permission.
Reproduction scenario or verifiable rationale: Populate the cache, make it read-only, and load a cached model as an unprivileged user. Creating the lock file—or opening an existing read-only lock file with `.write(true)`—returns a permission error.
Validation suggestion: Add a warm-cache loading test against a read-only cache, running without elevated privileges. Support cached reads without requiring cache writes while retaining serialization for downloads.

ID: CF-2
Severity: low
File: src/embedding/hf.rs:93
Description: The new lock error converts `std::io::Error` into a string instead of retaining it as a `#[source]`, violating PLAN §2.3 rules `err-source-chain` and `err-thiserror-lib`.
Impact: Callers lose the underlying error type and cannot inspect its I/O kind or traverse the source chain to distinguish permission failures from unsupported locking or other filesystem errors.
Reproduction scenario or verifiable rationale: Any directory creation, lock-file opening, or locking failure produces `HfError::Lock` with only a textual reason; its `Error::source()` is `None`.
Validation suggestion: Store the original `std::io::Error` as a source field and verify that a forced lock-file failure preserves its error kind through the source chain.