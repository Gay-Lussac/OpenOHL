// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl tensions`: the wire tension check ([`ohl_clearance::tension`]) by
//! the `[[tension_rule]]` tables of a rule file — every span of every wire
//! in each weather case of a rule: tension, allowed (factor × RTS, or the
//! rule's tension or catenary constant), usage, pass or fail, the largest
//! usage first; CSV with `--csv`.
//!
//! `--sag-to-limits` (roadmap O22 item 3): first finds, for each section
//! in the blocks, the largest stringing tension the rules allow
//! ([`ohl_clearance::tension::sag_to_limits`]), then checks the line so
//! sagged; `--write` writes the stringings found back to the line or
//! project file.

use std::path::PathBuf;

use clap::Args;
use ohl_clearance::rules::RuleSet;
use ohl_clearance::tension;

use crate::analysis_run::{CaseArgs, ModelArgs, cases, load};

/// `ohl tensions`.
#[derive(Args)]
pub struct TensionsArgs {
    #[command(flatten)]
    pub model: ModelArgs,
    #[command(flatten)]
    pub cases: CaseArgs,
    /// Rule file (.ohlr) with `[[tension_rule]]` tables.
    #[arg(long)]
    pub rules: PathBuf,
    /// Write every row as CSV here.
    #[arg(long)]
    pub csv: Option<PathBuf>,
    /// Sag to the limits before checking: give each section in the blocks
    /// the largest stringing tension (at its stringing temperature and
    /// condition) that keeps every tension rule — the tightest line the
    /// rules allow: the least sag and the highest structure loads, so
    /// check clearances and structures after it. About 40 trial solves
    /// of a section per weather case of the rules (seconds per section).
    /// Nothing is written without --write.
    #[arg(long)]
    pub sag_to_limits: bool,
    /// With --sag-to-limits: only these sections (ids, comma separated;
    /// default: every section in the blocks). The others stay as they are.
    #[arg(long, value_delimiter = ',', requires = "sag_to_limits")]
    pub sections: Vec<String>,
    /// With --sag-to-limits: write the stringings found back to the file
    /// (a line file is rewritten; in a project each is one logged,
    /// undoable change of `--user`). Without it the file is not touched
    /// and the stringings are printed.
    #[arg(long, requires = "sag_to_limits")]
    pub write: bool,
    /// With --write into a project: the user the changes are logged as.
    #[arg(long, default_value = "ohl tensions", requires = "write")]
    pub user: String,
    /// With --sag-to-limits: seconds a section may take; when reached the
    /// tension found so far is given (within the rules, not the tightest)
    /// and said.
    #[arg(long, default_value_t = tension::SAG_TO_LIMITS_SECONDS, requires = "sag_to_limits")]
    pub seconds: u64,
}

/// `--sag-to-limits`: sags the sections of `line` to the rules' limits,
/// prints what was found, writes it back with `--write`; the line so
/// sagged (to be checked).
fn sag_to_limits(
    a: &TensionsArgs,
    line: &ohl_line::Line,
    library: &ohl_cables::Library,
    rules: &[tension::TensionRule],
    cases: &[ohl_weather::WeatherCase],
) -> Result<ohl_line::Line, String> {
    let budget = ohl_core::budget::Budget::new()
        .time_limit(std::time::Duration::from_secs(a.seconds.max(1)));
    let sagged = tension::sag_to_limits(
        line,
        library,
        rules,
        cases,
        &a.sections,
        &Default::default(),
        &budget,
    );
    println!("Sag to limits:");
    for n in &sagged.notes {
        println!("  note: {n}");
    }
    for s in &sagged.sections {
        let Some(ohl_line::Stringing::Tension {
            horizontal_tension,
            temperature,
            condition,
        }) = &s.stringing
        else {
            println!("  {:<12} {}", s.section, s.notes.join("; "));
            continue;
        };
        let governs = s.governing.as_ref().map_or(String::new(), |g| {
            format!(
                "governed by \"{}\" in {}, span {} ({:.2} %)",
                rules[g.rule].name,
                ohl_clearance::calculation::label(&g.case, g.calculation),
                g.span + 1,
                100.0 * g.usage
            )
        });
        println!(
            "  {:<12} {:.3} kN {} at {} °C — {governs}; {} trials",
            s.section,
            horizontal_tension / 1000.0,
            condition.label(),
            temperature,
            s.trials
        );
        for (rule, usage) in &s.usages {
            println!("    {:<24} {:>7.2} %", rules[*rule].name, 100.0 * usage);
        }
        for n in &s.notes {
            println!("    note: {n}");
        }
    }
    // The initial tensions of the values that hold after creep or load.
    for n in ohl_line::solve::final_stringing_notes(
        &sagged.line,
        library,
        &ohl_wirefe::default_fe_budget(),
    ) {
        println!("  stringing: {n}");
    }
    let found: Vec<(&str, &ohl_line::Stringing)> = sagged.stringings().collect();
    if a.write {
        write_back(a, &found)?;
    } else if !found.is_empty() {
        println!("  not written (give --write); the stringings found:");
        for (id, s) in &found {
            println!(
                "    {id}: {}",
                serde_json::to_string(s).map_err(|e| e.to_string())?
            );
        }
    }
    Ok(sagged.line.clone())
}

