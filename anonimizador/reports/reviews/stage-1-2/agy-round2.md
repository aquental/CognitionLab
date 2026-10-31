**ID:** AB-8
**Severity:** high
**File:line:** `src/anonimizador/detect.py:211-212`, `src/anonimizador/detect.py:217-223`
**Description:** `ends_field` stops a field value before any word followed by a word starting with `:`, even when that word and colon belong to a compound label currently being matched. For multi-word labels with a detached colon (`Nome do paciente :`, `Data de nascimento :`), `COLON_LABEL` matches across two or more words and `last` is the word holding `:`. However, the earlier label words (`Nome`, `Data`) also match the single-word bare labels (`"nome"`, `"data"`, etc., or when a multi-word colon label follows a bare label or when `COLON_LABEL` overlaps or when `colon_value` / `run_from` scans across a multi-word label with a detached colon). Specifically:
1. When a value precedes a multi-word field label with a detached colon (e.g., `Paciente: Fulano Ficticio Data de nascimento : 01/02/1980` or `Paciente: Fulano Ficticio Nome da mae : Maria`), `ends_field` only checks if `seq[k]` contains `:` or `seq[k+1]` starts with `:`. Thus `Data` and `de` (or `Nome` and `da`) are NOT recognized as starting the next field because the `:` is on `seq[k+3]`, so `Data de` is swallowed into the preceding value, and when `ends_field` finally stops at `nascimento` (because `:` is after it), in `bare_value`'s `beside(words, last, gap)`, if a heading row is `Nome   Data de nascimento : 01/02/1980` (with gap <= 2x height), `beside` takes `["Data", "de"]` before stopping at `nascimento`, accepts it (`not_particles(["Data", "de"])` is `True`), and fails to look at `column_below` for the actual name under `Nome`!
2. Even more critically, in `label_values` (`detect.py:358`):
   `if (m := BARE_LABEL.match(text.folded)) and not COLON_LABEL.match(text.folded):`
   When a line starts with `Nome : Fulano` (a single-word label with a detached colon) or `Paciente : Fulano`: in `join`, words on the same line are joined by `" "`, so `text.folded` is `"nome : fulano"`. `COLON_LABEL` has `[\s.]*:` after the label words, so `COLON_LABEL.match("nome : fulano")` matches `"nome :"`. What if the line starts with `Data Nasc : 01/02/1980`? `"data nasc"` is in `BARE_LABELS` (`detect.py:310`), NOT in `COLON_LABELS` (`detect.py:57-60` has `"data de nascimento"`, `"nascimento"`, `"dt nasc"`, `"d n"`, but NOT `"data nasc"`!).
   Because `"data nasc"` is in `BARE_LABELS` and NOT in `COLON_LABELS`:
   On `Data Nasc : 01/02/1980` (or `Data Nasc: 01/02/1980` with a glued colon!):
   - `COLON_LABEL.match(text.folded)` is `None` (since `"data nasc"` is not in `COLON_LABELS`).
   - `BARE_LABEL.match(text.folded)` matches `"data nasc"` (word indices `{0, 1}`).
   - `bare_value` calls `beside(words, last=1, gap)`.
   - If the colon was glued (`Data Nasc: 01/02/1980`), `words[1].text` is `"Nasc:"`. `text.words(m.start(), m.end())` for `BARE_LABEL` (`r"data[\W_]+nasc(?![^\W_])"`) matches `"data nasc"` inside `"data nasc: 01/02/1980"`, so `last = 1` (`"Nasc:"`), and `beside` takes `"01/02/1980"`.
   - **Wait**: what if the colon is detached (`Data Nasc : 01/02/1980`), or what if `"Data Nasc:"` is at the end of a line (`Data Nasc:\n01/02/1980`) or in the middle of a line (`Paciente: Fulano   Data Nasc: 01/02/1980`)?
   Let's check both!
   Case A: `Data Nasc : 01/02/1980` (detached colon at start of line):
   `BARE_LABEL` matches `"data nasc"` (`first=0, last=1`). `bare_value` calls `beside(words, 1, gap)`. The first word in `row_after(words, 1)` is `":"` (`words[2]`). `run_from` checks `ends_field(words, row, 0)` on `":"` -> `":" in words[row[0]].text` is `True`! So `run_from` immediately breaks and returns `[]`! Then `column_below` returns `[]`, and `beside(..., inf)` also breaks on `":"` and returns `[]`! And `COLON_LABEL` does not have `"data nasc"`, so `COLON_LABEL.finditer` does not match it either! Under `--keep-dates`, the date of birth `01/02/1980` after `Data Nasc :` is **completely missed and survives unredacted**!
   Case B: `Data Nasc: 01/02/1980` (with colon) anywhere other than the start of a line (e.g. `Sexo: F   Data Nasc: 01/02/1980` or `Idade: 45   Data Nasc:\n01/02/1980`):
   Because `"data nasc"` is in `BARE_LABELS` (which only matches at the start of a line via `BARE_LABEL.match`) and is missing from `COLON_LABELS`, `Data Nasc:` anywhere after the start of a line is neither matched by `BARE_LABEL.match` nor by `COLON_LABEL.finditer`! Under `--keep-dates`, the date of birth after `Data Nasc:` survives completely unredacted!
   Case C: `Data Nasc:` at the end of a line with the date on the next line (even when not stacked directly at x=0 under `Data`, or when `column_below` gap > 2x height): for `COLON_LABELS`, `Data Nasc:` is not in `COLON_LABELS`, and for `BARE_LABELS`, `has_digit` requires digits, which works if `column_below` finds it, unless it had a detached colon `Data Nasc :` at the end of the line (where `last` is `Nasc`, and `":"` is on the same row to its right, though `has_digit([":"])` is False so `bare_value` falls through to `column_below`).
**Impact:** Under `--keep-dates`, a patient's date of birth following `Data Nasc:` (or `Data nasc.:`) anywhere after the first field on a line, or following `Data Nasc :` (with a space before the colon) even at the start of a line, is never detected and survives in the anonymized copy.
**Reproduction scenario or verifiable rationale:**
1. `found(line("Sexo:", "F", "Data", "Nasc.:", "01/02/1980"), remove_dates=False)` returns `{"label": {"F"}}` — `"01/02/1980"` is completely missed because `"data nasc"` is in `BARE_LABELS` (`detect.py:310`) but omitted from `COLON_LABELS` (`detect.py:57-60`).
2. `found(line("Data", "Nasc", ":", "01/02/1980"), remove_dates=False)` returns `{}` — `BARE_LABEL` matches `"data nasc"` (words 0 and 1), `beside` stops immediately at word 2 (`":"`) because `ends_field` sees `":"`, and `COLON_LABEL` does not contain `"data nasc"`.
3. Similarly, any bare label (`Nome`, `Paciente`, `Mae`, `Data de nascimento`, `Dt nasc`, `Nascimento`) written with a detached colon that has punctuation before it or when `BARE_LABEL` matches without `COLON_LABEL` matching (e.g., `Nome — : Fulano` or `Data Nasc : 01/02/1980`) fails to take its value.
**Validation suggestion:** Add `"data nasc"` to `COLON_LABELS` in `src/anonimizador/detect.py`, and add unit tests in `tests/test_detect.py` for `Data Nasc.: 01/02/1980` as a second field on a line and `Data Nasc : 01/02/1980` with a detached colon under `remove_dates=False`.

**ID:** AB-9
**Severity:** high
**File:line:** `src/anonimizador/detect.py:314-321`, `src/anonimizador/detect.py:358-360`
**Description:** `BARE_LABEL` matches a bare label only at position 0 of `text.folded` (`BARE_LABEL.match(text.folded)`). If the first word of the line has leading punctuation or a bullet/border character (e.g. `| Nome`, `- Nome`, `(Nome)`, or OCR noise like `.Nome` at the start of a table cell or box border), `text.folded` starts with that punctuation character (e.g. `"| nome ..."`). Unlike `COLON_LABEL` and `NUMBER_LABEL` (which use `finditer` with `LABEL_PREFIX`), `BARE_LABEL` uses `.match()` with no leading `[\W_]*`, so any leading non-word character on the first word of the line prevents `BARE_LABEL` from matching at all.
Furthermore, in `detect.py:358`:
`if (m := BARE_LABEL.match(text.folded)) and not COLON_LABEL.match(text.folded):`
If a line starts with a bare label (e.g., `Nome Fulano Ficticio Idade: 45` or `Paciente Sicrano Inventado Sexo: M`), `COLON_LABEL.match(text.folded)` only checks at index 0, which is `None` (unless the first label itself is a `COLON_LABEL`). However, what if a bare label is followed by a colon label *whose label words start at index 0*? For example, `Nome: Fulano` — `COLON_LABEL.match` is truthy so `BARE_LABEL` is skipped. What if the line is `Nome social: Fulano`? Both match at index 0, `COLON_LABEL.match` is truthy. What if a table header or multi-column line has a bare label as the *second* column on the same extracted line, e.g., `Data 05/06/2021   Nome Fulano Ficticio` or `Prontuario: 123   Nome Fulano Ficticio`? Or what if OCR extracts the vertical cell border `"|"` as the first word or character of the line (`"| Nome"`, `"|"` followed by `"Nome"`)? Because OCR (`tesseract`) frequently outputs table border characters (`|`, `[`, `!`, `—`) as the first token on a line in boxed medical forms, `text.folded` begins with `"| nome"`, causing `BARE_LABEL.match(text.folded)` to return `None` and the patient's name to survive!
**Impact:** On forms with boxed cells or table borders where OCR reads a leading border/punctuation character (`"| Nome ..."`, `"[Paciente ..."`), `BARE_LABEL.match(text.folded)` fails because `text.folded` starts with `"|"` or `"["` instead of a letter, leaving the patient's or mother's name or date of birth unredacted.
**Reproduction scenario or verifiable rationale:**
In `src/anonimizador/detect.py:314`, `BARE_LABEL` has no `^[\W_]*` prefix, and `text.words(m.start(), m.end())` would include a preceding punctuation word `|` if `^[\W_]*` matched across it unless restricted. Specifically:
`found(line("|Nome", "Sicrano", "Inventado"))` or `found(line("|", "Nome", "Sicrano", "Inventado"))`:
`text.folded` is `"|nome sicrano inventado"` or `"| nome sicrano inventado"`. `BARE_LABEL.match(text.folded)` returns `None`, so `found` returns `{}` and `"Sicrano Inventado"` survives.
**Validation suggestion:** Allow leading non-alphanumeric characters before a bare label at the start of a line (e.g. `r"^[\W_]*(?P<label>...)"` and use `text.words(m.start("label"), m.end("label"))`, and check `not COLON_LABEL.match(text.folded, m.start("label"))`), and test `line("|", "Nome", "Sicrano", "Inventado")` and `line("[Nome", "Sicrano")`.

