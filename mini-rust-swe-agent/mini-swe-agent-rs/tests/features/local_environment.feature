Feature: LocalEnvironment matches the Python reference
  Each scenario runs the same commands through Python's LocalEnvironment and the Rust one.

  Scenario: A missing cwd is a returncode -1 output with Python's error text
    Given the scenario "env_missing_cwd"
    When I run it in the Python and Rust harnesses
    Then command 1 returned code -1
    And the normalized results are equal

  Scenario: A signal exit is negative
    Given the scenario "env_signal_exit"
    When I run it in the Python and Rust harnesses
    Then command 1 returned code -9
    And command 2 returned code -15
    And the normalized results are equal

  Scenario: cd and export do not persist between commands, files do
    Given the scenario "env_stateless_shell"
    When I run it in the Python and Rust harnesses
    Then command 2 output contains "unset"
    And the normalized results are equal

  Scenario: The submission marker after leading blank lines; a failing command never submits
    Given the scenario "env_submission_marker"
    When I run it in the Python and Rust harnesses
    Then command 1 returned code 3
    And command 2 submitted "final answer\n"
    And the normalized results are equal

  Scenario: A background child holding the pipe is killed at the deadline; partial output kept
    Given the scenario "env_timeout_background_child"
    When I run it in the Python and Rust harnesses
    Then command 1 returned code -1
    And command 1 output contains "hi"
    And the normalized results are equal

  Scenario: UTF-8 output and invalid bytes
    Given the scenario "env_unicode_output"
    When I run it in the Python and Rust harnesses
    Then command 1 output contains "olá ✓"
    And command 2 output contains "bad�end"
    And the normalized results are equal
