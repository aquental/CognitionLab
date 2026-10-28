C2-1: CLOSED — `evaluate` now checks anchors before assigning judgments to returned text hits; `evaluate_text` also checks labeled chunks across the index.

C2-10: CLOSED — The original failed-labeled-source case now produces `IncompleteExperiment` before query-side work, and the CLI writes it and exits 2.

C2-11: CLOSED — Exact name matches are reserved before value fallback, preventing the reported fallback from stealing a later name match.

C2-12: CLOSED — Hardware subprocesses now use `tokio::process`, a timeout, and `kill_on_drop(true)`; `/proc` reads use asynchronous filesystem operations.

ID: C2-13  
Severity: high  
File:line: src/embedding/fastembed.rs:173  
Description: The FastEmbed loader treats any supplied revision string as an immutable pin. Values such as `"main"` pass validation, are used for downloads, and are stored unchanged in the embedding-space descriptor.  
Impact: A moving branch can produce different model weights under the same embedding-space identity. Existing document vectors can then be queried with incompatible new weights, while unchanged-source detection prevents re-embedding.  
Reproduction scenario or verifiable rationale: Configure `model_revision = "main"` and ingest a source. After that branch changes, rebuild the adapter with the same configuration. The descriptor still contains `"main"` and therefore derives the same space ID, although downloaded model files can differ. The loader checks only whether the revision is present. This violates the immutable-revision requirement documented by the adapter.  
Validation suggestion: Require immutable commit identifiers, or resolve symbolic revisions once and use the resolved commit for every download and the space descriptor. Test that moving a branch cannot change model contents without changing the recorded revision and space ID.

ID: C2-14  
Severity: medium  
File:line: src/experiment.rs:256  
Description: An incomplete ingestion still loses its report when labels remain applicable and subsequent query-side work fails. Only `LabelNotApplicable` is converted into `EvaluationNotRun`; adapter-load, warm-up, evaluation, and latency errors propagate without carrying the ingestion outcome.  
Impact: A failed required source can still result in exit 1 and no report, contrary to PLAN §4.3. The experiment snapshot already exists, preventing a straightforward retry with the same experiment ID.  
Reproduction scenario or verifiable rationale: Change the existing `a_failed_unlabeled_source_still_completes_the_evaluation` test to pass `fail_query_side = true`. Its image-only labels pass `check_text_labels`, then the second `build().await?` returns `ExperimentError::Build`. The CLI immediately returns `build_failure`, without writing the incomplete ingestion report.  
Validation suggestion: Preserve the ingestion outcome for subsequent failures whenever ingestion is incomplete. Add coverage for an unlabeled required-source failure combined with second-load and query failures; require a written incomplete report and exit 2.

ID: C2-15  
Severity: medium  
File:line: src/cli.rs:473  
Description: The new asynchronous experiment command directly calls synchronous `FsRecordStore::open_writer`, including filesystem setup and abandoned-staging cleanup. The new report command similarly performs synchronous file reads and writes directly from asynchronous `cli::run`. This violates PLAN §2.3 and `async-spawn-blocking`.  
Impact: Potentially lengthy store cleanup and report I/O block a Tokio worker instead of yielding, delaying other runtime work.  
Reproduction scenario or verifiable rationale: `experiment` calls `FsRecordStore::open_writer` without `spawn_blocking`; `Command::Report` directly invokes the synchronous `report` function, which uses `std::fs::read_to_string` and `std::fs::write`. Neither path delegates blocking work.  
Validation suggestion: Move store opening and cleanup into `spawn_blocking`; use asynchronous filesystem operations or a blocking task for report processing. Verify runtime responsiveness with controlled delayed filesystem work.

ID: C2-16  
Severity: medium  
File:line: src/fidelity.rs:271  
Description: Exact-name matching remains greedy and sample-order dependent when multiple sample rows on a page share a test name. Such samples are accepted without validation, and the first row claims the first matching extracted line regardless of its value.  
Impact: Missing or repeated laboratory rows can receive another row’s fields, and rearranging the same fidelity sample changes reported accuracy.  
Reproduction scenario or verifiable rationale: Supply sample rows `A | 1 | u | r` and `A | 2 | u | r`, with only `A | 2 | u | r` extracted. In that sample order, the first row claims the line and scores three correct fields; reversing the sample scores four. Both runs evaluate identical expected and extracted content, yielding different overall accuracy.  
Validation suggestion: Reject ambiguous sample identities or implement matching that resolves repeated names without depending on sample order. Add the described permutation test and require identical results without assigning a missing row another row’s fields.