**ID:** AB-10
**Severity:** medium
**File:line:** `src/anonimizador/locate.py:170-175`, `src/anonimizador/redact.py:56-60`
**Description:** In `read_pdf_pages` (`locate.py:170-175`), `Image.open(image)` only catches `Image.DecompressionBombError`, unlike `read_image_page` (`locate.py:191-198`) which catches `IMAGE_ERRORS`. More importantly, when `Image.MAX_IMAGE_PIXELS` is exceeded by a large image (between `MAX_IMAGE_PIXELS` and `2 * MAX_IMAGE_PIXELS`), Pillow issues a `DecompressionBombWarning` (which is a `UserWarning` printed to `stderr`, potentially including the image file path in the warning header `...: DecompressionBombWarning: Image size (... pixels) exceeds limit...`), and only raises `DecompressionBombError` above `2 * MAX_IMAGE_PIXELS`. Even if `read_image_page` opens an original image whose path contains the patient's name (`exams / "Relatorio Fulano.png"`), if a warning is emitted or if `read_image_page` hits `DecompressionBombError`, wait: in `read_image_page`, `Image.DecompressionBombError` is in `IMAGE_ERRORS` and raises `ToolError("the image could not be decoded")` (whereas `read_pdf_pages` raises `ToolError("a rendered page has too many pixels; lower --dpi")`), and in `redact.py:56`, `with Image.open(page.image) as im:` is called again without catching `IMAGE_ERRORS`. Worse: if Pillow emits a warning (such as `DecompressionBombWarning` or `UserWarning` on corrupt EXIF/PNG metadata when opening `path` in `read_image_page(path, ...)`), Python's `warnings` module prints the warning to `sys.stderr`. While `warnings.showwarning` prints the Python source file (`locate.py`), some Pillow warnings include metadata or chunk details in the warning message itself! For example, `PngImagePlugin` or `TiffImagePlugin` (used by `getexif()` / `exif_transpose()`) calls `warnings.warn("Possibly corrupt EXIF data.  Expecting to read ... bytes but only got ...")` or similar warnings on malformed tags.
Additionally, look at `read_image_page` (`locate.py:194`):
`shown = flattened(eight_bit(ImageOps.exif_transpose(im)))`
If `im` is a 16-bit PNG (`mode == "I;16"`) that ALSO has an EXIF Orientation tag (e.g., Orientation = 6, rotate 270 CW) and a `transparency` key in `im.info`:
`ImageOps.exif_transpose(im)` calls `im.transpose(...)`, which returns a new `Image` where `im.info` strips or does not preserve `"transparency"` on certain modes or converts `"I;16"` in older/current Pillow, OR even simpler: what if `im` is a 16-bit PNG with mode `"I;16B"` or `"I;16L"` and an EXIF orientation tag requiring rotation (`Orientation = 6`)?
In Pillow, `im.transpose(Image.Transpose.ROTATE_270)` on mode `"I;16B"` or `"I;16L"` raises `ValueError: bad image mode` (caught by `IMAGE_ERRORS`, turning a valid oriented 16-bit PNG into an error) or look at `pixel_mode(shown)` (`locate.py:233-234`):
```python
def pixel_mode(im: Image.Image) -> str:
    return "L" if im.mode in ("1", "L", "I;16", "I") else "RGB"
```
Look at what happens when `im` is a grayscale image with alpha (`mode == "LA"`, or a `"L"` / `"1"` / `"I;16"` PNG with `"transparency"` in `im.info`):
1. `eight_bit(im)` returns an image with mode `"LA"` (because `scaled` is `"L"` and `scaled.putalpha(alpha)` changes its mode from `"L"` to `"LA"`!).
2. `flattened(im)` (`locate.py:206-212`) converts `im` to `"RGBA"` and composites it over `white` (`"RGBA"`), returning an image with `shown.mode == "RGBA"`!
3. Then `pixel_mode(shown)` (`locate.py:233-234`) checks `if shown.mode in ("1", "L", "I;16", "I")` — which is `False` because `shown.mode` is `"RGBA"`! So `pixel_mode(shown)` returns `"RGB"`!
4. Then `read_image_page` saves `page-1.png` as `"RGB"` (3 channels instead of 1). That part is fine because `redact.paint` later calls `gray_if_neutral`, EXCEPT in `read_image_page`, `page-1.png` is written to `tmp` and read by `tesseract` as RGB, and notice `pixel_mode` checks `("1", "L", "I;16", "I")` even though after `flattened(eight_bit(...))`, `shown.mode` can NEVER be `"I;16"` or `"I"` (since `eight_bit` already converted all `SIXTEEN_BIT` modes to `"L"` or `"LA"`).
Wait — what if `im` has mode `"P"` (palette PNG) where the palette is grayscale, or mode `"I;16"` with `transparency`?
Wait: look closely at `eight_bit` (`locate.py:215-230`):
```python
def eight_bit(im: Image.Image) -> Image.Image:
    if im.mode not in SIXTEEN_BIT:
        return im
    samples = im.convert("I")
    scaled = samples.point(lambda v: v / 256).convert("L")
    # Bare pixels: the 16-bit transparent value means nothing to an 8-bit image.
    scaled = Image.frombytes("L", im.size, scaled.tobytes())
    key = im.info.get("transparency")
    if isinstance(key, int):
        alpha = Image.new("L", im.size)
        alpha.putdata([0 if v == key else 255 for v in samples.get_flattened_data()])
        scaled.putalpha(alpha)
    return scaled
```
Look at line 221: `samples = im.convert("I")`.
What if the PNG is a 16-bit PNG opened by Pillow with mode `"I;16"` where pixel values are > 32767 (e.g. 60000, as in `test_a_16_bit_image_keeps_its_contrast`), AND `im` is mode `"I;16B"` or `"I;16L"`?
In Pillow, `Image.open` on a 16-bit grayscale PNG produces mode `"I;16"`. When `ImageOps.exif_transpose(im)` runs BEFORE `eight_bit(im)` (`locate.py:194`):
If the PNG has an EXIF chunk with an Orientation tag (e.g. `exif[0x0112] = 6`), `ImageOps.exif_transpose(im)` copies `im.info` EXCEPT when transposing, wait: does `ImageOps.exif_transpose` preserve `im.info["transparency"]`? Yes, `transposed.info = im.info.copy()`.
Wait: what if the image is a 16-bit grayscale PNG with an alpha channel (PNG color type 4, 16-bit grayscale + 16-bit alpha)?
How does Pillow open a 16-bit grayscale+alpha PNG (color type 4, bit depth 16) or 16-bit RGBA PNG (color type 6, bit depth 16)?
Wait: `channels.py:867` explicitly supports PNG color type 4 with depth 16 (`PNG_DEPTHS = {0: (1, 2, 4, 8, 16), 2: (8, 16), 3: (1, 2, 4, 8), 4: (8, 16), 6: (8, 16)}`), so `verify` accepts 16-bit LA and 16-bit RGBA PNGs as whole PNGs!
And how does Pillow open a 16-bit color type 4 (LA) or color type 2 (RGB) or color type 6 (RGBA) PNG?
- For 16-bit RGB (color type 2, depth 16) and 16-bit RGBA (color type 6, depth 16), Pillow downconverts to 8-bit `"RGB"` / `"RGBA"` on open.
- For 16-bit Grayscale (color type 0, depth 16), Pillow opens it as `"I;16"`, which `eight_bit` handles.
- What if a 16-bit Grayscale PNG (`"I;16"`) has a `tRNS` chunk where `im.info["transparency"]` is a `tuple` or `bytes`, or what if a `"P"` or `"RGB"` image has `transparency` as `bytes`? `flattened` handles `"transparency" in im.info` via `im.convert("RGBA")`.
Wait — what if a 16-bit Grayscale PNG (`"I;16"`) does NOT have `transparency`, or has a malformed `tRNS` chunk (e.g. `tRNS` length != 2, so Pillow sets `im.info["transparency"]` or ignores it)? If `im.info["transparency"]` is NOT an `int` (for example, if someone or a plugin sets it to a tuple `(0,)`), `eight_bit` returns `scaled` without `im.info["transparency"]`, which is fine.

Let's check if there is a real bug in `detect.py`, `locate.py`, `redact.py`, `batch.py`, `terms.py`, or `cli.py` instead of speculating on `locate.py:170`. Let's drop AB-10 unless it's rock-solid, and check other things first!

