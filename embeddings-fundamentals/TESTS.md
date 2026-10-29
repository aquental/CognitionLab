# Tests and the owner corpus

This guide covers how to turn anonymized real documents into an evaluable corpus: the manifest, the relevance labels, and the fidelity sample.

Anonymizing the documents is the job of the sibling project [`../anonimizador`](../anonimizador), on branch `anonimizador/scaffold` until it merges. It holds the procedure, the `anonimizador verify` tool, and the design of the automatic anonymizer.

The commands that run the test suites are in [`reports/README.md`](reports/README.md#tests).

All metrics so far come from the **synthetic** corpus in `tests/fixtures/corpus`. Real documents become evidence only after three steps: they are anonymized, their review is recorded in a manifest, and they get their own relevance labels.

> **Real documents never leave this machine.** The exams in `tests/sample-exams/` are git-ignored. They are never committed, pushed, or sent to a reviewer or remote engine, and neither is anything derived from them (see `CLAUDE.md`). The anonymized corpus lives in `corpus/`, which git also ignores, except for `corpus/manifest.toml`. Section 4 covers what may be published.

## 1. Anonymize first

A source is processed only when its manifest entry records four passed checks, plus the reviewer and the date (PRD §11):
- `filename`;
- `metadata`;
- `text`;
- `pixels`.

The procedure to meet them is in [`../anonimizador/docs/ANONYMIZATION.md`](../anonimizador/docs/ANONYMIZATION.md). It covers what to remove and what to keep, true redaction versus drawn boxes, pixels, and metadata with a PDF rewrite.

Check every final file with the verifier. It prints counts only:

```bash
cd ../anonimizador && uv sync
uv run anonimizador verify --terms ~/private/terms.txt ../embeddings-fundamentals/corpus/
```

The terms file is itself identifying. Keep it outside the repository; the verifier refuses one that git could commit.

This pipeline's `anonymization-check` flags file names and metadata keys. Only a person records `"ok"`.

## 2. Building the corpus

The PRD asks for 10–20 documents that cover four kinds:
- searchable PDFs (laboratory results, reports);
- scanned PDFs;
- medical images as PNG or JPEG files (radiographs, ultrasound);
- mixed PDFs (text, tables, and embedded images).

The ten frozen queries (`config/evaluation/queries-v1.toml`) ask for hemoglobin, glucose, creatinine, cholesterol and triglycerides, a report conclusion, findings, technique, a chest radiograph, abdominal ultrasound images, and an ultrasound report with its image. A query whose content is missing from the corpus has no relevant item. That is valid, but the query then measures less.

### 2.1 Draft the manifest

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

### 2.2 Draft the relevance labels

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

### 2.3 Write the fidelity sample

Copy a few laboratory rows **from the original documents by hand**, never from the extraction output. Put them in `corpus/fidelity-owner-v1.toml`, in the shape of `config/fidelity/sample-synthetic-v1.toml`. Each row is the `source_id`, the page, the test name, the value, the unit, and the reference range. Include pages that have a text layer and pages that go through OCR.

### 2.4 Run the evaluation

Use the commands in [`reports/README.md`](reports/README.md#reproducing-the-final-comparison) with three additions:
- `--manifest corpus/manifest.toml`;
- `--labels corpus/labels-owner-v1.toml`;
- `--fidelity corpus/fidelity-owner-v1.toml`.

Use new experiment ids or a new store, because experiment ids are create-only.

## 3. Local tests with the real exams

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

The tools in this guide are tested on synthetic files only:
- `tests/drafts.rs` covers both drafts without any external tool.
- The integration test `the_cli_drafts_the_synthetic_manifest_with_real_tools` (`RUN_INTEGRATION=1`) runs `draft-manifest` with real poppler.
- The verifier's tests live in `../anonimizador/tests/`.

## 4. What may be published

Under the PRD, reports that contain chunk text or image references may be generated from a corpus that passed the gate. Under `CLAUDE.md`, nothing derived from the real exams leaves this machine.

Until the owner decides otherwise:
- results on the anonymized corpus stay local;
- the only committed file is `corpus/manifest.toml`, which holds neutral names and checksums.

## 5. Automatic anonymization

The design and the stages of the automatic anonymizer are in [`../anonimizador/docs/SPEC.md`](../anonimizador/docs/SPEC.md) and [`../anonimizador/docs/PLAN.md`](../anonimizador/docs/PLAN.md). It covers detection, true redaction, metadata removal, independent verification, and a review packet.

The review stays human. No automatic step records `"ok"`.
