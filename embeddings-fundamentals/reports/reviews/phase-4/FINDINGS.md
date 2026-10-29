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
| G4-4 | Grok | low | Since the `extract_pdf` refactor (`b14c7d1`), page reports were written only after every page extracted. A tool error on a later page (a `pdftotext` or `tesseract` failure or timeout) failed the source with `pages == []`, and the reason did not name the page | Valid. `extract_pdf` takes an `on_page` callback that sees each page as soon as it is extracted, and its error (`PdfExtractionError`) names the failing page (`page 2: tesseract failed: …`); the page count error is unchanged | `src/pipeline.rs`, `tests/sample_exams.rs` | `tests/pipeline.rs::a_failed_source_keeps_the_pages_extracted_before_the_failure`: OCR fails on page 2; the failed source keeps page 1 and the reason names page 2. A mutant reporting pages only after full success fails it (0 pages instead of 1) | Grok round 4 (`af9e59b`): closed | closed |

## Owner-requested corpus tools (after Grok round 3)

The owner asked for a local anonymization verifier, generators for the manifest and label drafts, and `TESTS.md`, which covers anonymization, building the corpus, and an automatic anonymizer design. They enter the next review round together with the G4-4 fix:

- `scripts/anonymization-verify.py`. It prints counts only, in six places: name, metadata, text layer, OCR, embedded images, and raw bytes. It refuses a terms file that git could commit.
- `scripts/make-covered-name-pdf.py` writes the synthetic fixture `tests/fixtures/anonymization/covered-name.pdf`, a name hidden under a drawn box.
- `src/drafts.rs`, with the CLI commands `draft-manifest` and `draft-labels`.
- Tests: `tests/drafts.rs` (3), `drafts::tests` (5, including a proptest that every proposed anchor is found by the evaluator), and 2 integration tests.

At the owner's request, the verifier, its fixture generator, and the fixture then moved to the sibling project `../anonimizador` (branch `anonimizador/scaffold`). Their review continues there.

### Privacy note (before Grok round 4)

The package check counts words from the real exams' file names, without printing them. It found that a synthetic test name added in `b14c7d1` (`anonymization::tests`) used a generic exam-type word that also occurs in a real exam's file name. The word is a procedure name, not a personal identifier. Under the owner's rule that nothing derived from the exams is committed, it was replaced with a neutral word before the round 4 package was built. `b14c7d1` is already pushed, so the word remains in that commit's history. Rewriting published history is left to the owner. No other word from the exam file names appears in the package except generic vocabulary that the repository already used before the exams existed.

## Grok round 4 (`grok-round4.md`, reviewed `af9e59b`)

G4-4 is confirmed closed.

| ID | Reviewer | Severity | Description | Resolution / refutation | Files | Validation evidence | Reviewer confirmation | Status |
|---|---|---|---|---|---|---|---|---|
| G4-5 | Grok | high | The verifier turned a failed tool (`pdftotext`, `pdfinfo`, `pdftoppm`, `pdfimages`, or `tesseract` exiting non-zero, an encrypted PDF, missing `por` data) into an empty channel and reported the file `ok` with exit 0. A tool timeout raised `TimeoutExpired`, whose message holds the file path | Valid. At the owner's request, the verifier had just moved to the sibling project `../anonimizador` (branch `anonimizador/scaffold`). This branch now removes the script, its fixture generator, the fixture, and its integration test, so Phase 4 no longer ships the defective copy. The fix is made in `anonimizador`, which has its own review loop | `scripts/anonymization-verify.py` (removed), `../anonimizador/src/anonimizador/` | See `anonimizador` (stage 0, commit `61e5d24`): stub-tool regression tests | Grok round 5 (`e5f2047`): nothing in Phase 4 depends on the removed script. The fix is confirmed in the `anonimizador` review | moved; fixed in `anonimizador` (`61e5d24`) |
| G4-6 | Grok | medium | `filename_findings` checked only the last path component. A directory such as `MARIA SILVA/lab-001.pdf` was never flagged, so `draft-manifest` produced a clean `source_id` and wrote the name into the tracked manifest. The verifier's `name` channel had the same gap | Valid. Every component of the relative path is checked, and directories are reported as "directory name …" without the value. This fixes `draft-manifest` and `anonymization-check` together. The verifier's gap is fixed in `anonimizador` with G4-5 | `src/anonymization.rs`, `tests/sample_exams.rs` | `anonymization::tests::identifier_like_filenames_are_flagged` (nested directory), `tests/drafts.rs::a_flagged_file_name_never_becomes_a_source_id` (`sub/BELTRANO DA SILVA/…` is flagged and gets `source-002`), and the integration test `the_cli_drafts_the_synthetic_manifest_with_real_tools` (exit 3, nothing written, no name printed). The filename-only mutant fails the first two | Grok round 5 (`e5f2047`): NO FINDINGS (closure confirmed) | closed |

