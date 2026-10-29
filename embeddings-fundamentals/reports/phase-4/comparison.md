# Retrieval comparison (end-to-end configuration comparison)

Configurations differ in engine, weight format, or model, so differences are not attributed to a single variable.

## Setup

- Hardware: macos aarch64 · CPU Apple M3 · 8 logical CPUs · 17179869184 bytes memory · accelerator cpu
- Corpus checksum: `sha256:0f808bc5723251bbe0f43da84de68bfa541075a6bb640e27719a123b12be74b6`
- Query set: queries-v1 (`sha256:0ca4735dc8b960715916fc29ac76d8de577465e6ef232e69116d6053638ebbed`)
- Labels: labels-synthetic-v4 (`sha256:2dd9b5f11bf30513e3717060e1e3f61f438736e74a724ec625fc6603f97a71ad`)

## Configurations

| Experiment | Engine | Model | Revision | Weights | Dims | Prefix | Batch | Space | Extraction fp | Chunking fp | Evaluation fp |
|---|---|---|---|---|---:|---|---:|---|---|---|---|
| p4-embed-anything-minilm | embed_anything 0.7.1 | sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2 | e8f8c211226b894fcb81acc59f3b34ba3efd5f42 | safetensors | 384 | None | 32 | `space-04233b7a97515dc7c052bb12db39a743` | `c82ad2eb3a9b` | `1be001b75489` | `484079928a35` |
| p4-fastembed-minilm | fastembed 7.5.0 | Xenova/paraphrase-multilingual-MiniLM-L12-v2 | 2c4055b12046f11709e9df2c122e59ffbdc2f900 | onnx | 384 | None | 32 | `space-25b9aeac30261d1226218758e6756264` | `c82ad2eb3a9b` | `6bd81d71d154` | `484079928a35` |
| p4-fastembed-e5-base | fastembed 7.5.0 | intfloat/multilingual-e5-base | d128750597153bb5987e10b1c3493a34e5a4502a | onnx | 768 | E5 | 32 | `space-20e1f35a45635362d2a1a1be05b34b34` | `c82ad2eb3a9b` | `ab9378502da8` | `484079928a35` |
| p4-clip-vit-b32 | embed_anything 0.7.1 | openai/clip-vit-base-patch32 | 3d74acf9a28c67741b2f4f2ea7635f0aaf6f0268 | candle | 512 | None | 16 | `space-6603884ac3fef0f888b11488a160b13c` | `8a4be74221f5` | `7d8dff06acd5` | `484079928a35` |
| p4-siglip-multilingual | embed_anything 0.7.1 | google/siglip-base-patch16-256-multilingual | 8952a4eafcde3cb7ab46b1dd629b33f8784ca9c6 | candle | 768 | None | 16 | `space-912b12ea1376ca71b44ae24b04fc91ca` | `8a4be74221f5` | `7d8dff06acd5` | `484079928a35` |

## Retrieval quality (k = 3, macro averages)

| Experiment | Mode | Ingestion run | Failed sources | Evaluation | Precision@3 | Hit@3 | nDCG@3 |
|---|---|---|---:|---|---:|---:|---:|
| p4-embed-anything-minilm | text-only | succeeded | 0 | Completed | 0.3333 | 0.5000 | 0.4093 |
| p4-fastembed-minilm | text-only | succeeded | 0 | Completed | 0.3333 | 0.5000 | 0.4093 |
| p4-fastembed-e5-base | text-only | succeeded | 0 | Completed | 0.4333 | 0.7000 | 0.5399 |
| p4-clip-vit-b32 | image-only | succeeded | 0 | Completed | 0.1667 | 0.3000 | 0.2407 |
| p4-siglip-multilingual | image-only | succeeded | 0 | Completed | 0.1667 | 0.3000 | 0.2704 |

## Combined configurations

| Experiment | Text leg (run / model / space) | Image leg (run / model / space) | Fusion | k | Depth |
|---|---|---|---|---:|---|
| p4-combined-e5-base-siglip | p4-fastembed-e5-base / fastembed intfloat/multilingual-e5-base / `space-20e1f35a45635362d2a1a1be05b34b34` | p4-siglip-multilingual / embed_anything google/siglip-base-patch16-256-multilingual / `space-912b12ea1376ca71b44ae24b04fc91ca` | rrf | 60 | all |
| p4-combined-minilm-clip | p4-embed-anything-minilm / embed_anything sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2 / `space-04233b7a97515dc7c052bb12db39a743` | p4-clip-vit-b32 / embed_anything openai/clip-vit-base-patch32 / `space-6603884ac3fef0f888b11488a160b13c` | rrf | 60 | all |

## Retrieval modes compared

| Experiment | Mode | Model(s) | Precision@3 | Hit@3 | nDCG@3 | Warm median ms | Warm p95 ms | Index size | Query-set cost |
|---|---|---|---:|---:|---:|---:|---:|---|---|
| p4-embed-anything-minilm | text-only | sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2 | 0.3333 | 0.5000 | 0.4093 | 15.61 | 17.10 | 165702 bytes | NotApplicable: local engine: no provider is billed |
| p4-fastembed-minilm | text-only | Xenova/paraphrase-multilingual-MiniLM-L12-v2 | 0.3333 | 0.5000 | 0.4093 | 32.27 | 36.68 | 164840 bytes | NotApplicable: local engine: no provider is billed |
| p4-fastembed-e5-base | text-only | intfloat/multilingual-e5-base | 0.4333 | 0.7000 | 0.5399 | 61.49 | 65.77 | 298563 bytes | NotApplicable: local engine: no provider is billed |
| p4-clip-vit-b32 | image-only | openai/clip-vit-base-patch32 | 0.1667 | 0.3000 | 0.2407 | 34.61 | 37.13 | 23281 bytes | NotApplicable: local engine: no provider is billed |
| p4-siglip-multilingual | image-only | google/siglip-base-patch16-256-multilingual | 0.1667 | 0.3000 | 0.2704 | 62.85 | 78.49 | 33100 bytes | NotApplicable: local engine: no provider is billed |
| p4-combined-e5-base-siglip | combined | intfloat/multilingual-e5-base + google/siglip-base-patch16-256-multilingual | 0.4000 | 0.8000 | 0.5453 | 94.64 | 104.41 | 331663 bytes | text NotApplicable: local engine: no provider is billed; image NotApplicable: local engine: no provider is billed |
| p4-combined-minilm-clip | combined | sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2 + openai/clip-vit-base-patch32 | 0.2667 | 0.6000 | 0.3399 | 41.63 | 43.91 | 188983 bytes | text NotApplicable: local engine: no provider is billed; image NotApplicable: local engine: no provider is billed |

## Combined retrieval against its legs

Quality change relative to each leg run alone, and the latency the second leg and the fusion add (PRD §6.6: quality improvement relative to added cost). A leg whose run is not in this report shows `-`.

| Combined | Leg | Leg run | ΔPrecision@3 | ΔHit@3 | ΔnDCG@3 | Warm median ms (leg → combined) |
|---|---|---|---:|---:|---:|---|
| p4-combined-e5-base-siglip | text | p4-fastembed-e5-base | -0.0333 | +0.1000 | +0.0054 | 61.49 → 94.64 |
| p4-combined-e5-base-siglip | image | p4-siglip-multilingual | +0.2333 | +0.5000 | +0.2749 | 62.85 → 94.64 |
| p4-combined-minilm-clip | text | p4-embed-anything-minilm | -0.0667 | +0.1000 | -0.0694 | 15.61 → 41.63 |
| p4-combined-minilm-clip | image | p4-clip-vit-b32 | +0.1000 | +0.3000 | +0.0992 | 34.61 → 41.63 |

## Combined efficiency

