# anonimizador: requirements

## Goal

Turn the owner's real medical PDFs and images into an anonymized corpus that the `embeddings-fundamentals` anonymization gate can accept. The tools automate most of the detection and redaction work. **The review stays human.** The gate requires a recorded decision with a reviewer and a date, and no detector is complete.

## Inputs and outputs

- **Inputs:**
  - original PDFs, PNGs, and JPEGs, which are read-only and never modified;
  - the owner's terms file, kept outside any git work tree;
  - settings: OCR languages, DPI, and date policy.
- **Outputs:**
  - anonymized copies under neutral names, in a new directory;
  - a per-file review packet;
  - an audit log of counts and kinds;
  - manifest entries for `embeddings-fundamentals`. The review is written into an entry only on human approval.

## Requirements

### R1. Local only, no traces
- Never make a network call. Models, such as name recognition, are installed ahead of time.
- Keep the originals untouched and write every output to a new directory.
- Reports, logs, and errors carry counts, positions, kinds, and neutral names, never a term, a match, or document text.
- Keep the map from neutral names to originals outside the repository.
- Refuse a terms file that git could commit.

### R2. Verify (stage 0, implemented)
Count each term in six places: the file name, the metadata, the text layer, the OCR of rendered pages, the OCR of embedded images, and the raw bytes. Raw bytes include inflated streams, so leftover incremental updates are covered.
- Matching ignores case and accents.
- Long terms are matched without spaces or punctuation.
- Numbers are matched by their digits. A term with 5 digits or more is a number whatever its label, and digits up to 3 non-alphanumeric characters apart are one number (Grok G0-1, G0-2).
- Every page and embedded image counts, and they are searched together, so a term split across a page break is found. Alternative readings of the same bytes (encodings) count an occurrence once (Grok G0-6).
- A term found in the file but not on the page is flagged, because it is text under a drawn box.
- Identifier patterns are counted as hints, in the file name too (Grok G0-16), across page and line breaks (Grok G0-18).
- Raw bytes include PDF strings, decoded from hex, octal escapes, and UTF-16 (with a byte order mark, or when at least half of the code units have a NUL high byte, as two-byte Identity-H strings do), so a leftover non-ASCII name is found (Grok G0-14, G0-30, G0-33). A literal's balanced parentheses belong to it, one level deep (Grok G0-20). The strings of a content stream are also read as drawn. Strings that simply continue (whitespace, NUL, comments, adjacent `Tj`, small TJ kerning, text-state and color operators between them) are one word. Anything else between two strings (a move, `ET`/`BT`, `q`, marked content, other bytes, a long gap) and a number of -200 or less are ambiguous, because only the fonts' widths and the positions, which are not read, tell glyph positioning, a word space, a new line, and another word apart. No gap is a sure break (Grok G0-37, G0-38, G0-39). Ambiguous moves are read joined, broken, and as every join of consecutive pieces up to 20 characters, so a short term or a number drawn in pieces is whole in one reading whatever each move is; an ambiguous boundary can add a match, never hide one (Grok G0-19, G0-22..G0-29, G0-32..G0-36). The joins are searched only for terms that must be whole words (under 6 letters) and for patterns, counted by distinct match, so an occurrence counts once. As for every channel, a term under 6 letters must be a whole word, and adjacent occurrences all count (Grok G0-28). The decoded strings are searched for patterns too, one per line, split at every ambiguous boundary, and in the joins; fully joined only for the text patterns (e-mail, CRM), since that reading glues the numbers of a whole page (Grok G0-21, G0-23, G0-34, G0-35, G0-40). An image file's bytes are read the same way (Grok G0-41).
- Hidden files and directories are searched; only AppleDouble files (`._name`) are left out (Grok G0-15).
- Every embedded image is read except icons, under 32 pixels on both sides; a thin strip can hold a line of text (Grok G0-17).
- A place that cannot be read is an error, never an empty place. This covers a failed or timed-out tool, an encrypted PDF, missing OCR language data, and pages that did not render. The file is reported `ERROR` with the place and the tool, never the path, and the run exits 2 (Grok G4-5).
- The file name is the path relative to the directory searched, so directory names are searched too (Grok G4-6).
- Directories are walked recursively, following symbolic links once per real directory. Every other relative path that reaches a directory is searched as a name of the files under it (Grok G0-7). A directory that cannot be listed, or a link that leads nowhere, is an error that stops the run, never a skipped place (Grok G0-3, G0-11). A file reached by several arguments is searched under every name they give it (Grok G0-13).
- Metadata and raw bytes are also read through their UTF-16 runs, so a short term stored as UTF-16 is a whole word (Grok G0-8).
- Raw bytes are searched for every term of 4 letters or digits or more. Shorter terms are not: on the synthetic fixtures, 9 of 10 three-letter names matched binary data by chance (Grok G0-10).
- JPEG fill bytes and standalone markers are skipped while reading metadata segments (Grok G0-12).
- A terms file that is not UTF-8 is refused with exit 2 and no traceback (Grok G0-4).
- A numeric option that is not a positive whole number (`--timeout`, `--dpi`) is a usage error: exit 2, no traceback (Grok G0-9).

