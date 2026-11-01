// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl lightning`: the lightning shielding check
//! ([`ohl_electrical::lightning`]) by the `[[lightning_rule]]` tables of a
//! rule file — per phase and span, in each weather case of a rule, the
//! length exposed (shielding angle or rolling sphere, in cross-sections
//! square to the span every `step` m), where it is worst and the margin;
//! CSV with `--csv`, every result and cross-section as JSON with `--json`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize};

use clap::Args;
use ohl_clearance::rules::RuleSet;
use ohl_electrical::lightning;

use crate::analysis_run::{CaseArgs, ModelArgs, cases, load};

/// `ohl lightning`.
#[derive(Args)]
pub struct LightningArgs {
    #[command(flatten)]
    pub model: ModelArgs,
    #[command(flatten)]
    pub cases: CaseArgs,
    /// Rule file (.ohlr) with `[[lightning_rule]]` tables.
    #[arg(long)]
    pub rules: PathBuf,
    /// Terrain file for the ground (default: the project's surface of the
    /// checks, else its point cloud's ground, else straight between the
    /// structures' feet and level across — said).
    #[arg(long, conflicts_with = "surface")]
    pub terrain: Option<PathBuf>,
    /// A terrain surface of the project file for the ground.
    #[arg(long)]
    pub surface: Option<String>,
    /// Write every row as CSV here.
    #[arg(long)]
    pub csv: Option<PathBuf>,
    /// Write the rows and the cross-sections as JSON here.
    #[arg(long)]
    pub json: Option<PathBuf>,
}

/// Runs `ohl lightning`: whether every phase is covered.
pub fn run(a: &LightningArgs) -> Result<bool, String> {
    let set = RuleSet::load(&a.rules)?;
    if set.lightning_rules.is_empty() {
        return Err(format!(
            "{}: no [[lightning_rule]] in the rule file",
            a.rules.display()
        ));
    }
    let (mut line, library) = load(&a.model)?;
    let chosen = cases(&a.cases, &mut line)?;
    let terrain = match (&a.terrain, &a.surface) {
        (Some(path), _) => Some(ohl_terrain::Terrain::load(path).map_err(|e| e.to_string())?),
        (None, Some(name)) => {
            let session = ohl_session::Session::open(&a.model.file).map_err(|e| e.to_string())?;
            let (points, triangles) = session.surface(name).map_err(|e| e.to_string())?;
            Some(ohl_terrain::Terrain::from_mesh(points, triangles).map_err(|e| e.to_string())?)
        }
        (None, None) => crate::electrical_run::project_ground(&a.model.file, &line, 250.0, false)?,
    };
    let height = |x: f64, y: f64| terrain.as_ref().and_then(|t| t.height(x, y));
    let ground: Option<&(dyn Fn(f64, f64) -> Option<f64> + Sync)> =
        terrain.as_ref().map(|_| &height as _);
    // The shielding wires' own cases (2026-11-01) from the same file, after
    // the cases checked.
    let checked = chosen.len();
    let mut cases = chosen;
    for c in ohl_weather::WeatherCases::load(&a.cases.weather)?.cases {
        let wanted = set.lightning_rules.iter().any(|r| {
            r.shield_cases
                .iter()
                .any(|p| ohl_core::pattern::matches(p, &c.name))
        });
        if wanted && !cases.iter().any(|x| x.name == c.name) {
            cases.push(c);
        }
    }
    let solve = |line: &ohl_line::Line, case: &ohl_weather::WeatherCase| -> lightning::Solved {
        ohl_line::solve::solve_line(line, &library, case)
            .into_iter()
            .map(|r| r.map_err(|e| e.to_string()))
            .collect()
    };
    let report = lightning::run(
        &line,
        &library,
        &cases,
        checked,
        &set.lightning_rules,
        ground,
        &solve,
        &AtomicUsize::new(0),
        &AtomicBool::new(false),
    )?;
    for n in &report.notes {
        println!("  note: {n}");
    }
    for r in &set.lightning_rules {
        println!(
            "  rule \"{}\": {}, every {} m",
            r.name,
            r.method.describe(),
            r.step
        );
    }
    println!(
        "{:<20} {:<14} {:<10} {:>4} {:<16} {:>9} {:>10} {:>9} {:>9} status",
        "rule", "case", "section", "span", "phase", "span m", "exposed m", "worst m", "margin"
    );
    for r in &report.rows {
        let rule = &set.lightning_rules[r.rule];
        let margin = match (rule.method, r.margin) {
            (ohl_clearance::lightning::Method::Angle { .. }, Some(m)) => format!("{m:.1}°"),
            (_, Some(m)) if m >= lightning::R_MAX => format!(">{:.0} m", lightning::R_MAX),
            (_, Some(m)) => match r.current {
                Some(i) => format!("{m:.1} m {i:.1} kA"),
                None => format!("{m:.1} m"),
            },
            (_, None) => "-".into(),
        };
        println!(
            "{:<20} {:<14} {:<10} {:>4} {:<16} {:>9.1} {:>10.1} {:>9.1} {:>9} {}",
            rule.name,
            r.case,
            r.section,
            r.span + 1,
            r.phase,
            r.length,
            r.exposed,
            r.worst_along,
            margin,
            if r.passes() { "covered" } else { "EXPOSED" }
        );
    }
    let exposed = report.rows.iter().fold(0.0, |a, r| a + r.exposed);
    let failed = report.rows.iter().filter(|r| !r.passes()).count();
    println!(
        "{} phases × spans × cases × rules checked, {failed} exposed, {exposed:.1} m of phase exposed",
        report.rows.len()
    );
    if let Some(path) = &a.csv {
        std::fs::write(path, lightning::csv(&report.rows, &set.lightning_rules))
            .map_err(|e| format!("{}: {e}", path.display()))?;
        println!("written: {}", path.display());
    }
    if let Some(path) = &a.json {
        let text = serde_json::to_string(&report).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("written: {}", path.display());
    }
    Ok(failed == 0)
}
