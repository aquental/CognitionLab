The review package was truncated in the message, so I am reading the rest of the inlined diff before judging any finding.The core modules are in. Next I am reading the pipeline, store, retrieval, and the acceptance steps.ID: G1-1
Status: CLOSED
The active-pointer compare runs before extraction, and `unchanged_sources_are_recognised_before_extraction` fails the run if OCR is invoked.

ID: G1-2
Status: CLOSED
An over-budget paragraph piece now returns `ChunkError::InputTooLong` or a recorded truncation, and `an_indivisible_paragraph_fails_or_is_truncated_like_a_row` covers budget 1.

ID: G1-3
Status: CLOSED
`publish` reuses a revision only when every `records.jsonl` line parses, quarantines a missing or unparseable file, leaves `records_committed` at 0 on reuse, and PLAN §4.1 step 2 matches that rule.

ID: G1-4
Status: CLOSED
Any existing experiment snapshot file is refused before processing, and `ValidationError::Store`, `Task`, and `Fingerprint` map to exit 1.

ID: G1-5
Status: CLOSED
`prefix_policy` is stored on the space descriptor, `input_budget` subtracts the prefix, and `embed_text` applies that policy before inference.

ID: G1-6
Status: CLOSED
Pipeline snapshot, pointer, active, stage, and publish calls go through `spawn_blocking`.

ID: G1-7
Status: CLOSED
Parse failures, PDF and image XMP element names, EXIF tag names, PNG keywords, and a JPEG APP13 marker are reported instead of an empty list.

ID: G1-8
Severity: high
File:line: embeddings-fundamentals/src/pipeline.rs:498
Description: A matching active pointer returns `Succeeded` with `unchanged` before extraction and before `publish`. The pointer's `records_checksum` is ignored, and `revisions/<source>/<revision>/records.jsonl` is never opened. The quarantine-and-replace path in `publish` therefore never runs for a re-ingest of the revision the pointer already names.
Impact: After the records file is deleted, emptied, or corrupted, a later ingest exits 0, reports `succeeded (unchanged)`, and makes no embedder calls. `search` then fails on the missing or corrupt file. The active pointer keeps the broken revision permanently, because the heal logic is unreachable on this path.
Reproduction scenario or verifiable rationale: Ingest a source so the active pointer equals the derived revision. Delete `records.jsonl` or replace it with `not json\n`. Ingest the same manifest under a new experiment id. The source is `Succeeded`, `unchanged` is set, and `publish` is not called. `records()` / `search_text` then returns a store error. `reuse_reports_the_existing_records_and_damaged_revisions_heal` calls `publish` directly, so it does not cover this retry.
Validation suggestion: With the pointer already equal to the derived revision, delete `records.jsonl` and replace it with unparseable bytes. The next ingest must fail the source or quarantine and reinstall, and a following search must return the new records. An intact revision must still skip with `attempts == 0` and no extractor calls.

ID: G1-9
Severity: medium
File:line: embeddings-fundamentals/src/storage.rs:329
Description: On a damaged revision, `publish` renames the live revision directory to `tmp/<stage>.stale` before `install`. `install` can still fail at `FaultPoint::BeforeRevisionRename` or on the replacement rename. The active pointer is written only after `install`. `FsRecordStore::open` then deletes the whole `tmp/` tree (storage.rs:173), including the quarantine and the staged replacement.
Impact: A fault or crash in that window removes the only remaining bytes while the pointer, or an experiment snapshot, still names the old path. Combined with G1-8, the next process start reports `succeeded (unchanged)` and never republishes. `a_fault_at_any_point_keeps_the_previous_revision_active` publishes a new revision id, so it does not cover a damaged existing directory.
Reproduction scenario or verifiable rationale: Point an active pointer at a revision whose `records.jsonl` is `not json`, move that pointer aside or call `publish` for the same revision id, and inject `FaultPoint::BeforeRevisionRename`. The revision directory is already gone, the staged tree sits under `tmp/`, and the next `open` removes `tmp/`. A later ingest whose derived id still matches the pointer skips.
Validation suggestion: A failed install must leave either the old directory or the staged replacement outside the tree that `open` deletes. Add a fault test on the damaged-revision path and assert the pointer still resolves to parseable records after `open` plus a retry.

