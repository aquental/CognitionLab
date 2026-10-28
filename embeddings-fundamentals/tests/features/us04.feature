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