Let's check `detect.py` very carefully!
Look at `detect.py:379-392`:
```python
        for m in NUMBER_LABEL.finditer(text.folded):
            label_words = text.words(m.start(), m.end())
            first, last = min(label_words), max(label_words)
            values += number_value(words, [i for i in line if i > last])
            if not any(i > last for i in line):
                # A label that ends its extracted line: its number is on its row, in
                # another block, else under it (Codex CB-18, as CB-12 and AB-2).
                values += number_value(words, row_after(words, last)) or number_value(
                    words, column_below(words, first, label_gap(words, first))
                )
            # A number glued to its label ("CRM:123") is in the label's word, however
            # short (Codex CB-11).
            values += [i for i in label_words if any(c.isdigit() for c in words[i].text)]
```
WAIT A SECOND!
Look at `detect.py:380-391`!
What if a number label has its state or colon AFTER it on the same line, and THEN the line ends (or the number is in another block on the same row or under it)?
Example 1:
Block 1: `CRM:` (where `:` is a detached word: `["CRM", ":"]`) and Block 2 on the same row (or below it): `["123456"]`!
Look at what happens when Block 1 is `["CRM", ":"]` and Block 2 is `["123456"]`:
1. `NUMBER_LABEL` is `(?<![^\W_])(?:crm|cpf|rg|cns|cnes|crbm|crf|coren)(?![^\W_\d])`.
2. In `line = [0, 1]` (`words[0] = "CRM"`, `words[1] = ":"`), `NUMBER_LABEL` matches `"crm"` which is ONLY word `0` (`label_words = {0}`, `first = 0`, `last = 0`)!
3. `[i for i in line if i > last]` is `[1]` (the detached colon `":"`!).
4. `number_value(words, [1])` sees `words[1] = ":"`, which has no digit, so `number_value` returns `[]`!
5. Next, line 383 checks:
   `if not any(i > last for i in line):`
   Because `1 > 0` (`":"` is in `line` after `"CRM"`!), `any(i > last for i in line)` is **`True`**!
   So the `if not any(i > last for i in line):` branch is **NOT TAKEN**!
   And because `"crm"` is in `NUMBER_LABELS`, NOT in `COLON_LABELS` (`COLON_LABELS` does NOT contain `"crm"`, `"cpf"`, `"rg"`, `"cns"`, `"cnes"`, `"crbm"`, `"crf"`, `"coren"`!), `COLON_LABEL` does NOT match `"CRM :"` either!
   Result: When `CRM :` (or `RG :`, `CPF :`, `CNS :`, `CRM/SP`, `CRM - SP`, `CRM SP :`) is in Block 1 and the number `123456` is in Block 2 (beside it on the same row, or under it in a column!), `any(i > last for i in line)` is `True` because `/SP`, `SP`, or `:` followed `CRM` in Block 1! So neither `row_after` nor `column_below` is EVER searched!
6. Even WITHOUT multiple blocks! What if `CRM/SP` (two words: `"CRM"`, `"/SP"`, or `"CRM"`, `"SP"`, or `"CRM:"`, `"SP"`, or `"CRM"`, `":"`) is a column heading or ends its line, and the number `123456` is on the NEXT line under it?!
   Let's trace `placed(("CRM", 0), ("SP", 50), number=0) + placed(("123456", 0), number=1)` or `line("CRM", ":", number=0) + line("123456", number=1)`:
   - Line 0 has `words[0] = "CRM"`, `words[1] = ":"` (or `"SP"` or `"/SP"`).
   - `NUMBER_LABEL` matches `"crm"` (`first = 0, last = 0`).
   - `number_value(words, [1])` returns `[]` (no digits in `":"` or `"SP"`).
   - `if not any(i > last for i in line):` is **`False`** because `1 > 0`!
   - So `column_below` and `row_after` are NEVER called!
   - And on Line 1 (`"123456"`), there is no label and `123456` is not a pattern in `REDACTED_PATTERNS`, so `"123456"` is **completely missed and survives unredacted**!

7. Wait — what if `CRM/SP` is a SINGLE word (`"CRM/SP"`) and ends its block/line, with `"123456"` beside it in another block or under it on the next line?!
   Let's trace `words[0] = "CRM/SP"`!
   - `text.folded` is `"crm/sp"`.
   - `NUMBER_LABEL` matches `"crm"` (chars 0..3).
   - `label_words = text.words(0, 3)` -> `{0}`. `first = 0, last = 0`.
   - Wait: what if `words[0]` is `"CRM/SP"` and `words[1]` is `"123456"` on the SAME line?
     Then `[i for i in line if i > last]` is `[1]`, and `number_value` sees `"123456"` — wait: why did `"CRM/SP"` work on the same line? Because `"CRM/SP"` was a single token!
   - What if `words[0]` is `"CRM"` and `words[1]` is `"SP"` (or `":"` or `"-"` or `"nº"`), and `"123456"` is in another block on the same row or on the line below?
     Because `words[1]` (`"SP"`, `":"`, `"nº"`) is on the same extracted line after `"CRM"`, `any(i > last for i in line)` is `True`! So `row_after` and `column_below` are completely skipped!
8. Even worse! What if on the SAME extracted line, there are 4 tokens after `"CRM"`: e.g. `CRM : SP - 123456` or `CRM - SP : 123456` or `CRM / SP nº 123456`?
   `NUMBER_VALUE_WORDS = 4` (`detect.py:73`).
   In `line("CRM", ":", "SP", "-", "123456")` (or `line("CRM", "/", "SP", ":", "123456")`):
   `last` is `0` (`"CRM"`).
   `seq` is `[1, 2, 3, 4]` (`[":", "SP", "-", "123456"]`).
   `seq[:NUMBER_VALUE_WORDS]` is `[1, 2, 3, 4]`, which has 4 words.
   Wait: what if `line("CRM", "/", "SP", "-", "123.456", "-", "X")` or `line("CRM", ":", "nº", "123", "456")`? The 5th word after `"CRM"` is dropped because `":"`, `"/"`, `"SP"`, `"-"` consumed slots in `seq[:NUMBER_VALUE_WORDS]`!
9. And what if a number label is glued to a state WITHOUT punctuation or after `:`? No, what if a number is glued to its label AND has a second part after it, or what if a number is glued to the colon after a detached colon (`CRM :123456`)? That's in `seq`.
10. What if a `NUMBER_LABEL` has its number on the next line in block order when it's NOT directly aligned at `x = left` (e.g., centered column heading `   CRM   ` above `123456-SP`, or indented on the next line)?Wait: `column_below` checks `abs(w.box[0] - left) <= gap / 2`.

Let's double-check point 5 & 6 above!
In `detect.py:380-388`:
```python
        for m in NUMBER_LABEL.finditer(text.folded):
            label_words = text.words(m.start(), m.end())
            first, last = min(label_words), max(label_words)
            values += number_value(words, [i for i in line if i > last])
            if not any(i > last for i in line):
                # A label that ends its extracted line: its number is on its row, in
                # another block, else under it (Codex CB-18, as CB-12 and AB-2).
                values += number_value(words, row_after(words, last)) or number_value(
                    words, column_below(words, first, label_gap(words, first))
                )
```
Look at what happens when:
- A form has a detached colon on a number label: `Word("RG", (0, 0, 30, 15), ("ocr", 1, 1, 0))`, `Word(":", (35, 0, 40, 15), ("ocr", 1, 1, 0))`, and the value `Word("12.345.671-X", (100, 0, 220, 15), ("ocr", 2, 1, 0))` (in another block on the same row, or on the next line `(0, 20, 120, 35)`!).
- Or a form has `CRM SP` or `CRM/SP:` (as two words `"CRM"` and `"SP"` or `"CRM"` and `":"`) as a column header or block-ending label, with `"123456"` beside it in another block or under it on the next line!
- Or a form has `CPF:   RG:` on one line (two column headers / stacked fields on a form!) and the CPF and RG values on the line below (`123.456.789-00   12.345.671-X`)!
  WAIT A SECOND!
  Look at that third case:
  Suppose a form has two fields side by side on the same extracted line (as `pdftotext -bbox-layout` and `tesseract` routinely group horizontally aligned labels into a single `line`!):
  Line 0: `Dt. Nasc.:` at x=0..80, and `Sexo:` at x=200..250 (in the SAME extracted `line`!).
  Line 1: `01/02/1980` at x=0..80, and `F` at x=200..220!
  WAIT A MINUTE!
  Look at `test_a_date_of_birth_under_its_label_in_another_block_is_taken` (`test_detect.py:241-250`):
  In that test, the author put `Dt. Nasc.:` in block 1 (`("ocr", 1, 1, 0)`) and `Sexo:` in block 2 (`("ocr", 2, 1, 0)`).
  What if `pdftotext -bbox-layout` or `tesseract` puts `Dt. Nasc.:` and `Sexo:` (or in `test_a_name_label_without_a_colon_heads_the_column_of_the_name`, `Nome`, `Data`, `Altura`) on the **SAME extracted line** (`("ocr", 1, 1, 0)`)?!
  Let's trace what happens when a table header or two stacked fields are on the SAME extracted line!
  1. In `test_a_name_label_without_a_colon_heads_the_column_of_the_name`:
     Line 0 (`("ocr", 1, 1, 0)`): `("Nome", 0), ("Data", 600), ("Altura", 900)`
     Line 1 (`("ocr", 1, 1, 1)`): `("Sicrano", 0), ("Inventado", 100), ("05/06/2021", 600)`
     Why did `Nome` work there? Because `Nome` was at the start of Line 0 (`BARE_LABEL.match`), and `bare_value` ALWAYS tries:
     1) `beside(words, last, gap)` -> `[]` (since `"Data"` is at x=600, gap > 30)
     2) `column_below(words, first, gap)` -> `["Sicrano", "Inventado"]`!
  2. Now, what if the column heading on Line 0 has a COLON (`Nome:` at x=0, `Data:` at x=600), OR what if the second column on Line 0 is `Data de Nascimento` (or `Dt. Nasc.:` or `CPF` or `RG` or `CRM` or `Paciente` or `Mãe`)?!
     Let's trace BOTH:
     - **Subcase 2a (Colon label in a multi-column header or stacked form row on the same extracted line):**
       Line 0 (`("ocr", 1, 1, 0)`): `("Dt.", 0), ("Nasc.:", 100), ("Sexo:", 600)`
       Line 1 (`("ocr", 1, 1, 1)`): `("01/02/1980", 0), ("Feminino", 600)`
       Let's trace `detect.py:365-378` on `Dt. Nasc.:`:
       - `first = 0, last = 1`.
       - `glued = False`.
       - `value = colon_value(words, line, last)`:
         `after` in `line` is `[2]` (`"Sexo:"`).
         `ends_field` sees `":"` in `"Sexo:"` and breaks! So `value` is `[]`!
       - Next: `elif not any(i > last for i in line):`
         Is `any(i > last for i in line)` False? **NO!** Because word `2` (`"Sexo:"`) is in `line` with `2 > 1`!
         Wait: why did `CB-17` add `elif not any(i > last for i in line):`?
         Because in CB-17 (`Paciente:   Idade: 45` on Line 0, `Hemoglobina 13,5` on Line 1), `Paciente:` was a blank inline field followed immediately by `Idade: 45`.
         Wait: in stacked form boxes (very common in Brazilian lab reports, where a row of boxes has labels at the top of each box `| Paciente:          | Dt. Nasc.:   | Sexo: |` and values at the bottom of each box `| Fulano Ficticio    | 01/02/1980   | M     |`), `pdftotext -bbox-layout` and `tesseract` put all the box-top labels on one `<line>` (`Paciente: Dt. Nasc.: Sexo:`) and all the box-bottom values on the next `<line>` (`Fulano Ficticio 01/02/1980 M`)!
         Wait: in CB-17 (`test_a_blank_field_followed_by_another_field_takes_nothing_below`), the test explicitly asserts:
         `words = line("Paciente:", "Idade:", "45", number=0) + line("Hemoglobina", "13,5", number=1)`
         `assert found(words) == {}`!
         So we MUST NOT break `test_a_blank_field_followed_by_another_field_takes_nothing_below`! (In that test, `"Hemoglobina"` is directly below `"Paciente:"` at x=0, y=20, and the test requires `found(words) == {}`!).
     - **Subcase 2b (Bare label not first on its extracted line, e.g. second column header):**
       Line 0: `("Exame", 0), ("Nome", 300), ("Data", 600), ("Nasc", 700)`
       Line 1: `("Hemograma", 0), ("Sicrano", 300), ("01/02/1980", 600)`
       Because `BARE_LABEL.match(text.folded)` only checks at the very beginning of `line`, neither `"Nome"` at x=300 nor `"Data Nasc"` at x=600 is matched!
     - **Subcase 2c (`NUMBER_LABEL` followed by non-value words of the label itself on the same extracted line):**
       Look at `detect.py:379-389`:
       Suppose Line 0 (or Block 1) is `CRM/SP:` or `CRM :` or `CRM - SP` or `CRM SP` (`words = [Word("CRM", ...), Word("SP", ...)]` or `[Word("CRM", ...), Word(":", ...)]`), and the number `"123456"` is in Block 2 on the same row or on Line 1 directly below `"CRM"`!
       In `detect.py:382-383`:
       `values += number_value(words, [i for i in line if i > last])`
       `if not any(i > last for i in line):`
       Here, `words[1]` (`":"` or `"SP"` or `"-"` or `"nº"`) is part of the label/qualifier (`NUMBER_PART` matches `":"`, `"sp"`, `"-"`, `"nº"`, and none of them has a digit!).
       Because `words[1]` is in `line` (`1 > last`), `any(i > last for i in line)` is `True`, even though every word after `last` in `line` is just a separator/state/qualifier (`NUMBER_PART` without digits)!
       So when a block is `["CRM", ":"]` or `["CRM", "SP"]` or `["RG", ":"]` and the number `"123456"` is in the next block on the same row or on the line below, `not any(i > last for i in line)` is `False` and the number is NEVER taken!
       Even simpler: what if a single word is `"CRM:"` vs two words `"CRM"` `":"`?
       With `"CRM:"` (one word), `last = 0`, `any(i > last for i in line)` is `False`, so it checks `row_after` and `column_below`.
       With `"CRM"` `":"` (two words, detached colon) or `"CRM"` `"SP"` (state after CRM), `last = 0`, `line` has word `1`, so `any(i > last for i in line)` is `True`, and it NEVER checks `row_after` or `column_below`!
       And what if `"123456"` IS in `row_after` or `column_below`, or what if `"CRM"` is on the same row as `":"` (in another block) and `"123456"`?
       In `row_after(words, last)`: if `last` is `"CRM"` and `"123456"` is below `"CRM"` (`column_below`), while nothing is in `row_after`, `column_below` is skipped because `":"` or `"SP"` was after `"CRM"` on its line!

