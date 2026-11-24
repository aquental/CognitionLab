//! P1 core types: messages, actions, interrupts, clock, host environment, trait defaults.

use std::ffi::OsString;

use mini_swe_agent_rs::clock::{Clock, ManualClock, SystemClock};
use mini_swe_agent_rs::error::{AgentError, Interrupt, InterruptKind, ModelError, StepError};
use mini_swe_agent_rs::host_env::HostEnv;
use mini_swe_agent_rs::message::{
    Action, ActionError, ExecOutput, ExitInfo, Message, RaiseSpec, Role,
};
use serde_json::{Value, json};

/// Test helper: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::unwrap_used)]
fn round_trip(value: Value) -> Value {
    let message: Message = serde_json::from_value(value).unwrap();
    serde_json::to_value(&message).unwrap()
}

// ---- Message -------------------------------------------------------------------------------

#[test]
fn message_without_extra_round_trips_without_gaining_one() {
    let value = json!({"role": "system", "content": "hello"});
    assert_eq!(round_trip(value.clone()), value);
}

#[test]
fn message_with_empty_extra_keeps_it() {
    let value = json!({"role": "user", "content": "x", "extra": {}});
    assert_eq!(round_trip(value.clone()), value);
}

#[test]
fn provider_fields_are_kept() {
    let value = json!({"role": "assistant", "content": "x", "tool_calls": null, "function_call": null,
                       "extra": {"cost": 1.0}});
    assert_eq!(round_trip(value.clone()), value);
}

#[test]
fn roles_serialize_in_lowercase() {
    for (role, text) in [
        (Role::System, "system"),
        (Role::User, "user"),
        (Role::Assistant, "assistant"),
        (Role::Exit, "exit"),
    ] {
        assert_eq!(serde_json::to_value(role).unwrap(), json!(text));
    }
}

#[test]
fn provider_field_setter_rejects_reserved_keys() {
    let mut message = Message::new(Role::Assistant, "x");
    for key in ["role", "content", "extra"] {
        assert!(message.set_provider_field(key, json!(1)).is_err(), "{key}");
    }
    message
        .set_provider_field("tool_calls", Value::Null)
        .unwrap();
    assert_eq!(
        serde_json::to_value(&message).unwrap(),
        json!({"role": "assistant", "content": "x", "tool_calls": null})
    );
}

/// Test helper: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::unwrap_used)]
fn message_with_extra(extra: Value) -> Message {
    serde_json::from_value(json!({"role": "assistant", "content": "", "extra": extra})).unwrap()
}

#[test]
fn cost_defaults_to_zero_only_when_missing() {
    // Python: `extra.get("cost", 0.0)`, then `+=`; a present non-number fails the addition.
    assert_eq!(Message::new(Role::Assistant, "x").cost().unwrap(), 0.0);
    assert_eq!(message_with_extra(json!({})).cost().unwrap(), 0.0);
    assert_eq!(message_with_extra(json!({"cost": 0})).cost().unwrap(), 0.0);
    assert_eq!(
        message_with_extra(json!({"cost": 2.5})).cost().unwrap(),
        2.5
    );
    assert!(message_with_extra(json!({"cost": null})).cost().is_err());
    assert!(message_with_extra(json!({"cost": "1"})).cost().is_err());
}

#[test]
fn null_extra_is_rejected_at_decoding() {
    // Python would fail later on `None.get(...)`; Rust refuses the message at the boundary.
    let result: Result<Message, _> =
        serde_json::from_value(json!({"role": "assistant", "content": "", "extra": null}));
    assert!(result.is_err());
}

#[test]
fn exit_message_constructor() {
    let message = Message::exit("Submitted", "done\n", "done\n");
    assert_eq!(
        serde_json::to_value(&message).unwrap(),
        json!({"role": "exit", "content": "done\n", "extra": {"exit_status": "Submitted", "submission": "done\n"}})
    );
    assert_eq!(
        message.exit_info(),
        Some(ExitInfo {
            exit_status: "Submitted".into(),
            submission: "done\n".into()
        })
    );
}

