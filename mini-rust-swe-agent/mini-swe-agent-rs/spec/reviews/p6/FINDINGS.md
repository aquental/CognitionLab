# P6 findings: adversarial review of the parity harness and BDD

- **Date:** 2026-11-24
- **Code reviewed:** commit `b96efab` (branch `mini-swe-agent-rs/p6-parity-bdd`), in a detached worktree. The
  P2–P5 review fixes were merged into P6 afterwards.
- **Prompt:** `review-prompt.md` in this directory. Every reviewer also got the Rust best-practices line; Antigravity
  also got the `rust-dev` plugin line and the no-shell-commands line; Grok got a `--rules` note for headless runs.
- **Raw outputs:** `codex.md`, `grok.md`, `agy.md`, copied byte for byte (checked with `cmp`). Grok's output starts
  with progress lines; they are kept because the copy must be exactly what the CLI printed.
- **Runs:** read-only, in parallel, no permission bypass, no retry.

  | Reviewer | Command | Exit | Time |
  |---|---|---|---|
  | Codex | `codex exec -s read-only` | 0 | 178 s |
  | Antigravity | `agy --mode plan -p` | 0 | 348 s |
  | Grok | `grok --permission-mode plan --output-format plain --rules … -p` | 0 | 1920 s (one auto-compaction) |

- **Also handled here:** two P7-review findings about P6 code (P7 C6 and C8).
- **Fixes:** test-first for the comparator (new mutation tests failed or did not compile before the fix). The new
  scenarios pass on both sides; they pin behavior that had no differential test.
- **Gates:** on macOS with the reference, 27 of 27 scenarios are equal and BDD passes 27 scenarios and 135 steps.
  The full-suite Linux gate (rebuilt image) exits 0.

Dispositions: **A** = accepted; **P** = partly accepted; **R** = refuted.

## Codex (9 findings: 2 high, 4 medium, 3 low)

| # | Sev. | Finding | Disp. | Resolution |
|---|---|---|---|---|
| C1 | High | The BDD manifest counts a `Given` as a completed test; duplicates collapse | A | A scenario id is recorded only after `support::check` passed; the manifest requires each id exactly once |
| C2 | High | No template-failure or save-on-failure cases; no configured output path | A | `invalid_template` (an undefined variable: no query, no exit message, no file; Python `UndefinedError` mapped to `TemplateError`) and `save_on_failure` (fatal in the loop, the saved file compared). Scenarios with `"output_path": true` save to `<base>/<id>/traj.json` on both sides; the file is compared in the state layer. New Gherkin steps check that a file was or was not saved |
| C3 | Medium | Layers or counters missing on both sides pass | A | `compare_layers(kind, …)` requires every layer of the scenario kind and rejects unknown keys; a missing `remaining_outputs` is a problem; mutation tests remove each layer from both sides |
| C4 | Medium | The Python checkout is not verified as the oracle | P | The runner reports `minisweagent.__file__`; the harness requires it to be inside the reference checkout (also in the Linux image). Copying the reference to a temporary directory on macOS is not done: the check catches an interpreter that imports another copy |
| C5 | Medium | `HOME`, `TMPDIR` and the config dir are shared across scenarios; snapshots skip symlinks and empty directories | A | Both runners recreate the three directories before each scenario; snapshots record directories and symlink targets |
| C6 | Medium | The background-child scenario does not prove the child died; no outer deadline | P | The descendant's death is proven in `tests/local_env.rs` (P3 review fixes: `proc_state`, `dead_after_wait`). The Python harness now runs under a 900 s deadline and is killed after it |
| C7 | Low | Integer vs equal float above 2^53 rejected | A | An integer equals a float only when the float is exactly that integer (`i128` domain); test with 2^54 and -2^60 |
| C8 | Low | `failing_command` never runs a missing executable | A | Renamed to what it tests (a missing file and a nonzero exit). A missing executable is not added: `/bin/sh` and `/bin/bash` print different texts (D11) |
| C9 | Low | Lint boundary bypassed; recorders clone their buffers | P | `tests/bdd.rs` steps return `Result` with no crate-level allows (done in P7); `compare.rs` re-denies the four lints for itself. The recorder clones stay: test-only and small |

