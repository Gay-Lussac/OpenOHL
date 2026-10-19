// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `ohl-structure-report` — the structure report of a structure file
//! (`.ohls`): its load cases solved (on an extension if chosen, with the
//! pole check for a pole) as a printable HTML page and the members'
//! results as CSV, as OHL-Structure's **Results › Structure report…**
//! writes them.
//!
//! ```bash
//! ohl-structure-report examples/synthetic-line/Structures/type-02.ohls \
//!     -o type-02-report.html
//! ohl-structure-report tower.ohls --sections libraries/sections \
//!     --loads tower-loads.csv --extension +3 -o tower-report.html --date
//! ```
//!
//! A tool of its own (not a subcommand of `ohl`), as
//! `ohl-structure-export`: the command line (`ohl-cli`) is a core crate
//! and core crates never depend on the structure application's crates.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use ohl_structure_export::report::{FromText, from_text, today};
use ohl_structure_loads::LoadTable;

/// Solves a structure's load cases and writes its report (HTML) and the
/// members' results (CSV).
#[derive(Parser)]
#[command(name = "ohl-structure-report", version)]
struct Args {
    /// Structure file (.ohls).
    file: PathBuf,
    /// Section library files (.ohlsec) or folders; the first one giving
    /// an id wins (default: the `Sections` folder beside the file, if
    /// there is one).
    #[arg(long, num_args = 1..)]
    sections: Vec<PathBuf>,
    /// A load table (CSV, as the structure application writes it) instead
    /// of the file's own load cases evaluated without line values.
    #[arg(long)]
    loads: Option<PathBuf>,
    /// The height extension to stand the structure on (default: as
    /// written).
    #[arg(long)]
    extension: Option<String>,
    /// Solve this case only (default: every case of the table).
    #[arg(long)]
    case: Option<String>,
    /// Report file (.html); the members' CSV is written beside it as
    /// `<stem>.members.csv`.
    #[arg(short, long)]
    output: PathBuf,
    /// Write today's date into the report (else none, so that reports of
    /// the same file are identical).
    #[arg(long)]
    date: bool,
}

fn run(args: &Args) -> Result<(), String> {
    let text =
        std::fs::read_to_string(&args.file).map_err(|e| format!("{}: {e}", args.file.display()))?;
    let dir = args.file.parent();
    let paths: Vec<PathBuf> = if args.sections.is_empty() {
        dir.map(|d| d.join("Sections"))
            .filter(|p| p.exists())
            .into_iter()
            .collect()
    } else {
        args.sections.clone()
    };
    let (library, left_out) = ohl_structure_export::load_sections(&paths)?;
    for id in left_out {
        eprintln!("note: section library: \"{id}\" given twice, the first one kept");
    }
    let sections_from = if paths.is_empty() {
        "none (the structure's own [[section]] tables)".to_owned()
    } else {
        paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let (own, recipes) = ohl_structure_export::own_load_table(&text, &library)
        .map_err(|e| format!("{}: {e}", args.file.display()))?;
    let table = match &args.loads {
        Some(p) => std::fs::read_to_string(p)
            .map_err(|e| format!("{}: {e}", p.display()))
            .and_then(|t| LoadTable::from_csv(&t).map_err(|e| format!("{}: {e}", p.display())))?,
        None => own,
    };
    let date = args.date.then(today);
    let report = from_text(&FromText {
        text: &text,
        file: &args.file.display().to_string(),
        dir,
        sections: &library,
        sections_from: &sections_from,
        table: &table,
        recipes: &recipes,
        extension: args.extension.as_deref(),
        case: args.case.as_deref(),
        incomplete: &[],
        software: &format!("ohl-structure-report {}", env!("CARGO_PKG_VERSION")),
        date: date.as_deref(),
    })
    .map_err(|e| format!("{}: {e}", args.file.display()))?;
    let csv = args.output.with_extension("members.csv");
    let io = |p: &PathBuf, e: std::io::Error| format!("{}: {e}", p.display());
    std::fs::write(&args.output, &report.html).map_err(|e| io(&args.output, e))?;
    std::fs::write(&csv, &report.csv).map_err(|e| io(&csv, e))?;
    for line in &report.summary {
        println!("{line}");
    }
    println!("→ {}, {}", args.output.display(), csv.display());
    Ok(())
}

fn main() -> ExitCode {
    let args = Args::parse();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
