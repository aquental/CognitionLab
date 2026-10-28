# PRD — Medical Document Embedding Lab

**Status:** Draft  
**Implementation language:** Rust  
**Initial embedding engine:** EmbedAnything 

## 1. Product Overview

Build a small, reproducible pipeline that converts anonymized medical documents and images into embeddings, stores metadata alongside each vector, and evaluates retrieval quality using real-world queries in Portuguese.

The project is an experimental environment for comparing embedding engines, models, and retrieval approaches. It must support switching engines through the **Strategy design pattern**, starting with EmbedAnything and adding FastEmbed, OpenAI, and other providers.

The system supports information extraction and retrieval. Diagnostic interpretation and clinical decision-making are outside its scope.

## 2. Problem Statement

Medical information appears in heterogeneous formats: searchable PDFs, scanned reports, laboratory tables, radiographs, and ultrasound images.

Retrieval quality depends on several independent factors:

- Accuracy of text extraction and OCR.
- Preservation of relationships between test names, values, units, and reference ranges.
- Embedding model performance on Portuguese medical terminology.
- Support for searching images using text.
- Chunking and retrieval strategies.
- Processing time, memory consumption, and provider costs.

The project must make these factors measurable and allow controlled comparisons without rewriting the ingestion pipeline.

## 3. Goals

- Create an end-to-end pipeline for a small, representative medical corpus.
- Preserve source traceability for every indexed item.
- Compare interchangeable local and remote embedding engines.
- Evaluate text-only, image-only, and combined retrieval.
- Measure retrieval quality, extraction fidelity, latency, memory usage, and cost.
- Produce reproducible evidence, including an indexed-record schema and top-three results for ten Portuguese queries.

## 4. Scope

### Initial corpus

Use approximately **10–20 anonymized documents**, covering:

| Content type | Examples |
|---|---|
| Searchable PDFs | Laboratory results and medical reports |
| Scanned documents | Scanned laboratory results and printed reports |
| Medical images | Radiographs and ultrasound images |
| Mixed documents | Reports containing text, tables, and embedded images |

The initial image scope is PNG/JPEG files and images embedded in PDFs. Native DICOM ingestion is a future extension.

Use a fixed corpus manifest with source identifiers, checksums, document types, and processing eligibility.

### Reliability is a core requirement

Reliability is part of the initial scope and a release acceptance condition, even for this experimental corpus. Quality comparisons are meaningful only when ingestion, indexing, retrieval, and reporting expose failures and preserve data integrity.

The pipeline must:

- Produce no duplicate active records when unchanged inputs are reprocessed with the same configuration.
- Publish records atomically per source revision and embedding configuration. A failed update must leave the previously committed revision searchable and must not expose partially written replacement records.
- Detect changed checksums, replace the active revision only after successful processing, and preserve the revision used by each saved experiment.
- Validate engine capabilities and embedding-space compatibility before processing; reject vectors with incorrect dimensions or non-finite values before committing them.
- Record per-source success or failure, failure stage, attempt count, and reason. A run with failed required inputs must be marked incomplete, never successful.
- Retry only errors classified as transient, using a configured finite attempt limit that includes the initial attempt. Permanent errors must fail immediately. Independent eligible sources must still be processed.
- Flag empty or unreadable content and record truncations explicitly. Neither may silently appear as valid, complete extraction.
- Preserve source, page or asset, model, and configuration provenance for every searchable item.
- Reproduce deterministic fixture results exactly. Real-model comparisons must freeze inputs and configurations and document numeric tolerances and any provider or hardware nondeterminism; bitwise equality of real embeddings is not required.

Acceptance must include automated failure-injection scenarios, not only successful runs. Retrieval relevance may be low as an experimental finding; silent corruption, duplicate records, hidden failures, and untraceable results are unacceptable.

### Out of scope

- Medical diagnosis or treatment recommendations.
- Training or fine-tuning embedding models.
- Production deployment and clinical system integration.
- A full document-management interface.
- Native DICOM processing in the initial release.
- Large-scale distributed ingestion.

## 5. Gherkin User Stories and Executable Acceptance Rules

The following Gherkin features replace abstract user stories with observable pass/fail rules. Each `Given` defines controlled preconditions, each `When` invokes the software, and each `Then` asserts persisted state, returned results, errors, or report contents. Narrative intent does not replace assertions.

Each code block must become a separate `.feature` file under `tests/features/`. Tags provide stable traceability to requirements and CI results. Fixtures must be anonymized, versioned, and local. Each scenario uses an isolated store and a fresh Cucumber `World`; no scenario depends on another scenario's execution.

