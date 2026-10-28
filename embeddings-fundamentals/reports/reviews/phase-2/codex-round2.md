| ID | Closure | Reason |
|---|---|---|
| C2-1 | NOT CLOSED | `evaluate_text` checks anchors, but public `evaluate` still assigns judgments solely by `ChunkId`, accepting hits with unrelated text. |
| C2-2 | CLOSED | Cost estimation checks the billed provider, model, and token unit before applying a price. |
| C2-3 | CLOSED | Results retain measurement-time hardware; comparison construction uses it and rejects differing hardware. |
| C2-4 | CLOSED | Experiment ingestion persists query and label versions and checksums in the create-only snapshot. |
| C2-5 | CLOSED | `search_vector` rejects non-finite components before reading the index. |
| C2-6 | CLOSED | Fidelity now consumes extracted pages independently of stored chunks and truncation. |
| C2-7 | CLOSED | Markdown displays ingestion status and failed-source count. |
| C2-8 | CLOSED | Both local loaders perform tokenizer deserialization and configuration reads inside `spawn_blocking`. |
| C2-9 | CLOSED | `Instruments` and `MeasuredEmbedder` now use generic clock and memory-probe types. |

ID: C2-1  
Severity: medium  
File:line: src/evaluation.rs:492  
Description: The public `evaluate` operation bypasses the label-applicability safeguard. It retrieves each judgment using only the query and chunk IDs, without checking the supplied hit’s text against the label’s anchor.  
Impact: Callers can still obtain completed reports assigning frozen relevance judgments to different text after a chunking change—the original correctness defect remains reachable through a public evaluation interface.  
Reproduction scenario or verifiable rationale: Supply a valid dataset labeling `s-p1-text-0` with anchor `hemoglobina`, then call `evaluate` with a `SearchHit` having that chunk ID but containing only a hematocrit row. The lookup assigns the hemoglobin judgment and computes metrics successfully. The check in `evaluate_text` is never invoked.  
Validation suggestion: Add a regression through public `evaluate` using a mismatched anchor. Require refusal, or make unchecked scoring private and expose evaluation only through an interface that guarantees applicability checks.

ID: C2-10  
Severity: medium  
File:line: src/experiment.rs:237  
Description: Failure of a required source containing text labels aborts the experiment through `LabelNotApplicable`, discarding the incomplete ingestion result. This contradicts PLAN §4.3’s incomplete-run reporting and exit-code contract.  
Impact: The experiment command exits through its generic error path instead of returning exit 2 and writing the ingestion status and reliability counters. The Markdown regression test fabricates an incomplete result and therefore does not exercise this failure.  
Reproduction scenario or verifiable rationale: Use a fresh store and inject an extraction or embedding failure for a labeled PDF while other sources succeed. Ingestion returns an incomplete report, but its snapshot lacks the failed source’s chunks. `evaluate_text` rejects the missing labeled chunk; `?` exits `experiment::run` before constructing `ConfigurationResult`. The CLI consequently never writes that result or reaches its `RunStatus::Incomplete` exit mapping.  
Validation suggestion: Exercise an actual failed labeled source through the experiment command. Assert exit 2, a persisted report with failed-source counters, and an explicit indication that evaluation could not complete.

ID: C2-11  
Severity: medium  
File:line: src/fidelity.rs:272  
Description: Value fallback can claim a line that exactly matches a later sample row’s name. Name matching takes priority only within the current iteration, so the global `used` set makes fidelity depend on sample ordering.  
Impact: A correctly extracted row can be reported missing, while its fields are credited to a different, missing row. Reported field accuracy is incorrect.  
Reproduction scenario or verifiable rationale: Define sample rows `A` and `B`, both expecting value `92`, unit `mg/dL`, and range `70–99`. Provide only `B | 92 | mg/dL | 70–99`. With A first, A claims B through the unique-value fallback and scores three fields; B then scores zero because the line is used. Reversing sample order scores four fields for B and zero for A. Identical extraction therefore yields different accuracy.  
Validation suggestion: Add this permutation regression and require four correct fields in either order. Reserve exact-name matches across the page before assigning fallback matches.

ID: C2-12  
Severity: medium  
File:line: src/measurement.rs:437  
Description: Hardware detection executes synchronous `std::process::Command::output()` calls directly from the async experiment command. These subprocesses also have no timeout or cancellation handling, contrary to PLAN §2.3 and `async-spawn-blocking`.  
Impact: On macOS, hardware detection blocks a Tokio worker while waiting for `sysctl`; a stalled subprocess prevents the experiment command from progressing indefinitely.  
Reproduction scenario or verifiable rationale: `cli::experiment` constructs `Instruments` by calling `Hardware::detect()` directly. On macOS, that function synchronously waits for two subprocesses. Neither call uses the blocking pool nor the required timed `tokio::process` execution.  
Validation suggestion: Move subprocess detection to async process execution with a timeout and `kill_on_drop(true)`. Test a delayed probe on a single-worker runtime and verify that a concurrent timer remains responsive and detection terminates within its timeout.