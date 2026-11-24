# Round 1 findings: adversarial review of migration-plan.md (revision 1)

- **Date:** 2026-11-23
- **Prompt:** `../review-prompt.md`. Antigravity's prompt also had a first line telling it to use its `rust-dev` plugin.
- **Raw outputs:** `codex.md`, `grok.md`, `agy.md` in this directory.
- **Runs:**
  - Codex: `codex exec -s read-only`; exit 0, about 5 min.
  - Grok: `grok --permission-mode plan -p`; exit 0, about 17 min.
  - Antigravity: `agy --mode plan -p`. The first attempt produced no output, because a tool needed a permission that headless mode cannot prompt for. The retry succeeded after about 6 min. No permission bypass was used.
- **Result:** every finding below is reflected in revision 2 of `../../migration-plan.md`, unless its disposition says otherwise. "Verified" means Claude checked the claim against the code or the tool, not only the reviewer's citation.

Dispositions: **A** = accepted; **P** = partly accepted (the fix differs, or a sub-claim was refuted); **R** = refuted.

## Codex

| # | Sev. | Finding | Disp. | Resolution in revision 2 |
|---|---|---|---|---|
| C1 | High | Polling `try_wait` reaps the shell, then the reader join can block forever when a same-group descendant holds the pipe. Python's deadline covers the drain too | A | §3.7 step 4: one deadline over drain and exit, no reaping before the kill decision, bounded grace drain. New tests in §3.15 |
| C2 | High | `/bin/sh` conflicts with "bash only"; process group ≠ new session | P | Kept `/bin/sh` for parity (Python's `shell=True`) and recorded it as D11 and R11. "Bash only" in the task means shell-only actions. The session vs. group difference was accepted: §2.4 and §3.7 (stdin `/dev/null`) |
| C3 | High | P6 depends on the P7 CLI; a Python `FormatError` in `outputs` breaks `save()`; fatal text compared; format-error tests use tool-call fakes | A | Verified (`PydanticSerializationError`). §2.2 JSON `raise` with a Python subclass adapter, §3.15 library-level P6, fatal `content`/`exception_str` dropped, ported fakes in §3.8, error → success → error test |
| C4 | High | `model_kwargs.drop_params` forwarded to the provider; collisions with `model`/`messages`/`tools`/`stream` undefined; no wire-level test | A | §3.8 settings split (transport / stripped / reserved / body); fake HTTP server test in §3.15 |
| C5 | High | Signal exits (`code()` is `None`), exact error fields, and the timeout text "uses `repr`" | P | Signal mapping and error fields accepted (§3.7 steps 5 and 7, tests). The `repr` claim is refuted: `TimeoutExpired.__str__` uses `%s` (verified in CPython) |
| C6 | Medium | `extra` omission, missing actions/command defaults, nullable content, empty `Interrupt` | A | §3.2 (`Option` extra, `WireMessage`, `Action` defaults), §3.5 (`Interrupt { head, tail }`) |
| C7 | Medium | Overlapping borrows in `execute_actions`; template ownership; `?` does not chain two `From` hops; path-bearing I/O errors | A | §3.4 step sequence and compiled-template ownership; §3.5 conversions and `map_err` for `Io` |
| C8 | Medium | Config boundary: `run.task`, explicit `-c` replaces the default, `-o` default always overlaid, suffix replacement, PyYAML 1.1 scalars, pydantic coercion | A | Traps 13-16, §3.10 YAML subset and typed-dump test, §3.12 task order, §2.4 divergences |
| C9 | Medium | The two-rate formula is not LiteLLM cost parity; lookup rules unspecified; `ignore_errors` fallback; global call condition | A | §3.8 step 4 states the supported subset, lookup order, missing-usage rule and `add(0.0)`; §2.4 divergence; boundary tests |
| C10 | Medium | A fixed `.tmp` path is not atomic between writers; "save after every step" excludes the startup render | A | §3.11 `NamedTempFile::new_in` + `persist`; §1.3 step 2 and the §3.5 state diagram |
| C11 | Medium | `lib.rs` lint attributes miss the binary and examples; optional dev-dependency; lock and MSRV; `.env` through `set_var` | A | §3.5 `[lints]` table; D9 regular dev-dependency; `Cargo.lock` committed, `rust-version = "1.99.1"`, advisory check; `HostEnv` overlay without `set_var` |

## Grok

| # | Sev. | Finding | Disp. | Resolution in revision 2 |
|---|---|---|---|---|
| G1 | High | `re.findall` returns the capture group; a `find`-style port would run the fence as the command | A | §1.3 step 6, §3.8 `findall` rule, P4 asserts the command text |
| G2 | High | `extra` omitted on system/user messages; `flatten` collisions; the request forwards every key except `extra`; missing cost/actions defaults | A | §3.2, §3.4 `ApiMessage`, §1.3 step 5 |
| G3 | High | Signal exits, CPython OS error strings, setsid vs process group | A | §3.7 steps 1, 3, 5, 7; §2.4 |
| G4 | High | The `Agent` trait cannot pass `run` kwargs or `save` extra dicts (`test_save.py:28`) | A | §3.3 `run(task, extra_vars)`, `save(path, extra)`; §3.11 |
| G5 | High | The format-error tests use tool-call fakes; JSON `raise` cannot feed stock Python; `max_consecutive_format_errors: 0` disables | A | Trap 11, §1.5, §3.8 ported fakes, §2.2 Python adapter |
| G6 | High | `drop_params`, `api_base`/`api_key`/`timeout` are client arguments; `base_url` undefined | A | §3.8 settings split with the default `base_url` |
| G7 | Medium | The config-only default for `template_vars` does not fit the environment (uname, env) | P | Accepted: no default on `Environment` (§3.3). Instead of "live per render", vars come from the injected `HostEnv` snapshot. The process env is never mutated, so the snapshot equals the live view |
| G8 | Medium | Tenacity wait uses `attempt_number - 1` (4, 4, 4, 8, …); 10 tries; `KeyboardInterrupt` is in the abort list | A | Verified in tenacity `wait.py`. §1.5, §3.8 step 3, retry tests |
| G9 | Medium | The global limit is a `RuntimeError` fatal path with strict `<` | A | §1.6, §3.5 state diagram, D6, boundary tests |
| G10 | Medium | The cost formula is not `completion_cost`; `ignore_errors` still calls `add(0.0)`; error text substrings | A | §3.8 step 4, §3.5 `Display` phrases, §2.4 |
| G11 | Medium | Config lookup has five candidates; empty YAML loads as `None` | P | Lookup order accepted in full (§3.10). Benchmark and `extra/` configs are not embedded: they are out of scope (§2.4) |
| G12 | Medium | `directories::ProjectDirs` does not match the `platformdirs` path on macOS | A | Verified: the Python path is `~/Library/Application Support/mini-swe-agent`. §3.12 uses `dirs::config_dir()` |
| G13 | Medium | `clippy.toml` "cannot encode unwrap only in tests"; `lib.rs` attributes miss other targets; `Mutex`/`join` results | P | Refuted in part: `allow-unwrap-in-tests` and its siblings are real clippy settings, used by `embeddings-fundamentals/clippy.toml`. The other points were accepted: `[lints]` table, poison recovery, a panicked reader maps to `-1` |
| G14 | Medium | `test_local.py` has 18 functions / 20 cases, not 19, and no CRLF or marker tests; no Linux run | A | Verified (18). §3.15 and P3 corrected, new tests listed, Linux container gate |
| G15 | Medium | P6 needs the P7 binary | A | §3.15 item 4, P6 at library level |
| G16 | Medium | `serde_json::Value` treats `2` ≠ `2.0`, but `json.loads` treats them as equal | A | §3.15 comparator rule and a self-test |
| G17 | Low | The auth hint mentions the out-of-scope `mini-extra` | A | §2.4 divergence; the ported test assertion is replaced |
| G18 | Low | An empty `FormatError` makes `messages[0]` raise `IndexError` | A | §1.6 row; §3.5 makes an empty interrupt unrepresentable |

## Antigravity

| # | Sev. | Finding | Disp. | Resolution in revision 2 |
|---|---|---|---|---|
| A1 | High | `E0502`: `&Message` from `self.messages.last()` passed into a `&mut self` method | A | §3.4 step sequence |
| A2 | High | Infallible default `template_vars`/`serialize` conflict with no-unwrap; `type_name`; environment vars; unspecified error type | A | §3.3: `Result` return types, `type_name()`, no environment default, error type specified |
| A3 | High | `drop_params: true` gives HTTP 400 on OpenAI | A | §3.8 settings split; fake HTTP server test |
| A4 | High | `raise`/`/sleep`/`/warning` live inside `extra.actions`, not as top-level outputs; `info.config.model.outputs` parity | A | Verified (`test_models.py:75-87`). §3.2 `Action` enum, §3.8, §3.15 comparator |
| A5 | High | `"extra": {}` would break parity; `content: null`; `flatten` duplicate keys | A | §3.2 |
| A6 | High | `-signal` return codes; `SIGTTIN` and stdin under `process_group(0)`; `Pid` conversion | A | §3.7 steps 1 and 5, §3.4 `try_from` rule |
| A7 | High | `set_var` is `unsafe` in edition 2024; env-patching tests race | A | §2.2 `HostEnv` injection; tests build their own |
| A8 | High | No built-in price table, so P8 fails by default; prefix stripping unspecified | A | Took the reviewer's second option: documented in §2.3, §3.8 step 4 and P8, with prefix-stripping lookup |
| A9 | Medium | The `Agent` trait drops `run` kwargs and `save` extras; saving without a path builds the tree every step | A | §3.3, §3.5 (save skipped when no path) |
| A10 | Medium | The P7 gate omits `-m` (the name resolves before the class); `run.task` is ignored | A | P7 gate includes `-m deterministic`; §3.12 task order |
| A11 | Medium | `lib.rs` attributes miss `main.rs`/`examples`; `Mutex::lock` poisoning | A | §3.5 `[lints]` table; §3.4 poison recovery |
| A12 | Medium | `create_dir_all("")` fails for a bare filename; fixed `.tmp` collisions | P | Collisions accepted (`NamedTempFile`). Refuted for the empty path: std's `create_dir_all` returns `Ok` for an empty path. §3.11 says so, and a bare-filename save is covered by the save tests |
| A13 | Medium | `ureq` 3 hides 4xx bodies by default; `2^attempt` overflow | A | Verified that `http_status_as_error` is configurable in ureq 3.5. §3.6, §3.8 `checked_shl` |
| A14 | Low | Test counts; unverifiable Linux gate; `3.`/`0.` floats and empty YAML | A | §3.15, P3, the §3.15 Linux container, §3.10 YAML subset tests |

## Cross-reviewer overlap

The same problem was found independently by:

| Problem | Reviewers |
|---|---|
| `drop_params` forwarded to the provider | C4, G6, A3 |
| `extra` key presence and nullable `content` | C6, G2, A5 |
| Signal return codes; session vs. process group | C5, G3, A6 |
| Borrow conflict in the step | C7, A1 |
| `Agent` trait missing kwargs and extras | G4, A9 |
| Lints do not cover all targets | C11, G13, A11 |
| Fixed `.tmp` path | C10, A12 |
| `test_local.py` counts | G14, A14 |
| P6 depends on P7 | C3, G15 |
| `set_var` in edition 2024 | C11, A7 |
| Cost subset | C9, G10, A8 |
