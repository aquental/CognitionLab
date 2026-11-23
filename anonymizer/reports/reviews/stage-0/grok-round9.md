I'll review the three pending fixes against the current code, then walk the rest of stage 0 for new defects. Reading the verifier, matching, and the new tests first.G4-5 and G4-6 still hold. G0-29 holds for the stream the new test searches, including a zlib blob. G0-30 holds for a two-byte string whose code units all sit in Latin-1. G0-31 holds: `--timeout 2147483648` and `86401` exit 2 with no traceback, and `86400` is accepted. The mixed-reading pattern is possessive and anchored at a word start. Five defects follow.

**G4-5 is fixed.** A non-zero exit, a timeout, or a tool that cannot start raises `ToolError` with the tool name only, and the timeout is raised `from None`. Each channel records its own error, the file is `ERROR`, and any unread channel makes the process exit 2. Missing OCR language data is refused before any file. An `OSError` is reported with `strerror` only.

**G4-6 is fixed.** A file found under a directory is named with `os.path.relpath` to that directory, and that string, plus every alias, is the `name` channel.

**G0-29 is fixed for the operators in isolation.** `Tm` at 72, 86, and 200 counts 1 for `Zuleo`, 0 for `Zulea`, and 1 for `Assunto`. The same bytes inside a FlateDecode stream count 1. Glyphs placed with `7 0 Td`, then another word, count 1. `mixed_readings` is what joins `Zu` and `leo` while leaving `Assunto` out.

**G0-30 is fixed when every other byte of the whole string is NUL.** `<005A0075006C0065006F>`, one-glyph `<005A> -2 <0075> …`, `(\000Z\000u\000l\000e\000o)`, the little-endian form, and `<FEFF005A…>` count 1 for `Zuleo` and 0 for `Zulea`. The unmarked hex CPF hints `cpf=1`.

**G0-31 is fixed.** `bounded_int` rejects anything outside 1…86400 for `--timeout` and 1…2400 for `--dpi`. `2147483648` exits 2 from argparse, with no `OverflowError`.

## G0-32

**Severity:** high

**File:line:** `src/anonimizador/channels.py:321`

**Description:** A horizontal `Td` or `TD` never records the baseline. Only `Tm` does. A later `Tm` is a same-line move only when its y equals that saved baseline. After `72 700 Td`, the baseline is still `None`, so `1 0 0 1 86 700 Tm` is a hard newline. The mixed view never sees the two pieces as one run.

**Impact:** A 4- or 5-letter name drawn on a line that was opened with `Td` and continued with `Tm` counts 0 in raw bytes. On a leftover content stream the text layer is clean, so the file is `ok` and the process exits 0. A term of 6 letters or more still matches through its compact form. A name that is one whole string still matches. `pdftotext -layout` reads the stream below as `Zuleo`.

**Reproduction:** Put `BT /F1 12 Tf 72 700 Td (Zu) Tj 1 0 0 1 86 700 Tm (leo) Tj ET` in a FlateDecode stream that no page references, beside a clean page, with a terms file that lists only `Zuleo` and with `--no-ocr`. The page text is the clean page. `verify` prints `[1] ok` and exits 0. Nothing in that output is the name. The same operators alone also count 0. `72 700 Td (Zu) Tj 14 0 Td (leo) Tj` still counts 1, and `(Zuleo) Tj` followed by a `Tm` still counts 1.

**Validation:** The leftover stream must count 1 for `Zuleo` and 0 for `Zulea`. A `Tm` whose y really changes (`72 680`) must stay 0. A mutant that treats an unknown baseline as a hard break fails the first stream.

## G0-33

**Severity:** high

**File:line:** `src/anonimizador/channels.py:246`

**Description:** A string is read as UTF-16 only when every other byte of the entire string is NUL. One code unit above U+00FF, such as an en dash or `≥`, makes an even byte non-zero, and the whole string is Latin-1. The short name then has a NUL between letters. `utf16_views` never sees those bytes: in the file they are hex or octal, and the NULs appear only after decoding.

**Impact:** A 4- or 5-letter name in a two-byte string that also holds any character outside Latin-1 counts 0. On a leftover stream the file is `ok` and the process exits 0. The same name in its own string still counts 1, and so does a string of Latin-1 code units only. A term of 6 letters or more still matches through its compact form. A listed number still matches, because one NUL is inside the digit gap. A CPF pattern in that string does not.

**Reproduction:** `raw_texts` of `BT <005A0075006C0065006F2014> Tj ET` (`Zuleo` plus U+2014) counts 0 for `Zuleo`. The octal form of those bytes counts 0. The drawn text is `\x00Z\x00u\x00l\x00e\x00o \x14`. An unreferenced FlateDecode stream with that hex string, beside a clean page, with terms `Zuleo` and `--no-ocr`, prints `[1] ok` and exits 0. `<005A0075006C0065006F> Tj` in the same place counts 1.

**Validation:** The hex string, the octal string, and a string that also contains U+2265 must count 1 for `Zuleo` and 0 for `Zulea`. A pure Latin-1 `(Zuleo)` must stay 1. A mutant that requires every other byte of the whole string to be NUL fails the first three.

