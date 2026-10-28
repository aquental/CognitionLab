# Indexed-record example (PRD §12 deliverable 4)

`record-001.json` is an actual record produced by
`cargo run --features engine-embed-anything -- ingest --manifest tests/fixtures/corpus/manifest.toml --config config/experiments/baseline-text-embed-anything.toml --store <dir> --experiment ea-baseline-1`.
The source is the **synthetic** fixture `lab-searchable-001` (invented content). No owner corpus was supplied, so this record is not evidence about real medical documents.

The record contains the complete 384-dimensional vector, normalized by the adapter, with model provenance:
- `sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2` at revision `e8f8c211…`
- EmbedAnything 0.7.1 (candle), safetensors weights
