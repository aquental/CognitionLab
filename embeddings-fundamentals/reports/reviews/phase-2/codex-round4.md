C2-13: CLOSED — The factory and both local loaders reject revisions that are not full 40-character lowercase hexadecimal commit IDs before downloading.

C2-14: CLOSED — Errors throughout the post-ingestion block become `EvaluationNotRun` when ingestion is incomplete, preserving the ingestion report and exit-2 handling.

C2-15: CLOSED — Writer-store opening and the identified synchronous CLI file operations now run through `spawn_blocking`; JSON file writes use `tokio::fs`.

C2-16: CLOSED — Repeated names among sampled rows skip name matching and use the ambiguity-checked value fallback. The regression tests cover both sample orders.

ID: C2-17
Severity: medium
File:line: src/fidelity.rs:295
Description: Name matching checks uniqueness among sample rows but not among extracted lines. For a unique sampled name, `.find()` still assigns the first extracted occurrence even when several lines have that name.
Impact: A sampled repeated test can be scored against another occurrence, producing incorrect extraction accuracy despite the correct row being present.
Reproduction scenario or verifiable rationale: Supply one sample row `A | 2 | u | r` and an extracted page containing `A | 1 | u | r` followed by `A | 2 | u | r`. The name pass selects the first line and reports 3/4 correct fields. Reversing the extracted lines reports 4/4. Sampling only one occurrence of a repeated test is sufficient; the C2-16 regression samples both occurrences and misses this case.
Validation suggestion: Add the single-sampled-row case with both extracted orders. Treat multiple extracted name matches as ambiguous and disambiguate explicitly, rather than accepting the first occurrence.

ID: C2-18
Severity: medium
File:line: tests/fixtures/measurements/expected.toml:6
Description: Timing expectations are derived from clock-read counts, while `ManualClock` advances on reads independently of the operation being measured. Consequently, the measurement tests do not establish that embedding or model-loading work occurs inside the measured interval.
Impact: Incorrect timing boundaries can pass the acceptance checks and publish plausible but false measurements, undermining the Phase 2 measurement requirement and PRD §5.7’s behavioral-testing contract.
Reproduction scenario or verifiable rationale: In `MeasuredEmbedder::embed_documents`, move `let start = self.clock.now()` immediately after `self.inner.embed_documents(raw).await`. This excludes the entire embedding operation from its measured duration, yet the supplied clock still yields one tick between `start` and `observe`. The decorator unit test and US-05 expected batch latency, throughput, and cold-start figures remain unchanged.
Validation suggestion: Use a clock whose time advances explicitly when the controlled backend performs work, with distinct load and inference durations. Require the tests to fail when either timer starts after its operation completes.