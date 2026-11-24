# P1 findings: adversarial review of the core types and traits

- **Date:** 2026-11-24
- **Code reviewed:** commit `cacab19` (branch `mini-swe-agent-rs/p1-core-types`), in a detached worktree.
- **Prompt:** `review-prompt.md` in this directory. Every reviewer also got the Rust best-practices line;
  Antigravity also got the `rust-dev` plugin line and the no-shell-commands line.
- **Raw outputs:** `codex.md`, `grok.md`, `agy.md` in this directory, copied byte for byte (checked with `cmp`).
- **Runs:** each one ran read-only and in parallel, with no permission bypass and no retry.

  | Reviewer | Command | Exit | Time |
  |---|---|---|---|
  | Codex | `codex exec -s read-only` | 0 | 176 s |
  | Antigravity | `agy --mode plan -p` | 0 | 290 s |
  | Grok | `grok --permission-mode plan --output-format plain -p` | 0 | 1381 s |

- **Fixes:** commit `1f9c7c7`, test-first (the new tests did not compile or failed on `cacab19`). The later
  phases were adapted in their own commits (P3: `child_vars`, `Interrupt::submitted`; P5: in-order actions,
  fallible cost, `format_message` with `extra`, limit constructors).

Dispositions: **A** = accepted; **P** = partly accepted; **R** = refuted.

## Codex (8 findings: 5 medium, 3 low)

| # | Sev. | Finding | Disp. | Resolution |
|---|---|---|---|---|
| C1 | Medium | Eager action decoding lets a malformed later entry hide an earlier `raise`, retry or submission | A | `Message::action_iter` decodes one entry at a time; the deterministic model and `execute_actions` consume it in order (P5 tests: scripted raise and `Submitted` win over a later malformed entry) |
| C2 | Medium | `Environment::execute` gets only the decoded `Action`; a `raise` reaching the environment becomes an empty command | R | The message keeps the original JSON (`extra.actions` is never rewritten), and the trajectory is what parity compares. Python's environment reads only `action.get("command", "")` (`local.py:26`): a `raise` dict that reaches it (from a model other than the deterministic one) runs an empty command. Returning `""` is that behavior |
| C3 | Medium | `extra: null` is erased; a null or string cost becomes 0 | A | `"extra": null` is refused at decoding; `Message::cost` returns `InvalidCost` for a present non-number (missing stays 0.0). P5: fatal before the append in `query`; in the format-error arm it propagates without an exit message, as Python raises inside `except FormatError` |
| C4 | Medium | `format_message` drops `extra` | A | `format_message(role, content, extra: Option<Map>)`, as plan §3.3 |
| C5 | Medium | Injected fatal errors use the fixture's class name as exit status | R | Python writes `type(e).__name__`, so the scripted class name is the exact parity value and needs no mapping. A fixture that names a Rust status is a fixture bug, not an ambiguity at runtime |
| C6 | Low | A fatal `raise` accepts a non-string `message` | A | Non-empty string `kind` required; `message` absent or a string. Tests for `42`, `""`, a numeric kind, `null`, and a malformed raise next to a command |
| C7 | Low | The merge order test cannot catch sorting; the nested-absent case is missing | A | Unsorted keys at two levels; nested-absent and falsy-override cases |
| C8 | Low | `ManualClock::advance` can lose concurrent increments | A | One atomic `update`; an 8-thread test |

## Antigravity (7 findings: 1 high, 3 medium, 3 low)

| # | Sev. | Finding | Disp. | Resolution |
|---|---|---|---|---|
| A1 | High | `format_message` omits `extra` | A | Same as C4 |
| A2 | Medium | `Action::command` turns `Raise` into `""` | R | Same as C2 |
| A3 | Medium | Unneeded `where Self: Sized` on `type_name` and `serialize` | A | Bounds removed on both traits |
| A4 | Medium | `RaiseSpec` coerces a non-string `message`, accepts an empty `kind` | A | Same as C6 |
| A5 | Low | `advance` not atomic | A | Same as C8 |
| A6 | Low | `merge_into` looks up twice and clones the key on overwrite | A | Overwrites in place |
| A7 | Low | `child_vars` yields `&OsString`; no per-kind `Interrupt` constructors; missing tests (`to_value` round trip, `head_mut`, falsy overrides) | A | `(&OsStr, &OsStr)`; `Interrupt::submitted`, `limits_exceeded`, `time_exceeded` (used by P3 and P5); the three tests |

## Grok (5 findings: 1 medium, 4 low)

| # | Sev. | Finding | Disp. | Resolution |
|---|---|---|---|---|
| G1 | Medium | `format_message` cannot carry `extra` | A | Same as C4 |
| G2 | Low | A present non-numeric cost bills as 0 | A | Same as C3 |
| G3 | Low | `Action::command` reports `""` for `Raise` | R | Same as C2 |
| G4 | Low | `advance` not atomic | A | Same as C8 |
| G5 | Low | The order test does not lock insertion order | A | Same as C7 |

## Cross-reviewer overlap

| Problem | Reviewers |
|---|---|
| `format_message` without `extra` | C4, A1, G1 |
| `command()` of a `Raise` (refuted) | C2, A2, G3 |
| `ManualClock::advance` | C8, A5, G4 |
| Invalid cost | C3, G2 |
| Merge order test | C7, G5 |
| Raise payload validation | C6, A4 |