A combined run ingests nothing: it searches the revisions its legs' runs pinned. Its cold start loads both query models in a fresh process.

| Experiment | Text model load ms | Image model load ms | First fused query ms | Cold start ms | Warm median ms | Warm p95 ms | Peak local RAM | Index size | GPU memory |
|---|---:|---:|---:|---:|---:|---:|---|---|---|
| p4-combined-e5-base-siglip | 7359.5 | 2687.0 | 87.5 | 10134.0 | 94.64 | 104.41 | 6357270528 bytes | 331663 bytes | unavailable (inference runs on the local CPU; no GPU is used) |
| p4-combined-minilm-clip | 1090.7 | 758.4 | 37.0 | 1886.1 | 41.63 | 43.91 | 1876180992 bytes | 188983 bytes | unavailable (inference runs on the local CPU; no GPU is used) |

## Efficiency

| Experiment | Ingestion ms | ms/page | chunks/s | Batch latency ms | Embeds/s | Model load ms | Cold start ms | Warm median ms | Warm p95 ms | Peak local RAM | Index size | GPU memory |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---|---|
| p4-embed-anything-minilm | 2769.8 | 461.6 | 10.1 | 134.50 | 69.4 | 455.4 | 584.4 | 15.61 | 17.10 | 1672593408 bytes | 165702 bytes | unavailable (inference runs on the local CPU; no GPU is used) |
| p4-fastembed-minilm | 2750.3 | 458.4 | 10.2 | 126.53 | 73.8 | 2386.9 | 2530.4 | 32.27 | 36.68 | 5566349312 bytes | 164840 bytes | unavailable (inference runs on the local CPU; no GPU is used) |
| p4-fastembed-e5-base | 3344.8 | 557.5 | 8.4 | 312.47 | 29.9 | 8382.8 | 8743.1 | 61.49 | 65.77 | 6588727296 bytes | 298563 bytes | unavailable (inference runs on the local CPU; no GPU is used) |
| p4-clip-vit-b32 | 345.4 | - | 8.7 | 50.35 | 19.9 | 808.2 | 860.1 | 34.61 | 37.13 | 699990016 bytes | 23281 bytes | unavailable (inference runs on the local CPU; no GPU is used) |
| p4-siglip-multilingual | 857.9 | - | 3.5 | 228.21 | 4.4 | 2587.7 | 2812.9 | 62.85 | 78.49 | 3550543872 bytes | 33100 bytes | unavailable (inference runs on the local CPU; no GPU is used) |

## Cost and reliability

| Experiment | Ingestion cost | Query-set cost | Provider memory | Failed pages | Failed batches | Retries | Truncations |
|---|---|---|---|---:|---:|---:|---:|
| p4-embed-anything-minilm | NotApplicable: local engine: no provider is billed | NotApplicable: local engine: no provider is billed | local (see peak RAM) | 0 | 0 | 0 | 0 |
| p4-fastembed-minilm | NotApplicable: local engine: no provider is billed | NotApplicable: local engine: no provider is billed | local (see peak RAM) | 0 | 0 | 0 | 0 |
| p4-fastembed-e5-base | NotApplicable: local engine: no provider is billed | NotApplicable: local engine: no provider is billed | local (see peak RAM) | 0 | 0 | 0 | 0 |
| p4-clip-vit-b32 | NotApplicable: local engine: no provider is billed | NotApplicable: local engine: no provider is billed | local (see peak RAM) | 0 | 0 | 0 | 0 |
| p4-siglip-multilingual | NotApplicable: local engine: no provider is billed | NotApplicable: local engine: no provider is billed | local (see peak RAM) | 0 | 0 | 0 | 0 |

## Extraction fidelity

| Experiment | Sample | Overall | Test name | Value | Unit | Reference range | Native | OCR |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| p4-embed-anything-minilm | fidelity-synthetic-v1 | 0.844 | 0.875 | 1.000 | 0.875 | 0.625 | 1.000 | 0.688 |
| p4-fastembed-minilm | fidelity-synthetic-v1 | 0.844 | 0.875 | 1.000 | 0.875 | 0.625 | 1.000 | 0.688 |
| p4-fastembed-e5-base | fidelity-synthetic-v1 | 0.844 | 0.875 | 1.000 | 0.875 | 0.625 | 1.000 | 0.688 |

## Top-3 results per query

### p4-embed-anything-minilm (embed_anything / sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2)

| Query | Rank | Retrieved item | Source/page | Score | Relevance | Note |
|---|---:|---|---|---:|---:|---|
| Onde está o resultado da hemoglobina? | 1 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Hemoglobina \| 13,5 \| g/dL \| 12,0-16,0 | lab-scanned-002 / 1 | 0.7309 | 2 | Hemoglobin row recovered by OCR |
| Onde está o resultado da hemoglobina? | 2 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Hemoglobina \| 13,5 \| g/dL \| 12,0–16,0 | lab-searchable-001 / 1 | 0.7255 | 2 | Hemoglobin row with value, unit, and reference range (native text) |
| Onde está o resultado da hemoglobina? | 3 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Hematocrito \| 40,1 \| % \| 36,0-46,0 | lab-scanned-002 / 1 | 0.5886 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual é o valor da glicemia e sua unidade? | 1 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Glicemia de jejum \| 92 \| mg/dL \| 70–99 | lab-searchable-001 / 2 | 0.6498 | 2 | Fasting glucose row with value 92 mg/dL |
| Qual é o valor da glicemia e sua unidade? | 2 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Glicemia de jejum \| 92 \| mg/dL \| 70-99 | lab-scanned-002 / 2 | 0.6495 | 2 | Fasting glucose row recovered by OCR |
| Qual é o valor da glicemia e sua unidade? | 3 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Colesterol total \| 182 \| mg/dL \| < 190 | lab-searchable-001 / 2 | 0.5509 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual intervalo de referência aparece para a creatinina? | 1 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Creatinina \| 0,9 \| mg/dL \| 0,6-1,1 | lab-scanned-002 / 2 | 0.7640 | 2 | Creatinine row recovered by OCR |
| Qual intervalo de referência aparece para a creatinina? | 2 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Creatinina \| 0,9 \| mg/dL \| 0,6–1,1 | lab-searchable-001 / 2 | 0.7630 | 2 | Creatinine row with reference range 0,6–1,1 |
| Qual intervalo de referência aparece para a creatinina? | 3 | Bioquímica / Conclusão: valores dentro dos intervalos de referência. | lab-searchable-001 / 2 | 0.5252 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde estão os resultados de colesterol e triglicerídeos? | 1 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Colesterol total \| 182 \| mg/dL \| < 190 | lab-searchable-001 / 2 | 0.6704 | 2 | Total cholesterol row |
| Onde estão os resultados de colesterol e triglicerídeos? | 2 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Colesterol total \| 182 \| mg/dL \| < 190 | lab-scanned-002 / 2 | 0.6704 | 2 | Total cholesterol row recovered by OCR |
| Onde estão os resultados de colesterol e triglicerídeos? | 3 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Triglicerideos \| 120 \| mg/dL \| < 150 | lab-scanned-002 / 2 | 0.5725 | 2 | Triglycerides row recovered by OCR |
| Qual página contém a conclusão do laudo? | 1 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Plaquetas \| 250.000 \| /mm³ \| 150.000–450.000 | lab-searchable-001 / 1 | 0.3504 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual página contém a conclusão do laudo? | 2 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Plaquetas \| 250.000 \| /mm? \| 150.000-450.000 | lab-scanned-002 / 1 | 0.3442 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual página contém a conclusão do laudo? | 3 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Hematócrito \| 40,1 \| % \| 36,0–46,0 | lab-searchable-001 / 1 | 0.3423 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o laudo descreve os achados do exame? | 1 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Hematócrito \| 40,1 \| % \| 36,0–46,0 | lab-searchable-001 / 1 | 0.5559 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o laudo descreve os achados do exame? | 2 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Plaquetas \| 250.000 \| /mm³ \| 150.000–450.000 | lab-searchable-001 / 1 | 0.5380 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o laudo descreve os achados do exame? | 3 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Plaquetas \| 250.000 \| /mm? \| 150.000-450.000 | lab-scanned-002 / 1 | 0.5337 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a radiografia de tórax presente no conjunto de exames. | 1 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Plaquetas \| 250.000 \| /mm³ \| 150.000–450.000 | lab-searchable-001 / 1 | 0.5424 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a radiografia de tórax presente no conjunto de exames. | 2 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Plaquetas \| 250.000 \| /mm? \| 150.000-450.000 | lab-scanned-002 / 1 | 0.5204 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a radiografia de tórax presente no conjunto de exames. | 3 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Leucócitos \| 6.800 \| /mms \| 4.000-10.000 | lab-scanned-002 / 1 | 0.4917 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Encontre as imagens de ultrassonografia abdominal. | 1 | Conclusão / Ultrassonografia abdominal sem alterações significativas. | us-mixed-003 / 2 | 0.8481 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Encontre as imagens de ultrassonografia abdominal. | 2 | Conclusão / Ultrassonografia abdominal sem alterações significativas. | us-mixed-003 / 1 | 0.8481 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Encontre as imagens de ultrassonografia abdominal. | 3 | LAUDO DE ULTRASSONOGRAFIA ABDOMINAL - SINTETICO / Paciente: [ANONIMIZADO] | us-mixed-003 / 1 | 0.6548 | 1 | Title of the abdominal ultrasound report, not an image |
| Mostre a imagem associada a este laudo de ultrassonografia. | 1 | Conclusão / Ultrassonografia abdominal sem alterações significativas. | us-mixed-003 / 2 | 0.6253 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a imagem associada a este laudo de ultrassonografia. | 2 | Conclusão / Ultrassonografia abdominal sem alterações significativas. | us-mixed-003 / 1 | 0.6253 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a imagem associada a este laudo de ultrassonografia. | 3 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Leucócitos \| 6.800 \| /mm³ \| 4.000–10.000 | lab-searchable-001 / 1 | 0.5172 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o documento informa a técnica utilizada no exame? | 1 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Plaquetas \| 250.000 \| /mm³ \| 150.000–450.000 | lab-searchable-001 / 1 | 0.4753 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o documento informa a técnica utilizada no exame? | 2 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Plaquetas \| 250.000 \| /mm? \| 150.000-450.000 | lab-scanned-002 / 1 | 0.4705 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o documento informa a técnica utilizada no exame? | 3 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Hematócrito \| 40,1 \| % \| 36,0–46,0 | lab-searchable-001 / 1 | 0.4572 | 0 | unlabeled: not judged relevant for this query (label 0) |

