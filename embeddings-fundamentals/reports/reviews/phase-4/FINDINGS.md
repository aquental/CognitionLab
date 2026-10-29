# Phase 4 — adversarial review findings

Reviewed from base `e1abe6f` (`main` after Phase 3, PR #8). Review order (owner rule, `CLAUDE.md`): Grok → Codex → Antigravity, repeated until no reviewer has an unresolved valid finding.

## Grok round 1 (`grok-round1.md`, reviewed `cc9a88c`)

| ID | Reviewer | Severity | Description | Resolution / refutation | Files | Validation evidence | Reviewer confirmation | Status |
|---|---|---|---|---|---|---|---|---|
| G4-1 | Grok | high | `search --mode combined --experiment` neither checked the snapshot's fusion parameters nor that it pins both leg spaces. Another `k` silently changed the ranking, and a missing leg was fused as an empty list | Valid. `check_combined_snapshot`: the fusion fingerprint must equal `Fingerprint::of(params)` and the snapshot must pin a revision in each leg's space. `search_combined` runs it for every experiment scope before any read or embedding; the CLI maps `SearchError::SnapshotMismatch` to exit 3 | `src/retrieval.rs`, `src/experiment.rs`, `src/cli.rs` | `tests/combined.rs::a_combined_search_of_an_experiment_must_be_that_combined_run`: another `k`, a leg snapshot, and a snapshot without the image leg are refused, and no query is embedded. A mutant dropping the check fails it | Grok round 2 (`a2a1e1e`): NO FINDINGS (closure confirmed) | closed |
| G4-2 | Grok | medium | `search_fused` built both models before reading `--experiment`, so a typo cost two model loads or downloads | Valid. The snapshot is read and its fusion fingerprint checked before either model loads; single-mode `search` now also reads the snapshot first | `src/cli.rs` | `tests/combined.rs::a_combined_search_reads_its_experiment_before_loading_any_model`: the default-feature binary refuses `no-such-run` without the "not compiled in" build error. A mutant building first fails it | Grok round 2 (`a2a1e1e`): NO FINDINGS (closure confirmed) | closed |
| G4-3 | Grok | medium | The combined index size summed the active revisions of both spaces, not the revisions the combined snapshot pins and evaluates | Valid. `FsRecordStore::revisions_size_bytes` over the combined snapshot's revisions; `index_size_bytes` now delegates to it | `src/storage.rs`, `src/experiment.rs`, `src/report.rs` | `tests/combined.rs::a_combined_run_measures_the_index_it_pinned_not_the_active_one`: a later text run replaces the active revisions, and the reported size equals the pinned directories, summed independently. A mutant using active sizes fails it. Evidence in `reports/phase-4` was regenerated with identical metrics | Grok round 2 (`a2a1e1e`): NO FINDINGS (closure confirmed) | closed |

## Grok round 2 (`grok-round2.md`, reviewed `a2a1e1e`)

NO FINDINGS. G4-1, G4-2, and G4-3 are confirmed closed.

## Owner-requested real-exam tests (after Grok round 2)

The owner asked for the real sample exams in `tests/sample-exams/` (git-ignored, local only) to improve the suite. The new local tests (`tests/sample_exams.rs`) found two defects. Both were fixed with synthetic regression tests and enter the next review round (Grok round 3 → Codex → Antigravity):

| ID | Found by | Severity | Description | Resolution | Files | Validation evidence | Status |
|---|---|---|---|---|---|---|---|
| S4-1 | real-exam test | high | `filename_findings` flagged none of the 12 real file names, which are mostly upper-case personal names | The rule now flags upper-case word sequences with one-letter particles, and every free-form name (anything beyond lower-case ASCII letters, digits, `-`, `_`, `.`). The gate is unchanged: no review, no processing | `src/anonymization.rs` | `anonymization::tests::identifier_like_filenames_are_flagged` (synthetic names); real: `every_real_exam_name_is_flagged_for_review_without_revealing_it` | fixed |
| S4-2 | real-exam test | high | A table header that no row used was dropped from every chunk: 24 lines on 9 of the 12 real documents | Kept as a paragraph. The chunking rules id `structure-aware-v1/r2` enters the chunking fingerprint, so old revisions are not reused. The combined BDD fixture was re-derived because text record ids changed | `src/chunking.rs`, `src/pipeline.rs`, `tests/fixtures/metrics/combined*.toml` | `chunking::tests::a_table_header_no_row_uses_is_kept_as_text` (a mutant fails it); real: `real_exams_extract_deterministically_and_chunk_within_the_invariants` | fixed |
