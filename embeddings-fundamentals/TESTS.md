# Tests and the owner corpus

This guide covers three things:

1. How to anonymize real PDFs and images.
2. How to turn them into an evaluable corpus: manifest, relevance labels, and fidelity sample.
3. What an automatic anonymizer would have to do.

The commands that run the test suites are in [`reports/README.md`](reports/README.md#tests).

All metrics so far come from the **synthetic** corpus in `tests/fixtures/corpus`. Real documents become evidence only after three steps: they are anonymized, their review is recorded in a manifest, and they get their own relevance labels.

> **Real documents never leave this machine.** The exams in `tests/sample-exams/` are git-ignored. They are never committed, pushed, or sent to a reviewer or remote engine, and neither is anything derived from them (see `CLAUDE.md`). The anonymized corpus lives in `corpus/`, which git also ignores, except for `corpus/manifest.toml`. Section 5 covers what may be published.

## 1. What the anonymization gate requires

A source is processed only when its manifest entry records four passed checks, plus the reviewer and the date (PRD §11). Missing or `"failed"` checks make the pipeline refuse the source before extraction.

| Check | What must hold |
|---|---|
| `filename` | The file name identifies nobody. Use neutral names such as `lab-real-001.pdf`. |
| `metadata` | Embedded metadata holds no identifier: the PDF information dictionary and XMP, and image EXIF, IPTC, and XMP. |
| `text` | The document text holds no identifier. This includes text that OCR reads from scans. |
| `pixels` | Nothing visible identifies anyone: page images, embedded images, burned-in annotations on ultrasound or radiograph frames, stamps, signatures, barcodes, and QR codes. |

The tools help with the review but never replace it:
- `anonymization-check` flags file names and metadata keys.
- `scripts/anonymization-verify.py` counts identifiers left in each part of a file.

Only a person records `"ok"`.

## 2. Anonymizing PDFs and images

### What to remove

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

Work on copies, never on the originals. Do the work outside the repository and copy only the finished files into `corpus/`.

### Step by step

1. **Rename** each file to a neutral name, for example `lab-real-001.pdf` or `us-real-002.pdf`. If you need a map from new names to originals, keep it outside the repository.

2. **Redact the text layer.** This applies to PDFs whose text you can select.
   - Use a tool whose redaction *removes* the text, such as the Redact tool in Adobe Acrobat Pro.
   - **A black rectangle drawn over text does not remove it.** `pdftotext`, search, and copy-paste still read the text underneath. The synthetic fixture `tests/fixtures/anonymization/covered-name.pdf` shows exactly this failure.
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

5. **Verify**, always on the final files, as described in the next subsection.

### Verifying with the local verifier

`scripts/anonymization-verify.py` takes a list of terms that must not appear: names, CPF, record numbers, addresses, and so on. It searches each file in several places and prints **counts only**. It never prints a term, a match, or document text, and it names files by position (`[3]`) unless you pass `--show-names`. It also counts common identifier patterns as hints: CPF, CNS, phone, e-mail, CEP, date, and CRM.

```bash
# The terms file is itself identifying: keep it OUTSIDE the repository.
# One term per line; lines starting with # are comments.
python3 scripts/anonymization-verify.py --terms ~/private/terms.txt corpus/
```

| Column | Where it looks |
|---|---|
| `name` | The file name |
| `metadata` | The PDF information dictionary and XMP (`pdfinfo`), and image EXIF, XMP, IPTC, and PNG text chunks |
| `text` | The PDF text layer (`pdftotext`) |
| `ocr` | What the rendered pages show (`pdftoppm` + `tesseract`), or what an image file shows |
| `images` | What each embedded image shows (`pdfimages` + `tesseract`) |
| `raw` | The file bytes and every compressed stream: old incremental updates, content streams, XMP packets, and PNG text. Terms shorter than 6 letters are not searched here, because binary data is full of short letter runs. |

- **Exit status:** 0 means nothing was found, 1 means a term was found, and 2 means a usage error or a missing tool. With `--strict-patterns`, a pattern match also fails.
- **Hint:** a term found in `text` or `raw` but not in `ocr` is the signature of a box drawn over text that is still in the file.
- **Matching:** case and accents are ignored. Long terms are matched without spaces or punctuation, so OCR spacing errors and line breaks do not hide them. Numbers are matched by their digits, so `123.456.789-09` and `12345678909` are the same term.
- **Safety:** the verifier refuses a terms file that git could commit.

Then complete the review:

- `embeddings-fundamentals anonymization-check --manifest corpus/manifest.toml` flags file names and metadata keys, without printing their values.
- Look at every rendered page (`pdftoppm -r 150 -png file.pdf page`) and every embedded image (`pdfimages -png file.pdf img`). The verifier cannot find what you did not list, and OCR misses handwriting, stamps, and poor scans.

## 3. Building the corpus

The PRD asks for 10–20 documents that cover four kinds:
- searchable PDFs (laboratory results, reports);
- scanned PDFs;
- medical images as PNG or JPEG files (radiographs, ultrasound);
- mixed PDFs (text, tables, and embedded images).

The ten frozen queries (`config/evaluation/queries-v1.toml`) ask for hemoglobin, glucose, creatinine, cholesterol and triglycerides, a report conclusion, findings, technique, a chest radiograph, abdominal ultrasound images, and an ultrasound report with its image. A query whose content is missing from the corpus has no relevant item. That is valid, but the query then measures less.

### 3.1 Draft the manifest

```bash
cargo build --release --features engine-embed-anything,engine-fastembed
B=target/release/embeddings-fundamentals
$B draft-manifest --dir corpus --corpus-id owner-v1 --out corpus/manifest.toml
```

For every PDF, PNG, and JPEG under `corpus/`, `draft-manifest` writes a manifest entry.

**Measured** fields:
- the SHA-256 checksum;
- the pages and how many have a usable text layer (the same rule as ingestion);
- the embedded images the image filter keeps;
- the filename and metadata findings, as keys and patterns, never values.

**Guessed** fields:
- `input_type`:
  - `scanned_pdf` when no page has a usable text layer;
  - `searchable_pdf` when every page has one and no image is kept;
  - `mixed_pdf` otherwise.
- `document_type`: always a guess.

Behavior to know about:
- No entry carries an anonymization review, so **the gate refuses every source** until you record one.
- The manifest is tracked by git. `draft-manifest` refuses to write when a file name is flagged (exit 3, sources named by position). Rename those files, or pass `--allow-flagged-names` if you have checked them. A flagged name never becomes a `source_id`.
- `--out` never overwrites an existing file. Without `--out`, the draft goes to stdout.

Then complete the entries:
1. Confirm each `input_type` and `document_type`. The `document_type` choices are `laboratory_report`, `imaging_report`, `radiograph`, and `ultrasound_image`.
2. Replace each `# anonymization = …` line with the review you did, for example: `anonymization = { filename = "ok", metadata = "ok", text = "ok", pixels = "ok", reviewer = "aquental", date = "2026-10-29" }`.
3. Check the manifest:

```bash
$B anonymization-check --manifest corpus/manifest.toml
$B manifest --manifest corpus/manifest.toml      # checksums and eligibility; exit 0 when every source passes
```

### 3.2 Draft the relevance labels

Labels are judged by hand. `draft-labels` builds the list to judge by **pooling**. It runs each frozen query against each indexed run and keeps the top `--depth` results of each run, one candidate per distinct chunk. Pool from every configuration you will compare, with a depth above 3, so that no configuration's top 3 holds an unjudged item.

```bash
S=store-owner
for run in "pool-e5 text-fastembed-e5-base" "pool-minilm text-fastembed-minilm" \
           "pool-siglip image-siglip-multilingual" "pool-clip image-clip-vit-b32"; do
  set -- $run
  $B ingest --manifest corpus/manifest.toml --config config/experiments/$2.toml --store $S --experiment $1
done
$B draft-labels --store $S --corpus owner-v1 --depth 10 \
  --run pool-e5=config/experiments/text-fastembed-e5-base.toml \
  --run pool-minilm=config/experiments/text-fastembed-minilm.toml \
  --run pool-siglip=config/experiments/image-siglip-multilingual.toml \
  --run pool-clip=config/experiments/image-clip-vit-b32.toml \
  --out corpus/labels-owner-v1.toml
```

What the draft contains:
- A comment with each candidate's full chunk text. For an image candidate, the comment gives the path of the stored image instead (`<store>/assets/….png`).
- The runs and ranks that returned each candidate.
- For a text candidate, an `anchor`: a line of the chunk that no other pooled chunk shares, cut to 80 characters.
- For an image candidate, its original reference and `asset_checksum`. The evaluator needs both to accept an image label.
- `label = "TODO"` and `note = "TODO"`. **The file does not load until every `TODO` is replaced.** A label is 0 (irrelevant), 1 (partially relevant), or 2 (directly relevant).

How to judge:
- Write a short note for each judgment.
- Check each text anchor: it must be a distinctive substring of the chunk.
- You may delete candidates you judge 0, because unlisted items count as irrelevant.
- If you know a relevant item that no run returned, add it by copying a candidate block.

The draft holds document text. Keep it in `corpus/`, which git ignores.

### 3.3 Write the fidelity sample

Copy a few laboratory rows **from the original documents by hand**, never from the extraction output. Put them in `corpus/fidelity-owner-v1.toml`, in the shape of `config/fidelity/sample-synthetic-v1.toml`. Each row is the `source_id`, the page, the test name, the value, the unit, and the reference range. Include pages that have a text layer and pages that go through OCR.

### 3.4 Run the evaluation

Use the commands in [`reports/README.md`](reports/README.md#reproducing-the-final-comparison) with three additions:
- `--manifest corpus/manifest.toml`;
- `--labels corpus/labels-owner-v1.toml`;
- `--fidelity corpus/fidelity-owner-v1.toml`.

Use new experiment ids or a new store, because experiment ids are create-only.

## 4. Local tests with the real exams

When the owner's exams are in `tests/sample-exams/`, run:

```bash
RUN_SAMPLE_EXAMS=1 cargo test --release --test sample_exams -- --include-ignored
```

These tests assert properties without reading any value:
- the gate refuses every unreviewed exam;
- every file name is flagged without being printed;
- extraction is deterministic, and chunking drops no token;
- kept images are stored without metadata.

They name documents by position only. They are not retrieval evidence.

The integration suite (`RUN_INTEGRATION=1`) covers the tools in this guide on synthetic files only:
- `the_anonymization_verifier_counts_a_covered_name_without_printing_it`;
- `the_cli_drafts_the_synthetic_manifest_with_real_tools`.

`tests/drafts.rs` covers both drafts without any external tool.

## 5. What may be published

Under the PRD, reports that contain chunk text or image references may be generated from a corpus that passed the gate. Under `CLAUDE.md`, nothing derived from the real exams leaves this machine.

Until the owner decides otherwise:
- results on the anonymized corpus stay local;
- the only committed file is `corpus/manifest.toml`, which holds neutral names and checksums.

## 6. Could anonymization be automatic?

Mostly, yes, as an assistant that does the detection and redaction work. **The review must stay human.** The PRD makes the review a recorded decision with a reviewer and a date, and no detector is complete. A tool for this repository would need to do the following.

1. **Work offline and leave no trace.**
   - Use local models only, never a network call.
   - Keep the originals untouched and write the outputs to a new directory.
   - Write an audit log of counts and kinds, never values.
   - Use neutral output names. Keep the map to the originals outside the repository.

2. **Find the text and where it sits on the page.**
   - Native text layer with positions: `pdftotext -bbox-layout`, or a PDF library that returns words with coordinates.
   - OCR with positions for scanned pages and embedded images: `tesseract … tsv`.

3. **Detect identifiers**, combining several detectors, because each one misses things:
   - the owner's term list, with the same normalization as the verifier;
   - patterns with validation: CPF and CNS with check digits, RG, phone, e-mail, CEP, and dates;
   - field labels and the value after them: `Paciente:`, `Nome:`, `Mãe:`, `Prontuário:`, `Atendimento:`, `Pedido:`, `Médico:`, `CRM`, `Convênio`;
   - Portuguese named-entity recognition for person names, for example a spaCy Portuguese model or Microsoft Presidio configured for Portuguese;
   - barcodes and QR codes, for example with `zbar`;
   - header and footer zones that repeat across pages;
   - burned-in text on medical images, found by OCR of each image.

4. **Redact for real.**
   - Text layer: remove the text objects inside each region, not just paint over them. One way is a PDF library's apply-redactions operation.
   - Pixels: paint the regions into the page and image pixels, then flatten.
   - Fallback when true text removal is impractical: rasterize the whole page, redact it, and rebuild the PDF. The source becomes a `scanned_pdf`.

5. **Handle dates.** Replace the date of birth with the age, and shift the other dates of one patient by one consistent offset, or remove them.

6. **Strip the metadata and rewrite each PDF**, so earlier incremental updates disappear.

7. **Verify independently.** Run `scripts/anonymization-verify.py` and `anonymization-check` on the output, with a different code path than the detector. Any count above zero sends the file back.

8. **Produce a review packet for the human.**
   - For each page: the before and after images side by side, with every redaction box outlined.
   - The detections as kinds and counts.
   - The pages where OCR confidence was low.

9. **Record the review only on approval.** The tool may draft the manifest entry, as `draft-manifest` does, but it must never write `"ok"` by itself. An explicit approval step by the reviewer writes the `anonymization` line with the reviewer and the date.

10. **Prove itself on synthetic fixtures.**
    - Plant known identifiers in every channel: text layer, covered text, metadata, incremental update, embedded image, burned-in text, and barcode.
    - Measure detection recall per channel, and require zero verifier counts after redaction.
    - `tests/fixtures/anonymization/covered-name.pdf` (from `scripts/make-covered-name-pdf.py`) is the first such fixture.

**Limitations.** Handwriting, stamps, and low-quality scans defeat OCR. Name recognition misses unusual names and names split across lines or table cells. Rare combinations of diagnosis, date, and institution can identify a person even with no direct identifier left.

The tool lowers the review effort. It does not remove the review.

**Implementation choices:**
- **Python** has mature libraries: PDF redaction with positions, Presidio, spaCy, and zbar bindings. Check each library's license before adopting it; some PDF libraries are AGPL.
- **Rust** inside this crate would reuse poppler, tesseract, and `lopdf`. However, removing text from content streams is hard there, so the realistic Rust path is the rasterize-and-rebuild fallback.

Either way, the tool belongs outside the evaluation pipeline. It produces the corpus, and the pipeline only checks it.
