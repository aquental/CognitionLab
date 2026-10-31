# anonimizador

Local tools to anonymize medical PDFs and images, and to verify that no identifier is left. The tools exist to produce the anonymized corpus that [`../embeddings-fundamentals`](../embeddings-fundamentals) evaluates. That pipeline only *checks* a corpus; it refuses any source whose anonymization review is not recorded in its manifest. Producing the corpus is this project's job.

> **Nothing leaves this machine.** Every tool runs locally, with no network call. Reports hold counts and kinds, never a term, a match, or document text. The owner's real exams are never committed and never sent to a reviewer or service (see [`CLAUDE.md`](CLAUDE.md)).

## Status

| Stage | Command | Status |
|---|---|---|
| 0 | `anonimizador verify`: counts the owner's identifiers left in a file, in six places | **Done.** Reviewed by Grok (15 rounds), Codex (2 rounds), and Antigravity (3 rounds, no open finding); merged by the owner's decision with Grok round 16 and Codex's confirmation of the later fixes pending ([`FINDINGS.md`](reports/reviews/stage-0/FINDINGS.md)) |
| 1 | Detection: identifiers with their positions on the page (terms, validated patterns, field labels) | **Implemented, in review**: runs inside `batch`; no separate `detect` command yet |
| 2 | Redaction: pixel boxes on rasterized pages, metadata removal, PDF rewrite, automatic `verify` of each copy | **Implemented, in review**: `batch`, with the rasterize fallback of R5 only (no true text removal, so no PDF library) |
| 3 | Name recognition, barcodes and QR codes, and text burned into images | Planned |
| 4 | `review` and `approve`: a review packet, and recording the review in the manifest only on human approval | Planned |

Requirements: [`docs/SPEC.md`](docs/SPEC.md). The manual procedure that the tools automate: [`docs/ANONYMIZATION.md`](docs/ANONYMIZATION.md).

## Setup

