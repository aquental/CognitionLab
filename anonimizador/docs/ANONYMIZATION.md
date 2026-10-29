# Anonymizing PDFs and images by hand

This is the manual procedure that the `anonimizador` stages automate (see [`PLAN.md`](PLAN.md)). Until they exist, follow it for every file. Then record the review in the corpus manifest of `embeddings-fundamentals`; [`../../embeddings-fundamentals/TESTS.md`](../../embeddings-fundamentals/TESTS.md) shows how to build the corpus.

> Work on copies, never on the originals, and outside any git work tree. Copy only the finished files into the corpus directory.

## What the anonymization gate requires

A source is processed only when its manifest entry records four passed checks, plus the reviewer and the date (PRD §11 of `embeddings-fundamentals`). Missing or `"failed"` checks make the pipeline refuse the source before extraction.

| Check | What must hold |
|---|---|
| `filename` | The file name identifies nobody. Use neutral names such as `lab-real-001.pdf`. |
| `metadata` | Embedded metadata holds no identifier: the PDF information dictionary and XMP, and image EXIF, IPTC, and XMP. |
| `text` | The document text holds no identifier. This includes text that OCR reads from scans. |
| `pixels` | Nothing visible identifies anyone: page images, embedded images, burned-in annotations on ultrasound or radiograph frames, stamps, signatures, barcodes, and QR codes. |

The tools help with the review but never replace it. `anonimizador verify` counts identifiers left in each part of a file, and the pipeline's `anonymization-check` flags file names and metadata keys. Only a person records `"ok"`.

## What to remove

- **Remove or replace these:**
  - Patient name and mother's name.
  - CPF, RG, CNS (cartão SUS), and health-plan card number.
  - Address, phone, and e-mail.
  - Record, visit, order, and protocol numbers.
  - Barcodes and QR codes, which usually encode those numbers.
  - Date of birth: replace it with the age.
  - Requesting physician's name and CRM, signatures, and stamps (recommended).
  - Headers and footers that repeat on every page.
- **Keep these**, because the evaluation queries measure them: test names, values, units, reference ranges, findings, technique, and conclusion.
- **Other dates** (collection, report) are quasi-identifiers. Remove them, or shift every date of one patient by the same offset.

## Step by step

1. **Rename** each file to a neutral name, for example `lab-real-001.pdf` or `us-real-002.pdf`. If you need a map from new names to originals, keep it outside the repository.

2. **Redact the text layer.** This applies to PDFs whose text you can select.
   - Use a tool whose redaction *removes* the text, such as the Redact tool in Adobe Acrobat Pro.
   - **A black rectangle drawn over text does not remove it.** `pdftotext`, search, and copy-paste still read the text underneath. The fixture `tests/fixtures/covered-name.pdf` shows exactly this failure, and `verify` flags it.
   - An alternative is to retype the clinical content into a new PDF with invented identifiers.
   - Another alternative is to rasterize the document. Render the pages with `pdftoppm -r 300 -png`, black out the regions, and rebuild a PDF from the images. The source then becomes a `scanned_pdf` and loses its text layer.

3. **Redact the pixels.**
   - Black out identifying regions in scanned pages and in every embedded image. You can see the embedded images with `pdfimages -list` and extract them with `pdfimages -png`.
   - Ultrasound and radiograph frames usually carry the name, the date, and the institution in the image itself.
   - Export or flatten the result, so the hidden layer is not kept underneath.

4. **Strip the metadata.**
   - Install the tools with `brew install exiftool qpdf`.
   - Images: run `exiftool -all= -overwrite_original file.jpg`.
   - PDFs: run `exiftool -all:all= -overwrite_original file.pdf`, then **rewrite** the file with `qpdf file.pdf clean.pdf`. exiftool edits a PDF by appending an incremental update. The old information dictionary and XMP stay in the file until a rewrite drops them.

5. **Verify** the final files:
   ```bash
   uv run anonimizador verify --terms ~/private/terms.txt <corpus dir>
   ```
   Every term must be at 0 in every place. A term in `text` or `raw` but not in `ocr` is text under a drawn box: go back to step 2.

6. **Look** at every rendered page (`pdftoppm -r 150 -png file.pdf page`) and every embedded image (`pdfimages -png file.pdf img`). The verifier cannot find what you did not list, and OCR misses handwriting, stamps, and poor scans.

7. **Record** the review in the manifest entry, with the reviewer and the date.
