# anonimizador: requirements

## Goal

Turn the owner's real medical PDFs and images into an anonymized corpus that the `embeddings-fundamentals` anonymization gate can accept. The tools automate most of the detection and redaction work. **The review stays human.** The gate requires a recorded decision with a reviewer and a date, and no detector is complete.

The identifiers that a copy loses are not thrown away: they are kept encrypted in a PII vault (R12). After approval, the copy is published to a write-once store and the original is moved to an encrypted vault in Brazil (R13).

## Inputs and outputs

- **Inputs:**
  - original PDFs, PNGs, and JPEGs, which are never modified, and which leave the source only to the original vault after approval (R13);
  - the owner's terms file, kept outside any git work tree;
  - settings: OCR languages, DPI, and date policy;
  - the master key of the PII vault, kept outside Postgres and outside every repository (R12).
- **Outputs:**
  - anonymized copies under neutral names, staged until review, then published to a write-once store (R13);
  - the PII vault in Postgres: every detection with its value encrypted, the map from neutral names to originals, the lifecycle state of each exam, and an audit table (R12, R13);
  - the original vault, in Brazil: the approved originals, encrypted (R13);
  - a per-file review packet;
  - an audit log of counts and kinds;
  - manifest entries for `embeddings-fundamentals`. The review is written into an entry only on human approval.

## Requirements

### R1. Private network only, no traces
- **The private network (owner, 2026-11-01)** is the owner's workstation and VPS, and the encrypted links between them. Everything else is external: OpenAI, TypeSafe's Jev, any other third-party service, and the reviewers (Grok, Codex, Antigravity).
- Never call an external service. The one exception is the classifier's remote mode, which sends masked text only, and only when both `--classify-remote` and `CLASSIFIER_REMOTE_ALLOWED=1` are given. Models, such as name recognition, are installed ahead of time.
- Postgres and the stores of R13 are reached inside the private network only, over TLS or a local socket.
- Read each original in place and never write it. After approval it is moved, never changed (R13).
- Write every output to a new place: a new run directory, a new vault row, a new stored object. Nothing written is ever overwritten.
- Reports, logs, errors, and the index carry counts, positions, kinds, and neutral names, never a term, a match, or document text. A value exists only encrypted in the PII vault (R12).
- The review packet (R9) shows the original pages, so it stays in the private network and is deleted once the review is recorded.
- Keep the map from neutral names to originals only in the PII vault, encrypted, and never in a repository.
- Refuse a terms file that git could commit, through a symbolic link too (Codex C0-11).