#[test]
fn limit_exit_message_has_status_as_content() {
    // default.py:133-147: content is the status, submission is empty.
    assert_eq!(
        serde_json::to_value(Message::exit("LimitsExceeded", "LimitsExceeded", "")).unwrap(),
        json!({"role": "exit", "content": "LimitsExceeded", "extra": {"exit_status": "LimitsExceeded", "submission": ""}})
    );
}

#[test]
fn exit_info_reads_missing_fields_as_empty() {
    let message: Message = serde_json::from_value(
        json!({"role": "exit", "content": "", "extra": {"exit_status": "X"}}),
    )
    .unwrap();
    assert_eq!(
        message.exit_info(),
        Some(ExitInfo {
            exit_status: "X".into(),
            submission: String::new()
        })
    );
    assert_eq!(Message::new(Role::User, "x").exit_info(), None);
}

// ---- Action decoding (key presence, plan §3.2) ----------------------------------------------

/// Test helper: clippy.toml exempts only `#[test]` functions.
#[allow(clippy::unwrap_used)]
fn actions_of(extra: Value) -> Result<Vec<Action>, ActionError> {
    let message: Message =
        serde_json::from_value(json!({"role": "assistant", "content": "", "extra": extra}))
            .unwrap();
    message.actions()
}

#[test]
fn missing_actions_are_empty() {
    assert_eq!(actions_of(json!({})).unwrap(), vec![]);
    assert_eq!(
        Message::new(Role::Assistant, "x").actions().unwrap(),
        vec![]
    );
}

#[test]
fn command_action() {
    assert_eq!(
        actions_of(json!({"actions": [{"command": "ls"}]})).unwrap(),
        vec![Action::Command {
            command: "ls".into()
        }]
    );
}

#[test]
fn empty_object_is_an_empty_command() {
    // Python: action.get("command", "") (local.py:26)
    assert_eq!(
        actions_of(json!({"actions": [{}]})).unwrap(),
        vec![Action::Command {
            command: String::new()
        }]
    );
}

#[test]
fn raise_only_is_a_raise() {
    let actions = actions_of(json!({"actions": [{"raise": {"kind": "FormatError", "messages": [{"role": "user", "content": "bad"}]}}]})).unwrap();
    let [Action::Raise(RaiseSpec::Interrupt(interrupt))] = actions.as_slice() else {
        panic!("{actions:?}")
    };
    assert_eq!(interrupt.kind(), InterruptKind::FormatError);
    assert_eq!(interrupt.messages().count(), 1);
}

#[test]
fn raise_wins_over_command() {
    // Python checks `"raise" in action` first (test_models.py:78).
    let actions = actions_of(json!({"actions": [{"command": "ls", "raise": {"kind": "RuntimeError", "message": "boom"}}]})).unwrap();
    assert_eq!(
        actions,
        vec![Action::Raise(RaiseSpec::Fatal {
            kind: "RuntimeError".into(),
            message: "boom".into()
        })]
    );
}

#[test]
fn malformed_raise_is_an_error_not_an_empty_command() {
    assert!(actions_of(json!({"actions": [{"raise": "FormatError"}]})).is_err());
    assert!(
        actions_of(json!({"actions": [{"raise": {"kind": "FormatError", "messages": []}}]}))
            .is_err()
    );
    assert!(actions_of(json!({"actions": [{"raise": {"kind": "FormatError"}}]})).is_err());
}

#[test]
fn malformed_actions_are_errors() {
    assert!(actions_of(json!({"actions": "ls"})).is_err());
    assert!(actions_of(json!({"actions": ["ls"]})).is_err());
    assert!(actions_of(json!({"actions": [{"command": 3}]})).is_err());
}

#[test]
fn decoding_actions_leaves_the_message_json_intact() {
    let value = json!({"role": "assistant", "content": "", "extra": {"actions": [{"command": "ls", "id": "call_1"}]}});
    let message: Message = serde_json::from_value(value.clone()).unwrap();
    let actions = message.actions().unwrap();
    assert_eq!(actions[0].command(), "ls");
    assert_eq!(actions[0].to_value(), json!({"command": "ls"}));
    assert_eq!(serde_json::to_value(&message).unwrap(), value);
}

// ---- ExecOutput ------------------------------------------------------------------------------

