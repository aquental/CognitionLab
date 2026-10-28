# Retrieval comparison (end-to-end configuration comparison)

Configurations differ in engine, weight format, or model, so differences are not attributed to a single variable.

## Setup

- Hardware: macos aarch64 · CPU Apple M3 Pro · 12 logical CPUs · 19327352832 bytes memory · accelerator cpu
- Corpus checksum: `sha256:0f808bc5723251bbe0f43da84de68bfa541075a6bb640e27719a123b12be74b6`
- Query set: queries-v1 (`sha256:0ca4735dc8b960715916fc29ac76d8de577465e6ef232e69116d6053638ebbed`)
- Labels: labels-synthetic-v1 (`sha256:c2f637cbaf5fbb1814f15c37c3251e543609b4e93da7639d388345c80c3f4b6a`)

## Configurations

| Experiment | Engine | Model | Revision | Weights | Dims | Prefix | Batch | Space | Extraction fp | Chunking fp | Evaluation fp |
|---|---|---|---|---|---:|---|---:|---|---|---|---|
| p2-embed-anything-minilm | embed_anything 0.7.1 | sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2 | e8f8c211226b894fcb81acc59f3b34ba3efd5f42 | safetensors | 384 | None | 32 | `space-04233b7a97515dc7c052bb12db39a743` | `c82ad2eb3a9b` | `96ccec200c13` | `40b5de69a70f` |
| p2-fastembed-minilm | fastembed 7.5.0 | Xenova/paraphrase-multilingual-MiniLM-L12-v2 | 2c4055b12046f11709e9df2c122e59ffbdc2f900 | onnx | 384 | None | 32 | `space-25b9aeac30261d1226218758e6756264` | `c82ad2eb3a9b` | `bb24c712b900` | `40b5de69a70f` |
| p2-fastembed-e5-base | fastembed 7.5.0 | intfloat/multilingual-e5-base | d128750597153bb5987e10b1c3493a34e5a4502a | onnx | 768 | E5 | 32 | `space-20e1f35a45635362d2a1a1be05b34b34` | `c82ad2eb3a9b` | `27662ad95771` | `40b5de69a70f` |

## Retrieval quality (k = 3, macro averages)

| Experiment | Status | Precision@3 | Hit@3 | nDCG@3 |
|---|---|---:|---:|---:|
| p2-embed-anything-minilm | Completed | 0.3333 | 0.5000 | 0.4093 |
| p2-fastembed-minilm | Completed | 0.3333 | 0.5000 | 0.4093 |
| p2-fastembed-e5-base | Completed | 0.4333 | 0.7000 | 0.5399 |

## Efficiency

| Experiment | Ingestion ms | ms/page | chunks/s | Batch latency ms | Embeds/s | Model load ms | Cold start ms | Warm median ms | Warm p95 ms | Peak local RAM | Index size | GPU memory |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---|---|
| p2-embed-anything-minilm | 2888.5 | 481.4 | 9.7 | 117.69 | 79.3 | 994.9 | 450.1 | 15.09 | 17.23 | 1641725952 bytes | 165702 bytes | unavailable (inference runs on the CPU; no GPU is used) |
| p2-fastembed-minilm | 2831.6 | 471.9 | 9.9 | 107.21 | 87.1 | 3123.0 | 2465.5 | 26.54 | 27.89 | 5221335040 bytes | 164840 bytes | unavailable (inference runs on the CPU; no GPU is used) |
| p2-fastembed-e5-base | 3297.5 | 549.6 | 8.5 | 248.05 | 37.6 | 6907.8 | 5453.4 | 48.61 | 51.61 | 7925219328 bytes | 298563 bytes | unavailable (inference runs on the CPU; no GPU is used) |

## Cost and reliability

| Experiment | Ingestion cost | Query-set cost | Provider memory | Failed pages | Failed batches | Retries | Truncations |
|---|---|---|---|---:|---:|---:|---:|
| p2-embed-anything-minilm | NotApplicable: local engine: no provider is billed | NotApplicable: local engine: no provider is billed | local (see peak RAM) | 0 | 0 | 0 | 0 |
| p2-fastembed-minilm | NotApplicable: local engine: no provider is billed | NotApplicable: local engine: no provider is billed | local (see peak RAM) | 0 | 0 | 0 | 0 |
| p2-fastembed-e5-base | NotApplicable: local engine: no provider is billed | NotApplicable: local engine: no provider is billed | local (see peak RAM) | 0 | 0 | 0 | 0 |

