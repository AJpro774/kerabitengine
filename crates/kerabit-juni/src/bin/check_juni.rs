//! CLI: compile-check a `.juni` game script against the Kerabit prelude.
//!
//! ```bash
//! cargo run -p kerabit-juni --bin check_juni -- path/to/script.juni
//! ```
//!
//! Prints `file:line:col: error: message` per diagnostic and exits non-zero on
//! errors, so editors and the MCP server can surface them directly.

use std::env;
use std::process::ExitCode;

use kerabit_juni::ScriptRuntime;

fn main() -> ExitCode {
    let path = match env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("usage: check_juni <file.juni>");
            return ExitCode::from(2);
        }
    };
    let source = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("check_juni: read {path}: {err}");
            return ExitCode::FAILURE;
        }
    };
    let diagnostics = ScriptRuntime::check_source_diagnostics(&source, Some(&path));
    let mut failed = false;
    for d in &diagnostics {
        if d.severity == kerabit_juni::Severity::Error {
            failed = true;
        }
        eprintln!("{}", d.format(&path));
    }
    if failed {
        ExitCode::FAILURE
    } else {
        println!("ok: {path}");
        ExitCode::SUCCESS
    }
}
