# Round 2 findings: adversarial review of migration-plan.md (revision 4)

- **Date:** 2026-11-23
- **Plan reviewed:** revision 4, the merge of the old `spec/PLAN.md` and `spec/STUDY.md` into the plan (SHA-256 `f3903f2c32c572d57579524c8fa111ed32a4f147accfcecd0b274c6f17abee58`).
- **Prompt:** `review-prompt.md` in this directory.
  - Every reviewer also got a first line asking it to apply Rust development best practices.
  - Antigravity's prompt had one more line telling it to use its `rust-dev` plugin.
- **Raw outputs:** `codex.md`, `grok.md`, `agy.md` in this directory, copied byte for byte from each CLI.
- **Runs:** each one was managed by its own subagent, all three in parallel, read-only, with no permission bypass and no retry.

  | Reviewer | Command | Exit | Time |
  |---|---|---|---|
  | Codex | `codex exec -s read-only` | 0 | ~4 min |
  | Antigravity | `agy --mode plan -p` | 0 | ~5.4 min |
  | Grok | `grok --permission-mode plan --output-format plain -p` | 0 | ~24.7 min |

- **Result:** every finding below is reflected in revision 5 of `../../migration-plan.md`, unless its disposition says otherwise. Claude checked the disputed claims against the Python code before deciding. The checks covered:
  - `run/mini.py:22`, `:68` and `:94-97`;
  - `run/utilities/config.py:62-66`;
  - `agents/default.py:88-94`, `:130-157`;
  - `models/litellm_model.py:25-46`, `:156-160`;
  - `models/test_models.py:75-104`;
  - `environments/local.py:58-59`, `:89`;
  - `run/hello_world.py:32-36`;
  - `tests/environments/test_local.py:219-228`;
  - the `serde-saphyr` 2.2.0 `Options` docs, where `strict_booleans` and `duplicate_keys` exist.

Dispositions: **A** = accepted; **P** = partly accepted (the fix differs, or a sub-claim was wrong); **R** = refuted.

## Codex (14 findings: 5 high, 8 medium, 1 low)

| # | Sev. | Finding | Disp. | Resolution in revision 5 |
|---|---|---|---|---|
| C1 | High | `recv_timeout` loop can pass the deadline on a continuously readable pipe; a detached reader leaks its thread and pipe; disconnect conflates EOF with errors | A | §3.7 item 4 rewritten as a single-thread `nix::poll` loop: monotonic absolute deadline checked every iteration, EOF vs read error distinct, reader dropped after the grace drain |
| C2 | High | Untagged `Action` with a defaulted `command` swallows `{"raise": …}` | A | §3.2: decoding by key presence, `raise` first, a malformed `raise` is an error, original JSON kept, four tests |
| C3 | High | Phase table: P5 needs P6's save, P7's `hello_world` needs P8's model, M1's P04 needs P8's HTTP adapter | A | §4: `serialize`/`save` moved to P5; `hello_world` to P8; P04 split into M1 (query history) and M2 (wire payload) |
| C4 | High | The P7 Python CLI run would pick `InteractiveAgent` and the setup wizard | A | §4 P7: exact Python and Rust invocations, `MSWEA_CONFIGURED=true`, `--agent-class default`, no `raise` specs, both exit codes asserted |
| C5 | High | Inherited credentials reach the oracle; a literal `api_key` is saved in the trajectory; whole messages are logged | A | §3.15 item 4 explicit credential-free environment for both sides; §3.8 key only in transport state, redacted in dumps (§2.4 row); §3.13 metadata-only logging with a sentinel-secret test |
| C6 | Medium | Trap 27 is wrong: limit 1 blocks at 1.999 s; no shared clock for the differential wall-time scenario | A | Trap 27 corrected; `ManualClock` boundary tests (0.999 / 1.000 / 1.999 / backward jump); the differential scenario uses real clocks on both sides |
| C7 | Medium | `msg.actions()?` before the push changes the saved history on malformed actions | A | §3.4: `query` pushes and charges, as in Python; `step` decodes actions from the last message afterwards |
| C8 | Medium | Comparator selectors are field names, not paths; workspace and output paths differ; query history is not the provider payload | A | §3.15 item 4: exact per-record JSON paths; D6 mapping for both exit statuses; integers above 2^53; same absolute paths through fresh copies; history vs payload separated |
| C9 | Medium | The deterministic model cannot prove the text path; the heredoc case is missing | A | §3.15 item 4 raw-response differential (P8); parser cases gain adjacent fences, the heredoc fence and an unmatched optional group |
| C10 | Medium | Initial messages must both be built before the append; the `output` collision must be lazy | A | Trap 19, §3.5 state diagram, §3.9 collision rule |
| C11 | Medium | `env::vars` panics on non-Unicode data; `/sleep N` duration; thread spawn; non-finite floats do not fail `to_value` | A | §3.12 `vars_os` (§2.4 row); §3.8 `Duration::try_from_secs_f64`; the thread is gone (C1); §3.3 rationale fixed |
| C12 | Medium | Non-interactive bash sources `BASH_ENV` | A | §3.7 item 1: inherited `BASH_ENV` removed, kept if set in `config.env`; §2.4 row; fixture; R11 |
| C13 | Medium | `serde-saphyr` defaults infer YAML 1.1 booleans | A | §3.10 item 5 loader policy (`strict_booleans`, last-wins duplicate keys, budget, the edge cases); D5 |
| C14 | Low | BDD and parity targets need shared support code; unmatched steps must fail | A | `tests/support/`, `fail_on_skipped`, scenario-manifest check (§3.1, §3.15 item 5) |

