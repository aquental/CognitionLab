# P2 findings: adversarial review of the template engine

- **Date:** 2026-11-24
- **Code reviewed:** commit `8f1c5e5` (branch `mini-swe-agent-rs/p2-templates`, after the P0/P1 review fixes), in a
  detached worktree.
- **Prompt:** `review-prompt.md` in this directory. Every reviewer also got the Rust best-practices line; Antigravity
  also got the `rust-dev` plugin line and the no-shell-commands line; Grok got a `--rules` note for headless runs.
- **Raw outputs:** `codex.md`, `grok.md`, `agy.md`, copied byte for byte (checked with `cmp`).
- **Runs:** read-only, in parallel, no permission bypass, no retry.

  | Reviewer | Command | Exit | Time |
  |---|---|---|---|
  | Codex | `codex exec -s read-only` | 0 | 201 s |
  | Antigravity | `agy --mode plan -p` | 0 | 325 s |
  | Grok | `grok --permission-mode plan --output-format plain --rules … -p` | 0 | 1053 s |

- **Fixes:** test-first (the new tests and goldens failed or did not compile before the fix).

Dispositions: **A** = accepted; **P** = partly accepted; **R** = refuted.

## Codex (5 findings: 2 medium, 3 low)

| # | Sev. | Finding | Disp. | Resolution |
|---|---|---|---|---|
| C1 | Medium | The Unicode fixture never reaches `[:5000]`/`[-5000:]` (8,001 code points) | A | `long_multibyte` is now 12,001 code points with different multibyte text at each end; the 8,001-code-point string stays as `under_limit_multibyte` (over 10,000 bytes, so `length` must count code points). The generator asserts both sizes |
| C2 | Medium | An unknown filter passes `Templates::add` and fails at render | R | Python builds `Template(...)` at render time (`default.py:66-67`, `actions_text.py:30-31`, `:53-54`), so an unknown filter fails at the same moment there. Compiling at load is a Rust addition for syntax errors only; the README states it |
| C3 | Low | Regenerated goldens could bless a changed Rust prompt; the count floor allows dropping a template | P | `tests/yaml.rs` already fails if `config/` differs from the reference by one byte, so a Rust-only prompt edit cannot pass. The coverage gap is fixed: the test requires goldens for all 8 shipped templates |
| C4 | Low | `join` (and other string conversions inside expressions) bypass the Python formatter | P | Documented in the README as a custom-template divergence (`~`, `| string`, `| join`); no shipped template converts a bool, `None` or float inside an expression |
| C5 | Low | No golden for the agent-stats context (`n_model_calls = 2`, `model_cost = 1.0`) | A | `probe:agent_stats` → `Calls: 2, Cost: 1.0` |

## Antigravity (8 findings: 1 high, 3 medium, 4 low)

| # | Sev. | Finding | Disp. | Resolution |
|---|---|---|---|---|
| A1 | High | `long_multibyte` is 8,001 code points | A | Same as C1 |
| A2 | Medium | `TemplateError` not wired into `AgentError`/`StepError`; no `DuplicateKey` | R | Delivered in P4 (`AgentError::Template`, `From<TemplateError> for StepError`, `TemplateError::DuplicateKey`), where the observation renderer that needs them lands; P2 is the engine only |
| A3 | Medium | `render` accepts a non-object context | A | `TemplateError::ContextNotObject`; tests for `null`, an array, a string and a number |
| A4 | Medium | No `finish_reason: None` golden; `long_ascii` padding is negative; config coverage not enforced | A | `none` format-error context; padding fixed to a 20,000-character string with an assertion; the 8-template coverage check (C3) |
| A5 | Low | The HTML test cannot catch a default auto-escape callback | A | Templates named `obs.html`, `obs.xml`, `obs.htm` render unescaped |
| A6 | Low | `~` and `| string` bypass the formatter; minijinja lacks `preserve_order` | P | `preserve_order` enabled; the formatter limitation is documented (C4) |
| A7 | Low | `python_float_repr` is public; boundary floats untested | A | Private; `0.0`, `1e-4`, `9.9999e-5`, `-1e-5` tested (unit test and the `probe:float_boundaries` golden) |
| A8 | Low | The generator's docstring omits the isolation variables | A | Docstring uses `MSWEA_SILENT_STARTUP=1` and a temporary `MSWEA_GLOBAL_CONFIG_DIR` |

## Grok (2 findings: 2 low)

| # | Sev. | Finding | Disp. | Resolution |
|---|---|---|---|---|
| G1 | Low | `long_multibyte` is 8,001 code points; the `long_ascii` padding is negative | A | Same as C1 and A4 |
| G2 | Low | `python_float_repr` should not be public | A | Same as A7 |

## Cross-reviewer overlap

| Problem | Reviewers |
|---|---|
| Multibyte slicing never tested | C1, A1, G1 |
| `python_float_repr` public | A7, G2 |
| Formatter bypassed inside expressions | C4, A6 |
| Golden coverage of the shipped templates | C3, A4 |