Let's also check what happens when `COLON_LABEL` has a detached colon or multiple words and `column_below` is called!
Look at `detect.py:375-377`:
```python
value = beside(words, last, float("inf")) or column_below(
    words, first, label_gap(words, first)
)
```
WAIT A SECOND!
Look at `column_below(words, first, label_gap(words, first))` in `detect.py:253-269`:
```python
def column_below(words: list[Word], label: int, gap: float) -> list[int]:
    """The value under a column heading: the nearest word below `label` on its page that
    starts where the heading starts, and the words after it on its row."""
    left, top, _, bottom = words[label].box
    under = [
        i
        for i, w in enumerate(words)
        if w.line[0] == words[label].line[0]
        and w.box[1] > (top + bottom) / 2
        and w.box[1] - bottom <= gap
        and abs(w.box[0] - left) <= gap / 2
    ]
    if not under:
        return []
    first = min(under, key=lambda i: words[i].box[1])
    return [first, *run_from(words, row_after(words, first), words[first].box[2], gap, gap)]
```
Look at what `column_below` does when `first` (the first word of the label) is passed:
`gap = label_gap(words, first) = 2 * height`.
So `gap / 2 = height` (e.g., 15 pixels)!
`column_below` requires the word below the label to satisfy:
`abs(w.box[0] - left) <= gap / 2` (i.e. `w.box[0]` must be within 1 label-height of `left`!).
Wait: that works when the value starts at the same `x` coordinate as the label (`x = 0`).

Now let's check if there are other issues in `detect.py`, `locate.py`, `redact.py`, `batch.py`, `terms.py`, `cli.py`.
Let's check every single file!

### Checking `terms.py`:
Look at `committable(path: str) -> bool` (`terms.py:154-183`):
```python
def committable(path: str) -> bool:
    """True when `path` lies in a git work tree and git does not ignore it. The path
    need not exist yet: git runs from its nearest existing directory."""
    directory = os.path.dirname(os.path.abspath(path))
    while not os.path.isdir(directory) and os.path.dirname(directory) != directory:
        directory = os.path.dirname(directory)
    # Git finds the repository from the path alone, not from an inherited GIT_DIR.
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    inside = subprocess.run(
        ["git", "-C", directory, "rev-parse", "--is-inside-work-tree"],
        capture_output=True,
        check=False,
        env=env,
    )
    if inside.returncode != 0:
        # Git failing is "outside" only where no repository is: under a `.git`, it
        # could not tell, and the answer would fail open (Codex CB-16).
        if any(os.path.lexists(os.path.join(d, ".git")) for d in ancestors(directory)):
            raise TermsError("git could not tell whether a path is in a work tree; check git")
        return False
    if inside.stdout.strip() != b"true":
        return False
    ignored = subprocess.run(
        ["git", "-C", directory, "check-ignore", "-q", os.path.abspath(path)],
        capture_output=True,
        check=False,
        env=env,
    )
    return ignored.returncode != 0
```
Wait: what if `git` is not installed on `PATH` (or fails to start with `OSError`, e.g., `FileNotFoundError`)?
`subprocess.run(["git", ...])` will raise `FileNotFoundError` (an `OSError`), which in `cli.py` is caught by `except OSError as e:` and prints `anonimizador batch: No such file or directory` and returns `EXIT_USAGE` (2).
What if `git check-ignore -q` returns exit status `128` (fatal error, e.g., corrupt `.git/info/exclude` or bad `.gitignore` syntax or unreadable index)?
`ignored.returncode != 0` is `True` (since `128 != 0`), so `committable` returns `True` (fails closed!).
Wait: what if `path` has a non-existent intermediate directory that is a **broken symlink** pointing into a git repository?!
Let's trace this!
Suppose the user runs:
`anonimizador batch --out /tmp/outside/link-to-repo-subdir ...`
where `/tmp/outside` is outside any git repository, and `/tmp/outside/link-to-repo-subdir` is a **symbolic link** pointing to `/path/to/repo/nonexistent-subdir` (a directory inside the git work tree that does not exist yet!).
Let's trace what `start(out, now)` (`batch.py:171-191`) does!
1. `out` is `Path("/tmp/outside/link-to-repo-subdir")`.
2. `run.report` is `/tmp/outside/link-to-repo-subdir/20261030-140509-anonimizer.md`.
3. `uncommittable(run.report)` runs:
   - `committable(str(run.report))`:
     `path` is `"/tmp/outside/link-to-repo-subdir/20261030-140509-anonimizer.md"`.
     `directory` starts at `"/tmp/outside/link-to-repo-subdir"`.
     Is `os.path.isdir("/tmp/outside/link-to-repo-subdir")` True?
     **NO!** Because its target `/path/to/repo/nonexistent-subdir` does not exist yet! `os.path.isdir` follows symlinks and returns `False` for a symlink whose target does not exist!
     So the `while not os.path.isdir(directory)` loop walks UP to `os.path.dirname(directory)`, which is `"/tmp/outside"`!
     Then `git -C /tmp/outside rev-parse --is-inside-work-tree` runs in `/tmp/outside` (outside the repo!) and returns `False`!
   - Next, `committable(os.path.realpath(run.report))` runs!
     What does `os.path.realpath("/tmp/outside/link-to-repo-subdir/20261030-140509-anonimizer.md")` return?
     `os.path.realpath` resolves the symlink `/tmp/outside/link-to-repo-subdir` to `"/path/to/repo/nonexistent-subdir"`, and appends `"20261030-140509-anonimizer.md"`, returning:
     `"/path/to/repo/nonexistent-subdir/20261030-140509-anonimizer.md"`!
     Then `committable` walks up from `"/path/to/repo/nonexistent-subdir"` to `"/path/to/repo"` (which DOES exist and IS a git work tree!) and returns `True`!
     So `committable(os.path.realpath(path))` catches that!
   - Wait: what happens at line 188 of `batch.py`:
     `out.mkdir(parents=True, exist_ok=True)`?
     If `out` is a dangling symlink `/tmp/outside/link-to-something`, `Path.mkdir(parents=True, exist_ok=True)` raises `FileExistsError` on the last component if it's a dangling symlink (in Python `Path.mkdir(exist_ok=True)` only ignores `FileExistsError` if `self.is_dir()` is True!).

