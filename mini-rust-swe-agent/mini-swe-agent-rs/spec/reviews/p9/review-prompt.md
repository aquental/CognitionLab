# Adversarial review: phase P9 of mini-swe-agent-rs (documentation)

You are an adversarial reviewer. Review what phase P9 of the Rust port of mini-swe-agent delivered. The current directory is `mini-rust-swe-agent/`. **Read only files under this directory.**

Context:
- The plan is `mini-swe-agent-rs/spec/migration-plan.md`. P9 is the "P9 Docs" row of its §4 phase table. Its "done" criteria: the README lists every divergence of §2.4 and every decision's outcome (§5.2), the commands that were run and their results, and links to the parity evidence; the paths in `CLAUDE.md`, `README.md` and `spec/prompt/migrate.md` (this directory) still match the crate.
- The Python reference is `mini-swe-agent/` (read-only). The crate is `mini-swe-agent-rs/`.
- Owner decisions not to re-argue: D1, D2, D4, D5, D9, D11.
- Earlier phases were reviewed already; their findings are in `mini-swe-agent-rs/spec/reviews/p*/FINDINGS.md`. Do not repeat a finding fixed or refuted there, unless the fix or the reason is wrong.
- No network calls. No API key is available.

Files delivered in P9:
- `mini-swe-agent-rs/README.md` (rewritten sections: Status, Usage, Running the BDD features, Divergences from Python, Decisions, Evidence)
- `mini-swe-agent-rs/tests/docs.rs` (checks of the done criteria), `mini-swe-agent-rs/tests/local_env.rs` (new test `timeout_kills_a_shell_that_left_its_process_group`)
- `mini-swe-agent-rs/spec/migration-plan.md` (status line, §6.1), `mini-swe-agent-rs/spec/reviews/p8/FINDINGS.md` (C8 closed)
- `README.md` and `CLAUDE.md` in this directory

What to do:
1. Check each P9 done criterion. Verify that every claim in the documentation is true against the code, the tests and the Python reference (cite file:line): test names cited as evidence exist and test what is claimed; counts (scenarios, cases, tests, findings) match the files; commands are correct and runnable as written; behavior described in Usage and the divergence table matches the code.
2. Look for: stale or contradictory statements across the crate README, the plan, `README.md` and `CLAUDE.md`; overclaims (evidence that does not prove what it says); missing divergences that the code has but the docs do not record; checks in `tests/docs.rs` that can pass vacuously or are brittle; the new `local_env` test being flaky or platform-dependent.
3. Judge `tests/docs.rs` and the new test by Rust best practices.

Do not edit any file. Do not run network installs or network calls.

Output format (Markdown):
- A short verdict (1-3 sentences).
- A numbered list of findings. For each: severity (critical / high / medium / low), the file and line, the problem, evidence, and a concrete recommended fix.
- If you find nothing in a category, do not invent findings. Precision matters more than volume.
