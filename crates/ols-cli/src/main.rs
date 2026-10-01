// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `ols` — OLS CAD command-line interface.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "ols",
    version,
    about = "OLS CAD — Open Line System Computer Assisted Design"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Print version and build information.
    Info,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Info) | None => print_info(),
    }
}

fn print_info() {
    println!("OLS CAD {}", env!("CARGO_PKG_VERSION"));
    println!("core    {}", ols_core::VERSION);
    println!("license MPL-2.0");
}
