The review package was cut off mid-diff. I'll read the omitted sections and then report only defects I can verify from that text.ID: G0-1
Severity: high
File:line: src/ids.rs:296
Description: `EmbeddingSpaceId::derive` emits `space-` plus the first 32 hex characters of the canonical-JSON SHA-256. `SpaceDescriptor` (src/embedding/mod.rs:205) also hashes `weight_format`, which PLAN §2.1 does not list. The §2.1 definition of `embedding_space_id` is the full SHA-256 of engine, engine version, model id, model revision, dimensions, normalized, metric, prefix policy, and preprocessing, with no prefix and no truncation. `revision_id` and `record_id` state their prefix and `[..16]` cut in that table; `embedding_space_id` does not. The parser at src/ids.rs:221 only accepts this shortened form.
Impact: Every space id this constructor produces disagrees with an implementation of §2.1. Later phases will freeze the wrong identity into paths, records, and experiment snapshots. Equal full digests will not compare equal, and a descriptor that differs only by `weight_format: null` still will not match the specified preimage.
Reproduction scenario or verifiable rationale: `ModelInfo::from_descriptor` always calls `EmbeddingSpaceId::derive`. The returned string is `space-` and 32 hex digits (`is_hex(h, 32)`). SHA-256 hex is 64 digits. Serializing `SpaceDescriptor` includes `weight_format`. No test asserts the digest against an independent full-hash oracle.
Validation suggestion: Hash the §2.1 field set with the same canonical JSON an external script uses, assert the full 64-hex digest, and accept that string in `FromStr`. Add `weight_format` to §2.1 only if it is an intentional part of the preimage, and freeze it in the same golden test.

ID: G0-2
Severity: medium
File:line: src/ids.rs:447
Description: `canonical_json_is_independent_of_key_insertion_order` builds two `serde_json::Map` values by inserting the same entries in opposite orders and asserts that `canonical_json` matches. With default serde_json features that map is a `BTreeMap`, so both collections are identical before `canonical_json` runs. Any pure function of the map passes, including deleting `entries.sort_by` at src/ids.rs:69. The unit test at src/ids.rs:356 checks one ASCII object whose `BTreeMap` order is already the expected order, and it does not turn on `preserve_order`, which is the case the sort comment cites.
Impact: A regression that stops sorting object keys still passes `cargo test` for the default feature set. If any dependency enables `serde_json/preserve_order`, key order starts changing fingerprints and these tests stay green.
Reproduction scenario or verifiable rationale: `serde_json::Map` preserves insertion order only with the `preserve_order` feature. That feature is not enabled in Cargo.toml, and the proptest never compares the output to a known encoding. Equal inputs in, equal outputs out, with the order already erased.
Validation suggestion: Build the object with a type that keeps insertion order (or enable `preserve_order` in the test), feed keys inserted as `b` then `a`, and assert the exact canonical string. Keep the NFC test; it does lock behavior.

ID: G0-3
Severity: medium
File:line: src/ids.rs:407
Description: PLAN §2.1 requires a golden-value unit test for each id. `revision_id_is_frozen_for_known_inputs` (src/ids.rs:375) freezes `rev-3d454748b6c6373a`. `record_id_depends_on_revision_and_space` only uses `assert_ne`. `equal_dimensions_do_not_imply_equal_spaces` (src/embedding/mod.rs:631) only uses `assert_ne` on `EmbeddingSpaceId`. No test freezes the hex of `RecordId` or `EmbeddingSpaceId`.
Impact: A change to the record preimage field names, the canonical-JSON rules, or the space truncation in G0-1 stays green as long as two different inputs still differ.
Reproduction scenario or verifiable rationale: Both tests call `derive` twice and compare the two results to each other. Neither test contains an expected digest computed outside `derive`.
Validation suggestion: Freeze one `record_id` and one `embedding_space_id` to digests computed by an independent script, the same way the revision golden was frozen.