### Checking `batch.py`:
Let's trace `batch.py` line by line!
Look at `originals(source: Path)` (`batch.py:139-168`):
```python
def originals(source: Path) -> list[Exam]:
    """The PDF, PNG, and JPEG files directly in `source`, in name order. Directories
    are not walked: the outputs usually live in one under `source`."""
    try:
        candidates = [
            p
            for p in source.iterdir()
            if not p.name.startswith(APPLEDOUBLE) and p.suffix.lower() in SUFFIXES
        ]
        # A link that leads nowhere is an original never read: an error, as in
        # `verify`, not a skipped file (Antigravity AB-6).
        if any(p.is_symlink() and not p.exists() for p in candidates):
            raise VerifyError("a link among the originals leads nowhere")
        paths = sorted(p for p in candidates if p.is_file())
    except OSError as e:
        # The message of an OSError holds the path: report its reason only.
        raise VerifyError(f"cannot list the originals ({e.strerror or type(e).__name__})") from None
```
Wait: what if `neutral_name(path)` in `originals` (`batch.py:161`) raises `OSError` (for example, one of the originals in `source` is unreadable, `chmod 000`, or is a FIFO/device or vanishes)?
In `originals`, lines 160-167 are OUTSIDE the `try ... except OSError` block!
Wait: in `cli.py:187-196`:
`run_batch` is called inside `try ... except OSError as e: print(f"anonimizador batch: {e.strerror or type(e).__name__}", file=sys.stderr); return EXIT_USAGE`.
What if `run_batch` is called directly (as part of the API, or in tests)? `originals` catches `OSError` during `iterdir`/`is_symlink`/`exists`/`is_file` and raises `VerifyError`, but `neutral_name(path)` in the same function (`originals`, line 161) opens and reads every file outside that `try` block.

Wait: look at `anonymize` (`batch.py:238-299`):
```python
    log = run.logs / f"{exam.log_prefix}-verify-original.txt"
    result, lines = verify_one(n, exam.original, exam.original.name, terms, options, log)
    for line in labelled(lines, n, "original"):
        console.print(line)
```
LOOK AT `labelled(lines, n, "original")` (`batch.py:226-228`):
```python
def labelled(lines: list[str], position: int, step: str) -> list[str]:
    """`verify` lines of a one-file run, labelled with the exam's position and the step."""
    return [line.replace("[1]", f"[{position}] {step}:", 1) for line in lines]
```
WAIT A SECOND!
Look at how `cli.py` and `verify.py` construct `options`!
In `verify.py:282-283`:
```python
def render(result: FileResult, options: Options) -> list[str]:
    """Report lines for one file: counts, positions, and kinds only."""
    label = f"[{result.index}] {result.path}" if options.show_names else f"[{result.index}]"
    lines = [f"{label} {result.status(options.strict_patterns)}"]
```
In `cli.py`, `batch` does not have `--show-names`, so `options.show_names` is `False` when called from CLI.
What if a caller of `run_batch(source, out, terms, options)` passes `Options(show_names=True)`?
Then `verify_one(n, exam.original, exam.original.name, terms, options, log)` calls `render(result, options)` where `result.path` is `exam.original.name` (the original's filename, which contains the patient's name!).
And `write_log(log, lines)` writes `exam.original.name` into `01-exam-<digest>-verify-original.txt` AND `console.print(line)` prints `[1] original: Relatorio Fulano Ficticio.pdf FOUND` to the console and `console.txt`!
Wait: why does `verify_one` pass `exam.original.name` as `CorpusFile.name` in the first place?!
Because `verify` searches the `name` channel (`texts["name"] = [file.name]`), so `verify` on the original checks if any term is in `exam.original.name`!
And if `options.show_names` is `True`, `render(result, options)` includes `result.path` (`exam.original.name`) in the log and console output, violating the privacy invariant (`batch.py:17`: "console output, logs, errors, and the detections JSON carry positions, counts, and kinds only, never a term, a match, document text, or an original's file name").

Wait — is there something even more direct in `verify_one`?
Look at `batch.py:250`:
`result, lines = verify_one(n, exam.original, exam.original.name, terms, options, log)`
Look at what `verify` does on `exam.original.name` (`verify.py:261-265`):
```python
    hints = count_patterns(
        [
            t
            for c in PATTERN_CHANNELS
            for t in (["\n".join(texts[c])] if c in PARTS_CHANNELS else texts[c])
        ]
        + reading.raw.strings,
        joins=reading.raw.joins,
        joined=reading.raw.drawn,
    )
```
That only counts patterns and terms.

Now let's check `locate.py`, `detect.py`, `redact.py` for any deeper bugs!

Let's check `detect.py` again:
Look at `detect.py:67-105` (`COLON_LABELS`, `NUMBER_LABELS`, `BARE_LABELS`):
1. Look at `COLON_LABELS` (`detect.py:29-67`):
```python
COLON_LABELS = (
    "paciente",
    "nome do paciente",
    "nome",
    "nome social",
    "nome da mae",
    "mae",
    "nome do pai",
    "pai",
    "responsavel",
    "prontuario",
    "atendimento",
    "pedido",
    "requisicao",
    "registro",
    "matricula",
    "carteirinha",
    "carteira",
    "convenio",
    "plano",
    "guia",
    "senha",
    "medico",
    "medica",
    "medico solicitante",
    "medico responsavel",
    "solicitante",
    "requisitante",
    "data de nascimento",
    "nascimento",
    "dt nasc",
    "d n",
    "endereco",
    "telefone",
    "celular",
    "e mail",
    "email",
    "cartao sus",
)
```
And look at `COLON_LABEL` (`detect.py:89-94`):
```python
COLON_LABEL = re.compile(
    LABEL_PREFIX
    + r"(?:"
    + "|".join(r"[\W_]*".join(map(re.escape, label.split())) for label in COLON_LABELS)
    + r")[\s.]*:"
)
```
HOLD EVERYTHING!
Look at lines 91-93 of `src/anonimizador/detect.py`!
```python
COLON_LABEL = re.compile(
    LABEL_PREFIX
    + r"(?:"
    + "|".join(r"[\W_]*".join(map(re.escape, label.split())) for label in COLON_LABELS)
    + r")[\s.]*:"
)
```
LOOK AT THE ORDER OF `COLON_LABELS` IN THE ALTERNATION `|` AND WHAT follows `)`:
Every branch of the `(?:a|b|c)` alternation is followed by `[\s.]*:`!
Wait: Because `[\s.]*:` is OUTSIDE the `(?:...)` group, a shorter prefix like `"nome"` in `(?:paciente|nome do paciente|nome|nome social|...)` will only match if `"nome"` is immediately followed by `[\s.]*:`.
Wait: what if the text is `"d.n.:"` (`"d n"` in `COLON_LABELS`)?
In `"d n"`, `label.split()` is `["d", "n"]`.
`r"[\W_]*".join(["d", "n"])` is `r"d[\W_]*n"`, followed by `[\s.]*:`.
If the text is `"D.N.:"`, `fold("D.N.:")` is `"d.n.:"`. `d` + `.` (`[\W_]*`) + `n` + `.` (`[\s.]*`) + `:` -> matches!
What if the text is `"Dt. Nasc.:"`?
`"dt nasc"` -> `r"dt[\W_]*nasc"`, followed by `[\s.]*:`.
In `"Dt. Nasc.:"`, `fold` is `"dt. nasc.:"`. `dt` + `. ` (`[\W_]*`) + `nasc` + `.` (`[\s.]*`) + `:` -> matches!
NOW WAIT:
What if the text in the exam is `"Data Nasc.:"` or `"Data de Nasc.:"`?!
Look at `COLON_LABELS` (`detect.py:57-60`):
- `"data de nascimento"`
- `"nascimento"`
- `"dt nasc"`
- `"d n"`
And look at `BARE_LABELS` (`detect.py:309-312`):
- `"data de nascimento"`
- `"data nasc"`
- `"dt nasc"`
- `"nascimento"`
Look at the difference between `BARE_LABELS` and `COLON_LABELS`!
In AB-3 (round 1), the implementer added `"data nasc"` to `BARE_LABELS` (`detect.py:310`), so `test_patient_mother_and_date_of_birth_labels_need_no_colon` passed on `placed(("Data", 0), ("Nasc", 100), number=0)`.
**The implementer FORGOT to add `"data nasc"` (and `"data de nasc"`) to `COLON_LABELS`!**
Let's trace what happens when a document has `"Data Nasc.:"` or `"Data Nasc:"`!
1. Suppose a document has `Data Nasc.: 01/02/1980` (with a colon and a period, the standard Brazilian abbreviation `Data Nasc.:`)!
   Let's trace `label_values` on `line("Data", "Nasc.:", "01/02/1980")`!
   - `text.folded` is `"data nasc.: 01/02/1980"`.
   - First, line 358:
     `if (m := BARE_LABEL.match(text.folded)) and not COLON_LABEL.match(text.folded):`
     - `BARE_LABEL.match("data nasc.: 01/02/1980")` matches `"data nasc"` (spans 0..9)!
       Why? Because `BARE_LABEL` is `(?:...|data[\W_]+nasc|...)(?![^\W_])`, and at index 9 the next character is `"."`, which satisfies `(?![^\W_])`!
     - `COLON_LABEL.match("data nasc.: 01/02/1980")` is **`None`** because `"data nasc"` is NOT in `COLON_LABELS`!
     - So it enters `bare_value`!
     - `m.start(), m.end()` is `(0, 9)`.
     - `text.words(0, 9)` is `{0, 1}` (`words[0] = "Data"`, `words[1] = "Nasc.:"`).
     -Wait: if `"01/02/1980"` is glued to `"Nasc.:"` (`"Data"`, `"Nasc.:01/02/1980"`):
       At index 9 in `"data nasc.:01/02/1980"`, `m` still matches `"data nasc"`, `last = 1`, and `bare_value` looks `beside(words, 1, gap)` (to the right of word 1) and `column_below(words, 0, gap)` (below word 0) — it NEVER checks `glued` on word 1! So `"Data"`, `"Nasc.:01/02/1980"` returns `[]`!
     - Even worse: what if `"Data Nasc.:"` is NOT the first word on the line (e.g., `Paciente: Fulano   Data Nasc.: 01/02/1980`, just like `CPF 123.456.789-09   Dt. Nasc.: 01/02/1980` in `test_batch.py:42`)?!
       Let's trace `line("CPF", "123.456.789-09", "Data", "Nasc.:", "01/02/1980")` with `remove_dates=False` (`--keep-dates`):
       - `text.folded` is `"cpf 123.456.789-09 data nasc.: 01/02/1980"`.
       - `BARE_LABEL.match(text.folded)` is **`None`** (because the line starts with `"cpf"`, not `"data nasc"`!).
       - `COLON_LABEL.finditer(text.folded)` yields **nothing** (because `"data nasc"` is not in `COLON_LABELS`, and `"nasc.:"` is not in `COLON_LABELS` either!).
       - `NUMBER_LABEL.finditer(text.folded)` matches `"cpf"` and takes `"123.456.789-09"`.
       - `"01/02/1980"` is **NOT DETECTED AT ALL**!
     - And what if the line is `line("Data", "Nasc", ":", "01/02/1980")` (at the start of a line, with a detached colon)?
       - `BARE_LABEL.match` matches `"data nasc"` (words 0, 1).
       - `COLON_LABEL.match` is `None`.
       - `bare_value` calls `beside(words, 1, gap)`.
       - The first word after word 1 (`"Nasc"`) is word 2 (`":"`).
       - `run_from` calls `ends_field(words, row, 0)` on word 2 (`":"`).
       - `ends_field` returns `True` because `":" in ":"`!
       - So `run_from` immediately breaks and returns `[]`!
       - `column_below` returns `[]`.
       - `beside(words, 1, inf)` also immediately breaks on `":"` and returns `[]`!
       - `"01/02/1980"` is **NOT DETECTED AT ALL**!

