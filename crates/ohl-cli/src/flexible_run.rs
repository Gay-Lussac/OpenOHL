// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl line … --calculation-level 3` (or 4): the weather cases solved
//! again with the structures flexible (calculation level 3, or level 4
//! with their full models; in active development), for testing — the
//! setup (flexible and rigid structures, why), per case the passes, the
//! time, the largest displacement of each structure and its largest
//! change of load, the largest change of tension and sag of each section,
//! at level 4 the change from level 3 and the structures that failed
//! there; a case that does not converge printed failed, with why.
//! Printed only: every other output of the command stays rigid.

use std::collections::BTreeMap;

use ohl_line::solve::flexible::{CalculationLevel, FlexibleSolve};

/// The structure axes' names (right of the line, ahead, up).
const AXES: [&str; 3] = ["transverse", "longitudinal", "vertical"];

/// Runs and prints [`ohl_loads::solve_line_flexible`] for `cases`.
pub(crate) fn report(
    line: &ohl_line::Line,
    library: &ohl_cables::Library,
    types: &ohl_structures::StructureLibrary,
    sections: &ohl_sections::SectionLibrary,
    cases: &[ohl_weather::WeatherCase],
    level: CalculationLevel,
) {
    println!(
        "calculation level {} (in active development): the tables above, the clearances, the \
         drawings and every file written are the rigid solve's (calculation level 2: they do not \
         take the line's level yet); the structure loads with --load-cases take the larger of \
         rigid and level {0}, `ohl tensions` and `ohl strings` the worse of the two",
        level.number()
    );
    let started = std::time::Instant::now();
    let run = ohl_loads::solve_line_flexible(
        line,
        library,
        types,
        sections,
        cases,
        level,
        &ohl_wirefe::default_fe_budget(),
    );
    let setup = &run.setup;
    if level.flexible() {
        println!(
            "  setup in {:.1?}: {} structures flexible, {} rigid; {} condensations (the \
             stringing state's solve {:.1?})",
            setup.elapsed,
            setup.structures.flexible.len(),
            setup.structures.rigid.len(),
            setup.condensed,
            setup.reference.elapsed
        );
        for r in &setup.structures.rigid {
            println!("  {} rigid: {}", r.structure, r.reason);
        }
        for s in &setup.reference.ruling_span {
            println!("  section {s} solved by the ruling span: rigid");
        }
    }
    let mut notes: Option<&[String]> = None;
    for r in &run.cases {
        match r {
            Ok(s) => {
                print_case(line, s);
                notes.get_or_insert(&s.notes);
            }
            Err(e) => {
                println!(
                    "  case \"{}\": FAILED after {} passes ({:.1?}): {} — the rigid results \
                     above stay, rigid",
                    e.case,
                    e.passes.len(),
                    e.elapsed,
                    e.reason
                );
            }
        }
    }
    for n in notes.unwrap_or_default() {
        println!("  note: {n}");
    }
    println!(
        "calculation level {}: {} cases in {:.1?}",
        level.number(),
        cases.len(),
        started.elapsed()
    );
}

