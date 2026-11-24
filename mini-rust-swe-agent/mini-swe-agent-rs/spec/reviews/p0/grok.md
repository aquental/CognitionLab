I'll review phase P0 against the plan's done criteria and the YAML subset, reading only files in this directory. First I'll pull the plan sections and the delivered crate files.

Phase P0's scaffold matches the plan's structure. The toolchain, `[lints]` table, lockfile, byte-identical configs, and 14 PyYAML goldens are real, and `cargo test --locked --test yaml` passes (9 tests). The loader still breaks the §3.10 subset on inputs those tests never feed it: the end-of-file newline workaround rewrites block scalars, and plain non-string map keys are kept as strings.

Checked against PyYAML 6.0.4 (`uv run --frozen` in `mini-swe-agent/`): every file under `config/` and `parity/yaml_cases/` matches `tests/goldens/yaml/`. `config/default.yaml` and `config/mini_textbased.yaml` are byte-identical to `src/minisweagent/config/`. `cargo deny check advisories` reports no advisories. The library path has no `unwrap`, `expect`, `panic!`, or `unsafe`.

1. **High.** `src/yaml.rs:36-59`. `remove_eof_line_break` drops or keeps a trailing newline by guessing from the last leaf and the last source line. That guess does not match PyYAML `safe_load` (`config/__init__.py:61`).

   The parser underneath `serde-saphyr` 2.2.0 is `granit-parser` 2.2.0. Its default reads a block scalar that ends at EOF with no line break as if the line had a break (YAML test-suite `L24T`). `Options::block_scalar_eof_without_line_feed` turns that off and matches libyaml and PyYAML. `serde-saphyr`'s `Options` never forwards the flag. The post-pass tries to undo the default, and the `eof_*` goldens only cover the shape where the file's last line is the scalar line itself (`eof_clip.yaml`, `eof_keep.yaml`, `eof_strip.yaml`, `eof_nested.yaml`). `default.yaml:171` is that shape (`{%- endif %}`, no final `\n`), so the shipped configs still match.

   Measured with `load_str` against PyYAML:

   | Input | Rust | PyYAML |
   |---|---|---|
   | `cmd: \|\n  /tmp\n# files land in /tmp` | `"/tmp"` | `"/tmp\n"` |
   | `foo: \|\n  x\n   ` (last line is spaces, no `\n`) | `"x\n \n"` | `"x\n "` |
   | `folded: >\n  foo\n  bar` | `"foo bar\n"` | `"foo bar"` |
   | `\|+\n  ` | `"\n"` | `""` |
   | `clip: \|\n  y\r` | `"y"` | `"y\n"` |

   The comment case is a false strip: the line `  /tmp` already has a newline, clip keeps it, and `ends_with` (`yaml.rs:57`) still pops because `# files land in /tmp` ends with `/tmp`. Trailing spaces miss the strip because `trim_end` erases them. A final `\r` is a real YAML break, but `ends_with('\n')` (`yaml.rs:36`) is false, so a real clip newline is removed. Folded text is not a suffix of the raw last line, so the invented `\n` stays. `|+` at an indented EOF is `"\n"` (`serde-saphyr`'s own `tests/block_scalar_eof.rs` expects that); `last_value_line` is empty, so the pop is skipped.

   **Fix:** delete the post-pass. Parse with `block_scalar_eof_without_line_feed: true` (forward it through `serde-saphyr`, or build the `Value` from `granit-parser` with that flag). Add the five inputs above as goldens. A span check on `serde_json::Value` cannot see whether the `\n` was invented.

2. **High.** `src/yaml.rs:34` and `tests/yaml.rs:94-96`. §3.10 item 5 says a non-string key is an error. Plain integer, float, and boolean keys are loaded as strings.

   `load_str` deserializes into `serde_json::Value`. That visitor asks for the key with `deserialize_str`. Untagged plain scalars have no explicit tag, and `serde-saphyr` accepts those as string text. Inference to bool or number happens only in `deserialize_any`, which this path never calls. Runtime results: `1: a` → `{"1":"a"}`, `1.5: a` → `{"1.5":"a"}`, `true: a` → `{"true":"a"}`. PyYAML `safe_load` returns an int key, a float key, and `True`. Quoted `"1"` and `"true"` must stay strings. `yes: 1` correctly stays the string key `"yes"` under `strict_booleans`. `~: a` and `? [a, b]` already error. The only test is the sequence key, which fails for a different reason (it is not a scalar).

   **Fix:** deserialize map keys with `deserialize_any` in a private visitor and return `YamlError` unless the key is a string. Do this before the value is stored in a JSON object. After that, `"1"` and `1` are the same key and the check is impossible. Add `1`, `1.5`, `true`, `"1"`, and `yes` as cases.

3. **Medium.** `src/yaml.rs:72-76`. `is_blank_document` uses `str::lines`, which splits on `\n` and `\r\n` only. A leading comment closed by `\r` makes the whole document `null`.

   On Rust 1.99, `"# comment\rkey: value\r"` is one line and `starts_with('#')` is true, so `load_str` returns `Null` and never calls the parser. The same happens for `"# header\rkey: value\n"`. PyYAML returns `{'key': 'value'}` for both. §3.10 says a null document is skipped by the merge, so this config would disappear. A CR-only file with no comment is unaffected, because that single `lines()` chunk does not look like a comment and the parser does accept `\r`.

   **Fix:** split the pre-check on `\r\n`, `\n`, and `\r`. Keep the empty and `# comment\n` cases as null. Add `# header\rkey: value\n` and expect the mapping.

4. **Low.** `Cargo.toml:17-18`. §3.14 says to pin exact versions. `serde-saphyr` is `=2.2.0`. `serde_json = "1.0.154"` and `thiserror = "2.0.21"` are caret requirements (`>=` the named version, `<` the next major). `Cargo.lock` and `--locked` pin today's build. `cargo update` can still move both.

   **Fix:** `serde_json = { version = "=1.0.154", features = ["preserve_order"] }` and `thiserror = "=2.0.21"`.

5. **Low.** `src/yaml.rs:16-20`. `YamlError::Parse(String)` formats the parser error and drops it. Callers cannot tell a budget breach from a syntax error, and `Error::source()` is empty. §3.5's context chain is aimed at the later `AgentError`, but this is the library error P7 will surface.

   **Fix:** keep `serde_saphyr::Error` as `#[source]` (or `#[error(transparent)]`) instead of `e.to_string()`.