## Extraction fidelity

| Experiment | Sample | Overall | Test name | Value | Unit | Reference range | Native | OCR |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| p2-embed-anything-minilm | fidelity-synthetic-v1 | 0.844 | 0.875 | 1.000 | 0.875 | 0.625 | 1.000 | 0.688 |
| p2-fastembed-minilm | fidelity-synthetic-v1 | 0.844 | 0.875 | 1.000 | 0.875 | 0.625 | 1.000 | 0.688 |
| p2-fastembed-e5-base | fidelity-synthetic-v1 | 0.844 | 0.875 | 1.000 | 0.875 | 0.625 | 1.000 | 0.688 |

## Top-3 results per query

### p2-embed-anything-minilm (embed_anything / sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2)

| Query | Rank | Retrieved item | Source/page | Score | Relevance | Note |
|---|---:|---|---|---:|---:|---|
| Onde está o resultado da hemoglobina? | 1 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Hemoglobina \| 13,5 \| g/dL \| 12,0-16,0 | lab-scanned-002 / 1 | 0.7309 | 2 | Hemoglobin row recovered by OCR |
| Onde está o resultado da hemoglobina? | 2 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Hemoglobina \| 13,5 \| g/dL \| 12,0–16,0 | lab-searchable-001 / 1 | 0.7255 | 2 | Hemoglobin row with value, unit, and reference range (native text) |
| Onde está o resultado da hemoglobina? | 3 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Hematocrito \| 40,1 \| % \| 36,0-46,0 | lab-scanned-002 / 1 | 0.5886 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual é o valor da glicemia e sua unidade? | 1 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Glicemia de jejum \| 92 \| mg/dL \| 70–99 | lab-searchable-001 / 2 | 0.6498 | 2 | Fasting glucose row with value 92 mg/dL |
| Qual é o valor da glicemia e sua unidade? | 2 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Glicemia de jejum \| 92 \| mg/dL \| 70-99 | lab-scanned-002 / 2 | 0.6495 | 2 | Fasting glucose row recovered by OCR |
| Qual é o valor da glicemia e sua unidade? | 3 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Colesterol total \| 182 \| mg/dL \| < 190 | lab-scanned-002 / 2 | 0.5509 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual intervalo de referência aparece para a creatinina? | 1 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Creatinina \| 0,9 \| mg/dL \| 0,6-1,1 | lab-scanned-002 / 2 | 0.7640 | 2 | Creatinine row recovered by OCR |
| Qual intervalo de referência aparece para a creatinina? | 2 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Creatinina \| 0,9 \| mg/dL \| 0,6–1,1 | lab-searchable-001 / 2 | 0.7630 | 2 | Creatinine row with reference range 0,6–1,1 |
| Qual intervalo de referência aparece para a creatinina? | 3 | Bioquímica / Conclusão: valores dentro dos intervalos de referência. | lab-scanned-002 / 2 | 0.5252 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde estão os resultados de colesterol e triglicerídeos? | 1 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Colesterol total \| 182 \| mg/dL \| < 190 | lab-scanned-002 / 2 | 0.6704 | 2 | Total cholesterol row recovered by OCR |
| Onde estão os resultados de colesterol e triglicerídeos? | 2 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Colesterol total \| 182 \| mg/dL \| < 190 | lab-searchable-001 / 2 | 0.6704 | 2 | Total cholesterol row |
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
| Encontre as imagens de ultrassonografia abdominal. | 1 | Conclusão / Ultrassonografia abdominal sem alterações significativas. | us-mixed-003 / 1 | 0.8481 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Encontre as imagens de ultrassonografia abdominal. | 2 | Conclusão / Ultrassonografia abdominal sem alterações significativas. | us-mixed-003 / 2 | 0.8481 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Encontre as imagens de ultrassonografia abdominal. | 3 | LAUDO DE ULTRASSONOGRAFIA ABDOMINAL - SINTETICO / Paciente: [ANONIMIZADO] | us-mixed-003 / 1 | 0.6548 | 1 | Title of the abdominal ultrasound report, not an image |
| Mostre a imagem associada a este laudo de ultrassonografia. | 1 | Conclusão / Ultrassonografia abdominal sem alterações significativas. | us-mixed-003 / 1 | 0.6253 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a imagem associada a este laudo de ultrassonografia. | 2 | Conclusão / Ultrassonografia abdominal sem alterações significativas. | us-mixed-003 / 2 | 0.6253 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a imagem associada a este laudo de ultrassonografia. | 3 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Leucócitos \| 6.800 \| /mm³ \| 4.000–10.000 | lab-searchable-001 / 1 | 0.5172 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o documento informa a técnica utilizada no exame? | 1 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Plaquetas \| 250.000 \| /mm³ \| 150.000–450.000 | lab-searchable-001 / 1 | 0.4753 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o documento informa a técnica utilizada no exame? | 2 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Plaquetas \| 250.000 \| /mm? \| 150.000-450.000 | lab-scanned-002 / 1 | 0.4705 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o documento informa a técnica utilizada no exame? | 3 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Hematócrito \| 40,1 \| % \| 36,0–46,0 | lab-searchable-001 / 1 | 0.4572 | 0 | unlabeled: not judged relevant for this query (label 0) |

