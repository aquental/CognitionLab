I'll review the current stage-0 tree against the claimed G0-10 through G0-13 fixes, then look for any new way a hit or a failed read can be reported as clean.The review stays read-only. I'll read the verifier, matching, and the tests that claim to close G0-10 through G0-13.G4-5, G4-6, and G0-1 through G0-13 hold. G0-10 is fixed for terms of 4 or more characters, and the exclusion of terms of 3 or fewer matches the code. Five new defects are below.

**G4-5 still holds.** `channels.run` raises `ToolError` for a non-zero exit, a timeout, or a tool that cannot start. The timeout is raised `from None`. Each channel records its own error, the file status is `ERROR`, and any unread channel makes the process exit 2. Missing OCR language data is refused before any file is read. An `OSError` is reported with `strerror` only.

**G4-6 still holds.** A file under a directory argument is named with `os.path.relpath` to that directory, and that string is the `name` channel.

**G0-1 through G0-9 still hold.** Digit runs use a gap of 3. A term with 5 or more digits is matched by those digits. Directory listing errors abort the walk. A non-UTF-8 terms file raises `TermsError` with a fixed message. The privacy helper fails through `pytest.fail(..., pytrace=False)`. OCR pages and embedded images are counted on the joined text. Later symlink paths are kept as aliases. UTF-16 metadata is decoded with one character split off each end. `--timeout` and `--dpi` use `positive_int`.

**G0-10 is fixed for terms of 4 or more, and the refutation for 3 or fewer stands.** `RAW_MIN_LENGTH` is 4. `check_file` leaves the raw count in place when `searchable_in_binary` is true, and forces 0 only below that. The regression goes through `raw_texts` and `check_file` for UTF-16 bytes, a PDF literal, and an inflated UTF-16 stream. Terms of 3 letters stay out of raw, as `SPEC.md` describes. One documentation line was left on the old rule: the channel table in `README.md:47` still says raw skips terms shorter than 6 letters. The paragraph below it, and `SPEC.md`, say 4.

**G0-11 is fixed.** Any listed entry that does not exist raises `BrokenLinkError`. `collect` reports `cannot read everything under argument N (a symbolic link leads nowhere)` and exits 2. The message has no path. A missing target and a self-loop are both covered.

**G0-12 is fixed.** A fill byte `0xFF` advances one byte and is not given a length. `TEM` and `RSTn` advance two bytes. The EXIF segment after those bytes is still returned.

**G0-13 is fixed.** Files are keyed by real path. The first name is kept, and every later name from any argument is stored as an alias and searched on the `name` channel.

## G0-14

**Severity:** high

**File:line:** `src/anonimizador/channels.py:171`

**Description:** `raw_texts` decodes file bytes as UTF-8, Latin-1, zlib streams, and UTF-16 runs. It does not decode a PDF text string. A PDF text string is either a hexadecimal string, often UTF-16BE with a `FEFF` BOM (`<FEFF005A0075006C0065006F>` for Zuleo), or a literal with octal escapes (`(Zule\347o)`). Those bytes contain neither the letters nor a NUL-separated UTF-16 run, so `Term.count` on every view `raw_texts` returns is 0. `pdfinfo` and `pdftotext` decode the string, and they read only the objects the current trailer and page tree still reference.

**Impact:** The leftover an incremental update is supposed to catch can be invisible. `docs/ANONYMIZATION.md` says exiftool clears an info dictionary by appending an update, and the old dictionary stays until a rewrite. The current page and `pdfinfo` are then clean. An old `/Author` or an old content stream stored as UTF-16 hex or as octal escapes — the usual PDF forms for a non-ASCII name — has count 0 in every channel. The file is `ok` and the process exits 0. An ASCII literal `(Zuleo)` in the same place is still found. A name that is still in the current page is still found by `pdftotext`.

**Reproduction:** `max(Term("Zuleo").count(t) for t in raw_texts(b"<< /Author <FEFF005A0075006C0065006F> >>"))` is 0. The same for a literal whose only non-ASCII letter is an octal escape. Build a one-page PDF whose live info dictionary and page text are empty, and whose first-generation info object is still in the file as that hex string. `pdfinfo` shows no author. `verify --no-ocr` prints `[1] ok` and exits 0. The same file with `(Zuleo)` in that old object reports `raw=1`.

**Validation:** That two-generation fixture must report `raw=1` and exit 1 for the hex form and for one octal escape. A changed final letter stays 0. The current ASCII literal test stays `raw=1`.

## G0-15

**Severity:** medium

**File:line:** `src/anonimizador/verify.py:129`

**Description:** `scan` drops every directory and file whose name starts with `.` before the broken-link check and before the suffix check. `os.walk` is told not to descend into those directories. No error is recorded. `SPEC.md` R2 says directories are walked recursively, and that a directory that cannot be listed or a link that leads nowhere stops the run.

