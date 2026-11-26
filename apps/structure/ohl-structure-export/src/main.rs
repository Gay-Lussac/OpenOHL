// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl-structure-export` — a level 3 structure file (`.ohls`) as IFC,
//! SAF or CSV tables.
//!
//! ```bash
//! ohl-structure-export examples/synthetic-line/Structures/type-02.ohls \
//!     --format saf -o type-02.xlsx
//! ohl-structure-export tower.ohls --format ifc --sections libraries/sections \
//!     --bolts libraries/bolts --loads tower-loads.csv -o tower.ifc
//! ohl-structure-export tower.ohls --format joints-left -o left.csv
//! ohl-structure-export tower.ohls --format joints-left-text -o -
//! ```
//!
//! `joints-left` (CSV) and `joints-left-text` are the joints the automatic
//! joint layout did not lay out whole, with why and their review ticks —
//! OHL-Structure's *Review the joints left* list
//! (`ohl_structure_link::joint_review`); `-o -` prints to the standard
//! output. They read the file only (no libraries, nothing laid out).
//!
//! A tool of its own (not `ohl structures export`): the command line
//! (`ohl-cli`) is a core crate and core crates never depend on the
//! structure application's crates.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use ohl_structure_export::Format;
use ohl_structure_loads::LoadTable;

/// Exports a level 3 structure (joints, members with sections, supports)
/// with its load cases.
#[derive(Parser)]
#[command(name = "ohl-structure-export", version)]
struct Args {
    /// Structure file (.ohls).
    file: PathBuf,
    /// ifc (IFC4: analysis model and members), ifc-members (IFC4, members
    /// only), ifc2x3 (members for steel-detailing programs), saf (xlsx), csv
    /// (tables in a folder), quantities (the bill of materials, xlsx);
    /// joints-left (CSV) and joints-left-text: the joints the automatic
    /// joint layout did not lay out whole, with why and their review ticks.
    #[arg(long)]
    format: String,
    /// Section library files (.ohlsec) or folders; the first one giving
    /// an id wins. Members' sections are looked up here first, then in
    /// the file's [[section]] tables.
    #[arg(long, num_args = 1..)]
    sections: Vec<PathBuf>,
    /// Bolt library files (.ohlb) or folders, for the bolts a connection
    /// names (diameter, head and nut in IFC); default: Bolts/ beside the
    /// file.
    #[arg(long, num_args = 1..)]
    bolts: Vec<PathBuf>,
    /// A load table (CSV, as the structure application writes it) instead
    /// of the file's own load cases evaluated without line values.
    #[arg(long)]
    loads: Option<PathBuf>,
    /// Output file (a folder for csv; `-` for joints-left and
    /// joints-left-text: the standard output).
    #[arg(short, long)]
    output: PathBuf,
    /// Write the time of the export into IFC headers (else 1970-01-01, so
    /// that exports of the same file are identical).
    #[arg(long)]
    timestamp: bool,
}

/// The formats of the joints left (O44 L31), CSV or text.
const JOINTS_LEFT: [(&str, bool); 2] = [("joints-left", true), ("joints-left-text", false)];

/// The joints left of the file, as CSV or text, into `args.output` (`-`:
/// the standard output).
fn joints_left(args: &Args, csv: bool) -> Result<(), String> {
    let text =
        std::fs::read_to_string(&args.file).map_err(|e| format!("{}: {e}", args.file.display()))?;
    let list = ohl_structure_link::joint_review::review_list(&text)
        .map_err(|e| format!("{}: {e}", args.file.display()))?;
    let out = if csv { list.csv() } else { list.text() };
    if args.output.as_os_str() == "-" {
        print!("{out}");
    } else {
        std::fs::write(&args.output, out).map_err(|e| format!("{}: {e}", args.output.display()))?;
        println!("{} → {}", list.summary(), args.output.display());
    }
    Ok(())
}

fn run(args: &Args) -> Result<(), String> {
    if let Some((_, csv)) = JOINTS_LEFT.iter().find(|(n, _)| *n == args.format) {
        return joints_left(args, *csv);
    }
    let format = Format::parse(&args.format).ok_or_else(|| {
        format!(
            "--format {}: one of {}, {}",
            args.format,
            Format::ALL.map(Format::as_str).join(", "),
            JOINTS_LEFT.map(|f| f.0).join(", ")
        )
    })?;
    let text =
        std::fs::read_to_string(&args.file).map_err(|e| format!("{}: {e}", args.file.display()))?;
    let (library, left_out) = ohl_structure_export::load_sections(&args.sections)?;
    for id in left_out {
        eprintln!("note: section library: \"{id}\" given twice, the first one kept");
    }
    let (bolts, unread) = ohl_structure_export::load_bolts(&args.bolts, Some(&args.file));
    for e in unread {
        eprintln!("note: bolt library: {e}");
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
    let s = ohl_structure_export::structure(&text, &library, &bolts, table.as_ref())
        .map_err(|e| format!("{}: {e}", args.file.display()))?;
    let time = if args.timestamp {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64)
    } else {
        0
    };
    let written = ohl_structure_export::write(&s, format, &args.output, time)?;
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
