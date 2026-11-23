I'll review the stage-0 fixes against the current code and tests, then look for any new defect that would report a dirty or unreadable file as clean.G4-5, G4-6, and G0-1 through G0-6 are fixed. Three new defects are below.

**G4-5 is fixed.** `channels.run` raises `ToolError` for a non-zero exit, a timeout, or a tool that cannot start. The timeout is raised `from None`, so the path inside `TimeoutExpired` is not chained. Each channel records its own error, the file status is `ERROR`, and any unread channel makes the process exit 2. Missing OCR language data is refused before any file is read. An `OSError` is reported with `strerror` only.

**G4-6 is fixed** for a real subdirectory. `collect` names a walked file with `os.path.relpath` to the directory argument, and that string is the `name` channel. `test_a_name_in_a_directory_is_searched_and_never_printed` expects `name=1` for `BELTRANO DA SILVA/lab-001.pdf` and checks that the name is absent from the report.

**G0-1 is fixed.** `DIGIT_RUN` joins digit runs separated by at most three non-alphanumeric characters, so parentheses, newlines, tabs, form feeds, carriage returns, and non-breaking spaces stay inside the number. The regression test covers those separators, a changed final digit, and a six-character gap.

**G0-2 is fixed.** A term with at least five digits is matched by those digits, including `CPF 123.456.789-09` and `Prontuario 4821-77`.

**G0-3 is fixed** for an unlistable directory and for following a symlink to files that are not otherwise in the tree. `os.walk` uses `onerror` and `followlinks`, and a repeated real directory is not descended into, so a link loop ends. The unlistable-directory failure names the argument position and `strerror` only, and the run exits 2.

**G0-4 is fixed.** A non-UTF-8 terms file raises `TermsError` with a fixed message, `from None`, and the CLI maps that to exit 2 without a traceback.

**G0-5 is fixed.** `check_report_privacy` fails through `pytest.fail(..., pytrace=False)` with a count or line positions only. The real-exam test uses that helper, and the unit test checks the failure text.

**G0-6 is fixed.** `ocr` and `images` are counted on the parts joined by a newline, so every occurrence counts and a long term split across a page break matches. `raw` and the other single-document channels still take the maximum over alternative readings.

## G0-7

**Severity:** medium

**File:line:** `src/anonimizador/verify.py:106`

**Description:** A directory is searched under only the first relative path whose real path is new. On a later symlink to that same directory, `walk` clears `dirs` and `continue`s without yielding. The files were already collected under the other path, so the second directory name never becomes a `name` channel.

**Impact:** An identifying directory name can be absent from every searched name while the file content is clean. The run then prints `ok` and exits 0. `lab` sorts before `maria`, so a neutral link that sorts first keeps the neutral name and drops the identifying one.

**Reproduction:** `root/lab/clean.pdf` contains none of the terms. `root/maria` is a symlink to `root/lab`. The terms file contains `Maria`. `anonimizador verify --terms … root` yields only `lab/clean.pdf`, reports `ok`, and exits 0. The same mechanism drops the name of `loop` in `test_symlinked_directories_are_followed_once`, which only checks that the loop does not add a second copy of the file.

**Validation:** That layout must exit 1, report a `name` hit for the term, and must not print `maria` or `Maria`. A link loop must still finish. A mutant that keeps the `continue` without searching the second relative path fails the test.

## G0-8

**Severity:** low

**File:line:** `src/anonimizador/channels.py:130`

**Description:** Metadata bytes are decoded only as UTF-8 (invalid bytes dropped) and Latin-1. UTF-16 text, the usual encoding of EXIF and PNG `eXIf` Unicode, becomes letters separated by NUL. `Term.count` then uses whole words for a term shorter than 6 letters. NUL is not a word character, so `Silva` is searched as five single letters. `verify.py:175` does not search that short term in `raw` either.

**Impact:** A 4- or 5-letter term that exists only as UTF-16 metadata (`Maria`, `Silva`, `[redacted]`) has count 0 in every channel. If nothing else hits, the file is `ok` and the process exits 0. A longer name still matches, because `compact` deletes the NULs. ASCII metadata with a single NUL between the tag and the value still matches, which is what `test_png_text_metadata_is_read_plain_and_compressed` covers. Page OCR still sees a name that is painted on the image.

**Reproduction:** `Term("Silva").count("S\x00i\x00l\x00v\x00a")` is 0, and the same for UTF-16-LE. Build a JPEG whose only identifier is an APP1 segment containing those bytes and pass it to `image_metadata`. The metadata count stays 0. The same file with OCR disabled is `ok` and exits 0.

**Validation:** `image_metadata` on that JPEG must produce a string on which `Term("Silva").count` is 1, and a changed final letter must stay 0. The existing ASCII PNG metadata test must stay 1.

## G0-9

**Severity:** low

**File:line:** `src/anonimizador/channels.py:43`

**Description:** `--timeout` is `type=int` with no lower bound. `subprocess.run` rejects a negative timeout with `ValueError: timeout must be non-negative`. `run` catches `TimeoutExpired` and `OSError` only, and `run_verify` does not catch `ValueError`.

**Impact:** `anonimizador verify --timeout -1 …` prints a traceback and exits 1. Exit 1 means identifiers were found. The message does not contain the path. A usage error is specified as exit 2 and a single error line.

**Reproduction:** A UTF-8 terms file outside the repo, any PDF, and `--no-ocr --timeout -1`. The process exits 1, prints `Traceback`, and does not print `anonimizador verify:`.

**Validation:** That command must exit 2, print one line on stderr, and print no traceback. A mutant that leaves `type=int` unchanged fails it.