### R3. Locate text
- Read the native text with positions on the page (`pdftotext -bbox-layout`, or a PDF library's word boxes).
- Read text from scanned pages and embedded images with positions (`tesseract … tsv`), together with OCR confidence.

### R4. Detect identifiers
Combine detectors, because each one misses things:
- the owner's terms, matched as in R2;
- validated patterns: CPF and CNS with their check digits, RG, phone, e-mail, CEP, and dates;
- field labels and the value after them: `Paciente:`, `Nome:`, `Mãe:`, `Prontuário:`, `Atendimento:`, `Pedido:`, `Médico:`, `CRM`, `Convênio`;
- Portuguese named-entity recognition for person names, for example a spaCy Portuguese model or Presidio;
- barcodes and QR codes (`zbar`);
- header and footer zones that repeat across pages;
- text burned into medical images, found by OCR of each image.

Every detection carries its page, its box, its kind, and its detector, and never its text in a report.

### R5. Redact for real
- **Text layer:** remove the text objects inside each box. Painting over them is not enough. A PDF library's apply-redactions operation does this.
- **Pixels:** paint the boxes into page images and embedded images, then flatten.
- **Fallback:** when true removal is impractical, rasterize the page, redact it, and rebuild the PDF. The output is then a scanned PDF, and the manifest entry says so.

### R6. Dates
Replace the date of birth with the age. For the other dates of one patient, either shift them all by one consistent offset or remove them, as a setting.

### R7. Metadata
Empty the information dictionary, XMP, EXIF, IPTC, and PNG text. Rewrite each PDF, so that earlier incremental updates disappear.

### R8. Independent verification
Run R2, and the pipeline's `anonymization-check`, on every output. They use a different code path than the detector. Any count above zero sends the file back to detection and review.

### R9. Review packet
For each file, produce:
- the before and after images of each page, side by side, with every redaction box outlined;
- the detections, as kinds and counts per page;
- the pages with low OCR confidence;
- the R8 results.

### R10. Approval
- An explicit command run by the reviewer records the review.
- It writes the `anonymization` line of the manifest entry (`filename`, `metadata`, `text`, `pixels` set to `"ok"`, with the reviewer and the date).
- No automatic step ever writes `"ok"`.

### R11. Proof on synthetic fixtures
- Plant invented identifiers in every channel: the text layer, covered text, metadata, an incremental update, an embedded image, burned-in text, and a barcode.
- Measure detection recall per channel, and require zero R2 counts after redaction.
- Never derive a fixture from a real exam.

## Limitations to state in every report

- Handwriting, stamps, and poor scans defeat OCR.
- Name recognition misses unusual names, and names split across lines or table cells.
- Rare combinations of diagnosis, date, and institution can identify a person even with no direct identifier left.

The tools lower the review effort. They never remove the review.