## Antigravity (12 findings: 5 high, 5 medium, 2 low)

| # | Sev. | Finding | Disp. | Resolution in revision 5 |
|---|---|---|---|---|
| A1 | High | `Action::Raise` unreachable under `untagged` | P | Problem accepted. The proposed fix (declare `Raise` first) still turns a malformed `raise` into an empty command, so decoding is by key presence instead (C2) |
| A2 | High | Trap 27 factual error | A | Same as C6 |
| A3 | High | P5 → P6 and P7 → P8 dependencies | A | Same as C3; P7 returns an "available from P8" error for `--model-class textbased`, and the `TextbasedModelConfig` struct lands in P7 for the typed dumps |
| A4 | High | Separate temp workspaces make every path differ, while "nothing else is normalized" | P | Problem accepted. Instead of placeholder substitution, both sides run at the **same** absolute path from fresh copies, so no string rewriting can mask a difference; canonicalization only if the OS forces it |
| A5 | High | The Linux container gate cannot run the parity and BDD targets | A | §3.15 item 6: a Rust-only gate plus a full-suite gate (`parity/Dockerfile` with `uv`); each phase names its gate |
| A6 | Medium | `query` vs `step` double push | P | Problem accepted. Its fix decodes actions before the push, which diverges from Python's order (C7); `query` pushes and `step` decodes afterwards |
| A7 | Medium | `TextbasedModelConfig` misses Python fields; env-backed defaults unspecified | A | §3.8 item 0: every field, env-backed defaults from `HostEnv` after deserialization |
| A8 | Medium | Kill can hang if the shell left its group; unbounded `mpsc` | P | `child.kill()` after `killpg` accepted (§2.4 row: Python itself hangs). The channel no longer exists (C1); the output buffer stays unbounded on purpose, as `communicate()` is |
| A9 | Medium | D5: gate `serde-saphyr` on `Value` edge cases, else `yaml-rust2` | A | Same as C13; empty and comment-only files added |
| A10 | Medium | `harness = false` loses `cfg(test)`; `allow-indexing-slicing-in-tests`; Cucumber silent skips | A | §3.5 lint rules, §3.15 item 5 |
| A11 | Low | `uname -p` per step; CPython maps `unknown` to `""` | A | §3.7 item 9: computed once, `unknown` → `""` |
| A12 | Low | Protocol recording unspecified | A | §3.15 item 4 recording wrappers in both runners |

## Grok (11 findings: 6 high, 5 medium)

| # | Sev. | Finding | Disp. | Resolution in revision 5 |
|---|---|---|---|---|
| G1 | High | The default config is `mini.yaml` at `run/mini.py:22`, not `mini_textbased.yaml` | A | Verified. §3.10 cites it correctly; §2.4 row; trap 16 |
| G2 | High | The third task source is a multi-line prompt after a setup wizard, not stdin | A | Verified. Trap 16 states Python's behavior; §3.12 reads stdin (§2.4 row); the wizard is never run; P7 sets `MSWEA_CONFIGURED` |
| G3 | High | `raise` decodes as an empty `Command` | A | Same as C2 |
| G4 | High | `ManualClock` cannot be moved; the parity harness cannot share it with Python | P | Accepted: settable `ManualClock`, `SystemClock` in the parity harness, the test advances the clock inside `execute`. Its example boundary ("1.999 still allowed") holds only for a limit of 2; with trap 27's limit of 1 it repeats revision 4's error, so the tests use C6's values |
| G5 | High | `ManualClock` behind `cfg(test)` is invisible to integration tests | P | Problem accepted. Instead of a `test-support` feature, `ManualClock` is a small public type (§2.2, §3.1) |
| G6 | High | `hello_world.rs` needs P8; Python's example is tool-calling | A | Verified (`hello_world.py:32-36`). Moved to P8; §2.4 row |
| G7 | Medium | A missing global `.env` must be an empty overlay | A | §3.12, with a test from an empty config dir |
| G8 | Medium | `run` does not reset `n_consecutive_format_errors` | A | Verified (`default.py:88-91`). Trap 19 and the reuse test |
| G9 | Medium | `serde-saphyr` defaults: YAML 1.1 booleans, duplicate keys rejected | A | Same as C13; last-wins duplicates match PyYAML |
| G10 | Medium | `echo -e` in the ported multiline test is shell-sensitive | A | Verified (`test_local.py:219-228`). Rust-only test, out of `parity/scenarios`; R11 |
| G11 | Medium | A panicking reader thread turns into a full-length `TimeoutExpired` | P | Problem accepted. Fixed by removing the thread altogether (C1), not by `catch_unwind` |

## Cross-reviewer overlap

| Problem | Reviewers |
|---|---|
| `raise` swallowed by the untagged `Action` | C2, A1, G3 |
| Phase dependencies (save, `hello_world`, HTTP gate) | C3, A3, G6 |
| `serde-saphyr` loader options and fallback | C13, A9, G9 |
| Subprocess runner: deadline, cleanup, reader thread | C1, A8, G11 |
| Wall-time trap and clock | C6, A2, G4 |
| `query` push order | C7, A6 |
| Comparator paths and selectors | C8, A4 |
| Cucumber skips, lints in `harness = false` | C14, A10 |
| Python CLI invocation for P7 (wizard, agent class) | C4, G2 |

## Claude's own corrections found while triaging

- Revision 4 trap 27 came from the old `spec/PLAN.md` ("limit 1 allows 1.999 s") and was merged without being checked; it is now corrected.
- `GlobalModelStats` had been listed under P8 although the P5 deterministic model already charges it; revision 4 moved it, and round 2 confirmed the phase order.
