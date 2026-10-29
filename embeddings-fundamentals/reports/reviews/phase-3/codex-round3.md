ID: C3-4
Severity: medium
File:line: src/extraction/poppler.rs:52
Description: Closure rejected. `Scratch::create` returns a raw `TempDir` from `spawn_blocking` and wraps it in `Scratch` only after awaiting the handle. Cancellation after the blocking task completes but before its result is consumed drops the completed handle’s unguarded output on the runtime worker, synchronously removing the directory. This still violates PLAN §2.3 and `async-spawn-blocking`.
Impact: Cancelling extraction can still block an executor thread with filesystem cleanup.
Reproduction scenario or verifiable rationale: A completed Tokio task retains its output until consumed. Dropping its `JoinHandle` can destroy that output on the dropping thread. The regression test cancels only after entering `work`, when the `Scratch` guard already exists, so it misses this window.
Validation suggestion: Construct the `Scratch` guard inside the blocking closure. Add controlled cancellation after creation completes but before the awaiting task resumes, verifying cleanup runs on the blocking pool.

ID: C3-7
Severity: low
File:line: examples/capture_extraction.rs:65
Description: C3-6’s original production defect is corrected, but fixture capture still excludes the masks now required by ingestion. `capture_images` saves the full image listing while copying PNGs only for rows passing `ImageFilter::keeps`, which rejects every mask.
Impact: Recapturing a qualifying masked image produces an incomplete fixture that cannot replay successfully.
Reproduction scenario or verifiable rationale: Capture a PDF containing a qualifying image followed by an `smask` row. The listing records both rows, but only the base PNG is copied. During replay, `source_images` finds the mask and requests its PNG; `FixtureExtractor::extract_image` returns `MissingFixture`.
Validation suggestion: Capture each retained image’s associated mask too. Generate and replay a fixture from `masked-images.pdf`, requiring successful ingestion and the expected masked pixels.