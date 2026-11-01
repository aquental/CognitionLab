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
| 3 | Name recognition (local Portuguese model), barcodes and QR codes, repeated header and footer zones, and burned-in text in images. Built in Rust, after RS2 | R4 (rest) | Recall measured per detector on fixtures with planted names, codes, and burned-in text. False positives on clinical terms are reported |
| 4 | Replaced by RS4: R10 changed, and approval now publishes the copy and vaults the original | R9, R10 | See RS4 |

## Rust rewrite

The tools are rewritten in Rust (owner, 2026-11-01). The Python code stays the reference until RS2 ends. Then it is removed.

### Why

- **Speed and memory.** `verify` reads the decompressed pixels of a copy as text, in Python. On a synthetic 3-page color PDF at 300 dpi, `batch` took 54 s and 2.3 GB, of which `verify --no-ocr` of the copy alone took 31 s and 2.3 GB. The original took 1.5 s. Folding and matching run over about 25 MB per page, in several readings. OCR time is unchanged by a rewrite.
- **Values cannot leak by mistake.** The PII vault (R12) puts values in the code's hands. In Rust, a value is a type with no `Display` and a redacted `Debug`, erased on drop (`secrecy`, `zeroize`). The compiler then enforces the rule that the Python code enforced through review: several findings were values that leaked in an exception message (CB-9, CB-13, G0-4).
- **One binary on the VPS**, with the stack that `embeddings-fundamentals` already uses. Poppler and Tesseract stay external tools.

### Layout

A Cargo workspace in `anonimizador/`. The crate boundaries keep R8 true: `anon-verify` never depends on `anon-redact`.

| Crate | Contents | Port of |
|---|---|---|
| `anon-text` | Python text semantics, `fold`, terms, patterns, matching, counting | `terms.py` |
| `anon-tools` | Running poppler and Tesseract with a timeout, the Poppler stderr rule, the missing-language check | `channels.run`, `missing_languages` |
| `anon-verify` | PDF streams and their filters, the six places, the file walk, the report | `pdfstreams.py`, `channels.py`, `verify.py` |
| `anon-redact` | Word boxes, detectors, image normalization, painting, the PDF writer | `locate.py`, `detect.py`, `redact.py` |
| `anon-vault` | Postgres, envelope encryption, the secret value type, the audit table, the lifecycle states, staging, and the two stores | new (R12, R13) |
| `anonimizador` | The binary: `verify`, `batch`, `review`, `approve`, `reject`, `reveal`, `erase`; the index; the classifier call | `cli.py`, `batch.py`, `classifier.py` |

**Python semantics.** `embeddings-fundamentals` already reproduces Python's `re` and `str` in Rust (`src/classifier/pyre.rs`, `pyclasses.rs`, `texto.rs`), generated from Python by `scripts/gen-pyclasses.py`. Move them into a shared crate that both projects depend on, in a separate change in `embeddings-fundamentals`. They cover the traps that change what is found:
- Python's `\s` matches `\x1c`–`\x1f`, and Rust's does not. `channels.py` marks ambiguous boundaries with `\x1f`, and `MIXED_HEAD = r"\s?\S*"` depends on it.
- `casefold` is not `to_lowercase`: `"ß".casefold()` is `"ss"`.
- `unicodedata.combining` and NFKD need the same Unicode version as the reference.

**Byte patterns.** `fancy-regex` searches text only. The byte patterns of `pdfstreams.py` and `channels.py` use `regex::bytes` with `(?-u)`, where `\s`, `\w`, and `\d` are ASCII, as in Python's byte patterns. Their 10 lookarounds become explicit boundary checks around each match, tested one by one against Python. The 17 lookarounds of the text patterns (`terms.py`, `detect.py`) go through the shared `pyre` translation.

**Dependencies.** All are MIT or Apache-2.0; record each license on adoption.
- From `embeddings-fundamentals`: `clap`, `regex`, `fancy-regex`, `image` (PNG, JPEG), `kamadak-exif`, `unicode-normalization`, `serde_json`, `sha2`, `tempfile`, `thiserror`, `tokio`.
- New: `flate2`, `quick-xml`, `csv`, `sqlx` (Postgres, rustls), `aes-gcm`, `hmac`, `secrecy`, `zeroize`, and an S3-compatible client for the stores, chosen in RS4.
- No PDF library: copies are still rebuilt from page images (decision of 2026-10-30).

**Concurrency.** `tokio` runs the tools and Postgres. The CPU work (folding, matching, decoding) is plain synchronous code, run on blocking threads.

