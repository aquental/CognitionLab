ID: CB-18
Severity: high
File:line: src/anonimizador/detect.py:367
Description: Number labels still search only their extracted line. The spatial recovery added for CB-12 and AB-2 does not apply to `RG`, `CRM`, or the other number labels.
Impact: Identifier values in another extraction block or beneath their label survive redaction.
Reproduction scenario or verifiable rationale: Put `RG:` and `12.345.671-X` in different extracted lines on the same physical row. With no terms, neither word produces a detection: the label has no subsequent word in its line, and the value matches no standalone pattern. This is the same extraction layout already acknowledged by CB-12.
Validation suggestion: Test number labels with values in separate blocks beside and beneath them, and require the value’s box to be detected and painted.

ID: CB-19
Severity: high
File:line: src/anonimizador/detect.py:78
Description: `NUMBER_PART` accepts a state abbreviation only as a separate word, rejecting council numbers whose state suffix shares the number’s word.
Impact: Council identifiers survive even when their label and value are correctly extracted on one line.
Reproduction scenario or verifiable rationale: For the words `CRM:` and `123456-SP`, `NUMBER_PART` rejects the second word because the digit branch permits only an optional `x` suffix. The label word contains no digits, and six digits match none of the standalone identifier patterns. Consequently, no box covers the number. `123456/SP` fails identically.
Validation suggestion: Add tests for attached state suffixes with hyphens and slashes, alongside the existing tests for separately tokenized state abbreviations.

ID: CB-20
Severity: medium
File:line: src/anonimizador/batch.py:178
Description: The AB-5 fix checks fabricated filenames instead of the actual output filenames. Git ignore decisions for the probes do not establish that the copies are ignored.
Impact: Unreviewed copies can be written to paths Git can commit despite the output-location guard.
Reproduction scenario or verifiable rationale: Ignore `*-anonimizer.md`, `*.txt`, `*.json`, `exam-000000000000.pdf`, and `exam-000000000000.png`. Every probe passes, but an actual copy with a different digest remains unignored. Negated ignore rules targeting particular actual filenames create the same problem.
Validation suggestion: Check every actual destination filename. Add a regression where probes are ignored but an actual copy is not, and require refusal before writing outputs.

ID: CB-21
Severity: medium
File:line: src/anonimizador/locate.py:170
Description: Opening rendered PDF pages is outside the handler for Pillow’s `DecompressionBombError`.
Impact: A supported DPI setting can terminate the batch with an uncaught traceback instead of an error row and exit status 2.
Reproduction scenario or verifiable rationale: A letter-sized page rendered at 1500 DPI contains approximately 210 million pixels, exceeding Pillow’s default decompression-bomb error threshold. The CLI permits this DPI. `Image.open(image)` raises an exception that neither `anonymize` nor `run_batch_command` catches. The corresponding image-original path explicitly handles this exception.
Validation suggestion: Lower `Image.MAX_IMAGE_PIXELS` in a regression test and process a synthetic PDF whose rendered page exceeds the threshold. Require a sanitized error, exit status 2, and continued processing of other originals.

ID: CB-22
Severity: medium
File:line: src/anonimizador/locate.py:190
Description: The 16-bit contrast fix runs after transparency flattening. A 16-bit grayscale PNG containing a transparency entry is converted to RGBA before its samples are scaled.
Impact: Distinct opaque grayscale values above 255 can collapse to white, erasing clinical content before detection and producing an apparently clean but damaged copy.
Reproduction scenario or verifiable rationale: Use a 16-bit PNG with background samples of 60000, text samples of 2000, and a `tRNS` transparency value of 0. `flattened` converts the image to 8-bit RGBA first; afterward, `eight_bit` sees RGBA and skips scaling. The existing contrast test lacks transparency and therefore does not exercise this path.
Validation suggestion: Extend the 16-bit fixture with a transparency entry and transparent pixels. Require preserved contrast between opaque text and background, plus correct white compositing of transparent pixels.