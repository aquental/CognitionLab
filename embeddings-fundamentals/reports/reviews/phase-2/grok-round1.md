The review package was truncated in the message. I am reading the rest of the Phase 2 diff before judging findings.The report JSON is evidence, not the contract. I am reading the Phase 2 source that implements adapters, metrics, cost, and the store.I am pinning the source line numbers for the Phase 2 defects before writing the findings.The feature file on disk includes a third scenario. I am checking whether that matches the reviewed commit or a later edit.**G2-1**
Severity: high
File:line: src/measurement.rs:310
Description: `EmbeddingCalls::summary` sets `usage_tokens` to the sum of successful calls that reported usage whenever that set is non-empty. A successful call with `usage_tokens == None` is dropped. `experiment::run` passes that `Option` straight to `cost::estimate` (`src/experiment.rs:252`). OpenAI accepts a 200 whose `usage` object or token fields are absent (`src/embedding/openai.rs:118`, `src/embedding/openai.rs:335`) and still returns `Ok`.
Impact: One billed batch that omits usage, next to a batch that reports it, becomes `EstimateStatus::Estimated` for a partial token count. PLAN Phase 2 requires a null cost with a reason when usage is missing. The report presents an invented total.
Reproduction scenario or verifiable rationale: Two successful document calls, `items > 0` on both, `usage_tokens` `Some(100)` and `None`. `summary` returns `Some(100)`. `estimate(Execution::Remote, Some(100), Some(price))` returns `Estimated` and a non-null cost. All-`None` stays `None` and is the only missing-usage case that stays null.
Validation suggestion: Add a call-log case with a non-empty success that omits usage beside one that reports it, and assert `usage_tokens` is `None` and the ingestion estimate is `Unavailable` with a missing-usage reason.

**G2-2**
Severity: medium
File:line: src/experiment.rs:171
Description: Cold start is a second `build()` plus the first frozen query, after ingestion has already loaded the model and, for a remote engine, already opened the connection in the same process (`src/experiment.rs:128` then `src/experiment.rs:171`). That pair is stored as `measurements.cold_start` (`src/experiment.rs:220`). PRD §9.4 defines cold start as the first operation in a fresh process, including model load or the first remote connection. The module comment (`src/experiment.rs:6`) states the second load as that definition.
Impact: `cold_start.total_ms` is a warm-process reload plus one query. It leaves out the real first load (`ingestion_model_load_ms`) and, on a remote engine, the first connection made during ingestion. Published cold-start figures do not measure the PRD metric. The BDD fixture locks this sequence in.
Reproduction scenario or verifiable rationale: `run` always calls `build` at line 129, ingests, then calls `build` again at line 172 and times only the second load and `search_text` of `queries[0]`. A fresh process's first operation is the first `build`, and it is not the cold-start field.
Validation suggestion: Time the first `build` and the first embedding or connection in a fresh process as cold start, or change the field so it no longer claims the PRD §9.4 definition. Assert the recorded cold start includes that first operation.

**G2-3**
Severity: medium
File:line: src/experiment.rs:228
Description: Every configuration gets `gpu_memory: Observation::unavailable("inference runs on the CPU; no GPU is used")`, including `Execution::Remote`. The remote branch separately sets `provider_inference_memory` to "remote provider: inference memory is not observable" (`src/experiment.rs:214`).
Impact: A remote run's resource report states that inference ran on the local CPU. PRD §9.4 requires GPU memory to be a number only when the engine runs on a GPU and memory is observable, otherwise `"unavailable"` with the reason. US-05 requires unavailable remote measurements to stay honest. The two fields contradict each other. The honesty scenario checks only `provider_inference_memory`.
Reproduction scenario or verifiable rationale: `experiment::run` assigns that CPU reason on line 228 with no `execution` check. A remote `ConfigurationResult` therefore carries both the CPU sentence and the remote-unobservable sentence.
Validation suggestion: For `Execution::Remote`, set the GPU reason to state that provider GPU memory is not observable. Assert the remote BDD report does not say inference ran on the CPU.

**G2-4**
Severity: medium
File:line: tests/bdd/steps/us05.rs:431
Description: The `@m2` scenario "Do not invent unknown costs" (`tests/features/us05.feature:26`, PRD §5.5) does not run a provider response that lacks usage. The Given calls `estimate(Execution::Remote, None, Some(&price))` and stores that value (`tests/bdd/steps/us05.rs:433`). The When "I generate its cost report" (`tests/bdd/steps/us05.rs:441`) copies `notes["costs"]` into `notes["json"]` and does not call `experiment::run` or the report writer. PRD §5.7 requires the When step to invoke the real operation. The missing-price arm does call `measured_run` (`tests/bdd/steps/us05.rs:430`); the missing-usage arm does not.
Impact: A bug that turns a successful call with omitted usage into a billed total (G2-1), or a report path that renders an unknown cost as a number, still passes this scenario. The Then steps read back the Given's own `estimate` return value. `cost::tests::unknown_costs_are_null_with_a_reason_never_zero` only covers a direct `None`.
Reproduction scenario or verifiable rationale: The step strings match the feature, so cucumber runs them. Replacing `EmbeddingCalls::summary` so a mixed usage log returns `Some(sum)` does not change `notes["costs"]`, and the scenario stays green.
Validation suggestion: Drive a remote measured run whose successful batches omit usage, generate the real cost report, and assert both ingestion and query-set costs are null with the missing-usage reason and are not numbers.

**G2-5**
Severity: low
File:line: src/fidelity.rs:245
Description: If no line's first cell matches the test name, the row binds to the first line on that page whose second cell equals the expected value. Two sample rows with the same value and unmatched names bind to that same first line. The later line is never judged.
Impact: Field accuracy can count one extracted line as two rows. Value, unit, and reference range of the second expected row are scored against the first line, which inflates accuracy when those cells coincide. The checked-in synthetic sample has unique values per page, so the published 0.688 OCR figure does not hit this path.
Reproduction scenario or verifiable rationale: On one page, two pipe-lines `A | 92 | mg/dL | 70–99` and `B | 92 | g/dL | 1–2`, and two sample rows whose names do not occur, both with value `92` and different units. Both rows resolve through `lines.iter().find` at line 245 to the first line. The second row's unit is compared to `mg/dL`.
Validation suggestion: When more than one line matches the value, or that line was already used, leave the row not found. Add a fixture with two equal values and different units and assert the second row is not scored against the first line.
