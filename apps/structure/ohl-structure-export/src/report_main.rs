// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

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
//! A tool of its own, as `ohl-structure-export`: the command line
//! (`ohl-cli`) is a core crate and core crates never depend on the
//! structure application's crates. `ohl structure analyse|report` starts
//! this tool with `--request -` and talks to it in JSON
//! ([`ohl_structure_export::request`]): the request on its input, the
//! reply on its output.
//!
//! ```bash
//! echo '{"format": "ohl-structure-request", "version": 1, "action": "analyse",
//!        "structure": "tower.ohls"}' | ohl-structure-report --request -
//! ```

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
    #[arg(required_unless_present = "request")]
    file: Option<PathBuf>,
    /// Answer a JSON request (a file, or `-` for the input) with a JSON
    /// reply on the output, instead of the other arguments
    /// (`ohl-structure-request`, version 1; how `ohl structure` asks).
    #[arg(long, conflicts_with_all = ["file", "output"])]
    request: Option<PathBuf>,
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
    #[arg(short, long, required_unless_present = "request")]
    output: Option<PathBuf>,
    /// Write today's date into the report (else none, so that reports of
    /// the same file are identical).
    #[arg(long)]
    date: bool,
}

/// The tool and its version, as the reports and replies name it.
fn software() -> String {
    format!("ohl-structure-report {}", env!("CARGO_PKG_VERSION"))
}

/// `--request`: the reply on the output; success when it went through.
fn answer(from: &std::path::Path) -> ExitCode {
    use std::io::Read as _;
    let text = if from.as_os_str() == "-" {
        let mut s = String::new();
        std::io::stdin()
            .read_to_string(&mut s)
            .map(|_| s)
            .map_err(|e| format!("request: {e}"))
    } else {
        std::fs::read_to_string(from).map_err(|e| format!("{}: {e}", from.display()))
    };
    let reply = match text {
        Ok(t) => ohl_structure_export::request::answer(&t, &software()),
        Err(e) => ohl_structure_export::request::Reply::failed(&software(), e),
    };
    println!("{}", reply.to_json());
    if reply.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn run(args: &Args) -> Result<(), String> {
    let (Some(file), Some(output)) = (&args.file, &args.output) else {
        return Err("give the structure file and -o (or --request)".into());
    };
    let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let dir = file.parent();
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
        .map_err(|e| format!("{}: {e}", file.display()))?;
    let table = match &args.loads {
        Some(p) => std::fs::read_to_string(p)
            .map_err(|e| format!("{}: {e}", p.display()))
            .and_then(|t| LoadTable::from_csv(&t).map_err(|e| format!("{}: {e}", p.display())))?,
        None => own,
    };
    let date = args.date.then(today);
    let software = software();
    let report = from_text(&FromText {
        text: &text,
        file: &file.display().to_string(),
        dir,
        sections: &library,
        sections_from: &sections_from,
        table: &table,
        recipes: &recipes,
        extension: args.extension.as_deref(),
        case: args.case.as_deref(),
        incomplete: &[],
        software: &software,
        date: date.as_deref(),
    })
    .map_err(|e| format!("{}: {e}", file.display()))?;
    let csv = output.with_extension("members.csv");
    let io = |p: &PathBuf, e: std::io::Error| format!("{}: {e}", p.display());
    std::fs::write(output, &report.html).map_err(|e| io(output, e))?;
    std::fs::write(&csv, &report.csv).map_err(|e| io(&csv, e))?;
    for line in &report.summary {
        println!("{line}");
    }
    println!("→ {}, {}", output.display(), csv.display());
    Ok(())
}

fn main() -> ExitCode {
    let args = Args::parse();
    if let Some(from) = &args.request {
        return answer(from);
    }
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
