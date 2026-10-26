// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl internal`: the internal clearances ([`ohl_clearance::internal`])
//! by the `[[internal_rule]]` tables of a rule file — at every structure
//! with a body (levels 2 and 3), the conductors near it, its jumpers, its
//! strings' wire points and the electrified volumes on its strings (the
//! insulator library's shapes, the strings' own) as solved in each weather
//! case of a rule, against every member of the body and the grounded
//! volumes; the worst first; CSV with `--csv`.

use std::path::PathBuf;

use clap::Args;
use ohl_clearance::internal;
use ohl_clearance::rules::RuleSet;

use crate::analysis_run::{CaseArgs, ModelArgs, cases};

/// `ohl internal`.
#[derive(Args)]
pub struct InternalArgs {
    #[command(flatten)]
    pub model: ModelArgs,
    #[command(flatten)]
    pub cases: CaseArgs,
    /// Rule file (.ohlr) with `[[internal_rule]]` tables.
    #[arg(long)]
    pub rules: PathBuf,
    /// Cross-section library (the members' profiles: their surfaces;
    /// default: the project's, else the `Sections` folder next to the
    /// line, else the shipped one).
    #[arg(long)]
    pub sections: Option<PathBuf>,
    /// Write every row as CSV here.
    #[arg(long)]
    pub csv: Option<PathBuf>,
    /// Stop after this many seconds (the exit clause).
    #[arg(long, default_value_t = 600)]
    pub time_limit: u64,
}

/// Runs `ohl internal`: whether every part keeps its distance.
pub fn run(a: &InternalArgs) -> Result<bool, String> {
    let set = RuleSet::load(&a.rules)?;
    if set.internal_rules.is_empty() {
        return Err(format!(
            "{}: no [[internal_rule]] in the rule file",
            a.rules.display()
        ));
    }
    let libs = crate::project::Libraries::resolve(
        &a.model.file,
        crate::project::GivenLibraries {
            cables: a.model.library.as_deref(),
            structures: a.model.structures.as_deref(),
            insulators: a.model.insulators.as_deref(),
            sections: a.sections.as_deref(),
        },
    )?;
    let mut line = crate::project::load_line(&a.model.file)?;
    let mut bodies = ohl_structures_fe::drawn::Bodies::default();
    // The insulator library: the volumes on the strings.
    let insulators =
        ohl_structures::InsulatorLibrary::load_dir(&libs.insulators).map_err(|e| e.to_string())?;
    if line.structures.iter().any(|s| s.structure_type.is_some()) {
        let structures = ohl_structures::StructureLibrary::load_dir(&libs.structures)
            .map_err(|e| e.to_string())?;
        line = line
            .resolve(&structures, &insulators)
            .map_err(|e| e.to_string())?;
        let sections = match libs
            .sections
            .clone()
            .or_else(|| Some(ohl_core::files::shipped_library("sections")).filter(|p| p.is_dir()))
        {
            Some(p) => ohl_sections::SectionLibrary::load(&p).unwrap_or_else(|e| {
                println!(
                    "  note: cross-section library not read ({e}): the members to their centre lines"
                );
                Default::default()
            }),
            None => Default::default(),
        };
        bodies = ohl_structures_fe::drawn::Bodies::load(&structures, &sections);
    }
    let library = ohl_cables::Library::load(&libs.cables).map_err(|e| e.to_string())?;
    let chosen = cases(&a.cases, &mut line)?;
    let mut solved = Vec::new();
    for case in &chosen {
        if !set.internal_rules.iter().any(|r| r.applies(&case.name)) {
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
        let mut jumpers = Vec::new();
        for (j, r) in line.jumpers.iter().zip(ohl_line::jumper::solve_jumpers(
            &line, &library, case, &shapes,
        )) {
            match r {
                Ok(s) => jumpers.push(s),
                Err(e) => println!("  \"{}\": jumper {} not solved: {e}", case.name, j.id),
            }
        }
        solved.push((case, shapes, jumpers));
    }
    let views: Vec<internal::SolvedCase> = solved
        .iter()
        .map(|(c, s, j)| internal::SolvedCase {
            name: &c.name,
            wind: c.wind_pressure.si() > 0.0,
            sections: s,
            jumpers: j,
        })
        .collect();
    let placed = internal::structure_bodies(&line, &bodies);
    let fits = internal::string_fits(&line, &insulators);
    let budget = ohl_core::budget::Budget::new()
        .time_limit(std::time::Duration::from_secs(a.time_limit.max(1)));
    let report = internal::check(
        &line,
        &views,
        &placed,
        &fits,
        &set.internal_rules,
        &Default::default(),
        &budget,
    )?;
    for (s, why) in &report.not_checked {
        println!("  not checked: {s}: {why}");
    }
    for n in &report.notes {
        println!("  note: {n}");
    }
    println!(
        "{:<20} {:<16} {:<10} {:<28} {:<28} {:>10} {:>10} {:<12} status",
        "rule", "case", "structure", "part", "member or shape", "distance", "required", "to"
    );
    for r in &report.rows {
        println!(
            "{:<20} {:<16} {:<10} {:<28} {:<28} {:>8.3} m {:>8.3} m {:<12} {}",
            set.internal_rules[r.rule].name,
            r.case,
            r.structure,
            r.part.to_string(),
            Some(r.against())
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| "-".into()),
            r.distance,
            r.required,
            if r.to_surface {
                "surface"
            } else {
                "centre line"
            },
            if r.pass() { "pass" } else { "FAIL" }
        );
    }
    let failed = report.failures().count();
    println!(
        "{} parts × structures × cases checked, {failed} too near",
        report.rows.len()
    );
    if let Some(path) = &a.csv {
        std::fs::write(path, report.csv(&set.internal_rules))
            .map_err(|e| format!("{}: {e}", path.display()))?;
        println!("written: {}", path.display());
    }
    Ok(failed == 0)
}