/// One case: passes and times, per structure its largest move and load
/// change, per section its largest tension and sag change.
fn print_case(line: &ohl_line::Line, s: &FlexibleSolve) {
    let rigid_time = s.passes.first().map(|p| p.elapsed).unwrap_or_default();
    let last = s.passes.last();
    let solves = s.line_solves();
    println!(
        "  case \"{}\": converged in {} line solve{} ({:.1?}; the rigid solve {:.1?}, estimated \
         {:.1?}); mismatch {:.4} mm, force change {:.3} N",
        s.case,
        solves,
        if solves == 1 { "" } else { "s" },
        s.elapsed,
        rigid_time,
        s.estimate,
        last.map_or(0.0, |p| p.displacement_residual * 1e3),
        last.map_or(0.0, |p| p.force_residual)
    );
    if s.level == CalculationLevel::Full {
        let at3 = s.passes.iter().filter(|p| p.level == 3 && p.solved).count();
        println!(
            "    level 4: {} line solves at level 3, {} at level 4; {} full structure solves \
             ({:.1?} in all, side by side){}",
            at3,
            solves - at3,
            s.structure_solves.0,
            s.structure_solves.1,
            if s.passes.iter().any(|p| p.relaxed.is_some()) {
                "; staggered passes (the Newton step could not be taken)"
            } else {
                ""
            }
        );
        if let Some(c) = s.change_from_level3() {
            println!("    {c}");
        }
        for f in &s.full_failed {
            println!(
                "    {} FAILED at level 4: {} — taken linear (level 3) in this case",
                f.structure, f.reason
            );
        }
    }
    // Per structure: its point moving most, and its largest change of a
    // load component (structure axes, the whole bundles', unfactored).
    let mut by_structure: BTreeMap<&str, Vec<&ohl_line::solve::flexible::PointResult>> =
        BTreeMap::new();
    for p in &s.points {
        by_structure.entry(&p.structure).or_default().push(p);
    }
    if !by_structure.is_empty() {
        println!(
            "    {:<10} {:<10} {:>28} {:>8}   largest load change (structure axes)",
            "structure", "point", "move right/ahead/up (mm)", "|u| mm"
        );
    }
    for (structure, points) in &by_structure {
        let orientation = line.structure(structure).map_or(0.0, |x| x.orientation_deg);
        let size = |u: &ohl_line::Point| u.iter().map(|x| x * x).sum::<f64>().sqrt();
        let Some(most) = points
            .iter()
            .max_by(|a, b| size(&a.displacement).total_cmp(&size(&b.displacement)))
        else {
            continue;
        };
        let mut change: Option<(f64, String)> = None;
        for p in points {
            let (Some(a), Some(b)) = (p.rigid, p.flexible) else {
                continue;
            };
            let a = ohl_line::links::to_structure_axes(orientation, a);
            let b = ohl_line::links::to_structure_axes(orientation, b);
            for k in 0..3 {
                let d = b[k] - a[k];
                if change.as_ref().is_none_or(|(x, _)| d.abs() > x.abs()) {
                    let share = if a[k].abs() > 1.0 {
                        format!(" ({:+.1} %)", 100.0 * d / a[k].abs())
                    } else {
                        String::new()
                    };
                    change = Some((
                        d,
                        format!(
                            "{} {}: {:.2} → {:.2} kN{share}",
                            p.label,
                            AXES[k],
                            a[k] / 1e3,
                            b[k] / 1e3
                        ),
                    ));
                }
            }
        }
        // Less than a newton: no change worth printing.
        let change = change.filter(|(d, _)| d.abs() >= 1.0);
        let u = most.local;
        println!(
            "    {:<10} {:<10} {:>9.2} {:>9.2} {:>9.2} {:>8.2}   {}",
            structure,
            most.label,
            u[0] * 1e3,
            u[1] * 1e3,
            u[2] * 1e3,
            size(&u) * 1e3,
            change.map_or_else(|| "less than 1 N".to_owned(), |(_, t)| t)
        );
    }
    // Per section: its largest change of tension and of sag.
    let mut by_section: BTreeMap<&str, Vec<&ohl_line::solve::flexible::SpanChange>> =
        BTreeMap::new();
    for c in &s.spans {
        by_section.entry(&c.section).or_default().push(c);
    }
    let moved = by_section.values().any(|spans| {
        spans
            .iter()
            .any(|c| c.tension[0] != c.tension[1] || c.sag[0] != c.sag[1])
    });
    if !moved {
        return;
    }
    println!(
        "    {:<12} {:>34} {:>30}",
        "section", "largest H change (span: kN)", "largest sag change (span: m)"
    );
    for (section, spans) in &by_section {
        let dh = spans
            .iter()
            .max_by(|a, b| {
                (a.tension[1] - a.tension[0])
                    .abs()
                    .total_cmp(&(b.tension[1] - b.tension[0]).abs())
            })
            .expect("not empty");
        let ds = spans
            .iter()
            .max_by(|a, b| {
                (a.sag[1] - a.sag[0])
                    .abs()
                    .total_cmp(&(b.sag[1] - b.sag[0]).abs())
            })
            .expect("not empty");
        println!(
            "    {:<12} {:>34} {:>30}",
            section,
            format!(
                "{}: {:.2} → {:.2} ({:+.2} %)",
                dh.span + 1,
                dh.tension[0] / 1e3,
                dh.tension[1] / 1e3,
                100.0 * (dh.tension[1] - dh.tension[0]) / dh.tension[0].abs().max(1.0)
            ),
            format!(
                "{}: {:.3} → {:.3} ({:+.3})",
                ds.span + 1,
                ds.sag[0],
                ds.sag[1],
                ds.sag[1] - ds.sag[0]
            )
        );
    }
}
