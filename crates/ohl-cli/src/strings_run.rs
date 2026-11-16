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

use crate::analysis_run::{CaseArgs, ModelArgs, cases, check_level, load_with_structures};

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
    let mut l = load_with_structures(&a.model)?;
    let chosen = cases(&a.cases, &mut l.line)?;
    // The line's calculation level (or --structures-as): at level 3 each
    // case solved rigid and flexible, the worse kept.
    let level = check_level(&l, &chosen)?;
    let (solved, tally) = crate::analysis_run::solve_for_rules(
        &l.line,
        &l.library,
        &chosen,
        &set.string_rules,
        |r, c| r.applies(c),
        |r| r.calculation,
        &level,
    );
    let (line, library) = (&l.line, &l.library);
    // One solve's checks (`flexible`: the level-3 solves'), the fixed
    // posts' loads in the same cases with them (their ratings).
    let check = |flexible: bool| {
        let shapes: Vec<(
            &str,
            ohl_clearance::calculation::Calc,
            &[ohl_line::solve::SectionShape],
        )> = solved
            .iter()
            .filter_map(|s| {
                let shapes = if flexible {
                    s.flexible.as_deref()?
                } else {
                    s.rigid.as_slice()
                };
                Some((s.case.as_str(), s.calc, shapes))
            })
            .collect();
        let posts: Vec<(
            &str,
            ohl_clearance::calculation::Calc,
            Vec<ohl_line::post::PostLoad>,
        )> = shapes
            .iter()
            .filter_map(|(n, c, s)| {
                let case = chosen.iter().find(|w| &w.name == n)?;
                let l = ohl_clearance::calculation::line_for(line, *c);
                Some((*n, *c, ohl_line::post::post_loads(&l, library, case, s)))
            })
            .collect();
        let post_views: Vec<strings::PostCase> = posts
            .iter()
            .map(|(n, c, p)| (*n, *c, p.as_slice()))
            .collect();
        strings::check(
            line,
            &shapes,
            &post_views,
            &set.string_rules,
            &Default::default(),
        )
    };
    let mut report = check(false);
    let mut change = None;
    if level.setup.is_some() {
        (report, change) = strings::envelope(report, check(true));
    }
    report.notes.splice(0..0, level.notes(&tally, change));
    for n in &report.notes {
        println!("  note: {n}");
    }
    println!(
        "{:<20} {:<16} {:<16} {:<8} {:>12} {:>12} {:>8} status",
        "rule", "case", "string", "check", "value", "limit", "usage %"
    );
    for r in &report.rows {
        let (value, limit) = match r.measure {
            Measure::Load | Measure::Cantilever | Measure::Compression => (
                format!("{:.2} kN", r.value / 1000.0),
                format!("{:.2} kN", r.limit / 1000.0),
            ),
            Measure::Uplift => (format!("{:.2} kN", r.value / 1000.0), "hangs".into()),
            _ => (format!("{:.1}°", r.value), format!("{:.1}°", r.limit)),
        };
        // At level 3: both solves' values, the worse checked.
        let both = r.levels.map_or(String::new(), |[a, b]| {
            if r.measure.is_force() {
                format!("  (rigid {:.2} / level 3 {:.2} kN)", a / 1000.0, b / 1000.0)
            } else {
                format!("  (rigid {a:.1}° / level 3 {b:.1}°)")
            }
        });
        println!(
            "{:<20} {:<16} {:<16} {:<8} {:>12} {:>12} {:>8.1} {}{both}",
            set.string_rules[r.rule].name,
            ohl_clearance::calculation::label(&r.case, r.calculation),
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