### R2. Verify (stage 0, implemented)
Count each term in six places: the file name, the metadata, the text layer, the OCR of rendered pages, the OCR of embedded images, and the raw bytes. Raw bytes include every stream, so leftover incremental updates are covered. Each stream is read as its object's dictionary says (with only whitespace and comments between the dictionary and `stream`), indirect references (to the last definition that `endobj` ends, so a header in a string or a comment is none) and `#xx` name escapes resolved: its data ends after `/Length` bytes when `endstream` follows them, and is decoded through every filter of its chain (Flate, LZW, ASCII85, ASCIIHex, RunLength) with its `/DecodeParms` (predictors 2 and 10 to 15, `/EarlyChange`; an image keeps its `/EarlyChange` and only skips its predictor). Filters before an image codec are decoded, and their output, which may hold the image's metadata, is read (Antigravity A0-5). A stream that cannot be decoded (a decoder that fails or stops short, a filter that is not read other than an image codec, a reference that cannot be resolved) makes the raw bytes unreadable (Codex C0-3, C0-13..C0-16). A stream without a filter is still tried as zlib data; a dictionary that cannot be read is an unread stream, since its filters and parameters are unknown (Antigravity A0-2). A `stream` keyword with no dictionary (in a comment or a string) before data that is no zlib stream is no stream, and hides no stream after it (Antigravity A0-1). Data must end with its filter's end-of-data mark (`~>`, `>`, the LZW end code, the run-length end byte). A stream the file is cut in, with no `endstream`, runs to the end of the file; unless it decodes whole, it is an unread stream (Antigravity round 1). Bytes that cannot be parsed at all make the raw bytes unreadable, never a traceback.
- Matching ignores case and accents.
- Long terms are matched without spaces or punctuation.
- Numbers are matched by their digits. A term with 5 digits or more is a number whatever its label, and digits up to 3 non-alphanumeric characters apart are one number (Grok G0-1, G0-2).
- Every page and embedded image counts, and they are searched together, so a term split across a page break is found. Alternative readings of the same bytes (encodings) count an occurrence once (Grok G0-6); independent places (two streams, two metadata segments, the information dictionary and the XMP packet) add up (Codex C0-12).
- A term found in the file but not on the page is flagged, because it is text under a drawn box.
- Identifier patterns are counted as hints, in the file name too (Grok G0-16), across page and line breaks (Grok G0-18).
- Raw bytes include PDF strings, decoded from hex, octal escapes, and UTF-16 (with a byte order mark, or when at least half of the code units have a NUL high byte, as two-byte Identity-H strings do), so a leftover non-ASCII name is found (Grok G0-14, G0-30, G0-33). A hex string may hold NUL and a single digit (Codex C0-6). Text strings are also read as PDFDocEncoding where it differs from Latin-1, such as the ligature at 0x93 (Codex C0-7). A literal's balanced parentheses belong to it at any depth: one level deep by a regular expression, deeper up to 1 KB within one stream (the keywords, not the word in a text, Codex C0-5), read besides the strings in it so that a false one in binary data hides nothing (Grok G0-20, G0-47); the line breaks between its pieces are in its own encoding, so UTF-16 text stays aligned (Antigravity A0-3). The strings of a content stream are also read as drawn. Strings that simply continue (whitespace, NUL, comments, adjacent `Tj`, small TJ kerning, text-state and color operators between them) are one word. Anything else between two strings (a move, `ET`/`BT`, `q`, marked content, other bytes, a long gap) and a number of -50 or less (`pdftotext` splits words from about -100) are ambiguous, because only the fonts' widths and the positions, which are not read, tell glyph positioning, a word space, a new line, and another word apart. No gap is a sure break (Grok G0-37, G0-38, G0-39). A string in a comment, in an inline image's data (which ends after the samples its dictionary gives, or at its filter's end-of-data mark, when `EI` follows; otherwise at its first `EI`, and the raw bytes are unreadable when another `EI` follows before the next image, whose `BI` must have its `ID`, Codex C0-4, Antigravity A0-4; `ID` is followed by one whitespace byte, so with `ID` and CRLF the LF is data), or in a dictionary (a marked-content property such as `<< /Lang (pt) >> BDC`) is not drawn: it is left out of these readings and stays in the per-string one (Grok G0-44). Only a `<<` that a `>>` closes opens a dictionary, so a stray one in binary data hides nothing (Grok G0-46). Ambiguous moves are read joined, broken, and as every join of consecutive pieces up to 20 characters, one space at the boundary kept, so a number whose own space is drawn at a move is whole too (Grok G0-45), so a short term or a number drawn in pieces is whole in one reading whatever each move is; an ambiguous boundary can add a match, never hide one (Grok G0-19, G0-22..G0-29, G0-32..G0-36). The joins are searched only for terms that must be whole words (under 6 letters) and for patterns, counted by distinct match, so an occurrence counts once. As for every channel, a term under 6 letters must be a whole word, and adjacent occurrences all count (Grok G0-28). The decoded strings are searched for patterns too, one per line, split at every ambiguous boundary, and in the joins; fully joined only for the text patterns (e-mail, CRM), since that reading glues the numbers of a whole page (Grok G0-21, G0-23, G0-34, G0-35, G0-40). An image file's bytes are read the same way (Grok G0-41).
- Hidden files and directories are searched; only AppleDouble files (`._name`) are left out (Grok G0-15).
- Every embedded image is read except icons, under 32 pixels on both sides; a thin strip can hold a line of text (Grok G0-17).
- A place that cannot be read is an error, never an empty place. This covers a failed or timed-out tool, an encrypted PDF, missing OCR language data, and pages that did not render. A Poppler error on stderr counts, even with exit 0, since a stream Poppler could not decode leaves its text out (Codex C0-2), and so does an image file that is neither a whole PNG (checksums, header, palette, pixel data of the size the header gives, each row with a known filter) nor a whole JPEG (frame and scan headers with their fields, an end after the scan), OCR or not (Codex C0-9), a compressed PNG text chunk that does not inflate (Codex C0-17), or a temporary directory that cannot be made or removed (Codex C0-10). The file is reported `ERROR` with the place and the tool, never the path, and the run exits 2 (Grok G4-5).
- The file name is the path relative to the directory searched, so directory names are searched too (Grok G4-6).
- Directories are walked recursively, following symbolic links once per real directory. Every other relative path that reaches a directory, through one link or a chain of them, is searched as a name of the files under it (Grok G0-7, Codex C0-8). A directory that cannot be listed, or a link that leads nowhere, is an error that stops the run, never a skipped place (Grok G0-3, G0-11). A file reached by several arguments is searched under every name they give it (Grok G0-13).
- Metadata and raw bytes are also read through their UTF-16 runs, so a short term stored as UTF-16 is a whole word (Grok G0-8).
- Raw bytes are searched for every term of 4 letters or digits or more. Shorter terms are not: on the synthetic fixtures, 9 of 10 three-letter names matched binary data by chance (Grok G0-10).
- JPEG fill bytes and standalone markers are skipped while reading metadata segments (Grok G0-12).
- A terms file that is not UTF-8 is refused with exit 2 and no traceback (Grok G0-4).
- A numeric option that is not a positive whole number (`--timeout`, `--dpi`) is a usage error: exit 2, no traceback (Grok G0-9).
- A usage error never prints an argument, and a `--lang` language without data is named by position: an argument can be a path or a term (Codex C0-1).

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