**CLI errors.** `clap` prints the value of a bad argument by default. Its errors are rewritten so that they never print an argument (Codex C0-1), with a test per error kind.

### How parity is proven

- **Ported tests.** Every pytest case is ported, under the same name and with the same finding ID (G0-…, C0-…, A0-…, CB-…, AB-…).
- **Differential runs.** A script runs the Python and the Rust `verify` and `batch` on the same inputs and compares their reports, their detection JSON, and the decoded pixels of the copies. Byte equality of the copies is not required: zlib encoders differ.
  - The inputs are the synthetic fixtures plus generated PDFs.
  - On the owner's real exams, the comparison is local (`RUN_SAMPLE_EXAMS=1`) and prints counts only.
- **No panic.** Python's rule "never a traceback" becomes "never a panic". `cargo-fuzz` and `proptest` run the stream parser, the decoders, the string readers, and the PNG and JPEG checks on arbitrary bytes.
- **Benchmarks.** The 3-page color PDF above becomes a benchmark. The target is `verify --no-ocr` of the copy in under 2 s and 300 MB.

### Stages

| Stage | Scope | Requirements | Exit criteria | Estimate |
|---|---|---|---|---|
| RS0 | Workspace; the shared Python-semantics crate; `anon-text`; `anon-tools`; the differential script | R1, R2 (matching) | The `test_terms.py` cases pass. On generated strings, `fold`, `count`, and `count_patterns` agree with Python | 3–5 days |
| RS1 | `anon-verify` and `anonimizador verify` | R1, R2 | The ported `test_channels`, `test_verify`, and `test_cli` cases pass. Reports equal Python's on every fixture. Fuzzing finds no panic. The benchmark target is met | 2–3 weeks |
| RS2 | `anon-redact` and `anonimizador batch`, with the index and the classifier. Then the Python code is removed, and `CLAUDE.md` moves to Rust conventions and to the `rust-dev` review | R3–R8 | The ported `test_locate`, `test_detect`, `test_batch`, and `test_classifier` cases pass. Detection JSON and the copies' decoded pixels equal Python's. The real-exam comparison is equal | 1.5–2 weeks |
| RS3 | `anon-vault`, part 1: the schema and its migrations, envelope encryption, the secret value type, the HMAC digests, the audit table, and `reveal` and `erase`. `batch` stores the detections | R12 | A planted value is found in no log, report, error, or index, and in no column of the database read raw. A row moved to another exam fails to decrypt. `erase` makes the values unreadable before deleting them | 1–1.5 weeks |
| RS4 | `anon-vault`, part 2: the lifecycle states, staging, the review packet, `approve` and `reject`, the published store, and the original vault | R9, R10, R13 | Stopping the process at every step, by fault injection, leaves a state that resumes without a lost original, a copy written twice, or an unapproved copy published. The published store refuses an overwrite. Without `approve`, the `embeddings-fundamentals` gate refuses the copy; after it, the gate accepts it | 1.5–2 weeks |

Each stage still ends with the adversarial review loop. Counting it, the rewrite takes about 2 to 3 months.

## Decisions

- **Redaction by rasterizing (owner, 2026-10-30).** Stage 2 implements the R5 fallback only: each page is rendered, boxes are painted, and the PDF is rebuilt from the images with Pillow (HPND license, permissive). This removes every text layer, covered text, metadata entry, and earlier revision by construction. No PDF library with true text removal is needed, so the AGPL candidates are avoided. The cost is the text layer: copies are scanned PDFs, which the `embeddings-fundamentals` pipeline already reads by OCR.
- **The batch runs the stages (owner, 2026-10-30).** `anonimizador batch` runs detection, redaction, and verification over a directory and writes a Markdown index of the run. Separate `detect`, `redact`, and `scrub` commands are not needed for now.
- **Rust rewrite and PII vault (owner, 2026-11-01).** See "Rust rewrite" and SPEC R1, R10, R12, and R13:
  - The private network (workstation and VPS) is not external.
  - Values are kept encrypted in Postgres, by the application, with the master key outside the database. Only the OCR text of pixels is kept, never crops.
  - An approved copy goes to a write-once store, which may be outside Brazil.
  - The original goes to an encrypted vault in Brazil, only after `verify` is `ok` and a person approves.
  - Until RS2 ends, the Python code receives fixes for valid findings only, and no new features.
- **Date policy default: remove.** Every date is boxed unless `--keep-dates` is given. The date of birth is boxed with its label either way. Shifting dates and replacing the date of birth with the age (R6) would need text drawn into the image, so both remain open.