**Impact:** A clean PDF beside a hidden directory of identifying files is `[1] ok` with exit 0. The same happens for a hidden PDF whose name or text contains a listed term. A hidden dangling link is also dropped, so the G0-11 failure does not apply to it. AppleDouble files (`._lab.pdf`) are hidden too; opening those as PDFs would make `pdfinfo` fail and the run exit 2. That is a reason to ignore the `._` prefix, and it leaves every other hidden directory and hidden PDF skipped.

**Reproduction:** `root/clean.pdf` contains none of the terms. `root/.Zuleo/lab.pdf` is a PDF, or `root/.Zuleo` is only a directory name and `lab.pdf` is clean. The terms file contains `Zuleo`. `anonimizador verify --no-ocr --terms … root` yields one file named `clean.pdf`, prints `[1] ok`, and exits 0.

**Validation:** That layout must exit 1 with `name=1`, or exit 2 if a hidden corpus entry cannot be read. The output must not contain `Zuleo`. A visible symlink to a real directory must still be followed. A mutant that keeps `startswith(".")` fails it.

## G0-16

**Severity:** medium

**File:line:** `src/anonimizador/channels.py:26`

**Description:** Pattern hints are counted only on `text`, `ocr`, `images`, and `metadata`. The `name` channel is searched for listed terms and is not passed to `count_patterns`. `--strict-patterns` fails the run only when that hint map is non-empty.

**Impact:** A CPF, phone number, or e-mail that appears only in the file name produces no pattern line. With or without `--strict-patterns`, a file whose content is clean is `ok` and the process exits 0. The filename check in `docs/ANONYMIZATION.md` is the case: the identifier is the name of the file. A listed term still counts on the `name` channel. An unlisted pattern does not.

**Reproduction:** A PDF named `123.456.789-09.pdf` whose page text is empty. The CPF pattern matches that name (`09` is followed by `.`, which is not a digit). `anonimizador verify --no-ocr --strict-patterns` that file, with no terms file, prints `[1] ok` and exits 0. The same digits in the page text exit 1, which is what `test_strict_patterns_fail_on_a_pattern_alone` covers.

**Validation:** That file must exit 1 under `--strict-patterns` and must print `cpf=1` without printing the digits. A name with one digit changed stays `ok`. The page-text test stays exit 1.

## G0-17

**Severity:** low

**File:line:** `src/anonimizador/channels.py:250`

**Description:** An embedded image is OCR'd only when `min(width, height) >= 32`. Both sides must clear `MIN_OCR_SIDE`. A wide image 20 pixels tall is dropped, and the images channel stays empty with no error. The comment describes icons and masks, which are small on both sides.

**Impact:** A name painted only in a short strip (a header bar, a label) is absent from `images`. If a box covers that strip on the rendered page, page OCR does not see it either, and the pixels are not letters in `raw`. The file is `ok` and the process exits 0. A square icon smaller than 32 pixels on both sides is still skipped. An image that is visible on the page is still found by page OCR.

**Reproduction:** A one-page PDF whose only identifier is the word Zuleo drawn in an embedded PNG of 400×20, with a filled rectangle over that region in the content stream and no text-layer copy of the word. `pdfimages` extracts the PNG. `min(400, 20) < 32`, so `texts["images"]` is empty. `verify` prints `[1] ok` and exits 0. The same image at 400×40 reports `images=1`.

**Validation:** The 400×20 fixture must report `images=1` and exit 1. A 16×16 icon may stay skipped. A mutant that keeps `min(...) >= 32` fails the wide case.

## G0-18

**Severity:** low

**File:line:** `src/anonimizador/terms.py:117`

**Description:** `count_patterns` takes the largest count of each pattern over separate strings. `check_file` passes each OCR page and each embedded image as its own string. G0-6 joined those parts for listed terms. Patterns still see one page at a time. The CPF, phone, CEP, and date patterns do not allow a line break inside the number.

**Impact:** A number split by a page break has pattern count 0. If it is not also a listed term, `--strict-patterns` prints `ok` and exits 0. A complete pattern on any one page still fails that run. Two complete hits on two pages are reported as 1.

**Reproduction:** `count_patterns(["CPF 123.456.789-", "09"])` is empty. The same digits in one string give `cpf=1`. With OCR texts `["123.456.789-", "09"]`, no other text, and `--strict-patterns`, `FileResult.status` is `ok`.

**Validation:** Those two page strings must yield `cpf=1` and exit 1 under `--strict-patterns`. A changed final digit stays 0. Joining pages for the pattern search can follow the term path; alternative readings of one page should still contribute a single count.
