# Stages 1–2: adversarial review findings

Stages 1–2 add `anonimizador batch`:
- detection (`locate.py`, `detect.py`);
- redaction by rasterizing (`redact.py`);
- verification of each copy;
- a Markdown index of each run.

The owner chose the rasterize approach on 2026-10-30. The review order follows `CLAUDE.md`. Grok is skipped for now, by the owner's waiver for its quota (as for the hub-lock fix of `embeddings-fundamentals`), so Codex reviews first and Antigravity follows.

Packages hold the code, the SPEC, and the PLAN only. They hold no real exam and nothing derived from one. Each package was checked for exam file-name words (counts only): 1 word of 10 letters is in the package. It is a generic date-of-birth field label, which the label detector in `detect.py` must list. The synthetic fixtures, the tests, and the README avoid it. Codex quotes it once in `codex-round1.md` (CB-7), kept verbatim.

## Found by the implementer: JPEG pages

| ID | Reviewer | Severity | Description | Resolution | Files | Validation evidence | Status |
|---|---|---|---|---|---|---|---|
| I-1 | implementer (real-exam run, counts only) | high | Pillow writes grayscale and color PDF pages as JPEG (quality 75). On a real exam, a page with no box went from 1709 to 3158 OCR characters after the round trip, and another from 3930 to 1629. `verify` of the copy then counted 42 phone-shaped digit runs that the original's OCR did not have. The `embeddings-fundamentals` pipeline reads these copies by OCR, so the copies degraded the lab's input | `redact.pdf_bytes` writes the PDF itself: per page, one image with FlateDecode (lossless, `zlib`), gray when the page is neutral. It writes no information dictionary, and the same pages give the same bytes | `src/anonimizador/redact.py` | `tests/test_batch.py::test_a_copy_keeps_only_pixels` (FlateDecode, no DCTDecode, no `/Info`); `test_the_same_original_gives_the_same_copy`. The mutant without FlateDecode is killed | fixed |

## Codex round 1 (`codex-round1.md`, reviewed `6abb72b`)

Codex reported 11 findings. All are valid.

| ID | Severity | Description | Resolution | Validation evidence | Status |
|---|---|---|---|---|---|
| CB-1 | high | Copies were written into `--out` over any existing file: with `--out` the source, a file named like a copy, or a link, an original could be overwritten | Each run writes into its own new directory, `<out>/<YYYYMMDD-HHMMSS>/` (copies, plus `logs/`). Every file is created exclusively (`open(..., "x")`), so no existing file or link is written through. Two originals with the same bytes get distinct copy names | `test_the_source_itself_as_output_writes_nothing_over_an_original`. The mutant writing the copies into `--out` itself is killed | fixed |
| CB-2 | high | The git check looked at the index's lexical path only: a link at the index path could lead into the repository | `start` also checks where each path's links lead (`realpath`), refuses an index path that already exists or is a link, and writes the index exclusively | `test_an_index_path_that_links_into_the_repository_is_refused` (the target is never created). The mutant without the `realpath` check is killed | fixed |
| CB-3 | high | The last console line printed the index's resolved path, whose directories can name a person | The console prints the index file name only (`<stamp>-anonimizer.md`) | `test_the_console_never_prints_the_output_path` (outputs under a directory named after an invented person). The mutant printing the path is killed | fixed |
| CB-4 | high | With `--no-ocr`, nothing read a copy's pixels, so a copy was `ok` unread | `batch` refuses `--no-ocr` before writing anything: a copy holds only pixels | `test_a_run_without_ocr_is_refused_before_anything_is_written`. The mutant without the check is killed | fixed |
| CB-5 | high | Each page was searched alone, so a term split across a page break got no box, though `verify` searches pages together | `detect_reading` searches every page of a reading as one text, a line break between pages; each word keeps its page | `test_detect.py::test_a_term_split_across_a_page_break_is_found_on_both_pages`, through `detect_reading` and through `detect`. The per-page mutant is killed | fixed |
| CB-6 | high | An RG ending in the check letter X was no number value, so it survived its `RG` label | A number value may end in `x` | `test_an_rg_with_the_check_letter_x_is_taken`. The mutant is killed | fixed |
| CB-7 | high | A label's value on the next line was never taken: the date of birth under its label survived under `--keep-dates` | A colon label that ends its line takes the next line of the same page, up to the next label | `test_a_label_that_ends_its_line_takes_the_next_line` (also neither a following line when the value is on the label's line, nor across a page). The mutant is killed | fixed |
| CB-8 | medium | Errors in verifying the original were not counted, so the run could exit 0 | An exam counts as an error when its original, its steps, or its copy has an unread place | `test_an_unreadable_original_fails_the_run_even_with_a_clean_copy`. The mutant is killed | fixed |
| CB-9 | medium | Malformed `pdftotext` or `tesseract` output raised `ParseError`, `ValueError`, or `KeyError` as a traceback | Parsing failures become a `ToolError` naming the tool only. The exam gets an `ERROR` row and the run exits 2 | `test_malformed_tool_output_is_an_error_row_not_a_traceback` (a stub `pdftotext` that exits 0 with broken XHTML). The mutant without the handler is killed | fixed |
| CB-10 | medium | Short multiword terms took non-overlapping matches, unlike `Term.count`, so a word of a second, overlapping occurrence got no box | The word pattern runs in a lookahead, so overlapping occurrences are all found | `test_overlapping_occurrences_of_a_short_term_are_all_found`. The mutant is killed | fixed |
| CB-11 | medium | A number glued to its label (`CRM:123`) needed 4 digits | Any digit glued to a number label makes its word the value | `test_a_short_number_glued_to_its_label_is_taken`. The mutant is killed | fixed |