You need Python ≥ 3.12, [uv](https://docs.astral.sh/uv/), poppler (`pdftotext`, `pdfinfo`, `pdftoppm`, `pdfimages`), and Tesseract with Portuguese data:

```bash
brew install uv poppler tesseract tesseract-lang     # macOS
# apt-get install poppler-utils tesseract-ocr tesseract-ocr-por   (Debian/Ubuntu)
cd anonimizador
uv sync                  # creates .venv with the package and the dev tools
```

## `anonimizador verify`

```bash
# The terms file is itself identifying: keep it OUTSIDE the repository.
# One term per line (names, CPF, record numbers, addresses...); lines starting with # are comments.
uv run anonimizador verify --terms ~/private/terms.txt ../embeddings-fundamentals/corpus/
```

The verifier searches six places in each PDF, PNG, or JPEG:

| Place | What is read |
|---|---|
| `name` | The file name, with every directory of its path |
| `metadata` | The PDF information dictionary and XMP (`pdfinfo`), and image EXIF, XMP, IPTC, PNG text, and comments |
| `text` | The PDF text layer (`pdftotext`) |
| `ocr` | What the rendered pages show (`pdftoppm` + `tesseract`), or what an image file shows |
| `images` | What each image embedded in a PDF shows (`pdfimages` + `tesseract`); only icons, under 32 pixels on both sides, are skipped |
| `raw` | The file bytes and every compressed stream: old incremental updates, content streams, XMP packets, and PNG text, with PDF strings decoded (hex, octal escapes, nested parentheses, UTF-16), and the pieces of text drawn glyph by glyph read every way their moves allow, since a move between glyphs can be positioning, a word space, or a new line. Terms shorter than 4 letters are not searched here, because binary data is full of short letter runs. |

```text
anonimizador verify: 2 file(s), 2 term(s), OCR at 300 dpi (por+eng)
[1] FOUND
    term 1: metadata=1 text=1 raw=2
    hint: term 1 is in the file but not visible on the page: text under a drawn box? Redact for real (docs/ANONYMIZATION.md).
    term 2: text=1 ocr=1 raw=1
    possible identifiers (patterns): cpf=1
[2] ok
```

How to read the report:
- **Directories** are searched recursively, following symbolic links (each real directory once; every other path that reaches it, from any argument, is searched as a name too). A directory that cannot be listed, or a link that leads nowhere, stops the run with exit 2, because a skipped place proves nothing. Hidden files and directories are searched too; only macOS AppleDouble files (`._name`) are left out.
- **Files** are named by position. Pass `--show-names` to print their names as the corpus records them: the path relative to the directory searched, directories included. Directory names are searched too, because a manifest records the whole relative path.
- **Terms** are named by their position among the terms in the file, so `term 1` is the first term listed.
- **Matching** ignores case and accents. Long terms are matched without spaces or punctuation, so OCR spacing errors and line breaks do not hide them. Numbers are matched by their digits: `123.456.789-09`, `12345678909`, and `CPF 123.456.789-09` are the same term. Any term with 5 digits or more is matched by its digits, and digits up to 3 characters apart (spaces, punctuation, parentheses, line breaks) are one number, so `(11) 98765-4321` and a number broken across lines are found. Every page and embedded image counts, and a term split across a page break is found. UTF-16 metadata (EXIF, Windows tags) is decoded, so a short name stored in it is found. Raw bytes are searched for terms of 4 letters or more; shorter terms match binary data by chance, so list a patient's full name, not only a 3-letter first name.
- **The hint** marks the classic failed redaction: a box drawn over text that is still in the file.
- **Pattern counts** (CPF, CNS, phone, e-mail, CEP, date, CRM) are hints, searched in the file name, across page breaks, and in the PDF strings of old objects too. They fail the check only with `--strict-patterns`.
- **Safety:** the verifier refuses a terms file that git could commit. The terms file must be UTF-8 text.

- **ERROR** marks a file in which a place could not be read: a tool failed or timed out, a PDF is encrypted, or pages did not render. The `unreadable:` lines name the place and the tool, never the file. An unread place proves nothing, so such a file is never reported `ok`.

Exit status: 0 means nothing was found, 1 means something was found, and 2 means a usage error, a missing tool or OCR language, or a file with an unreadable place. `--no-ocr` skips rendering and OCR. `--lang` and `--dpi` tune the OCR. `--timeout` (default 600 s) bounds each tool run. `--dpi` (up to 2400) and `--timeout` (up to 86400 s) must be whole numbers from 1.

## `anonimizador batch`

Anonymizes every original in a directory and indexes the run:

```bash
uv run anonimizador batch --terms ~/private/terms.txt --out <outputs> <originals>
scripts/anonymize-sample-exams.sh --terms ~/private/terms.txt   # the owner's sample exams
```

> [!WARNING]
> **"Anonymized" does not yet mean "free of names."** Stages 1–2 remove only what is in your terms list and what a few patterns match. Other names can stay in the file.
>
> - Stages 1–2 look for identifiers in three ways:
>   - the terms **you** write in the terms file, which stays outside the repository;
>   - patterns such as CPF, phone, e-mail, CEP, and dates;
>   - labels such as `Paciente:` or `Médico:`, and the value after them.
> - Automatic recognition of person names comes only in **stage 3**. Until then, a name that is not in your list and does not follow a label can get through.
> - `verify` also searches only for what is in the list. A count of zero means "nothing from the list is left", not "no name is left".
> - **Before you run the script, put in the list every name and number that can appear**, including those of the doctors, the mother, and the clinic. Then look at the output pages with your own eyes.

For each original, in order:
1. **Verify the original**, for the counts before anonymization.
2. **Locate the words.** Each page is rendered at `--dpi`. Its words are read twice, with their boxes: from the PDF text layer (`pdftotext -bbox-layout`, scaled onto the image) and by OCR of the image (`tesseract … tsv`). A text layer whose page shape does not match the image, such as a rotated page, is not placed; that page gets OCR boxes only, and the console says so.
3. **Detect identifiers** in both readings:
   - the owner's terms, matched as `verify` counts them;
   - CPF and CNS with valid check digits, phones, e-mails, CEPs, and dates (`--keep-dates` keeps dates);
   - the value after a field label: `Paciente:`, `Nome:`, `Mãe:`, `Prontuário:`, `Pedido:`, `Médico:`, `Convênio:`, the date-of-birth labels (`Dt. Nasc.:`), and similar labels with a colon. Council and document numbers are taken after `CRM`, `CPF`, `RG`, and `CNS`, colon or not. A value runs to the next field, whose colon may be glued (`Idade:45`) or apart (`Idade :`). A label that ends its line takes the value on its row, even in another block of the extraction, else the value under it, as under a column heading. A `Nome`, `Paciente`, `Mãe`, or date-of-birth label that starts its line needs no colon: `Nome` takes the words beside it up to a wide gap, else the words under it; `Paciente` and `Mãe` take only a capitalized name, and a date-of-birth label only a value with digits.
4. **Redact by rasterizing.** A black box is painted over every detected word. The copy is rebuilt from the page images alone, so no text layer, covered text, metadata, or earlier revision of the original survives. A PDF becomes a scanned PDF, with its pages stored losslessly (JPEG artifacts change what OCR reads), and an image becomes a PNG.
5. **Verify the copy** with `verify`, a separate code path (R8). A term still found sends the copy back to review, and the run exits 1.

- **Originals** are the PDF, PNG, and JPEG files directly in the source directory. Subdirectories are not walked, so the outputs can live under the source. Originals are read, never written.
- **Each copy** is named from a digest of its original's bytes, `exam-<digest>.pdf` or `.png`, never from the original's name. Each run writes into its own new directory, `<outputs>/<YYYYMMDD-HHMMSS>/`, and creates every file there, so no original, earlier copy, or link is ever written over.
- **The index**, `<outputs>/<YYYYMMDD-HHMMSS>-anonimizer.md`, has one row per original. Each row links the original, the original's `verify` log, the detections (boxes with page, kind, and detector, never text), the copy, and the copy's `verify` log. It also links the console output of the whole run. Logs are in `<outputs>/<YYYYMMDD-HHMMSS>/logs/`.
- **The index names the originals**, and a copy is unreviewed, so `--out` must be a place where git can commit neither the index, the logs, nor the copies: a git-ignored directory or one outside any repository. When git cannot tell, the run is refused. The script writes to `../embeddings-fundamentals/tests/sample-exams/anonymized/`, which git ignores. The console names originals by position and prints counts only.
- **Without `--terms`**, only patterns and labels are detected and verified. `ok` then does not mean that no name is left, and the run warns about it.
- **Exam type (optional).** `--classifier <binary>` takes the `embeddings-fundamentals` binary. With it, every copy whose verification is `ok` is also classified by exam type (`classify`, `../embeddings-fundamentals/spec/CLASSIFIER.md` §11):
  - The index gains the columns *Exam type (classifier)* and *Weights*, and a header line with the classifier's identity. Each copy's JSON line is kept in `<outputs>/<YYYYMMDD-HHMMSS>/classifier/`.
  - A copy that fails verification is never classified.
  - Nothing is sent by default: the cell says `not sent`. `--classify-remote` lets the classifier send each copy's masked text to TypeSafe's Jev, and only when `CLASSIFIER_REMOTE_ALLOWED=1` is also set. The key comes from `RLCD_API_KEY` or `TYPESAFE_API_KEY`, in the environment or in `../embeddings-fundamentals/.env` (see its `.env.example`). `CLASSIFIER_REMOTE_ALLOWED` must be set on the command itself: a `.env` never sets it.
  - **For `--classify-remote`, the binary must be built with the `classifier-jev` feature:**

    ```bash
    (cd ../embeddings-fundamentals && cargo build --release --features classifier-jev)
    CLASSIFIER_REMOTE_ALLOWED=1 scripts/anonymize-sample-exams.sh --terms ~/private/terms.txt \
        --classifier ../embeddings-fundamentals/target/release/embeddings-fundamentals --classify-remote
    ```

    A binary built without it cannot send. Every copy then shows `error: configuracao_invalida`, and nothing leaves the machine.
  - A classifier failure (timeout, crash, invalid output) is the cell `error: classifier failed` and never changes the exit status.

**What it does not do yet:**
- recognize names without a label or a term (stage 3);
- read barcodes, QR codes, or repeated headers (stage 3);
- replace the date of birth with the age or shift dates (R6): it removes them;
- remove text while keeping a text layer (true redaction, R5).

The copy is checked by the same OCR engine that found the words, so a word that OCR misreads in both passes is missed by both. **A person still reviews every copy.**

It takes the reading options of `verify` (`--terms`, `--lang`, `--dpi`, `--timeout`, `--strict-patterns`). It refuses `--no-ocr`: a copy holds only pixels, and only OCR reads them. On a copy, `verify` can show pattern hints (CPF, CNS, phone, e-mail, date) taken from its pixel bytes, by the hundred on a large page: noise, not text, since the copy has no text layer. Only `--strict-patterns` makes them fail it. Exit status:
- 0: every copy was written and verifies clean;
- 1: a copy still holds something `verify` finds;
- 2: the run cannot start, or a file or step failed.

## Development

```bash
uv run pytest                                   # unit tests, plus end-to-end tests when the tools are installed
uv run ruff check . && uv run ruff format --check .
RUN_SAMPLE_EXAMS=1 uv run pytest tests/test_sample_exams.py   # local only: the owner's real exams
```

| Path | Contents |
|---|---|
| `src/anonimizador/terms.py` | Terms, patterns, and matching. The terms file is refused when git could commit it |
| `src/anonimizador/channels.py` | Readers for the six places |
| `src/anonimizador/pdfstreams.py` | PDF streams decoded as their dictionaries say: filters, parameters, lengths |
| `src/anonimizador/verify.py` | File collection, per-file results, and report lines |
| `src/anonimizador/cli.py` | The `anonimizador` command |
| `tests/fixtures/make_covered_name_pdf.py` | Writes the synthetic fixtures. `covered-name.pdf` hides an invented name under a drawn box |

The tools were first written inside `embeddings-fundamentals` during its Phase 4 (2026-10-29) and moved here, so that the evaluation pipeline and the corpus tooling are separate projects.