### p4-fastembed-minilm (fastembed / Xenova/paraphrase-multilingual-MiniLM-L12-v2)

| Query | Rank | Retrieved item | Source/page | Score | Relevance | Note |
|---|---:|---|---|---:|---:|---|
| Onde está o resultado da hemoglobina? | 1 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Hemoglobina \| 13,5 \| g/dL \| 12,0-16,0 | lab-scanned-002 / 1 | 0.7309 | 2 | Hemoglobin row recovered by OCR |
| Onde está o resultado da hemoglobina? | 2 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Hemoglobina \| 13,5 \| g/dL \| 12,0–16,0 | lab-searchable-001 / 1 | 0.7255 | 2 | Hemoglobin row with value, unit, and reference range (native text) |
| Onde está o resultado da hemoglobina? | 3 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Hematocrito \| 40,1 \| % \| 36,0-46,0 | lab-scanned-002 / 1 | 0.5886 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual é o valor da glicemia e sua unidade? | 1 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Glicemia de jejum \| 92 \| mg/dL \| 70–99 | lab-searchable-001 / 2 | 0.6498 | 2 | Fasting glucose row with value 92 mg/dL |
| Qual é o valor da glicemia e sua unidade? | 2 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Glicemia de jejum \| 92 \| mg/dL \| 70-99 | lab-scanned-002 / 2 | 0.6495 | 2 | Fasting glucose row recovered by OCR |
| Qual é o valor da glicemia e sua unidade? | 3 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Colesterol total \| 182 \| mg/dL \| < 190 | lab-searchable-001 / 2 | 0.5509 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual intervalo de referência aparece para a creatinina? | 1 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Creatinina \| 0,9 \| mg/dL \| 0,6-1,1 | lab-scanned-002 / 2 | 0.7640 | 2 | Creatinine row recovered by OCR |
| Qual intervalo de referência aparece para a creatinina? | 2 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Creatinina \| 0,9 \| mg/dL \| 0,6–1,1 | lab-searchable-001 / 2 | 0.7630 | 2 | Creatinine row with reference range 0,6–1,1 |
| Qual intervalo de referência aparece para a creatinina? | 3 | Bioquímica / Conclusão: valores dentro dos intervalos de referência. | lab-scanned-002 / 2 | 0.5252 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde estão os resultados de colesterol e triglicerídeos? | 1 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Colesterol total \| 182 \| mg/dL \| < 190 | lab-searchable-001 / 2 | 0.6704 | 2 | Total cholesterol row |
| Onde estão os resultados de colesterol e triglicerídeos? | 2 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Colesterol total \| 182 \| mg/dL \| < 190 | lab-scanned-002 / 2 | 0.6704 | 2 | Total cholesterol row recovered by OCR |
| Onde estão os resultados de colesterol e triglicerídeos? | 3 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Triglicerideos \| 120 \| mg/dL \| < 150 | lab-scanned-002 / 2 | 0.5725 | 2 | Triglycerides row recovered by OCR |
| Qual página contém a conclusão do laudo? | 1 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Plaquetas \| 250.000 \| /mm³ \| 150.000–450.000 | lab-searchable-001 / 1 | 0.3504 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual página contém a conclusão do laudo? | 2 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Plaquetas \| 250.000 \| /mm? \| 150.000-450.000 | lab-scanned-002 / 1 | 0.3442 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual página contém a conclusão do laudo? | 3 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Hematócrito \| 40,1 \| % \| 36,0–46,0 | lab-searchable-001 / 1 | 0.3423 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o laudo descreve os achados do exame? | 1 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Hematócrito \| 40,1 \| % \| 36,0–46,0 | lab-searchable-001 / 1 | 0.5559 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o laudo descreve os achados do exame? | 2 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Plaquetas \| 250.000 \| /mm³ \| 150.000–450.000 | lab-searchable-001 / 1 | 0.5380 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o laudo descreve os achados do exame? | 3 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Plaquetas \| 250.000 \| /mm? \| 150.000-450.000 | lab-scanned-002 / 1 | 0.5337 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a radiografia de tórax presente no conjunto de exames. | 1 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Plaquetas \| 250.000 \| /mm³ \| 150.000–450.000 | lab-searchable-001 / 1 | 0.5424 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a radiografia de tórax presente no conjunto de exames. | 2 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Plaquetas \| 250.000 \| /mm? \| 150.000-450.000 | lab-scanned-002 / 1 | 0.5204 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a radiografia de tórax presente no conjunto de exames. | 3 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Leucócitos \| 6.800 \| /mms \| 4.000-10.000 | lab-scanned-002 / 1 | 0.4917 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Encontre as imagens de ultrassonografia abdominal. | 1 | Conclusão / Ultrassonografia abdominal sem alterações significativas. | us-mixed-003 / 2 | 0.8481 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Encontre as imagens de ultrassonografia abdominal. | 2 | Conclusão / Ultrassonografia abdominal sem alterações significativas. | us-mixed-003 / 1 | 0.8481 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Encontre as imagens de ultrassonografia abdominal. | 3 | LAUDO DE ULTRASSONOGRAFIA ABDOMINAL  - SINTETICO / Paciente: [ANONIMIZADO] | us-mixed-003 / 2 | 0.6548 | 1 | Title of the abdominal ultrasound report repeated on page 2, not an image |
| Mostre a imagem associada a este laudo de ultrassonografia. | 1 | Conclusão / Ultrassonografia abdominal sem alterações significativas. | us-mixed-003 / 2 | 0.6253 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a imagem associada a este laudo de ultrassonografia. | 2 | Conclusão / Ultrassonografia abdominal sem alterações significativas. | us-mixed-003 / 1 | 0.6253 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a imagem associada a este laudo de ultrassonografia. | 3 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Leucócitos \| 6.800 \| /mm³ \| 4.000–10.000 | lab-searchable-001 / 1 | 0.5172 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o documento informa a técnica utilizada no exame? | 1 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Plaquetas \| 250.000 \| /mm³ \| 150.000–450.000 | lab-searchable-001 / 1 | 0.4753 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o documento informa a técnica utilizada no exame? | 2 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Plaquetas \| 250.000 \| /mm? \| 150.000-450.000 | lab-scanned-002 / 1 | 0.4705 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o documento informa a técnica utilizada no exame? | 3 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Hematócrito \| 40,1 \| % \| 36,0–46,0 | lab-searchable-001 / 1 | 0.4572 | 0 | unlabeled: not judged relevant for this query (label 0) |

