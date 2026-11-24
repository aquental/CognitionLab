I'll review P4 against the plan's done criteria and the Python reference, staying inside this directory and only reading.

P4 matches the phase-table cases for ordinary fences: the command is the inside of the capture, zero and two blocks become a `FormatError`, `finish_reason=length` renders the shipped cut-off text, and observation `extra` carries Python's keys. Error handling is idiomatic (`thiserror`, `Result`, format-error render failures stay fatal, the `output` collision is lazy). Parity is still open for Python's `\s` on the default pattern, for several user regexes the `regex` crate accepts, and for format-error keyword collisions.

1. **Medium** — The default fence regex rejects whitespace Python accepts.  
   **Where:** `mini-swe-agent-rs/src/model/actions_text.rs:15` and `:45`.  
   **Problem:** `DEFAULT_ACTION_REGEX` uses `\s`, compiled by the Rust `regex` crate. That `\s` is Unicode `White_Space` and does not include U+001C–U+001F. Python `re` `\s` does. A fence Python accepts becomes a zero-action `FormatError`. `pystr::strip` already treats those characters as spaces, but stripping runs only after a match.  
   **Evidence:** `regex` 1.13 documents `\s` as `\p{White_Space}` (`regex-1.13.1/src/lib.rs` Perl classes). Python 3 `re.findall` with `re.DOTALL` on `` ```mswea_bash_command\x1c\nls\n``` `` returns `['ls']` (span covering the whole fence). The same `\s*\n` pattern matches `"\x1c\n"`. `pystr::is_space` (`pystr.rs:5-7`) adds `\x1c`..=`\x1f` only for `strip` / `lstrip`.  
   **Fix:** Give the shipped pattern a class with Python's `\s` set, including `\x1c-\x1f`, and add a golden whose opening fence line contains U+001C. User patterns that write `\s` still need the engine gap in finding 2.

2. **Medium** — Custom regexes that compile still disagree with `re.findall` on anchors, word characters, and empty matches.  
   **Where:** `mini-swe-agent-rs/src/model/actions_text.rs:44-69`.  
   **Problem:** `ActionRegex` rejects lookaround and more than one capture group (risk R6). Patterns that compile can still return a different command, or a different action count, so a reply Python rejects is accepted here and the reverse.  
   **Evidence:** Python 3 `re.findall(..., re.DOTALL)`:
   - `(x)$` on `"x\n"` returns `['x']` because `$` matches before a final newline. Rust `$` without `(?m)` is the end of the haystack only (`regex-1.13.1/src/lib.rs` empty matches). There is no `\Z`. Rust finds nothing and raises `FormatError`.
   - `(\w+)` on `"a\u0301"` returns `['a']`. Rust `\w` is `\p{Alphabetic}+\p{M}+\p{Nd}+\p{Pc}+\p{Join_Control}`, so the combining mark is included. Python `\w` also matches `²` (U+00B2); Rust `\w` does not.
   - `(?:^|x)` on `"x"` returns `['', 'x']` with spans `(0, 0)` and `(0, 1)`, so `parse_regex_actions` raises `FormatError` (`actions_text.py:24-26`). After a zero-width match, `regex-automata` advances one byte (`util/iter.rs` `handle_overlapping_empty_match`) and does not report the `x` at the same index. Rust yields one empty match and accepts the command `""`.  
   **Fix:** Add those three cases to `parity/action_goldens.py`. Document them on `ActionRegex` as engine limits next to R6. Rejecting `$`, `\w`, `\d`, `\s`, or any pattern that can match empty, with a specific error, is the alternative if those forms must match Python.

3. **Medium** — Format-error kwargs overwrite `actions` and `error`.  
   **Where:** `mini-swe-agent-rs/src/model/actions_text.rs:128-133`.  
   **Problem:** On the failure path, `parse` inserts `actions` and `error`, then inserts every `template_kwargs` entry. A kwargs key of the same name replaces the value, and the template still renders as a `FormatError`. Python raises `TypeError` while building the `render` call, before any `FormatError` exists (trap 26: a render failure is fatal and is not a format error).  
   **Evidence:** `actions_text.py:30-32` calls `Template(...).render(actions=actions, error=error_msg, **(template_kwargs or {}))`. A duplicate keyword raises `TypeError` at that call. `observations` already returns `TemplateError::DuplicateKey("output")` when the agent vars contain `output` (`actions_text.rs:169-171`). With zero matches, template `{{ error }}`, and kwargs `{"error": "override"}`, Rust's message content is `override`. A single successful action never renders this template, in either implementation. The text model only passes `finish_reason` (`litellm_textbased_model.py:36`), so the shipped call does not hit this.  
   **Fix:** If `template_kwargs` contains `actions` or `error`, return `Err(TemplateError::DuplicateKey(key).into())` (`StepError::Fatal`). Test both keys on a zero-match input, and test that one valid action still returns `Ok` when those keys are present.

