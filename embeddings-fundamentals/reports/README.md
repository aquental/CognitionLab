# Reports and reproduction

This directory holds the reviewed evidence of the lab (PRD §12 deliverables 6–8, 10). Raw per-run outputs, the BDD JSON report, and the index store stay out of version control. Only the final, reviewed reports below are committed.

All numbers come from the **synthetic fixture corpus** (`tests/fixtures/corpus`: 3 PDFs with invented content and 3 synthetic images) and the **synthetic** relevance labels. They exercise the pipeline end to end and say nothing about retrieval quality on real medical documents. The owner corpus, relevance labels, and fidelity sample remain outstanding (see [`ACCEPTANCE.md`](ACCEPTANCE.md)).

| Path | Contents |
|---|---|
| [`phase-4/comparison.md`](phase-4/comparison.md) (+ `.json`) | **Final comparison.** Text-only (3 configurations), image-only (2), and combined RRF retrieval (2), evaluated on the same corpus, queries, labels, and machine. It reports quality, latency, memory, index size, cost, reliability, fidelity, and the top-3 results with relevance notes for every query |
| `phase-4/p4-*.json` | One result per configuration (`experiment --report` / `combine --report`), the inputs of the comparison |
| [`phase-2/comparison.md`](phase-2/comparison.md), [`phase-3/comparison.md`](phase-3/comparison.md) | Earlier stage reports (text engines; image models), kept as produced at the end of those phases |
| [`examples/record-001.json`](examples/record-001.json) | An actual indexed record (deliverable 4) |
| [`spikes/phase3-models.md`](spikes/phase3-models.md) | Cross-modal model compatibility spike |
| [`progress/PROGRESS.md`](progress/PROGRESS.md) | Per-phase implementation, decisions, validation, and blockers |
| `reviews/phase-N/` | Raw adversarial review reports (Grok, Codex, Antigravity) and the tracked findings (`FINDINGS.md`) |
| [`ACCEPTANCE.md`](ACCEPTANCE.md) | PRD §12 deliverables and §13 acceptance criteria, each linked to its evidence |

## Requirements

- Rust 1.99.1 (`rust-toolchain.toml`).
- poppler (`pdftotext`, `pdftoppm`, `pdfimages`, `pdfinfo`) and Tesseract with Portuguese data (`brew install poppler tesseract tesseract-lang`, or `apt-get install poppler-utils tesseract-ocr tesseract-ocr-por`). `cargo run -- doctor` checks them.
- Network access on the first real run: the pinned models download from Hugging Face. Every model is pinned by its revision in `config/experiments/*.toml`.
- OpenAI runs only: `OPENAI_API_KEY` in the environment and `remote_allowed = true` in the corpus manifest. Both are absent here, so OpenAI evidence is outstanding.

## Tests

Run from `embeddings-fundamentals/`:

```bash
cargo fmt --check
for f in "" engine-embed-anything engine-fastembed engine-openai engine-embed-anything,engine-fastembed,engine-openai; do
  cargo clippy --all-targets ${f:+--features $f} -- -D warnings
done
cargo test                                   # unit, pipeline, experiment, combined, BDD (milestone 4), runner self-test
RUN_INTEGRATION=1 cargo test --release --features engine-embed-anything,engine-fastembed \
  --test integration -- --include-ignored    # real poppler/tesseract, real local models, the corpus tools
```

Local only, when the owner's real sample exams are present in `tests/sample-exams/` (git-ignored; never committed or sent anywhere, see `CLAUDE.md`):

```bash
RUN_SAMPLE_EXAMS=1 cargo test --release --test sample_exams -- --include-ignored
```

The BDD run writes `reports/bdd/cucumber.json`, which CI uploads as the `bdd-report` artifact. It fails unless every scenario that the PRD requires up to the milestone in `config/acceptance.toml` ran and passed.

## Reproducing the final comparison

These commands produce `reports/phase-4/`. Every run writes into one store. The combined runs search the revisions pinned by the snapshots of their text and image runs.