### p4-fastembed-e5-base (fastembed / intfloat/multilingual-e5-base)

| Query | Rank | Retrieved item | Source/page | Score | Relevance | Note |
|---|---:|---|---|---:|---:|---|
| Onde está o resultado da hemoglobina? | 1 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Hemoglobina \| 13,5 \| g/dL \| 12,0–16,0 | lab-searchable-001 / 1 | 0.8817 | 2 | Hemoglobin row with value, unit, and reference range (native text) |
| Onde está o resultado da hemoglobina? | 2 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Hemoglobina \| 13,5 \| g/dL \| 12,0-16,0 | lab-scanned-002 / 1 | 0.8804 | 2 | Hemoglobin row recovered by OCR |
| Onde está o resultado da hemoglobina? | 3 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Hematocrito \| 40,1 \| % \| 36,0-46,0 | lab-scanned-002 / 1 | 0.8622 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual é o valor da glicemia e sua unidade? | 1 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Glicemia de jejum \| 92 \| mg/dL \| 70–99 | lab-searchable-001 / 2 | 0.8709 | 2 | Fasting glucose row with value 92 mg/dL |
| Qual é o valor da glicemia e sua unidade? | 2 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Glicemia de jejum \| 92 \| mg/dL \| 70-99 | lab-scanned-002 / 2 | 0.8691 | 2 | Fasting glucose row recovered by OCR |
| Qual é o valor da glicemia e sua unidade? | 3 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Triglicerídeos \| 120 \| mg/dL \| < 150 | lab-searchable-001 / 2 | 0.8644 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual intervalo de referência aparece para a creatinina? | 1 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Creatinina \| 0,9 \| mg/dL \| 0,6–1,1 | lab-searchable-001 / 2 | 0.8849 | 2 | Creatinine row with reference range 0,6–1,1 |
| Qual intervalo de referência aparece para a creatinina? | 2 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Creatinina \| 0,9 \| mg/dL \| 0,6-1,1 | lab-scanned-002 / 2 | 0.8817 | 2 | Creatinine row recovered by OCR |
| Qual intervalo de referência aparece para a creatinina? | 3 | Bioquímica / Conclusão: valores dentro dos intervalos de referência. | lab-searchable-001 / 2 | 0.8498 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde estão os resultados de colesterol e triglicerídeos? | 1 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Triglicerideos \| 120 \| mg/dL \| < 150 | lab-scanned-002 / 2 | 0.8625 | 2 | Triglycerides row recovered by OCR |
| Onde estão os resultados de colesterol e triglicerídeos? | 2 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Triglicerídeos \| 120 \| mg/dL \| < 150 | lab-searchable-001 / 2 | 0.8600 | 2 | Triglycerides row |
| Onde estão os resultados de colesterol e triglicerídeos? | 3 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Colesterol total \| 182 \| mg/dL \| < 190 | lab-scanned-002 / 2 | 0.8459 | 2 | Total cholesterol row recovered by OCR |
| Qual página contém a conclusão do laudo? | 1 | LABORATORIO FICTICIO DE ANALISES - LAUDO SINTETICO / Paciente: [ANONIMIZADO] Registro: [ANONIMIZADO] | lab-searchable-001 / 1 | 0.7999 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual página contém a conclusão do laudo? | 2 | LABORATORIO FICTICIO DE ANALISES  - LAUDO SINTETICO / Paciente: [ANONIMIZADO]  Registro: [ANONIMIZADO] | lab-scanned-002 / 1 | 0.7999 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual página contém a conclusão do laudo? | 3 | LAUDO DE ULTRASSONOGRAFIA ABDOMINAL - SINTETICO / Paciente: [ANONIMIZADO] | us-mixed-003 / 1 | 0.7936 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o laudo descreve os achados do exame? | 1 | LABORATORIO FICTICIO DE ANALISES - LAUDO SINTETICO / Paciente: [ANONIMIZADO] Registro: [ANONIMIZADO] | lab-searchable-001 / 1 | 0.8194 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o laudo descreve os achados do exame? | 2 | LABORATORIO FICTICIO DE ANALISES  - LAUDO SINTETICO / Paciente: [ANONIMIZADO]  Registro: [ANONIMIZADO] | lab-scanned-002 / 1 | 0.8194 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o laudo descreve os achados do exame? | 3 | Achados / Fígado de dimensões normais, contornos regulares e ecotextura homogênea. / Vesícula biliar normodistendida, paredes finas, sem cálculos. | us-mixed-003 / 1 | 0.8148 | 2 | Findings section of the ultrasound report |
| Mostre a radiografia de tórax presente no conjunto de exames. | 1 | LABORATORIO FICTICIO DE ANALISES - LAUDO SINTETICO / Paciente: [ANONIMIZADO] Registro: [ANONIMIZADO] | lab-searchable-001 / 1 | 0.8481 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a radiografia de tórax presente no conjunto de exames. | 2 | LABORATORIO FICTICIO DE ANALISES  - LAUDO SINTETICO / Paciente: [ANONIMIZADO]  Registro: [ANONIMIZADO] | lab-scanned-002 / 1 | 0.8481 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a radiografia de tórax presente no conjunto de exames. | 3 | LAUDO DE ULTRASSONOGRAFIA ABDOMINAL - SINTETICO / Paciente: [ANONIMIZADO] | us-mixed-003 / 1 | 0.8439 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Encontre as imagens de ultrassonografia abdominal. | 1 | Conclusão / Ultrassonografia abdominal sem alterações significativas. | us-mixed-003 / 2 | 0.8726 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Encontre as imagens de ultrassonografia abdominal. | 2 | Conclusão / Ultrassonografia abdominal sem alterações significativas. | us-mixed-003 / 1 | 0.8726 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Encontre as imagens de ultrassonografia abdominal. | 3 | LAUDO DE ULTRASSONOGRAFIA ABDOMINAL - SINTETICO / Paciente: [ANONIMIZADO] | us-mixed-003 / 1 | 0.8645 | 1 | Title of the abdominal ultrasound report, not an image |
| Mostre a imagem associada a este laudo de ultrassonografia. | 1 | LAUDO DE ULTRASSONOGRAFIA ABDOMINAL - SINTETICO / Paciente: [ANONIMIZADO] | us-mixed-003 / 1 | 0.8664 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a imagem associada a este laudo de ultrassonografia. | 2 | LAUDO DE ULTRASSONOGRAFIA ABDOMINAL  - SINTETICO / Paciente: [ANONIMIZADO] | us-mixed-003 / 2 | 0.8664 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a imagem associada a este laudo de ultrassonografia. | 3 | LABORATORIO FICTICIO DE ANALISES - LAUDO SINTETICO / Paciente: [ANONIMIZADO] Registro: [ANONIMIZADO] | lab-searchable-001 / 1 | 0.8536 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o documento informa a técnica utilizada no exame? | 1 | Técnica / Exame realizado com transdutor convexo de 3,5 MHz, em jejum de 8 horas. | us-mixed-003 / 1 | 0.8404 | 2 | Technique section (transducer and preparation) |
| Onde o documento informa a técnica utilizada no exame? | 2 | Técnica / Exame realizado com transdutor convexo de 3,5 MHz, em jejum de 8 horas. | us-mixed-003 / 2 | 0.8404 | 2 | Technique section (OCR page) |
| Onde o documento informa a técnica utilizada no exame? | 3 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Plaquetas \| 250.000 \| /mm? \| 150.000-450.000 | lab-scanned-002 / 1 | 0.8221 | 0 | unlabeled: not judged relevant for this query (label 0) |

