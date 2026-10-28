ID: C1-1
Severity: medium
File:line: src/embedding/embed_anything.rs:96
Description: `HfTokenCounter` retains the truncation configuration loaded from `tokenizer.json`. Consequently, `count()` measures truncated input rather than the original token count.
Impact: Truncation provenance and input-limit errors underreport original lengths. Configuring a budget above the tokenizer’s truncation ceiling also allows oversized inputs through the adapter’s checks.
Reproduction scenario or verifiable rationale: The pinned MiniLM tokenizer contains `truncation.max_length = 128`. With that cached tokenizer, `"Hemoglobina | 13,5 | g/dL | "` followed by 300 repetitions of `"palavra "` counts as 128 tokens; disabling tokenizer truncation gives 316. The production counter uses the former behavior.
Validation suggestion: Disable truncation and padding on the counting tokenizer. Test original counts above 128 and rejection or explicit chunking of inputs exceeding the configured budget.

ID: C1-2
Severity: high
File:line: src/storage.rs:558
Description: Revision reads bypass `revision.json` entirely. The integrity seal is checked during ingestion and publication, but `records()` only parses individual JSON lines.
Impact: Search silently serves incomplete or altered records, including from saved experiments, until another ingestion detects the damage. An emptied records file appears to be a legitimate empty index.
Reproduction scenario or verifiable rationale: Publish a multi-record revision, truncate `records.jsonl` to one complete line or zero bytes, then call `search_text` with either active or experiment scope. Both paths call `records()` without checking the seal and return successful partial or empty results.
Validation suggestion: Verify the seal against the exact bytes being read before returning records. Test truncation and valid-JSON alterations through both search scopes, without intervening ingestion.

ID: C1-3
Severity: medium
File:line: src/storage.rs:183
Description: Every `FsRecordStore::open` deletes all staging directories without establishing that their writers have stopped. CLI search also calls this destructive open operation.
Impact: Starting a search or another ingestion can destroy an ongoing ingestion’s staged revision and cause publication to fail.
Reproduction scenario or verifiable rationale: Open store A and successfully call `stage()`. Before publishing its returned handle, open store B on the same root. B removes A’s staging directory; A’s subsequent `publish()` fails because its rename source no longer exists. No concurrent threads are necessary to reproduce this interleaving.
Validation suggestion: Separate read-only opening from recovery cleanup and protect recovery with writer ownership or locking. Test that opening a reader between staging and publication preserves the staged revision.

ID: C1-4
Severity: medium
File:line: src/pipeline.rs:477
Description: Eligibility and anonymization failures become ordinary source failures rather than validation refusals. Other sources may already have been published, and the run returns exit 2—or 0 for an optional rejected source—instead of the exit 3 required by PLAN §4.4. The CLI `validate` command never checks source eligibility or checksums.
Impact: Validation can approve an unreviewed corpus, and ingestion does not provide the specified refusal behavior before processing.
Reproduction scenario or verifiable rationale: A read-only invocation of the existing CLI with a deterministic configuration and a manifest containing an eligible PDF entry, no anonymization review, and a missing file returned exit 0 from `validate`. During ingestion, the same missing review reaches `report.fail(FailureStage::Manifest, ...)`, not `ValidationError`.
Validation suggestion: Share eligibility validation between `validate` and ingestion preflight. Test missing anonymization checks with both required and optional entries, asserting the specified exit code and that no extraction, embedding, or publication occurs before refusal.

ID: C1-5
Severity: medium
File:line: src/chunking.rs:150
Description: Every table-shaped line without an ASCII digit is classified as a header. Qualitative laboratory results are therefore discarded or replace the real header instead of becoming searchable rows.
Impact: Successfully ingested documents can omit laboratory findings, violating the row-preservation requirement.
Reproduction scenario or verifiable rationale: Chunk the lines `Exame | Resultado | Referência` and `Proteína | Negativo | Negativo`. Both become `TableHeader`; neither produces a unit, and chunking returns an empty list. Multiple qualitative rows overwrite each other.
Validation suggestion: Distinguish headers from data using table context rather than digit presence alone. Test qualitative rows at the end of a table and between numeric rows, asserting that every result survives with the correct header.

ID: C1-6
Severity: medium
File:line: src/extraction/tsv.rs:111
Description: OCR reconstruction joins every word with one space, discarding horizontal column gaps. The chunker recognizes whitespace-aligned tables only through runs of at least two spaces.
Impact: Scanned tables without printed pipe separators lose their row structure before chunking. Their rows become ordinary paragraphs, can merge together, and may be split independently of their headers.
Reproduction scenario or verifiable rationale: Supply TSV words for an aligned row containing `Hemoglobina`, `13,5`, `g/dL`, and `12,0–16,0`, with large horizontal gaps between columns. Parsing produces `Hemoglobina 13,5 g/dL 12,0–16,0`; `cells()` finds one cell and classifies it as text. Existing OCR fixtures use literal pipes and do not exercise this path.
Validation suggestion: Preserve column boundaries from OCR geometry or pass structured cells to chunking. Add a TSV-to-chunker test for a table without pipes and assert one header-attached chunk per row.

ID: C1-7
Severity: low
File:line: src/pipeline.rs:477
Description: `process_source` synchronously reads and hashes the entire source through `manifest.verify()` on the async runtime worker. Its subsequent page-chunking loop also performs potentially substantial tokenization synchronously. These paths violate PLAN §2.3 and rust-skills rule `async-spawn-blocking`.
Impact: Large PDFs and long extracted paragraphs can stall a Tokio worker and delay unrelated tasks. Moving store operations into `spawn_blocking` did not cover these operations.
Reproduction scenario or verifiable rationale: `LoadedManifest::verify` directly calls `std::fs::read` and checksum computation; line 477 invokes it without a blocking boundary. Lines 537–545 similarly call the synchronous chunker and model token counter directly.
Validation suggestion: Offload source verification and substantial chunking work to the blocking pool. Verify runtime responsiveness while an injected slow verification or token-counting operation runs.

🌱 graft saved ~28,796 tokens this turn, 1 call.