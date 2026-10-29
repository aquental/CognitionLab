I'll review stage 0 against the spec and the two carried-over fixes, reading the verifier and tests only.G4-5 and G4-6 are fixed. Six new defects are below.

**G4-5 is fixed.** `channels.run` raises `ToolError` for a non-zero exit, a timeout, or a tool that cannot start. The message is the tool name and the reason. The timeout exception is discarded with `from None`, so the command and the path in `TimeoutExpired` are not chained. Each channel records its own error, the file status is `ERROR`, and any unread channel makes the process exit 2. Missing OCR language data is refused before any file is read. An `OSError` is reported with `strerror` only. The tests in `tests/test_cli.py` cover a failing tesseract, a hung tesseract, a `pdftoppm` that writes nothing, failing `pdfinfo` and `pdftotext`, and missing `por` data.

**G4-6 is fixed.** A file found under a directory is named with `os.path.relpath` to that directory, directories included (`verify.py:100`), and that string is the `name` channel (`channels.py:181`). `test_a_name_in_a_directory_is_searched_and_never_printed` expects `name=1` for `BELTRANO DA SILVA/lab-001.pdf` and checks that the name is absent from the report.

## G0-1

**Severity:** high

**File:line:** `src/anonimizador/terms.py:58` (applied at line 90)

**Description:** A numeric term is counted only inside digit runs. The run may contain `.`, `-`, `/`, and a normal space. A parenthesis, line break, tab, carriage return, form feed, or non-breaking space ends the run, and the full digit string must then sit inside one piece.

**Impact:** A listed phone, CPF, or protocol number, written as it appears in a Brazilian document or in OCR, has count 0 in every channel. If nothing else hits, the file is `ok` and the process exits 0. A CPF broken by a newline also misses the CPF pattern, so there is no hint. `(2024) 123456` has no pattern hit either. A parenthesized mobile can still show `phone=1`; that hint does not fail the run unless `--strict-patterns` is set, and the listed term still has no count.

**Reproduction:** `Term.count` and `count_patterns` on these strings:

- `Term("(11) 98765-4321").count("(11) 98765-4321")` is 0 (groups `11` and `987654321`).
- `Term("11987654321").count("(11) 98765-4321")` is 0. `count_patterns` reports `phone=1` only.
- `Term("2024.123456").count("(2024) 123456")` is 0, and `count_patterns` is empty.
- `Term("123.456.789-09").count("123.456.789-\n09")` is 0, and the same for `\u00a0`, `\t`, `\f`, and `\r`. `count_patterns` on the newline form is empty.
- `Term("123.456.789-09").count("123.456.789-09")` is 1, so `.` and `-` do join.

Every channel uses `Term.count`, including text from `pdftotext` (form feeds between pages) and OCR.

**Validation:** Unit tests that those pairs count 1, and that a different final digit still counts 0. A synthetic PDF or image whose only identifier is `(11) 98765-4321` or a CPF broken across a line must exit 1 and report that term.

## G0-2

**Severity:** medium

**File:line:** `src/anonimizador/terms.py:75`

**Description:** A term is numeric only when it has at least 5 digits and those digits are at least 80% of its compacted characters. `compact` counts letters and digits together, so `CPF 123.456.789-09` is 11 digits out of 14 characters (0.786). It is stored as the compact string `cpf12345678909`, and the label letters must sit directly against the digits.

**Impact:** The document's bare number does not count. The file can be `ok` with exit 0. A CPF-shaped number can still show `cpf=1`, which does not fail the default run. A labeled record number that is not a CPF, CNS, phone, CEP, or date has no hint at all.

**Reproduction:**

- `Term("CPF 123.456.789-09").numeric` is false.
- `.count("123.456.789-09")` is 0.
- `.count("Cadastro 123.456.789-09")` is 0.
- `.count("CPF: 123.456.789-09")` is 1.

**Validation:** Assert those three counts. Assert that a labeled record number matches the digits alone.

## G0-3

**Severity:** high

**File:line:** `src/anonimizador/verify.py:95`

**Description:** `collect` uses `os.walk` with no `onerror`. `os.walk` ignores `scandir` failures, and it does not follow a directory symlink. Neither skip is recorded.