ID: G1-10
Severity: medium
File:line: embeddings-fundamentals/src/chunking.rs:159
Description: `classify` marks a line as a heading when it has at most four words, at most 40 characters, starts with an uppercase letter, and contains no ASCII digit, `|`, or `:`. A heading is not emitted as its own chunk. Two consecutive headings, or a heading that is the last content on the page, produce no chunk for that text.
Impact: Short body sentences are omitted from the index while the source still succeeds. A conclusion such as `Exame normal.` is not searchable. `paragraphs_carry_their_section` uses a longer sentence that contains a digit, so this path stays green.
Reproduction scenario or verifiable rationale: Chunk a page whose lines are `Conclusão` and `Exame normal.` with `WhitespaceTokenCounter` and a budget above 2. Both lines match the heading rule. `chunk` returns `Ok` with an empty list.
Validation suggestion: A page whose only body is a short capitalized sentence must yield a paragraph chunk containing that sentence. A short line that is followed by a body may still be stored only as that body's section prefix.

ID: G1-11
Severity: low
File:line: embeddings-fundamentals/src/cli.rs:270
Description: `ingest`, `validate`, and `search` print every `adapters::build` error as `validation failed` and return exit 3. `BuildError::Load` covers weight, tokenizer, and dimension-probe failures. PLAN Phase 1 item 8 reserves exit 3 for a validation refusal and exit 1 for an internal error. G1-4 changed only `ValidationError::exit_code`.
Impact: A missing cache file or a failed model load is reported as a refused corpus. Automation that treats exit 3 as a manifest problem will not retry or surface an internal failure.
Reproduction scenario or verifiable rationale: Run `ingest` with `engine-embed-anything` when `tokenizer.json` or the pinned weights are absent. The process prints `validation failed` and exits 3 before `pipeline::ingest`.
Validation suggestion: Map load and runtime failures to exit 1 with the `error` label. Keep `EngineNotCompiled`, `MissingRevision`, and remote-permission refusals on exit 3. Assert a load error does not print `validation failed`.

ID: G1-12
Severity: low
File:line: embeddings-fundamentals/src/retrieval.rs:116
Description: `search_text` calls `store.active`, `load_experiment`, and `store.records` directly from an async function. `cli::search` does the same for `FsRecordStore::open` and `load_experiment`. `storage.rs` states that async callers use `spawn_blocking` because directory fsync and record reads are blocking. This violates PLAN §2.3 and `async-spawn-blocking`. The pipeline sites fixed for G1-6 do not cover retrieval.
Impact: Parsing `records.jsonl` blocks a Tokio worker for the duration of the read. Search results stay the same, and a stall delays other tasks on that runtime.
Reproduction scenario or verifiable rationale: `search_text` reaches `store.active` and `store.records` with no `spawn_blocking`. `cli::search` awaits those calls on the runtime worker.
Validation suggestion: Run those store calls through the same `spawn_blocking` wrapper as `on_store`, and keep the space-guard test on that path.

ID: G1-13
Severity: low
File:line: embeddings-fundamentals/src/anonymization.rs:218
Description: JPEG IPTC is reported only as `JPEG APP13 (IPTC/Photoshop) block present` when the bytes contain `Photoshop 3.0`. No IPTC dataset names are read. The XMP scan (anonymization.rs:73) keeps only element tags of the form `<prefix:name`, so an `rdf:Description` attribute such as `dc:creator="..."` is not listed. PLAN §4.4 requires the anonymization CLI to report image IPTC and XMP keys. `jpeg_xmp_and_iptc_are_reported_by_key` and `pdf_xmp_keys_and_info_keys_are_listed` use an element-form XMP packet and the APP13 marker, so both gaps pass.
Impact: A reviewer can record metadata as reviewed while the report never names the IPTC datasets or attribute-form XMP keys present in the file.
Reproduction scenario or verifiable rationale: Point `image_findings` at a JPEG whose IPTC record contains a by-line and whose XMP packet sets `dc:creator` only as an attribute. The findings are the APP13 sentence and `XMP packet present (no element names found)`.
Validation suggestion: List IPTC dataset names and XMP attribute names, keys only. Add a fixture that must contain those names and must not contain the values.