Validation:
- `uv run pytest`: 159 passed, 2 skipped (opt-in).
- ruff: clean.
- `RUN_SAMPLE_EXAMS=1`: 2 passed.
  - The batch check runs on the 2 smallest exams, since it now needs OCR.
  - The stage-0 check now passes the originals by name, because the copies live under the exams directory.
- One mutant per fix, each run against its regression test: all killed. The CB-5 mutant survived at first, because the first test called `detect_reading` directly. A test through `detect` now kills it.

### Known limitation, not a finding

`verify` reads a copy's raw bytes, which are now its pixel data. Gray levels such as `0x40` and `0x2E` read as `@` and `.`, so it can report e-mail or `crm` pattern hints on a clean copy. Hints fail a run only with `--strict-patterns`. The test checks what OCR reads on each copy instead: no pattern but the `CRM` label.

## Owner's review of the copies (`ecc58ba`)

Looking at the copies of the first real run, the owner found patient names left on two kinds of exam. Both forms print `Nome` with no colon: one has the name beside it, the other under it, as under a column heading. The label detector took colon labels only.

| ID | Severity | Description | Resolution | Validation evidence | Status |
|---|---|---|---|---|---|
| OW-1 | high | A `Nome` label without a colon was no label, so the name beside it or under it survived | A `Nome` that starts its line (also `Nome do paciente`, `Nome da mãe`, `Nome do pai`, `Nome social`) takes, in order: the words beside it up to a gap wider than twice its height; else the words under it that start where it starts; else the next words of its line. A value of particles only (`Nome do exame:`) is dropped | 5 tests in `test_detect.py`, each part killed by a mutant. On the real exams, checked locally by counts and positions only: the two names the owner reported are found in the originals and absent from the copies, and the region under the column heading is fully black | fixed |

## Codex round 2 (`codex-round2.md`, reviewed `ecc58ba`)

Codex reported 6 findings. All are valid. The three on image and TSV reading were reproduced with synthetic inputs before the fix.

