### Verdict

Phase P1 meets its core done criteria—`recursive_merge` faithfully ports `test_serialize.py`, `Message` preserves `extra` presence and provider fields, `Interrupt` is non-empty by construction, and no `unwrap`/`expect`/`panic!`/`unsafe` appears outside tests. One high-severity trait mismatch (`Model::format_message` dropping `extra`) and three medium-severity design/validation issues (`Action::command` collapsing `Raise` to `""`, unnecessary `where Self: Sized` bounds on `Model`/`Environment`, and lax `RaiseSpec` decoding) should be fixed before P3–P5 build on these traits.

---

### Findings

1. **High — `Model::format_message` omits `extra`, diverging from plan §3.3 and Python's `Model` protocol**
   - **Location**: [src/model.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p1/mini-rust-swe-agent/mini-swe-agent-rs/src/model.rs#L29-L33)
   - **Problem**: `Model::format_message(&self, role: Role, content: String) -> Message` drops the `extra: Option<Map<String, Value>>` parameter.
   - **Evidence**: Plan §3.3 ([migration-plan.md](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p1/mini-rust-swe-agent/mini-swe-agent-rs/spec/migration-plan.md#L465)) specifies `format_message(&self, role, content, extra: Option<Map>) -> Message`. In Python, `DefaultAgent.handle_uncaught_exception` ([default.py](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p1/mini-rust-swe-agent/mini-swe-agent/src/minisweagent/agents/default.py#L76-L85)) calls `self.model.format_message(role="exit", content=str(e), extra={...})`, and `Model` implementations can inspect `extra` during formatting (e.g., [test_models.py](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p1/mini-rust-swe-agent/mini-swe-agent/src/minisweagent/models/test_models.py#L234-L243)). Without `extra`, P5's `DefaultAgent` must bypass `format_message` or mutate `.extra` afterward.
   - **Fix**: Change the signature to `fn format_message(&self, role: Role, content: String, extra: Option<Map<String, Value>>) -> Message` with default body:
     ```rust
     match extra {
         Some(extra) => Message::new(role, content).with_extra(extra),
         None => Message::new(role, content),
     }
     ```
     Update [tests/traits.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p1/mini-rust-swe-agent/mini-swe-agent-rs/tests/traits.rs#L90) and add a test with `Some(extra)`.

2. **Medium — `Action::command(&self) -> &str` silently turns `Action::Raise` into an empty command `""`**
   - **Location**: [src/message.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p1/mini-rust-swe-agent/mini-swe-agent-rs/src/message.rs#L210-L216) (used in [tests/traits.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p1/mini-rust-swe-agent/mini-swe-agent-rs/tests/traits.rs#L54))
   - **Problem**: Both plan §3.2 ([migration-plan.md](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p1/mini-rust-swe-agent/mini-swe-agent-rs/spec/migration-plan.md#L448)) and the doc comment at [src/message.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p1/mini-rust-swe-agent/mini-swe-agent-rs/src/message.rs#L154) state that `Action::Raise` is accepted only by `DeterministicModel`. Yet `Environment::execute` takes `&Action`, and `Action::command(&self)` returns `""` for `Action::Raise(_)`. Any `Environment` calling `action.command()` (as `FakeEnv` already does in `tests/traits.rs:54`) will silently execute an empty bash command (`returncode: 0`) if a `Raise` action ever reaches `execute`.
   - **Fix**: Return `Option<&str>` from `Action::command(&self)` (`Some(command)` for `Action::Command`, `None` for `Action::Raise`), forcing `LocalEnvironment::execute` in P3 to reject `Action::Raise` explicitly with an error.

3. **Medium — Unnecessary `where Self: Sized` bounds on `Model::{type_name, serialize}` and `Environment::{type_name, serialize}`**
   - **Location**: [src/model.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p1/mini-rust-swe-agent/mini-swe-agent-rs/src/model.rs#L18-L23), [src/model.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p1/mini-rust-swe-agent/mini-swe-agent-rs/src/model.rs#L49-L56), [src/environment.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p1/mini-rust-swe-agent/mini-swe-agent-rs/src/environment.rs#L20-L25), [src/environment.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p1/mini-rust-swe-agent/mini-swe-agent-rs/src/environment.rs#L35-L42)
   - **Problem**: Both `type_name` and `serialize` require `Self: Sized`, even though `std::any::type_name::<T>()` accepts `T: ?Sized` and `serde_json::to_value(self.config())` only needs `Self::Config: Serialize` (already `Sized`).
   - **Evidence**: These bounds prevent forwarding `type_name()` and `serialize()` in blanket impls for unsized wrappers (`impl<M: Model + ?Sized> Model for &mut M` / `Box<M>`) and add unnecessary friction if P6 recording wrappers or D10 ever generalize over `?Sized` types.
   - **Fix**: Remove `where Self: Sized` from `type_name` and `serialize` on both traits.

4. **Medium — `RaiseSpec::decode` silently coerces non-string `message` to `""` and accepts empty `kind`**
   - **Location**: [src/message.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p1/mini-rust-swe-agent/mini-swe-agent-rs/src/message.rs#L232-L253)
   - **Problem**:
     - `map.get("message").and_then(Value::as_str).unwrap_or_default()` silently drops non-string values (e.g., `{"raise": {"kind": "RuntimeError", "message": 123}}` decodes as `message: ""`), inconsistent with `Action::decode` rejecting non-string `command` at line 206.
     - `{"raise": {"kind": ""}}` is accepted as `RaiseSpec::Fatal { kind: "", message: "" }`, producing an empty `exit_status`, and a non-string `kind` misleadingly reports ``"`raise.kind` is missing"``.
   - **Fix**: Match explicitly on `map.get("kind")` (requiring a non-empty `Value::String`) and `map.get("message")` (`None => String::new()`, `Some(Value::String(s)) => s.clone()`, `Some(_) => Err(...)`). Add corresponding negative tests in [tests/core_types.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p1/mini-rust-swe-agent/mini-swe-agent-rs/tests/core_types.rs#L190-L197).

5. **Low — `ManualClock::advance` uses a non-atomic load-then-store on a `Sync` type**
   - **Location**: [src/clock.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p1/mini-rust-swe-agent/mini-swe-agent-rs/src/clock.rs#L51-L53)
   - **Problem**: `ManualClock` is `Sync` (shared via `Arc<dyn Clock>`) and uses `AtomicU64` with `Ordering::SeqCst`, but `advance` calls `self.set(self.now() + seconds)`, which can lose an update if called concurrently.
   - **Fix**: Replace the body with `AtomicU64::fetch_update`:
     ```rust
     let _ = self.bits.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |bits| {
         Some((f64::from_bits(bits) + seconds).to_bits())
     });
     ```

6. **Low — `merge_into` does a redundant lookup and key clone when overwriting an existing key**
   - **Location**: [src/merge.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p1/mini-rust-swe-agent/mini-swe-agent-rs/src/merge.rs#L44-L51)
   - **Problem**: When `target.get_mut(key)` returns `Some(slot)` for a non-object override, the `_` arm drops `slot` and calls `target.insert(key.clone(), value.clone())`, incurring a second hash lookup and an unnecessary `String` allocation for `key`.
   - **Fix**: Split `_` into `(Some(existing), incoming) => *existing = incoming.clone()` and `(None, incoming) => { target.insert(key.clone(), incoming.clone()); }`.

7. **Low — Minor API and test coverage gaps against plan §3.5 and borrowing conventions**
   - **Locations**:
     - [src/host_env.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p1/mini-rust-swe-agent/mini-swe-agent-rs/src/host_env.rs#L44-L46): `child_vars` yields `(&OsString, &OsString)` instead of `(&OsStr, &OsStr)`, leaking the backing map's owned key/value types. Map to `(k.as_os_str(), v.as_os_str())`.
     - [src/error.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p1/mini-rust-swe-agent/mini-swe-agent-rs/src/error.rs#L75-L119): Plan §3.5 ([migration-plan.md](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p1/mini-rust-swe-agent/mini-swe-agent-rs/spec/migration-plan.md#L542)) calls for constructors per `InterruptKind`; adding helpers such as `Interrupt::submitted`, `Interrupt::limits_exceeded`, `Interrupt::time_exceeded`, and `Interrupt::format_error` centralizes the exact `Message::exit` shapes from [default.py](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p1/mini-rust-swe-agent/mini-swe-agent/src/minisweagent/agents/default.py#L133-L147) and [local.py](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p1/mini-rust-swe-agent/mini-swe-agent/src/minisweagent/environments/local.py#L50-L56) instead of repeating status strings in P3–P5.
     - [tests/core_types.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p1/mini-rust-swe-agent/mini-swe-agent-rs/tests/core_types.rs) and [tests/merge.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p1/mini-rust-swe-agent/mini-swe-agent-rs/tests/merge.rs): `Action::Raise(...).to_value()` round-tripping ([src/message.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p1/mini-rust-swe-agent/mini-swe-agent-rs/src/message.rs#L256-L263)) and `Interrupt::head_mut` ([src/error.rs](file:///private/tmp/claude-501/-Users-aquental-projects-ai-CognitionLab/ec96b18b-f9e7-4344-8c8f-4355f6804c29/scratchpad/review-p1/mini-rust-swe-agent/mini-swe-agent-rs/src/error.rs#L103-L105)) are untested, and `tests/merge.rs` does not yet test falsy overrides (`false`, `0`, `""` overriding truthy values, plan §3.10).