Every detection carries its page, its box, its kind, and its detector. Its text never reaches a report: it is stored only encrypted, in the PII vault (R12).

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

### R10. Approval (flow changed by the owner, 2026-11-01)
- An explicit command run by the reviewer records the decision on one staged copy: approve or reject, with the reviewer's name.
- **Approve** is refused unless the copy verifies `ok` (R8) and its review packet (R9) was produced. It records the review in Postgres (`filename`, `metadata`, `text`, `pixels` set to `"ok"`, the reviewer, and the date). Then it publishes the copy and moves the original to the original vault, in the order of R13. Last, it writes the `anonymization` line of the manifest entry for `embeddings-fundamentals`.
- **Reject** records the reviewer, the date, and the kind of problem. The staged copy is discarded, nothing is published, and the original stays in the source for a new run.
- No automatic step ever writes `"ok"`, publishes a copy, or moves an original.

### R11. Proof on synthetic fixtures
- Plant invented identifiers in every channel: the text layer, covered text, metadata, an incremental update, an embedded image, burned-in text, and a barcode.
- Measure detection recall per channel, and require zero R2 counts after redaction.
- Never derive a fixture from a real exam.

### R12. PII vault (owner, 2026-11-01)
- Every detection is stored in Postgres with its exam, page, box, kind, detector, the reading it came from (file name, metadata, text layer, OCR), and its value. A value found in pixels is the text that OCR read; no image crop is stored.
- **Encryption in the application.** A value is encrypted before it reaches Postgres, with AES-256-GCM. Each exam has its own data key. A master key wraps the data keys; it is kept in a secret file on the VPS, never in Postgres, never in a repository, and never in a log. Postgres never sees a value or a key.
- The associated data of each ciphertext names its exam and its row, so a value moved to another row fails to decrypt.
- A value may also be stored as a keyed digest (HMAC-SHA-256 of its folded form, with a key separate from the master key), to find the same identifier in several exams without decrypting. A plain hash is never stored: a CPF or a phone number is recovered from its hash by trying every value.
- In memory, a value has a type that cannot be printed, formatted, or logged, and it is erased after use. An error never carries a value.
- Reading a value back (re-identification) is a separate command that needs the master key.
- An audit table records every write, read, and erasure: who, when, which exam, and which operation, never a value. The tools' database role can add audit rows but cannot change or delete them.
- **Erasure.** Erasing an exam first deletes its data key, so its values and its vaulted original become unreadable at once (crypto-shredding). Then its rows and its vaulted file are deleted.

### R13. Lifecycle and storage (owner, 2026-11-01)
- Each original has a state in Postgres, keyed by the digest of its bytes:
  1. `received`: found in the source and read in place.
  2. `staged`: its copy is written to staging, verified (R8), and its detections are in the vault. A copy that verifies `FOUND` or `ERROR` goes back to detection; it is never published.
  3. `approved` or `rejected`: the reviewer's decision (R10).
  4. `published`: the approved copy is written to the published store, then read back and checked against its digest.
  5. `vaulted`: the original is encrypted with its exam's data key, written to the original vault, read back, and checked against its digest. Only then is it removed from the source.
- **Staging** is in the private network. It holds the copies waiting for review and their packets. A copy leaves staging when it is published or rejected.
- **The original vault** is in Brazil, in the private network. It holds the approved originals, encrypted and named by digest, for the retention period the owner sets. R12 erases them.
- **The published store** holds approved copies only, named `exam-<digest>`. It may be outside Brazil. It is write-once: a published copy is never changed, overwritten, or deleted by the tools. A correction is a new copy, with a new name and a new review. The store's immutability (for example, S3 Object Lock in compliance mode) is checked before the first write.
- Every step is idempotent and keyed by the digest. A run stopped at any point resumes without losing an original, writing a copy twice, or publishing an unapproved copy. An original already processed is reported by position and not read again.

## Legal status (LGPD)

- While the vault holds an exam's values and its map, its published copy is pseudonymized data for whoever holds the master key (LGPD art. 13 §4), not anonymized data (art. 12). It is still personal health data (art. 11).
- A copy published outside Brazil is an international transfer (art. 33). Record its legal basis before the first one.
- Erasing an exam (R12) removes the link. Its published copy is then as anonymous as its redaction and its review made it.

## Limitations to state in every report

- Handwriting, stamps, and poor scans defeat OCR.
- Name recognition misses unusual names, and names split across lines or table cells.
- Rare combinations of diagnosis, date, and institution can identify a person even with no direct identifier left.

The tools lower the review effort. They never remove the review.
