# Phase 3 spike — text-to-image models through EmbedAnything 0.7.1

Date: 2026-10-28. Machine: Apple M3 Pro, CPU (candle), release build.
Spike crate: a throwaway binary using only `embed_anything::embeddings::embed::VisionEmbedder`
(`from_pretrained_hf`, `embed_image_batch`, `TextEmbed::embed`). Inputs: the synthetic
fixtures `chest-001.png` and `abd-us-001.jpg`. These images are synthetic, so the cosines
below are a load-and-wiring sanity check, **not** evidence of medical retrieval quality.

## Results

| Model | Pinned revision | Architecture | Load (cold download + init) | Image dims / norm | Text dims / norm |
|---|---|---|---|---|---|
| `openai/clip-vit-base-patch32` | `3d74acf9a28c67741b2f4f2ea7635f0aaf6f0268` | `CLIPModel` (weights from `pytorch_model.bin`; no safetensors at this commit) | 49.5 s | 512 / 1.0 | 512 / 1.0 |
| `google/siglip-base-patch16-256-multilingual` | `8952a4eafcde3cb7ab46b1dd629b33f8784ca9c6` | `SiglipModel` (`model.safetensors`, `tokenizer.json` present) | 65.7 s | 768 / 1.0 | 768 / 1.0 |

Cosine of each query against each image (higher ranks first):

| Query | CLIP chest | CLIP abd-us | SigLIP chest | SigLIP abd-us |
|---|---:|---:|---:|---:|
| Mostre a radiografia de tórax | 0.2249 | **0.2297** | **0.0532** | 0.0518 |
| Encontre as imagens de ultrassonografia abdominal | 0.2086 | **0.2389** | 0.0099 | **0.0596** |
| a chest x-ray | **0.2360** | 0.2289 | **0.0638** | 0.0545 |
| an abdominal ultrasound image | 0.2262 | **0.2478** | 0.0275 | **0.0668** |

CLIP ranks the wrong image first for the Portuguese chest query and the right one for the
English one. SigLIP-multilingual ranks the expected image first for all four, with small margins.

## Verdict

Both models **load and embed through the EmbedAnything Rust API**. The multilingual SigLIP
candidate therefore qualifies under PLAN §0.1 and is evaluated alongside the CLIP baseline.

## Defects found in EmbedAnything 0.7.1 (source read, `src/embeddings/local/clip.rs`)

1. **Wrong image preprocessing for both models.** `embed_image(_batch)` always resizes to
   `ClipConfig::vit_base_patch32().vision_config.image_size` (224) and maps pixels to
   `[-1, 1]` (`affine(2/255, -1)`). CLIP's `preprocessor_config.json` expects resize-shortest
   + center-crop 224 with CLIP mean/std; SigLIP-256 expects 256×256 with mean = std = 0.5.
   So CLIP gets SigLIP's normalization and SigLIP gets CLIP's size. The spike numbers above
   were produced with this preprocessing.
2. **Panics on long text.** `tokenize_sequences` computes `max_len - token_vec.len()` in
   `usize` (underflow panic when a query exceeds the position limit, 77 for CLIP, 64 for
   SigLIP) and calls `.unwrap()`; `embed_image_batch` unwraps image decoding and
   `fs::canonicalize`. A malformed image or long query panics inside the engine.

## Consequences for the Phase 3 adapter

- Do not use `EmbedImage::embed_image_batch`. Load weights with `ClipEmbedder::new` (pinned
  revision) and run `model.get_image_features` / `get_text_features` (public fields) on
  tensors the adapter builds itself from the model's pinned `preprocessor_config.json`
  (size, crop, mean, std, resample). Record the preprocessing in the space descriptor.
- Enforce the text budget with the pinned `tokenizer.json` before calling the engine
  (`max_input_tokens` = text `max_position_embeddings`), never rely on engine truncation.
- Run inference in `spawn_blocking` and treat a panic (`JoinError`) as a permanent
  embedding failure, never a crash.
- Re-run this table with the corrected preprocessing and record both sets of numbers.