### p4-clip-vit-b32 (embed_anything / openai/clip-vit-base-patch32)

| Query | Rank | Retrieved item | Source/page | Score | Relevance | Note |
|---|---:|---|---|---:|---:|---|
| Onde está o resultado da hemoglobina? | 1 | assets/70b1a16a5cdbaa3be0bd2a4bad0e2e56f0fbfb30a5a9259452005c0e0f9c42d5.png | us-mixed-003 / 1 | 0.2305 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde está o resultado da hemoglobina? | 2 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.2288 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde está o resultado da hemoglobina? | 3 | assets/6737f7fae792224f5ccd27310a22c6f79a6d9a7d469ba0756a73223f30b8c7d5.png | chest-001 / 1 | 0.2078 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual é o valor da glicemia e sua unidade? | 1 | assets/70b1a16a5cdbaa3be0bd2a4bad0e2e56f0fbfb30a5a9259452005c0e0f9c42d5.png | us-mixed-003 / 1 | 0.2273 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual é o valor da glicemia e sua unidade? | 2 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.2249 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual é o valor da glicemia e sua unidade? | 3 | assets/6737f7fae792224f5ccd27310a22c6f79a6d9a7d469ba0756a73223f30b8c7d5.png | chest-001 / 1 | 0.2105 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual intervalo de referência aparece para a creatinina? | 1 | assets/6737f7fae792224f5ccd27310a22c6f79a6d9a7d469ba0756a73223f30b8c7d5.png | chest-001 / 1 | 0.2272 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual intervalo de referência aparece para a creatinina? | 2 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.2193 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual intervalo de referência aparece para a creatinina? | 3 | assets/70b1a16a5cdbaa3be0bd2a4bad0e2e56f0fbfb30a5a9259452005c0e0f9c42d5.png | us-mixed-003 / 1 | 0.2178 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde estão os resultados de colesterol e triglicerídeos? | 1 | assets/70b1a16a5cdbaa3be0bd2a4bad0e2e56f0fbfb30a5a9259452005c0e0f9c42d5.png | us-mixed-003 / 1 | 0.2289 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde estão os resultados de colesterol e triglicerídeos? | 2 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.2278 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde estão os resultados de colesterol e triglicerídeos? | 3 | assets/6737f7fae792224f5ccd27310a22c6f79a6d9a7d469ba0756a73223f30b8c7d5.png | chest-001 / 1 | 0.2116 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual página contém a conclusão do laudo? | 1 | assets/6737f7fae792224f5ccd27310a22c6f79a6d9a7d469ba0756a73223f30b8c7d5.png | chest-001 / 1 | 0.2223 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual página contém a conclusão do laudo? | 2 | assets/70b1a16a5cdbaa3be0bd2a4bad0e2e56f0fbfb30a5a9259452005c0e0f9c42d5.png | us-mixed-003 / 1 | 0.2028 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual página contém a conclusão do laudo? | 3 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.2023 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o laudo descreve os achados do exame? | 1 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.2183 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o laudo descreve os achados do exame? | 2 | assets/70b1a16a5cdbaa3be0bd2a4bad0e2e56f0fbfb30a5a9259452005c0e0f9c42d5.png | us-mixed-003 / 1 | 0.2178 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o laudo descreve os achados do exame? | 3 | assets/6737f7fae792224f5ccd27310a22c6f79a6d9a7d469ba0756a73223f30b8c7d5.png | chest-001 / 1 | 0.2041 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a radiografia de tórax presente no conjunto de exames. | 1 | assets/70b1a16a5cdbaa3be0bd2a4bad0e2e56f0fbfb30a5a9259452005c0e0f9c42d5.png | us-mixed-003 / 1 | 0.2190 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a radiografia de tórax presente no conjunto de exames. | 2 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.2167 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a radiografia de tórax presente no conjunto de exames. | 3 | assets/6737f7fae792224f5ccd27310a22c6f79a6d9a7d469ba0756a73223f30b8c7d5.png | chest-001 / 1 | 0.1987 | 2 | Synthetic chest radiograph |
| Encontre as imagens de ultrassonografia abdominal. | 1 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.2381 | 2 | Synthetic abdominal ultrasound image |
| Encontre as imagens de ultrassonografia abdominal. | 2 | assets/70b1a16a5cdbaa3be0bd2a4bad0e2e56f0fbfb30a5a9259452005c0e0f9c42d5.png | us-mixed-003 / 1 | 0.2378 | 2 | Ultrasound image embedded in the ultrasound report |
| Encontre as imagens de ultrassonografia abdominal. | 3 | assets/6737f7fae792224f5ccd27310a22c6f79a6d9a7d469ba0756a73223f30b8c7d5.png | chest-001 / 1 | 0.2110 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a imagem associada a este laudo de ultrassonografia. | 1 | assets/70b1a16a5cdbaa3be0bd2a4bad0e2e56f0fbfb30a5a9259452005c0e0f9c42d5.png | us-mixed-003 / 1 | 0.2247 | 2 | Image embedded in the ultrasound report |
| Mostre a imagem associada a este laudo de ultrassonografia. | 2 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.2219 | 1 | Abdominal ultrasound image not linked to the report |
| Mostre a imagem associada a este laudo de ultrassonografia. | 3 | assets/6737f7fae792224f5ccd27310a22c6f79a6d9a7d469ba0756a73223f30b8c7d5.png | chest-001 / 1 | 0.2168 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o documento informa a técnica utilizada no exame? | 1 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.2197 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o documento informa a técnica utilizada no exame? | 2 | assets/70b1a16a5cdbaa3be0bd2a4bad0e2e56f0fbfb30a5a9259452005c0e0f9c42d5.png | us-mixed-003 / 1 | 0.2187 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o documento informa a técnica utilizada no exame? | 3 | assets/6737f7fae792224f5ccd27310a22c6f79a6d9a7d469ba0756a73223f30b8c7d5.png | chest-001 / 1 | 0.2116 | 0 | unlabeled: not judged relevant for this query (label 0) |

### p4-siglip-multilingual (embed_anything / google/siglip-base-patch16-256-multilingual)

