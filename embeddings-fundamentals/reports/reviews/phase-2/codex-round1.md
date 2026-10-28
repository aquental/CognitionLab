ID: C2-1  
Severity: high  
File:line: src/evaluation.rs:99  
Description: Relevance labels identify items solely by `ChunkId`, although that identifier is unique within a revision and can identify different content after chunking changes.
Impact: Configurations with different token budgets can receive relevance labels belonging to different text, producing incorrect retrieval metrics.
Reproduction scenario or verifiable rationale: Place a long paragraph before a labeled row on one page. A smaller budget splits the paragraph into additional chunks, shifting the row’s index. The evaluator still assigns the frozen label to the original chunk index without checking its revision, chunking configuration, or content.
Validation suggestion: Evaluate that fixture at two budgets and verify labels remain attached to the intended content. Bind judgments to stable content locations or explicitly validate their applicability to each chunking revision.

ID: C2-2  
Severity: medium  
File:line: src/experiment.rs:268  
Description: The experiment applies the supplied price table without checking its provider, model, or billing unit against the measured usage.
Impact: A valid but unrelated price table produces an `estimated` cost in the wrong units or at another model’s rate.
Reproduction scenario or verifiable rationale: Supply an OpenAI run with a table naming another model or `unit = "request"`. `PriceTable::load` accepts it, and `estimate` multiplies token usage by that table’s price.
Validation suggestion: Add experiment tests for provider, model, and unit mismatches. Reject incompatible tables or report cost as unavailable with the mismatch reason.

ID: C2-3  
Severity: medium  
File:line: src/cli.rs:537  
Description: Hardware is detected when generating the comparison report, rather than captured in each measured experiment.
Impact: Reports can attribute timings and memory measurements to the wrong machine, and runs from different machines are silently presented under one hardware description.
Reproduction scenario or verifiable rationale: Generate experiment JSON on machine A, copy it to machine B, and invoke `report`. `ConfigurationResult` contains no hardware provenance; the report records machine B’s hardware.
Validation suggestion: Capture hardware in the experiment result. Test report regeneration with different reporting-host hardware and verify original provenance is preserved and heterogeneous measurement hosts are identified.

ID: C2-4  
Severity: medium  
File:line: src/experiment.rs:168  
Description: The evaluation reuses the ingestion snapshot without recording query and label checksums in that create-only experiment snapshot, contrary to PLAN §4.2.
Impact: The stored experiment ID does not identify the complete evaluation inputs needed for reproducible regeneration.
Reproduction scenario or verifiable rationale: `pipeline.ingest` receives only the manifest and ingestion settings. The dataset is used afterward by `evaluate_text`; the remaining experiment code never writes its checksums to the snapshot. Checksums appear only in the returned report.
Validation suggestion: Inspect the persisted experiment JSON in a regression test and require its query/label checksums to equal the evaluated dataset. Arrange snapshot creation so those inputs are included before the create-only write.

ID: C2-5  
Severity: medium  
File:line: src/retrieval.rs:242  
Description: The new public `search_vector` path bypasses query-vector finiteness validation. It checks space identity and dimensions but accepts NaN and infinity.
Impact: Invalid vectors reach similarity scoring and sorting instead of producing a validation error, allowing meaningless rankings and potentially non-finite scores.
Reproduction scenario or verifiable rationale: Against a populated index, supply its correct space ID and a correctly sized vector containing `f32::NAN`. Unlike `search_text`, this path never calls vector validation before `rank`.
Validation suggestion: Add matching-space tests for NaN and positive/negative infinity and require `InvalidQueryVector` before scoring.

ID: C2-6  
Severity: medium  
File:line: src/experiment.rs:216  
Description: Extraction fidelity is evaluated against stored, potentially truncated chunks rather than the extraction output.
Impact: Changing an embedding budget or truncation policy changes the reported extraction accuracy even when extraction is identical and completely correct.
Reproduction scenario or verifiable rationale: Correctly extract a sampled laboratory row, then ingest with truncation enabled and a budget that removes its reference range. `fidelity::evaluate` reads the truncated `chunk_text` and counts the missing range as an extraction mismatch.
Validation suggestion: Run identical extraction with two chunking budgets and require identical extraction-fidelity results. Evaluate captured page output before chunking, and report chunk preservation separately if needed.

ID: C2-7  
Severity: medium  
File:line: src/report.rs:325  
Description: Markdown renders only `evaluation.status`, which is always `Completed`, and omits both `run_status` and `failed_sources`.
Impact: An incomplete ingestion can appear as a completed experiment without any visible failure indication.
Reproduction scenario or verifiable rationale: Make one required source fail during chunking while another succeeds. Evaluation can complete over the available snapshot, while failed pages, failed embedding batches, retries, and truncations remain zero. The Markdown status still reads `Completed`.
Validation suggestion: Generate Markdown from an incomplete run caused by a chunking failure and assert that the incomplete status and failed-source count are explicitly displayed.

ID: C2-8  
Severity: medium  
File:line: src/embedding/fastembed.rs:187  
Description: Tokenizer loading and model-limit file reads execute directly inside the async loader. `HfTokenCounter::load` performs synchronous filesystem reads, tokenizer deserialization, and tokenization outside `spawn_blocking`, violating PLAN §2.3 and `async-spawn-blocking`.
Impact: Loading a large tokenizer blocks a Tokio worker and delays unrelated async work. The EmbedAnything loader follows the same pattern.
Reproduction scenario or verifiable rationale: After awaiting the blocking download task, the loader directly calls `HfTokenCounter::load` and `hf::effective_limit`; only subsequent session construction returns to the blocking pool.
Validation suggestion: Move tokenizer/config loading into the blocking task. Verify a concurrent timer remains responsive while loading a large tokenizer on a single-worker runtime.

ID: C2-9  
Severity: low  
File:line: src/experiment.rs:57  
Description: `Instruments` uses `Arc<dyn Clock>` and `Arc<dyn MemoryProbe>`, and `MeasuredEmbedder` also stores a dynamic clock, despite PLAN §2.2 explicitly requiring generic static dispatch for these components.
Impact: Phase 2 introduces an undocumented deviation from the required dispatch and ownership design.
Reproduction scenario or verifiable rationale: Both instrument fields are trait objects; neither the experiment nor the measuring decorator is generic over these replacement components.
Validation suggestion: Parameterize the instruments and measuring decorator over their concrete probe types, then compile and run the existing production and fixture callers.