#[test]
fn exec_output_omits_extra_on_success() {
    let output = ExecOutput {
        output: "hi\n".into(),
        returncode: 0,
        exception_info: String::new(),
        extra: None,
    };
    assert_eq!(
        serde_json::to_value(&output).unwrap(),
        json!({"output": "hi\n", "returncode": 0, "exception_info": ""})
    );
}

// ---- Interrupt and errors --------------------------------------------------------------------

#[test]
fn interrupt_cannot_be_empty() {
    assert!(Interrupt::from_messages(InterruptKind::Submitted, vec![]).is_none());
    let interrupt = Interrupt::new(
        InterruptKind::Submitted,
        Message::new(Role::Exit, ""),
        vec![],
    );
    assert_eq!(interrupt.head().role, Role::Exit);
    assert_eq!(interrupt.into_messages().len(), 1);
}

#[test]
fn time_exceeded_is_a_limit() {
    assert!(InterruptKind::TimeExceeded.is_limit());
    assert!(InterruptKind::LimitsExceeded.is_limit());
    assert!(!InterruptKind::Submitted.is_limit());
    assert!(!InterruptKind::FormatError.is_limit());
}

#[test]
fn interrupt_kind_names_match_python() {
    for (kind, name) in [
        (InterruptKind::Submitted, "Submitted"),
        (InterruptKind::LimitsExceeded, "LimitsExceeded"),
        (InterruptKind::TimeExceeded, "TimeExceeded"),
        (InterruptKind::UserInterruption, "UserInterruption"),
        (InterruptKind::FormatError, "FormatError"),
    ] {
        assert_eq!(kind.name(), name);
        assert_eq!(InterruptKind::from_name(name), Some(kind));
    }
    assert_eq!(InterruptKind::from_name("RuntimeError"), None);
}

#[test]
fn errors_convert_into_step_error() {
    fn model() -> Result<(), StepError> {
        Err(ModelError::OutputsExhausted)?
    }
    fn agent() -> Result<(), StepError> {
        Err(AgentError::Internal("x".into()))?
    }
    assert!(matches!(
        model(),
        Err(StepError::Fatal(AgentError::Model(
            ModelError::OutputsExhausted
        )))
    ));
    assert!(matches!(
        agent(),
        Err(StepError::Fatal(AgentError::Internal(_)))
    ));
}

#[test]
fn fatal_exit_status_names_are_stable() {
    // D6: a stable name per variant; the comparator maps them to Python's class names.
    assert_eq!(
        AgentError::Model(ModelError::OutputsExhausted).exit_status(),
        "OutputsExhausted"
    );
    assert_eq!(
        AgentError::Model(ModelError::GlobalLimitExceeded {
            cost: 1.0,
            calls: 2
        })
        .exit_status(),
        "GlobalLimitExceeded"
    );
    assert_eq!(
        AgentError::Model(ModelError::Injected {
            kind: "RuntimeError".into(),
            message: "x".into()
        })
        .exit_status(),
        "RuntimeError"
    );
}

// ---- Clock -----------------------------------------------------------------------------------

