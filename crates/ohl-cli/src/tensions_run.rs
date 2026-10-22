// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl tensions`: the wire tension check ([`ohl_clearance::tension`]) by
//! the `[[tension_rule]]` tables of a rule file — every span of every wire
//! in each weather case of a rule: tension, allowed (factor × RTS), usage,
//! pass or fail, the largest usage first; CSV with `--csv`.

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
    let mut solved = Vec::new();
    for case in &chosen {
        if !set.tension_rules.iter().any(|r| r.applies(&case.name)) {
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
    let report = tension::check(&line, &library, &views, &set.tension_rules);
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
            r.case,
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
