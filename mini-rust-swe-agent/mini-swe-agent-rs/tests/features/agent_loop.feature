Feature: The text-based agent loop matches the Python reference
  Every scenario runs the same JSON script (parity/scenarios/) through the Python reference
  and through the Rust library, with one explicit environment and a fresh workspace.

  Scenario: A command timeout keeps the partial output, then the agent recovers
    Given the scenario "command_timeout_partial_output"
    When I run it in the Python and Rust harnesses
    Then each harness made exactly 2 model queries
    And both end with exit status "Submitted"
    And the normalized results are equal

  Scenario: Cost exactly at the limit stops the run before the next query
    Given the scenario "cost_limit"
    When I run it in the Python and Rust harnesses
    Then each harness made exactly 2 model queries
    And both end with exit status "LimitsExceeded"
    And the normalized results are equal

  Scenario: A command failing on a missing file and a nonzero exit are observed, not fatal
    Given the scenario "failing_command"
    When I run it in the Python and Rust harnesses
    Then each harness made exactly 3 model queries
    And both end with exit status "Submitted"
    And the normalized results are equal

  Scenario: max_consecutive_format_errors 0: billed format errors reach the cost limit
    Given the scenario "format_errors_count_against_cost_limit"
    When I run it in the Python and Rust harnesses
    Then each harness made exactly 3 model queries
    And both end with exit status "LimitsExceeded"
    And the normalized results are equal

  Scenario: Two format errors, then a clean step resets the counter, then submit
    Given the scenario "format_errors_then_recovery"
    When I run it in the Python and Rust harnesses
    Then each harness made exactly 5 model queries
    And both end with exit status "Submitted"
    And the normalized results are equal

  Scenario: MSWEA_GLOBAL_CALL_LIMIT=2 fails on the second call (RuntimeError in Python)
    Given the scenario "global_call_limit"
    When I run it in the Python and Rust harnesses
    Then each harness made exactly 2 model queries
    And both end with exit status "GlobalLimitExceeded"
    And the normalized results are equal

  Scenario: A scripted RuntimeError is fatal and keeps its class name
    Given the scenario "injected_runtime_error"
    When I run it in the Python and Rust harnesses
    Then each harness made exactly 1 model query
    And both end with exit status "RuntimeError"
    And the normalized results are equal

  Scenario: The two replies of the upstream local.traj.json fixture: echo, then submit
    Given the scenario "local_traj_seed"
    When I run it in the Python and Rust harnesses
    Then each harness made exactly 2 model queries
    And both end with exit status "Submitted"
    And the normalized results are equal

  Scenario: An output over 10,000 characters goes through the template's truncation branch
    Given the scenario "long_output"
    When I run it in the Python and Rust harnesses
    Then each harness made exactly 2 model queries
    And both end with exit status "Submitted"
    And the normalized results are equal

  Scenario: Non-ASCII text and CRLF line endings in command output
    Given the scenario "non_ascii_and_crlf_output"
    When I run it in the Python and Rust harnesses
    Then each harness made exactly 2 model queries
    And both end with exit status "Submitted"
    And the normalized results are equal

  Scenario: Running past the scripted outputs is fatal (IndexError in Python)
    Given the scenario "outputs_exhausted"
    When I run it in the Python and Rust harnesses
    Then each harness made exactly 2 model queries
    And both end with exit status "OutputsExhausted"
    And the normalized results are equal

  Scenario: Three format errors in a row end the run with RepeatedFormatError
    Given the scenario "repeated_format_errors"
    When I run it in the Python and Rust harnesses
    Then each harness made exactly 3 model queries
    And both end with exit status "RepeatedFormatError"
    And the normalized results are equal

  Scenario: save() merges extra dicts last
    Given the scenario "save_with_extra_dicts"
    When I run it in the Python and Rust harnesses
    Then each harness made exactly 1 model query
    And both end with exit status "Submitted"
    And the normalized results are equal

  Scenario: A command killed by a signal reports -9
    Given the scenario "signal_exit"
    When I run it in the Python and Rust harnesses
    Then each harness made exactly 2 model queries
    And both end with exit status "Submitted"
    And the normalized results are equal

  Scenario: Scripted /sleep and /warning outputs are skipped without a query or a charge
    Given the scenario "sleep_and_warning"
    When I run it in the Python and Rust harnesses
    Then each harness made exactly 1 model query
    And both end with exit status "Submitted"
    And the normalized results are equal

  Scenario: The step limit is checked before the next query
    Given the scenario "step_limit"
    When I run it in the Python and Rust harnesses
    Then each harness made exactly 1 model query
    And both end with exit status "LimitsExceeded"
    And the normalized results are equal

  Scenario: Three commands that write and read files, then submit
    Given the scenario "submit_after_steps"
    When I run it in the Python and Rust harnesses
    Then each harness made exactly 4 model queries
    And both end with exit status "Submitted"
    And the normalized results are equal

  Scenario: Submit on the first step, with a multi-line submission
    Given the scenario "submit_step1"
    When I run it in the Python and Rust harnesses
    Then each harness made exactly 1 model query
    And both end with exit status "Submitted"
    And the normalized results are equal

  Scenario: Wall-time limit on real clocks: sleep 2 with a 1 s limit; the second reply is never consumed
    Given the scenario "wall_time_limit"
    When I run it in the Python and Rust harnesses
    Then each harness made exactly 1 model query
    And both end with exit status "TimeExceeded"
    And the normalized results are equal

  Scenario: An undefined template variable fails before the loop and saves nothing
    Given the scenario "invalid_template"
    When I run it in the Python and Rust harnesses
    Then each harness made exactly 0 model queries
    And both end with exit status "TemplateError"
    And no trajectory file was saved
    And the normalized results are equal

  Scenario: A fatal error inside the loop still saves the trajectory file
    Given the scenario "save_on_failure"
    When I run it in the Python and Rust harnesses
    Then each harness made exactly 2 model queries
    And both end with exit status "OutputsExhausted"
    And a trajectory file was saved
    And the normalized results are equal
