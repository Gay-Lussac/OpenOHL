// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl internal`: the internal clearances ([`ohl_clearance::internal`])
//! by the `[[internal_rule]]` tables of a rule file — at every structure
//! with a body (levels 2 and 3), the conductors near it, its jumpers and
//! its strings' wire points as solved in each weather case of a rule,
//! against every member of the body; the worst first; CSV with `--csv`.

use std::path::PathBuf;

use clap::Args;
use ohl_clearance::internal::{self, BodyMember, StructureBody};
use ohl_clearance::rules::RuleSet;
use ohl_structures::structures::to_global;

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

/// The bodies of the structures of `line` at levels 2 and 3 as placed in
/// project coordinates (as the desktop application places them: the type
/// at the structure's position raised by its height adjustment, turned by
/// its orientation): each member with its solid where its profile is
/// known, a pole as one member (its axis and its outline's solid).
pub fn structure_bodies(
    line: &ohl_line::Line,
    bodies: &ohl_structures_fe::drawn::Bodies,
) -> Vec<StructureBody> {
    let mut out = Vec::new();
    for st in &line.structures {
        let Some(b) = st
            .structure_type
            .as_deref()
            .and_then(|t| bodies.get(t, st.extension.as_deref()))
        else {
            continue;
        };
        let at = [
            st.position[0],
            st.position[1],
            st.position[2] + st.height_adjust,
        ];
        let place = |p: &[f64; 3]| to_global(at, st.orientation_deg, *p);
        let mut members: Vec<BodyMember> = b
            .members
            .iter()
            .map(|m| BodyMember {
                id: m.id.clone(),
                line: m.points.iter().map(place).collect(),
                solid: m.solid.iter().map(|t| t.each_ref().map(place)).collect(),
            })
            .collect();
        if let Some(p) = &b.pole
            && let (Some(lo), Some(hi)) = (p.rings.first(), p.rings.last())
        {
            members.push(BodyMember {
                id: Some("pole".into()),
                line: vec![place(&[0.0, 0.0, lo.0]), place(&[0.0, 0.0, hi.0])],
                solid: p.solid.iter().map(|t| t.each_ref().map(place)).collect(),
            });
        }
        out.push(StructureBody {
            structure: st.id.clone(),
            members,
        });
    }
    out
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
    if line.structures.iter().any(|s| s.structure_type.is_some()) {
        let structures = ohl_structures::StructureLibrary::load_dir(&libs.structures)
            .map_err(|e| e.to_string())?;
        let insulators = ohl_structures::InsulatorLibrary::load_dir(&libs.insulators)
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
        solved.push((case.name.clone(), shapes, jumpers));
    }
    let views: Vec<(
        &str,
        &[ohl_line::solve::SectionShape],
        &[ohl_line::jumper::JumperShape],
    )> = solved
        .iter()
        .map(|(n, s, j)| (n.as_str(), s.as_slice(), j.as_slice()))
        .collect();
    let placed = structure_bodies(&line, &bodies);
    let budget = ohl_core::budget::Budget::new()
        .time_limit(std::time::Duration::from_secs(a.time_limit.max(1)));
    let report = internal::check(
        &line,
        &views,
        &placed,
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
        "{:<20} {:<16} {:<10} {:<20} {:<12} {:>10} {:>10} {:<12} status",
        "rule", "case", "structure", "part", "member", "distance", "required", "to"
    );
    for r in &report.rows {
        println!(
            "{:<20} {:<16} {:<10} {:<20} {:<12} {:>8.3} m {:>8.3} m {:<12} {}",
            set.internal_rules[r.rule].name,
            r.case,
            r.structure,
            r.part.to_string(),
            r.member.as_deref().unwrap_or("-"),
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
