# anonimizador

Local tools to anonymize medical PDFs and images, and to verify that no identifier is left. The tools exist to produce the anonymized corpus that [`../embeddings-fundamentals`](../embeddings-fundamentals) evaluates. That pipeline only *checks* a corpus; it refuses any source whose anonymization review is not recorded in its manifest. Producing the corpus is this project's job.

> **Nothing leaves this machine.** Every tool runs locally, with no network call. Reports hold counts and kinds, never a term, a match, or document text. The owner's real exams are never committed and never sent to a reviewer or service (see [`CLAUDE.md`](CLAUDE.md)).

## Status

| Stage | Command | Status |
|---|---|---|
| 0 | `anonimizador verify`: counts the owner's identifiers left in a file, in six places | **Implemented** |
| 1 | `detect`: finds identifiers with their positions on the page | Planned ([`docs/PLAN.md`](docs/PLAN.md)) |
| 2 | `redact` and `scrub`: true text removal, pixel boxes, metadata removal, and PDF rewrite | Planned |
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
| `name` | The file name |
| `metadata` | The PDF information dictionary and XMP (`pdfinfo`), and image EXIF, XMP, IPTC, PNG text, and comments |
| `text` | The PDF text layer (`pdftotext`) |
| `ocr` | What the rendered pages show (`pdftoppm` + `tesseract`), or what an image file shows |
| `images` | What each image embedded in a PDF shows (`pdfimages` + `tesseract`) |
| `raw` | The file bytes and every compressed stream: old incremental updates, content streams, XMP packets, and PNG text. Terms shorter than 6 letters are not searched here, because binary data is full of short letter runs. |

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
- **Files** are named by position. Pass `--show-names` to print their paths.
- **Terms** are named by their position among the terms in the file, so `term 1` is the first term listed.
- **Matching** ignores case and accents. Long terms are matched without spaces or punctuation, so OCR spacing errors and line breaks do not hide them. Numbers are matched by their digits: `123.456.789-09` and `12345678909` are the same term.
- **The hint** marks the classic failed redaction: a box drawn over text that is still in the file.
- **Pattern counts** (CPF, CNS, phone, e-mail, CEP, date, CRM) are hints. They fail the check only with `--strict-patterns`.
- **Safety:** the verifier refuses a terms file that git could commit.

Exit status: 0 means nothing was found, 1 means something was found, and 2 means a usage error or a missing tool. `--no-ocr` skips rendering and OCR. `--lang` and `--dpi` tune the OCR.

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
| `src/anonimizador/verify.py` | File collection, per-file results, and report lines |
| `src/anonimizador/cli.py` | The `anonimizador` command |
| `tests/fixtures/make_covered_name_pdf.py` | Writes the synthetic fixtures. `covered-name.pdf` hides an invented name under a drawn box |

The tools were first written inside `embeddings-fundamentals` during its Phase 4 (2026-10-29) and moved here, so that the evaluation pipeline and the corpus tooling are separate projects.