## Grok (8 findings: 2 high, 2 medium, 4 low)

| # | Sev. | Finding | Disp. | Resolution |
|---|---|---|---|---|
| G1 | High | `long_output` can pass without the truncation branch (`seq` missing on macOS) | P | `seq` is at `/usr/bin/seq` on macOS, so the scenario does reach the branch. The gap is real: `expect.observation_contains` now requires the warning sentence of `default.yaml` on both sides |
| G2 | High | The Linux image does not contain the pinned Python | P | `RUN uv python install 3.15.2` at build time. The reference's packages are still synced at run time, because the reference (with its git-ignored `uv.lock`) is mounted only then |
| G3 | Medium | `MSWEA_RS_NO_REFERENCE=1` exits successfully without running | R | It is the explicit switch of the Rust-only gate. It prints that the suite was skipped ("this is not a pass", since P7), and the README never records a skipped suite as passed. Without the switch and without the reference, the harness fails |
| G4 | Medium | No invalid-template scenario | A | Same as C2 |
| G5 | Low | `failing_command` description | A | Same as C8 |
| G6 | Low | `max_concurrent_scenarios(4)` and the real-time wall-time scenario | A | Set to 1: steps block, so scenarios ran one at a time anyway |
| G7 | Low | The well-formed test only requires 20 ids | A | It requires the exact set of 27 ids |
| G8 | Low | The authorized mutation does not change `environment_type` | A | Added |

## Antigravity (7 findings: 4 medium, 3 low)

| # | Sev. | Finding | Disp. | Resolution |
|---|---|---|---|---|
| A1 | Medium | Missing invalid-template and save-on-failure scenarios | A | Same as C2 |
| A2 | Medium | `failing_command` does not run a missing command | A | Same as C8 |
| A3 | Medium | The Python side reads the crate's `default.yaml` | A | It reads `builtin_config_dir / "default.yaml"` from the reference package (`tests/yaml.rs` already requires the crate's copy to equal it byte for byte) |
| A4 | Medium | `expect.fatal` ignored; `remaining_outputs` defaults to 0; fatal texts removed instead of replaced; any timestamp type normalized; empty layers equal | A | `check()` compares `expect.fatal` with the error and returned values; the count is required; `content`, `exception_str` and `traceback` are replaced only when they are strings; only numeric timestamps are normalized; layers are required (C3). Mutation tests for each |
| A5 | Low | `EXECUTED` recorded in `Given`; `max_concurrent_scenarios(4)` is a no-op | A | Same as C1 and G6 |
| A6 | Low | The image does not pre-install Python | A | Same as G2 |
| A7 | Low | The support module's `#![allow]` covers `compare.rs` | A | `compare.rs` denies the four lints again and no longer indexes (`m["role"]`) |

## P7-review findings about P6 code

| # | Sev. | Finding | Disp. | Resolution |
|---|---|---|---|---|
| P7 C6 | Medium | No invalid-template scenario; the Python runner builds the agent outside the outcome capture | A | `invalid_template` (C2). Both sides fail inside `run` (the initial render), which is inside the capture |
| P7 C8 | Medium | Comparator: required fields missing on both sides; big integers vs equal floats | A | Same as C3 and C7 |

## Cross-reviewer overlap

| Problem | Reviewers |
|---|---|
| Missing invalid-template scenario | C2, G4, A1, P7 C6 |
| `failing_command` description | C8, G5, A2 |
| Manifest recorded before the comparison | C1, A5 |
| Vacuous comparator passes | C3, A4, P7 C8 |
| Python not baked into the image | G2, A6 |
| Big integers vs floats | C7, P7 C8 |