### p2-fastembed-minilm (fastembed / Xenova/paraphrase-multilingual-MiniLM-L12-v2)

| Query | Rank | Retrieved item | Source/page | Score | Relevance | Note |
|---|---:|---|---|---:|---:|---|
| Onde está o resultado da hemoglobina? | 1 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Hemoglobina \| 13,5 \| g/dL \| 12,0-16,0 | lab-scanned-002 / 1 | 0.7309 | 2 | Hemoglobin row recovered by OCR |
| Onde está o resultado da hemoglobina? | 2 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Hemoglobina \| 13,5 \| g/dL \| 12,0–16,0 | lab-searchable-001 / 1 | 0.7255 | 2 | Hemoglobin row with value, unit, and reference range (native text) |
| Onde está o resultado da hemoglobina? | 3 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Hematocrito \| 40,1 \| % \| 36,0-46,0 | lab-scanned-002 / 1 | 0.5886 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual é o valor da glicemia e sua unidade? | 1 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Glicemia de jejum \| 92 \| mg/dL \| 70–99 | lab-searchable-001 / 2 | 0.6498 | 2 | Fasting glucose row with value 92 mg/dL |
| Qual é o valor da glicemia e sua unidade? | 2 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Glicemia de jejum \| 92 \| mg/dL \| 70-99 | lab-scanned-002 / 2 | 0.6495 | 2 | Fasting glucose row recovered by OCR |
| Qual é o valor da glicemia e sua unidade? | 3 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Colesterol total \| 182 \| mg/dL \| < 190 | lab-scanned-002 / 2 | 0.5509 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual intervalo de referência aparece para a creatinina? | 1 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Creatinina \| 0,9 \| mg/dL \| 0,6-1,1 | lab-scanned-002 / 2 | 0.7640 | 2 | Creatinine row recovered by OCR |
| Qual intervalo de referência aparece para a creatinina? | 2 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Creatinina \| 0,9 \| mg/dL \| 0,6–1,1 | lab-searchable-001 / 2 | 0.7630 | 2 | Creatinine row with reference range 0,6–1,1 |
| Qual intervalo de referência aparece para a creatinina? | 3 | Bioquímica / Conclusão: valores dentro dos intervalos de referência. | lab-searchable-001 / 2 | 0.5252 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde estão os resultados de colesterol e triglicerídeos? | 1 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Colesterol total \| 182 \| mg/dL \| < 190 | lab-scanned-002 / 2 | 0.6704 | 2 | Total cholesterol row recovered by OCR |
| Onde estão os resultados de colesterol e triglicerídeos? | 2 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Colesterol total \| 182 \| mg/dL \| < 190 | lab-searchable-001 / 2 | 0.6704 | 2 | Total cholesterol row |
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
| Encontre as imagens de ultrassonografia abdominal. | 1 | Conclusão / Ultrassonografia abdominal sem alterações significativas. | us-mixed-003 / 1 | 0.8481 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Encontre as imagens de ultrassonografia abdominal. | 2 | Conclusão / Ultrassonografia abdominal sem alterações significativas. | us-mixed-003 / 2 | 0.8481 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Encontre as imagens de ultrassonografia abdominal. | 3 | LAUDO DE ULTRASSONOGRAFIA ABDOMINAL - SINTETICO / Paciente: [ANONIMIZADO] | us-mixed-003 / 1 | 0.6548 | 1 | Title of the abdominal ultrasound report, not an image |
| Mostre a imagem associada a este laudo de ultrassonografia. | 1 | Conclusão / Ultrassonografia abdominal sem alterações significativas. | us-mixed-003 / 1 | 0.6253 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a imagem associada a este laudo de ultrassonografia. | 2 | Conclusão / Ultrassonografia abdominal sem alterações significativas. | us-mixed-003 / 2 | 0.6253 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a imagem associada a este laudo de ultrassonografia. | 3 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Leucócitos \| 6.800 \| /mm³ \| 4.000–10.000 | lab-searchable-001 / 1 | 0.5172 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o documento informa a técnica utilizada no exame? | 1 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Plaquetas \| 250.000 \| /mm³ \| 150.000–450.000 | lab-searchable-001 / 1 | 0.4753 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o documento informa a técnica utilizada no exame? | 2 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Plaquetas \| 250.000 \| /mm? \| 150.000-450.000 | lab-scanned-002 / 1 | 0.4705 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o documento informa a técnica utilizada no exame? | 3 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Hematócrito \| 40,1 \| % \| 36,0–46,0 | lab-searchable-001 / 1 | 0.4572 | 0 | unlabeled: not judged relevant for this query (label 0) |

