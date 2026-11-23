**ID:** AB-1
**Severity:** high
**File:line:** `src/anonimizador/detect.py:216` (also `src/anonimizador/detect.py:242`)
**Description:** `colon_value` and `run_from` stop collecting a field's value only when a word ends with `:` (`words[index].text.endswith(":")`). A following label on the same line whose colon is glued to its value (such as `Idade:45`, `CPF:123.456.789-09`, or `Sexo:M`) or separated as its own token (`Idade : 45`) does not end with `:`. Consequently, when the preceding label is blank (`Paciente: CPF:123.456.789-09` or `Paciente: Idade : 45` followed by a line with a patient identifier), `colon_value` takes the following label and its value as the first label's value, and when the preceding label has a value (`Paciente: Fulano Idade:45`), `colon_value` and `run_from` swallow the following field into the value. Worse, if a non-identifying colon label is followed by a glued identifying label (or vice versa), or if a bare `Nome` label is followed on the same row by `Data:05/06/2021` or `Sexo:F`, the boundary is missed; and if a bare `Nome` is blank and followed on its row by `Sexo : Feminino` while the patient's name is in the column below `Nome` (`column_below`), `run_from(words, row, edge, gap, gap)` or `run_from(..., float("inf"), gap)` takes `Sexo` (and `:` and `Feminino`) on the same row instead of falling back to `column_below`, leaving the patient's name in the column below unredacted.
**Impact:** A patient name under a column-heading `Nome` survives unredacted in the copy when another label on the heading row has a detached colon (`Data : 05/06/2021`) or glued colon (`Data:05/06/2021`) within reach or in the fallback `run_from`, because `bare_value` sees a non-empty horizontal `run_from` result and skips `column_below`.
**Reproduction scenario or verifiable rationale:** In `detect.py`, `bare_value` computes `run_from(words, row, edge, gap, gap) or column_below(words, first, gap) or run_from(words, row, edge, float("inf"), gap)`. Suppose a table header row has `placed(("Nome", 0), ("Data", 50), (":", 145), ("05/06", 200), number=0)` (or `("Data:05/06", 50)`) and the row below has `placed(("Sicrano", 0), ("Inventado", 100), number=1)`. Neither `"Data"` nor `"Data:05/06"` ends with `":"`, so `run_from` takes `"Data"` (or `"Data:05/06"`) as the value of `"Nome"`, `bare_value` returns truthy without calling `column_below`, and `"Sicrano Inventado"` in row 1 is never detected or redacted.
**Validation suggestion:** Stop `colon_value` and `run_from` at any word that starts or belongs to a `COLON_LABEL` or `NUMBER_LABEL` match on that line/row (including glued `Label:value` and detached `Label :` forms), not only at `words[index].text.endswith(":")`, and add a regression test with a column heading `Nome` beside a glued or detached-colon label.

---

