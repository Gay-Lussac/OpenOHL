// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl strings`: the string check ([`ohl_clearance::strings`]) by the
//! `[[string_rule]]` tables of a rule file — every string in each weather
//! case of a rule: its load against its own maximum load, its swing
//! against its own and the rule's limits, the suspension strings' uplift;
//! failures first; CSV with `--csv`.

use std::path::PathBuf;

use clap::Args;
use ohl_clearance::rules::RuleSet;
use ohl_clearance::strings::{self, Measure};

use crate::analysis_run::{CaseArgs, ModelArgs, cases, load};

/// `ohl strings`.
#[derive(Args)]
pub struct StringsArgs {
    #[command(flatten)]
    pub model: ModelArgs,
    #[command(flatten)]
    pub cases: CaseArgs,
    /// Rule file (.ohlr) with `[[string_rule]]` tables.
    #[arg(long)]
    pub rules: PathBuf,
    /// Write every row as CSV here.
    #[arg(long)]
    pub csv: Option<PathBuf>,
}

/// Runs `ohl strings`: whether every string is within its limits.
pub fn run(a: &StringsArgs) -> Result<bool, String> {
    let set = RuleSet::load(&a.rules)?;
    if set.string_rules.is_empty() {
        return Err(format!(
            "{}: no [[string_rule]] in the rule file",
            a.rules.display()
        ));
    }
    let (mut line, library) = load(&a.model)?;
    let chosen = cases(&a.cases, &mut line)?;
    let mut solved = Vec::new();
    for case in &chosen {
        if !set.string_rules.iter().any(|r| r.applies(&case.name)) {
            continue;
        }
        let mut shapes = Vec::new();
        for (section, r) in line
            .sections
            .iter()
            .zip(ohl_line::solve::solve_line(&line, &library, case))
        {
            match r {
                Ok(s) => shapes.push(s),
                Err(e) => println!(
                    "  \"{}\": section {} not solved: {e}",
                    case.name, section.id
                ),
            }
        }
        solved.push((case.name.clone(), shapes));
    }
    let views: Vec<(&str, &[ohl_line::solve::SectionShape])> = solved
        .iter()
        .map(|(n, s)| (n.as_str(), s.as_slice()))
        .collect();
    let report = strings::check(&line, &views, &set.string_rules);
    for n in &report.notes {
        println!("  note: {n}");
    }
    println!(
        "{:<20} {:<16} {:<16} {:<8} {:>12} {:>12} {:>8} status",
        "rule", "case", "string", "check", "value", "limit", "usage %"
    );
    for r in &report.rows {
        let (value, limit) = match r.measure {
            Measure::Load => (
                format!("{:.2} kN", r.value / 1000.0),
                format!("{:.2} kN", r.limit / 1000.0),
            ),
            Measure::Uplift => (format!("{:.2} kN", r.value / 1000.0), "hangs".into()),
            _ => (format!("{:.1}°", r.value), format!("{:.1}°", r.limit)),
        };
        println!(
            "{:<20} {:<16} {:<16} {:<8} {:>12} {:>12} {:>8.1} {}",
            set.string_rules[r.rule].name,
            r.case,
            format!("{} {}", r.structure, r.string),
            r.measure.short(),
            value,
            limit,
            100.0 * r.usage,
            if r.pass { "pass" } else { "FAIL" }
        );
    }
    let failed = report.failures().count();
    println!(
        "{} limits × strings × cases checked, {failed} beyond",
        report.rows.len()
    );
    if let Some(path) = &a.csv {
        std::fs::write(path, report.csv(&set.string_rules))
            .map_err(|e| format!("{}: {e}", path.display()))?;
        println!("written: {}", path.display());
    }
    Ok(failed == 0)
}