Let's check if ANY OTHER label in `BARE_LABELS` has the detached-colon bug (`Nome : Fulano`, `Paciente : Fulano`, `Mae : Fulana`, `Data de nascimento : 01/02/1980`)!
WAIT A SECOND!
Trace `line("Nome", ":", "Sicrano")` or `line("Paciente", ":", "Sicrano")` or `line("Data", "de", "nascimento", ":", "01/02/1980")` or `line("Dt.", "Nasc.", ":", "01/02/1980")`!
Wait: why did `test_a_label_with_a_detached_or_glued_colon` (`test_detect.py:107`) pass on `line("Nome", ":", "Sicrano")`?
Because `"nome"` IS in `COLON_LABELS`! So for `line("Nome", ":", "Sicrano")`:
`text.folded` is `"nome : sicrano"`.
`COLON_LABEL` has `[\s.]*:`, which matches `" "` + `":"`, so `COLON_LABEL.match("nome : sicrano")` matched `"nome :"`, skipping `BARE_LABEL`, and `COLON_LABEL.finditer` matched `"nome :"` (`last = 1`, which is `":"`), and `colon_value` took words after `last = 1`, which is `"Sicrano"`!
Now, why did `line("Data", "Nasc", ":", "01/02/1980")` and `line("Data", "Nasc.:", "01/02/1980")` (when not at start of line, or glued `Data Nasc.:01/02/1980`) FAIL?
Because `"data nasc"` is in `BARE_LABELS` (`detect.py:310`) and **missing from `COLON_LABELS`** (`detect.py:57-60`)!

Now check `NUMBER_LABELS` with a detached colon or state!
Look at `NUMBER_LABELS` (`detect.py:70`):
`NUMBER_LABELS = ("crm", "cpf", "rg", "cns", "cnes", "crbm", "crf", "coren")`
None of `NUMBER_LABELS` are in `COLON_LABELS`!
Now trace what happens when a number label has a detached colon (`CRM :`, `RG :`, `CPF :`, `CNS :`) or a state (`CRM SP`, `CRM/SP :`) at the end of a block or line, and its number is in the next block on the same row or on the line below (`column_below`):
Let's trace `detect.py:379-391`:
```python
        for m in NUMBER_LABEL.finditer(text.folded):
            label_words = text.words(m.start(), m.end())
            first, last = min(label_words), max(label_words)
            values += number_value(words, [i for i in line if i > last])
            if not any(i > last for i in line):
                # A label that ends its extracted line: its number is on its row, in
                # another block, else under it (Codex CB-18, as CB-12 and AB-2).
                values += number_value(words, row_after(words, last)) or number_value(
                    words, column_below(words, first, label_gap(words, first))
                )
```
Look at what happens in THREE concrete scenarios for `NUMBER_LABEL`:
Scenario 1: The label is `"CRM"` `"SP"` (or `"CRM"` `":"`, or `"RG"` `":"`) on Line 0, and the number `"123456"` (or `"12.345.671-X"`) is on Line 1 under it (or in Block 2 beside it on the same row):
- `m` matches `"crm"` (or `"rg"`).
- `label_words` is `{0}`. `first = 0, last = 0`.
- `line` is `[0, 1]` (where `words[1]` is `"SP"` or `":"`).
- `number_value(words, [i for i in line if i > last])` calls `number_value(words, [1])`, which checks `[":"]` or `["SP"]`. Since neither has a digit (`has_digit` is `False`), `number_value` returns `[]`!
- Next: `if not any(i > last for i in line):`
  Because `1 > 0` (`words[1]` is in `line`), `not any(i > last for i in line)` is **`False`**!
- Therefore, `row_after` and `column_below` are **never called**! The number `"123456"` or `"12.345.671-X"` is completely missed!

Scenario 2: What if `"CRM"` is in Block 1 (`Word("CRM", (0, 0, 40, 15), ("ocr", 1, 1, 0))`), or `"CRM"` is above `"SP"` `"123456"`, wait: what if `"CRM:"` is at `(0, 0, 50, 15)` on Line 0, and on the SAME row to its right in another block is `"Hemoglobina"` (another column header at `(300, 0, 400, 15)`), while `"123456"` is UNDER `"CRM:"` at `(0, 20, 60, 35)`?!
Wait: let's check what happens for a `COLON_LABEL` (`Dt. Nasc.:` or `Paciente:` or `Nome:`) when it is a column heading in Block 1 (`(0, 0, 80, 15)`), and another column heading in Block 2 (`(200, 0, 250, 15)`) is on the SAME row to its right!
WAIT A SECOND!
Look at `test_a_date_of_birth_under_its_label_in_another_block_is_taken` in `tests/test_detect.py:241-250`!
Let's read lines 241-250 of `tests/test_detect.py` VERY CAREFULLY!
```python
def test_a_date_of_birth_under_its_label_in_another_block_is_taken():
    # Antigravity AB-3: with "Sexo:" next in block order, the date was missed.
    words = [
        Word("Dt.", (0, 0, 30, 15), ("ocr", 1, 1, 0)),
        Word("Nasc.:", (35, 0, 80, 15), ("ocr", 1, 1, 0)),
        Word("Sexo:", (200, 0, 250, 15), ("ocr", 2, 1, 0)),
        Word("01/02/1980", (0, 20, 80, 35), ("ocr", 3, 1, 0)),
        Word("F", (200, 20, 220, 35), ("ocr", 4, 1, 0)),
    ]
    assert found(words, remove_dates=False) == {"label": {"01/02/1980"}}
```
LOOK AT WHY THAT TEST PASSED:
In that test, the second column header in Block 2 is `"Sexo:"` (WITH a colon!).
When `Dt. Nasc.:` ran `detect.py:375`:
```python
value = beside(words, last, float("inf")) or column_below(
    words, first, label_gap(words, first)
)
```
Why did `beside(words, last, float("inf"))` return `[]` in that test?!
Because `"Sexo:"` had a colon (`ends_field` was `True` on `"Sexo:"`), AND because `beside` used `reach = float("inf")`, wait:
What if the second column header in Block 2 on the same row does NOT have a colon (for example, `Sexo` without a colon, or `Idade`, or `Data`, or `Exame`, as in `test_a_name_label_without_a_colon_heads_the_column_of_the_name`: `("Nome", 0), ("Data", 600), ("Altura", 900)`)?!
LOOK AT `detect.py:375`:
`value = beside(words, last, float("inf")) or column_below(words, first, label_gap(words, first))`
`beside(words, last, float("inf"))` passes **`reach = float("inf")`**!
Let's trace what happens when a form has:
- Column 1 header (Block 1): `Dt. Nasc.:` at `(0, 0, 80, 15)` (or `Paciente:` or `Nome:`)
- Column 2 header (Block 2): `Sexo` (or `Idade` or `Data` — without a colon!) at `(600, 0, 650, 15)` (600 pixels away, across the page!)
- Column 1 value (Block 3, below `Dt. Nasc.:`): `01/02/1980` at `(0, 20, 80, 35)` (or `Fulano Ficticio` below `Paciente:`)!
Let's trace `detect.py:375` on `Dt. Nasc.:` (or `Paciente:`):
1. `Dt. Nasc.:` ends its extracted line (Block 1).
2. Line 375 calls `beside(words, last, float("inf"))`!
3. `row_after(words, last)` finds `Word("Sexo", (600, 0, 650, 15), ...)` on the same row (600 pixels to the right!).
4. `run_from` checks `box[0] - edge > (gap if value else reach)`.
   Because `value` is empty (`[]`), `(gap if value else reach)` is **`reach` = `float("inf")`**!
   And `"Sexo"` has no colon (`ends_field` is `False`)!
   So `beside(words, last, float("inf"))` **TAKES `"Sexo"` (from 600px away across the page!)** as the value of `Dt. Nasc.:` (or `Paciente:`)!
