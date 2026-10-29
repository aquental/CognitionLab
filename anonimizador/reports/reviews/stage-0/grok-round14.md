I'll confirm the G0-43 stream-ending fix against the current code, then review the stage for missed identifiers, crashes, wrong exit codes, and leaks.The stream-ending change is in place. Next I’ll check layouts that can still drop a short name or a pattern, and whether any error path leaks or exits wrong.G0-43 is fixed. G4-5 and the verifier part of G4-6 still hold. Two misses remain.

**G0-43 (confirmed).** `raw_blobs` treats CR, LF, and CRLF after `stream` as the end of the keyword (`channels.py:393`). The CR alternative is tried only when the next bytes are not CRLF, so a normal CRLF stream is unchanged. `tests/test_channels.py::test_a_stream_whose_keyword_ends_with_cr_alone_is_inflated` covers all three endings: the 5-letter name counts 1 and a different final letter counts 0.

**G4-5 (confirmed).** A non-zero exit, a timeout, or a tool that cannot start raises `ToolError` with the tool name only. Each channel records that error, the file is `ERROR`, and the run exits 2.

**G4-6 (confirmed).** A file found under a directory is named by its relative path, directories included, and that path is the `name` channel. `--show-names` prints the same path.

## G0-44

**Severity:** high

**File:line:** `src/anonimizador/channels.py:270` (`pdf_string_spans`), `src/anonimizador/channels.py:311` (`drawn_text`)

**Description:** Every literal and hex string in byte order is treated as text that was drawn. A string that a reader does not draw is spliced between the surrounding pieces. Two cases do this: a `%` comment that itself contains `(...)` or `<...>`, which is not a PDF string at all, and a real string that is a dictionary operand rather than an operand of `Tj`, `TJ`, `'`, or `"`. `gap_joint` still treats the comment, the brackets, and the text operators as a continuation, so the extra text sits inside the word in the per-string lines, the joined reading, the split reading, and every mixed join. There is no reading in which the drawn word is whole. A term of 6 letters or more is missed too, because the compact form has the extra letters in the middle.

**Impact:** A name drawn in pieces, with that extra string between the pieces, counts 0 in raw bytes. On a live page, `pdftotext` still has it. In an old or unreferenced stream the text layer is clean, so the file is `ok` and the run exits 0. A name that is one whole string still counts 1. A comment with no string delimiter, and a dictionary with no string, still count 1.

**Reproduction:** `pdftotext -layout` prints `Zuleo` for each of these content streams:

- `BT [(Z) -2 (u) -2 (l) % (XX)\n -2 (e) -2 (o)] TJ ET`
- `BT (Zu) Tj % (XX)\n (leo) Tj ET`
- `BT (Zu) Tj /Span << /Lang (pt) >> BDC (leo) Tj EMC ET`

`raw_readings` of those bytes counts `Zuleo` 0 and `Zuleos` 0. The same glyph stream, zlib-compressed, appended as an unreferenced `9 0 obj << /Filter /FlateDecode >> stream` after a clean page `(Hemoglobina 13,5 g/dL) Tj`, still counts 0. The drawn text is `ZulXXeo`, `ZuXXleo`, and `Zuptleo`.

**Validation suggestion:** Those three streams must count 1 for `Zuleo` and 0 for `Zulea`, including inside a Flate stream beside a clean page. The same glyph array with `% note` and no parenthesis, and the same `BDC` with `/MCID 1` and no string, must stay 1. A 6-letter word split around the comment must count 1. An octal name that exists only inside a comment, such as `% (Zul\351o)`, must stay 1. Skipping comments and still splicing dictionary strings leaves the third stream at 0.

## G0-45

**Severity:** medium

**File:line:** `src/anonimizador/channels.py:334` (`mixed_readings`), `src/anonimizador/terms.py:144` (`count_patterns`)

**Description:** A number pattern that contains a space, split by one ambiguous move, is whole only in the fully joined reading. `mixed_readings` keeps only the tail after the last space of a piece and only the head before the first space of the next, then stops. The space that belongs to the phone or the CPF is dropped, so no join holds the number. The split reading has that space plus the newline that replaced the move, and each number pattern allows only one whitespace. The fully joined reading has the normal number (`(11) 98765-4321`, `123.456.789- 09`) and would match, and number patterns are not searched there.

**Impact:** An unlisted phone or CPF in that layout gives no hint. With `--strict-patterns`, a leftover stream of it prints `[1] ok` and exits 0. Nothing in the output is a digit. A listed number still matches, because the digit run allows the space and the newline. The same number with the move and no extra space character already hints 1. An e-mail split by a move still hints 1, because the joined reading is searched for e-mail and CRM.

**Reproduction:** `pdftotext -layout` prints `11 98765-4321` for `BT (11) Tj 20 0 Td ( 98765-4321) Tj ET`, and prints `123.456.789-` / `09` for `BT (123.456.789- ) Tj 0 -14 Td (09) Tj ET`. `count_patterns` on the raw readings of either stream is empty. The same phone with `(98765-4321)` and no leading space hints `phone=1`. The kerned CPF closed by `-300` before `(09)`, with no space character in the pieces, hints `cpf=1`.

**Validation suggestion:** Both streams must hint `phone=1` and `cpf=1`, including as an unreferenced Flate stream under `--strict-patterns`, with exit 1 and no digits printed. The stream without the extra space must stay 1. Two separate numbers that meet only in the fully joined reading must stay without a hint, and a clean page must stay without a CPF hint. Searching that fully joined reading for every number pattern brings the page-wide false hints back. The number has to be rebuilt across the one move while the space already in the piece is kept.
