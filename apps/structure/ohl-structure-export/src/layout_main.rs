// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl-structure-layout` — the automatic joint layout of a structure file
//! (`.ohls`) from the command line (O44 L30), and the joints it leaves
//! for a person to check, sorted by why.
//!
//! ```bash
//! # Lay every joint out (with Solve the clashes after the run), write the file.
//! ohl-structure-layout run tower.ohls --sections libraries/sections \
//!     --bolts libraries/bolts --solve-left -o tower-laid.ohls
//! # The joints still not laid out whole, with their reason classes.
//! ohl-structure-layout left tower-laid.ohls
//! ohl-structure-layout left tower-laid.ohls --class leg-off-flange --words
//! ohl-structure-layout left tower-laid.ohls --csv tower-left.csv
//! ```
//!
//! A tool of its own, as `ohl-structure-export`: the command line
//! (`ohl-cli`) is a core crate and core crates never depend on the
//! structure application's crates. `left` reads the file's layout record
//! only (no run, no libraries); the classes are
//! [`ohl_structure_link::left_reasons::LeftReason`].

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use ohl_structure_link::left_reasons::{self, LeftReason};

/// The automatic joint layout from the command line, and the joints it
/// leaves for a person to check.
#[derive(Parser)]
#[command(name = "ohl-structure-layout", version)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Lays the joints of a structure file out and writes the file.
    Run(RunArgs),
    /// Lists the joints a laid-out file's layout record keeps as left for
    /// a hand detail, each with its reason classes, and counts them.
    Left(LeftArgs),
}

#[derive(clap::Args)]
struct RunArgs {
    /// Structure file (.ohls).
    file: PathBuf,
    /// Section library files (.ohlsec) or folders; the first one giving
    /// an id wins.
    #[arg(long, num_args = 1..)]
    sections: Vec<PathBuf>,
    /// Bolt library files (.ohlb) or folders (default: Bolts/ beside the
    /// file).
    #[arg(long, num_args = 1..)]
    bolts: Vec<PathBuf>,
    /// The bolt's diameter, mm, for the ends without bolts of their own.
    #[arg(long, default_value_t = 16.0)]
    bolt_diameter: f64,
    /// After the run, Solve the clashes at every joint left not whole,
    /// applied only where the joint gets better and every joint around is
    /// no worse (every turn and move said, marked).
    #[arg(long)]
    solve_left: bool,
    /// Replace the values given by hand too.
    #[arg(long)]
    replace_hand: bool,
    /// Only these joints (comma-separated; default: every joint).
    #[arg(long, value_delimiter = ',')]
    joints: Vec<String>,
    /// Print the run's notes.
    #[arg(long)]
    notes: bool,
    /// O44 L33: a load table (CSV, as the structure application writes
    /// it) for the forces of the two-plate step, instead of the file's own
    /// load cases.
    #[arg(long)]
    loads: Option<PathBuf>,
    /// O44 L33: no second plate on any end (one plate each, as before).
    #[arg(long)]
    no_two_plates: bool,
    /// The file written.
    #[arg(short, long)]
    output: PathBuf,
}

#[derive(clap::Args)]
struct LeftArgs {
    /// A laid-out structure file (.ohls).
    file: PathBuf,
    /// Only the joints whose main class is this (its key: clash,
    /// leg-off-flange, …).
    #[arg(long)]
    class: Option<String>,
    /// Each end's words too.
    #[arg(long)]
    words: bool,
    /// Also write the list as CSV (joint, laid out, main class, class,
    /// member, words: one row per end).
    #[arg(long)]
    csv: Option<PathBuf>,
}

