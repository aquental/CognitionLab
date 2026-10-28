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

  @m1
  Scenario: Search an empty compatible index
    Given an empty index compatible with the configured query model
    When I search for "Onde está o resultado da hemoglobina?" with limit 3
    Then an empty result list is returned without an error
    And no synthetic result or source reference is created