Milestone tags (`@m1`–`@m4`, section 14) on scenarios and on `Examples` blocks state when each scenario becomes required. A scenario or example row is added to its `.feature` file in the milestone that implements it; from then on it is required in every CI run. The required inventory for milestone *N* is every scenario and example row tagged `@m1`…`@mN` in this section.

### 5.1 US-01 — Consistent document ingestion

```gherkin
@us01 @ingestion
Feature: Ingest medical inputs through one pipeline
  As a developer
  I want a common ingestion pipeline
  So that embedding configurations can be compared consistently

  Scenario Outline: Ingest each supported input type
    Given an eligible anonymized fixture of type "<type>" in the corpus manifest
    And a compatible deterministic embedding adapter
    When I ingest the fixture
    Then the source status is "succeeded"
    And the active records match the fixture's expected item count and modalities
    And every active record has the manifest source ID and checksum
    And every active record resolves to its original page or image asset

    @m1
    Examples: Text-bearing PDFs
      | type           |
      | searchable PDF |
      | scanned PDF    |

    @m3
    Examples: Inputs that produce image records
      | type           |
      | mixed PDF      |
      | PNG            |
      | JPEG           |

  @m1
  Scenario: Preserve a laboratory result through extraction and chunking
    Given a scanned laboratory page containing the row "Hemoglobina | 13,5 | g/dL | 12,0–16,0"
    And a controlled OCR fixture returning that row and its header
    When I ingest the page
    Then a text chunk contains the test name, value, unit, and reference range together
    And that chunk references the original page and the OCR extraction method
    And extraction confidence equals the OCR value or is null when not supplied

  @m1
  Scenario: Select extraction per page in a mixed PDF
    Given a two-page PDF with native text on page 1 and a scanned image on page 2
    And controlled native extraction and OCR outputs for those pages
    When I ingest the PDF
    Then page 1 text records have extraction method "native"
    And page 2 text records have extraction method "ocr"
    And both pages retain their original page numbers and fixture reading order

  @m1
  Scenario: Reingestion is idempotent
    Given a source successfully ingested with a fixed configuration
    When I ingest the same source checksum with the same configuration again
    Then the set of active record IDs is unchanged
    And the active record count is unchanged
    And no duplicate source chunk exists in that embedding space
```

### 5.2 US-02 — Portuguese search with source traceability

```gherkin
@us02 @retrieval
Feature: Retrieve traceable results for Portuguese queries
  As a researcher
  I want to search in Portuguese and inspect original evidence
  So that I can verify each retrieved result

  @m1
  Scenario: Return ordered text results with original-page references
    Given a text index with at least three fixture items and deterministic query vectors
    And the expected ranking for "Onde está o resultado da hemoglobina?" is frozen
    When I search for "Onde está o resultado da hemoglobina?" in text-only mode with limit 3
    Then exactly three text results are returned in the expected order
    And each result includes its record ID, source ID, page number, chunk text, and score
    And each source and page reference resolves to the corresponding original fixture

  @m3
  Scenario: Retrieve an image from a Portuguese text query
    Given an image index and a compatible text-to-image model fixture
    And filenames and captions do not contain the query terms
    And the frozen expected first result is image "chest-001"
    When I search for "Mostre a radiografia de tórax" in image-only mode with limit 3
    Then the first result references image "chest-001"
    And every returned item has modality "image" and a resolvable original asset URI
    And the query uses the model's text-query embedding path

  @m1
  Scenario: Search an empty compatible index
    Given an empty index compatible with the configured query model
    When I search for "Onde está o resultado da hemoglobina?" with limit 3
    Then an empty result list is returned without an error
    And no synthetic result or source reference is created
```

### 5.3 US-03 — Comparable retrieval evaluation

