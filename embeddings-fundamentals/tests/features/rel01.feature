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