```bash
cargo build --release --features engine-embed-anything,engine-fastembed
B=target/release/embeddings-fundamentals
M=tests/fixtures/corpus/manifest.toml; S=store; O=reports/phase-4
FID="--fidelity config/fidelity/sample-synthetic-v1.toml"

# Text-only (Stages 1-2) and image-only (Stage 3)
$B experiment --manifest $M --config config/experiments/baseline-text-embed-anything.toml --store $S --experiment p4-embed-anything-minilm --report $O/p4-embed-anything-minilm.json $FID
$B experiment --manifest $M --config config/experiments/text-fastembed-minilm.toml       --store $S --experiment p4-fastembed-minilm      --report $O/p4-fastembed-minilm.json $FID
$B experiment --manifest $M --config config/experiments/text-fastembed-e5-base.toml      --store $S --experiment p4-fastembed-e5-base     --report $O/p4-fastembed-e5-base.json $FID
$B experiment --manifest $M --config config/experiments/image-clip-vit-b32.toml          --store $S --experiment p4-clip-vit-b32          --report $O/p4-clip-vit-b32.json
$B experiment --manifest $M --config config/experiments/image-siglip-multilingual.toml   --store $S --experiment p4-siglip-multilingual   --report $O/p4-siglip-multilingual.json

# Combined (Stage 4): RRF k = 60, fusion depth "all"
$B combine --manifest $M --config config/experiments/combined-e5-base-siglip.toml --store $S \
  --text-experiment p4-fastembed-e5-base --image-experiment p4-siglip-multilingual \
  --experiment p4-combined-e5-base-siglip --report $O/p4-combined-e5-base-siglip.json
$B combine --manifest $M --config config/experiments/combined-minilm-clip.toml --store $S \
  --text-experiment p4-embed-anything-minilm --image-experiment p4-clip-vit-b32 \
  --experiment p4-combined-minilm-clip --report $O/p4-combined-minilm-clip.json

# Comparison (the committed report adds the limitations listed at its end with --note)
$B report --result $O/p4-embed-anything-minilm.json --result $O/p4-fastembed-minilm.json \
  --result $O/p4-fastembed-e5-base.json --result $O/p4-clip-vit-b32.json --result $O/p4-siglip-multilingual.json \
  --combined $O/p4-combined-e5-base-siglip.json --combined $O/p4-combined-minilm-clip.json \
  --markdown $O/comparison.md --json $O/comparison.json --note "..."
```

Experiment ids are create-only, so a second run needs new ids or a fresh store. Retrieval metrics are deterministic for a given corpus, configuration, and model revision. The committed runs reproduced the Phase 2 and Phase 3 metrics exactly. Latency, cold start, and memory depend on the machine and its load, and each result records the hardware it ran on.

## Querying a reproduced index

```bash
# Text-only, image-only, and combined search of the pinned experiment snapshots
$B search --mode text     --config config/experiments/text-fastembed-e5-base.toml    --store $S --experiment p4-fastembed-e5-base       --query "Onde está o resultado da hemoglobina?"
$B search --mode image    --config config/experiments/image-siglip-multilingual.toml --store $S --experiment p4-siglip-multilingual     --query "Mostre a radiografia de tórax presente no conjunto de exames."
$B search --mode combined --config config/experiments/combined-e5-base-siglip.toml   --store $S --experiment p4-combined-e5-base-siglip --query "Mostre a radiografia de tórax presente no conjunto de exames."
```

A combined result carries its fused RRF score and its rank and cosine score in each leg (`fusion.text`, `fusion.image`). Raw scores of the two models are never compared.

## Running the owner corpus

[`../TESTS.md`](../TESTS.md) is the full guide: how to anonymize real PDFs and images, check them with `scripts/anonymization-verify.py`, and build the evaluable corpus. In short:

1. Put the anonymized files under `corpus/`, which git ignores except for the manifest. Draft the manifest with `$B draft-manifest --dir corpus --corpus-id <id> --out corpus/manifest.toml`. It holds checksums and measured input types, but no anonymization review.
2. Record each source's review (filename, embedded metadata, text, pixels, reviewer, date). Then run `$B anonymization-check --manifest corpus/manifest.toml` and `$B manifest --manifest corpus/manifest.toml`. Ineligible sources are refused before extraction.
3. Ingest pool runs, then draft the labels with `$B draft-labels --store <store> --corpus <id> --run <experiment>=<config> ...`. Judge every candidate, and write a fidelity sample in the shape of `config/fidelity/sample-synthetic-v1.toml`.
4. Run the commands above with `--manifest corpus/manifest.toml --labels <labels> --fidelity <sample>`.