```gherkin
@us03 @evaluation
Feature: Compare text, image, and combined retrieval fairly
  As an evaluator
  I want to reuse fixed queries and relevance labels
  So that mode comparisons measure the configured retrieval behavior

  Scenario Outline: Evaluate a retrieval mode against the frozen dataset
    Given ten frozen Portuguese queries and frozen relevance labels
    And a compatible fixture index with at least three candidates for each query in "<mode>" mode
    When I evaluate "<mode>" mode
    Then the report contains exactly ten query entries with three ranked results each
    And each result includes its source, page or asset, score, relevance label, and relevance note
    And the report records the same query-set and relevance-label checksums as the inputs
    And Precision@3, Hit@3, and nDCG@3 match the independently precomputed fixture values within 0.000001

    @m2
    Examples: Text
      | mode       |
      | text-only  |

    @m3
    Examples: Image
      | mode       |
      | image-only |

    @m4
    Examples: Combined
      | mode       |
      | combined   |

  @m4
  Scenario: Fuse rankings without comparing incompatible raw scores
    Given text ranking "A,B,E" and image ranking "C,E,D"
    And reciprocal rank fusion with k equal to 60 and ranks starting at 1
    When I combine the rankings
    Then the fused score of each item is the sum of 1 / (60 + rank) over every list containing it
    And the fused scores are "A=1/61, B=1/62, C=1/61, D=1/63, E=1/62+1/63" within 0.000001
    And the result order is "E,A,C,B,D" by descending fused score, breaking equal-score ties by ascending record ID
    When I multiply all raw text similarity scores by 100 without changing their order
    Then the fused scores and result order are unchanged

  @m2
  Scenario: Report low relevance without hiding a completed experiment
    Given a completed fixture evaluation whose retrieved items all have relevance label 0
    When I generate the comparison report
    Then Precision@3, Hit@3, and nDCG@3 are all 0
    And the evaluation status is "completed"
    And the report preserves the irrelevant results and their failure notes
```

For metric assertions, ranks start at 1 and relevance labels 1 and 2 count as relevant. Per query:

- **Precision@3** = (number of returned results among ranks 1–3 with label ≥ 1) / 3. It always divides by 3, even when fewer than three results are returned.
- **Hit@3** = 1 if any returned result among ranks 1–3 has label ≥ 1, otherwise 0.
- **nDCG@3** = DCG@3 / IDCG@3, where DCG@3 = Σ over returned ranks r ≤ 3 of `(2^label − 1) / log2(r + 1)`. IDCG@3 applies the same formula to the three highest labels in that query's frozen relevance labels, sorted descending. Missing results contribute zero gain, and nDCG@3 is 0 when IDCG@3 is 0.

Each metric is macro-averaged as the arithmetic mean of the per-query values over the ten queries. Fixture expected values must be precomputed independently of the evaluator and checked in. The evaluator must record fewer than three results explicitly when fewer candidates exist, without padding with invented items.

### 5.4 US-04 — Interchangeable embedding adapters

```gherkin
@us04 @strategy
Feature: Select embedding adapters through configuration
  As a developer
  I want providers behind the embedding strategy contract
  So that adding a provider preserves extraction and evaluation workflows

  @m2
  Scenario Outline: Run the same pipeline with a configured adapter
    Given fixed extracted chunks, queries, relevance labels, and pipeline configuration
    And a contract-test implementation for engine "<engine>"
    When I select "<engine>" through configuration and run ingestion and evaluation
    Then the selected adapter receives the unchanged document chunks and query texts
    And the saved records identify that engine and its configured model separately
    And extraction, chunking, and evaluation configuration fingerprints are unchanged

    Examples:
      | engine        |
      | EmbedAnything |
      | FastEmbed     |
      | OpenAI        |

  @m2
  Scenario: Reject unsupported image retrieval before processing
    Given an adapter that does not support compatible text-to-image retrieval
    When I request an image-only run
    Then validation fails with an error identifying the engine, model, and missing capability
    And no extraction, embedding, or index-write operation is invoked

  @m2
  Scenario: Separate incompatible spaces even when dimensions match
    Given two models with equal vector dimensions and different embedding-space identities
    When I ingest the same source using both configurations
    Then their records are stored in separate indexes or namespaces
    When I query the first index using the second model's query vector
    Then the query fails with an embedding-space mismatch error before similarity search
```

Contract tests may substitute deterministic provider responses, but they must execute the real orchestration and adapter contract. They do not establish actual model quality or successful live provider integration. Separate integration runs must exercise each real adapter; remote runs require explicit corpus permission and credentials.

### 5.5 US-05 — Measurable quality and efficiency tradeoffs

```gherkin
@us05 @measurement @m2
Feature: Report quality alongside processing resources and costs
  As a researcher
  I want comparable quality and efficiency evidence
  So that I can assess the cost of a retrieval improvement

  Scenario: Save a reproducible comparison report
    Given two completed configurations evaluated against the same frozen corpus and queries
    And controlled clock, memory, and usage observations with known expected measurements
    When I generate the comparison report
    Then both configurations include retrieval metrics, ingestion duration, and embedding throughput
    And both include median and p95 query latency, peak local RAM, and index size
    And all reported measurements match the fixture's expected values and units
    And cold-start and warm-run measurements are reported separately
    And the report records hardware, corpus checksum, batch size, engine, model, and configuration fingerprints

  Scenario: Represent unavailable remote measurements honestly
    Given a remote provider run with known usage and a versioned price table
    And provider inference memory is not observable
    When I generate its cost and resource report
    Then estimated cost equals the fixture usage divided by the price table's unit quantity and multiplied by its unit price
    And the report identifies the price version, currency, usage units, unit quantity, and estimate status
    And provider inference memory is marked "unavailable"
    And local peak RAM is labeled as local process memory

  Scenario: Do not invent unknown costs
    Given a run with missing provider usage or missing pricing data
    When I generate its cost report
    Then estimated provider cost is null with the missing-data reason
    And unknown cost is not reported as zero
```