| ID | Severity | Description | Resolution | Validation evidence | Status |
|---|---|---|---|---|---|
| CB-12 | high | A bare label searched for its value only in its own extracted line, so a name the extractor put in another block on the same row survived | The value is searched in the label's row: the words of its page to its right whose middle lies within its height, whatever their line. The column value continues along its row the same way | `test_a_name_beside_its_label_on_another_extracted_line_is_taken`. The mutant limiting the row to the label's line is killed | fixed |
| CB-13 | high | Malformed EXIF raised Pillow's `SyntaxError`, which quotes the header bytes, outside every handler: a traceback with document bytes | Decoding an image file catches Pillow's non-`OSError` errors and raises a `ToolError` that names no byte, with the context suppressed. The exam gets an `ERROR` row | `test_malformed_exif_is_a_tool_error_without_its_bytes`, `test_malformed_exif_in_a_batch_is_an_error_row` (a PNG with a valid `eXIf` chunk holding an invented sentinel). The mutant without the handler is killed | fixed |
| CB-14 | medium | TSV without its columns was read as an empty page, so a broken OCR reading let redaction go on without OCR boxes | TSV without the columns of a word is a parse error: a `ToolError` and an `ERROR` row. A header with no word is still an empty page | `test_tsv_without_its_columns_is_no_reading`, `test_malformed_tsv_in_a_batch_is_an_error_row` (a stub `tesseract` whose TSV is garbage). The mutant without the check is killed | fixed |
| CB-15 | medium | A transparent image lost its alpha channel instead of being composited, so a page could turn black | Transparency is composited onto white, as a viewer shows it | `test_a_transparent_background_is_read_as_white` (black under transparent pixels, opaque black text). The mutant without compositing is killed | fixed |
| CB-16 | high | `committable` took any `git rev-parse` failure, such as an unsafe-ownership error or an inherited `GIT_DIR`, for "outside any repository" and failed open | Git runs without inherited `GIT_*` variables, so it finds the repository from the path. A failure under a directory holding `.git` raises a `TermsError` instead of answering "not committable" | `test_a_git_that_cannot_tell_refuses_instead_of_failing_open`. Both mutants, answering `False` on failure and passing the inherited environment, are killed | fixed |
| CB-17 | medium | The next-line fallback ran when the label had no value, not when it ended its line, so a blank field followed by another field took the next line | The fallback runs only when no word follows the label on its line | `test_a_blank_field_followed_by_another_field_takes_nothing_below`. The mutant with the old condition is killed | fixed |

Validation:
- `uv run pytest`: 172 passed, 2 skipped (opt-in).
- ruff: clean.
- `RUN_SAMPLE_EXAMS=1`: 2 passed.
- One mutant per fix, two for CB-16: all killed.

## Antigravity round 1 (`agy-round1.md`, reviewed `45a6e3d`)

Antigravity reported 7 findings. AB-1, AB-2, AB-3, and AB-6 are valid. AB-4 and AB-5 are valid in part. AB-7 is refuted.

