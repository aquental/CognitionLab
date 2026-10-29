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