### 5.6 REL-01 — Reliability across failure and recovery

```gherkin
@rel01 @reliability @m1
Feature: Preserve index integrity and expose processing failures
  As a researcher
  I want failures to be explicit and recoverable
  So that experiment evidence remains trustworthy

  Scenario: A failed replacement preserves the committed revision
    Given source "S1" has a committed revision with checksum "old"
    And a changed version of "S1" has checksum "new"
    And a storage fault is injected before the new revision commit
    When I ingest the changed source
    Then the source attempt is marked "failed" with stage and reason
    And only the "old" revision remains searchable for that configuration
    And no partial records from the "new" revision are searchable
    When I remove the fault and retry the changed source successfully
    Then only the "new" revision is active for that configuration
    And no duplicate active records exist
    And saved experiment provenance still identifies the revision used by each earlier run

  Scenario: Recover from a transient embedding failure within the limit
    Given an embedding batch fails transiently on attempt 1 and succeeds on attempt 2
    And the configured maximum attempt count is 3
    When I ingest the source
    Then exactly two embedding attempts are made
    And the source status is "succeeded"
    And the run records one retry
    And each expected record is committed exactly once

  Scenario Outline: Stop retrying and continue independent sources
    Given source "S1" always fails with a "<error_class>" embedding error
    And independent source "S2" can be processed successfully
    And the configured maximum attempt count is 3
    When I ingest both sources
    Then "S1" has exactly <attempts> embedding attempts and status "failed"
    And "S1" has no newly committed records
    And "S2" has status "succeeded" and its expected committed records
    And the overall run status is "incomplete"
    And the CLI exits with a nonzero status
    And the report includes the failure stage, reason, and attempt count for "S1"

    Examples:
      | error_class | attempts |
      | transient   | 3        |
      | permanent   | 1        |

  Scenario Outline: Reject invalid embedding vectors
    Given source "S1" has a committed revision in a live isolated store
    And a changed version of "S1" whose adapter response contains a vector with "<defect>"
    When I ingest the changed source
    Then validation fails with the affected item ID and defect
    And no records from the new source revision are staged or committed
    And the active revision pointer for "S1" is unchanged

    Examples:
      | defect               |
      | incorrect dimensions |
      | NaN component        |
      | infinite component   |

  Scenario: Do not index unreadable text as valid content
    Given a required scanned text page whose OCR output is empty
    When I ingest the source
    Then the page is flagged "unreadable"
    And no text embedding is requested for that page
    And the source is not marked "succeeded"
    And the run is marked "incomplete" with the page-level reason

  Scenario: Make permitted truncation observable
    Given a chunk exceeds the configured model input limit
    And truncation is explicitly enabled for the experiment
    When I prepare that chunk for embedding
    Then the submitted input does not exceed the model limit
    And the record identifies the truncation and original and submitted input lengths
    And the report counts one truncation

  Scenario: Reject over-limit input when truncation is disabled
    Given a single table row whose token count exceeds the configured model input limit
    And truncation is disabled for the experiment
    When I ingest the source
    Then the source has status "failed" with stage "chunking" and an input-limit reason
    And no embedding is requested for that source revision
    And the run is marked "incomplete"

  Scenario: Block remote processing without corpus permission
    Given remote-provider processing is disabled for the corpus
    When I request ingestion with a remote embedding adapter
    Then validation fails with a corpus-permission error
    And no remote-provider request is sent
```

### 5.7 Cucumber for Rust — mandatory BDD test contract

Use the Rust `cucumber` crate (Cucumber.rs) for acceptance tests. Its **macro-friendly** API uses Rust attribute syntax with procedural macros `#[given]`, `#[when]`, and `#[then]` to map Gherkin step text directly to Rust functions. These attributes are supplied by the crate; they are not built-in Rust testing attributes.

- `#[given]` prepares the isolated `World`, fixtures, configurations, and fault injection.
- `#[when]` invokes the real application operation through its public interface and captures outputs, errors, and exit status where relevant.
- `#[then]` compares observed behavior with explicit expected values. It must fail on a mismatch, never merely log a result or assign the expected value to the observed state.
- Use parameterized expressions for repeated steps, such as `#[then(expr = "the active record count is {int}")]`.
- Keep deterministic BDD checks independent of external services, hardware timing, and model downloads by controlling provider responses and measurement sources. Real extraction and model integration tests complement these checks.