4. **Low** — Observation tests do not prove key order, extra overrides, or `template_vars`.  
   **Where:** `mini-swe-agent-rs/tests/actions_text.rs:66-70`, `src/model/actions_text.rs:172-184`.  
   **Problem:** The inserts follow Python: `raw_output`, `returncode`, `timestamp`, `exception_info`, then `output.extra` (`actions_text.py:59-65`). `serde_json` is built with `preserve_order` (`Cargo.toml:18`), and `Map::insert` keeps an existing key's position. `assert_eq!` on `Value` ignores order, and the timeout fixture only adds new keys (`exception_type`, `exception`). Every observation template in the golden reads `output.*` only, so replacing `base.clone()` with an empty map would still pass.  
   **Fix:** Compare key order (serialized JSON text, or `keys()` in order). Add a golden whose template reads `task` from `template_vars`, and one whose `output.extra` overwrites `returncode` while that key stays in its original position.

5. **Low** — The action goldens skip `config/default.yaml`'s format-error template.  
   **Where:** `mini-swe-agent-rs/parity/action_goldens.py:67-75`, `config/default.yaml:144-170`.  
   **Problem:** The script loads `mini_textbased.yaml` and the pydantic default. `default.yaml`'s `format_error_template` is a different string (the `COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT` lines and the `TRIPLEBACKTICKSBASH` note). The cases named `shipped:*:None` omit `finish_reason` (`action_goldens.py:49`) rather than passing JSON `null`, which is what `LitellmTextbasedModel._parse_actions` passes when the provider sends no finish reason (`litellm_textbased_model.py:36`).  
   **Evidence:** P2 already renders both YAML templates (`parity/template_goldens.py:68-77`) with `zero`, `two`, `stop`, `length`, and `tool_calls`. For this template, absent and `null` take the same branch: `finish_reason is defined` is false when the key is missing; JSON `null` is minijinja `None`, which is defined, and `None in ["length", "tool_calls"]` is false (`minijinja` `is_defined` and `value::ops::contains`). The current text matches. A later edit that distinguishes none from undefined would not be caught here.  
   **Fix:** Emit zero-block, two-block, and `length` cases for `default.yaml`, plus one case with `"finish_reason": null`.

6. **Low** — One timestamp is written for every output in the batch, and a non-finite value becomes JSON `null`.  
   **Where:** `mini-swe-agent-rs/src/model/actions_text.rs:152-180`.  
   **Problem:** `observations` takes one `f64` and stores it on every message. Python calls `time.time()` after each template render (`actions_text.py:52-63`). Plan §3.4 passes the `Clock` into the observation formatter. `json!(timestamp)` goes through `serde_json::Value::from(f64)`, which is `null` for NaN and infinity (`serde_json-1.0.154/src/value/from.rs`). The parity comparator ignores timestamp values, so this does not fail a trajectory diff. `SystemClock` does not produce a non-finite reading.  
   **Fix:** Take `&dyn Clock`, sample it after each successful render, and return an error from `Number::from_f64` when the value is non-finite. A test clock with two distinct readings should show two timestamps.

7. **Low** — Each observation clones the full template-variable map.  
   **Where:** `mini-swe-agent-rs/src/model/actions_text.rs:158-172`.  
   **Problem:** `base` clones the object, then every output clones `base` again. Those variables are the agent merge, including the environment block. Plan §3.4 says not to clone large `Value` trees in the loop. One action per step is the common case, so this is two clones per observation.  
   **Fix:** Return immediately when `outputs` is empty, reject a top-level `output` key once, clone the map once, and replace the `output` entry on each iteration.
