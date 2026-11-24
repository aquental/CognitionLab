//! Rust port of the [mini-swe-agent](https://github.com/SWE-agent/mini-swe-agent) harness.
//!
//! The plan is `spec/migration-plan.md`. Loading the library has no side effects: the process
//! environment, the global config dir and `.env` are read only by the binary (plan §2.2).

pub mod agent;
pub mod clock;
pub mod config;
pub mod environment;
pub mod error;
pub mod host_env;
pub mod merge;
pub mod message;
pub mod model;
pub mod pystr;
pub mod template;
pub mod yaml;
