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

## Grok round 3 (`grok-round3.md`, reviewed `b14c7d1`)

| ID | Reviewer | Severity | Description | Resolution / refutation | Files | Validation evidence | Reviewer confirmation | Status |
|---|---|---|---|---|---|---|---|---|
| G4-4 | Grok | low | Since the `extract_pdf` refactor (`b14c7d1`), page reports were written only after every page extracted. A tool error on a later page (a `pdftotext` or `tesseract` failure or timeout) failed the source with `pages == []`, and the reason did not name the page | Valid. `extract_pdf` takes an `on_page` callback that sees each page as soon as it is extracted, and its error (`PdfExtractionError`) names the failing page (`page 2: tesseract failed: …`); the page count error is unchanged | `src/pipeline.rs`, `tests/sample_exams.rs` | `tests/pipeline.rs::a_failed_source_keeps_the_pages_extracted_before_the_failure`: OCR fails on page 2; the failed source keeps page 1 and the reason names page 2. A mutant reporting pages only after full success fails it (0 pages instead of 1) | pending (next Grok round) | fixed |

## Owner-requested corpus tools (after Grok round 3)

The owner asked for a local anonymization verifier, generators for the manifest and label drafts, and `TESTS.md`, which covers anonymization, building the corpus, and an automatic anonymizer design. They enter the next review round together with the G4-4 fix:

- `scripts/anonymization-verify.py`. It prints counts only, in six places: name, metadata, text layer, OCR, embedded images, and raw bytes. It refuses a terms file that git could commit.
- `scripts/make-covered-name-pdf.py` writes the synthetic fixture `tests/fixtures/anonymization/covered-name.pdf`, a name hidden under a drawn box.
- `src/drafts.rs`, with the CLI commands `draft-manifest` and `draft-labels`.
- Tests: `tests/drafts.rs` (3), `drafts::tests` (5, including a proptest that every proposed anchor is found by the evaluator), and 2 integration tests.

### Privacy note (before Grok round 4)

The package check counts words from the real exams' file names, without printing them. It found that a synthetic test name added in `b14c7d1` (`anonymization::tests`) used a generic exam-type word that also occurs in a real exam's file name. The word is a procedure name, not a personal identifier. Under the owner's rule that nothing derived from the exams is committed, it was replaced with a neutral word before the round 4 package was built. `b14c7d1` is already pushed, so the word remains in that commit's history. Rewriting published history is left to the owner. No other word from the exam file names appears in the package except generic vocabulary that the repository already used before the exams existed.
