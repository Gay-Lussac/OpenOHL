// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The reference state of the structures (plan of 2026-11-15, phase 5b;
//! owner 2026-11-16; in active development): the line's reference case
//! solved once with the structures flexible, kept with the line
//! ([`ohl_line::reference`]), checked current against the inputs before
//! any solve takes it, and the structures' deflected bodies for drawing.
//!
//! - [`solve_reference`]: the case at calculation level 3 (each structure
//!   with a model a linear stiffness at its points, condensed at the
//!   stringing state, its own wind and ice of the case in its
//!   deflection; the structures as drawn are the structures as strung)
//!   — the points' displacements and forces, the structures taken rigid,
//!   the fingerprint, and each flexible structure's deflected body.
//!   Level 4 is not built: [`REFERENCE_LEVEL`] is the one place the
//!   reference solve's level is chosen — when level 4 exists the solve
//!   switches to it there (its non-linear structures starting from the
//!   level-3 state), and a state of another level is out of date.
//! - [`check_reference`]: compares the stored fingerprint with the
//!   inputs now (the line, the wires, the structure types with their
//!   models and poles and the cross-sections, the case's weather) and
//!   marks the state current — then every level-2 solve of that line
//!   takes it — or out of date with what changed (never used then).
//! - [`reference_bodies`]: the deflected bodies again from the stored
//!   state (a project opened: not stored, they are large).

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use ohl_core::budget::Budget;
use ohl_line::Line;
use ohl_line::reference::{
    ReferenceFingerprint, ReferencePoint, ReferenceRigid, ReferenceState, ReferenceStatus,
    StableHash, line_fingerprint,
};
use ohl_line::solve::flexible::CalculationLevel;
use ohl_structures_fe::drawn::BodyDeflection;
use ohl_weather::WeatherCase;

use crate::LoadCase;
use crate::analysis::Analyses;

/// The calculation level the reference state is solved at: 3 (level 4,
/// the structures' full non-linear models, is not built yet — the hook:
/// switch here when it is).
pub const REFERENCE_LEVEL: CalculationLevel = CalculationLevel::Linear;

/// Why a check keeps the structures as drawn: the structure loads (rigid,
/// on the safe side — owner 2026-11-15: never below rigid).
pub const LOADS_AS_DRAWN: &str = "the structure loads keep the structures as drawn, rigid: on \
    the safe side";

/// The fingerprint of what a reference state of `line` in weather case
/// `case` is solved from ([`ReferenceFingerprint`]): the line as solved,
/// the wires it uses, the structure types it places (each type, its
/// model file, its pole) with the cross-section library, the case's
/// weather.
///
/// Expected cost: a pass over the line as JSON, the types used and
/// their model files (a few ms to some tens of ms for a long line).
pub fn fingerprint(
    line: &Line,
    library: &ohl_cables::Library,
    types: &ohl_structures::StructureLibrary,
    sections: &ohl_sections::SectionLibrary,
    case: &WeatherCase,
) -> ReferenceFingerprint {
    let mut wires = StableHash::default();
    let mut cables: Vec<&str> = line
        .sections
        .iter()
        .map(|s| s.cable.as_str())
        .chain(line.jumpers.iter().map(|j| j.cable.as_str()))
        .collect();
    cables.sort_unstable();
    cables.dedup();
    for id in cables {
        wires.debug(&(id, library.get(id)));
    }
    let mut structures = StableHash::default();
    let mut used: Vec<&str> = line
        .structures
        .iter()
        .filter_map(|s| s.structure_type.as_deref())
        .collect();
    used.sort_unstable();
    used.dedup();
    for id in used {
        let Some(t) = types.get(id) else {
            structures.debug(&(id, "not in the library"));
            continue;
        };
        structures.debug(t);
        let dir = types
            .path(id)
            .and_then(std::path::Path::parent)
            .map(std::path::Path::to_path_buf)
            .unwrap_or_default();
        if let Some(file) = &t.model {
            match std::fs::read(dir.join(file)) {
                Ok(bytes) => structures.bytes(&bytes),
                Err(e) => structures.debug(&e.kind()),
            }
        } else if t.members_designed()
            && let Some(path) = types.path(id)
            && let Ok(bytes) = std::fs::read(path)
        {
            structures.bytes(&bytes);
        }
        structures.debug(&ohl_structures_fe::pole::structure_pole(t, &dir, sections));
    }
    structures.debug(sections);
    let mut weather = StableHash::default();
    weather.debug(case);
    ReferenceFingerprint {
        line: line_fingerprint(line),
        wires: wires.hex(),
        structures: structures.hex(),
        case: weather.hex(),
    }
}

