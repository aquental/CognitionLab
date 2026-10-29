I'll confirm the G0-22 through G0-24 resolutions against the current verifier, then review the whole stage for any new defect that would hide a hit or report a failure as clean.The worktree is the `anonimizador` package itself. I'll read the verifier and the round-6 fixes first, then look for anything that still reports a dirty or unreadable file as clean.G4-5 and G4-6 still hold. G0-22 holds for a TJ word space and a vertical move, and the no-space refutation holds. G0-23 holds. G0-24 holds for text-state and color operators, and the `"` refutation holds. The same-baseline `Tm` rule does not. Three defects follow.

**G4-5 is fixed.** `channels.run` raises `ToolError` for a non-zero exit, a timeout, or a tool that cannot start, and the timeout is raised `from None`. Each channel records its own error, the file status is `ERROR`, and any unread channel makes the process exit 2. Missing OCR language data is refused before any file is read. An `OSError` is reported with `strerror` only.

**G4-6 is fixed.** A file under a directory argument is named with `os.path.relpath` to that directory, and that string is the `name` channel.

**G0-22 is fixed for a TJ word space and a vertical move.** `[(Z) -2 (u) -2 (l) -2 (e) -2 (o) -300 (L) …] TJ` and the same words separated by `0 -20 Td` count 1 for `Zuleo` and 0 for `Zulea`. With Helvetica 12, `pdftotext -layout` reads the vertical case as `Zuleo Laudo`.

**The no-space refutation holds.** `(Zu) Tj (leo) Tj (Assunto) Tj` is drawn as one run. `pdftotext` reads `ZuleoAssunto`. A term under 6 letters has to be a whole word there, so `Zuleo` counts 0. `Assunto` still counts, because a term of 6 letters or more matches through its compact form.

**G0-23 is fixed.** `raw_readings` returns the per-line strings and the drawn text, and `check_file` searches both for patterns. The three glyph-by-glyph CPF forms hint `cpf=1`. A TJ adjustment of -300 stays a space, so `123.456.789-09` beside `1234` still hints `cpf=1`.

**G0-24 is fixed for text-state and color operators.** With Helvetica 12, `pdftotext` reads `(Zu) Tj 0.1 Tc (leo) Tj` and `(Zu) Tj 1 0 0 rg (leo) Tj` as `Zuleo`. The code counts 1.

**The `"` refutation holds.** `0 0 (Zu) " 0 0 (leo) "` shows each string on its own next line. `pdftotext` reads `leo Zu`. The code counts 0. The same is true of `(Zu) Tj (leo) '`, where `'` shows `(leo)` on the next line.

**The same-baseline `Tm` rule does not hold.** It is G0-26 below.

The performance guards are still the ones from round 6: `fold` is cached, the e-mail pattern is anchored and bounded at 64 characters, and a literal is one regular expression.

## G0-25

**Severity:** high

**File:line:** `src/anonimizador/channels.py:302`

**Description:** `Td` and `TD` break a drawn word only when the vertical operand is not 0. Any horizontal move stays inside the word. `SAME_TEXT_OPS` includes both operators, and the vertical test is `numbers[-1] != 0`, so `tx` is ignored.

**Impact:** A 4- or 5-letter name drawn glyph by glyph, then separated from the next word by a horizontal move, counts 0 in every raw view. The per-string view is one glyph per line, and the drawn view glues the words. On a leftover content stream this is a normal way to move to the next word. The text layer and OCR are clean after a rewrite, `raw` is the only channel that still has the stream, and the file is `ok` with exit 0. A term of 6 letters or more still matches through its compact form. A short name that is one whole string still matches, because the per-string view keeps it.

**Reproduction:** Build the stream with the fixture's Helvetica and `pdftotext -enc UTF-8 -layout`. It prints `Zuleo Laudo` for each of these:

- `BT /F1 12 Tf 72 700 Td [(Z) -2 (u) -2 (l) -2 (e) -2 (o)] TJ 0.25 0 Td [(L) -2 (a) -2 (u) -2 (d) -2 (o)] TJ ET`
- the same stream with `20 0 Td` instead of `0.25 0 Td`
- `BT /F1 12 Tf 72 700 Td (Zu) Tj 4 0 Td (leo) Tj ET`, which prints `Zu leo`

`max(Term("Zuleo").count(t) for t in raw_texts(data))` is 0 for the glyph streams, and `Term("Laudo")` is 0 too. `raw_readings` of the `20 0 Td` stream has `raw` 0, so the file is `ok` with OCR off. `test_a_word_drawn_glyph_by_glyph_is_whole_in_raw_bytes` currently requires the `4 0 Td` stream to count 1.

