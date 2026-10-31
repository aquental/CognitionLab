# anonimizador: plan

Every stage ends the same way:
1. Its tests pass, including end-to-end tests on synthetic fixtures with the real tools.
2. The adversarial review loop (`CLAUDE.md`) reports no valid finding.
3. Its status is updated in `README.md`.

| Stage | Scope | Requirements | Exit criteria |
|---|---|---|---|
| 0 | Project structure; `verify` moved from `embeddings-fundamentals` | R1, R2 | `uv run pytest` green: unit tests for terms and channels, end-to-end tests on the covered-name fixture and a really redacted PDF, and a local real-exam test proving that no file name reaches the report |
| 1 | `detect`: text with positions (R3), plus terms, validated patterns, and field-label detectors (R4, first part). Output: per-file detections as JSON (page, box, kind, detector), stored with the outputs, never printed as values | R3, R4 | Recall of 1.0 on synthetic fixtures for every planted term, pattern, and field label, native and OCR. No detection text in any report |
| 2 | `redact` and `scrub`: true text removal and pixel boxes from the detections, the rasterize fallback, metadata removal with a PDF rewrite, date handling, then automatic `verify` of the outputs | R5, R6, R7, R8 | Every synthetic fixture verifies at zero after redaction, covered text included. The originals are byte-identical after a run. The decision on the PDF library and its license is recorded |
| 3 | Name recognition (local Portuguese model), barcodes and QR codes, repeated header and footer zones, and burned-in text in images | R4 (rest) | Recall measured per detector on fixtures with planted names, codes, and burned-in text. False positives on clinical terms are reported |
| 4 | `review`: a per-file packet (R9). `approve`: writes the manifest review on human approval (R10), so the `embeddings-fundamentals` manifest accepts the output | R9, R10 | The packet shows every box. Without `approve`, the gate still refuses the files; after it, the gate accepts them |

## Decisions

- **Redaction by rasterizing (owner, 2026-10-30).** Stage 2 implements the R5 fallback only: each page is rendered, boxes are painted, and the PDF is rebuilt from the images with Pillow (HPND license, permissive). This removes every text layer, covered text, metadata entry, and earlier revision by construction. No PDF library with true text removal is needed, so the AGPL candidates are avoided. The cost is the text layer: copies are scanned PDFs, which the `embeddings-fundamentals` pipeline already reads by OCR.
- **The batch runs the stages (owner, 2026-10-30).** `anonimizador batch` runs detection, redaction, and verification over a directory and writes a Markdown index of the run. Separate `detect`, `redact`, and `scrub` commands are not needed for now.
- **Date policy default: remove.** Every date is boxed unless `--keep-dates` is given. The date of birth is boxed with its label either way. Shifting dates and replacing the date of birth with the age (R6) would need text drawn into the image, so both remain open.
