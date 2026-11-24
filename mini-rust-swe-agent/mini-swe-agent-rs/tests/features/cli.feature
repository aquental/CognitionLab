Feature: The mini-swe-agent-rs command line
  The P7 gate: Python's `mini` (default agent, deterministic model) and the Rust CLI get the
  same arguments, one after the other in a fresh workspace at the same path, and must exit
  with the same code and write equal normalized trajectories.

  Scenario: A command, then the submission
    Given the CLI case "submit"
    When I run the Python and Rust CLIs
    Then both CLIs exit with code 0 and write equal trajectories

  Scenario: The cost limit from -l stops the run before the next query
    Given the CLI case "cost_limit_flag"
    When I run the Python and Rust CLIs
    Then both CLIs exit with code 0 and write equal trajectories

  Scenario: The task comes from run.task in the config
    Given the CLI case "task_from_config"
    When I run the Python and Rust CLIs
    Then both CLIs exit with code 0 and write equal trajectories

  Scenario: Running out of scripted outputs is fatal, after the trajectory is saved
    Given the CLI case "outputs_exhausted"
    When I run the Python and Rust CLIs
    Then both CLIs exit with code 1 and write equal trajectories

  Scenario: The task comes from -t first
    Given a Rust CLI sandbox
    When the Rust CLI runs with task flag "from flag", config task "from config" and stdin "from stdin"
    Then the task message contains "from flag"

  Scenario: Without -t, the task comes from run.task
    Given a Rust CLI sandbox
    When the Rust CLI runs with task flag "", config task "from config" and stdin "from stdin"
    Then the task message contains "from config"

  Scenario: Without -t or run.task, the task is read from stdin
    Given a Rust CLI sandbox
    When the Rust CLI runs with task flag "", config task "" and stdin "from stdin"
    Then the task message contains "from stdin"

  Scenario: A config name is found in MSWEA_CONFIG_DIR before the built-ins
    Given a Rust CLI sandbox
    And "default.yaml" in MSWEA_CONFIG_DIR sets the system template to "custom system"
    When the Rust CLI runs with config "default" and task "t"
    Then the system message starts with "custom system"

  Scenario: Without a file, a config name finds the built-in config
    Given a Rust CLI sandbox
    When the Rust CLI runs with config "default" and task "t"
    Then the system message starts with "You are a helpful assistant"