The planned test target is `tests/bdd.rs`, configured with `[[test]]`, `name = "bdd"`, and `harness = false` in `Cargo.toml`, using `cucumber` as a development dependency and Tokio for the asynchronous runner. Once implemented, run it with `cargo test --test bdd` from the crate directory.

The runner must build on `World::cucumber().fail_on_skipped()` with the project's derived `cucumber::World` type and the `cucumber::World` trait in scope, and must `.await` the run future. It must also write a machine-readable scenario report (the crate's `output-json` or `output-junit` writer, teed with the console writer) and flush it before the process exits. Use `run(...)` and exit explicitly after the report is flushed and the inventory is checked. `run_and_exit` alone is insufficient, because it exits before any later code runs. CI must fail for assertion failures, missing or ambiguous step definitions, skipped required scenarios, parsing failures, and an empty or accidentally filtered required suite. Inside the same `bdd` binary, after the run, verify executed scenario IDs against the required scenario inventory. Derive that inventory from the Gherkin blocks and milestone tags in this PRD, never from a hand-maintained list. A zero exit code alone does not prove coverage.

Publish scenario-level pass/fail results with the corpus and configuration fingerprints. Keep live-provider integration evidence separate from deterministic BDD evidence, and do not report an unexecuted integration as passed. This PRD specifies the suite to implement; the Gherkin blocks alone are not an implemented or executed test suite.

