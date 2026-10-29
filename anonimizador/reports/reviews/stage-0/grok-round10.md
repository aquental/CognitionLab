I'll review the redesigned content-stream reader against the current tree and check whether G0-32 through G0-36, plus the carried-over fixes, still hold.The review stays read-only. Next I’ll read the content-stream reader and the tests that claim to close G0-32 through G0-36.The five fixes are in the tree. I’ll probe layouts the new tests don’t cover, especially word spaces whose array marker sits past the 256-byte gap.G4-5 and G4-6 still hold. G0-32, G0-33, G0-34, G0-35, and G0-36 hold for the layouts in their tests: a `Td` then a `Tm` counts 1, a string with one character above Latin-1 counts 1, a kerned CPF separated by `Td` hints `cpf=1` inside a real one-page PDF, and a `Tm` after a long first gap counts 1. The joined, split, and mixed readings are what make those five pass.

The design is not conservative for every boundary. Two layouts still drop a short name.

## G0-37

**Severity:** high

**File:line:** `src/anonimizador/channels.py:296`

**Description:** A gap longer than 256 bytes returns a hard break and clears the TJ-array state without reading the gap. The `[` that opens the array is in that gap, so a later adjustment of -200 or less is an ordinary number. The glyph strings stay glued. Small kerning still forms each word, and the word space disappears. `Td` and `Tm` sit in the short gaps between strings, so the new tests still pass.

**Impact:** A 4- or 5-letter name drawn in a TJ array and separated from the next word by a word-space adjustment counts 0 in raw bytes. Two short names in that array both count 0. A term of 6 letters or more still matches through its compact form. On a leftover uncompressed stream the text layer is the clean page, so the file is `ok` and the process exits 0. The same bytes as a page’s content stream are read by `pdftotext -layout` as the short name, and that live page is still `FOUND` through the text channel. A kerned CPF closed by the same adjustment and followed by another number gives no pattern hint from the PDF strings. A listed number still matches, because the glued digits contain it. A Flate stream whose own bytes start at `BT [` still counts 1.

**Reproduction:** Build a one-page PDF whose page content is `BT /F1 12 Tf 72 700 Td (Hemoglobina 13,5 g/dL) Tj ET`, then append an unreferenced object `9 0 obj << /Length … >> stream` containing `BT [(Z) -2 (u) -2 (l) -2 (e) -2 (o) -300 (A) -2 (s) -2 (s) -2 (u) -2 (n) -2 (t) -2 (o)] TJ ET`. The gap from the page’s last string to `[` is over 256 bytes. `pdftotext -layout` prints the clean page. The raw count of `Zuleo` is 0 and of `Zulea` is 0. `verify --no-ocr` prints `[1] ok` and exits 0. Those operators alone, and the same array compressed as its own short stream, count 1. The same page content with `-300` replaced by `30 0 Td` counts 1.

**Validation:** The appended object must count 1 for `Zuleo` and 0 for `Zulea`, and the kerned CPF form with `-300` then `(99)` must hint `cpf=1`. A mutant that returns on a long gap without keeping the array state fails the appended object and still passes `test_any_move_is_ambiguous_without_state`.

## G0-38

**Severity:** high

**File:line:** `src/anonimizador/channels.py:309`

**Description:** A move becomes ambiguous only while the gap is still a continuation. `ET`, `BT`, `q`, `Q`, `gs`, `BMC`, `BDC`, `EMC`, `w`, and any other operator outside the text-state list set a hard break, and a later `Td` or `Tm` in that same gap stays a hard break. The split reading is the only one. Mixed readings do not cross it. This happens inside a short stream, so the 256-byte rule is not involved.

**Impact:** A 4- or 5-letter name whose pieces are two text objects on one baseline, or two strings with only `q`, `gs`, or a marked-content operator between them, counts 0 in raw bytes. `pdftotext -layout` reads each of those as one word. A leftover Flate stream of those operators, beside a clean page, is `ok` and the process exits 0. On a live page the text channel still counts 1. A changed letter stays 0. A term of 6 letters or more still matches through its compact form.

**Reproduction:** `BT /F1 12 Tf 72 700 Td (Zu) Tj ET BT /F1 12 Tf 84 700 Td (leo) Tj ET` has raw count 0, including inside a Flate stream. The same content as the page of a one-page PDF is one word in `pdftotext -layout`. At x=88 that PDF is two words. `BT (Zu) Tj q (leo) Tj Q ET` and `BT (Zu) Tj /Span BMC (leo) Tj EMC ET` are one word in `pdftotext -layout` and raw count 0. Append either stream, compressed, as an unreferenced object on a clean page. `verify --no-ocr` prints `[1] ok` and exits 0.

**Validation:** The x=84 stream, the `q` stream, and the `BMC` stream must count 1 for `Zuleo` and 0 for `Zulea`, as bare bytes and as an unreferenced Flate stream. A mutant that keeps a hard break once any other operator has been seen fails all three and still passes `test_text_state_and_same_baseline_keep_a_word_whole`.
