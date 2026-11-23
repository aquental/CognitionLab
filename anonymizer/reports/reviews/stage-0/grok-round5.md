I'll confirm G0-14 through G0-18 against the current verifier, then review the whole stage for any new way a hit or a failed read can be reported as clean.The review stays read-only. I'll read the verifier, matching, and the tests that claim to close G0-14 through G0-18.G4-5, G4-6, and G0-14 through G0-18 hold. Three new defects are below.

**G4-5 still holds.** `channels.run` raises `ToolError` for a non-zero exit, a timeout, or a tool that cannot start. The timeout is raised `from None`. Each channel records its own error, the file status is `ERROR`, and any unread channel makes the process exit 2. Missing OCR language data is refused before any file is read. An `OSError` is reported with `strerror` only.

**G4-6 still holds.** A file under a directory argument is named with `os.path.relpath` to that directory, and that string is the `name` channel.

**G0-14 holds** for the forms the fix names. `pdf_strings` decodes a UTF-16 hex string (spaces and newlines included), an octal escape, an escaped UTF-16 literal, and the same hex string inside a zlib stream. A changed final letter stays 0. The end-to-end test reports `raw=1` and exit 1 for an unreferenced hex author.

**G0-15 holds.** Hidden directories and hidden corpus files are walked. Only names starting with `._` are left out. A hidden dangling link still aborts the run.

**G0-16 holds.** `name` is a pattern channel, and `check_file` fills it with the relative name and the aliases before `count_patterns`.

**G0-17 holds.** `worth_ocr` keeps an image when either side is at least 32 pixels. A 400×20 strip is read. A 31×31 icon is not.

**G0-18 holds** for the case that was fixed. OCR pages and embedded images are joined before the pattern search, so one CPF on each page is `cpf=2` and `123.456.789-` plus `09` is `cpf=1`. The README channel table now says raw skips terms shorter than 4 letters.

## G0-19

**Severity:** medium

**File:line:** `src/anonimizador/channels.py:222`

**Description:** `pdf_strings` decodes every PDF string and joins them with a newline. A term of 4 or 5 letters is then matched only as a whole word (`terms.py:110`, `COMPACT_MIN` is 6). Content streams usually draw one word as several strings: `(Z)(u)(l)(e)(o)` or `[(Z) -2 (u) -2 (l) -2 (e) -2 (o)] TJ`, including one-character hex strings. Each fragment becomes its own word, so the count is 0 in every raw view. A term of 6 letters or more is compacted across those newlines and still matches. A newline inside one string (`Zu\nleo`) fails the same way.

**Impact:** A 4- or 5-letter term that survives only in an old content stream is `ok` and the process exits 0. `pdftotext` and `pdfinfo` do not read an unreferenced object, and the rendered page does not show it, so OCR does not see it either. The same bytes with the whole word in one string `(Zuleo)` are `raw=1`. This is the leftover an incremental update is supposed to catch, for the short names G0-10 put back into the raw search.

**Reproduction:** `max(Term("Zuleo").count(t) for t in raw_texts(b"(Z)(u)(l)(e)(o)"))` is 0. The same for `b"[(Z) -2 (u) -2 (l) -2 (e) -2 (o)] TJ"` and for that TJ operator inside a zlib stream between `stream\n` and `endstream`. `Term("Zulea")` is 0. `Term("Rosa")` on `(R)(o)(s)(a)` in a zlib stream is 0. `Term("Fulano")` on `(F)(u)(l)(a)(n)(o)` is 1. `check_file` with only those raw views, OCR off, returns status `ok` and no hits.

**Validation:** A one-page PDF whose live page text is empty and whose unreferenced content stream is the kerned 5-letter string must report `raw=1` and exit 1. A changed final letter stays 0. The whole-string form and a 6-letter per-glyph name stay `raw=1`. A mutant that still joins every string with a newline and word-matches terms under 6 letters fails it.

## G0-20

**Severity:** medium

**File:line:** `src/anonimizador/channels.py:179`

**Description:** The literal-string expression stops at the first parenthesis. PDF allows a balanced parenthesis with no backslash, so `(Concei\347\343o (mae))` is one string. The expression matches none of it at the outer `(`, then matches only the inner `mae`. The octal escapes never reach `unescape_literal`. The Latin-1 view still has the backslash and the digits, so the accented letters are not in any view. An escaped parenthesis (`\(mae\)`) is decoded. A hex string is unaffected. An ASCII name before the parenthesis is still found in the raw bytes; the miss is a letter that exists only as an escape.

**Impact:** A non-ASCII name written with octal escapes, or as an escaped UTF-16 literal, and followed by an unescaped parenthesis, has count 0 in every channel when `pdfinfo` and `pdftotext` do not see that object. The file is `ok` and the process exits 0. The length does not matter: compact matching never sees the decoded letters. The G0-14 forms without a parenthesis still count 1.

**Reproduction:** `max(Term("Conceição").count(t) for t in raw_texts(b"<< /Author (Concei\\347\\343o (mae)) >>"))` is 0, and the only captured literal is `mae`. The same bytes without the parenthesis count 1. `Term("Fulano").count` on `(Ful\343no (Dr))` is 0, and on `(Fulano (Dr))` is 1. `Term("Zuleo")` on `(\376\377\000Z\000u\000l\000e\000o (Dr))` is 0.

**Validation:** Those three byte strings must count 1 for the listed term and 0 for a changed letter. The existing hex, octal, and escaped-UTF-16 tests stay 1. A mutant that keeps the current expression fails the parenthetical cases.

## G0-21

**Severity:** medium

**File:line:** `src/anonimizador/channels.py:27`

**Description:** Decoded PDF strings are appended to the `raw` channel only. `PATTERN_CHANNELS` is `name`, `text`, `ocr`, `images`, and `metadata`. `count_patterns` never sees the decoded strings. `pdfinfo` and `pdftotext` read only objects the current page tree still references, so a CPF left in an old object is in none of the pattern channels. Searching every raw view would be noisy: the synthetic one-page PDF with no phone already produces `phone=7` from xref digits and other byte runs. The decoded strings of that same file produce no pattern at all.

**Impact:** An unlisted CPF stored as a literal or as UTF-16 hex in an unreferenced object gives no hint. `--strict-patterns` still prints `ok` and exits 0. A listed CPF term is still found in `raw`. The hint exists to show identifiers that were not listed. On the live page, `pdftotext` still shows the pattern.

**Reproduction:** `count_patterns(raw_texts(b"9 0 obj << /Subject (123.456.789-09) >> endobj\n"))` is `{"cpf": 1}`. `check_file` with those raw views, no terms, and `strict_patterns` returns `hints == {}`, status `ok`, and `failed` false. The same for `<FEFF003100320033002E...>` of that CPF: `pdf_strings` yields the digits, and the pattern channels stay empty. `count_patterns([pdf_strings(clean_pdf)])` on the fixture builder's page with no CPF is `{}`.

**Validation:** An unreferenced literal CPF and an unreferenced UTF-16 hex CPF must report `cpf=1` and exit 1 under `--strict-patterns`, with no digits printed. The clean one-page fixture must not gain a phone or CPF hint. A mutant that leaves `raw` out of the pattern channels fails the unreferenced cases.
