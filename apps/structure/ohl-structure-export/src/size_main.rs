// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl-structure-size` — the member sizer of OHL-Structure (roadmap O49
//! stage 1, 2026-11-26; built, in active development, not approved yet)
//! on a structure file (`.ohls`): per member group the lightest section of
//! the section library in its family that passes the structure's code
//! check, the code's limits and the joint-fit limits, the forces solved
//! again until the choice settles, then a full run with the chosen
//! sections. Prints the proposal, one line per group; `-o` writes the
//! structure file with every change (or those of `--accept`) taken.
//!
//! ```bash
//! ohl-structure-size tower.ohls --sections Sections --loads tower-loads.csv
//! ohl-structure-size tower.ohls --target 0.9 --split web -o tower-sized.ohls
//! ```
//!
//! A tool of its own beside `ohl-structure-report`: the command line
//! (`ohl-cli`) is a core crate and never depends on the structure
//! application's crates.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use ohl_structure_link::sizing::{self, Settings, SizingInput};
use ohl_structure_loads::LoadTable;
use ohl_structures::LibraryItem as _;

/// Proposes the lightest passing section for each member group.
#[derive(Parser)]
#[command(name = "ohl-structure-size", version)]
struct Args {
    /// Structure file (.ohls), level 3 (members with sections).
    file: PathBuf,
    /// Section library files (.ohlsec) or folders, the only sections
    /// proposed; the first one giving an id wins (default: the `Sections`
    /// folder beside the file, if there is one).
    #[arg(long, num_args = 1..)]
    sections: Vec<PathBuf>,
    /// A load table (CSV, as the structure application writes it) instead
    /// of the file's own load cases evaluated without line values.
    #[arg(long)]
    loads: Option<PathBuf>,
    /// Size for this case only (default: every case of the table).
    #[arg(long)]
    case: Option<String>,
    /// The largest usage a chosen section may have (1: its capacity).
    #[arg(long, default_value_t = 1.0)]
    target: f64,
    /// Leave the joint-fit limits out (the leg taking its bolt, the length
    /// for the bolts): a chosen member may then not be connectable.
    #[arg(long)]
    no_joint_fit: bool,
    /// The most rounds of forces and choice (at least 2: one round cannot
    /// show a choice holds with its own forces).
    #[arg(long, default_value_t = sizing::DEFAULT_ROUNDS)]
    rounds: usize,
    /// Split this group: each member (with its mirror images) sized alone
    /// (repeatable).
    #[arg(long)]
    split: Vec<String>,
    /// Leave this group as it is (repeatable).
    #[arg(long)]
    keep: Vec<String>,
    /// Print each group's joint-fit problems before and after, in words.
    #[arg(long)]
    problems: bool,
    /// Write the structure file with the changes taken here.
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// With -o: take only these groups' changes (repeatable; default every
    /// change).
    #[arg(long, requires = "output")]
    accept: Vec<String>,
}

fn run(args: &Args) -> Result<bool, String> {
    let file = &args.file;
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
    let (own, recipes) = ohl_structure_export::own_load_table(&text, &library)
        .map_err(|e| format!("{}: {e}", file.display()))?;
    let table = match &args.loads {
        Some(p) => std::fs::read_to_string(p)
            .map_err(|e| format!("{}: {e}", p.display()))
            .and_then(|t| LoadTable::from_csv(&t).map_err(|e| format!("{}: {e}", p.display())))?,
        None => own,
    };
    let cases = ohl_structure_link::cases_to_run(&table, &recipes, args.case.as_deref());
    if cases.is_empty() {
        return Err("no case in the load table".into());
    }
    let structure = ohl_structures::StructureType::parse(file, &text).map_err(|e| e.message)?;
    let settings = Settings {
        target: args.target,
        joint_fit: !args.no_joint_fit,
        rounds: args.rounds,
        split: args.split.clone(),
        keep: args.keep.clone(),
    };
    let s = sizing::size_with(
        &SizingInput {
            structure: &structure,
            text: &text,
            dir,
            sections: &library,
            table: &table,
            recipes: &recipes,
            cases: &cases,
            settings: &settings,
        },
        &ohl_core::budget::Budget::new(),
        &|step| eprintln!("… {step}"),
    )?;
    for line in sizing::summary(&s) {
        println!("{line}");
    }
    if args.problems {
        for g in &s.groups {
            for (when, list) in ["before", "after"].iter().zip(&g.problems) {
                for p in list {
                    println!("joint fit {when}: {} · {p}", g.group.name);
                }
            }
        }
    }
    if let Some(out) = &args.output {
        let accepted: Vec<String> = if args.accept.is_empty() {
            s.groups.iter().map(|g| g.group.name.clone()).collect()
        } else {
            args.accept.clone()
        };
        let sized = sizing::apply(&text, &s, &accepted)?;
        std::fs::write(out, sized).map_err(|e| format!("{}: {e}", out.display()))?;
        println!("→ {}", out.display());
    }
    Ok(s.over_target.is_empty())
}

fn main() -> ExitCode {
    let args = Args::parse();
    match run(&args) {
        Ok(true) => ExitCode::SUCCESS,
        // A member above the target in the final run: said, exit 2.
        Ok(false) => ExitCode::from(2),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
