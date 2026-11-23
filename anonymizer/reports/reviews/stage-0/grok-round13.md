I'll review the current stage against G0-42, the carried-over fixes, and the rest of `verify`. First I'll read the word-boundary logic and the tests that claim the -50 cutoff.G0-42’s threshold is in the code. Next I’ll check the boundary itself and whether any layout still joins a short name into the following word.G0-42 is fixed, and the “every negative number” refutation holds. G4-5 and the verifier part of G4-6 still hold. One new defect.

## Confirmations

**G0-42 (confirmed).** `WORD_SPACE_ADJUSTMENT` is `-50`, and `gap_joint` marks a TJ number at or below that as `AMBIGUOUS` (`channels.py:213` and `channels.py:305`). The split reading then keeps a 5-letter name whole. `tests/test_channels.py::test_a_tj_adjustment_that_a_reader_shows_as_a_space_is_ambiguous` covers `-150`, `-100`, `-199`, and `-50` in an unreferenced Flate stream, and it requires kerning of `-30` and `-49` to leave the split reading as one word.

I remeasured `pdftotext -layout` on Helvetica, Times, and Courier (plain and bold) at 8, 12, and 24 pt, and Helvetica from 6 pt through 72 pt. Words join at `-90` and above, and split at `-100` (12 pt and up, and some 24 pt faces) or `-110` (8 and 10 pt). `-50` sits under that split on every face measured. A threshold of `0` would mark ordinary kerning (`-30`, `-49`) as a join point; those adjustments still join in `pdftotext`, and the kerning assertion would fail. The refutation stands.

**G4-5 (still holds).** `run` raises `ToolError` with the tool name only, for a non-zero exit, a timeout, or a tool that cannot start. The channel records that error, the file is `ERROR`, and the run exits 2.

**G4-6, verifier part (still holds).** A file found under a directory is named by its path relative to that directory, and every other path that reaches it is searched as an alias on the `name` channel.

## G0-43

**Severity:** medium

**File:line:** `src/anonimizador/channels.py:392`

**Description:** `raw_blobs` only inflates a stream when `stream` is followed by LF or CRLF (`stream\r?\n`). A bare CR is a valid end-of-line in PDF, and `pdftotext` reads a page content stream that uses it, but this regex never starts an inflate. The compressed bytes are not PDF text, so a name stored only in that stream is absent from every raw view.

**Impact:** A short name left in an old or unreferenced Flate stream whose `stream` keyword ends with CR only counts 0. The live page does not contain it, so `pdftotext` and OCR stay clean. The file is reported `ok` and the run exits 0. The same bytes with `stream\n` count `raw=1`. On a live page the text channel still saves the run; the raw channel is what covers the leftover stream, and it does not.

**Reproduction:** Build a one-page PDF whose only page content is `(Hemoglobina 13,5 g/dL) Tj`, then append an unreferenced object `<< /Length N /Filter /FlateDecode >>` followed by `stream\r`, a zlib stream of `BT /F1 12 Tf 72 700 Td (Zuleo) Tj ET\n`, and `endstream`. `pdftotext -layout` prints only the clean page. `raw_readings` counts `Zuleo` 0. Replace that CR with LF and the raw count is 1. Make the CR stream the page’s `/Contents` instead: `pdftotext -layout` prints `Zuleo` and the raw count stays 0.

**Validation suggestion:** That unreferenced Flate object, with `stream` followed by CR only, must count the 5-letter name 1 in `raw`. The same object with LF already does. A different 5-letter string stays 0. A regex that accepts only LF fails the CR case.
