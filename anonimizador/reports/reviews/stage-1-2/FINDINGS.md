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
