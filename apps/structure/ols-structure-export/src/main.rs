// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `ols-structure-export` — a level 3 structure file (`.ohls`) as IFC,
//! SAF or CSV tables.
//!
//! ```bash
//! ols-structure-export examples/synthetic-line/Structures/type-02.ohls \
//!     --format saf -o type-02.xlsx
//! ols-structure-export tower.ohls --format ifc --sections libraries/sections \
//!     --loads tower-loads.csv -o tower.ifc
//! ```
//!
//! A tool of its own (not `ohl structures export`): the command line
//! (`ols-cli`) is a core crate and core crates never depend on the
//! structure application's crates.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use ols_structure_export::Format;
use ols_structure_loads::LoadTable;

/// Exports a level 3 structure (joints, members with sections, supports)
/// with its load cases.
#[derive(Parser)]
#[command(name = "ols-structure-export", version)]
struct Args {
    /// Structure file (.ohls).
    file: PathBuf,
    /// ifc (IFC4: analysis model and members), ifc-members (IFC4, members
    /// only), ifc2x3 (members for Tekla Structures), saf (xlsx), csv
    /// (tables in a folder).
    #[arg(long)]
    format: String,
    /// Section library files (.ohlsec) or folders; the first one giving
    /// an id wins. Members' sections are looked up here first, then in
    /// the file's [[section]] tables.
    #[arg(long, num_args = 1..)]
    sections: Vec<PathBuf>,
    /// A load table (CSV, as the structure application writes it) instead
    /// of the file's own load cases evaluated without line values.
    #[arg(long)]
    loads: Option<PathBuf>,
    /// Output file (a folder for csv).
    #[arg(short, long)]
    output: PathBuf,
    /// Write the time of the export into IFC headers (else 1970-01-01, so
    /// that exports of the same file are identical).
    #[arg(long)]
    timestamp: bool,
}

fn run(args: &Args) -> Result<(), String> {
    let format = Format::parse(&args.format).ok_or_else(|| {
        format!(
            "--format {}: one of {}",
            args.format,
            Format::ALL.map(Format::as_str).join(", ")
        )
    })?;
    let text =
        std::fs::read_to_string(&args.file).map_err(|e| format!("{}: {e}", args.file.display()))?;
    let (library, left_out) = ols_structure_export::load_sections(&args.sections)?;
    for id in left_out {
        eprintln!("note: section library: \"{id}\" given twice, the first one kept");
    }
    let table = match &args.loads {
        Some(p) => Some(
            std::fs::read_to_string(p)
                .map_err(|e| format!("{}: {e}", p.display()))
                .and_then(|t| {
                    LoadTable::from_csv(&t).map_err(|e| format!("{}: {e}", p.display()))
                })?,
        ),
        None => None,
    };
    let s = ols_structure_export::structure(&text, &library, table.as_ref())
        .map_err(|e| format!("{}: {e}", args.file.display()))?;
    let time = if args.timestamp {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64)
    } else {
        0
    };
    let written = ols_structure_export::write(&s, format, &args.output, time)?;
    for n in &s.notes {
        eprintln!("note: {n}");
    }
    println!(
        "{}: {} joints, {} members, {} sections, {} load cases → {}",
        s.id,
        s.nodes.len(),
        s.members.len(),
        s.sections.len(),
        s.load_cases.len(),
        written
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
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