/// Writes the stringings found back: a line file is read again as stored
/// (its structure types unresolved), changed and rewritten; a project
/// takes one logged change per section.
fn write_back(a: &TensionsArgs, found: &[(&str, &ohl_line::Stringing)]) -> Result<(), String> {
    let file = &a.model.file;
    if found.is_empty() {
        println!("  nothing to write");
        return Ok(());
    }
    if crate::project::is_project(file) {
        let mut session = ohl_session::Session::open(file).map_err(|e| e.to_string())?;
        for (id, s) in found {
            session
                .apply(
                    &a.user,
                    ohl_session::Command::SetStringing {
                        section: (*id).to_owned(),
                        stringing: (*s).clone(),
                    },
                )
                .map_err(|e| format!("section {id}: {e}"))?;
        }
    } else {
        let mut stored = ohl_line::Line::load(file).map_err(|e| e.to_string())?;
        for (id, s) in found {
            let section = stored
                .sections
                .iter_mut()
                .find(|x| x.id == *id)
                .ok_or_else(|| format!("no section {id} in {}", file.display()))?;
            section.stringing = (*s).clone();
        }
        std::fs::write(file, stored.to_json()).map_err(|e| format!("{}: {e}", file.display()))?;
    }
    println!(
        "  written: {} stringing{} in {}",
        found.len(),
        if found.len() == 1 { "" } else { "s" },
        file.display()
    );
    Ok(())
}

/// Runs `ohl tensions`: whether every span is within its allowed tension.
pub fn run(a: &TensionsArgs) -> Result<bool, String> {
    let set = RuleSet::load(&a.rules)?;
    if set.tension_rules.is_empty() {
        return Err(format!(
            "{}: no [[tension_rule]] in the rule file",
            a.rules.display()
        ));
    }
    let (mut line, library) = load(&a.model)?;
    let chosen = cases(&a.cases, &mut line)?;
    if a.sag_to_limits {
        line = sag_to_limits(a, &line, &library, &set.tension_rules, &chosen)?;
    }
    let solved = crate::analysis_run::solve_for_rules(
        &line,
        &library,
        &chosen,
        &set.tension_rules,
        |r, c| r.applies(c),
        |r| r.calculation,
    );
    let views: Vec<(
        &str,
        ohl_clearance::calculation::Calc,
        &[ohl_line::solve::SectionShape],
    )> = solved
        .iter()
        .map(|(n, c, s)| (n.as_str(), *c, s.as_slice()))
        .collect();
    let report = tension::check(
        &line,
        &library,
        &views,
        &set.tension_rules,
        &Default::default(),
    );
    for n in &report.notes {
        println!("  note: {n}");
    }
    println!(
        "{:<24} {:<16} {:<12} {:>4} {:>11} {:>11} {:>8} status",
        "rule", "case", "section", "span", "tension kN", "allowed kN", "usage %"
    );
    for r in &report.rows {
        println!(
            "{:<24} {:<16} {:<12} {:>4} {:>11.2} {:>11.2} {:>8.1} {}",
            set.tension_rules[r.rule].name,
            ohl_clearance::calculation::label(&r.case, r.calculation),
            r.section,
            r.span + 1,
            r.tension / 1000.0,
            r.allowed / 1000.0,
            100.0 * r.usage,
            if r.passes() { "pass" } else { "FAIL" }
        );
    }
    let failed = report.failures().count();
    println!(
        "{} spans × cases × rules checked, {failed} above the allowed tension",
        report.rows.len()
    );
    if let Some(path) = &a.csv {
        std::fs::write(path, report.csv(&set.tension_rules))
            .map_err(|e| format!("{}: {e}", path.display()))?;
        println!("written: {}", path.display());
    }
    Ok(failed == 0)
}
