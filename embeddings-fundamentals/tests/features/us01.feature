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