**Impact:** One readable clean file plus an unreadable subdirectory of identifying PDFs is reported `ok` with exit 0. The same happens when the identifying files are behind a nested directory symlink and the walked tree also contains a clean file. A file that cannot be opened is already `ERROR` (`verify.py:126`). A directory that cannot be listed is omitted. When the only argument is a directory that cannot be listed, the process exits 2 with "no PDF, PNG, or JPEG file to check".

**Reproduction:** `root/clean.pdf` contains none of the listed terms. `root/locked/` is mode `0o000` and contains a PDF whose name or text contains a listed term. `anonimizador verify --terms … root` exits 0. `str(OSError)` for the locked directory includes the path; only `strerror` is safe to print.

**Validation:** That layout must exit 2, must not print `ok`, and must not print `locked` or the term. Restore the mode in a `finally`.

## G0-4

**Severity:** medium

**File:line:** `src/anonimizador/terms.py:136` and `src/anonimizador/cli.py:73`

**Description:** `load_terms` opens the terms file as strict UTF-8. `UnicodeDecodeError` is a `ValueError`, so it matches neither handler in `run_verify`.

**Impact:** A terms file saved as Latin-1 or Windows-1252, a normal encoding for a hand-edited Portuguese list, crashes with a traceback and exit status 1. Exit 1 means identifiers were found. The exception text is the byte and the position. For `Fulano` plus `ç` (`0xE7`), it is `utf-8 codec can't decode byte 0xe7 in position 7: invalid continuation byte`. The term and the path are absent from that text.

**Reproduction:** A terms file whose bytes are `Fulano \xe7\n`, kept outside the repo, then `verify --terms` that file and any PDF. The process exits 1 and prints a traceback. No `anonimizador verify:` line is printed, because the exception leaves `run_verify` before the report.

**Validation:** That file must exit 2, print one fixed line on stderr, and print no traceback.

## G0-5

**Severity:** medium

**File:line:** `tests/test_sample_exams.py:49` and `tests/test_sample_exams.py:55`

**Description:** The real-exam test puts file-name words in `leaked` and asserts `not leaked`. It also asserts that each report line matches a shape. Pytest rewrites both asserts. On failure it prints the operand values. The messages themselves contain only a count and a line index.

**Impact:** A failing local run prints file-name words. `CLAUDE.md` forbids those words in anything sent to a reviewer, and the test docstring says the test never prints a file name. A passing run prints nothing. Assertion rewriting is on: `addopts` is `-ra` only.

**Reproduction:** A failing `assert not leaked, "N file-name word(s) reached the report"` is reported as `assert not ['…words…']` plus the count. A failing shape check prints the line that failed.

**Validation:** Fail the check on a synthetic stand-in word and assert that the pytest output contains the count and does not contain the word. Use `if leaked: pytest.fail(f"{len(leaked)} file-name word(s) reached the report")` so the words are not an assertion operand. Fail the shape check with the line index only.

## G0-6

**Severity:** low

**File:line:** `src/anonimizador/verify.py:143`

**Description:** Each channel count is the maximum of `term.count` over that channel's strings. The maximum is appropriate for `raw` and `metadata`, where the strings are UTF-8, Latin-1, and UTF-16 views of the same bytes. OCR pages and embedded images are separate texts. Two pages that each contain the term once are reported as `ocr=1`. A long term split across two page strings matches neither string, so the OCR count is 0. On a scan, the text layer and the raw bytes do not contain that term either.

**Impact:** OCR and embedded-image counts are lower than the number of occurrences. A term that occurs wholly on one page still makes the file `FOUND`. A long term divided by a page break can be missed on a scanned file, and that file can be `ok`. The covered-name fixture is one page, so the end-to-end test does not see this.

**Reproduction:** `texts["ocr"] = ["Fulano Ficticio", "Fulano Ficticio"]` stores `ocr=1`. `texts["ocr"] = ["Fulano", "Ficticio"]` with term `Fulano Ficticio` stores 0.

**Validation:** A two-page fixture with the term on both pages expects `ocr=2`. Sum per page and per image. Keep a single count for alternative decodings of the same bytes; summing those views would turn the covered-name fixture's `raw=2` into 4.