| ID | Severity | Description | Resolution | Validation evidence | Status |
|---|---|---|---|---|---|
| AB-1 | high | A value ended only at a word ending in `:`, so a next field with a glued (`Idade:45`) or detached (`Idade :`) colon was taken into it. A heading `Nome` with such a field beside it took that field instead of the name under it | A value ends at a word holding a colon, or before a word that starts with one | `test_a_field_with_a_glued_or_detached_colon_ends_a_value`. Both mutants (no detached colon; `endswith` again) are killed | fixed |
| AB-2 | high | A colon label that ended its extracted line took the next line in block order, not the value beside it in another block | A colon label that ends its line takes the value on its row (as CB-12 for bare labels), else the value under it | `test_a_colon_label_ending_its_block_takes_the_value_on_its_row`. The mutant without the row search is killed | fixed |
| AB-3 | high | (1) `Paciente`, `Mãe`, and date-of-birth labels without a colon were no labels. (2) A date of birth under its colon label was missed when another block came next in block order | (1) They are bare labels, each with a test of its value: `Paciente` and `Mãe` take only capitalized words and particles, so running text ("Paciente refere…") is left alone; a date-of-birth label takes only a value with digits. (2) The value under a label is found by position (`column_below`), not by block order | `test_a_date_of_birth_under_its_label_in_another_block_is_taken`, `test_patient_mother_and_date_of_birth_labels_need_no_colon` (also running text and a word without digits taken by nothing). Mutants removing each label, each value test, and the position search are killed | fixed |
| AB-4 | medium | (1) A 16-bit grayscale PNG converted to 8 bits clipped every value above 255, so the page turned white. (2) `zlib.error` would escape the handlers | (1) Valid, reproduced: a 16-bit image is scaled to 8 bits (`/ 256`) first. (2) Not reproduced: a PNG whose `zTXt` chunk holds bytes that are not zlib data loads without an error in Pillow 13.0.0, and corrupt image data raises `OSError`, which is handled. No change | `test_a_16_bit_image_keeps_its_contrast`. The mutant without scaling is killed | (1) fixed, (2) refuted |
| AB-5 | medium | (1) Directory-only ignore rules on paths not yet created; (2) `realpath` through components not yet created; (3) `--out` inside `.git` passes, since `git rev-parse` prints `false` there; and the copies are never checked | The copies' place is checked too (a copy and a detections file), so ignore rules that cover only the index and the logs refuse the run. Refuted: (1) a wrong answer there says "committable", which refuses the run (fails closed); (2) a component that does not exist is no link, and `mkdir` creates real directories; (3) git never tracks a path inside `.git`, so nothing written there can be committed | `test_copies_git_could_commit_are_refused_before_anything_is_written`. The mutant without the copies is killed | fixed in part, rest refuted |
| AB-6 | medium | (1) A link among the originals that leads nowhere was skipped, and the run reported every exam done. (2) An unreadable original stops the run before it starts | (1) Valid: such a link is an error that stops the run, as in `verify`. (2) Refuted: the run stops with exit 2 and the error's reason only (`OSError.strerror`), so nothing passes unread; a run that stops is not a hidden failure | `test_a_link_among_the_originals_that_leads_nowhere_stops_the_run`. The mutant without the check is killed | (1) fixed, (2) refuted |
| AB-7 | low | `aligned` checks the aspect ratio only, so a page turned 180° would get its text-layer boxes at inverted places | Refuted: poppler gives `pdftotext -bbox-layout` coordinates in the page's displayed orientation, as `pdftoppm` renders it. On a synthetic page with `/Rotate 180`, a word's box starts at (386.6, 695.0) points, and its ink rendered at 72 dpi starts at (386, 694). No change | Synthetic PDF, checked locally | refuted |

Validation:
- `uv run pytest`: 179 passed, 2 skipped (opt-in).
- ruff: clean.
- `RUN_SAMPLE_EXAMS=1`: 2 passed.
- 12 mutants, at least one per fixed part: all killed.

## Codex round 3 (`codex-round3.md`, reviewed `1792e6d`)

Codex reported 5 findings. All are valid. CB-22 was reproduced with a synthetic 16-bit PNG before the fix (its text read 255, as white as the background).

| ID | Severity | Description | Resolution | Validation evidence | Status |
|---|---|---|---|---|---|
| CB-18 | high | Number labels searched only their extracted line, so a number in another block beside the label, or under it, survived | A number label that ends its extracted line takes the number on its row, else under it, by position (as CB-12 and AB-2) | `test_a_number_label_ending_its_block_takes_the_number_beside_or_under_it` (also a word that is no number, under `CRM`, taken by nothing). The mutants without the row and without the column are killed | fixed |
| CB-19 | high | A state glued to a council number (`123456-SP`, `123456/SP`) made the word no number part, so the number survived | A number part may end in a state as it may end in the check letter X | `test_a_state_glued_to_a_council_number_is_part_of_it`. The mutant with the old pattern is killed | fixed |
| CB-20 | medium | The AB-5 check asked git about probe file names, not the real ones, so rules naming files could leave a real copy or log committable | Every file is checked by its own path before it is written: each log and detections file (`write_new`) and each copy. The probes stay, to refuse a run before it starts | `test_a_copy_git_could_commit_is_refused_even_when_the_probes_are_ignored`, `test_a_log_git_could_commit_is_refused_even_when_the_probes_are_ignored`. Both mutants are killed | fixed |
| CB-21 | medium | A page rendered at a high `--dpi` raised Pillow's `DecompressionBombError` outside every handler | It becomes a `ToolError` ("a rendered page has too many pixels; lower --dpi") and an `ERROR` row | `test_a_rendered_page_past_the_pixel_limit_is_a_tool_error` (the pixel limit lowered). The mutant without the handler is killed | fixed |
| CB-22 | medium | A 16-bit PNG with a transparent value was flattened to 8-bit RGBA before it was scaled, so its content turned white | The 16-bit samples are scaled first; the transparent value becomes an alpha channel, which is then composited onto white | `test_a_16_bit_image_with_a_transparent_value_keeps_its_contrast`. The mutants with the old order and without the alpha channel are killed | fixed |

