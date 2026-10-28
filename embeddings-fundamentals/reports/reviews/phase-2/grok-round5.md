The review needs the rest of the phase diff. I'll read it in sections and check only the round-2 fixes and the contracts they touch.The latest unconfirmed fixes are in evaluation, experiment, fidelity, measurement, report, and the CLI. I'll read those diffs next.ID: G2-7
Severity: medium
File:line: src/fidelity.rs:277
Description: The value fallback walks sample rows in file order and binds a row when exactly one line on the whole page has that value and the line is still free. Two unmatched rows that share a value, with one extracted line, give that line to whichever row is listed first, so unit and reference-range credit follow TOML order. A value that also sits on a line already reserved by an exact name is treated as ambiguous, so the remaining unclaimed line is left unused.
Impact: Field-level accuracy changes when the sample rows are reordered, and an OCR name error on a duplicated value is scored as "not found" even though one unclaimed line carries that value. The synthetic sample's values happen to be unique per page, so the published 32/32 and 22/32 figures stay stable.
Reproduction scenario or verifiable rationale: Page text `Xxx | 92 | mg/dL | 70–99`. Rows `(A, 92, mmol/L, 1–2)` then `(B, 92, mg/dL, 70–99)` score 1 correct field (A takes the line). The reverse order scores 3 (B takes it). With lines `Glucose | 92 | mg/dL | 70–99` and `Xxx | 92 | mg/dL | 70–99`, the Glucose name match reserves the first line and the second row never binds, because 92 occurs twice.
Validation suggestion: Assign residual values in one pass over unclaimed lines only: bind a row when exactly one unclaimed line has its value, and assert both sample orders and the name-reserved duplicate-value case.

ID: G2-8
Severity: medium
File:line: src/report.rs:281
Description: `Hardware::detect` (`src/measurement.rs:466`) sets `cpu` and `memory_bytes` to `None` when `sysctl` or `/proc` fails, returns empty output, or hits the 5 s timeout. `ComparisonReport::new` requires the entire `Hardware` value to be equal, including those probe fields.
Impact: Two runs on the same machine become `NotComparable` ("measurement hardware") when one probe times out or is absent. The `report` command then exits as validation and writes no comparison. C2-12's timeout is the path that produces the `None`.
Reproduction scenario or verifiable rationale: Build two `ConfigurationResult`s that share os, arch, logical CPUs, and accelerator, and differ only by `cpu: None` versus `cpu: Some("Apple M3")` (the value `probe_output` returns after `timeout` on `sysctl`). `ComparisonReport::new` returns `ReportError::NotComparable { what: "measurement hardware" }`.
Validation suggestion: Compare the stable identity (os, arch, logical CPUs, accelerator) and treat a missing cpu string or memory size as unknown, or record the probe failure explicitly. Add a comparison test whose only difference is a timed-out probe field.

ID: G2-9
Severity: medium
File:line: src/experiment.rs:238
Description: An incomplete ingestion already writes the create-only experiment snapshot (`src/pipeline.rs:486`). `experiment::run` then builds a second adapter (`src/experiment.rs:233`) and runs the warm-up `search_text` with `?` before it maps `EvalError::LabelNotApplicable` on `RunStatus::Incomplete` to `ExperimentError::EvaluationNotRun`. Any `SearchError` or second `BuildError` leaves that match. The CLI (`src/cli.rs:6744`) turns that error into `Err`, and `main` returns `ExitCode::FAILURE` (1) without writing `--report`.
Impact: PLAN §4.3's incomplete run (exit 2 plus a report) is dropped when the discarded warm-up query or the second adapter load fails. The experiment id is already stored, so a retry stops at `ValidationError::ExperimentExists` and the failed labeled source is never reported. The C2-10 test uses a query embedder that always succeeds, so this path stays green.
Reproduction scenario or verifiable rationale: Fail extraction of `lab-searchable-001` (text labels in `labels-synthetic-v2.toml`) while another source ingests, and make the query embedder return a transient error. `run` returns `ExperimentError::Search` rather than `EvaluationNotRun`. The snapshot file exists; a second `experiment` with the same id is `ExperimentExists` (exit 3) and no incomplete JSON is written.
Validation suggestion: On `RunStatus::Incomplete`, return `EvaluationNotRun` for a label mismatch before the query-side load and warm-up query, and cover a warm-up `SearchError` with an assertion on exit code 2 and the `not_run` report.
