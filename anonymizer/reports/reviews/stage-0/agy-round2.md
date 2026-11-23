# Antigravity Adversarial Review — Stage 0 (Round 2)

## Confirmation of Prior Fixes

- **Carried over (`G4-5`, `G4-6` verifier part):** Confirmed. `channels.run` converts non-zero exits, timeouts, startup `OSError`s, and Poppler `stderr` errors into `ToolError` without leaking arguments or paths; unread channels mark the file `ERROR` and exit `2`. Relative paths under walked directories (including symlink aliases) are searched in `name`.
- **Grok Round 15 (`G0-46`, `G0-47`):** Partly confirmed. Pairing `<<`/`>>` via `closed_dictionaries` and pairing deeper nested parentheses via `literal_ends` / `nested_literal` resolve the specific cases tested, but two defects remain: `outer_text` corrupts UTF-16 strings in deeper nested literals (**A0-3**), and `PDF_STRUCTURE` / `inline_image_end` mishandle `ID\r\n` and `BI` bytes inside inline-image samples (**A0-4**).
- **Codex Round 2 (`C0-1`..`C0-17`) and Implementer (`I3-1`..`I3-4`, `IA1-1`):**
  - `C0-1`, `C0-2`, `C0-5`, `C0-6`, `C0-7`, `C0-8`, `C0-9`, `C0-10`, `C0-11`, `C0-12`, `C0-17`, `I3-2`, `I3-4`: Confirmed.
  - `IA1-1` (Antigravity Round 1 incomplete note): `IA1-1` fixed the truncated-at-EOF stream case, which was valid, but it was **not** the primary flaw in how [`decoded_streams`](file://<review-worktree>/anonimizador/src/anonimizador/pdfstreams.py#L373-L407) and [`Document.stream_dictionary`](file://<review-worktree>/anonimizador/src/anonimizador/pdfstreams.py#L151-L164) locate streams and advance `pos`. That primary flaw is reported below as **A0-1** (along with related stream-decoding flaws in **A0-2** and **A0-5**).

---

## Findings

### A0-1
- **Severity:** High
- **File:line:** [`src/anonimizador/pdfstreams.py:151-163`](file://<review-worktree>/anonimizador/src/anonimizador/pdfstreams.py#L151-L163), [`src/anonimizador/pdfstreams.py:383-391`](file://<review-worktree>/anonimizador/src/anonimizador/pdfstreams.py#L383-L391)
- **Description:**
  `STREAM_START` (`rb"(?<![A-Za-z])stream(?:\r\n|\r|\n)"`) is searched across raw file bytes, so it matches `stream\n` inside a comment (`% stream\n`), inside a literal string (`(a\nstream\nb)`), or in arbitrary non-stream data. Two compounding bugs then cause [`decoded_streams`](file://<review-worktree>/anonimizador/src/anonimizador/pdfstreams.py#L373-L407) to jump over subsequent real stream objects without decoding them and without incrementing `unread`:
  1. In [`Document.stream_dictionary`](file://<review-worktree>/anonimizador/src/anonimizador/pdfstreams.py#L160-L162), `between = self.data[end:keyword]` does not check `end <= keyword`. When `stream\n` occurs *inside* a dictionary (e.g., `1 0 obj << /Title (a\nstream\nb) >> endobj`), `parse_object` returns `end > keyword`. In Python, slicing `data[end:keyword]` with `end > keyword` returns `b""`, so `not between.strip(SPACE)` is `True` and `stream_dictionary` falsely returns the enclosing dictionary `value` as the stream's dictionary.
  2. In [`decoded_streams`](file://<review-worktree>/anonimizador/src/anonimizador/pdfstreams.py#L386-L390), `end = stream_end(document, entries, start)` calls `document.data.find(b"endstream", start)`, which finds the `endstream` of the **next real stream** in the file, and line 390 sets `pos = end`. `flate_decode(body)` then fails on the multi-object slice starting inside the comment or string, `unread` remains `0` (because `cut` is `False` and `data[:keyword.start()]` does not end with `>>` or `entries` is not `None`), and the next loop iteration resumes at `pos = end` — completely skipping every real compressed stream between the false `stream\n` and that `endstream`.
- **Impact:**
  Any compressed stream (Flate, ASCII85, LZW, etc.) that follows a comment `% stream\n` or a string containing `\nstream\n` is silently skipped: an identifier in that stream counts `0` in `raw` and `unread` is `0`, producing a false `ok` (exit `0`).
- **Reproduction scenario:**
  ```python
  data_comment = (
      b"%PDF-1.4\n% stream\n"
      b"1 0 obj << /Filter /FlateDecode >> stream\n"
      + zlib.compress(b"BT /F1 12 Tf 72 700 Td (Zuleo) Tj ET")
      + b"\nendstream endobj\n"
  )
  data_string = (
      b"%PDF-1.4\n"
      b"1 0 obj << /Title (line1\nstream\nline2) >> endobj\n"
      b"2 0 obj << /Filter /FlateDecode >> stream\n"
      + zlib.compress(b"BT /F1 12 Tf 72 700 Td (Zuleo) Tj ET")
      + b"\nendstream endobj\n"
  )
  ```
  - **Expected:** `raw_count("Zuleo", data_comment) == 1` and `raw_count("Zuleo", data_string) == 1`.
  - **Actual:** `raw_count("Zuleo", data_comment) == 0` (with `raw_readings(data_comment).unread == 0`) and `raw_count("Zuleo", data_string) == 0` (with `raw_readings(data_string).unread == 0`).
- **Validation suggestion:**
  Require `end <= keyword` in `Document.stream_dictionary` (and ignore comments in `between`), and when `entries is None` and `flate_decode(body)` fails, advance `pos` only to `start` (not `end`) so a non-stream `stream` keyword never consumes subsequent stream objects. Add unit tests for both `data_comment` and `data_string`.

---

### A0-2
- **Severity:** High
- **File:line:** [`src/anonimizador/pdfstreams.py:160-162`](file://<review-worktree>/anonimizador/src/anonimizador/pdfstreams.py#L160-L162), [`src/anonimizador/pdfstreams.py:392-400`](file://<review-worktree>/anonimizador/src/anonimizador/pdfstreams.py#L392-L400)
- **Description:**
  1. When a stream has a preceding dictionary (`>>` before `stream`) that [`Document.stream_dictionary`](file://<review-worktree>/anonimizador/src/anonimizador/pdfstreams.py#L151-L163) cannot parse (`entries is None`, e.g., nested deeper than `MAX_DEPTH = 256`, more than `HEADER_TRIES = 16` object-header-like strings in the dictionary, or an object number > 10 digits), lines 392–400 try a single raw `flate_decode(body)` and only increment `unread` inside `except Unread:`. If the stream's outer encoding is `FlateDecode`, `flate_decode(body)` succeeds without raising `Unread`, so `unread` remains `0` even though the unparsed dictionary also declared `/DecodeParms` (such as `/Predictor 15` or `/Predictor 2`) or additional filters in a chain (such as `/Filter [/FlateDecode /FlateDecode]`). The stream bytes remain scrambled/compressed after the first inflate pass, and no unread error is reported.
  2. In [`Document.stream_dictionary`](file://<review-worktree>/anonimizador/src/anonimizador/pdfstreams.py#L160-L161) and line 398, whitespace between `>>` and `stream` is checked with `.strip(SPACE)` / `.rstrip(SPACE)`. If a PDF comment appears between `>>` and `stream` (e.g., `9 0 obj << /Filter [/A85 /Fl] >> % comment\nstream\n`), `stream_dictionary` returns `None` and `data[:keyword.start()].rstrip(SPACE).endswith(b">>")` is `False`, so the filter chain is skipped and `unread` is `0`.
- **Impact:**
  A stream with `/DecodeParms` (PNG/TIFF predictor) or stacked filters whose dictionary fails to parse, or any filtered stream with a comment between `>>` and `stream`, is neither decoded nor reported as unread (`unread == 0`, count `0`, exit `0`).
- **Reproduction scenario:**
  ```python
  content = b"%" + b" " * 158 + b"\nBT (Zu) Tj (leo) Tj ET\n"
  padded = content.ljust(200, b" ")
  deep = b"<< /Y " * 300 + b"1" + b" >>" * 300
  data_pred = (
      b"9 0 obj << /Filter /FlateDecode /DecodeParms << /Predictor 15 /Columns 40 >> /X "
      + deep
      + b" >> stream\n"
      + zlib.compress(png_predict(padded, 40))
      + b"\nendstream endobj\n"
  )
  a85 = base64.a85encode(zlib.compress(b"BT (Zu) Tj (leo) Tj ET")) + b"~>"
  data_comment = b"9 0 obj << /Filter [/A85 /Fl] >> % note\nstream\n" + a85 + b"\nendstream endobj\n"
  ```
  - **Expected:** `raw_readings(data_pred).unread == 1`; `raw_count("Zuleo", data_comment) == 1` and `raw_readings(data_comment).unread == 0`.
  - **Actual:** `raw_readings(data_pred).unread == 0` and `raw_count("Zuleo", data_pred) == 0`; `raw_count("Zuleo", data_comment) == 0` and `raw_readings(data_comment).unread == 0`.
- **Validation suggestion:**
  Strip PDF comments (`%[^\r\n]*`) as well as `SPACE` when checking the gap between `>>` and `stream`. When `entries is None` and the bytes before `stream` end with `>>` (after stripping whitespace and comments), always count the stream as `unread` even if a speculative `flate_decode(body)` succeeds.

---

### A0-3
- **Severity:** High
- **File:line:** [`src/anonimizador/channels.py:439-449`](file://<review-worktree>/anonimizador/src/anonimizador/channels.py#L439-L449), [`src/anonimizador/channels.py:355-374`](file://<review-worktree>/anonimizador/src/anonimizador/channels.py#L355-L374)
- **Description:**
  [`outer_text`](file://<review-worktree>/anonimizador/src/anonimizador/channels.py#L439-L449) strips inner spans from a deeper nested literal (`G0-47`) at the raw byte level and joins the remaining byte slices with a single byte `b"\n"` before calling `unescape_literal` and `decode_pdf_string`:
  ```python
  return PdfString(literal.start, literal.end, unescape_literal(b"\n".join(pieces)))
  ```
  When the literal string is UTF-16 (escaped or raw, with or without a BOM), inserting the 1-byte `b"\n"` into the raw byte stream before `decode_pdf_string` corrupts the text in three ways:
  1. **UTF-16 phase shift:** If `pieces[0]` unescapes to an even number of bytes (e.g., `pieces[0]` is the 2-byte BOM `\376\377` in `(\376\377(obs (2))\000Z\000u\000l\000e\000o)`), the 1-byte `\n` shifts all subsequent 2-byte UTF-16BE code units by 1 byte (`\n\x00`, `Z\x00`, `u\x00`, `l\x00`, `e\x00`), decoding `"Zuleo"` as `\u0a00\u5a00\u7500\u6c00\u6500`.
  2. **Displaced BOM:** If `pieces[0]` is empty (e.g., `((obs (2))\376\377\000Z\000u\000l\000e\000o)`), `b"\n".join(pieces)` prepends `\n` before `\xfe\xff` and makes the byte length odd (`13`), so `raw.startswith(b"\xfe\xff")` and `len(raw) % 2 == 0` are both `False`, and `decode_pdf_string` falls back to `latin-1`.
  3. **Odd byte length on BOM-less two-byte strings:** For `(\000Z\000u\000l\000e\000o(obs (2)))`, `pieces[0]` is 10 bytes and `pieces[1]` is 0 bytes; joining them with 1-byte `b"\n"` produces 11 bytes (odd), which fails `len(raw) % 2 == 0` in [`decode_pdf_string`](file://<review-worktree>/anonimizador/src/anonimizador/channels.py#L367) and decodes as `latin-1` with NULs between letters.
  When the string is in an object dictionary (such as `/Author`) or a marked-content dictionary (`<< ... >> BDC`), `drawn_spans` skips dictionary strings, so `outer_text` is the only reading that covers it.
- **Impact:**
  Any name stored in a UTF-16 PDF literal nested deeper than one parenthesis level inside an object or marked-content dictionary counts `0` in `raw`.
- **Reproduction scenario:**
  ```python
  for data in (
      b"9 0 obj << /Author (\\376\\377(obs (2))\\000Z\\000u\\000l\\000e\\000o) >> endobj",
      b"9 0 obj << /Author ((obs (2))\\376\\377\\000Z\\000u\\000l\\000e\\000o) >> endobj",
      b"9 0 obj << /Author (\\000Z\\000u\\000l\\000e\\000o(obs (2))) >> endobj",
  ):
      assert raw_count("Zuleo", data) == 1
  ```
  - **Expected:** `raw_count("Zuleo", data) == 1` for all three inputs.
  - **Actual:** `raw_count("Zuleo", data) == 0` for all three inputs.
- **Validation suggestion:**
  Unescape the outer literal and preserve its BOM/UTF-16 encoding per piece (or decode each unescaped piece with `decode_pdf_string`, propagating any leading BOM to subsequent pieces, and join the decoded `str` pieces with `"\n"`).

---

### A0-4
- **Severity:** High
- **File:line:** [`src/anonimizador/channels.py:264`](file://<review-worktree>/anonimizador/src/anonimizador/channels.py#L264), [`src/anonimizador/channels.py:503-521`](file://<review-worktree>/anonimizador/src/anonimizador/channels.py#L503-L521)
- **Description:**
  Two flaws in inline-image handling allow bytes inside an inline image's samples to hide drawn text after the image:
  1. **CRLF after `ID`:** In `PDF_STRUCTURE` (line 264), `(?P<inline>(?<![\w/])BI[\s\x00/].{0,4096}?(?<![\w/])ID[\s\x00])` consumes only one byte after `ID`. When `ID` is followed by a CRLF line ending (`ID\r\n`), `head.end()` points at `\n` instead of the first sample byte. For a `/FlateDecode` inline image, `zlib.decompressobj().decompress(blob[head.end():])` in [`filtered_size`](file://<review-worktree>/anonimizador/src/anonimizador/channels.py#L489-L494) fails on the leading `\n` and returns `None`; for an unfiltered inline image, `head.end() + size` is 1 byte short of `EI`. `inline_image_end` then falls back to the first ` EI ` inside the image's data, so a `%` after ` EI ` inside the image data comments out the drawn strings after the image.
  2. **`BI` inside samples when sample size is unknown:** In [`inline_image_end`](file://<review-worktree>/anonimizador/src/anonimizador/channels.py#L517-L521), when `size is None` (e.g., `/CS /Cs1`), `following = INLINE_IMAGE_START.search(blob, end.end())` looks for any `BI[\s\x00/]` after the first `EI`. If the image's own samples contain ` EI % BI ...` before the real ` EI`, `end` matches the fake ` EI` in the samples and `following` matches `BI ` in the same image's samples. Because no `EI` lies between `end` and `following`, `later` is `None` (`clear = True`, `unclear_images == 0`), and `drawn_spans` resumes inside the samples where `%` comments out the rest of the line (including the real `EI` and `BT (Zu) Tj (leo) Tj ET`).
- **Impact:**
  Short terms or split patterns drawn after the inline image are missed (`raw` count `0`), and in case 2 `unclear_images` is `0` as well, causing a false `ok` (exit `0`).
- **Reproduction scenario:**
  ```python
  data_crlf = (
      b"BI /W 8 /H 1 /CS /G /BPC 8 /F /Fl ID\r\n"
      + zlib.compress(b" EI %abc", 0)
      + b" EI BT /F1 12 Tf 72 700 Td (Zu) Tj (leo) Tj ET"
  )
  data_bi_in_samples = (
      b"BI /W 12 /H 1 /CS /Cs1 /BPC 8 ID  EI % BI \x00\x00\x00 EI BT (Zu) Tj (leo) Tj ET"
  )
  ```
  - **Expected:** `raw_count("Zuleo", data_crlf) == 1` and `raw_readings(data_crlf).unclear_images == 0`; for `data_bi_in_samples`, `raw_readings(data_bi_in_samples).unclear_images == 1`.
  - **Actual:** `raw_count("Zuleo", data_crlf) == 0`; for `data_bi_in_samples`, `raw_count("Zuleo", data_bi_in_samples) == 0` and `raw_readings(data_bi_in_samples).unclear_images == 0`.
- **Validation suggestion:**
  Match `(?:\r\n|[\s\x00])` after `ID` in `PDF_STRUCTURE` (and skip leading whitespace in `filtered_size` for `FlateDecode`/`DCTDecode`), and in `inline_image_end` when `size is None`, require `following` to be a complete `BI ... ID` header whose own end is clear (or treat any later `EI` before the end of the stream as ambiguous when `BI` occurs before it).

---

### A0-5
- **Severity:** Medium
- **File:line:** [`src/anonimizador/pdfstreams.py:134-149`](file://<review-worktree>/anonimizador/src/anonimizador/pdfstreams.py#L134-L149), [`src/anonimizador/pdfstreams.py:281`](file://<review-worktree>/anonimizador/src/anonimizador/pdfstreams.py#L281), [`src/anonimizador/pdfstreams.py:338-346`](file://<review-worktree>/anonimizador/src/anonimizador/pdfstreams.py#L338-L346)
- **Description:**
  Three defects in `pdfstreams.py` affect indirect object resolution, filter chains ending in image codecs, and predictor validation:
  1. **`Document.objects` indexes `N G obj` matches inside strings, comments, and streams (`pdfstreams.py:134-138`):** `self.objects = {(int(h[1]), int(h[2])): h.end() for h in headers}` records every `OBJECT_HEADER` match across the file, with the last match winning. If a later string, comment, or stream body contains `10 0 obj /DCTDecode` (or `10 0 obj []`), it overwrites an earlier real object `10 0 obj [/A85 /Fl] endobj` referenced by `/Filter 10 0 R` (or `/Length 11 0 R`), so `document.resolve` returns the shadowed value from inside the string/comment and skips decoding the stream with `unread == 0`.
  2. **Filter chains ending in `IMAGE_FILTERS` discard bytes decoded by preceding filters (`pdfstreams.py:338-346`):** In `decode_stream`, when a stream has `/Filter [/FlateDecode /DCTDecode]` or `/Filter [/ASCII85Decode /DCTDecode]`, the outer filter decodes into `body`, and then `if filter_name in IMAGE_FILTERS: return None` discards `body`. Any EXIF/XMP/COM metadata or text inside a Flate- or ASCII85-wrapped JPEG stream (which `pdfinfo`, `pdftotext`, and `pdfimages -png` never read) is discarded instead of being returned for `raw`. Moreover, line 341 (`if entries.get(b"Subtype") == b"Image": parameters = [None] * len(filters)`) wipes `/DecodeParms << /EarlyChange 0 >>` on LZW image streams, causing `lzw_decode` to fail with `Unread`.
  3. **Unknown predictors above 15 are not rejected (`pdfstreams.py:281`):** `unpredict` raises `Unread(data)` for `predictor < 10` (other than 1 and 2), but does not reject `predictor > 15`, treating e.g. `/Predictor 20` as a PNG predictor instead of raising `Unread(data)`.
- **Impact:**
  (1) An indirect `/Filter` or `/Length` shadowed by a later string or comment causes a compressed stream to be skipped with `unread == 0`. (2) Identifiers in EXIF/COM metadata of Flate/ASCII85-wrapped JPEG streams in PDFs are missed in all channels, and valid `/Subtype /Image` LZW streams with `/EarlyChange 0` fail as unread. (3) Streams with invalid `/Predictor > 15` are not flagged as unread when row first bytes happen to be `0..4`.
- **Reproduction scenario:**
  ```python
  # 1. Indirect /Filter shadowed by a string in a later object:
  a85 = base64.a85encode(zlib.compress(b"BT (Zu) Tj (leo) Tj ET")) + b"~>"
  data_shadow = (
      b"10 0 obj [/A85 /Fl] endobj\n"
      b"9 0 obj << /Filter 10 0 R >> stream\n" + a85 + b"\nendstream endobj\n"
      b"11 0 obj << /Note (10 0 obj /DCTDecode) >> endobj\n"
  )
  # 2. Flate-wrapped DCTDecode stream containing EXIF metadata:
  jpeg = JPEG_START + b"\xff\xe1\x00\x10Exif\x00\x00Zuleo" + JPEG_SCAN
  data_flate_dct = (
      b"4 0 obj << /Subtype /Image /Filter [/FlateDecode /DCTDecode] >> stream\n"
      + zlib.compress(jpeg)
      + b"\nendstream endobj\n"
  )
  # 3. Predictor > 15 with 0x00 leading byte:
  data_pred20 = (
      b"9 0 obj << /Filter /Fl /DecodeParms << /Predictor 20 /Columns 4 >> >> stream\n"
      + zlib.compress(b"\x00abcd")
      + b"\nendstream endobj\n"
  )
  ```
  - **Expected:** `raw_count("Zuleo", data_shadow) == 1`; `raw_count("Zuleo", data_flate_dct) == 1`; `raw_readings(data_pred20).unread == 1`.
  - **Actual:** `raw_count("Zuleo", data_shadow) == 0` (`unread == 0`); `raw_count("Zuleo", data_flate_dct) == 0` (`unread == 0`); `raw_readings(data_pred20).unread == 0`.
- **Validation suggestion:**
  (1) In `Document`, only index object definitions that are followed by `endobj` after the parsed object (and resolve all definitions matching `(number, generation)` or ignore matches inside strings/comments/streams). (2) In `decode_stream`, when `filter_name in IMAGE_FILTERS`, return `body` if a preceding filter in the chain already decoded it (and preserve `EarlyChange` in `parameters` for `/Subtype /Image`). (3) In `unpredict`, check `if not 10 <= predictor <= 15: raise Unread(data)`.

---

### A0-6
- **Severity:** Low
- **File:line:** [`src/anonimizador/verify.py:191-192`](file://<review-worktree>/anonimizador/src/anonimizador/verify.py#L191-L192)
- **Description:**
  [`scan`](file://<review-worktree>/anonimizador/src/anonimizador/verify.py#L142) filters directory entries by `not name.startswith(APPLEDOUBLE) and os.path.splitext(name)[1].lower() in SUFFIXES`, so running `anonimizador verify dir/` when `dir/` contains only `notes.txt` raises `VerifyError("no PDF, PNG, or JPEG file to check")` before printing a report header. However, when a file path is passed directly to [`collect`](file://<review-worktree>/anonimizador/src/anonimizador/verify.py#L191-L192) (`elif os.path.isfile(path):`), `collect` does not check `SUFFIXES` (or `APPLEDOUBLE`). A non-corpus file passed directly (e.g., `anonimizador verify notes.txt` or via shell glob `anonimizador verify dir/*`) is accepted into `files`, routed to `read_image` in [`read`](file://<review-worktree>/anonimizador/src/anonimizador/verify.py#L209-L220), and reported as `[1] ERROR / unreadable: file: not a whole PNG or JPEG image` (and passed to `tesseract` when OCR is enabled).
- **Impact:**
  Passing a non-corpus file directly (or expanding a directory glob `dir/*` containing a `.txt`, `.json`, or `._*` AppleDouble file) runs `read_image` and `tesseract` on the non-image file and reports a per-file image error instead of rejecting or skipping non-corpus files in `collect`.
- **Reproduction scenario:**
  ```python
  notes = outside_repo / "notes.txt"
  notes.write_text("x", encoding="utf-8")
  collect([str(notes)])
  ```
  - **Expected:** Raises `VerifyError` (no PDF, PNG, or JPEG file to check), consistent with `collect([str(outside_repo)])` in `test_cli.py:81-82`.
  - **Actual:** Returns `[CorpusFile(path='.../notes.txt', name='notes.txt', aliases=())]`.
- **Validation suggestion:**
  Check `os.path.splitext(path)[1].lower() in SUFFIXES` (and AppleDouble prefix) in the `elif os.path.isfile(path):` branch of `collect`, and add a test in `tests/test_verify.py` / `tests/test_cli.py`.