Validation:
- `uv run pytest`: 185 passed, 2 skipped (opt-in).
- ruff: clean.
- `RUN_SAMPLE_EXAMS=1`: 2 passed.
- 8 mutants, at least one per fix: all killed.

## Antigravity round 2 (`agy-round2.md`, reviewed `0df51b8`)

The file holds Antigravity's drafts before its final list, which are its last three entries. It is recorded as written. All three findings are valid: each reproduction case returned what Antigravity said before the fix.

| ID | Severity | Description | Resolution | Validation evidence | Status |
|---|---|---|---|---|---|
| AB-8 | high | `data nasc` was only a bare label, so with a colon (mid-line, detached, or glued) its date was missed | It is a colon label too; a test requires that of every bare label | `test_every_bare_label_is_a_colon_label_too`; mutant killed | fixed |
| AB-9 | high | (1) A colon label ending its line took a far word on its row before looking under itself. (2) A number label followed only by a state or a colon (`CRM SP`, `RG :`) never searched its row or the line under it | (1) Beside it within a gap is the value; otherwise both the words under it and the far words of its row are taken. (2) When the words after the label are number parts without a digit, the row and the column under the label are searched | `test_a_colon_heading_takes_the_value_under_it_past_a_far_heading`, `test_a_colon_label_ending_its_block_takes_a_far_value_on_its_row`, `test_a_number_label_followed_by_a_state_or_colon_finds_its_number_elsewhere`; 4 mutants killed | fixed |
| AB-10 | medium | A border or bullet OCR reads before a bare label (`\| Nome`) hid the label | Leading non-word characters are skipped before the label; the colon check starts at the label | `test_a_border_before_a_bare_label_is_skipped`, `test_a_border_before_a_colon_label_leaves_it_a_colon_label`; 2 mutants killed | fixed |

Validation: `uv run pytest` 191 passed, 2 skipped; ruff clean; `RUN_SAMPLE_EXAMS=1` 2 passed; 7 mutants, all killed.

## Codex round 4 (`codex-round4.md`, reviewed `0e5a152`)

Codex reported 3 findings. All are valid; CB-23 and CB-24 were reproduced before the fix.

| ID | Severity | Description | Resolution | Validation evidence | Status |
|---|---|---|---|---|---|
| CB-23 | high | A state before a document number (`MG-12.345.678`) made the word no number part | A number part may start with a state, as it may end in one | `test_a_state_before_a_document_number_is_part_of_it`; mutant killed | fixed |
| CB-24 | high | A dash between a bare `Paciente` or `Mãe` label and the name failed the whole value as no name | Separators are not words of a name: they neither fail it nor make one by themselves | `test_a_dash_between_a_bare_label_and_a_name_is_not_the_name` (also a dash alone, and prose after a dash, taken by nothing); mutant killed | fixed |
| CB-25 | medium | `get_flattened_data` exists only in Pillow 11.3+, while the project allows 10.1: an `AttributeError` outside every handler | The 16-bit samples are read as bytes (`tobytes`, native 32-bit integers), which every supported Pillow gives | `test_a_transparent_16_bit_image_needs_no_pillow_newer_than_declared` (the newer pixel APIs made to fail); mutant killed | fixed |

Validation: `uv run pytest` 194 passed, 2 skipped; ruff clean; `RUN_SAMPLE_EXAMS=1` 2 passed; 3 mutants, all killed.

## Antigravity round 3 (`agy-round3.md`, reviewed `111487c`)

`NO FINDINGS`. Antigravity has no open finding on stages 1–2.