| Query | Rank | Retrieved item | Source/page | Score | Relevance | Note |
|---|---:|---|---|---:|---:|---|
| Onde está o resultado da hemoglobina? | 1 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.0159 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde está o resultado da hemoglobina? | 2 | assets/70b1a16a5cdbaa3be0bd2a4bad0e2e56f0fbfb30a5a9259452005c0e0f9c42d5.png | us-mixed-003 / 1 | 0.0105 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde está o resultado da hemoglobina? | 3 | assets/6737f7fae792224f5ccd27310a22c6f79a6d9a7d469ba0756a73223f30b8c7d5.png | chest-001 / 1 | -0.0019 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual é o valor da glicemia e sua unidade? | 1 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | -0.0030 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual é o valor da glicemia e sua unidade? | 2 | assets/70b1a16a5cdbaa3be0bd2a4bad0e2e56f0fbfb30a5a9259452005c0e0f9c42d5.png | us-mixed-003 / 1 | -0.0094 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual é o valor da glicemia e sua unidade? | 3 | assets/6737f7fae792224f5ccd27310a22c6f79a6d9a7d469ba0756a73223f30b8c7d5.png | chest-001 / 1 | -0.0213 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual intervalo de referência aparece para a creatinina? | 1 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.0038 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual intervalo de referência aparece para a creatinina? | 2 | assets/70b1a16a5cdbaa3be0bd2a4bad0e2e56f0fbfb30a5a9259452005c0e0f9c42d5.png | us-mixed-003 / 1 | 0.0022 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual intervalo de referência aparece para a creatinina? | 3 | assets/6737f7fae792224f5ccd27310a22c6f79a6d9a7d469ba0756a73223f30b8c7d5.png | chest-001 / 1 | -0.0035 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde estão os resultados de colesterol e triglicerídeos? | 1 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.0119 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde estão os resultados de colesterol e triglicerídeos? | 2 | assets/70b1a16a5cdbaa3be0bd2a4bad0e2e56f0fbfb30a5a9259452005c0e0f9c42d5.png | us-mixed-003 / 1 | 0.0094 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde estão os resultados de colesterol e triglicerídeos? | 3 | assets/6737f7fae792224f5ccd27310a22c6f79a6d9a7d469ba0756a73223f30b8c7d5.png | chest-001 / 1 | -0.0117 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual página contém a conclusão do laudo? | 1 | assets/6737f7fae792224f5ccd27310a22c6f79a6d9a7d469ba0756a73223f30b8c7d5.png | chest-001 / 1 | 0.0196 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual página contém a conclusão do laudo? | 2 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.0071 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual página contém a conclusão do laudo? | 3 | assets/70b1a16a5cdbaa3be0bd2a4bad0e2e56f0fbfb30a5a9259452005c0e0f9c42d5.png | us-mixed-003 / 1 | 0.0036 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o laudo descreve os achados do exame? | 1 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.0054 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o laudo descreve os achados do exame? | 2 | assets/6737f7fae792224f5ccd27310a22c6f79a6d9a7d469ba0756a73223f30b8c7d5.png | chest-001 / 1 | 0.0043 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o laudo descreve os achados do exame? | 3 | assets/70b1a16a5cdbaa3be0bd2a4bad0e2e56f0fbfb30a5a9259452005c0e0f9c42d5.png | us-mixed-003 / 1 | -0.0047 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a radiografia de tórax presente no conjunto de exames. | 1 | assets/6737f7fae792224f5ccd27310a22c6f79a6d9a7d469ba0756a73223f30b8c7d5.png | chest-001 / 1 | 0.0381 | 2 | Synthetic chest radiograph |
| Mostre a radiografia de tórax presente no conjunto de exames. | 2 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.0326 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a radiografia de tórax presente no conjunto de exames. | 3 | assets/70b1a16a5cdbaa3be0bd2a4bad0e2e56f0fbfb30a5a9259452005c0e0f9c42d5.png | us-mixed-003 / 1 | 0.0219 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Encontre as imagens de ultrassonografia abdominal. | 1 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.0657 | 2 | Synthetic abdominal ultrasound image |
| Encontre as imagens de ultrassonografia abdominal. | 2 | assets/70b1a16a5cdbaa3be0bd2a4bad0e2e56f0fbfb30a5a9259452005c0e0f9c42d5.png | us-mixed-003 / 1 | 0.0510 | 2 | Ultrasound image embedded in the ultrasound report |
| Encontre as imagens de ultrassonografia abdominal. | 3 | assets/6737f7fae792224f5ccd27310a22c6f79a6d9a7d469ba0756a73223f30b8c7d5.png | chest-001 / 1 | 0.0133 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a imagem associada a este laudo de ultrassonografia. | 1 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.0786 | 1 | Abdominal ultrasound image not linked to the report |
| Mostre a imagem associada a este laudo de ultrassonografia. | 2 | assets/70b1a16a5cdbaa3be0bd2a4bad0e2e56f0fbfb30a5a9259452005c0e0f9c42d5.png | us-mixed-003 / 1 | 0.0685 | 2 | Image embedded in the ultrasound report |
| Mostre a imagem associada a este laudo de ultrassonografia. | 3 | assets/6737f7fae792224f5ccd27310a22c6f79a6d9a7d469ba0756a73223f30b8c7d5.png | chest-001 / 1 | 0.0328 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o documento informa a técnica utilizada no exame? | 1 | assets/6737f7fae792224f5ccd27310a22c6f79a6d9a7d469ba0756a73223f30b8c7d5.png | chest-001 / 1 | 0.0297 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o documento informa a técnica utilizada no exame? | 2 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.0259 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o documento informa a técnica utilizada no exame? | 3 | assets/70b1a16a5cdbaa3be0bd2a4bad0e2e56f0fbfb30a5a9259452005c0e0f9c42d5.png | us-mixed-003 / 1 | 0.0163 | 0 | unlabeled: not judged relevant for this query (label 0) |

### p4-combined-e5-base-siglip (combined)

Score is the fused RRF score; the leg columns give each result's rank and cosine score in its own space.