/// Marks `line`'s reference state current or out of date against the
/// inputs now (`library`, `types`, `sections`, the weather cases
/// `weather`): current only when it was solved in the case set now, at
/// the level the reference solve takes ([`REFERENCE_LEVEL`]), and every
/// part of its fingerprint is that of the inputs ([`fingerprint`]); else
/// out of date with what changed. No state: not checked (said as not
/// solved). Returns the status set (none: no reference case).
///
/// Expected cost: [`fingerprint`].
pub fn check_reference(
    line: &mut Line,
    library: &ohl_cables::Library,
    types: &ohl_structures::StructureLibrary,
    sections: &ohl_sections::SectionLibrary,
    weather: &[WeatherCase],
) -> Option<ReferenceStatus> {
    let status = {
        let r = line.reference.as_ref()?;
        match &r.state {
            None => ReferenceStatus::Unchecked,
            Some(state) => match weather.iter().find(|w| w.name == r.case) {
                _ if state.case != r.case => ReferenceStatus::Stale(format!(
                    "solved in case \"{}\", the reference case is now \"{}\"",
                    state.case, r.case
                )),
                _ if state.level != REFERENCE_LEVEL.number() => ReferenceStatus::Stale(format!(
                    "solved at calculation level {}, the reference solve is now at level {}",
                    state.level,
                    REFERENCE_LEVEL.number()
                )),
                None => ReferenceStatus::Stale(format!(
                    "weather case \"{}\" is not among the project's weather cases",
                    r.case
                )),
                Some(w) => {
                    let now = fingerprint(line, library, types, sections, w);
                    let changed = state.fingerprint.changed(&now);
                    if changed.is_empty() {
                        ReferenceStatus::Current
                    } else {
                        ReferenceStatus::Stale(format!(
                            "{} changed since it was solved",
                            join(&changed)
                        ))
                    }
                }
            },
        }
    };
    if let Some(r) = line.reference.as_mut() {
        r.status = status.clone();
    }
    Some(status)
}

/// "a, b and c".
fn join(items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [first @ .., last] => format!("{} and {last}", first.join(", ")),
    }
}

/// A reference state solved ([`solve_reference`]).
#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceSolve {
    /// The state, to keep with the line.
    pub state: ReferenceState,
    /// Each flexible structure's deflected body (by id; display only,
    /// not stored: [`reference_bodies`] makes them again).
    pub bodies: BTreeMap<String, BodyDeflection>,
    /// The structures whose body could not be found, with why (drawn as
    /// they are).
    pub bodies_failed: Vec<String>,
    /// The time of the structures' setup (the stringing state's solve and
    /// the condensations).
    pub setup: Duration,
    /// The time of the coupled solve of the case.
    pub solve: Duration,
    /// The time of the deflected bodies.
    pub drawing: Duration,
}

/// `line`'s structures solved in weather case `case` (by name, among
/// `weather`) at the reference level ([`REFERENCE_LEVEL`]: 3) to keep as
/// its reference state: the structures set up as the checks at level 3
/// set them up ([`crate::prepare_check_level`]: condensed at the
/// stringing state, their own wind and ice of the case in their
/// deflection), the case solved by the coupled loop (from the structures
/// as strung: a state already in use is set aside), each flexible
/// structure's points with their displacement and forces, the
/// structures taken rigid with why, the fingerprint of the inputs, and
/// the deflected bodies. `done` told per structure set up, `cancel`
/// stops it. Err: the case is not there, or its coupled solve did not
/// converge (with why) — nothing to keep.
///
/// Expected cost: one rigid line solve and one condensation per placed
/// structure (the setup), 2–5 line solves (the case), one more
/// factorisation per flexible structure (its body).
#[allow(clippy::too_many_arguments)]
pub fn solve_reference(
    line: &Line,
    library: &ohl_cables::Library,
    types: &ohl_structures::StructureLibrary,
    sections: &ohl_sections::SectionLibrary,
    weather: &[WeatherCase],
    case: &str,
    budget: &Budget,
    done: Option<&(dyn Fn(usize, usize) + Sync)>,
    cancel: Option<&ohl_core::budget::CancelToken>,
) -> Result<ReferenceSolve, String> {
    let w = weather
        .iter()
        .find(|w| w.name == case)
        .ok_or_else(|| format!("no weather case \"{case}\" among the project's weather cases"))?;
    let drawn = line.as_drawn(ohl_line::solve::flexible::STRUNG);
    let line = &*drawn;
    let started = Instant::now();
    let level = crate::prepare_check_level(
        line,
        library,
        types,
        sections,
        std::slice::from_ref(w),
        REFERENCE_LEVEL,
        budget,
        done,
        cancel,
    );
    let setup_time = started.elapsed();
    if cancel.is_some_and(ohl_core::budget::CancelToken::is_cancelled) {
        return Err("stopped".into());
    }
    let solving = Instant::now();
    let solved = level.solve(line, library, w, budget);
    let solve_time = solving.elapsed();
    let flexible = match solved.level {
        Some(Ok(f)) => f,
        Some(Err(why)) => return Err(why),
        None => return Err("no structure set up at calculation level 3".into()),
    };
    let setup = level
        .setup
        .as_ref()
        .ok_or("no structure set up at calculation level 3")?;
    let strung = &setup.setup.reference.forces;
    let points: Vec<ReferencePoint> = flexible
        .points
        .iter()
        .map(|p| ReferencePoint {
            structure: p.structure.clone(),
            label: p.label.clone(),
            displacement: p.displacement,
            force: p.flexible.unwrap_or([0.0; 3]),
            strung: strung
                .get(&(p.structure.clone(), p.label.clone()))
                .copied()
                .unwrap_or([0.0; 3]),
        })
        .collect();
    let rigid = setup
        .setup
        .structures
        .rigid
        .iter()
        .map(|r| ReferenceRigid {
            structure: r.structure.clone(),
            reason: r.reason.clone(),
        })
        .collect();
    let mut state = ReferenceState {
        case: case.to_owned(),
        level: REFERENCE_LEVEL.number(),
        fingerprint: fingerprint(line, library, types, sections, w),
        points,
        rigid,
        notes: flexible.notes.clone(),
        passes: flexible.passes.len(),
        seconds: 0.0,
    };
    let drawing = Instant::now();
    let analyses = Analyses::load(types, sections);
    let (bodies, bodies_failed) = bodies_of(line, types, &analyses, &state, w);
    let drawing_time = drawing.elapsed();
    state.seconds = (setup_time + solve_time).as_secs_f64();
    Ok(ReferenceSolve {
        state,
        bodies,
        bodies_failed,
        setup: setup_time,
        solve: solve_time,
        drawing: drawing_time,
    })
}