**ID:** AB-2
**Severity:** high
**File:line:** `src/anonimizador/detect.py:296`
**Description:** When a `COLON_LABEL` ends its extracted `line` (`not any(i > last for i in line)`), `label_values` unconditionally falls back to taking `lines[k + 1]` on the same page (`below = lines[k + 1]`), without checking whether the label already has a value to its right on the same visual row that `pdftotext` or `tesseract` placed into a different block or non-adjacent line (the exact layout split addressed for `BARE_LABEL` in CB-12). In multi-column or boxed medical forms, extractors frequently emit all labels of a box as one block (`line` `k`: `Paciente:`, `line` `k+1`: `Data:`) and their values to the right as another block (`line` `k+2`: `Fulano Ficticio`, `line` `k+3`: `01/02/1980`), or emit another block before the value block.
**Impact:** When `Paciente:` and its value `Fulano Ficticio` are on the same horizontal row but placed in separate blocks by `pdftotext` or `tesseract` (with another line `lines[k + 1]` between them in block order, e.g. the next field label `Idade: 45` or a clinical line), `COLON_LABEL` on `Paciente:` takes `lines[k + 1]` instead of the value to its right on its row, leaving the patient name `Fulano Ficticio` unredacted.
**Reproduction scenario or verifiable rationale:** Construct a page with `Word("Paciente:", (0, 0, 80, 15), ("ocr", 1, 1, 0))`, `Word("Hemoglobina", (0, 30, 90, 45), ("ocr", 1, 1, 1))`, and `Word("Sicrano", (100, 0, 180, 15), ("ocr", 2, 1, 0))`. `lines_of` orders lines by block: line 0 is `["Paciente:"]`, line 1 is `["Hemoglobina"]`, line 2 is `["Sicrano"]`. For `"Paciente:"`, `not any(i > last for i in line)` is true, so `label_values` takes `lines[1]` (`"Hemoglobina"`) and never looks at `row_after` (`"Sicrano"` at `(100, 0, 180, 15)`), leaving `"Sicrano"` unredacted while redacting `"Hemoglobina"`.
**Validation suggestion:** Before falling back to the line below a `COLON_LABEL` (and when finding the value on the label's line), check `row_after` on the same visual row (as `bare_value` does for CB-12), and when falling back below, use geometrical proximity below the label rather than blindly taking `lines[k + 1]` in extractor block order.

---

**ID:** AB-3
**Severity:** high
**File:line:** `src/anonimizador/detect.py:286` (and `src/anonimizador/detect.py:300`)
**Description:** Two label-detection gaps cause date-of-birth and bare-name values to survive:
1. `BARE_LABEL` (`detect.py:286`) only matches `nome do paciente`, `nome da mae`, `nome do pai`, `nome social`, and `nome`. Medical forms that print bare headers without colons for `Paciente` or `Mãe` / `Filiação` or `Data de Nascimento` / `Dt. Nasc.` / `Nascimento` / `DN` (e.g. a table header `Data de Nascimento` above `01/02/1980` or `Paciente` above/beside the patient name) are not matched when there is no colon. Under `--keep-dates`, a date of birth under a bare `Data de Nascimento` or `Dt. Nasc.` header survives completely.
2. Even for `COLON_LABEL` (`Data de nascimento:`), when the date is on the line below (`detect.py:300`) in a separate block (`("ocr", 2, 1, 0)`), `lines[k + 1]` is only the next line in block order if no other line in block 1 followed the label; if the header row had two blocks (`block 1`: `Data de Nascimento:`, `block 2`: `Sexo:`), `lines[k + 1]` is `Sexo:`, and the date of birth in `block 3` directly below `Data de Nascimento:` is missed under `--keep-dates`.
**Impact:** Under `--keep-dates` (or for bare `Paciente` / `Mãe`), the patient's date of birth or name survives redaction despite the README/PLAN guarantee that *"the date of birth is removed with its label either way"*.
**Reproduction scenario or verifiable rationale:**
1. Create a two-column header with colon labels in separate OCR blocks: `Word("Dt. Nasc.:", (0, 0, 80, 15), ("ocr", 1, 1, 0))`, `Word("Sexo:", (200, 0, 250, 15), ("ocr", 2, 1, 0))`, `Word("01/02/1980", (0, 20, 80, 35), ("ocr", 3, 1, 0))`, `Word("F", (200, 20, 220, 35), ("ocr", 4, 1, 0))`. Run `detect_reading("ocr", [(1, words)], [], remove_dates=False)`. `lines[0]` is `["Dt. Nasc.:"]`, `lines[1]` is `["Sexo:"]` (where `colon_value` stops immediately because `"Sexo:"` ends with `":"`), so `"01/02/1980"` in `lines[2]` is never detected.
2. Same without colons (`"Data de Nascimento"` over `"01/02/1980"`, or `"Paciente"` beside `"Sicrano Inventado"`): neither `COLON_LABEL` nor `BARE_LABEL` matches.
**Validation suggestion:** When a `COLON_LABEL` has no value on its line/row, look for the nearest word geometrically below the label (`column_below`) as fallback in addition to `lines[k + 1]`, and include date-of-birth labels (and `paciente`) in bare-label detection or column-below lookup so DOB is always removed under `--keep-dates`.

---

**ID:** AB-4
**Severity:** medium
**File:line:** `src/anonimizador/locate.py:168` (and `src/anonimizador/locate.py:189`)
**Description:** Two image-handling gaps in `locate.py` and `redact.py`:
1. In `locate.py:188-190`, `read_image_page` converts `1`, `L`, `I;16`, and `I` images to `"L"` via `shown.convert("L")`. In Pillow, `Image.convert("L")` on a 16-bit grayscale PNG (`mode == "I;16"` or `"I"` as produced by medical X-ray / ultrasound / CT exports) does **not** scale the 16-bit dynamic range (`0..65535`) to 8-bit (`0..255`); instead it clamps values `>= 255` to `255` (or truncates), turning the entire medical image pure white and destroying both the burned-in text before OCR and the clinical image in the copy.
2. In `locate.py:168`, `read_pdf_pages` opens the PNG rendered by `pdftoppm` outside any `IMAGE_ERRORS` handler, whereas `read_image_page` catches `IMAGE_ERRORS`. More importantly, in `read_image_page`, `ImageOps.exif_transpose(im)` and `im.convert(...)` load the image without verifying that the input PNG or JPEG is whole (`png_is_whole` / `jpeg_is_whole`), and if Pillow raises `zlib.error` on corrupt compressed chunks (which is not a subclass of `OSError` or `IMAGE_ERRORS` in Python's `zlib` module), it escapes `IMAGE_ERRORS` and `batch.py`'s `except (ToolError, OSError):` handler as an unhandled traceback.
**Impact:** (1) 16-bit grayscale medical PNGs (`I;16`) are wiped to all-white (`255`) before OCR in `read_image_page`, so identifiers in the image are neither detected by `locate` nor preserved legibly, while `verify` runs `tesseract` directly on the original 16-bit PNG (which `tesseract`/Leptonica scales properly) and on the all-white copy (which reads nothing and passes `verify` as `"ok"`!). (2) A corrupt PNG chunk raising `zlib.error` crashes `batch` with a raw traceback instead of recording an `ERROR` row.
**Reproduction scenario or verifiable rationale:**
1. Create a 16-bit PNG (`im = Image.new("I;16", (400, 100), 60000)` with text drawn at intensity `2000`, which is dark gray in 16-bit). In `locate.read_image_page`, `shown.convert("L")` clamps every pixel `>= 255` (both `2000` and `60000`) to `255`, producing a blank white PNG. `verify` on the copy sees a blank white PNG (`ok`), while the original had visible dark text.
2. Check `issubclass(zlib.error, (*IMAGE_ERRORS, OSError))` in Python 3.12: it is `False`.
**Validation suggestion:** Add `zlib.error` to `IMAGE_ERRORS`; scale `I;16` (e.g. `im.point(lambda i: i * (1 / 256)).convert("L")` or `ImageOps.autocontrast` / bit-shift `>> 8`) before converting to `"L"`; and test a 16-bit grayscale PNG with dark text (`intensity > 255`, background `65535`).

---

**ID:** AB-5
**Severity:** medium
**File:line:** `src/anonimizador/terms.py:157` (and `src/anonimizador/batch.py:170`)
**Description:** `committable(path)` determines whether `path` is ignored by running `git -C <nearest_existing_dir> check-ignore -q os.path.abspath(path)`. However:
1. When `path` does not exist yet and its parent directory inside the work tree does not exist yet either, a `.gitignore` rule scoped to directories (such as `out/` or `**/logs/`, with a trailing slash) requires git to know that the intermediate component is a directory, or when a `.gitignore` rule is negated/scoped, `git check-ignore` on a non-existent path inside a non-existent parent directory can differ from after `mkdir`.
2. More critically, in `batch.py:170`, `os.path.realpath(p)` resolves symlinks only for prefix components of `p` that *currently exist* on disk. If `out` is `/tmp/outside/link/sub` where `/tmp/outside` exists, `/tmp/outside/link` does not exist yet, or if `out` is a relative path through a non-existent directory, `realpath` stops resolving at the non-existent component. And `start` only checks `places = [run.report, run.logs / "console.txt"]`; if `out` itself is a git work tree ignored by a parent repo, or if a subdirectory `<out>/<stamp>` matches a `.gitignore` negation rule (`!2026*/exam-*.pdf`), `run.copies` is not checked.
3. In `committable` (`terms.py:171`), when `git rev-parse --is-inside-work-tree` returns `0` with `false` (which happens when `directory` is inside a `.git` directory itself, e.g. `--out /path/to/repo/.git/anon`), `inside.stdout.strip() != b"true"` returns `False` at line 175! Thus `committable("/path/to/repo/.git/index-or-subdir")` returns `False` ("not committable"), allowing `batch` to write files directly inside `.git/`!
**Impact:** Passing `--out .git/out` (or a symlink pointing into `.git/`) bypasses `committable` because `git rev-parse --is-inside-work-tree` exits `0` and prints `false` inside `.git`, so `committable` returns `False` and `batch` writes the index (with original filenames) and copies directly into the repository's `.git` directory.
**Reproduction scenario or verifiable rationale:** Run `git -C .git rev-parse --is-inside-work-tree` in any git repository: it exits with return code `0` and stdout `false\n`. In `terms.py:174-175`:
```python
if inside.stdout.strip() != b"true":
    return False
```
Therefore `committable(".git/out/20261030-140509-anonimizer.md")` returns `False`, and `start(Path(".git/out"), now)` succeeds and writes the index inside `.git/out/`.
**Validation suggestion:** In `committable`, also check `git rev-parse --is-inside-git-dir` (or check `any(os.path.basename(d) == ".git" or os.path.lexists(os.path.join(d, ".git")) for d in ancestors(directory))` when `stdout.strip() != b"true"`), and refuse any path inside `.git` as committable/unsafe.

---

**ID:** AB-6
**Severity:** medium
**File:line:** `src/anonimizador/batch.py:144` (and `src/anonimizador/batch.py:228`)
**Description:** Two issues in `batch.py`'s file discovery and error handling:
1. In `originals(source)` (`batch.py:141-145`), `source.iterdir()` filters with `p.is_file()`. For a broken symlink in `source` (a symlink to a `.pdf` whose target is missing or a symlink loop), `p.is_file()` returns `False`, so `originals` silently skips the broken symlink instead of failing the run—violating R2/SPEC line 152 (*"A directory that cannot be listed, or a link that leads nowhere, is an error that stops the run, never a skipped place (Grok G0-3, G0-11)"*).
2. In `originals(source)` (`batch.py:154`), `neutral_name(path)` opens and reads every file in `source` *outside* the per-exam `try ... except OSError` in `anonymize` (and before `start()` creates the console log and index). If a single original in `source` is unreadable (`PermissionError` on `path.open("rb")`), `originals(source)` aborts the entire batch before starting rather than recording an `ERROR` row for that exam, or if `verify_one` on the original or `verify_one` on the copy raises `OSError` when writing the log (`write_log`), it is not caught in `anonymize`.
**Impact:** A broken symlink to an original exam in `source` is silently ignored by `batch` without any warning or error, so the user believes every exam in `source` was anonymized when one was skipped completely.
**Reproduction scenario or verifiable rationale:** In a source directory with `valid.pdf` and a broken symlink `missing.pdf -> nowhere.pdf`, `p.is_file()` is `False` and `p.is_symlink()` is `True`. `originals(source)` returns only `[Exam(1, valid.pdf, ...)]` and `run_batch` exits `0` (`1 of 1 copy(ies) verified ok`). Compare with `verify.py:138-141`, which explicitly checks `if not os.path.exists(path): raise BrokenLinkError`.
**Validation suggestion:** Check `if p.is_symlink() and not p.exists(): raise BrokenLinkError` (or raise `VerifyError`) while iterating `source.iterdir()` in `originals`, and add a test for a broken symlink in `source`.

---

**ID:** AB-7
**Severity:** low
**File:line:** `src/anonimizador/locate.py:155`
**Description:** `aligned(width, height, size)` only checks whether the aspect ratio `width / height` matches `size[0] / size[1]` within `2%`. It cannot detect:
1. A page rotated by **180 degrees** (`/Rotate 180`), where `width / height` is identical to `size[0] / size[1]`, or a square page (`/Rotate 90` or `270` when `width == height`), where the aspect ratio is `1.0` in all orientations.
2. When `/Rotate 180` (or a square rotated page) occurs, `pdftotext -bbox-layout` reports coordinates in the unrotated (or differently oriented) space while `pdftoppm` renders the rotated image, so `aligned` returns `True` and `placed` maps native boxes to the inverted positions on the page, while suppressing the `unaligned` warning. (Although OCR still runs on the rendered image, any identifier found only in the text layer—or where OCR misreads a character—has its native box painted on the wrong side of the page.)
**Impact:** On a 180°-rotated or square 90°/270°-rotated PDF page, native text-layer boxes are placed at inverted coordinates instead of being flagged `unaligned`.
**Reproduction scenario or verifiable rationale:** On a page with `/MediaBox [0 0 600 600] /Rotate 90` (or `/MediaBox [0 0 595 842] /Rotate 180`), `aligned(width, height, size)` evaluates `abs(1.0 - 1.0) <= 0.02` (`True`) or `abs(595/842 - 595/842) <= 0.02` (`True`).
**Validation suggestion:** Check whether `pdftotext -bbox-layout` and `pdftoppm` disagree on orientation (or inspect `pdfinfo` page rotation) or document/handle square and 180° rotated pages.
