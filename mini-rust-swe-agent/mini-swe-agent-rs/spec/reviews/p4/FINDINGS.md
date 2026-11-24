# P4 findings: adversarial review of text actions and observations

- **Date:** 2026-11-24
- **Code reviewed:** commit `8b9c91c` (branch `mini-swe-agent-rs/p4-text-actions`, after the P0/P1 review fixes), in
  a detached worktree.
- **Prompt:** `review-prompt.md` in this directory. Every reviewer also got the Rust best-practices line; Antigravity
  also got the `rust-dev` plugin line and the no-shell-commands line; Grok got a `--rules` note for headless runs.
- **Raw outputs:** `codex.md`, `grok.md`, `agy.md`, copied byte for byte (checked with `cmp`).
- **Runs:** read-only, in parallel, no permission bypass, no retry.

  | Reviewer | Command | Exit | Time |
  |---|---|---|---|
  | Codex | `codex exec -s read-only` | 0 | 167 s |
  | Antigravity | `agy --mode plan -p` | 0 | 286 s |
  | Grok | `grok --permission-mode plan --output-format plain --rules … -p` | 0 | 904 s |

- **Fixes:** test-first. The new goldens and tests failed or did not compile before the fix.

Dispositions: **A** = accepted; **P** = partly accepted; **R** = refuted.

## Codex (6 findings: 3 medium, 3 low)

| # | Sev. | Finding | Disp. | Resolution |
|---|---|---|---|---|
| C1 | Medium | The default fence regex rejects `\x1c`-`\x1f`, which Python's `\s` accepts | A | `ActionRegex` rewrites `\s` (and `\S`) to Python's whitespace set and `\Z` to `\z`. New goldens: `python_space_on_fence_line`, `python_space_before_command`, `custom:backslash_s_in_class`, `custom:backslash_upper_s`, `custom:backslash_upper_z` |
| C2 | Medium | Custom regexes that compile still differ: `$`, `\w` on combining marks, empty matches | P | Refused at load with `ActionRegexError::Unsupported`: `$` outside a class, and any pattern whose minimum length is zero (checked with `regex-syntax`). `\w`/`\d`/`\b` differences are documented on `ActionRegex` and in the README (R6); refusing every `\w` would break ordinary patterns |
| C3 | Medium | Format-error kwargs can replace `actions` and `error` | A | `TemplateError::DuplicateKey` (fatal) when a kwarg repeats either name and the template would render; one valid action still returns `Ok`, as in Python |
| C4 | Low | One timestamp for every output in a batch | A | `observations` takes `&dyn Clock` and reads it once per output, after the render; goldens patch `time.time` with increasing values and the test uses a stepping clock |
| C5 | Low | The observation golden cannot catch key reordering | A | Parse and observation goldens are compared as JSON text (key order included); `extra_override` output replaces `returncode`, which keeps its position |
| C6 | Low | No P4 goldens for `default.yaml`'s format-error template | A | Both shipped YAML templates, with `finish_reason` absent, `null`, `stop`, `length`, `tool_calls` |

## Grok (7 findings: 3 medium, 4 low)

| # | Sev. | Finding | Disp. | Resolution |
|---|---|---|---|---|
| G1 | Medium | Default regex `\s` | A | Same as C1 |
| G2 | Medium | Custom regex differences (anchors, word characters, empty matches) | P | Same as C2 |
| G3 | Medium | Format-error kwargs overwrite `actions` and `error` | A | Same as C3 |
| G4 | Low | Key order, extra overrides and `template_vars` not proven | A | Same as C5, plus a `vars` observation template that reads `task` and `n_model_calls` |
| G5 | Low | Goldens skip `default.yaml`'s format-error template and `finish_reason: null` | A | Same as C6 |
| G6 | Low | One timestamp per batch; a non-finite time becomes `null` | A | Same as C4; a non-finite reading is `AgentError::Internal` (`non_finite_clock_readings_are_errors`) |
| G7 | Low | The template variables are cloned for every observation | A | Cloned once per call; only `output` is replaced between renders; empty `outputs` returns at once |

## Antigravity (7 findings: 1 high, 4 medium, 2 low)

| # | Sev. | Finding | Disp. | Resolution |
|---|---|---|---|---|
| A1 | High | `parse` overwrites `actions` and `error` from `template_kwargs` | A | Same as C3. Severity: the shipped text model passes only `finish_reason`, so only custom callers reach it |
| A2 | Medium | A single `timestamp: f64`; non-finite values become `null` | A | Same as G6 |
| A3 | Medium | Golden comparisons ignore key order | A | Same as C5 |
| A4 | Medium | No `default.yaml` format-error goldens; no `finish_reason: null` | A | Same as C6 |
| A5 | Medium | No test that `template_vars` reach the template or that `output.extra` overrides a key | A | Same as G4 |
| A6 | Low | `N + 1` clones of `template_vars` | A | Same as G7 |
| A7 | Low | `(?s)` by string formatting; engine differences undocumented | A | `RegexBuilder::dot_matches_new_line(true)`; the differences are documented (C2) |

## Cross-reviewer overlap

| Problem | Reviewers |
|---|---|
| Format-error kwargs replace `actions`/`error` | C3, G3, A1 |
| Default regex `\s` misses `\x1c`-`\x1f` | C1, G1 |
| Custom-regex engine differences | C2, G2, A7 |
| One timestamp per batch | C4, G6, A2 |
| Key order and overrides untested | C5, G4, A3, A5 |
| `default.yaml` format-error goldens | C6, G5, A4 |
| Clones of the template variables | G7, A6 |