5. Because `beside(words, last, float("inf"))` is now truthy (`[2]`, i.e., `["Sexo"]`), the `or column_below(...)` short-circuits and **NEVER RUNS**!
6. As a result, `"Sexo"` is redacted, and the actual date of birth `"01/02/1980"` under `"Dt. Nasc.:"` (or the patient's name `"Fulano Ficticio"` under `"Paciente:"`) is **COMPLETELY MISSED AND SURVIVES UNREDACTED**!

MIND BLOWN!
Compare `bare_value` (`detect.py:324-336`) with `COLON_LABEL` (`detect.py:375-377`):
In `bare_value` (`detect.py:329-333`):
```python
    for value in (
        beside(words, last, gap),
        column_below(words, first, gap),
        beside(words, last, float("inf")),
    ):
```
In `bare_value`, it checks:
1. `beside(words, last, gap)` (a value close beside the label, within `gap = 2 * height`)
2. `column_below(words, first, gap)` (a value directly under the label, within `gap = 2 * height`)
3. `beside(words, last, float("inf"))` (a value far to the right on the same row, if there was nothing close beside it and nothing under it!)
Whereas in `COLON_LABEL` (`detect.py:375-377`):
```python
                value = beside(words, last, float("inf")) or column_below(
                    words, first, label_gap(words, first)
                )
```
It skipped `beside(words, last, gap)` and went straight to `beside(words, last, float("inf"))` BEFORE `column_below`!
So if there is ANY word without a colon anywhere to the right on the same horizontal row across the page (such as another column header `Sexo`, `Idade`, `Convênio`, `Unidade`, or form text in another column), `beside(words, last, float("inf"))` grabs that distant word instead of the value directly underneath the label (`column_below`)!
AND even in `bare_value`:
Look at `bare_value` when the label is `Paciente` or `Nome` or `Dt. Nasc.` in a table/form header:
Suppose a table header has `Nome` at `(0, 0, 50, 15)` (height 15, so `gap = 30`), and the next column header `"Data"` is at `(75, 0, 120, 15)` (25px to the right of `"Nome"`, which is `<= gap` of 30px!).
Because `"Data"` does not have a colon, `beside(words, last, gap)` takes `["Data"]`, so `Nome` takes `"Data"` instead of the patient's name `"Sicrano Inventado"` directly below `"Nome"` at `(0, 20, 90, 35)`!

Wait: verify the `COLON_LABEL` bug (`beside(words, last, float("inf"))` before `column_below`) first!
Let's trace:
```python
words = [
    Word("Dt.", (0, 0, 30, 15), ("ocr", 1, 1, 0)),
    Word("Nasc.:", (35, 0, 80, 15), ("ocr", 1, 1, 0)),
    Word("Sexo", (200, 0, 250, 15), ("ocr", 2, 1, 0)),
    Word("01/02/1980", (0, 20, 80, 35), ("ocr", 3, 1, 0)),
    Word("F", (200, 20, 220, 35), ("ocr", 4, 1, 0)),
]
```
In `test_a_date_of_birth_under_its_label_in_another_block_is_taken` (`test_detect.py:241`), the ONLY difference is `"Sexo"` instead of `"Sexo:"` at `x = 200` (120 pixels away from `Nasc.:`, which is 8 label heights away!).
With `"Sexo"` (a bare column heading in the next box, 120px to the right):
- `Dt. Nasc.:` calls `beside(words, last, float("inf"))`.
- `beside` finds `"Sexo"` at `x = 200` because `reach` is `float("inf")`!
- `column_below` is never called!
- `found(words, remove_dates=False)` returns `{"label": {"Sexo"}}`, and `"01/02/1980"` (the date of birth directly below `Dt. Nasc.:` at `(0, 20, 80, 35)`) is **completely missed**!
Same thing if `Paciente:` is at `(0, 0, 80, 15)`, `Sexo` is at `(400, 0, 450, 15)`, and `Fulano Ficticio` is at `(0, 20, 90, 35)` directly under `Paciente:`:
`beside(words, last, float("inf"))` grabs `"Sexo"` at `x=400`, and `Fulano Ficticio` at `(0, 20)` under `Paciente:` is **completely missed**!
If `COLON_LABEL` instead checks `beside(words, last, gap) or column_below(words, first, gap) or beside(words, last, float("inf"))` (the exact same order as `bare_value`!), then:
1. When the value IS on the same row near the label (within `gap`), `beside(words, last, gap)` takes it.
2. When nothing is close beside the label (`gap`), it checks `column_below(words, first, gap)` and finds the value directly under the label (`01/02/1980` or `Fulano Ficticio`), ignoring distant column headers like `Sexo` at `x=200`!
3. When nothing is close beside the label AND nothing is directly under the label (`column_below` is `[]`), it falls back to `beside(words, last, float("inf"))`, so a value far to the right on a wide 2-column form (`Paciente:` at x=0, `Sicrano` at x=200 with nothing under `Paciente:`, as in `test_a_colon_label_ending_its_block_takes_the_value_on_its_row` where `Hemoglobina` is at `y=30`, or if `Hemoglobina` is at `y=20` wait: in `test_a_colon_label_ending_its_block_takes_the_value_on_its_row`, `Paciente:` is at `(0, 0, 80, 15)`, `Hemoglobina` is at `(0, 30, 90, 45)`, `Sicrano` is at `(100, 0, 180, 15)` — notice `Sicrano` is at `x=100`, so `100 - 80 = 20 <= gap (30)` AND `Hemoglobina` is at `y=30`, so `30 - 15 = 15 <= gap (30)`! Because `100 - 80 = 20 <= 30`, `beside(words, last, gap)` takes `Sicrano` first!).

And look at `NUMBER_LABEL` (`detect.py:384-386`) too!
```python
                values += number_value(words, row_after(words, last)) or number_value(
                    words, column_below(words, first, label_gap(words, first))
                )
```
Look at what `NUMBER_LABEL` does when `CRM` (or `RG:`) is a column heading at `(0, 0, 40, 15)` with `"123456"` under it at `(0, 20, 60, 35)`, AND on the header row at `(300, 0, 380, 15)` there is another column or header with a number (e.g. `"2026"` or `"1"` or `"Página"` `"1"` at `(500, 0, 520, 15)`):
1. `row_after(words, last)` returns ALL words on the same horizontal row all the way across the page, with NO `reach` or `gap` check at all!
2. Even if `"CRM"` is at `x=0` and `(500, 0)` (500px away on the right side of the header!) has `"1"` (e.g. page number or date), `number_value(words, row_after(words, last))` takes `"1"` from 500px away (if `"1"` is the first word to the right of `"CRM"` on that row), and `or number_value(words, column_below(...))` never runs, leaving `"123456"` directly under `"CRM"` unredacted!
3. And even when `"123456"` IS on the same row beside `"CRM"` (e.g. `CRM 123456 99` where `99` is 500px away in another column on the same row): `row_after(words, last)` has no `gap` limit (unlike `beside`), so `number_value(words, row_after(words, last))` will greedily take numbers from other columns 500px away on the same row!

Let's check if we need a plan or if the user prompt says:
Wait: look at the `<USER_REQUEST>` and `<ADDITIONAL_METADATA>`!
In `<USER_REQUEST>`:
`/plan You are Antigravity, an adversarial code reviewer for stages 1-2 of the anonimizador subproject (the batch command: detection, redaction by rasterizing, verification of the copies). The code is Python (>= 3.12, with Pillow): use Python best practices and idiomatic Python as your yardstick, as well as correctness. Read the review package .review/agy-batch-round2-package.md in the current directory first, in full: it states the task, the scope, the findings tracked so far with their resolutions, the full current files, and the complete diff against base ed56cb5. Where the package says not to use tools, you may still read that package and the files of this checkout, read-only; do not edit files and do not run commands. Output only the findings in the package's format, or exactly NO FINDINGS.`

Notice that the user explicitly instructed:
"Where the package says not to use tools, you may still read that package and the files of this checkout, read-only; do not edit files and do not run commands. Output only the findings in the package's format, or exactly NO FINDINGS."
And in the package:
"For EACH finding output exactly these fields:
ID (AB-1, AB-2, ...), Severity (critical/high/medium/low), File:line, Description, Impact,
Reproduction scenario or verifiable rationale, Validation suggestion.
This is your round 2. Since your round 1, commit 1792e6d answered it (AB-1 to AB-7; some parts refuted with evidence) and commit 0df51b8 fixed Codex round 3 (CB-18 to CB-22). Everything is listed below with its resolution. Report a finding only if it is new, or if a resolution or refutation does not hold. If you find nothing valid, output exactly: NO FINDINGS.
Do not praise. Do not restate the code. End with nothing else."

Let's format the findings cleanly as `AB-8`, `AB-9`, `AB-10` (or `AB-1`, `AB-2`, `AB-3` — wait: let's check how round 2 numbering works or if `AB-8` vs `AB-1 (round 2)` is better; the prompt says `ID (AB-1, AB-2, ...)`, so `AB-8`, `AB-9`, `AB-10` continues from `AB-1 to AB-7` in round 1, or we can label them `AB-8`, `AB-9`, `AB-10`). Let's check if every finding is 100% solid and indisputable.