#[test]
fn manual_clock_advance_is_atomic() {
    let clock = std::sync::Arc::new(ManualClock::new(0.0));
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let clock = clock.clone();
            std::thread::spawn(move || {
                for _ in 0..1000 {
                    clock.advance(1.0);
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(clock.now(), 8000.0);
}

#[test]
fn manual_clock_is_settable() {
    let clock = ManualClock::new(100.0);
    assert_eq!(clock.now(), 100.0);
    clock.advance(1.5);
    assert_eq!(clock.now(), 101.5);
    clock.set(99.0);
    assert_eq!(clock.now(), 99.0);
}

#[test]
fn system_clock_is_epoch_seconds() {
    let now = SystemClock.now();
    assert!(now > 1.7e9 && now.is_finite(), "{now}");
}

// ---- HostEnv ---------------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn host_env_keeps_non_utf8_entries_for_children_only() {
    use std::os::unix::ffi::OsStringExt;
    let bad = OsString::from_vec(vec![0x66, 0xff]);
    let env = HostEnv::from_vars([
        (OsString::from("PATH"), OsString::from("/bin")),
        (OsString::from("BAD"), bad.clone()),
    ]);
    assert_eq!(env.get("PATH"), Some("/bin"));
    assert_eq!(env.get("BAD"), None);
    assert!(
        env.child_vars()
            .any(|(k, v)| k == "BAD" && v == bad.as_os_str())
    );
    assert_eq!(
        env.template_vars(),
        json!({"PATH": "/bin"}).as_object().unwrap().clone()
    );
}

#[test]
fn host_env_overlay_does_not_override() {
    let mut env = HostEnv::from_vars([(OsString::from("A"), OsString::from("1"))]);
    env.overlay_missing([
        ("A".to_owned(), "2".to_owned()),
        ("B".to_owned(), "3".to_owned()),
    ]);
    assert_eq!(env.get("A"), Some("1"));
    assert_eq!(env.get("B"), Some("3"));
}

// ---- Review fixes: raise payloads, in-order decoding, constructors ---------------------------

#[test]
fn fatal_raise_payload_is_validated() {
    for raise in [
        json!({"kind": "RuntimeError", "message": 42}),
        json!({"kind": ""}),
        json!({"kind": 7, "message": "x"}),
        Value::Null,
    ] {
        let result = actions_of(json!({"actions": [{"raise": raise}]}));
        assert!(
            matches!(result, Err(ActionError::Invalid { index: 0, .. })),
            "{raise}"
        );
    }
    let result =
        actions_of(json!({"actions": [{"raise": {"kind": "FormatError"}, "command": "ls"}]}));
    assert!(
        result.is_err(),
        "a malformed raise never falls back to the command"
    );
    let ok = actions_of(json!({"actions": [{"raise": {"kind": "RuntimeError"}}]})).unwrap();
    assert_eq!(
        ok,
        [Action::Raise(RaiseSpec::Fatal {
            kind: "RuntimeError".into(),
            message: String::new()
        })]
    );
}

#[test]
fn actions_decode_in_order() {
    // Python inspects actions one at a time, so a valid first action is seen before a
    // malformed second one.
    let message = message_with_extra(json!({"actions": [{"command": "a"}, 5]}));
    let mut actions = message.action_iter();
    assert_eq!(actions.next().unwrap().unwrap().command(), "a");
    assert!(matches!(
        actions.next(),
        Some(Err(ActionError::Invalid { index: 1, .. }))
    ));
    assert!(actions.next().is_none());
    let not_a_list = message_with_extra(json!({"actions": "ls"}));
    let collected: Vec<_> = not_a_list.action_iter().collect();
    assert!(matches!(collected.as_slice(), [Err(ActionError::NotAList)]));
}

#[test]
fn raise_actions_round_trip_through_to_value() {
    let raw = json!([
        {"raise": {"kind": "FormatError", "messages": [{"role": "user", "content": "bad"}]}},
        {"raise": {"kind": "RuntimeError", "message": "boom"}},
        {"command": "ls"}
    ]);
    let actions = actions_of(json!({"actions": raw.clone()})).unwrap();
    let back: Vec<Value> = actions.iter().map(Action::to_value).collect();
    assert_eq!(Value::Array(back), raw);
}

#[test]
fn interrupt_constructors_build_python_messages() {
    let submitted = Interrupt::submitted("out\n".to_owned());
    assert_eq!(submitted.kind(), InterruptKind::Submitted);
    assert_eq!(
        serde_json::to_value(submitted.head()).unwrap(),
        json!({"role": "exit", "content": "out\n", "extra": {"exit_status": "Submitted", "submission": "out\n"}})
    );
    for (interrupt, name) in [
        (Interrupt::limits_exceeded(), "LimitsExceeded"),
        (Interrupt::time_exceeded(), "TimeExceeded"),
    ] {
        assert_eq!(interrupt.kind().name(), name);
        assert_eq!(
            serde_json::to_value(interrupt.head()).unwrap(),
            json!({"role": "exit", "content": name, "extra": {"exit_status": name, "submission": ""}})
        );
    }
}

#[test]
fn interrupt_head_mut_edits_the_first_message() {
    let mut interrupt = Interrupt::new(
        InterruptKind::FormatError,
        Message::new(Role::User, "a"),
        vec![Message::new(Role::User, "b")],
    );
    interrupt.head_mut().content.push('!');
    let contents: Vec<String> = interrupt
        .into_messages()
        .into_iter()
        .map(|m| m.content)
        .collect();
    assert_eq!(contents, ["a!", "b"]);
}