**Validation:** The `20 0 Td` glyph stream must count 1 for `Zuleo` and for `Laudo`, and 0 for `Zulea`, including inside a zlib stream. The `4 0 Td` assertion has to follow `pdftotext` and expect 0. A mutant that breaks only on a non-zero `ty` fails it.

## G0-26

**Severity:** medium

**File:line:** `src/anonimizador/channels.py:305`

**Description:** A `Tm` on the same baseline always glues the surrounding strings. The line test compares only the last of the six operands, the vertical translation, with the stored baseline. The horizontal translation is ignored. The regression stream chosen for this rule already sits past the end of the first glyphs.

**Impact:** The same false clean as G0-25, for words positioned with `Tm` instead of `Td`. A short name drawn in pieces, then another word on that baseline, counts 0. A later absolute position that merely continues the glyphs is a real same-word case, and a blanket split would miss that one. Gluing every same baseline misses the other.

**Reproduction:** With Helvetica 12, `pdftotext -layout` prints `Zu leo` for `BT /F1 12 Tf 1 0 0 1 72 700 Tm (Zu) Tj 1 0 0 1 90 700 Tm (leo) Tj ET`. The code's drawn text is `Zuleo` and the count is 1. The same command prints `Zuleo` when the second position is `86 700`, and `Zuleo Laudo` for glyph arrays at `72 700` and `90 700`. On that glyph stream `Term("Zuleo")` and `Term("Laudo")` are both 0. `test_text_state_and_same_baseline_keep_a_word_whole` requires the `90 700` stream to count 1.

**Validation:** The `90 700` stream must count 0 for `Zuleo`. The `86 700` stream must stay 1. The glyph-word stream at `72 700` and `200 700` must count 1 for `Zuleo` and 0 for `Zulea`. A mutant that glues every same baseline fails the `90 700` case.

## G0-27

**Severity:** medium

**File:line:** `src/anonimizador/channels.py:331`

**Description:** `'` and `"` are excluded from `SAME_TEXT_OPS`, so the gap that contains the operator becomes a line break after the string it shows. Both operators move to the next line and then show the string that precedes them. The next `Tj` or `TJ` continues on that same line. The extra assignment to the previous joint only inserts the break before the operand. The break after the operand remains.

**Impact:** A 4- or 5-letter name whose first piece is shown with `'` or `"` and whose remaining pieces follow on that line counts 0. The per-string view has the pieces apart, and the drawn view puts a newline between them. A leftover stream in that shape is `ok` with exit 0. The refuted streams stay two lines: `0 0 (Zu) " 0 0 (leo) "` and `(Zu) Tj (leo) '`.

**Reproduction:** `pdftotext -layout` prints `Zuleo` for each of these, and the drawn view is `\nZu\nleo`, so `Term("Zuleo")` is 0:

- `BT /F1 12 Tf 72 720 Td (Zu) ' (leo) Tj ET`
- `BT /F1 12 Tf 72 720 Td (Zu) ' [(l) -2 (e) -2 (o)] TJ ET`
- `BT /F1 12 Tf 72 720 Td 0 0 (Zu) " (leo) Tj ET`

`pdftotext` prints `leo Zu` for `0 0 (Zu) " 0 0 (leo) "` and for `(Zu) Tj (leo) '`. Those two must stay 0.

**Validation:** The three streams above must count 1 for `Zuleo` and 0 for `Zulea`. The two refuted streams must stay 0. A mutant that treats `'` and `"` as ordinary breaks fails the continuation streams.

## G0-28

**Severity:** low

**File:line:** `src/anonimizador/terms.py:116`

**Description:** A term shorter than 6 letters is counted with `words(text).count(self.words)`. Both sides are padded with a single space. `str.count` does not overlap, so the space after one match is the space the next match needs.

**Impact:** Two occurrences of a 4- or 5-letter term in one channel are reported as 1. The file is still `FOUND` when any occurrence remains. Three occurrences are reported as 2. Terms of 6 letters or more go through `compact` and are not affected. Numeric terms are not affected.

**Reproduction:** `Term("Zuleo").count("Zuleo Zuleo")` is 1. `Term("Zuleo").count("Zuleo Zuleo Zuleo")` is 2. `Term("Zuleo").count("Zuleo")` is 1.

**Validation:** Two and three copies must count 2 and 3. One copy must stay 1, and `Zulea` must stay 0. A mutant that keeps `str.count` on the padded strings fails the two-copy case.