ID: G0-4
Severity: medium
File:line: tests/bdd.rs:21
Description: Phase 0 item 2 requires the Cucumber `World` to hold a `TempDir` store, call logs, and fault flags. `LabWorld` is an empty `Default` struct. The comment on tests/bdd.rs:19 says each scenario gets an isolated store. `DeterministicEmbedder` can record calls and inject faults, and nothing on the `World` owns an embedder, a store directory, or those flags.
Impact: The milestone-0 run passes with no scenario state. The harness specified for later steps has no store isolation, no call log, and no fault flags to drive.
Reproduction scenario or verifiable rationale: `tests/bdd.rs` defines `pub struct LabWorld {}` and no `World::new` that creates a `TempDir`. `tests/features` contains only `.gitkeep`. A scenario cannot reach a store path, a call log, or a fault flag from the `World`.
Validation suggestion: Give `LabWorld` a `TempDir`, a call log, and fault flags, construct them in `World::new`, and add a temporary self-test scenario that reads those fields so they stay live under `-D warnings`.

ID: G0-5
Severity: medium
File:line: tests/bdd_selftest.rs:122
Description: Phase 0 item 3 says the self-test proves `fail_on_skipped`. The "undefined step fails" feature is `Given the value "2"` / `And an undefined step` / `Then the value is "2"`, while `PRD_BASIC` requires only the two steps without `And an undefined step`. `inventory::check` reports the required scenario as not executed because the step lists differ. `Report::passed` is false even if `.fail_on_skipped()` at tests/support/runner.rs:79 is removed and cucumber treats the undefined step as a non-failing skip. The failing-step and ambiguous-step cases do lock `execution_has_failed` for those outcomes; they use steps that resolve and fail. No case keeps the required step text and still skips.
Impact: CI can stay green after `fail_on_skipped` is dropped. A required scenario whose step text still matches the PRD, but whose step is skipped, is not what this case measures.
Reproduction scenario or verifiable rationale: Required key steps are `Given the value "2"` and `Then the value is "2"` (`PRD_BASIC`). The feature under test inserts a third step. `ScenarioKey` includes the full step list, so `executed.get(key)` is `None` and `problems` is non-empty regardless of the writer stats.
Validation suggestion: Add a case whose feature step text matches the PRD exactly, force a skipped step (undefined step with the same text the inventory expects, or a `@skip` scenario that still appears in the JSON), and assert failure only from `execution_has_failed` or from a step status other than `passed` while the key still matches.

ID: G0-6
Severity: medium
File:line: tests/support/inventory.rs:131
Description: `from_prd` skips an `Examples` block when `examples.table` is missing (line 131) or when the table has no data rows after the header (`split_first` then an empty `rows` loop, line 134). Both paths `continue` and return `Ok`. The module contract says a PRD scenario that cannot be resolved rejects the PRD. A milestone-tagged outline with a header and no rows adds nothing and does not error.
Impact: Deleting the data rows of a required outline shrinks the inventory and `from_prd` still succeeds. Updating `PRD_REQUIRED_PER_MILESTONE` in the same change keeps `bdd_selftest` green, so the required suite can shrink without a parse failure.
Reproduction scenario or verifiable rationale: For a block such as `Examples:` with only `| v |` and tag `@m1`, `table.rows.split_first` yields a header and an empty row slice, the insert loop does not run, and the function returns the remaining set.
Validation suggestion: `bail` when a milestone-qualified `Examples` block has no data rows or no table. Extend the self-test PRD with that block and assert `from_prd` returns an error.

ID: G0-7
Severity: medium
File:line: examples/gen_fixtures.rs:236
Description: Phase 0 item 5 requires generation to be deterministic and byte-identical. `render_scan` shells out to `pdftoppm -png -gray -r 150` and embeds that raster into `lab-scanned-002.pdf` and `us-mixed-003.pdf`. Poppler’s rasterizer and the host’s fonts decide those pixels. `lopdf` text pages and the pure-Rust PNG/JPEG drawings do not go through that tool. Nothing in CI regenerates the corpus and diffs it.
Impact: Two checkouts that run `cargo run --example gen_fixtures` on different Poppler builds or font configurations produce different PDF bytes and different `manifest.toml` checksums. A regeneration cannot be reviewed as a content change separate from renderer noise.
Reproduction scenario or verifiable rationale: `main` calls `render_scan` for both lab pages and for the ultrasound page, then `build_pdf` stores the PNG as an image XObject. `pdftoppm` is not pinned. The committed scanned PDF is 88835 bytes from whatever renderer produced it (progress records poppler 26.10.0). Ubuntu’s `poppler-utils` in the integration job is a different build.
Validation suggestion: Pin the raster bytes (checked-in page PNGs hashed in the manifest) and embed those files, or run generation in a pinned container and fail CI on a diff. Recompute each manifest checksum in a test either way.

