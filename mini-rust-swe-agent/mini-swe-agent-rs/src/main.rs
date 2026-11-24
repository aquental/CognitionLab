//! `mini-swe-agent-rs` binary. The CLI arrives in phase P7 (spec/migration-plan.md §3.12).

use std::io::Write;
use std::process::ExitCode;

fn main() -> ExitCode {
    // `eprintln!` panics when stderr is closed; a failed diagnostic must not change the exit code.
    let _ = writeln!(
        std::io::stderr(),
        "mini-swe-agent-rs {}: the command-line interface arrives in phase P7",
        env!("CARGO_PKG_VERSION")
    );
    ExitCode::from(2)
}
