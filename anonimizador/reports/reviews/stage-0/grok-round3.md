I'll confirm G0-7 through G0-9 against the current verifier, then review the whole stage for any new defect that would hide a hit or report a failure as clean.G4-5, G4-6, and G0-1 through G0-9 are fixed. Four new defects are below.

**G4-5 is fixed.** `channels.run` raises `ToolError` for a non-zero exit, a timeout, or a tool that cannot start. The timeout is raised `from None`. Each channel records its own error, the file status is `ERROR`, and any unread channel makes the process exit 2. Missing OCR language data is refused before any file is read. An `OSError` is reported with `strerror` only.

**G4-6 is fixed.** A file under a directory argument is named with `os.path.relpath` to that directory, and that string is the `name` channel.

**G0-1 is fixed.** `DIGIT_RUN` joins digit runs separated by at most three non-alphanumeric characters.

**G0-2 is fixed.** A term with at least five digits is matched by those digits.

**G0-3 is fixed** for a directory `os.walk` cannot list, and for a symlink whose target is a real directory. `onerror` aborts the walk, and `followlinks` is set.

**G0-4 is fixed.** A non-UTF-8 terms file raises `TermsError` with a fixed message, `from None`, and the CLI maps that to exit 2.

**G0-5 is fixed.** `check_report_privacy` fails through `pytest.fail(..., pytrace=False)` with a count or line positions only.

**G0-6 is fixed.** `ocr` and `images` are counted on the parts joined by a newline. Other channels still take the maximum over alternative readings.

**G0-7 is fixed.** On a later visit to a real directory, `scan` records that relative path and rewrites it onto every file already collected under the first path. `test_a_second_link_to_a_directory_keeps_its_name` gets `name=1` for `sicrano` when `laudos/clean.pdf` was the first path, and the loop test records `loop/...` aliases.

**G0-8 is fixed** for a metadata segment `image_metadata` actually returns. `utf16_views` decodes both byte orders, keeps an unsplit view, and also splits one glued character off each end. A normal JPEG (`FF D8 FF E1`, including an EXIF `UNICODE` prefix) gives `metadata=1` for a 5-letter term. The holes below are outside that test.

**G0-9 is fixed.** `--timeout` and `--dpi` use `positive_int`. `-1`, `0`, and a non-integer become argparse's usage error, exit 2, with no traceback.

## G0-10

**Severity:** medium

**File:line:** `src/anonimizador/verify.py:195`

**Description:** `check_file` forces the `raw` count to 0 for every term with `searchable_in_binary` false, which is every non-numeric term shorter than 6 letters. `raw_texts` does put UTF-16 runs and inflated streams into that channel, and `Term.count` on those strings is 1, but the result is thrown away. `SPEC.md` R2 says leftover streams are covered and that a short UTF-16 term is a whole word.

**Impact:** A 4- or 5-letter term that survives only in raw bytes is `ok` and the process exits 0. That is the usual leftover after an incremental update: the current page and `pdfinfo` are clean, and the old info string or content stream is still in the file. Image EXIF is unaffected when `image_metadata` returns the segment, because `metadata` is a different channel.

**Reproduction:** `Term("Zuleo").searchable_in_binary` is false. `max(Term("Zuleo").count(t) for t in raw_texts("Zuleo".encode("utf-16-le")))` is 1, and the same for the literal bytes `Author (Zuleo)`. `check_file` with only those raw strings, OCR off, returns no hits and status `ok`. The same for `"Zuleo".encode("utf-16-le")` inside a zlib stream that `raw_texts` inflates. A 6-letter term on the same UTF-16 bytes is `raw=1`.

**Validation:** A fixture whose only identifier is that 5-letter UTF-16 string, or the same letters in an unreferenced inflated stream, must report `raw=1` and exit 1. A changed final letter stays 0. The existing ASCII PNG metadata test stays a `metadata` hit. A mutant that still assigns 0 for every short raw count fails it.

## G0-11

**Severity:** medium

**File:line:** `src/anonimizador/verify.py:122`

