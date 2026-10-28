# Harness self-checks (not PRD requirements): every scenario gets its own store,
# adapter call log, and fault plan.
@harness
Feature: BDD harness isolation

  Scenario: A scenario writes to its own store and call log
    Given a fresh scenario world
    When the scenario writes a marker file into its store
    And the scenario embeds one document through the deterministic adapter
    Then the store contains only the marker file
    And the adapter call log has exactly 1 call
    And the last embedding attempt succeeded

  Scenario: State does not leak between scenarios
    Given a fresh scenario world
    Then the store is empty
    And the adapter call log is empty

  Scenario: Fault flags apply only to the scenario that set them
    Given a fresh scenario world
    And the adapter fails the next 1 call with a "transient" error
    When the scenario embeds one document through the deterministic adapter
    Then the last embedding attempt failed with class "transient"
    And the adapter call log has exactly 1 call
