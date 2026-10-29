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
| p3-clip-vit-b32 | embed_anything 0.7.1 | openai/clip-vit-base-patch32 | 3d74acf9a28c67741b2f4f2ea7635f0aaf6f0268 | candle | 512 | None | 16 | `space-6603884ac3fef0f888b11488a160b13c` | `8a4be74221f5` | `edac75deed57` | `484079928a35` |
| p3-siglip-multilingual | embed_anything 0.7.1 | google/siglip-base-patch16-256-multilingual | 8952a4eafcde3cb7ab46b1dd629b33f8784ca9c6 | candle | 768 | None | 16 | `space-912b12ea1376ca71b44ae24b04fc91ca` | `8a4be74221f5` | `edac75deed57` | `484079928a35` |

## Retrieval quality (k = 3, macro averages)

| Experiment | Mode | Ingestion run | Failed sources | Evaluation | Precision@3 | Hit@3 | nDCG@3 |
|---|---|---|---:|---|---:|---:|---:|
| p3-clip-vit-b32 | image-only | succeeded | 0 | Completed | 0.1667 | 0.3000 | 0.2407 |
| p3-siglip-multilingual | image-only | succeeded | 0 | Completed | 0.1667 | 0.3000 | 0.2704 |

## Efficiency

| Experiment | Ingestion ms | ms/page | chunks/s | Batch latency ms | Embeds/s | Model load ms | Cold start ms | Warm median ms | Warm p95 ms | Peak local RAM | Index size | GPU memory |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---|---|
| p3-clip-vit-b32 | 7899.7 | - | 0.4 | 1631.24 | 0.6 | 12780.7 | 13463.0 | 1771.27 | 4070.09 | 701530112 bytes | 23256 bytes | unavailable (inference runs on the local CPU; no GPU is used) |
| p3-siglip-multilingual | 37558.0 | - | 0.1 | 11830.64 | 0.1 | 43678.4 | 59702.8 | 3296.59 | 6599.41 | 3512320000 bytes | 33075 bytes | unavailable (inference runs on the local CPU; no GPU is used) |

## Cost and reliability

| Experiment | Ingestion cost | Query-set cost | Provider memory | Failed pages | Failed batches | Retries | Truncations |
|---|---|---|---|---:|---:|---:|---:|
| p3-clip-vit-b32 | NotApplicable: local engine: no provider is billed | NotApplicable: local engine: no provider is billed | local (see peak RAM) | 0 | 0 | 0 | 0 |
| p3-siglip-multilingual | NotApplicable: local engine: no provider is billed | NotApplicable: local engine: no provider is billed | local (see peak RAM) | 0 | 0 | 0 | 0 |

## Top-3 results per query

### p3-clip-vit-b32 (embed_anything / openai/clip-vit-base-patch32)

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

### p3-siglip-multilingual (embed_anything / google/siglip-base-patch16-256-multilingual)

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

## Limitations and outstanding items

- Synthetic fixture corpus and synthetic labels only: three synthetic images (two image files and the embedded page-1 image of us-mixed-003). These numbers exercise the image pipeline and do not measure medical retrieval quality. The owner corpus, labels, and fidelity sample are outstanding.
- Image-only mode: the frozen text queries are embedded through each model's text-query path and ranked against image records only. nDCG uses each query's frozen labels over every modality (PRD §5.3), so text-only relevant items lower the ideal-normalized score of an image-only ranking.
- Extraction fidelity is not reported: it measures text extraction, which an image run does not use (see reports/phase-2).
- Measurement host under heavy unrelated load (load average about 34-39 on 8 logical CPUs from other desktop applications during these runs): latencies and throughput are indicative only and not comparable with the Phase 2 text runs.
- Single run on one machine; latencies are indicative, not statistically compared.
