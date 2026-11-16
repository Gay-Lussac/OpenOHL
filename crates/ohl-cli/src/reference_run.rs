// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The line's reference state of the structures on the command line (plan
//! of 2026-11-15, phase 5b; in active development): `--structure-reference
//! use` (default) checks the stored state against the libraries and the
//! weather cases of the run and, current, every level-2 solve of the run
//! takes the structures as deflected in the reference case; `ignore` sets
//! it aside (the structures as drawn). A state out of date or not
//! checkable is never used — said. `ohl line --solve-reference CASE`
//! solves one (at calculation level 3) for the run, and `--save-reference
//! FILE` writes the line with it.

use std::path::Path;
use std::sync::Mutex;

use ohl_line::Line;
use ohl_line::reference::{ReferenceStatus, StructureReference};
use ohl_weather::WeatherCase;

/// `--structure-reference`: `use` (true) or `ignore` (false).
pub(crate) fn parse_use(text: &str) -> Result<bool, String> {
    match text.trim() {
        "use" => Ok(true),
        "ignore" => Ok(false),
        other => Err(format!("structure reference \"{other}\": use or ignore")),
    }
}

/// Why a state is set aside by `--structure-reference ignore`.
pub(crate) const IGNORED: &str = "ignored by --structure-reference ignore";

/// What a run checks a reference state against, kept from the reading of
/// the line until its weather cases are read ([`checked`]).
pub(crate) struct Inputs {
    pub library: ohl_cables::Library,
    pub types: ohl_structures::StructureLibrary,
    pub sections: ohl_sections::SectionLibrary,
    pub use_it: bool,
}

/// The inputs of the line read last (one line per run).
static INPUTS: Mutex<Option<Inputs>> = Mutex::new(None);

/// Keeps `inputs` for [`checked`] (a line with a reference case).
pub(crate) fn keep(inputs: Inputs) {
    *INPUTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(inputs);
}

/// `line`'s reference state checked against the inputs kept when it was
/// read ([`keep`]) and the weather cases `weather` (all the file's), the
/// note printed; nothing without a reference case.
pub(crate) fn checked(line: &mut Line, weather: &[WeatherCase]) {
    if line.reference.is_none() {
        return;
    }
    let inputs = INPUTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match inputs.as_ref() {
        Some(i) => check(line, &i.library, &i.types, &i.sections, weather, i.use_it),
        None => print_note(line),
    }
}

/// `line`'s reference state checked (`use_it`) or set aside, the note
/// printed; nothing without a reference case.
pub(crate) fn check(
    line: &mut Line,
    library: &ohl_cables::Library,
    types: &ohl_structures::StructureLibrary,
    sections: &ohl_sections::SectionLibrary,
    weather: &[WeatherCase],
    use_it: bool,
) {
    if line.reference.is_none() {
        return;
    }
    if use_it {
        ohl_loads::reference::check_reference(line, library, types, sections, weather);
    } else if let Some(r) = line.reference.as_mut()
        && r.state.is_some()
    {
        r.status = ReferenceStatus::SetAside(IGNORED.to_owned());
    }
    print_note(line);
}

/// Prints the line's reference note.
fn print_note(line: &Line) {
    if let Some(n) = line.reference_note() {
        println!("note: {n}");
    }
}

/// `--solve-reference CASE`: `line`'s structures solved in weather case
/// `case` (among `weather`) at calculation level 3 and kept as its
/// reference state for this run (checked current, so the run's level-2
/// solves take it unless `use_it` is false), the points' largest moves
/// and the times printed; written with the line to `save` when given.
#[allow(clippy::too_many_arguments)]
pub(crate) fn solve(
    line: &mut Line,
    library: &ohl_cables::Library,
    types: &ohl_structures::StructureLibrary,
    sections: &ohl_sections::SectionLibrary,
    weather: &[WeatherCase],
    case: &str,
    use_it: bool,
    save: Option<&Path>,
) -> Result<(), String> {
    let solved = ohl_loads::reference::solve_reference(
        line,
        library,
        types,
        sections,
        weather,
        case,
        &ohl_wirefe::default_fe_budget(),
        None,
        None,
    )
    .map_err(|e| format!("reference case \"{case}\": {e}"))?;
    let s = &solved.state;
    println!(
        "reference case \"{case}\" solved at calculation level {} in {:.1?} (structures set up \
         {:.1?}, coupled solve {:.1?} in {} passes; deflected bodies {:.1?}): {} points of {} \
         structures moved, {} structures as drawn",
        s.level,
        solved.setup + solved.solve + solved.drawing,
        solved.setup,
        solved.solve,
        s.passes,
        solved.drawing,
        s.points.len(),
        solved.bodies.len(),
        s.rigid.len()
    );
    let mut largest: std::collections::BTreeMap<&str, (f64, [f64; 3], String)> = Default::default();
    let local = ohl_loads::reference::moves_in_structure_axes(line, s);
    for (structure, label, u) in &local {
        let d = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt();
        let e = largest
            .entry(structure.as_str())
            .or_insert((0.0, [0.0; 3], String::new()));
        if d >= e.0 {
            *e = (d, *u, label.clone());
        }
    }
    for (structure, (d, u, label)) in &largest {
        println!(
            "  {structure}: largest move {:.1} mm at {label} (transverse {:+.1}, longitudinal \
             {:+.1}, vertical {:+.1} mm)",
            d * 1e3,
            u[0] * 1e3,
            u[1] * 1e3,
            u[2] * 1e3
        );
    }
    for r in &s.rigid {
        println!("  {} as drawn: {}", r.structure, r.reason);
    }
    for f in &solved.bodies_failed {
        println!("  body not drawn deflected: {f}");
    }
    let mut reference = StructureReference::new(case);
    reference.state = Some(solved.state);
    line.reference = Some(reference);
    if let Some(path) = save {
        std::fs::write(path, line.to_json()).map_err(|e| format!("{}: {e}", path.display()))?;
        println!(
            "line with its reference state written to {}",
            path.display()
        );
    }
    check(line, library, types, sections, weather, use_it);
    Ok(())
}