| Query | Rank | Retrieved item | Source/page | Fused score | Text leg | Image leg | Relevance | Note |
|---|---:|---|---|---:|---|---|---:|---|
| Onde está o resultado da hemoglobina? | 1 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Hemoglobina \| 13,5 \| g/dL \| 12,0–16,0 | lab-searchable-001 / 1 | 0.016393 | #1 (0.8817) | - | 2 | Hemoglobin row with value, unit, and reference range (native text) |
| Onde está o resultado da hemoglobina? | 2 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.016393 | - | #1 (0.0159) | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde está o resultado da hemoglobina? | 3 | assets/70b1a16a5cdbaa3be0bd2a4bad0e2e56f0fbfb30a5a9259452005c0e0f9c42d5.png | us-mixed-003 / 1 | 0.016129 | - | #2 (0.0105) | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual é o valor da glicemia e sua unidade? | 1 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Glicemia de jejum \| 92 \| mg/dL \| 70–99 | lab-searchable-001 / 2 | 0.016393 | #1 (0.8709) | - | 2 | Fasting glucose row with value 92 mg/dL |
| Qual é o valor da glicemia e sua unidade? | 2 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.016393 | - | #1 (-0.0030) | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual é o valor da glicemia e sua unidade? | 3 | assets/70b1a16a5cdbaa3be0bd2a4bad0e2e56f0fbfb30a5a9259452005c0e0f9c42d5.png | us-mixed-003 / 1 | 0.016129 | - | #2 (-0.0094) | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual intervalo de referência aparece para a creatinina? | 1 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Creatinina \| 0,9 \| mg/dL \| 0,6–1,1 | lab-searchable-001 / 2 | 0.016393 | #1 (0.8849) | - | 2 | Creatinine row with reference range 0,6–1,1 |
| Qual intervalo de referência aparece para a creatinina? | 2 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.016393 | - | #1 (0.0038) | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual intervalo de referência aparece para a creatinina? | 3 | assets/70b1a16a5cdbaa3be0bd2a4bad0e2e56f0fbfb30a5a9259452005c0e0f9c42d5.png | us-mixed-003 / 1 | 0.016129 | - | #2 (0.0022) | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde estão os resultados de colesterol e triglicerídeos? | 1 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Triglicerideos \| 120 \| mg/dL \| < 150 | lab-scanned-002 / 2 | 0.016393 | #1 (0.8625) | - | 2 | Triglycerides row recovered by OCR |
| Onde estão os resultados de colesterol e triglicerídeos? | 2 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.016393 | - | #1 (0.0119) | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde estão os resultados de colesterol e triglicerídeos? | 3 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Triglicerídeos \| 120 \| mg/dL \| < 150 | lab-searchable-001 / 2 | 0.016129 | #2 (0.8600) | - | 2 | Triglycerides row |
| Qual página contém a conclusão do laudo? | 1 | assets/6737f7fae792224f5ccd27310a22c6f79a6d9a7d469ba0756a73223f30b8c7d5.png | chest-001 / 1 | 0.016393 | - | #1 (0.0196) | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual página contém a conclusão do laudo? | 2 | LABORATORIO FICTICIO DE ANALISES - LAUDO SINTETICO / Paciente: [ANONIMIZADO] Registro: [ANONIMIZADO] | lab-searchable-001 / 1 | 0.016393 | #1 (0.7999) | - | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual página contém a conclusão do laudo? | 3 | LABORATORIO FICTICIO DE ANALISES  - LAUDO SINTETICO / Paciente: [ANONIMIZADO]  Registro: [ANONIMIZADO] | lab-scanned-002 / 1 | 0.016129 | #2 (0.7999) | - | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o laudo descreve os achados do exame? | 1 | LABORATORIO FICTICIO DE ANALISES - LAUDO SINTETICO / Paciente: [ANONIMIZADO] Registro: [ANONIMIZADO] | lab-searchable-001 / 1 | 0.016393 | #1 (0.8194) | - | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o laudo descreve os achados do exame? | 2 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.016393 | - | #1 (0.0054) | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o laudo descreve os achados do exame? | 3 | assets/6737f7fae792224f5ccd27310a22c6f79a6d9a7d469ba0756a73223f30b8c7d5.png | chest-001 / 1 | 0.016129 | - | #2 (0.0043) | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a radiografia de tórax presente no conjunto de exames. | 1 | assets/6737f7fae792224f5ccd27310a22c6f79a6d9a7d469ba0756a73223f30b8c7d5.png | chest-001 / 1 | 0.016393 | - | #1 (0.0381) | 2 | Synthetic chest radiograph |
| Mostre a radiografia de tórax presente no conjunto de exames. | 2 | LABORATORIO FICTICIO DE ANALISES - LAUDO SINTETICO / Paciente: [ANONIMIZADO] Registro: [ANONIMIZADO] | lab-searchable-001 / 1 | 0.016393 | #1 (0.8481) | - | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a radiografia de tórax presente no conjunto de exames. | 3 | LABORATORIO FICTICIO DE ANALISES  - LAUDO SINTETICO / Paciente: [ANONIMIZADO]  Registro: [ANONIMIZADO] | lab-scanned-002 / 1 | 0.016129 | #2 (0.8481) | - | 0 | unlabeled: not judged relevant for this query (label 0) |
| Encontre as imagens de ultrassonografia abdominal. | 1 | Conclusão / Ultrassonografia abdominal sem alterações significativas. | us-mixed-003 / 2 | 0.016393 | #1 (0.8726) | - | 0 | unlabeled: not judged relevant for this query (label 0) |
| Encontre as imagens de ultrassonografia abdominal. | 2 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.016393 | - | #1 (0.0657) | 2 | Synthetic abdominal ultrasound image |
| Encontre as imagens de ultrassonografia abdominal. | 3 | assets/70b1a16a5cdbaa3be0bd2a4bad0e2e56f0fbfb30a5a9259452005c0e0f9c42d5.png | us-mixed-003 / 1 | 0.016129 | - | #2 (0.0510) | 2 | Ultrasound image embedded in the ultrasound report |
| Mostre a imagem associada a este laudo de ultrassonografia. | 1 | LAUDO DE ULTRASSONOGRAFIA ABDOMINAL - SINTETICO / Paciente: [ANONIMIZADO] | us-mixed-003 / 1 | 0.016393 | #1 (0.8664) | - | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a imagem associada a este laudo de ultrassonografia. | 2 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.016393 | - | #1 (0.0786) | 1 | Abdominal ultrasound image not linked to the report |
| Mostre a imagem associada a este laudo de ultrassonografia. | 3 | assets/70b1a16a5cdbaa3be0bd2a4bad0e2e56f0fbfb30a5a9259452005c0e0f9c42d5.png | us-mixed-003 / 1 | 0.016129 | - | #2 (0.0685) | 2 | Image embedded in the ultrasound report |
| Onde o documento informa a técnica utilizada no exame? | 1 | assets/6737f7fae792224f5ccd27310a22c6f79a6d9a7d469ba0756a73223f30b8c7d5.png | chest-001 / 1 | 0.016393 | - | #1 (0.0297) | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o documento informa a técnica utilizada no exame? | 2 | Técnica / Exame realizado com transdutor convexo de 3,5 MHz, em jejum de 8 horas. | us-mixed-003 / 1 | 0.016393 | #1 (0.8404) | - | 2 | Technique section (transducer and preparation) |
| Onde o documento informa a técnica utilizada no exame? | 3 | Técnica / Exame realizado com transdutor convexo de 3,5 MHz, em jejum de 8 horas. | us-mixed-003 / 2 | 0.016129 | #2 (0.8404) | - | 2 | Technique section (OCR page) |

### p4-combined-minilm-clip (combined)

Score is the fused RRF score; the leg columns give each result's rank and cosine score in its own space.