**Description:** `scan` keeps a directory entry only when its suffix is `.pdf`, `.png`, `.jpg`, or `.jpeg`. `os.walk` calls `DirEntry.is_dir()` and, on `OSError`, treats the entry as a file and does not call `onerror` (`os.py` walk: the `except OSError` around `is_dir` sets `is_dir = False` and appends the name to `nondirs`). A dangling symlink raises `FileNotFoundError` there, so it lands in `names`. With no corpus suffix it is dropped, and the walk continues.

**Impact:** A directory symlink whose target is missing (unmounted volume, moved folder, symlink loop that raises `ELOOP`) is neither read nor reported. A clean PDF beside it is `[1] ok` and the process exits 0. Passing the dangling path itself as an argument is exit 2 (`not a file or directory`). The hole is an entry inside a directory that was listed. A real directory of mode `000` still errors, because `is_dir()` succeeds and `scandir` of the child hits `onerror`.

**Reproduction:** `root/clean.pdf` contains none of the terms. `root/Zuleo` is a symlink to a path that does not exist. The terms file contains `Zuleo`. `anonimizador verify --no-ocr --terms … root` prints `[1] ok` and exits 0. The same layout with the target present and full of identifying PDFs is the G0-3 case and must keep failing closed when the target cannot be listed.

**Validation:** The dangling layout must exit 2, must not print `ok`, and must not print `Zuleo`. A symlink to a real directory is still followed. A mutant that only filters `names` by suffix fails it.

## G0-12

**Severity:** low

**File:line:** `src/anonimizador/channels.py:204`

**Description:** The JPEG walker treats every `0xFF` as a marker that has a 2-byte length. JPEG fill bytes are extra `0xFF` bytes before a marker and have no length. On `FF D8 FF FF E1 …`, the marker read is `0xFF`, the length is taken from the following `E1` and length bytes, and `pos` jumps past the APP1 segment. `image_metadata` returns no blobs.

**Impact:** A short name that exists only in that EXIF or comment segment counts 0 in `metadata`. G0-10 then forces `raw` to 0 as well, so `--no-ocr` prints `ok` and exits 0. A term of 6 letters or more is still found in `raw`, because `compact` deletes the NULs. A normal `FF D8 FF E1` segment is read; the G0-8 test uses that form.

**Reproduction:** APP1 body `Exif\x00\x00UNICODE\x00` plus `Zuleo` in UTF-16-LE. Bytes `FF D8 FF E1 …` give `Term("Zuleo").count` 1 on `image_metadata`. One extra `FF` after SOI (`FF D8 FF FF E1 …`) gives 0 blobs. `check_file` with that metadata and `raw_texts` of the same bytes returns status `ok`.

**Validation:** `image_metadata` on the padded JPEG must produce a string on which the 5-letter term counts 1, including a fill run of more than one `FF`. The unpadded EXIF test stays 1. A mutant that still reads a length for marker `0xFF` fails it.

## G0-13

**Severity:** low

**File:line:** `src/anonimizador/verify.py:147`

**Description:** `collect` stores the first `CorpusFile` for a path and ignores later ones. The relative name is computed per argument. The first argument, when it is the nested directory, names the file `clean.pdf`. A later parent argument would name it `Zuleo/clean.pdf` and would attach symlink aliases, but `setdefault` keeps the first object.

**Impact:** Argument order decides whether a directory name is searched. `verify root` reports `name=1`. `verify root/Zuleo root`, with a clean file, reports `ok` and exits 0. Checking a subfolder and its parent together is weaker than checking the parent alone.

**Reproduction:** `root/Zuleo/clean.pdf` contains none of the terms. The terms file contains `Zuleo`. `anonimizador verify --no-ocr --terms … root/Zuleo root` yields one file named `clean.pdf`, prints `[1] ok`, and exits 0. The same command with the arguments swapped reports `name=1` and exits 1.

**Validation:** Both orders must exit 1 with `name=1`, and the output must not contain `Zuleo`. A mutant that keeps `setdefault` fails the nested-first order.
