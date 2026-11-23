You are working in the mini-rust-swe-agent/ directory.

There is a Python reference project in mini-rust-swe-agent/mini-swe-agent/ (the mini-swe-agent code: agents with a linear history, independent actions through subprocess, bash only, no native tool-calling).

Your task:

1. Create a new Rust project in mini-rust-swe-agent/rust-mini-swe-agent/ with `cargo new --bin`.
2. Follow Rust best practices (clear ownership, Result instead of panic, tests, no unnecessary unsafe). Use the /rust-skills skill as a reference.
3. Use the Python code as a living specification. Do not copy blindly; preserve the radical simplicity of the original.
4. Implement in this order:
   - Core traits (Model, Environment, Agent)
   - LocalEnvironment (std::process::Command)
   - DefaultAgent with a linear history and a simple loop
   - Mock model for parity tests
   - Trajectory serialization (serde_json)
   - Minimal CLI
5. Create or update spec/PLAN.md with the detailed plan (an initial version already exists; refine it as needed).
6. Keep README.md up to date, describing the status and the design decisions.

Success criteria:
- The Rust agent produces trajectories structurally equivalent to Python's in deterministic cases (use mocks) and BDD (Behavior-Driven Development: Cucumber).
- The code stays readable for someone who knows the original.
- Zero panics on normal paths.

Do not add tools beyond bash, do not introduce stateful shell sessions, and do not complicate the control flow without an explicit need.