| Query | Rank | Retrieved item | Source/page | Fused score | Text leg | Image leg | Relevance | Note |
|---|---:|---|---|---:|---|---|---:|---|
| Onde está o resultado da hemoglobina? | 1 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Hemoglobina \| 13,5 \| g/dL \| 12,0-16,0 | lab-scanned-002 / 1 | 0.016393 | #1 (0.7309) | - | 2 | Hemoglobin row recovered by OCR |
| Onde está o resultado da hemoglobina? | 2 | assets/70b1a16a5cdbaa3be0bd2a4bad0e2e56f0fbfb30a5a9259452005c0e0f9c42d5.png | us-mixed-003 / 1 | 0.016393 | - | #1 (0.2305) | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde está o resultado da hemoglobina? | 3 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Hemoglobina \| 13,5 \| g/dL \| 12,0–16,0 | lab-searchable-001 / 1 | 0.016129 | #2 (0.7255) | - | 2 | Hemoglobin row with value, unit, and reference range (native text) |
| Qual é o valor da glicemia e sua unidade? | 1 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Glicemia de jejum \| 92 \| mg/dL \| 70–99 | lab-searchable-001 / 2 | 0.016393 | #1 (0.6498) | - | 2 | Fasting glucose row with value 92 mg/dL |
| Qual é o valor da glicemia e sua unidade? | 2 | assets/70b1a16a5cdbaa3be0bd2a4bad0e2e56f0fbfb30a5a9259452005c0e0f9c42d5.png | us-mixed-003 / 1 | 0.016393 | - | #1 (0.2273) | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual é o valor da glicemia e sua unidade? | 3 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.016129 | - | #2 (0.2249) | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual intervalo de referência aparece para a creatinina? | 1 | assets/6737f7fae792224f5ccd27310a22c6f79a6d9a7d469ba0756a73223f30b8c7d5.png | chest-001 / 1 | 0.016393 | - | #1 (0.2272) | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual intervalo de referência aparece para a creatinina? | 2 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Creatinina \| 0,9 \| mg/dL \| 0,6-1,1 | lab-scanned-002 / 2 | 0.016393 | #1 (0.7640) | - | 2 | Creatinine row recovered by OCR |
| Qual intervalo de referência aparece para a creatinina? | 3 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.016129 | - | #2 (0.2193) | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde estão os resultados de colesterol e triglicerídeos? | 1 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Colesterol total \| 182 \| mg/dL \| < 190 | lab-searchable-001 / 2 | 0.016393 | #1 (0.6704) | - | 2 | Total cholesterol row |
| Onde estão os resultados de colesterol e triglicerídeos? | 2 | assets/70b1a16a5cdbaa3be0bd2a4bad0e2e56f0fbfb30a5a9259452005c0e0f9c42d5.png | us-mixed-003 / 1 | 0.016393 | - | #1 (0.2289) | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde estão os resultados de colesterol e triglicerídeos? | 3 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.016129 | - | #2 (0.2278) | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual página contém a conclusão do laudo? | 1 | assets/6737f7fae792224f5ccd27310a22c6f79a6d9a7d469ba0756a73223f30b8c7d5.png | chest-001 / 1 | 0.016393 | - | #1 (0.2223) | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual página contém a conclusão do laudo? | 2 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Plaquetas \| 250.000 \| /mm³ \| 150.000–450.000 | lab-searchable-001 / 1 | 0.016393 | #1 (0.3504) | - | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual página contém a conclusão do laudo? | 3 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Plaquetas \| 250.000 \| /mm? \| 150.000-450.000 | lab-scanned-002 / 1 | 0.016129 | #2 (0.3442) | - | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o laudo descreve os achados do exame? | 1 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Hematócrito \| 40,1 \| % \| 36,0–46,0 | lab-searchable-001 / 1 | 0.016393 | #1 (0.5559) | - | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o laudo descreve os achados do exame? | 2 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.016393 | - | #1 (0.2183) | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o laudo descreve os achados do exame? | 3 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Plaquetas \| 250.000 \| /mm³ \| 150.000–450.000 | lab-searchable-001 / 1 | 0.016129 | #2 (0.5380) | - | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a radiografia de tórax presente no conjunto de exames. | 1 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Plaquetas \| 250.000 \| /mm³ \| 150.000–450.000 | lab-searchable-001 / 1 | 0.016393 | #1 (0.5424) | - | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a radiografia de tórax presente no conjunto de exames. | 2 | assets/70b1a16a5cdbaa3be0bd2a4bad0e2e56f0fbfb30a5a9259452005c0e0f9c42d5.png | us-mixed-003 / 1 | 0.016393 | - | #1 (0.2190) | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a radiografia de tórax presente no conjunto de exames. | 3 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Plaquetas \| 250.000 \| /mm? \| 150.000-450.000 | lab-scanned-002 / 1 | 0.016129 | #2 (0.5204) | - | 0 | unlabeled: not judged relevant for this query (label 0) |
| Encontre as imagens de ultrassonografia abdominal. | 1 | Conclusão / Ultrassonografia abdominal sem alterações significativas. | us-mixed-003 / 2 | 0.016393 | #1 (0.8481) | - | 0 | unlabeled: not judged relevant for this query (label 0) |
| Encontre as imagens de ultrassonografia abdominal. | 2 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.016393 | - | #1 (0.2381) | 2 | Synthetic abdominal ultrasound image |
| Encontre as imagens de ultrassonografia abdominal. | 3 | Conclusão / Ultrassonografia abdominal sem alterações significativas. | us-mixed-003 / 1 | 0.016129 | #2 (0.8481) | - | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a imagem associada a este laudo de ultrassonografia. | 1 | Conclusão / Ultrassonografia abdominal sem alterações significativas. | us-mixed-003 / 2 | 0.016393 | #1 (0.6253) | - | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a imagem associada a este laudo de ultrassonografia. | 2 | assets/70b1a16a5cdbaa3be0bd2a4bad0e2e56f0fbfb30a5a9259452005c0e0f9c42d5.png | us-mixed-003 / 1 | 0.016393 | - | #1 (0.2247) | 2 | Image embedded in the ultrasound report |
| Mostre a imagem associada a este laudo de ultrassonografia. | 3 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.016129 | - | #2 (0.2219) | 1 | Abdominal ultrasound image not linked to the report |
| Onde o documento informa a técnica utilizada no exame? | 1 | assets/8310a398d8e00b187bef59243bec6d2d8998e5e056d02f383d063ee071a7d4dc.png | abd-us-001 / 1 | 0.016393 | - | #1 (0.2197) | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o documento informa a técnica utilizada no exame? | 2 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Plaquetas \| 250.000 \| /mm³ \| 150.000–450.000 | lab-searchable-001 / 1 | 0.016393 | #1 (0.4753) | - | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o documento informa a técnica utilizada no exame? | 3 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Plaquetas \| 250.000 \| /mm? \| 150.000-450.000 | lab-scanned-002 / 1 | 0.016129 | #2 (0.4705) | - | 0 | unlabeled: not judged relevant for this query (label 0) |

## Limitations and outstanding items

- Synthetic fixture corpus and synthetic labels only (tests/fixtures/corpus, labels-synthetic-v4): 3 PDFs (28 text chunks) and 3 images. These numbers exercise the pipeline end to end and do not measure medical retrieval quality. The owner corpus, relevance labels, and fidelity sample are outstanding.
- OpenAI text-embedding-3-small is not in this comparison: OPENAI_API_KEY and owner permission for remote processing (remote_allowed) are outstanding. Its adapter is implemented and covered by deterministic tests only.
- Combined retrieval fuses ranks with RRF (k = 60, ranks from 1, fusion depth all); raw scores of the two models are never compared. A combined run ingests nothing: it pins the revisions of its text run and image run in its own snapshot, so 'search --mode combined --experiment <id>' reproduces it (the search refuses a snapshot fused with other parameters or missing a leg). Its index size is the size of the revisions that snapshot pins.
- On this corpus, fusion raises Hit@3 above every single leg (E5-base + SigLIP 0.80 vs 0.70 text-only and 0.30 image-only; MiniLM + CLIP 0.60 vs 0.50 and 0.30): on the image queries (q07-q09) relevant images reach the top 3, and on the laboratory queries (q01-q04) irrelevant images displace relevant text chunks. Precision@3 and nDCG@3 are NOT robust. With disjoint lists and depth all, the fused ranks 1-2 are the two leg winners (1/61 each) and rank 3 is one of the two runners-up (1/62 each), so the order of every fused tie between a text and an image item is decided by the record-id tie-break (PRD 6.6). Record ids hash (revision, chunk, space), and the chunking-rules revision r2 changed every text revision id without changing any chunk of this corpus. That alone moved E5-base + SigLIP from P@3 0.367 / nDCG@3 0.508 to 0.400 / 0.545, and MiniLM + CLIP from 0.300 / 0.388 to 0.267 / 0.340. Differences between combined and text-only quality on this corpus are therefore within tie-break noise. Low quality is a valid result; a relevance-aware cross-leg tie rule, weighted fusion, a score floor, or a smaller image depth are not evaluated here.
- Latency was measured on a shared developer machine under moderate load (load average 4-11 during the runs; the last regeneration about 5-6); warm medians are indicative, not benchmarks. Cold start depends on the OS file cache of the model files (see reports/progress/PROGRESS.md, Phase 2) and is a single indicative run.
- Extraction fidelity is reported for text runs only: it measures text extraction, which image runs and combined runs do not perform.
