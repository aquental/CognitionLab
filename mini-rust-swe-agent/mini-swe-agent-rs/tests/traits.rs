//! Default methods of the `Model` and `Environment` traits (plan §3.3).

// Test crate: helpers outside `#[test]` functions may panic on broken fixtures.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::path::Path;

use mini_swe_agent_rs::environment::Environment;
use mini_swe_agent_rs::error::{AgentError, StepError};
use mini_swe_agent_rs::message::{Action, ExecOutput, Message, Role};
use mini_swe_agent_rs::model::Model;
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Serialize)]
struct FakeConfig {
    model_name: String,
}

struct FakeModel(FakeConfig);

impl Model for FakeModel {
    type Config = FakeConfig;
    fn config(&self) -> &FakeConfig {
        &self.0
    }
    fn query(&mut self, _messages: &[Message]) -> Result<Message, StepError> {
        Ok(Message::new(Role::Assistant, "x"))
    }
    fn format_observation_messages(
        &self,
        _: &Message,
        _: &[ExecOutput],
        _: &Value,
    ) -> Result<Vec<Message>, AgentError> {
        Ok(vec![])
    }
}

struct FakeEnv(FakeConfig);

impl Environment for FakeEnv {
    type Config = FakeConfig;
    fn config(&self) -> &FakeConfig {
        &self.0
    }
    fn execute(&self, action: &Action, _cwd: Option<&Path>) -> Result<ExecOutput, StepError> {
        Ok(ExecOutput {
            output: action.command().to_owned(),
            returncode: 0,
            exception_info: String::new(),
            extra: None,
        })
    }
    fn template_vars(&self) -> Result<Value, AgentError> {
        Ok(json!({}))
    }
}

#[test]
fn model_defaults_mirror_model_dump() {
    let model = FakeModel(FakeConfig {
        model_name: "m".into(),
    });
    assert_eq!(model.template_vars().unwrap(), json!({"model_name": "m"}));
    let serialized = model.serialize().unwrap();
    assert_eq!(
        serialized["info"]["config"]["model"],
        json!({"model_name": "m"})
    );
    assert!(
        serialized["info"]["config"]["model_type"]
            .as_str()
            .unwrap()
            .ends_with("FakeModel")
    );
}

#[test]
fn format_message_returns_its_arguments() {
    let model = FakeModel(FakeConfig {
        model_name: "m".into(),
    });
    assert_eq!(
        serde_json::to_value(model.format_message(Role::System, "hi".into())).unwrap(),
        json!({"role": "system", "content": "hi"})
    );
}

#[test]
fn environment_default_serialize() {
    let env = FakeEnv(FakeConfig {
        model_name: "e".into(),
    });
    let serialized = env.serialize().unwrap();
    assert_eq!(
        serialized["info"]["config"]["environment"],
        json!({"model_name": "e"})
    );
    assert!(
        serialized["info"]["config"]["environment_type"]
            .as_str()
            .unwrap()
            .ends_with("FakeEnv")
    );
    let output = env
        .execute(
            &Action::Command {
                command: "ls".into(),
            },
            None,
        )
        .unwrap();
    assert_eq!(output.output, "ls");
}
