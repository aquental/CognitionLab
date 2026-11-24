### Verdict

Phase P0 delivers a clean, minimal Rust scaffold that satisfies most of the plan's P0 criteria (toolchain, `unsafe_code = "forbid"`, strict Clippy lints, `serde-saphyr` integration, attribution, and Python baseline). However, the EOF block-scalar workaround in [`src/yaml.rs`](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/src/yaml.rs#L49-L60) is brittle and has both data-corrupting false positives and false negatives, and several §3.10 item 5 claims (notably non-string keys and error-chain preservation) are either untested or unfulfilled. Fix the `remove_eof_line_break` logic and close the YAML subset/test gaps before building `config.rs` on top of it.

---

### Findings

1. **High — `remove_eof_line_break` silently corrupts valid strings and misses multiple valid EOF block-scalar cases**
   - **File & line:** [src/yaml.rs](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/src/yaml.rs#L36-L69)
   - **Problem:** `remove_eof_line_break` uses a post-hoc string-suffix heuristic (`last_text_line.trim_end().ends_with(last_value_line)`) against the last leaf in `serde_json::Value`. When the input has no trailing `\n`, this fails in five concrete ways:
     1. *False positive (data corruption) — quoted scalar with a trailing comment:* On `quoted: "x\n" # x` (no final newline), both PyYAML and `serde-saphyr` produce `"x\n"`. Because `body` is `"x"` and `last_text_line.trim_end()` ends with `"x"`, `s.pop()` corrupts `"x\n"` into `"x"`. This directly contradicts the doc comment at [src/yaml.rs](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/src/yaml.rs#L48) ("Quoted scalars are not affected: their last line ends with the quote").
     2. *False positive (data corruption) — block scalar with a real newline followed by a matching comment:* On `a: |\n  foo\n# foo` (no final newline), PyYAML produces `"foo\n"`, but `last_text_line` ends with `"foo"`, so `s.pop()` strips the legitimate `\n`.
     3. *False negative — trailing spaces on the last line of a literal block scalar:* On `a: |\n  foo   ` (no final newline), YAML preserves trailing spaces (`last_value_line = "foo   "`), while `last_text_line.trim_end()` strips them (`"  foo"`), so `ends_with` returns `false` and the invented `\n` stays.
     4. *False negative — multi-line folded block scalar (`>` / `>+`) at EOF:* On `a: >\n  hello\n  world` (no final newline), `last_value_line` is `"hello world"` while `last_text_line` is `"  world"`, so `ends_with` returns `false` and the invented `\n` stays.
     5. *False negative — duplicate key or `<<` merge override:* Because `serde_json` has `preserve_order` (`IndexMap`), overwriting an earlier key updates its value in place without moving it to the back. For `a: 1\nb: 2\na: |\n  x` (no final newline), `last_leaf` inspects `b` (`2`) instead of `a`, leaving the invented `\n` on `a`.
   - **Evidence:** [src/yaml.rs](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/src/yaml.rs#L49-L69) and the missing `eof_*` cases in [tests/yaml.rs](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/tests/yaml.rs#L43-L56).
   - **Recommended fix:** Replace the value/text suffix heuristic with a check that operates on YAML structure (for example, detecting whether the parser/lexer ended inside a clip/keep block scalar before deserializing, or inspecting the raw trailing block-scalar header/indentation rather than matching `last_value_line` against `last_text_line.trim_end()`). Add PyYAML golden fixtures in `parity/yaml_cases/` for all five edge cases above.

2. **Medium — Scalar non-string keys are untested and silently coerced to strings, contradicting plan §3.10 item 5**
   - **File & line:** [tests/yaml.rs](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/tests/yaml.rs#L93-L96) vs. [spec/migration-plan.md](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/spec/migration-plan.md#L672)
   - **Problem:** Plan §3.10 item 5 specifies that **"a non-string key is an error."** However, `tests/yaml.rs` only defines `non_scalar_keys_are_an_error` with a sequence key (`? [a, b]\n: c\n`). Because `load_str` deserializes directly into `serde_json::Value` (whose map visitor calls `deserialize_string` for keys), scalar non-string keys such as `1: a`, `true: b`, or `null: c` are silently accepted as `"1"`, `"true"`, and `"null"` rather than rejected (whereas PyYAML `safe_load` returns `{1: "a", True: "b", None: "c"}`).
   - **Evidence:** [tests/yaml.rs](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/tests/yaml.rs#L93-L96), [src/yaml.rs](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/src/yaml.rs#L34), and [spec/migration-plan.md](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/spec/migration-plan.md#L672).
   - **Recommended fix:** Either enforce string-only keys explicitly and add tests for `1: a`, `true: b`, and `null: c`, or—if scalar key coercion is intended—narrow plan §3.10 item 5 and the `src/yaml.rs` doc comment to "non-scalar keys are an error; scalar keys stringify" and test that explicitly.

3. **Medium — `YamlError::Parse(String)` discards the underlying error `source()` chain**
   - **File & line:** [src/yaml.rs](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/src/yaml.rs#L15-L20), [src/yaml.rs](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/src/yaml.rs#L34-L35)
   - **Problem:** `load_str` converts `serde_saphyr::Error` with `.map_err(|e| YamlError::Parse(e.to_string()))`, flattening the error into a `String` so `std::error::Error::source()` always returns `None`. This breaks the `err-context-chain` guideline in the plan (where fatal `extra.traceback` is constructed from the `source()` chain) and loses structured error context.
   - **Evidence:** [src/yaml.rs](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/src/yaml.rs#L16-L20), [spec/migration-plan.md](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/spec/migration-plan.md#L321), [spec/migration-plan.md](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/spec/migration-plan.md#L839).
   - **Recommended fix:** Store the boxed source error in `YamlError::Parse`:
     ```rust
     #[derive(Debug, thiserror::Error)]
     pub enum YamlError {
         #[error("invalid YAML: {0}")]
         Parse(#[source] Box<dyn std::error::Error + Send + Sync>),
     }
     ```
     This preserves `source()` and keeps `serde-saphyr` out of the public API while keeping `YamlError` pointer-sized.

4. **Medium — `is_blank_document` bypasses `serde-saphyr` resource budgets, accepts invalid YAML control characters, and budget/stream tests have gaps**
   - **File & line:** [src/yaml.rs](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/src/yaml.rs#L31-L35), [src/yaml.rs](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/src/yaml.rs#L71-L77), [tests/yaml.rs](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/tests/yaml.rs#L98-L109)
   - **Problem:**
     1. `is_blank_document(text)` scans the entire input before `serde_saphyr` enforces its resource budget. An arbitrarily large comment-only or whitespace-only input is scanned in full and returns `Ok(Value::Null)` with no size cap.
     2. `is_blank_document` calls `str::trim()`, which strips Unicode whitespace including `\x0c` (form feed) and `\x0b` (vertical tab)—characters that are **illegal** in YAML 1.2 and rejected by PyYAML `safe_load`. Passing `"\x0c"` returns `Ok(Value::Null)` instead of `Err(YamlError::Parse(_))`.
     3. Tests cover alias expansion (`alias_expansion_is_bounded`), but do not verify deep nesting limits (`[[[...]]]`) or that multi-document streams (`a: 1\n---\nb: 2\n`) are rejected despite the doc comment "Parses one YAML document" at [src/yaml.rs](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/src/yaml.rs#L29).
   - **Evidence:** [src/yaml.rs](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/src/yaml.rs#L71-L77) and [tests/yaml.rs](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/tests/yaml.rs#L79-L109).
   - **Recommended fix:** Enforce an explicit byte-length cap at the start of `load_str`, restrict `is_blank_document` trimming to YAML whitespace (`[' ', '\t', '\r']` after `lines()`), and add tests for multi-document streams, deep nesting, and invalid control characters.

5. **Low — Dependency version pinning in `Cargo.toml` is inconsistent with plan §3.14 / P0**
   - **File & line:** [Cargo.toml](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/Cargo.toml#L14-L18) vs. [spec/migration-plan.md](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/spec/migration-plan.md#L728)
   - **Problem:** Plan §3.14 says "Pin exact versions in P0" and the P0 deliverable says "dependencies pinned." `serde-saphyr` is pinned with `"=2.2.0"`, but `serde_json` (`"1.0.154"`) and `thiserror` (`"2.0.21"`) use caret ranges (`^`) rather than `=`.
   - **Evidence:** [Cargo.toml](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/Cargo.toml#L16-L18).
   - **Recommended fix:** Either use `"=1.0.154"` and `"=2.0.21"` for consistency with the plan and `publish = false`, or document in `Cargo.toml` that only `serde-saphyr` uses `=` (per D5) while `Cargo.lock` pins the rest.

6. **Low — `subset_cases_match_pyyaml` hardcodes fixture names while `parity/yaml_dump.py` globs dynamically**
   - **File & line:** [tests/yaml.rs](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/tests/yaml.rs#L41-L59) vs. [parity/yaml_dump.py](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/parity/yaml_dump.py#L24-L29)
   - **Problem:** Adding a new `.yaml` fixture to `parity/yaml_cases/` and regenerating goldens with `parity/yaml_dump.py` will silently leave it untested unless its filename is also added to the hardcoded array in `tests/yaml.rs`. In addition, `parity/yaml_dump.py` writes both `config/*.yaml` and `parity/yaml_cases/*.yaml` into the same flat `tests/goldens/yaml/<stem>.json` directory without checking for stem collisions.
   - **Evidence:** [tests/yaml.rs](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/tests/yaml.rs#L43-L56) and [parity/yaml_dump.py](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/parity/yaml_dump.py#L18-L28).
   - **Recommended fix:** Read `parity/yaml_cases/` in sorted order in `subset_cases_match_pyyaml` (asserting the list is non-empty), and check for duplicate output stems in `parity/yaml_dump.py`.

7. **Low — Minor documentation inaccuracies in `src/yaml.rs` and `README.md`**
   - **File & line:** [src/yaml.rs](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/src/yaml.rs#L46), [README.md](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/README.md#L20-L25), [README.md](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/README.md#L54-L60)
   - **Problem:**
     - Both [src/yaml.rs](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/src/yaml.rs#L46) and [README.md](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/README.md#L60) state only that `default.yaml` ends inside a block scalar without a final newline, omitting that [config/mini_textbased.yaml](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/config/mini_textbased.yaml#L171) ends the exact same way.
     - [README.md](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/README.md#L54-L55) omits `dbg_macro` (`dbg!`) from the summary of denied Clippy lints, even though it is denied in [Cargo.toml](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/Cargo.toml#L30).
     - [README.md](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/README.md#L20-L25) does not list `cargo test --doc` from the per-phase quality gates in [spec/migration-plan.md](file:///Users/aquental/projects/ai/CognitionLab/mini-rust-swe-agent/mini-swe-agent-rs/spec/migration-plan.md#L805) (though `cargo test --locked` runs doctests implicitly).
   - **Recommended fix:** Update the doc comment and `README.md` to reference both shipped configs, include `dbg_macro` in the lint note, and note `cargo test --doc` (or that `cargo test --locked` covers doctests).