## Grok round 5 (`grok-round5.md`, reviewed `e5f2047`)

NO FINDINGS. G4-6 is confirmed closed, and nothing in Phase 4 depends on the verifier removed for G4-5. The Grok loop for Phase 4 is complete; next come Codex and then Antigravity.

## Codex round 1 (`codex-round1.md`, reviewed `3ba687f`)

| ID | Reviewer | Severity | Description | Resolution / refutation | Files | Validation evidence | Reviewer confirmation | Status |
|---|---|---|---|---|---|---|---|---|
| C4-1 | Codex | high | `run_combined` did not carry the run status of its legs. A leg whose run failed a required source could be combined into apparently complete evidence (exit 0) | Valid. `ExperimentSnapshot.run_status` (serde default) records the run's outcome. `run_combined` refuses a leg that is `incomplete`, and a snapshot that records no status, before any model loads | `src/storage.rs`, `src/pipeline.rs`, `src/experiment.rs` | `tests/combined.rs::a_combined_run_refuses_an_incomplete_or_unrecorded_leg`: neither refusal saves a combined snapshot. `tests/pipeline.rs::a_failed_source_keeps_the_pages_extracted_before_the_failure`: a failed required source writes `incomplete`. A mutant ignoring the status fails | pending (Codex round 2) | fixed |
| C4-2 | Codex | medium | The remote permission check accepted any manifest that allowed remote processing, without binding it to the corpus actually searched. `search` (text, image, combined) and `draft-labels` could embed a query remotely against another corpus's index | Valid. `remote_permission` takes what is searched. Every searched snapshot must have indexed the `--manifest` corpus. An active-revision search, whose corpus no record names, is refused for a remote engine (`--experiment` is required). `combine` already checks that its legs indexed its manifest. All of this happens before any adapter is built | `src/cli.rs` | `tests/pipeline.rs::a_remote_search_needs_the_manifest_of_the_corpus_it_searches`: another corpus's permitted manifest gives exit 3, an active scope gives exit 3, and the right manifest passes the check. A mutant that skips the corpus comparison fails | pending (Codex round 2) | fixed |
| C4-3 | Codex | medium | A combined leg was matched to its reported run by experiment id and space only. Results from another store, or with other extraction or chunking, were accepted for "combined against its legs" | Valid. `storage::revisions_fingerprint`, an order-independent fingerprint of the pinned revisions, is recorded in `ConfigurationResult.revisions` and in each `LegConfiguration.revisions`. `with_combined` refuses a reported leg whose revisions differ or are unrecorded (`LegMismatch`, with its reason). The Phase 4 evidence was regenerated so that every result records its revisions | `src/storage.rs`, `src/experiment.rs`, `src/report.rs`, `reports/phase-4/` | `tests/combined.rs::the_comparison_report_sets_combined_retrieval_beside_its_legs` (other revisions and unrecorded revisions are refused; the real pair matches). A mutant without the check fails | pending (Codex round 2) | fixed |
| C4-4 | Codex | medium | `corpus_files` put the directory path into `DraftError::Read`, so an unreadable directory with a patient's name would be printed | Valid. Traversal errors name no path ("a directory under the corpus directory"), and `NotUtf8` and `Empty` carry none either. `io::Error` itself holds no path | `src/drafts.rs` | `tests/drafts.rs::an_unreadable_directory_is_reported_without_its_name` (permissions 000). A mutant restoring the path fails | pending (Codex round 2) | fixed |
| C4-5 | Codex | low | The README said each phase had passed its review loop, Antigravity included, while Phase 4's reviews were still in progress | Valid. The README says Phases 0–3 passed and the Phase 4 loop is in progress, and the integration count is updated | `README.md` | Text matches `PROGRESS.md` | pending (Codex round 2) | fixed |

## Grok round 6 (`grok-round6.md`, reviewed `2ec6835`)

NO FINDINGS. Grok re-reviewed the whole phase after the Codex round 1 fixes, which changed behavior it had reviewed. Next: Codex round 2, to confirm C4-1..C4-5.
