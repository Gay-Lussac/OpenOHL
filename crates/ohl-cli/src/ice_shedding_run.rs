// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl ice-shedding`: the conductor jump when the ice drops
//! ([`ohl_clearance::ice_shedding`], roadmap O13, the simplified method)
//! by the `[[ice_shedding_rule]]` tables of a rule file — per iced case of
//! a rule, every span of every wire it names jumps by the rule's rebound
//! factor × its static rise, against each other wire of its span: the
//! clearance before and after, the required, pass or fail, the smallest
//! margin first; CSV with `--csv`.

use std::path::PathBuf;

use clap::Args;
use ohl_clearance::ice_shedding;
use ohl_clearance::rules::RuleSet;

use crate::analysis_run::{CaseArgs, ModelArgs, cases, load};

/// `ohl ice-shedding`.
#[derive(Args)]
pub struct IceSheddingArgs {
    #[command(flatten)]
    pub model: ModelArgs,
    #[command(flatten)]
    pub cases: CaseArgs,
    /// Rule file (.ohlr) with `[[ice_shedding_rule]]` tables.
    #[arg(long)]
    pub rules: PathBuf,
    /// Write every row as CSV here.
    #[arg(long)]
    pub csv: Option<PathBuf>,
}

/// Runs `ohl ice-shedding`: whether every jumped wire keeps its clearance.
pub fn run(a: &IceSheddingArgs) -> Result<bool, String> {
    let set = RuleSet::load(&a.rules)?;
    if set.ice_shedding_rules.is_empty() {
        return Err(format!(
            "{}: no [[ice_shedding_rule]] in the rule file",
            a.rules.display()
        ));
    }
    let (mut line, library) = load(&a.model)?;
    let chosen = cases(&a.cases, &mut line)?;
    // The bare cases the rules name, from the same file, after the cases
    // checked.
    let checked = chosen.len();
    let mut all = chosen;
    for c in ohl_weather::WeatherCases::load(&a.cases.weather)?.cases {
        if !all.iter().any(|x| x.name == c.name) {
            all.push(c);
        }
    }
    let rules = &set.ice_shedding_rules;
    let (plan, notes) = ice_shedding::solves(rules, &all, checked);
    for n in &notes {
        println!("  note: {n}");
    }
    let shapes: Vec<Vec<ohl_line::solve::SectionShape>> = plan
        .iter()
        .map(|(case, calc)| {
            let l = ohl_clearance::calculation::line_for(&line, *calc);
            let mut shapes = Vec::new();
            for (section, r) in l
                .sections
                .iter()
                .zip(ohl_line::solve::solve_line(&l, &library, case))
            {
                match r {
                    Ok(s) => shapes.push(s),
                    Err(e) => println!(
                        "  \"{}\": section {} not solved: {e}",
                        ohl_clearance::calculation::label(&case.name, *calc),
                        section.id
                    ),
                }
            }
            shapes
        })
        .collect();
    let solved: Vec<ice_shedding::Solved<'_>> = plan
        .iter()
        .zip(&shapes)
        .map(|((c, k), s)| (c, *k, s.as_slice()))
        .collect();
    // Exit clause: a few seconds for a long line.
    let budget = ohl_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(600));
    let report = ice_shedding::check(
        &line,
        &library,
        rules,
        &all,
        checked,
        &solved,
        &Default::default(),
        None,
        &budget,
    )?;
    for n in &report.notes {
        println!("  note: {n}");
    }
    println!(
        "{:<24} {:<16} {:<8} {:>4} {:<8} {:>7} {:>7} {:>8} {:>8} {:>9} status",
        "rule",
        "case",
        "section",
        "span",
        "other",
        "rise m",
        "jump m",
        "before m",
        "after m",
        "required"
    );
    for r in &report.rows {
        println!(
            "{:<24} {:<16} {:<8} {:>4} {:<8} {:>7.2} {:>7.2} {:>8.2} {:>8.2} {:>9.2} {}",
            rules[r.rule].name,
            ohl_clearance::calculation::label(&r.case, r.calculation),
            r.section,
            r.span + 1,
            r.other,
            r.rise,
            r.jump,
            r.before,
            r.after,
            r.required,
            if r.passes() { "pass" } else { "FAIL" }
        );
    }
    let failed = report.failures().count();
    println!(
        "{} pairs × cases × rules checked, {failed} below the required clearance after the jump",
        report.rows.len()
    );
    if let Some(path) = &a.csv {
        std::fs::write(path, report.csv(rules)).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("written: {}", path.display());
    }
    Ok(failed == 0)
}
