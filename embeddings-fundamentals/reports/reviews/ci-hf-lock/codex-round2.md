CF-1: CLOSED — `src/embedding/hf.rs:132–134` opens an existing lock read-only or permits fetching without one when creation is denied; lines 104–108 execute that fallback.

CF-2: CLOSED — `src/embedding/hf.rs:57–63` retains `#[source] source: std::io::Error`; lines 97–100 preserve the original error.

ID: CF-3
Severity: medium
File:line: src/embedding/hf.rs:132–134
Description: Permission failure on the lock path silently disables cross-process serialization. An unwritable `.locks` directory or inaccessible application lock does not establish that the model cache itself is unwritable.
Impact: Processes using a shared, writable model cache can bypass the new lock and reproduce the concurrent downloads and blob-lock timeouts this fix addresses.
Reproduction scenario or verifiable rationale: Make `.locks` nonwritable, leave the application lock absent, and leave existing per-model lock directories and model cache directories writable. Both processes receive `PermissionDenied` creating the application lock and `NotFound` opening it, so both execute their fetch closures with only their independent process-local mutexes.
Validation suggestion: Add a two-process regression test with these permissions and verify that fetch closures cannot overlap. Restrict the unlocked fallback to verified cached-only operations; inability to create or read the application lock alone must not authorize an unrestricted download.