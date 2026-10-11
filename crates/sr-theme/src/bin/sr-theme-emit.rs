//! `sr-theme-emit ts|schema`: prints generated artifacts to stdout.

use std::process::ExitCode;

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("ts") => {
            print!("{}", sr_theme::typescript());
            ExitCode::SUCCESS
        }
        Some("schema") => {
            println!("{}", sr_theme::json_schema());
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("usage: sr-theme-emit <ts|schema>");
            ExitCode::from(2)
        }
    }
}