Implementation references: [Cucumber.rs step macros and World](https://github.com/cucumber-rs/cucumber/blob/main/README.md) and [failure on skipped steps](https://github.com/cucumber-rs/cucumber/blob/main/book/src/writing/tags.md).

## 6. Functional Requirements

### 6.1 Document ingestion

The system must:

- Accept PDF, PNG, and JPEG inputs.
- Assign a stable `source_id` to each input.
- Record a checksum to detect duplicate or changed inputs.
- Preserve page numbers and relationships between documents and extracted images.
- Report processing status and failures per source.
- Support rerunning ingestion without creating duplicate records.

### 6.2 Extraction and OCR

The system must:

- Extract native PDF text when available.
- Apply OCR to scanned pages or image regions requiring text extraction.
- Handle documents containing both searchable and scanned pages.
- Preserve reading order and page provenance.
- Preserve laboratory test names, values, units, and reference ranges together where possible.
- Extract or reference embedded images.
- Record extraction method and confidence when the extraction tool provides it.
- Flag unreadable or empty content instead of silently indexing it as valid text.

OCR and document extraction must remain independent of the embedding engine.

### 6.3 Chunking

The system must:

- Produce text chunks using a configurable, versioned chunking strategy.
- Prefer document structure, such as sections, paragraphs, and table rows.
- Keep laboratory rows attached to relevant headers and units.
- Preserve page and bounding-box references when available.
- Record chunk order and any overlap.
- Respect the selected embedding model’s input limits, measured in that model's own tokenizer units (tokens), and record any truncation as original and submitted token counts.
- When truncation is disabled, split oversized structural units where the strategy allows it. If an indivisible unit, such as a single laboratory row with its header, still exceeds the limit, fail the source at stage `chunking`. Never drop or cut it silently.

### 6.4 Embedding generation

The system must:

- Use EmbedAnything as the first implemented engine.
- Support FastEmbed and OpenAI through additional adapters.
- Allow future engines without changing ingestion or evaluation workflows.
- Support batch processing when the selected engine permits it.
- Record engine, model identifier, model revision, vector dimensions, and preprocessing configuration. Real-model runs must pin an immutable model revision (commit hash or content digest) in the saved experiment configuration and fail validation without one. Only deterministic test adapters may omit it.
- Use one canonical engine identifier per adapter in records, configuration, and reports: `embed_anything`, `fastembed`, `openai`. Gherkin display names (`EmbedAnything`, `FastEmbed`, `OpenAI`) map to these identifiers, and BDD asserts the canonical value.
- Validate required capabilities before starting a run.
- Reject unsupported modalities with an actionable error.

**Engine and model must be separate configuration choices.** A model executed through two engines should be distinguishable from two different models executed through the same engine.

### 6.5 Storage and indexing

The system must store:

- Embedding vectors.
- Chunk text, where applicable.
- Source and page identifiers.
- Metadata and extraction provenance.
- Image references, where applicable.
- Embedding and experiment configuration identifiers.

Each incompatible embedding space must use a separate index or namespace. Equal vector dimensions do not establish compatibility.

A small local vector store or exact-search implementation is sufficient for the initial corpus.

### 6.6 Retrieval modes

| Mode | Required behavior | Evaluation focus |
|---|---|---|
| Text-only | Embed Portuguese queries and retrieve extracted text | Terminology, OCR impact, laboratory-table retrieval |
| Image-only | Retrieve images from Portuguese text queries using a compatible cross-modal model | Visual relevance and medical-domain suitability |
| Combined | Retrieve text and image candidates and merge their rankings | Quality improvement relative to added cost |

Image-only text-to-image retrieval requires a model that aligns text and image embeddings into a compatible space.

Single-modality search orders results by descending similarity score and breaks equal scores by ascending `record_id`, so frozen rankings are deterministic.

Combined retrieval should initially use **rank-based fusion**, such as Reciprocal Rank Fusion. Raw similarity scores from unrelated models must not be treated as directly comparable.

A combined configuration is an ordered pair of embedding spaces (text space, image space) plus the fusion parameters. Each leg's query vector must match its own index space. The report names both spaces.

Before fusion, each modality contributes its top `fusion_depth` candidates. `fusion_depth` is a versioned configuration value recorded in every combined-mode report. Its default is the full candidate list of each index, which is feasible at this corpus size. Fused results are ordered by descending fused score, with equal scores broken by ascending `record_id`.

## 7. Architecture

Use the **Strategy pattern** to isolate embedding implementations.

```text
Corpus
  → Ingestion
  → Extraction / OCR
  → Chunking / Image Preparation
  → Embedding Strategy
      ├── EmbedAnything Adapter
      ├── FastEmbed Adapter
      ├── OpenAI Adapter
      └── Future Provider Adapter
  → Vector and Metadata Storage
  → Retrieval
  → Evaluation Report
```

The conceptual strategy contract must expose:

| Operation | Responsibility |
|---|---|
| `capabilities()` | Describe supported modalities and execution constraints |
| `model_info()` | Return model identity, dimensions, and embedding-space identity |
| `embed_documents()` | Generate embeddings for document text |
| `embed_queries()` | Generate embeddings using query-specific formatting where required |
| `embed_images()` | Generate image embeddings when supported |

In Rust, this contract should be represented through traits and provider adapters. Unsupported operations must return explicit capability errors.

Capabilities should describe:

- Text-document embedding.
- Text-query embedding.
- Image embedding.
- Compatible text-to-image retrieval.
- Batch support.
- Input limits.
- Local or remote execution.

Extraction, chunking, storage, fusion, and evaluation must have their own interfaces rather than becoming responsibilities of the embedding strategy.

## 8. Indexed Record Schema

Each record represents one searchable item in one embedding space. The same source chunk may produce multiple records across experimental configurations.

| Field | Type | Purpose |
|---|---|---|
| `record_id` | String | Unique indexed-record identifier: derived from `revision_id`, `chunk_id`, and `embedding_space_id` |
| `chunk_id` | String | Identifier of the source chunk, unique within a source revision across modalities |
| `source_id` | String | Identifier of the original document |
| `revision_id` | String | Immutable source revision: derived from source checksum, extraction fingerprint, and chunking fingerprint |
| `chunk_text` | String or null | Extracted text; null for image-only items |
| `modality` | Enum | `text` or `image` |
| `asset_uri` | String or null | Reference to the image asset |
| `metadata` | Object | Source, page, language, and extraction details |
| `embedding` | Object | Vector and model provenance |
| `experiment_id` | String | Experiment run that produced this record; immutable after commit |

A record is immutable after commit. A later experiment that reuses an unchanged active revision does not rewrite `experiment_id`. Instead, each experiment saves an append-only snapshot listing the `(embedding_space_id, source_id, revision_id)` triples it searched. Committed revisions are retained after replacement, so any saved experiment can be searched again against exactly the revisions it used.

**Illustrative text record**  
The vector is omitted for readability; a stored record must contain the complete numeric vector.

```json
{
  "record_id": "record-001",
  "chunk_id": "source-001-p2-text-3",
  "source_id": "source-001",
  "revision_id": "rev-<revision-fingerprint>",
  "chunk_text": "Hemoglobina: 13,5 g/dL. Intervalo de referência: 12,0–16,0 g/dL.",
  "modality": "text",
  "asset_uri": null,
  "metadata": {
    "source_file": "anonymized-lab-report-001.pdf",
    "source_checksum": "sha256:<source-checksum>",
    "document_type": "laboratory_report",
    "language": "pt-BR",
    "page_number": 2,
    "section": "Hemograma",
    "chunk_index": 3,
    "bounding_box": null,
    "extraction_method": "ocr",
    "extraction_engine": "<ocr-engine-and-version>",
    "extraction_confidence": null,
    "chunking_strategy": "structure-aware-v1",
    "anonymization_status": "verified"
  },
  "embedding": {
    "engine": "embed_anything",
    "engine_version": "<engine-version>",
    "model_id": "<selected-multilingual-model>",
    "model_revision": "<model-revision>",
    "dimensions": 768,
    "embedding_space_id": "<configuration-fingerprint>",
    "normalized": true,
    "distance_metric": "cosine",
    "vector": "<full-array-of-768-floating-point-values>"
  },
  "experiment_id": "baseline-text-001"
}
```

The model identifier, dimensions, normalization setting, and metric must reflect the actual selected configuration. Values above are illustrative.

Image records must reference the original asset. Image records keep `chunk_text` null. Any generated caption is stored only under `metadata.generated_caption`, marked as generated, and is never embedded as source text or used as image-only query text. Retrieval fixtures for image-only scenarios contain no captions.

## 9. Evaluation Plan

### 9.1 Query set

Create ten Portuguese queries grounded in the selected corpus. The following are initial candidates; adjust them to match the actual documents.

| # | Query | Target |
|---|---|---|
| 1 | Onde está o resultado da hemoglobina? | Laboratory row |
| 2 | Qual é o valor da glicemia e sua unidade? | Value and unit |
| 3 | Qual intervalo de referência aparece para a creatinina? | Reference range |
| 4 | Onde estão os resultados de colesterol e triglicerídeos? | Related laboratory rows |
| 5 | Qual página contém a conclusão do laudo? | Report section |
| 6 | Onde o laudo descreve os achados do exame? | Findings section |
| 7 | Mostre a radiografia de tórax presente no conjunto de exames. | Medical image |
| 8 | Encontre as imagens de ultrassonografia abdominal. | Medical images |
| 9 | Mostre a imagem associada a este laudo de ultrassonografia. | Linked report and image |
| 10 | Onde o documento informa a técnica utilizada no exame? | Technique section |

For visual evaluation, include cases where filenames or captions alone cannot resolve the query. Distinguish direct visual retrieval from retrieval using associated text.

### 9.2 Ground truth

Before comparing configurations:

- Identify relevant chunks, pages, or images for each query.
- Assign relevance labels: `0 = irrelevant`, `1 = partially relevant`, `2 = directly relevant`.
- Keep labels fixed across experimental runs.
- Use development queries for tuning and a frozen ten-query set for final comparison.

### 9.3 Required evidence

For each query and configuration, produce:

- The top three retrieved items.
- Source ID and page number.
- Chunk text or image reference.
- Retrieval score or fused rank score.
- Relevance label.
- A short explanation of relevance or failure.

Example output structure:

| Query | Rank | Retrieved item | Source/page | Relevance | Note |
|---|---:|---|---|---|---|
| Onde está o resultado da hemoglobina? | 1 | Extracted laboratory row | source-001 / 2 | 2 | Contains the requested result and unit |

### 9.4 Metrics

| Dimension | Metrics |
|---|---|
| Retrieval quality | Precision@3, Hit@3, nDCG@3 |
| Extraction fidelity | Accuracy of sampled test names, values, units, and reference ranges |
| Ingestion performance | Total duration, time per page, chunks per second |
| Embedding performance | Batch latency, throughput, model-load time |
| Search performance | Median and p95 query latency over repeated runs |
| Resource usage | Peak RAM, GPU memory when applicable, index size |
| Cost | Provider usage and estimated cost per ingestion run and query |
| Reliability | Failed pages, failed batches, retries, and truncations |

Report cold-start and warm-run measurements separately. Record hardware, batch size, corpus version, and all relevant configuration settings.

Measurement definitions:

- **Cold start:** the first operation in a fresh process. It includes model load (local engines) or the first connection (remote engines).
- **Warm run:** later operations in the same process, after one discarded warm-up query.
- **Query latency:** measured over `latency_repeats` warm repetitions of each frozen query (configured, default 20). The median and p95 use the nearest-rank method on the pooled samples.
- **Peak local RAM:** the process high-water mark (`getrusage` `ru_maxrss`, normalized to bytes; it is reported in bytes on macOS and kilobytes on Linux), not a point-in-time RSS sample.
- **GPU memory:** a number when the engine runs on a GPU and memory is observable, otherwise `"unavailable"` with the reason.
- **Extraction fidelity:** a frozen, checked-in sample of laboratory rows with expected test name, value, unit, and reference range. Report field-level accuracy over that sample for each extraction configuration.

For remote engines, local memory usage must not be presented as total inference resource usage.

## 10. Experiment Design

Run comparisons in stages to avoid changing every variable at once.

| Stage | Comparison | Variables held fixed |
|---|---|---|
| 1 | EmbedAnything text baseline | Corpus, extraction, chunking, queries |
| 2 | Alternative text engines and models | Corpus, extraction, chunking, relevance labels |
| 3 | Image retrieval | Image assets, queries, relevance labels |
| 4 | Combined retrieval | Text and image configurations, fusion method |
| 5 | Efficiency tuning | Selected model and evaluation dataset |

If the same model cannot run across engines, describe results as **end-to-end configuration comparisons**, rather than attributing differences solely to the engine.

## 11. Data Handling Requirements

- Use only anonymized study inputs.
- Check both document text and image pixels for identifying information.
- Inspect filenames and embedded metadata for identifiers.
- Keep remote-provider processing disabled unless explicitly enabled for the corpus. `remote_allowed` defaults to false when absent. The permission check runs during validation, before any remote client is constructed or any credential is read.
- Avoid recording document contents in operational logs.
- Keep API credentials outside indexed records and experiment reports, and outside version control (`.env` is git-ignored).
- Make anonymization a blocking eligibility gate. A source is eligible only when its manifest entry records completed checks for filename, embedded metadata (PDF info dictionary/XMP, image EXIF/IPTC/XMP), document text, and image pixels, with reviewer and date. The pipeline strips embedded image metadata from stored assets and refuses ineligible sources before extraction. Records carry `anonymization_status: "verified"` only when the gate has passed. Reports that contain chunk text or image references are generated only from gate-passed corpora. Raw per-run outputs stay out of version control, and only the final, reviewed evidence reports are committed.
- Prices are recorded per unit quantity as published by the provider (for example, USD per 1,000,000 tokens). Usage is never multiplied by a per-unit price that has not been scaled.

## 12. Deliverables

1. A Rust CLI or small service for ingestion, indexing, querying, and evaluation.
2. An embedding strategy interface with provider adapters.
3. A versioned corpus manifest.
4. An indexed-record schema and at least one actual indexed-record example.
5. A ten-query evaluation dataset with relevance labels.
6. Top-three retrieval results and relevance notes for every evaluated configuration.
7. A comparison report covering quality, latency, memory, storage, and cost.
8. Reproduction instructions and saved experiment configurations.
9. Versioned Gherkin feature files, Rust Cucumber step definitions, and a CI acceptance-test target.
10. BDD execution reports and failure-injection evidence covering reliability and recovery.

## 13. Acceptance Criteria

The experiment is complete when:

- Searchable PDFs, scanned documents, and medical images can be ingested.
- OCR is applied where required, with extraction provenance preserved.
- Every indexed item links back to its source and page or image asset.
- EmbedAnything, FastEmbed, and OpenAI adapters participate in text comparisons using suitable models.
- Engines can be selected through configuration without changes to pipeline orchestration.
- Unsupported modality combinations are rejected before processing.
- At least one compatible model is evaluated for text-to-image retrieval.
- Text-only, image-only, and combined retrieval are evaluated.
- All ten queries include top-three results and relevance assessments.
- Performance measurements and experiment configurations are saved.
- Reingesting unchanged inputs does not create duplicate records.
- All required section 5 scenarios pass in the Cucumber Rust suite, with no missing, ambiguous, or skipped steps and no empty required suite.
- Failure injection proves bounded retries, atomic source revision updates, invalid-vector rejection, and explicit incomplete-run reporting.
- Real adapter integration evidence accompanies deterministic BDD results; unavailable or unauthorized remote runs remain outstanding acceptance items.

Low retrieval quality is a valid experimental result. Completion depends on reproducible measurements and documented limitations, rather than an assumed accuracy target.

## 14. Implementation Milestones

**Cross-cutting milestone requirement:** Implement each feature’s Gherkin scenarios and Rust Cucumber steps alongside its behavior. Reliability scenarios and CI execution are required throughout all milestones. A milestone is done when every scenario and example row tagged with that milestone or an earlier one (section 5) is present and passes, and none is skipped.

**Milestone 1 — Text baseline:** Ingest searchable and scanned reports, implement OCR and chunking, generate embeddings with EmbedAnything, and retrieve traceable text results.

**Milestone 2 — Engine comparison:** Add FastEmbed and OpenAI strategies, isolate embedding spaces, and run the fixed evaluation suite.

**Milestone 3 — Image retrieval:** Index radiographs and ultrasound images with a compatible cross-modal model and evaluate Portuguese text queries.

**Milestone 4 — Combined evaluation:** Add rank-based fusion, compare all three retrieval modes, and publish the reproducible experiment report.
