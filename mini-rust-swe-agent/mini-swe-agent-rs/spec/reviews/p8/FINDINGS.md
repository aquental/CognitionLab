# P8 findings: adversarial review of the text-based model

- **Date:** 2026-11-24
- **Code reviewed:** commit `f816a1e` (branch `mini-swe-agent-rs/p8-textbased-model`, PR #29), in a detached worktree.
- **Prompt:** `review-prompt.md` in this directory. Every reviewer also got the Rust best-practices line. Antigravity
  also got the `rust-dev` plugin line and the no-shell-commands line. Grok got a `--rules` note for headless runs.
- **Raw outputs:** `codex.md`, `grok.md`, `agy.md`, copied byte for byte from the CLI outputs (checked with `cmp`).
  `grok.md` starts with two progress lines, which are part of the CLI output.
- **Runs:** read-only, in parallel, no permission bypass, no retry.

  | Reviewer | Command | Exit | Time |
  |---|---|---|---|
  | Codex | `codex exec -s read-only` | 0 | 163 s |
  | Antigravity | `agy --mode plan -p` | 0 | 329 s |
  | Grok | `grok --permission-mode plan --output-format plain --rules … -p` | 0 | 1109 s |

- **Fixes:** test-first.
  - New file: `tests/openai_compat.rs` (settings, endpoint, proxy, error mapping, response validation, usage).
  - New tests in `tests/textbased.rs`, `tests/fake_http.rs` and `tests/config.rs`. They failed, hung or did not
    compile before the fix.
  - The fake server now gives up after 10 s without a connection, so a failing test cannot hang the suite.
  - The raw-response differential now also compares the messages sent to the provider and the content kept in
    `extra.response`. Two sabotage checks fail it: one sends `extra` to the provider, the other drops
    `extra.response`.
- **Gates after the fixes:** all pass.
  - `cargo test --locked` on macOS with the reference.
  - The full-suite Linux gate.
  - The Rust-only Linux gate.
  - clippy `-D warnings` on all targets.
  - `cargo deny check advisories`.

Dispositions:
- **A** = accepted.
- **P** = partly accepted.
- **R** = refuted.

## Codex (11 findings: 3 high, 6 medium, 2 low)

| # | Sev. | Finding | Disp. | Resolution |
|---|---|---|---|---|
| C1 | High | Provider errors can disclose credentials and prompts | A | The error part of a provider body is cut to 2,000 characters. An echoed API key is replaced by `<redacted>`, also in transport errors. A settings error about the key never prints its value. `TextbasedModelConfig` (and so `ModelChoice`) gets a `Debug` that redacts a literal key. A 401 names the key variable. The prompt text a provider quotes in an error stays, bounded: the trajectory holds the prompt anyway. Tests: `an_echoed_key_is_redacted_from_errors`, `a_failed_cli_run_does_not_leak_the_key` (exit 1, stderr and trajectory checked), `api_key_sources_and_validation`, `config_debug_redacts_a_literal_key`, `http_errors_map_from_structured_bodies` |
| C2 | High | Model commands inherit the credential used by the transport | R | Parity: Python runs commands with `os.environ \| config.env` (`local.py:29`) and puts `os.environ` in the template variables (`local.py:59`). So the key variable is visible there too. Filtering it would change commands, templates and trajectories relative to the reference. Recorded in the README |
| C3 | High | The default request can block indefinitely | A | `DEFAULT_TIMEOUT` = 600 s (LiteLLM's default) when `model_kwargs.timeout` is unset. It is ureq's global deadline: connect, headers and body. A timeout must be a positive number. Tests: `timeout_defaults_and_validation`, and `a_stalled_response_times_out` (a peer that reads the request and never answers; fails in under 4 s with a 0.3 s timeout) |
| C4 | Medium | Reading the body can turn a known abort status into a retry | A | For a non-2xx status, the body is read lossily and capped at 64 KiB. A read failure gives an empty body, so the status alone decides 3xx/401/403/404. A 400 is classified by its `error.code`, `error.type` and `error.message` (or the top-level fields, as vLLM sends them). It falls back to the whole body only when that body is not JSON. Tests: `an_unreadable_abort_body_still_aborts_with_a_key_hint`, `http_errors_map_from_structured_bodies` (text quoted outside the error fields does not decide) |
| C5 | Medium | Any JSON value returned with HTTP 2xx is accepted as a completion | A | `ChatResponse::validated` requires: a first choice, a message object, role `assistant` or absent, and content that is text, `null` or absent. It runs inside the retry closure, so an invalid reply is retried within the same attempt budget and is never billed. This matches Python, where LiteLLM fails while parsing the reply inside the retried `_query`, before any cost. A missing role becomes `assistant` (LiteLLM's `Message` default). Tests: `completions_are_validated`, `invalid_completions_are_retried_and_never_billed`, `a_missing_role_means_assistant`, `a_server_error_is_retried_then_succeeds` |
| C6 | Medium | Endpoint construction can send the request to the wrong route | A | `api_base` is validated once, when the settings are built. It must be `http(s)://` with a host, no credentials and no fragment, and the error never echoes the URL. `Transport::endpoint` appends `/chat/completions` to the path and keeps the query. Tests: `endpoint_is_built_and_validated_once`, `the_query_string_of_the_base_url_is_kept` (request line `POST /v1/chat/completions?tenant=x`) |
| C7 | Medium | The raw-response differential misses the behavior it claims to prove | P | The projection now includes `request_messages` (the messages each side sent) and `response_content` (`extra.response.choices[0].message.content`). The fixture history has an assistant message with a provider field (`refusal`) and an `extra`, plus user messages with `extra`. Two sabotage checks fail it: `extra` sent to the provider, and `extra.response` dropped. Agent accounting of a billed format error is covered by `the_agent_charges_a_billed_format_error` (canned backend, real `DefaultAgent`: cost 0.04 over 2 calls), not by the differential. The raw-JSON versus `model_dump()` divergence stays documented |
| C8 | Medium | The real-endpoint acceptance gate is outstanding | A | `tests/real_endpoint.rs` now makes `MSWEA_RS_REAL_REGISTRY` absolute before passing it to the sandboxed CLI, and requires a positive `instance_cost` when a registry is given. After the merge, the owner provided an API key and the run passed (2026-11-24): OpenAI `gpt-4o-mini`, registry prices from LiteLLM's bundled table, 6.4 s. A separate CLI run of the same task ended `Submitted` after 3 calls at $0.00049, and the key appears in neither the trajectory, stdout nor stderr |
| C9 | Medium | Invalid model configuration can panic while building an error | A | P7 code. `invalid()` cut the JSON text with `String::truncate(80)`, which panics inside a multi-byte character. It now keeps the first 80 characters. Test: `invalid_value_messages_truncate_on_a_char_boundary` (byte 80 inside an `é`) |
| C10 | Low | `hello_world` leaves its Clap feature requirement implicit | A | The example no longer uses `#[arg(env = …)]`. `-m` is optional and falls back to `MSWEA_MODEL_NAME` from `HostEnv` |
| C11 | Low | Invalid registry rates can silently reduce billed cost | A | Each price must be finite and non-negative, else it is a cost error. Test: `registry_prices_must_be_nonnegative` |

## Grok (10 findings: 1 high, 6 medium, 3 low)

| # | Sev. | Finding | Disp. | Resolution |
|---|---|---|---|---|
| G1 | High | No request timeout unless the user sets one | A | As C3 |
| G2 | Medium | Redirects forward the chat body | A | `max_redirects(0)`: a 3xx is returned, not followed. It maps to the new `BackendError::Redirect`, which aborts (exit status `APIError`) with a hint to set the final `api_base`. Test: `redirects_are_not_followed` (a 307 to a second fake server; nothing reaches it) |
| G3 | Medium | Base URL joining breaks query strings | A | As C6 |
| G4 | Medium | The proxy comes from the process environment, not `HostEnv` | A | The settings resolve the proxy from `HostEnv`, which includes the global `.env` that Python loads into `os.environ`. They use ureq's variable order (`ALL_PROXY`, `HTTPS_PROXY`, `HTTP_PROXY`, each also in lower case) and `NO_PROXY` (`*`, the host, or a parent domain with or without a leading dot). The agent gets that proxy explicitly, so ureq never reads the process environment. `Transport`'s `Debug` hides the proxy URI. Tests: `proxy_comes_from_the_host_env`, and the fake-server test `the_proxy_comes_from_the_host_env` (the request arrives at the proxy as `CONNECT upstream.invalid:80`) |
| G5 | Medium | HTTP error mapping is skipped when the body cannot be read, and the context-window check is narrow | A | As C4. The 400 markers also cover `prompt is too long`, `input is too long` and `reduce the length of the messages`. Bodies in warnings and errors are bounded (C1). Error bodies are decoded lossily, so that the status still decides. A 2xx body that is not UTF-8 is a transport error and is retried |
| G6 | Medium | A 200 with no choice is billed as a format error | A | As C5. The reply is retried without billing rather than billed and made fatal: in Python, the failure happens inside the retried call, before the cost |
| G7 | Medium | Token counts that are JSON floats or strings are treated as missing usage | A | As pydantic's lax `int` does, a count can be a non-negative integer, an integral float or a base-10 string. `10.5`, `-1` and `"ten"` are still missing usage. Test: `usage_accepts_integral_numbers_and_digit_strings` |
| G8 | Low | `Debug` on the config prints a literal API key | A | As C1 |
| G9 | Low | Authentication errors do not name the key variable | A | A 401 message ends with "Set the `<key_env>` environment variable (for example in the global .env file) or model_kwargs.api_key." `Transport::key_env` is `api_key_env` or `OPENAI_API_KEY`. Plan §2.4 |
| G10 | Low | `examples/hello_world.rs` does not load the global `.env` | P | Accepted for `.env`. `config::global_config_dir` and `HostEnv::overlay_dotenv` moved from `main.rs` into the library, and both the CLI and the example use them. Refuted for the cost limit: Python's `hello_world.py` passes `default.yaml`'s agent section unchanged, and so does the example |

## Antigravity (12 findings: 3 high, 6 medium, 3 low)

| # | Sev. | Finding | Disp. | Resolution |
|---|---|---|---|---|
| A1 | High | No default HTTP timeout in `OpenAiCompatBackend` | A | As C3. `timeout: 0`, a negative value, a string and `1e300` are settings errors |
| A2 | High | Malformed 2xx JSON responses are billed and misclassified as `FormatError` | A | As C5 |
| A3 | High | `read_to_string()` before the status check turns non-UTF-8 or oversized 4xx responses into 10 retries | A | As C4 (a 64 KiB cap, lossy, status first) |
| A4 | Medium | `TextbasedModelConfig` and `ModelChoice` derive raw `Debug`, leaking a literal `api_key` | A | As C1. `Model::config()` still returns the unredacted config, as an in-process accessor. The agent writes the model only through `serialize()` and `template_vars()`, which redact it |
| A5 | Medium | `O(N^2)` serialization and cloning of `Message.extra` and `ChatRequest` on every query | P | `api_message` builds `role`, `content` and the provider fields directly and never serializes `extra` (raw responses, command outputs). `ChatRequest` implements `Serialize` and is written straight to bytes, without a second `Value` copy. Resending the whole history on each call is the protocol, as in Python |
| A6 | Medium | Missing API-key environment variable hint on `AuthenticationError` | A | As G9 |
| A7 | Medium | HTTP 400 error mapping misses common OpenAI and Anthropic abort errors | A | Context markers: see G5. Unsupported-parameter markers add `unknown parameter`, `unknown_parameter` and `extra inputs are not permitted`. Tests cover the Anthropic, OpenAI and vLLM bodies, plus 403 and an unsupported-parameter 400 over HTTP |
| A8 | Medium | Unvalidated `api_base` and `api_key` values cause malformed headers or 10 wasted retries | A | `api_base` as C6. An empty literal `api_key` is unset and falls back to the variable, as an empty variable already did. A key with control characters (from `model_kwargs` or the variable) is a settings error that names only the source |
| A9 | Medium | `examples/hello_world.rs` relies on clap's `env` feature without declaring it | A | As C10 |
| A10 | Low | `completion_cost` accepts negative per-token prices | A | As C11 |
| A11 | Low | `TextbasedModel::query` does not guard against a non-finite `clock.now()` | A | The timestamp goes through `Number::from_f64`; a non-finite time is `AgentError::Internal`, as in `Observations::messages`. Test: `a_non_finite_clock_is_an_error` |
| A12 | Low | Test coverage gaps in `tests/fake_http.rs` and a relative-path bug in `tests/real_endpoint.rs` | A | Over real HTTP: 403, an unsupported-parameter 400, 500 then an invalid 200 then success, a non-UTF-8 401, an echoed key, a stall, a redirect, a query string, a proxy, and a failed CLI run. The registry path: see C8 |

## Open items

None. The real-endpoint run (C8) passed on 2026-11-24, after the merge.
