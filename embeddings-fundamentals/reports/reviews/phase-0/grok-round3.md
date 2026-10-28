The inlined diff stops mid-file, so I am reading the rest of the review package before judging G0-12 through G0-17 and looking for new defects.G0-12 CLOSED. `gherkin_blocks` returns an error when a fence is still open, and the self-test rejects a closed block followed by an unclosed one.
G0-13 CLOSED. The NFC test now requires the composed JSON bytes `"Hemoglobina é"`, which NFD does not produce.
G0-14 CLOSED. The first three components are frozen for a known text, and a second text must produce a different vector.
G0-15 CLOSED. The runner writes `fingerprints.json` (PRD, acceptance config, corpus manifest, feature files) beside `cucumber.json`, and that directory is the uploaded artifact.
G0-16 CLOSED. `probe` keeps a version only when `status.success()` is true, and the unit test covers exit 1 with stderr.
G0-17 CLOSED. Equal-length reordering returns `ItemOrder` with index, expected id, and actual id, and the reorder test locks that message.

ID: G0-18
Severity: medium
File:line: tests/support/runner.rs:115
Description: `.normalized()` wraps the tee of the console writer and the JSON writer. Cucumber's `Normalized` writer rewrites `And` and `But` to the previous `Given`, `When`, or `Then` before the inner writer, including `writer::Json`, sees the step. `inventory::executed` builds `ScenarioKey` steps as `"{keyword} {name}"` from that JSON, while `from_prd` uses the keyword printed in the PRD. Every passing self-test scenario uses only `Given` and `Then`. The only inventory case that contains `And` is the undefined-step failure, which is red whether or not the keys match. `harness.feature` uses `And`, but those scenarios are not part of the required set at milestone 0.
Impact: A required scenario that passed is reported as not executed once its PRD text contains `And` or `But`. Milestone 1 fails the inventory check on a green cucumber run. The self-test still reports 0 mismatches, so `fail_on_skipped` and the step-text gate look proven.
Reproduction scenario or verifiable rationale: Required and feature text `Given the value "2"` / `And a defined step` / `Then the value is "2"`, with both steps defined and passing. The PRD key contains `And a defined step`. After normalization the JSON key contains `Given a defined step`. `executed.get(key)` is `None`, `problems` is non-empty, and `execution_failed` is false.
Validation suggestion: Add a passing self-test whose PRD and feature use `And` and `But` and assert `problems` is empty. Apply `.normalized()` only to the console writer, or expand `And`/`But` in the PRD keys with the same rule before comparing.
