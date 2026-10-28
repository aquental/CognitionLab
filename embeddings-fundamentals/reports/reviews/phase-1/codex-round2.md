C1-1 CLOSED — The counting tokenizer disables truncation and padding and counts without special tokens.
C1-2 CLOSED — Revision reads verify the seal against the exact bytes returned, including experiment-scoped reads.
C1-3 CLOSED — Readers preserve staging; writer cleanup requires an exclusive OS lock.
C1-4 CLOSED — Ingestion and `validate` run the eligibility gate and return exit 3 before source processing.
C1-5 CLOSED — Contiguous qualitative rows following a header now remain rows.
C1-6 CLOSED — OCR reconstruction preserves wide horizontal gaps as column separators.
C1-7 CLOSED — Eligibility, per-source verification, and chunking now run inside `spawn_blocking`.

ID: C1-8
Severity: medium
File:line: src/chunking.rs:334
Description: A blank line clears the table header while preserving `in_table`. The qualitative-row correction requires both table context and a header, so a qualitative result after a blank line becomes a new header and is never emitted.
Impact: Successfully ingested tables can still lose laboratory results. Numeric rows after the same blank line also lose their original column header.
Reproduction scenario or verifiable rationale: Chunk `Exame | Resultado | Referência`, an empty line, and `Proteína | Negativo | Negativo`. The first line sets `header`, the blank clears it, and the last line becomes `TableHeader`. With no section heading, chunking returns zero chunks.
Validation suggestion: Add table tests with blank lines between the header and first row and between rows. Assert every qualitative result survives and every row retains the correct header.

ID: C1-9
Severity: medium
File:line: src/embedding/embed_anything.rs:221
Description: The adapter trusts the optional configured token limit without checking the loaded model’s actual limit. Omitting `max_input_tokens` disables checks entirely; setting it above the engine limit permits silent engine truncation despite PLAN §0.1’s explicit prohibition.
Impact: Records can retain full chunk text with `truncation: null` while their vectors represent only a prefix. Truncation-disabled ingestion can therefore succeed with unreported content loss.
Reproduction scenario or verifiable rationale: The cached pinned MiniLM configuration has `max_position_embeddings = 512`. EmbedAnything 0.7.1’s `BertEmbedder::new` installs tokenizer truncation at that value. Load this model with `max_input_tokens = None` and submit a paragraph exceeding 512 model tokens: the pipeline has no budget, the adapter skips its check, and the engine truncates. The updated full-length counter does not prevent this path.
Validation suggestion: Derive or validate a finite effective limit from the pinned model configuration, respecting the planned 128-token sentence-model limit. Test omitted and oversized configuration values and assert oversized input cannot reach inference without explicit truncation provenance.

ID: C1-10
Severity: low
File:line: src/chunking.rs:276
Description: `StructureAwareV1::new` accepts nonzero `overlap_tokens`, but chunk splitting never implements overlap. The pipeline nevertheless copies that configured value into every record at `src/pipeline.rs:652`.
Impact: Accepted experiment configurations and stored provenance claim overlap that did not occur, violating PRD §6.3’s requirement to record actual chunk overlap.
Reproduction scenario or verifiable rationale: Set `overlap_tokens = 5` and split a long paragraph without a section prefix using a small budget. Consecutive chunks contain disjoint words, while persisted records report five overlapping tokens. The only production uses of the setting are configuration, fingerprinting, and metadata assignment.
Validation suggestion: Reject nonzero overlap for this strategy unless implemented. Add a test asserting rejection or verifying actual shared tokens against recorded metadata.

🌱 graft saved ~24,552 tokens this turn, 1 call.