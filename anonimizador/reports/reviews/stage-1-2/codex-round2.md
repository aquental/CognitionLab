ID: CB-12
Severity: high
File:line: src/anonimizador/detect.py:262
Description: A bare name label cannot find a name beside it when the extractor assigns the label and value different line IDs.
Impact: Names remain visible despite being correctly recognized and positioned by OCR or native extraction.
Reproduction scenario or verifiable rationale: Give `Nome` box `(0, 0, 40, 15)` on one line and `Sicrano Inventado` starting at `(200, 0)` on another line of the same page. Both `run_after` calls search only the label’s line; `column_below` excludes the name because its top is not below the label’s midpoint. No name box is returned. Separate blocks on the same horizontal row are valid extractor output. The existing wide-gap test assigns everything one synthetic line ID.
Validation suggestion: Test adjacent label/value blocks with different line IDs, including an actual extraction fixture. Search spatially beside the label as well as within its extracted line.

ID: CB-13
Severity: high
File:line: src/anonimizador/locate.py:167
Description: Malformed EXIF can raise a `SyntaxError` containing original metadata bytes, outside the sanitized exception handlers.
Impact: Document content can leak through a traceback, and the batch terminates without its promised error row and exit status.
Reproduction scenario or verifiable rationale: A structurally valid PNG can contain an `eXIf` chunk with an invalid TIFF header such as `b"Fulano!!"`. `ImageOps.exif_transpose` reads EXIF through Pillow’s TIFF parser, which raises `SyntaxError` identifying the invalid header bytes. Neither `anonymize` nor `run_batch_command` catches `SyntaxError`.
Validation suggestion: Run a synthetic PNG with valid chunk checksums and malformed EXIF containing a sentinel. Require exit 2, a sanitized error row, and no sentinel or traceback in either output stream or logs.

ID: CB-14
Severity: medium
File:line: src/anonimizador/locate.py:78
Description: Malformed TSV without the required headers is silently accepted as an empty OCR reading. The CB-9 resolution therefore does not cover this malformed-output case.
Impact: Detection proceeds without OCR boxes instead of reporting an unreadable step; label-only identifiers in image originals can survive.
Reproduction scenario or verifiable rationale: `tsv_words("garbage\n")` returns an empty list. With additional arbitrary rows, missing `level` and `text` fields still cause every row to be skipped. No exception reaches the new parsing handler. A stub that corrupts only Tesseract’s TSV output therefore permits redaction to continue.
Validation suggestion: Validate required TSV headers and distinguish valid empty-page output from invalid output. Test a TSV-only malfunction against an image containing a labelled name absent from the terms list; require an error rather than a clean copy.

ID: CB-15
Severity: medium
File:line: src/anonimizador/locate.py:169
Description: Converting an image with transparency directly to RGB discards alpha instead of compositing the displayed image.
Impact: Visible document content can disappear or change before detection and redaction, while verification accepts the altered copy.
Reproduction scenario or verifiable rationale: Consider an RGBA PNG with black RGB samples everywhere, transparent background pixels, and opaque black text. It displays readable black text on white, but `shown.convert("RGB")` produces an entirely black image. Clinical content and identifiers disappear together without any redaction boxes.
Validation suggestion: Composite transparency onto an explicit background before dropping alpha. Test an RGBA document with transparent background, a labelled identifier, and unrelated clinical text; require the clinical text to remain readable.

ID: CB-16
Severity: high
File:line: src/anonimizador/batch.py:170
Description: The index-location guard fails open when Git cannot determine repository membership.
Impact: An index naming originals can be written into a directory that Git can subsequently commit.
Reproduction scenario or verifiable rationale: `committable` returns `False` for every unsuccessful `git rev-parse`, including repository ownership errors or an invalid inherited `GIT_DIR`. Both lexical and real-path checks can consequently return `False` for an ordinary, unignored directory inside a repository. `start` then writes there without another check.
Validation suggestion: Test Git inspection failures separately from a confirmed location outside a repository. Refuse index creation when repository membership cannot be established, using a sanitized error.

ID: CB-17
Severity: medium
File:line: src/anonimizador/detect.py:285
Description: The next-line fallback tests whether a value was found, rather than whether the label actually ends its line. This contradicts the CB-7 resolution.
Impact: A blank identifier field followed by another field can cause unrelated clinical content on the next line to be redacted.
Reproduction scenario or verifiable rationale: For `Paciente: Idade: 45` followed by `Hemoglobina 13,5`, `colon_value` stops immediately at `Idade:` and returns nothing. The fallback then takes both words of the clinical line, although `Paciente:` did not end its line.
Validation suggestion: Add this blank-field regression and require no clinical boxes. Permit the next-line fallback only when the matched label actually ends the current line.