fn run(a: &RunArgs) -> Result<(), String> {
    let text =
        std::fs::read_to_string(&a.file).map_err(|e| format!("{}: {e}", a.file.display()))?;
    let (library, left_out) = ohl_structure_export::load_sections(&a.sections)?;
    for id in left_out {
        eprintln!("note: section library: \"{id}\" given twice, the first one kept");
    }
    let (bolts, unread) = ohl_structure_export::load_bolts(&a.bolts, Some(&a.file));
    for e in unread {
        eprintln!("note: bolt library: {e}");
    }
    let settings = ohl_structure_link::LayoutSettings {
        bolt: Some(ohl_structure_link::BoltArea::picked(
            None,
            a.bolt_diameter / 1000.0,
            None,
        )),
        exact_plan: true,
        clash_tries: true,
        solve_left: a.solve_left,
        two_plates: !a.no_two_plates,
        ..Default::default()
    };
    let ask = ohl_structure_link::RunAsk {
        library: &library,
        bolts: &bolts,
        settings: &settings,
        preset_fields: Default::default(),
        joints: (!a.joints.is_empty()).then(|| a.joints.iter().cloned().collect()),
        replace_hand: a.replace_hand,
    };
    let table = match &a.loads {
        Some(p) => Some(
            std::fs::read_to_string(p)
                .map_err(|e| format!("{}: {e}", p.display()))
                .and_then(|t| {
                    ohl_structure_loads::LoadTable::from_csv(&t)
                        .map_err(|e| format!("{}: {e}", p.display()))
                })?,
        ),
        None => None,
    };
    let loads = table
        .as_ref()
        .map(|table| ohl_structure_link::two_plates::Loads {
            table,
            recipes: &[],
        });
    let r = ohl_structure_link::lay_out_loads(&text, &ask, loads.as_ref())
        .map_err(|e| format!("{}: {e}", a.file.display()))?;
    std::fs::write(&a.output, &r.text).map_err(|e| format!("{}: {e}", a.output.display()))?;
    if a.notes {
        for n in &r.notes {
            println!("note: {n}");
        }
    }
    let whole = r.laid_out().filter(|o| o.hand.is_empty()).count();
    let left = left_reasons::joints_left_of_run(&r);
    let solved = r.solved.len();
    println!(
        "{}: {} joints asked, {whole} laid out whole, {} left for a person to check{}; {:.0} s → {}",
        a.file.display(),
        r.joints.len(),
        left.len(),
        if a.solve_left {
            format!(" (Solve the clashes after the run solved {solved})")
        } else {
            String::new()
        },
        r.seconds,
        a.output.display()
    );
    Ok(())
}

fn left(a: &LeftArgs) -> Result<(), String> {
    let text =
        std::fs::read_to_string(&a.file).map_err(|e| format!("{}: {e}", a.file.display()))?;
    let only = match &a.class {
        Some(k) => Some(LeftReason::of_key(k).ok_or_else(|| {
            format!(
                "--class {k}: one of {}",
                LeftReason::ALL.map(LeftReason::key).join(", ")
            )
        })?),
        None => None,
    };
    let left =
        left_reasons::joints_left(&text).map_err(|e| format!("{}: {e}", a.file.display()))?;
    let whole =
        left_reasons::joints_whole(&text).map_err(|e| format!("{}: {e}", a.file.display()))?;
    let t = <ohl_structures::StructureType as ohl_structures::LibraryItem>::parse(&a.file, &text)
        .map_err(|e| format!("{}: {}", a.file.display(), e.message))?;
    let listed: BTreeSet<&str> = left.iter().map(|j| j.joint.as_str()).collect();
    let others = t
        .nodes
        .iter()
        .filter(|n| !listed.contains(n.id.as_str()) && !whole.contains(&n.id))
        .count();
    let mut csv = String::from("joint,laid out,main class,class,member,words\n");
    for j in left.iter().filter(|j| only.is_none_or(|c| j.main() == c)) {
        let classes: Vec<&str> = j.classes().into_iter().map(LeftReason::key).collect();
        println!(
            "{}  {}  {}  [{}]  {} end(s)",
            j.joint,
            if j.written {
                "laid out"
            } else {
                "not laid out"
            },
            j.main().key(),
            classes.join(", "),
            j.ends.len()
        );
        for e in &j.ends {
            if a.words {
                println!("    {}: {}", e.reason.key(), e.words);
            }
            let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
            let _ = writeln!(
                csv,
                "{},{},{},{},{},{}",
                q(&j.joint),
                u8::from(j.written),
                j.main().key(),
                e.reason.key(),
                q(&e.member),
                q(&e.words)
            );
        }
    }
    let n = t.nodes.len().max(1);
    let pct = |k: usize| 100.0 * k as f64 / n as f64;
    println!(
        "\n{} joints: {} laid out whole ({:.1} %), {} left for a person to check ({:.1} %), {} \
         not in the layout record (nothing for the layout there, or never laid out)",
        t.nodes.len(),
        whole.len(),
        pct(whole.len()),
        left.len(),
        pct(left.len()),
        others
    );
    println!("\nclass                joints (main)  joints (any)  ends   what to look at");
    for (c, (main, any, ends)) in left_reasons::counts(&left) {
        println!(
            "{:<20} {main:>14}  {any:>12}  {ends:>5}   {}",
            c.key(),
            c.check()
        );
    }
    if let Some(p) = &a.csv {
        std::fs::write(p, csv).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    Ok(())
}

fn main() -> ExitCode {
    let args = Args::parse();
    let r = match &args.command {
        Command::Run(a) => run(a),
        Command::Left(a) => left(a),
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
