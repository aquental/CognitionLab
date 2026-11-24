# P0 findings: adversarial review of the scaffold and the YAML loader

- **Date:** 2026-11-24
- **Code reviewed:** commit `7386f94` (branch `mini-swe-agent-rs/p0-scaffold`).
- **Prompt:** `review-prompt.md` in this directory.
  - Every reviewer also got a first line asking it to apply Rust development best practices.
  - Antigravity's prompt also told it to use its `rust-dev` plugin, and not to run shell commands.
- **Raw outputs:** `codex.md`, `grok.md`, `agy.md` in this directory, copied byte for byte from each CLI.
- **Runs:** each one ran read-only in its own subagent, with no permission bypass.

  | Reviewer | Command | Exit | Time | Notes |
  |---|---|---|---|---|
  | Codex | `codex exec -s read-only` | 0 | ~4 min | |
  | Antigravity | `agy --mode plan -p` | 0 | ~5 min | |
  | Grok | `grok --permission-mode plan --output-format plain -p` | 0 | ~21 min | First run stopped after a refused shell command (headless). The retry added only a `--rules` note asking it to prefer read-only tools. |

- **Fixes:** commits `630bf96` (review fixes) and the commit that adds this file. All fixes are test-first: the new
  goldens and tests failed on `7386f94`.

Dispositions: **A** = accepted; **P** = partly accepted (the fix differs, or a sub-claim was wrong); **R** = refuted.

## Codex (7 findings: 1 medium, 6 low)

| # | Sev. | Finding | Disp. | Resolution |
|---|---|---|---|---|
| C1 | Medium | The EOF heuristic removes real newlines (trailing comment, alias with comment) and misses trailing spaces | A | `src/yaml.rs` no longer compares text. It parses again with a sentinel appended to the last line; the one string that ends with sentinel + `\n` gives the path where the invented `\n` is removed. New PyYAML goldens: `eof_comment`, `eof_alias_comment`, `eof_quoted_comment`, `eof_trailing_spaces`, `eof_duplicate`, plus `eof_folded`, `eof_keep_folded`, `eof_sequence` |
| C2 | Low | The blank-document shortcut turns a lone U+00A0 into `null` | A | Shortcut removed; the parser classifies empty and comment-only input. `nbsp` golden |
| C3 | Low | `eprintln!` in `main` can panic on a closed stderr (exit 101, not 2) | A | `writeln!` to stderr, result ignored |
| C4 | Low | Plain scalar keys untested; alias-budget test accepts any error; parser error flattened to a string | P | Plain keys (`42`, `true`, `1.5`) load as `"42"`, `"true"`, `"1.5"`. That is exactly PyYAML + `json.dumps`, so the loader keeps it and the test pins it; the README states the rule. Mapping and sequence keys stay errors. The budget test asserts `budget` in the message; `YamlError::Parse` keeps the parser error as its `source()` |
| C5 | Low | The crate-level `#![allow]` in `tests/yaml.rs` is broader than needed; the README overstates the test exemptions | A | Allowance moved to the one helper; README lists the four exempt lints and says `todo!`, `unimplemented!`, `dbg!` stay denied. Same rule applied to the later phases' test files |
| C6 | Low | `serde_json` and `thiserror` are not `=` pins | A | `=1.0.154`, `=2.0.21` (and later phases pin every dependency the same way) |
| C7 | Low | No explicit `cargo build` gate in the README | A | `cargo build --locked` added to both gates and to the evidence table |

## Antigravity (7 findings: 1 high, 3 medium, 3 low)

| # | Sev. | Finding | Disp. | Resolution |
|---|---|---|---|---|
| A1 | High | The EOF heuristic: five false positives and negatives (quoted + comment, block + comment, trailing spaces, folded, duplicate key) | A | Same as C1; each of the five has a golden |
| A2 | Medium | Plain scalar keys silently coerced | P | Same as C4 |
| A3 | Medium | `YamlError::Parse(String)` loses the source | A | Boxed `serde_saphyr::Error` as `#[source]` |
| A4 | Medium | Blank shortcut bypasses budgets and accepts `\x0c`; no tests for multi-document streams or deep nesting | P | Shortcut removed; tests for `\x0c`, `\x0b` (PyYAML: `ReaderError`), multi-document streams and 100 000-deep nesting, all errors. An explicit byte cap is refused: Python has none, configs are local files, and the parser budget already bounds the work |
| A5 | Low | Pins | A | Same as C6 |
| A6 | Low | Hardcoded fixture list; no stem-collision check | A | The test globs `parity/yaml_cases/` (at least 20 cases); `yaml_dump.py` refuses duplicate stems |
| A7 | Low | Docs mention only `default.yaml`; README omits `dbg!` and doc tests | A | Both configs named; `dbg!` listed; `cargo test --locked` noted as running doc tests |

## Grok (5 findings: 2 high, 1 medium, 2 low)

| # | Sev. | Finding | Disp. | Resolution |
|---|---|---|---|---|
| G1 | High | The EOF post-pass guesses | P | Problem accepted (C1). The proposed fix, the option `block_scalar_eof_without_line_feed`, does not exist in `serde-saphyr` 2.2.0 (checked in `src/de/options.rs`), so the sentinel approach is used |
| G2 | High | Plain non-string keys load as strings | P | Same as C4 |
| G3 | Medium | `str::lines` misses a comment closed by `\r`, so the document becomes `null` | A | Shortcut removed; `cr_comment` golden (`# comment\ra: 1` → `{"a": 1}`) |
| G4 | Low | Pins | A | Same as C6 |
| G5 | Low | Parse error flattened | A | Same as A3 |

## Cross-reviewer overlap

| Problem | Reviewers |
|---|---|
| EOF newline heuristic | C1, A1, G1 |
| Plain scalar keys | C4, A2, G2 |
| Blank-document shortcut | C2, A4, G3 |
| Parse error source | C4, A3, G5 |
| Exact pins | C6, A5, G4 |
