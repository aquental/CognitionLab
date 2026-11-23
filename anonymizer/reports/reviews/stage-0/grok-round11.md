I'll review the current stage against G0-37 and G0-38 first, then walk the rest of `verify` for missed identifiers, crashes, exit codes, and leaks.The two fixes are in the tree. Next I’ll check layouts the new tests skip, especially syntax bytes the gap still treats as a hard break.G4-5, G4-6, G0-37, and G0-38 hold for the layouts in their tests. A long syntax gap stays ambiguous and keeps the open TJ array (`Zuleo` 1, `Zulea` 0). `ET`/`BT`, `q`, `BMC`, and `BDC` are ambiguous, so those streams count 1. A failed tool is still `ERROR` with exit 2, and a directory name is still searched.

Three layouts still drop an identifier.

## G0-39

**Severity:** high

**File:line:** `src/anonimizador/channels.py:304`

**Description:** `gap_joint` still treats a gap as a sure break as soon as `NOT_SYNTAX` matches. That class is every byte outside `\s`, word characters, and `.+-[]/*'\"<>#`. PDF whitespace includes NUL, and `%` starts a comment to the end of the line. Both are content-stream syntax, and so are the delimiters `( ) { }` and regular characters such as `,`. One of those bytes makes the whole gap a newline. Mixed readings do not cross a newline, so the pieces are never joined. `Tj`, `-2`, and `Td` in that gap are never tokenized.

**Impact:** A 4- or 5-letter name drawn in a TJ array, with a comment or a NUL between two glyph strings, counts 0 in raw bytes. A changed letter stays 0. A term of 6 letters or more still matches through its compact form. On a live page the text channel still counts 1, because `pdftotext -layout` prints the word. The same operators in an unreferenced Flate stream beside a clean page are `ok` and the process exits 0.

**Reproduction:** `pdftotext -layout` prints `Zuleo` for `BT /F1 24 Tf 72 700 Td [(Z) -2 (u)%\n -2 (l) -2 (e) -2 (o)] TJ ET` and for `BT /F1 24 Tf 72 700 Td [(Z) -2 (u)\x00 -2 (l) -2 (e) -2 (o)] TJ ET`. `raw_readings` counts `Zuleo` 0 and `Zulea` 0, including when either stream is Flate-compressed inside `stream`/`endstream`. The same kerning with the `%` or the NUL removed counts 1. `pdftotext` also prints `Zuleo` for a comma or a `{` between those strings (it warns about the token) and the raw count is 0.

**Validation:** Both streams must count 1 for `Zuleo` and 0 for `Zulea`, as bare bytes and as an unreferenced Flate stream. A comment’s brackets must not change whether a later `-300` is a word space. A mutant that keeps the hard break on `%` or on NUL fails both.

## G0-40

**Severity:** medium

**File:line:** `src/anonimizador/channels.py:351`

**Description:** A pattern that is whole only when ambiguous pieces are joined is kept only while the join is at most 20 characters (`MAX_MIXED_READING`). `mixed_readings` adds the next piece, and if the total is longer than 20 it drops that reading instead of recording it. `for_patterns` searches the per-string lines and the split view only, so the fully joined drawing, which does contain the address, is never searched. The cap is documented as covering every identifier pattern. It covers a CPF (14) and a CNS (18). An e-mail is longer than that.

**Impact:** An e-mail split by one positioning move, longer than 20 characters, produces no pattern hint. `pdftotext -layout` inserts a space before `@`, so the text channel does not match it either. `--strict-patterns` still exits 0. A shorter address, and the same address with small TJ kerning instead of a move, still hint `email=1`.

**Reproduction:** For `BT /F1 12 Tf 72 700 Td (ana.silva) Tj 4 0 Td (@hospital.com) Tj ET` (22 characters), `pdftotext -layout` prints `ana.silva @hospital.com`, `count_patterns` on that text is empty, and `count_patterns(views.for_patterns, [views.mixed])` is empty while `views.drawn` is `ana.silva@hospital.com`. The same split of `fulano.silva@exa.com` (20 characters) hints `email=1`. `fulano.silva@clinica.com` (24) and `fulano.silva@clinica.example.com` (32) hint nothing.

**Validation:** The 22-character stream must hint `email=1` through `check_file`. A page of separate numbers must not gain a new `cpf` hint from searching the fully joined drawing. A mutant that leaves the cap at 20 fails the 22-character stream and still passes `test_numbers_across_ambiguous_moves_are_pattern_hints`.

## G0-41

**Severity:** high

**File:line:** `src/anonimizador/channels.py:508`

**Description:** `read_pdf` keeps all three results of `raw_readings`: the raw texts, the pattern strings, and the joins. `read_image` stores only `raw_texts`, which is the first result. `reading.strings` and `reading.raw_joins` stay empty. The drawn and split views are still searched. The mixed joins are not, and neither are pattern matches that exist only in those strings.

**Impact:** The G0-29 layout counts 0 in a PNG or JPEG and 1 in a PDF: a 4- or 5-letter name placed with `Td`, beside another word, is whole in neither the drawn view nor the split view. A changed letter stays 0. A term of 6 letters or more still matches through its compact form. A CPF drawn group by group hints `cpf=1` on the PDF path and nothing on the image path. OCR and metadata do not see operators that are only in the file bytes. The image is `ok` and the process exits 0.

**Reproduction:** `raw_texts` of `BT (Zu) Tj 7 0 Td (leo) Tj 30 0 Td (Assunto) Tj ET`, which is what `read_image` searches, counts `Zuleo` 0. `raw_readings`, which is what `read_pdf` searches, counts `Zuleo` 1. The grouped CPF stream `BT 72 700 Td (123) Tj 18 0 Td (.) Tj … (09) Tj ET` hints `cpf=1` from `raw_readings` and no hint from an image’s empty `reading.strings`.

**Validation:** Those bytes in a `.jpg` or `.png` must count 1 for `Zuleo` and 0 for `Zulea`, and the grouped CPF must hint `cpf=1`. A mutant that keeps `raw_texts` alone fails both and still passes the PDF tests that call `raw_readings` directly.