### p2-fastembed-e5-base (fastembed / intfloat/multilingual-e5-base)

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
| Qual intervalo de referência aparece para a creatinina? | 3 | Bioquímica / Conclusão: valores dentro dos intervalos de referência. | lab-scanned-002 / 2 | 0.8498 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde estão os resultados de colesterol e triglicerídeos? | 1 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Triglicerideos \| 120 \| mg/dL \| < 150 | lab-scanned-002 / 2 | 0.8625 | 2 | Triglycerides row recovered by OCR |
| Onde estão os resultados de colesterol e triglicerídeos? | 2 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Triglicerídeos \| 120 \| mg/dL \| < 150 | lab-searchable-001 / 2 | 0.8600 | 2 | Triglycerides row |
| Onde estão os resultados de colesterol e triglicerídeos? | 3 | Bioquímica / Exame \| Resultado \| Unidade \| Intervalo de referência / Colesterol total \| 182 \| mg/dL \| < 190 | lab-scanned-002 / 2 | 0.8459 | 2 | Total cholesterol row recovered by OCR |
| Qual página contém a conclusão do laudo? | 1 | LABORATORIO FICTICIO DE ANALISES - LAUDO SINTETICO / Paciente: [ANONIMIZADO] Registro: [ANONIMIZADO] | lab-searchable-001 / 1 | 0.7999 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual página contém a conclusão do laudo? | 2 | LABORATORIO FICTICIO DE ANALISES  - LAUDO SINTETICO / Paciente: [ANONIMIZADO]  Registro: [ANONIMIZADO] | lab-scanned-002 / 1 | 0.7999 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Qual página contém a conclusão do laudo? | 3 | LAUDO DE ULTRASSONOGRAFIA ABDOMINAL  - SINTETICO / Paciente: [ANONIMIZADO] | us-mixed-003 / 2 | 0.7936 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o laudo descreve os achados do exame? | 1 | LABORATORIO FICTICIO DE ANALISES - LAUDO SINTETICO / Paciente: [ANONIMIZADO] Registro: [ANONIMIZADO] | lab-searchable-001 / 1 | 0.8194 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o laudo descreve os achados do exame? | 2 | LABORATORIO FICTICIO DE ANALISES  - LAUDO SINTETICO / Paciente: [ANONIMIZADO]  Registro: [ANONIMIZADO] | lab-scanned-002 / 1 | 0.8194 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o laudo descreve os achados do exame? | 3 | Achados / Fígado de dimensões normais, contornos regulares e ecotextura homogênea. / Vesícula biliar normodistendida, paredes finas, sem cálculos. | us-mixed-003 / 1 | 0.8148 | 2 | Findings section of the ultrasound report |
| Mostre a radiografia de tórax presente no conjunto de exames. | 1 | LABORATORIO FICTICIO DE ANALISES - LAUDO SINTETICO / Paciente: [ANONIMIZADO] Registro: [ANONIMIZADO] | lab-searchable-001 / 1 | 0.8481 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a radiografia de tórax presente no conjunto de exames. | 2 | LABORATORIO FICTICIO DE ANALISES  - LAUDO SINTETICO / Paciente: [ANONIMIZADO]  Registro: [ANONIMIZADO] | lab-scanned-002 / 1 | 0.8481 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a radiografia de tórax presente no conjunto de exames. | 3 | LAUDO DE ULTRASSONOGRAFIA ABDOMINAL  - SINTETICO / Paciente: [ANONIMIZADO] | us-mixed-003 / 2 | 0.8439 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Encontre as imagens de ultrassonografia abdominal. | 1 | Conclusão / Ultrassonografia abdominal sem alterações significativas. | us-mixed-003 / 1 | 0.8726 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Encontre as imagens de ultrassonografia abdominal. | 2 | Conclusão / Ultrassonografia abdominal sem alterações significativas. | us-mixed-003 / 2 | 0.8726 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Encontre as imagens de ultrassonografia abdominal. | 3 | LAUDO DE ULTRASSONOGRAFIA ABDOMINAL  - SINTETICO / Paciente: [ANONIMIZADO] | us-mixed-003 / 2 | 0.8645 | 1 | Title of the abdominal ultrasound report repeated on page 2, not an image |
| Mostre a imagem associada a este laudo de ultrassonografia. | 1 | LAUDO DE ULTRASSONOGRAFIA ABDOMINAL  - SINTETICO / Paciente: [ANONIMIZADO] | us-mixed-003 / 2 | 0.8664 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a imagem associada a este laudo de ultrassonografia. | 2 | LAUDO DE ULTRASSONOGRAFIA ABDOMINAL - SINTETICO / Paciente: [ANONIMIZADO] | us-mixed-003 / 1 | 0.8664 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Mostre a imagem associada a este laudo de ultrassonografia. | 3 | LABORATORIO FICTICIO DE ANALISES - LAUDO SINTETICO / Paciente: [ANONIMIZADO] Registro: [ANONIMIZADO] | lab-searchable-001 / 1 | 0.8536 | 0 | unlabeled: not judged relevant for this query (label 0) |
| Onde o documento informa a técnica utilizada no exame? | 1 | Técnica / Exame realizado com transdutor convexo de 3,5 MHz, em jejum de 8 horas. | us-mixed-003 / 2 | 0.8404 | 2 | Technique section (OCR page) |
| Onde o documento informa a técnica utilizada no exame? | 2 | Técnica / Exame realizado com transdutor convexo de 3,5 MHz, em jejum de 8 horas. | us-mixed-003 / 1 | 0.8404 | 2 | Technique section (transducer and preparation) |
| Onde o documento informa a técnica utilizada no exame? | 3 | Hemograma / Exame \| Resultado \| Unidade \| Intervalo de referência / Plaquetas \| 250.000 \| /mm? \| 150.000-450.000 | lab-scanned-002 / 1 | 0.8221 | 0 | unlabeled: not judged relevant for this query (label 0) |

## Limitations and outstanding items

- Synthetic fixture corpus and synthetic labels only: these numbers exercise the pipeline and do not measure medical retrieval quality. The owner corpus, labels, and fidelity sample are outstanding.
- OpenAI (text-embedding-3-small) is outstanding: no OPENAI_API_KEY and no owner authorization for remote processing. Its adapter is covered by local HTTP contract tests only.
- Queries q07-q09 target images; text-only mode cannot retrieve them (Milestone 3).
- Single run on one machine; latencies are indicative, not statistically compared.
