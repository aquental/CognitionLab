I'll confirm the G0-25 through G0-28 fixes against the current code, then review the whole stage for anything those changes reopened.The review stays read-only. I'll read the verifier and the tests that claim to close G0-25 through G0-28.The two-view move logic is the part most likely to still hide a short name. I'll run it on the streams the fixes claim to cover.G4-5 and G4-6 still hold. G0-25 and G0-26 hold for the two readings that were added, and the request to count those ambiguous streams as 0 stays refuted. G0-27 and G0-28 hold. Three defects follow.

**G4-5 is fixed.** `channels.run` raises `ToolError` for a non-zero exit, a timeout, or a tool that cannot start, and the timeout is raised `from None`. Each channel records its own error, the file status is `ERROR`, and any unread channel makes the process exit 2. Missing OCR language data is refused before any file is read. An `OSError` is reported with `strerror` only.

**G4-6 is fixed.** A file under a directory argument is named with `os.path.relpath` to that directory, and that string is the `name` channel.

**G0-25 is fixed for a word drawn with `TJ` and separated from the next word by a horizontal move.** `20 0 Td` and `0.25 0 Td` between glyph arrays count 1 for `Zuleo` and for `Laudo`, and 0 for `Zulea`. The split view is what separates them. `pdftotext -layout` reads the `0.25 0 Td` stream as `Zuleo` and `Laudo` on two lines.

**The G0-25 refutation holds.** `(Zu) Tj 4 0 Td (leo) Tj` is still counted as 1. `pdftotext -layout` reads `Zu` and `leo` apart. With Helvetica 12 that gap is a word space, and with a wider font the same `4 0 Td` is glyph positioning. Counting it is the safe error. The same is true of a same-baseline `Tm` at `90 700`: the count stays 1.

**G0-26 is fixed for a whole word placed with `Tm`.** `(Zuleo) Tj` at `72 700` and `(Laudo) Tj` at `200 700` count 1 each, because the split view breaks every `Tm`. The continuation `(Zu) Tj` at `72 700` and `(leo) Tj` at `86 700`, with nothing after it, stays 1.

**G0-27 is fixed.** `(Zu) ' (leo) Tj`, the same line continued by a glyph array, and `0 0 (Zu) " (leo) Tj` count 1 for `Zuleo` and 0 for `Zulea`. The refuted streams `0 0 (Zu) " 0 0 (leo) "` and `(Zu) Tj (leo) '` stay 0.

**G0-28 is fixed.** `Term("Zuleo").count` is 1, 2, and 3 for one, two, and three adjacent copies, and `Zulea` stays 0. `Zuleo, Zuleo` counts 2.

## G0-29

**Severity:** high

**File:line:** `src/anonimizador/channels.py:303`

**Description:** A horizontal `Td`, `TD`, or `Tm` is read in only two ways: every such move joins, or every such move breaks. A short word whose own pieces are separated by one of those moves, with another word later on that baseline, needs the first move to join and the second to break. Neither reading has that word whole. The joined view glues the next word on (`ZuleoLaudo`). The split view leaves the pieces apart (`Zu`, `leo`, `Laudo`).

**Impact:** A 4- or 5-letter name drawn as a same-baseline continuation, then any later word placed with `Tm` or a horizontal `Td`, counts 0 in every raw view. On a leftover content stream the text layer and OCR are clean, so the file is `ok` and the process exits 0. A term of 6 letters or more still matches through its compact form. A short name that is one whole string still matches, because the split view keeps that string whole. The isolated continuation, with no later word, still counts 1.

**Reproduction:** `pdftotext -enc UTF-8 -layout` on a one-page Helvetica PDF whose content is `BT /F1 12 Tf 1 0 0 1 72 700 Tm (Zu) Tj 1 0 0 1 86 700 Tm (leo) Tj 1 0 0 1 200 700 Tm (Laudo) Tj ET` prints `Zuleo   Laudo`. `max(Term("Zuleo").count(t) for t in raw_texts(data))` is 0. The drawn view is `ZuleoLaudo` and the split view is `Zu\nleo\nLaudo`. The same stream, zlib-compressed, as an unreferenced object beside a clean page, with a terms file that lists only `Zuleo` and with `--no-ocr`, exits 0 and prints `[1] ok`. Nothing in that output is the name. The same bytes without the final `(Laudo)` count 1.

**Validation:** That stream must count 1 for `Zuleo` and 0 for `Zulea`, including inside a zlib stream. The stream without `(Laudo)` must stay 1. Whole strings separated by `Tm`, and `TJ` words separated by `20 0 Td`, must stay 1. A mutant that searches only the two whole-word extremes fails the stream that contains both moves.

## G0-30

**Severity:** high

**File:line:** `src/anonimizador/channels.py:243`

**Description:** A PDF string is decoded as UTF-16 only when it starts with a byte order mark. Otherwise it is Latin-1. Identity-H content streams store each character as UTF-16 code units with no mark (`<005A0075006C0065006F>`, or one glyph at a time as `<005A> -2 <0075> …`). Latin-1 then puts a NUL between the letters, so a term under 6 letters is not a whole word. The octal form `(\000Z\000u\000l\000e\000o)` fails the same way: the NULs exist only after unescaping, and `utf16_views` never sees them. A literal that already contains the raw NUL bytes is found, and a string that starts with `FEFF` is found.

**Impact:** A 4- or 5-letter name left only in an old content stream counts 0, and the file is `ok` with exit 0. The same bytes with a mark count 1. A term of 6 letters or more still matches through its compact form, because the NULs are not letters. A listed number still matches, because one NUL is inside the digit gap. A CPF pattern does not: `<003100320033002E…>` is no hint, and the same digits with a `FEFF` mark are `cpf=1`.

**Reproduction:** `raw_texts` of `BT <005A0075006C0065006F> Tj ET`, of `BT [<005A> -2 <0075> -2 <006C> -2 <0065> -2 <006F>] TJ ET`, and of `BT (\000Z\000u\000l\000e\000o) Tj ET` all give `Term("Zuleo")` count 0 and `Term("Rosa")` count 0. The drawn text is `\x00Z\x00u\x00l\x00e\x00o`. An unreferenced object with the hex stream, beside a clean page, with terms `Zuleo` and `Laudo` and `--no-ocr`, exits 0 and prints `[1] ok`. `pdftotext` on that file prints only the clean page.

**Validation:** Those three streams must count 1 for `Zuleo` and 0 for `Zulea`, and the marked form `<FEFF005A…>` must stay 1. The hex CPF without a mark must hint `cpf=1`. A mutant that decodes UTF-16 only after a byte order mark fails the three streams.

## G0-31

**Severity:** low

**File:line:** `src/anonimizador/channels.py:48`

**Description:** `--timeout` is only checked to be a positive integer. `subprocess.run` raises `OverflowError` when the timeout does not fit in a C time value. That exception is not a `ToolError` or an `OSError`, so `run_verify` does not catch it.

**Impact:** `anonimizador verify --timeout 2147483648` prints the header, then a traceback, and exits 1. Exit 1 means identifiers were found. The traceback text does not contain the file path. A negative or zero timeout is still a usage error with exit 2.

**Reproduction:** On Python 3.13, `verify --no-ocr --timeout 2147483648 --terms <file> <pdf>` prints `anonimizador verify: 1 file(s), 1 term(s), no OCR`, then `OverflowError: timeout is too large`, and the process status is 1.

**Validation:** That command must exit 2, print one usage line, and print no traceback. A mutant whose `positive_int` accepts every positive integer fails it.
