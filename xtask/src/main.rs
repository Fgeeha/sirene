#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented
    )
)]
use std::io::Write;
use std::process::ExitCode;

fn main() -> ExitCode {
    match xtask::run(std::env::args_os(), &mut std::io::stdout()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            let _ = writeln!(std::io::stderr().lock(), "{}", e.report().trim_end());
            ExitCode::FAILURE
        }
    }
}
