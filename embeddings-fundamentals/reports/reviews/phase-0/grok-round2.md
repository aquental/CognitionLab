The review package was cut off mid-diff. I'll read the omitted sections and then report only defects I can verify from that text.G0-1 CLOSED. PLAN §2.1 now specifies `space-` plus 32 hex digits and `weight_format`, and `embedding_space_id_is_frozen_for_known_descriptor` freezes that digest.
G0-2 CLOSED. `serde_json/preserve_order` is a dev-dependency, keys inserted `b` then `a` must encode as `{"a":2,"b":1}`, and the proptest checks an independently built sorted string.
G0-3 CLOSED. `record_id_is_frozen_for_known_inputs` and `embedding_space_id_is_frozen_for_known_descriptor` freeze both digests.
G0-4 CLOSED. `LabWorld` owns a `TempDir`, a `DeterministicEmbedder`, and the last outcome, and `harness.feature` exercises the store, the call log, and fault flags.
G0-5 CLOSED. The new case keeps PRD-identical step text, leaves one step undefined, and requires `execution_failed == true`.
G0-6 CLOSED. `from_prd` returns an error when an in-milestone `Examples` block has no table or no data rows, and the self-test asserts that rejection.
G0-7 CLOSED. Scan rasters are checked in, regeneration embeds those files, and CI diffs a poppler-free regenerate against the corpus.
G0-8 CLOSED. With ignored tests included and `RUN_INTEGRATION` unset, the test asserts and is recorded as failed.
G0-9 CLOSED. The workflow has no OpenAI job, and PLAN §6 moves that job to Phase 2 behind a repository variable.
G0-10 CLOSED. `tests/fixtures.rs` hashes every manifest source on the always-on `cargo test` path.
G0-11 CLOSED. `split_args` keeps quoted arguments together, and the self-test covers a space inside `--name`.

ID: G0-12
Severity: medium
File:line: tests/support/inventory.rs:65
Description: `gherkin_blocks` returns only fences closed by a ` ``` ` line. If the file ends while a fence is still open, that block is discarded and `from_prd` still returns `Ok` whenever another block was closed. The milestone-count check only compares `from_prd`’s length to `PRD_REQUIRED_PER_MILESTONE`, so updating that constant hides the dropped scenarios.
Impact: The last PRD Gherkin block can disappear from the required inventory without a parse error. Later milestones then pass while those scenarios never run.
Reproduction scenario or verifiable rationale: Two fenced blocks, the second missing its closing fence, make `blocks` contain only the first. `from_prd` does not see `current == Some(...)` and does not bail. `check_real_prd` stays green if the expected count is lowered to the shortened set.
Validation suggestion: After the scan, `bail` when `current` is still `Some`. Add a self-test fixture with a closed block followed by an unclosed one and assert `from_prd` returns an error.

ID: G0-13
Severity: medium
File:line: src/ids.rs:365
Description: `canonical_json_normalizes_strings_to_nfc` only asserts that a decomposed string and a composed string encode equally. NFC, NFD, and NFKC all make that pair equal. No golden contains a non-ASCII character. `revision_id_is_frozen_for_known_inputs` and `embedding_space_id_is_frozen_for_known_descriptor` use ASCII only.
Impact: Replacing `.nfc()` with `.nfd()` keeps `cargo test` green and changes every fingerprint that includes Portuguese text.
Reproduction scenario or verifiable rationale: The test inputs are `"Hemoglobina e\u{301}"` and `"Hemoglobina \u{e9}"`. `UnicodeNormalization::nfd` maps both to the same bytes, so `assert_eq!` still holds. Nothing asserts the JSON text is the composed form.
Validation suggestion: Assert the decomposed input encodes as the JSON string of the composed wording, and freeze one revision or space preimage that contains `é`.

ID: G0-14
Severity: medium
File:line: src/embedding/deterministic.rs:435
Description: `vectors_are_deterministic_and_normalized` calls `embed_documents` twice on the same text and compares the two batches, then checks the norm. No test requires two different texts to differ, and no expected component is an independently computed SHA-256 value. `overrides_freeze_vectors_for_fixtures` only covers the override map.
Impact: `vector_for` can ignore the text and return one constant unit vector, and the Phase 0 determinism test still passes. Later retrieval fixtures would lock a hash that was never checked.
Reproduction scenario or verifiable rationale: A unit vector that does not depend on `key` satisfies `assert_eq!(a, b)` and a norm of 1. The other embedder tests assert call-log strings, error classes, or space ids, not vector bytes.
Validation suggestion: Freeze the first components for a known text from an independent SHA-256 script, and assert that a second text produces a different vector.

ID: G0-15
Severity: medium
File:line: .github/workflows/embeddings-fundamentals.yml:45
Description: PLAN §6 step 4 requires uploading `reports/bdd/cucumber.json` together with the corpus and configuration fingerprints. PRD §5.7 requires scenario results to be published with those fingerprints. The upload is only `reports/bdd/`, and `run_suite` never computes or writes a corpus or config fingerprint.
Impact: A green BDD artifact cannot be tied to the fixture bytes or to `config/acceptance.toml`. A later corpus change is not visible on the run that is kept.
Reproduction scenario or verifiable rationale: The workflow’s “Upload BDD report” step sets `path: embeddings-fundamentals/reports/bdd/`. `runner.rs` writes `cucumber.json` and returns counts and inventory problems only. No fingerprint file is created.
Validation suggestion: Hash the manifest and the acceptance config, write those digests next to `cucumber.json`, and include them in the uploaded artifact. Assert the digests in `bdd_selftest`.

ID: G0-16
Severity: low
File:line: src/tools.rs:62
Description: `probe` treats `Ok(Ok(output))` as success whenever stdout or stderr has a line. It never reads `output.status`. `doctor` and `required_external_tools_are_installed` both treat `version: Some(_)` as installed.
Impact: A binary that exits non-zero and still prints a line is reported `ok`, and the integration probe passes. A broken poppler or tesseract install is not outstanding.
Reproduction scenario or verifiable rationale: The `Ok(Ok(out))` arm selects stderr only when stdout is empty, then takes `lines().next()`. There is no `out.status.success()` check. A non-zero exit with `pdfinfo: error` on stderr yields `Some("pdfinfo: error")`.
Validation suggestion: Set `version` only when `out.status.success()` is true and the chosen stream has a non-empty line. Add a test double that exits 1 with stderr and assert `version` is `None`.

ID: G0-17
Severity: low
File:line: src/embedding/mod.rs:473
Description: When the batch length matches `expected_items` but the ids are missing or reordered, `validate_batch` still returns `VectorDefect::ItemMismatch` with those two equal lengths. The variant has no `item_id`. PLAN §4.3 requires the defect report to name the item.
Impact: A swapped embedding is rejected with `expected 2 items, got 2`. The caller cannot see which id was wrong, and a count mismatch is not distinguishable from an identity mismatch.
Reproduction scenario or verifiable rationale: `missing_or_reordered_items_are_rejected` reverses two ids, keeps both vectors of length 2, and expects `ItemMismatch { expected: 2, actual: 2 }`. `ids_match` is false only because the zip comparison failed. The error fields are the two lengths.
Validation suggestion: Return an item-bearing defect that names the expected id and the id found at that index. Assert that message in the reorder test.
