//! Rust port of the [mini-swe-agent](https://github.com/SWE-agent/mini-swe-agent) harness.
//!
//! The plan is `spec/migration-plan.md`. Phase P0 provides only the YAML loader; the agent,
//! environment and model arrive in later phases. Loading the library has no side effects.

pub mod yaml;
