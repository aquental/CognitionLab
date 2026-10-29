I'll confirm G0-39, G0-40, and G0-41 against the current code, then review the rest of stage 0 for missed identifiers, crashes, wrong exit codes, and leaks.The review stays read-only. I'll read the gap, pattern, and image-reading paths that G0-39 through G0-41 changed, then the rest of the verifier.The three fixes are in the tree. Next I’ll check layouts the new tests skip: stream boundaries, hex-string whitespace, and joins that can still drop a short name.The three fixes are wired through. I’ll check a few layouts the tests don’t cover, including stream endings and hex-string whitespace, without writing anything.Vou confirmar as correções G0-39 a G0-41 no código atual e fechar a linha exata do limiar de espaço em TJ antes de emitir o relatório.O limiar de −200 está no código. Vou medir, no mesmo fluxo de bytes, onde o `pdftotext` separa as palavras e onde a contagem bruta continua em zero.G0-39, G0-40, and G0-41 are confirmed. G4-5 and the verifier part of G4-6 still hold. One new defect.

## Confirmations

**G0-39 (confirmed).** `gap_joint` returns only a continuation or `AMBIGUOUS`. Whitespace, NUL, and `%` comments continue the text. Any other byte, an operator outside the text-showing, text-state, and color set, a TJ number of −200 or less, and a gap over 256 bytes are ambiguous. Nothing between two PDF strings is a sure break. `tests/test_channels.py::test_comments_nul_and_other_bytes_between_glyphs_keep_a_word` covers a comment, NUL, a comma, and a brace, including a Flate stream.

**G0-40 (confirmed).** `count_patterns` searches the fully joined reading only for `TEXT_PATTERNS` (`email`, `crm`). `check_file` passes `joined=reading.raw.drawn`, and `raw_readings` stores the drawn strings there. Numbers are left out of that reading. The 22- and 32-character cases are covered.

**G0-41 (confirmed).** `read_image` stores `raw_readings(data)` through `take_raw`, the same reading a PDF stores. `check_file` searches those joins for whole-word terms and those strings, joins, and drawn text for patterns on every file. The JPEG case goes through `main()`.

**G4-5 (still holds).** `run` raises `ToolError` with the tool name only, for a non-zero exit, a timeout, or a tool that cannot start. The channel records that error, so the file is `ERROR` and the run exits 2.

**G4-6, verifier part (still holds).** A file under a directory is named by its path relative to that directory, and every other path that reaches it is searched as an alias on the `name` channel.

## G0-42

**Severity:** high

**File:line:** `src/anonimizador/channels.py:303` (`WORD_SPACE_ADJUSTMENT` at line 211; the continuation is the `return ""` at line 306)

**Description:** A TJ number greater than −200 is a sure continuation. In a glyph-by-glyph array, `pdftotext -layout` already prints two words at a smaller gap: at 12 pt, from −100 through −199 (`Zuleo Assunto`); at 8 pt and 10 pt, from −110. `gap_joint` still returns `""` there, so the pieces stay one word in every reading. A term under 6 letters must be a whole word, so that name counts 0. A term of 6 letters or more still matches through its compact form, which is why the neighboring word counts 1 in the same glued string.

**Impact:** A short name left only in an old or unreferenced stream, with the next word placed by a TJ adjustment in that range, is invisible on every channel. The file is reported `ok` and the run exits 0. On a live page the text channel still sees the split. The raw channel is what covers a leftover stream, and it does not.

**Reproduction:** Content stream, Helvetica, 12 pt:

`BT /F1 12 Tf 72 700 Td [(Z) -2 (u) -2 (l) -2 (e) -2 (o) -150 (A) -2 (s) -2 (s) -2 (u) -2 (n) -2 (t) -2 (o)] TJ ET`

`pdftotext -layout` prints `Zuleo Assunto`. The raw count is 0 for `Zuleo`, 1 for `Assunto`, and 0 for `Zulea`. The same holds at −100 and at −199. At −200 the raw count of `Zuleo` becomes 1. The same stream, zlib-compressed as an unreferenced `9 0 obj << >> stream` after a clean page `(Hemoglobina 13,5 g/dL) Tj`, is absent from `pdftotext`, and the raw count of `Zuleo` stays 0 while `Assunto` stays 1.

**Validation suggestion:** That 12 pt layout with adjustments −150, −100, and −199, in an unreferenced Flate stream beside a clean page, must count the 5-letter name 1 and a different 5-letter string 0. The 7-letter word must still count 1, and kerning of −2 inside one word must still count 1. Read every negative TJ number as ambiguous, the same way a move is read (joined, split, and mixed). That can add a match on a tight gap. It cannot hide the name. A sure break at −150 would be the wrong fix.
