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
