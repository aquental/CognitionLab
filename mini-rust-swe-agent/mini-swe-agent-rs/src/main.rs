//! `mini-swe-agent-rs` binary. The CLI arrives in phase P7 (spec/migration-plan.md §3.12).

use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!(
        "mini-swe-agent-rs {}: the command-line interface arrives in phase P7",
        env!("CARGO_PKG_VERSION")
    );
    ExitCode::from(2)
}
