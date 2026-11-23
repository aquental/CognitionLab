I'll confirm G0-19 through G0-21 and the performance fix against the current verifier, then review the whole stage for any new defect that would hide a hit or report a failure as clean.The review stays read-only. I'll read the verifier and the new string-joining tests next, and check the three pending fixes against that code.G4-5 and G4-6 still hold. G0-20 and G0-21 hold for the cases their tests cover. G0-19 holds only when the split word is the only text in the text object. The performance change holds: the e-mail pattern is anchored and bounded, `fold` is cached, and a long run of opening parentheses returns in milliseconds. Three defects remain.

**G4-5 is fixed.** `channels.run` raises `ToolError` for a non-zero exit, a timeout, or a tool that cannot start, and the timeout is raised `from None`. Each channel records its own error, the file status is `ERROR`, and any unread channel makes the process exit 2. Missing OCR language data is refused before any file is read. An `OSError` is reported with `strerror` only.

**G4-6 is fixed.** A file under a directory argument is named with `os.path.relpath` to that directory, and that string is the `name` channel.

**G0-19 is fixed for a text object that contains only the split word.** `[(Z) -2 (u) -2 (l) -2 (e) -2 (o)] TJ`, `(Zu) Tj 4 0 Td (leo) Tj`, a one-byte hex array, and a newline inside one string all count 1 for `Zuleo`. The same word counts 0 when that text object also draws another word. That hole is G0-22.

**G0-20 is fixed for one level of parentheses.** `(Zulei\347\343o (mae))`, a second group at that same level, and an escaped UTF-16 literal with a remark all decode. A second nesting level still drops an outer octal name; `docs/SPEC.md` limits the requirement to one level, so that limit stays closed.

**G0-21 is fixed for a CPF stored as one literal or one UTF-16 hex string.** Those strings are in `Reading.strings` and are searched with the pattern channels. A CPF drawn as separate glyph strings is not a hint. That hole is G0-23.

**The performance fix holds.** `fold` is cached. The e-mail pattern is a single anchored, length-bounded attempt. `pdf_string_views` on 20 000 opening parentheses returned in 6 ms.

G0-1 through G0-18 still match the current code.

## G0-22

**Severity:** high

**File:line:** `src/anonimizador/channels.py:193` (applied at line 264)

**Description:** Pieces of drawn text are concatenated whenever the bytes between them match `TEXT_GLUE`. That pattern treats `Tj`, `TJ`, `Td`, `TD`, `T*`, `'`, brackets, and every number as glue. A word space in a content stream is usually one of those numbers (a large negative adjustment in a `TJ` array, or a `Td` move), so it is deleted. A 4- or 5-letter term must be a whole word. The per-string view still has one glyph per line, and the joined view has the whole text object as one word.

**Impact:** A short name drawn glyph by glyph counts 0 once the same text object draws anything else. Every other 4- or 5-letter word in that object counts 0 too. On a leftover content stream this is the usual shape: the name shares a `BT`/`ET` block with the rest of the line. The text layer and OCR are clean after a rewrite, `raw` is the only channel that still has the stream, and the file is `ok` with exit 0. A term of 6 letters or more still matches, because `compact` does not need a word boundary. A short name that is one whole string still matches, because the per-string view keeps it.

**Reproduction:** `max(Term("Zuleo").count(t) for t in raw_texts(data))` is 0 for each of these, and the joined text is `ZuleoLaudo`:

- `BT [(Z) -2 (u) -2 (l) -2 (e) -2 (o) -300 (L) -2 (a) -2 (u) -2 (d) -2 (o)] TJ ET`
- `BT [(Z) -2 (u) -2 (l) -2 (e) -2 (o)] TJ 0 -20 Td [(L) -2 (a) -2 (u) -2 (d) -2 (o)] TJ ET`
- `BT (Zu) Tj (leo) Tj (Laudo) Tj ET`

The existing isolated array counts 1. So does `(Zu) Tj (leo) Tj ( Laudo) Tj`, because the space is a character in a string, and so does a second word placed in a later `BT`/`ET` block. `Term("Laudo")` on the first stream is 0 as well. `check_file` with that stream as the only text, OCR off, stores `raw=0` and status `ok`.

**Validation:** The first stream must report `raw=1` for `Zuleo` and 0 for `Zulea`, including when it is a zlib stream before `endstream`. The isolated array must stay 1. A mutant that concatenates every glued piece with no word break fails it.

## G0-23

**Severity:** medium

**File:line:** `src/anonimizador/channels.py:317`

**Description:** Patterns are searched on `Reading.strings`, which is `PdfStringViews.lines`: one decoded string per line. The joined view, where those glyphs form `123.456.789-09`, is searched for terms and is not searched for patterns. The CPF pattern needs three digits in a row. A newline between glyph strings breaks a group, so `\.?\s?` never sees the dot that sits on the next line.

**Impact:** An unlisted CPF, phone, or similar number drawn glyph by glyph in an old content stream produces no hint. `--strict-patterns` still exits 0. A listed CPF still counts, because digit runs join across a newline. A CPF kept inside one string still hints. This is the same leftover stream as G0-22, for the identifier that was never in the terms file.

**Reproduction:** For `data = b"BT [(123)(.)(456)(.)(789)(-)(09)] TJ ET"`, `count_patterns` on `pdf_string_views([data]).lines` is `{}` and on `.glued` is `{"cpf": 1}`. The same split appears for per-digit strings, for `(123) Tj (.) Tj (456) Tj (.) Tj (789) Tj (-) Tj (09) Tj`, and for `[(12) -2 (3.456.789-09)] TJ`. `[(123.456.) -2 (789-09)] TJ` still hints, because each piece keeps a full group.

**Validation:** The first stream must hint `cpf=1`, and a stream whose last two digits differ must hint nothing. The clean-page test must stay free of pattern hits. A mutant that searches only the per-string lines fails it. Joining today's glued text without restoring word spaces will paste neighboring numbers together, so the word break from G0-22 has to be in the text that patterns search.

## G0-24

**Severity:** low

**File:line:** `src/anonimizador/channels.py:193`

**Description:** The glue whitelist is `Tj`, `TJ`, `Td`, `TD`, `T*`, and `'`. The other text-showing operator, `"`, and the positioning operator `Tm`, are absent, and so are text-state operators such as `Tc`. Any of them between two pieces inserts a word break. The pieces stay separate in the per-string view too.

**Impact:** A 4- or 5-letter word split the way the G0-19 test splits it, but positioned with `Tm` or shown with `"`, counts 0. A text object that contains only that word is `ok` with exit 0. The same bytes with `Td` count 1, which is what `test_a_word_drawn_glyph_by_glyph_is_whole_in_raw_bytes` locks in.

**Reproduction:** `Term("Zuleo")` over `raw_texts` is 0 for:

- `BT 1 0 0 1 72 700 Tm (Zu) Tj 1 0 0 1 90 700 Tm (leo) Tj ET`
- `BT 0 0 (Zu) " 0 0 (leo) " ET`
- `BT (Zu) Tj 0.1 Tc (leo) Tj ET`

`BT (Zu) Tj 4 0 Td (leo) Tj ET` is 1.

**Validation:** The `Tm` stream and the `"` stream must count 1 for `Zuleo` and 0 for `Zulea`. The existing `Td` assertion must stay 1. A mutant that keeps the current whitelist fails it.