## G0-34

**Severity:** medium

**File:line:** `src/anonimizador/channels.py:451`

**Description:** Patterns are searched on the one-per-line strings and on the drawn view. The drawn view deletes every ambiguous mark, so a kerned number and the digits after one horizontal move become one digit run. The split view keeps the number and the break (`123.456.789-09` and `99` on two lines) and is not searched. `(?!\d)` then fails on the drawn view, and no single literal is a whole CPF.

**Impact:** An unlisted CPF drawn with kerning, then a horizontal `Td` or a same-baseline `Tm` into another number, gives no hint. `--strict-patterns` prints `[1] ok` and exits 0 when that stream is the only place the number exists. A listed CPF still matches, because the digit run contains it. A kerned CPF with no following digits still hints `cpf=1`. On a live page, `pdftotext` usually still shows the CPF, so this bite is a leftover stream.

**Reproduction:** A FlateDecode stream that no page references, containing `BT 1 0 0 1 72 700 Tm [(123) -2 (.) -2 (456) -2 (.) -2 (789) -2 (-) -2 (09)] TJ 20 0 Td (99) Tj ET`, beside a clean page. `pdftotext` prints the clean page. `verify --no-ocr --strict-patterns` prints `[1] ok` and exits 0. No digit is printed. `count_patterns` on the split view of those bytes is `cpf=1`. The same stream with the final `(99)` removed hints `cpf=1`.

**Validation:** That leftover stream must hint `cpf=1` under `--strict-patterns` and exit 1, with no digits printed. The stream without `(99)` must stay `cpf=1`. A clean page must stay without a CPF hint. A mutant that searches only `lines` and `drawn` fails the leftover stream.

## G0-35

**Severity:** medium

**File:line:** `src/anonimizador/channels.py:416`

**Description:** When the groups of a CPF are themselves separated by horizontal moves, every one of those moves is ambiguous. The drawn view joins all of them, so the following number sticks to the CPF. The split view breaks all of them, so the dot sits on its own line (`123`, `.`, `456`, …). The current CPF pattern matches neither, and the mixed view only emits joins shorter than 6 letters. Searching the split view does not fix this stream.

**Impact:** A CPF drawn a group at a time, with another number later on that baseline, is no hint in any channel. On a live page `--strict-patterns` exits 0. `pdftotext -layout` still shows the digits. A listed CPF still matches through the glued digits.

**Reproduction:** Page content `BT /F1 12 Tf 72 700 Td (123) Tj 18 0 Td (.) Tj 6 0 Td (456) Tj 22 0 Td (.) Tj 6 0 Td (789) Tj 22 0 Td (-) Tj 6 0 Td (09) Tj 30 0 Td (99) Tj ET`. `pdftotext -layout` prints `123. 456 . 789 - 09   99`. `verify --no-ocr --strict-patterns` prints `[1] ok` and exits 0. `count_patterns` on the lines, the drawn view, and the split view is empty. The drawn view is `123.456.789-0999`.

**Validation:** That page must hint `cpf=1` and, with `--strict-patterns`, exit 1. The same operators without the final `(99)` must stay a hint. A clean page must stay without a CPF hint. A mutant that only adds the split view to the pattern strings still fails this page.

## G0-36

**Severity:** high

**File:line:** `src/anonimizador/channels.py:304`

**Description:** A gap longer than 256 bytes is a hard break and is not tokenized, and it clears the baseline. In an uncompressed stream the gap before the first string starts at the beginning of the file, so the `Tm` that opens the line is thrown away with the header. The next `Tm`, in a short gap, then sees no baseline and is a hard newline. The mixed view never runs across the two pieces. A FlateDecode stream is its own blob, the first gap is short, and the same operators count 1. The regression test searches the bare operators and the zlib blob only.

**Impact:** The G0-29 stream stored as an ordinary uncompressed stream object counts 0. Beside a clean page, with terms listing only `Zuleo` and with `--no-ocr`, the process prints `[1] ok` and exits 0. `pdftotext` prints the clean page. The same bytes compressed with FlateDecode count 1. A name that is one whole string still matches.

**Reproduction:** Append `9 0 obj << /Length … >> stream` plus `BT 1 0 0 1 72 700 Tm (Zu) Tj 1 0 0 1 86 700 Tm (leo) Tj 1 0 0 1 200 700 Tm (Assunto) Tj ET` and `endstream` to a clean one-page PDF, with no FlateDecode and with no page referring to the object. `max(Term("Zuleo").count(t) for t in raw_texts(pdf))` is 0. The drawn text of the file is `Zu`, a newline, then `leoAssunto`. `verify --no-ocr --terms <file> <pdf>` exits 0 and prints `[1] ok`.

**Validation:** That uncompressed object must count 1 for `Zuleo` and 0 for `Zulea`, and the FlateDecode form must stay 1. A mutant that returns a newline for every gap longer than 256 bytes without reading its tail fails the uncompressed object and still passes the current test.