/// The deflected bodies of `line`'s reference state `state` (a stored
/// one: the forces at the stringing state and in the case at each point,
/// the structures' own wind and ice of the case from `weather`), each
/// flexible structure's ([`BodyDeflection`]; by id), and the structures
/// whose body could not be found, with why. None of it when the case is
/// not among `weather`.
///
/// Expected cost: one reference solve and one factorisation per flexible
/// structure (as one condensation each).
pub fn reference_bodies(
    line: &Line,
    types: &ohl_structures::StructureLibrary,
    sections: &ohl_sections::SectionLibrary,
    state: &ReferenceState,
    weather: &[WeatherCase],
) -> (BTreeMap<String, BodyDeflection>, Vec<String>) {
    let Some(w) = weather.iter().find(|w| w.name == state.case) else {
        return (
            BTreeMap::new(),
            vec![format!("weather case \"{}\" not found", state.case)],
        );
    };
    let analyses = Analyses::load(types, sections);
    bodies_of(line, types, &analyses, state, w)
}

/// [`reference_bodies`] with the models read.
fn bodies_of(
    line: &Line,
    types: &ohl_structures::StructureLibrary,
    analyses: &Analyses,
    state: &ReferenceState,
    w: &WeatherCase,
) -> (BTreeMap<String, BodyDeflection>, Vec<String>) {
    let mut by_structure: BTreeMap<&str, Vec<&ReferencePoint>> = BTreeMap::new();
    for p in &state.points {
        by_structure.entry(&p.structure).or_default().push(p);
    }
    let budget = ohl_structures_fe::default_budget();
    let case = LoadCase::from_weather(&w.name);
    let items: Vec<(&str, Vec<&ReferencePoint>)> = by_structure.into_iter().collect();
    let answers = crate::flexibility::over_cores(
        &items,
        |(id, points)| -> Result<BodyDeflection, String> {
            let s = line
                .structure(id)
                .ok_or_else(|| format!("{id}: not in the line"))?;
            let t = s
                .structure_type
                .as_deref()
                .and_then(|t| types.get(t))
                .ok_or_else(|| format!("{id}: no structure type in the library"))?;
            let local =
                |f: ohl_line::Point| ohl_line::links::to_structure_axes(s.orientation_deg, f);
            let strung: Vec<(String, [f64; 3])> = points
                .iter()
                .map(|p| (p.label.clone(), local(p.strung)))
                .collect();
            let forces: Vec<(String, [f64; 3])> = points
                .iter()
                .map(|p| (p.label.clone(), local(p.force)))
                .collect();
            let body = case
                .in_structure_axes(w, line, s)
                .unwrap_or_else(|| case.clone());
            analyses
                .deflected_body(t, Some(s), &strung, &forces, Some((&body, w)), &budget)
                .map_err(|e| format!("{id}: {e}"))
        },
        None,
        None,
    );
    let mut bodies = BTreeMap::new();
    let mut failed = Vec::new();
    for ((id, _), a) in items.iter().zip(answers) {
        match a {
            Some(Ok(b)) => {
                bodies.insert((*id).to_owned(), b);
            }
            Some(Err(e)) => failed.push(e),
            None => failed.push(format!("{id}: not reached")),
        }
    }
    (bodies, failed)
}

/// The reference state's displacement of each structure's points in its
/// own axes (right of the line, ahead, up; m), and the largest, for
/// reports: (structure, label, local displacement).
pub fn moves_in_structure_axes(
    line: &Line,
    state: &ReferenceState,
) -> Vec<(String, String, [f64; 3])> {
    state
        .points
        .iter()
        .map(|p| {
            let orientation = line
                .structure(&p.structure)
                .map_or(0.0, |s| s.orientation_deg);
            (
                p.structure.clone(),
                p.label.clone(),
                ohl_line::links::to_structure_axes(orientation, p.displacement),
            )
        })
        .collect()
}