1. **AB-8 (high)**: Missing `"data nasc"` (and `"data de nasc"`) in `COLON_LABELS` (`src/anonimizador/detect.py:57-60`, `309-313`, `358-360`), plus detached-colon handling when `BARE_LABEL` matches a label not in `COLON_LABELS`.
   - `"data nasc"` was added to `BARE_LABELS` in AB-3 (`detect.py:310`), but omitted from `COLON_LABELS` (`detect.py:57-60`).
   - Consequently:
     (a) When `Data Nasc:` or `Data Nasc.:` appears anywhere after the first word of a line (e.g., `Sexo: F   Data Nasc.: 01/02/1980`), `BARE_LABEL.match` fails (not at line start) and `COLON_LABEL.finditer` fails (`"data nasc"` not in `COLON_LABELS`), so the date of birth is completely missed under `--keep-dates`.
     (b) When `Data Nasc : 01/02/1980` (detached colon) appears at the start of a line, `BARE_LABEL.match` matches `"data nasc"` while `COLON_LABEL.match` is `None`; `bare_value` calls `beside`, which immediately stops at the detached `":"` token because `ends_field` treats `":"` as starting a new field, returning `[]` and leaving `01/02/1980` unredacted under `--keep-dates`.
     (c) When `Data Nasc:01/02/1980` (glued colon) appears at the start of a line, `BARE_LABEL.match` matches `"data nasc"` inside `"data nasc:01/02/1980"`, and `bare_value` only searches to the right of or below word 1 (`"Nasc:01/02/1980"`), never taking word 1 itself (unlike `COLON_LABEL`'s `glued` check), leaving `Nasc:01/02/1980` unredacted under `--keep-dates`.

2. **AB-9 (high)**: Multi-block / multi-column search for `COLON_LABEL` and `NUMBER_LABEL` (`src/anonimizador/detect.py:375-388`):
   - (1) In `COLON_LABEL` (`detect.py:375-377`), when a colon label ends its extracted line, it evaluates `beside(words, last, float("inf")) or column_below(words, first, label_gap(words, first))`. Unlike `bare_value` (`detect.py:329-333`), which checks `beside(words, last, gap)` first, then `column_below(words, first, gap)`, and only then `beside(words, last, float("inf"))`, `COLON_LABEL` uses `reach = float("inf")` before checking `column_below`. If a colon label is a column/box heading (e.g. `Dt. Nasc.:` or `Paciente:` at `x=0`) and another block on the same horizontal row to the right has a heading without a colon (e.g. `Sexo` or `Idade` at `x=200`), `beside(words, last, float("inf"))` grabs `"Sexo"` from across the page, short-circuiting `or column_below(...)` and leaving the actual value (`01/02/1980` or the patient's name) directly under the label unredacted.
   - (2) In `NUMBER_LABEL` (`detect.py:382-388`), the fallback to `row_after` / `column_below` is guarded by `if not any(i > last for i in line):`, where `last` is the word containing the label keyword (`crm`, `rg`, `cpf`, `cns`). When the label is followed on the same extracted line by its own qualifier or detached colon without digits (e.g. `["CRM", "SP"]`, `["CRM", ":"]`, `["RG", ":"]`) and the number is in the next block on the same row or on the line below (`column_below`), `any(i > last for i in line)` is `True`, so `row_after` and `column_below` are never checked and the number survives. Furthermore, `row_after(words, last)` in `detect.py:386` does not bound word gaps with `run_from` or check `column_below` before unbounded horizontal distance.

3. **AB-10 (medium)**: `BARE_LABEL` (`src/anonimizador/detect.py:314-321`, `358-360`) uses `.match(text.folded)` without allowing leading non-alphanumeric characters (`^[\W_]*`), so any leading border or punctuation character read by OCR at the start of a line or table cell (e.g. `"| Nome Sicrano"`, `"- Paciente Sicrano"`) causes `BARE_LABEL.match` to fail and the identifier to survive.

Everything is concise, well-verified, and strictly follows the required output format. Let's output the findings now.**ID:** AB-8
**Severity:** high
**File:line:** `src/anonimizador/detect.py:57-60`, `src/anonimizador/detect.py:309-313`, `src/anonimizador/detect.py:358-378`
**Description:** In AB-3, `"data nasc"` was added to `BARE_LABELS` (`detect.py:310`) but omitted from `COLON_LABELS` (`detect.py:57-60`). Because `"data nasc"` is in `BARE_LABELS` and not in `COLON_LABELS`:
1. When `Data Nasc:` or `Data Nasc.:` appears anywhere after the first word of a line (for example, `Sexo: F   Data Nasc.: 01/02/1980` or `CPF 123.456.789-09   Data Nasc: 01/02/1980`), `BARE_LABEL.match(text.folded)` is `None` (it only matches at index 0) and `COLON_LABEL.finditer(text.folded)` finds nothing.
2. When `Data Nasc : 01/02/1980` (with a detached colon) starts a line, `BARE_LABEL.match` matches `"data nasc"` while `COLON_LABEL.match` is `None`. `bare_value` then calls `beside`, which immediately stops at the detached `":"` token because `ends_field` sees `":"` in that word, returning `[]`.
3. When `Data Nasc:01/02/1980` or `Data Nasc.:01/02/1980` (with the date glued to the colon) starts a line, `BARE_LABEL.match` matches `"data nasc"` inside the glued token (since `:` satisfies `(?![^\W_])`), and `bare_value` only searches to the right of or below `last`, never taking `last` itself as the `COLON_LABEL` `glued` check does.
**Impact:** Under `--keep-dates` (`remove_dates=False`), a patient's date of birth written after `Data Nasc:` / `Data Nasc.:` / `Data Nasc :` in any of these positions is not detected and survives in the anonymized copy.
**Reproduction scenario or verifiable rationale:**
- `found(line("Sexo:", "F", "Data", "Nasc.:", "01/02/1980"), remove_dates=False)` returns `{"label": {"F"}}` — `"01/02/1980"` is missed.
- `found(line("Data", "Nasc", ":", "01/02/1980"), remove_dates=False)` returns `{}` — `"01/02/1980"` is missed.
- `found(line("Data", "Nasc:01/02/1980"), remove_dates=False)` returns `{}` — `"Nasc:01/02/1980"` is missed.
**Validation suggestion:** Add `"data nasc"` to `COLON_LABELS` in `src/anonimizador/detect.py`, and add regression tests in `tests/test_detect.py` covering `Data Nasc.:` mid-line, with a detached colon, and with a glued value under `remove_dates=False`.

**ID:** AB-9
**Severity:** high
**File:line:** `src/anonimizador/detect.py:370-389`
**Description:** Two defects in the cross-block / column-below fallback for `COLON_LABEL` and `NUMBER_LABEL` cause values under or beside their labels to be missed:
1. In `COLON_LABEL` (`detect.py:375-377`), a label that ends its extracted line evaluates `beside(words, last, float("inf")) or column_below(words, first, label_gap(words, first))`. Unlike `bare_value` (`detect.py:329-333`), which checks `beside(words, last, gap)` first, then `column_below(words, first, gap)`, and only then `beside(words, last, float("inf"))`, `COLON_LABEL` passes `reach = float("inf")` to `beside` *before* checking `column_below`. When a colon label is a box/column heading (e.g., `Dt. Nasc.:` or `Paciente:` at `x=0`) and another block on the same horizontal row far to the right holds a heading without a colon (e.g., `Sexo` or `Idade` at `x=200`), `beside(words, last, float("inf"))` takes that distant heading word, short-circuiting `or column_below(...)` and missing the actual value directly under the label.
2. In `NUMBER_LABEL` (`detect.py:380-388`), `last` is the word containing the label keyword (`crm`, `rg`, `cpf`, `cns`, etc.), and the fallback to `row_after` / `column_below` runs only `if not any(i > last for i in line):`. When the label is followed on the same extracted line by its own non-digit qualifier or detached colon (such as `["CRM", "SP"]`, `["CRM", ":"]`, or `["RG", ":"]`) and the number itself is on the next line under the label (`column_below`) or in another block on the same row (`row_after`), `any(i > last for i in line)` is `True` even though `number_value` on the current line returned `[]`. Neither `row_after` nor `column_below` is searched, and the number survives.
**Impact:** A date of birth or patient name directly under a `Dt. Nasc.:` / `Paciente:` heading survives whenever a colonless heading lies to its right on the same row in another block; and a council or document number under or beside `CRM SP`, `CRM :`, or `RG :` survives whenever the state or detached colon is a separate token on the label's line.
**Reproduction scenario or verifiable rationale:**
1. In `test_a_date_of_birth_under_its_label_in_another_block_is_taken` (`tests/test_detect.py:241-250`), changing `Word("Sexo:", (200, 0, 250, 15), ("ocr", 2, 1, 0))` to `Word("Sexo", (200, 0, 250, 15), ("ocr", 2, 1, 0))` causes `found(words, remove_dates=False)` to return `{"label": {"Sexo"}}`, leaving `"01/02/1980"` unredacted.
2. `found(placed(("CRM", 0), ("SP", 50), number=0) + placed(("123456", 0), number=1))` and `found([Word("RG", (0, 0, 30, 15), ("ocr", 1, 1, 0)), Word(":", (35, 0, 40, 15), ("ocr", 1, 1, 0)), Word("12.345.671-X", (100, 0, 220, 15), ("ocr", 2, 1, 0))])` both return `{}` because `words[1]` (`"SP"` / `":"`) makes `not any(i > last for i in line)` false.
**Validation suggestion:** In `COLON_LABEL` (`detect.py:375`), try `beside(words, last, gap) or column_below(words, first, gap) or beside(words, last, float("inf"))` (matching `bare_value`). In `NUMBER_LABEL` (`detect.py:382-388`), advance past any trailing `NUMBER_PART` words on `line` when no digit was found on `line` (and bound row searches so distant columns do not preempt `column_below`). Add tests for both cases in `tests/test_detect.py`.

**ID:** AB-10
**Severity:** medium
**File:line:** `src/anonimizador/detect.py:314-321`, `src/anonimizador/detect.py:358-360`
**Description:** `BARE_LABEL` is anchored at index 0 via `BARE_LABEL.match(text.folded)` with no leading `[\W_]*`. When OCR or `pdftotext` includes a leading table border, box edge, or punctuation character on the first token of a line (such as `"| Nome Sicrano"`, `"|"` followed by `"Nome"`, or `"- Paciente Sicrano"`), `text.folded` starts with `"|"` or `"-"`, so `BARE_LABEL.match(text.folded)` returns `None`.
**Impact:** On boxed medical forms where OCR reads a vertical cell border or bullet before a bare `Nome`, `Paciente`, `Mãe`, or date-of-birth label, the label is not matched and its value survives in the copy.
**Reproduction scenario or verifiable rationale:**
`found(line("|", "Nome", "Sicrano", "Inventado"))` and `found(line("|Nome", "Sicrano", "Inventado"))` both return `{}` because `text.folded` starts with `"|"`, which `BARE_LABEL.match` rejects.
**Validation suggestion:** Allow leading non-word characters before the label group in `BARE_LABEL` (e.g., `r"^[\W_]*(?P<label>...)"`), extract the label words from `m.start("label")` to `m.end("label")`, and test bare labels preceded by `"|"` in `tests/test_detect.py`.