ID: G0-8
Severity: medium
File:line: tests/integration.rs:18
Description: `required_external_tools_are_installed` returns immediately when `RUN_INTEGRATION` is not `1`, after printing `outstanding`. The function then ends successfully. PLAN §5.3 and PRD §5.7 say an integration test that did not run is outstanding and is not reported as passed. `#[ignore]` covers the default `cargo test` run. `--include-ignored` without `RUN_INTEGRATION=1` executes this body and cargo records `ok`.
Impact: A local or CI invocation that includes ignored tests and forgets the environment variable reports the tool probe as passed without probing pdfinfo, pdftotext, pdftoppm, pdfimages, or tesseract.
Reproduction scenario or verifiable rationale: `integration_enabled` is `var == "1"`. The unset path is `eprintln` then `return` inside `#[tokio::test]`. There is no `panic` or `Err` on that path. The test comment on line 5 says a test that did not run is never reported as passed.
Validation suggestion: On the unset path, panic with the outstanding message, or skip with an ignored/pending API that cargo does not count as passed. Assert the process status is not success.

ID: G0-9
Severity: medium
File:line: .github/workflows/embeddings-fundamentals.yml:91
Description: `integration-openai` exits 0 when `OPENAI_API_KEY` is empty: checkout, toolchain, and `cargo test` are skipped, and the last step echoes that OpenAI is outstanding. GitHub marks the job successful. When the secret is set, line 88 runs `cargo test --features engine-openai --test integration -- --include-ignored` and does not install poppler or tesseract. The only integration test probes those local binaries. No test constructs an OpenAI client or reads `OPENAI_API_KEY`.
Impact: With no secret, the OpenAI integration check is green without running. With a secret, the job fails on missing `pdfinfo` / `tesseract`, and a passing run would still only prove local tools exist. PRD §5.7 says an unexecuted integration is not reported as passed.
Reproduction scenario or verifiable rationale: Job `integration-openai` sets `OPENAI_API_KEY` from secrets. Every step except "Report outstanding" is guarded by `env.OPENAI_API_KEY != ''`. The outstanding step is `echo` and has no `exit 1`. `tests/integration.rs` calls `tools::probe(REQUIRED_TOOLS)` only.
Validation suggestion: Make the empty-secret job conclusion neutral or failing with an explicit outstanding status, and do not use a green check as the outstanding signal. When the secret exists, install nothing for poppler unless that probe is intentional, and run a test that fails if the OpenAI adapter is not actually invoked.

ID: G0-10
Severity: low
File:line: tests/fixtures/corpus/manifest.toml:10
Description: Each source row stores a `sha256:` checksum written by `examples/gen_fixtures.rs`, and no test or CI step reads the files back and compares them to `manifest.toml`. `cargo test` never opens `tests/fixtures/corpus`.
Impact: A replaced PDF or image can land with a stale checksum. Later phases will treat the manifest as the integrity gate for fixtures that do not match the bytes.
Reproduction scenario or verifiable rationale: The checksum assignments are the strings in `manifest.toml`. Grep of the Rust tests shows no `sha256:d68e3240` (or any other fixture digest) and no call that hashes `tests/fixtures/corpus`.
Validation suggestion: Add a test that walks `[[sources]]`, hashes each `file`, and asserts equality with `checksum`. Run it from the always-on `cargo test` job.

ID: G0-11
Severity: low
File:line: tests/bdd.rs:28
Description: `BDD_ARGS` is split with `split_whitespace`. That does not honor quotes. `CLAUDE.md` tells the user to pass `BDD_ARGS='--name "<regex>"'`. The value cucumber receives includes the quote characters as separate fields when the pattern contains spaces, so `--name` is not one regex.
Impact: A name filter whose scenario title contains a space does not select that scenario. The inventory check then fails the run. The harness-less binary cannot express the filter the docs describe. The self-test passes `--name` and `does-not-match` as an already-split `Vec`, so it does not exercise this parser.
Reproduction scenario or verifiable rationale: `std::env::var("BDD_ARGS").map(|args| args.split_whitespace()...)`. For `BDD_ARGS=--name "Hemoglobina 13,5"`, the iterator yields `--name`, `"Hemoglobina`, and `13,5"`. `cli::Opts::try_parse_from` binds `--name` to the quoted fragment.
Validation suggestion: Parse `BDD_ARGS` with a quote-aware splitter, and add a self-test argument list that includes a space inside `--name`.
