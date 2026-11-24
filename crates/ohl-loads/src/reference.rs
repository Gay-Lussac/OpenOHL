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
//!   deflection; the structures as drawn are the structures as strung),
//!   or at level 4 (from there, each structure solved in full) when the
//!   line's setting is full structures ([`reference_level`], the one
//!   place the level is chosen) — the points' displacements and forces,
//!   the structures taken rigid, the fingerprint, and each flexible
//!   structure's deflected body (drawn on the level-3 tangent). A state
//!   solved at another level than the setting's now is out of date.
//! - [`check_reference`]: compares the stored fingerprint with the
//!   inputs now (the line, the wires, the structure types with their
//!   models and poles and the cross-sections, the case's weather) and
//!   marks the state current — then every level-2 solve of that line
//!   takes it — or out of date with what changed (never used then).
//! - [`reference_bodies`]: the deflected bodies again from the stored
//!   state (a project opened: not stored, they are large).
//! - **Foot movements** (owner 2026-11-16: "the tower should not be
//!   initially built from a scan but from plans then adjusted based on the
//!   movement of the feet and the lean of the weather case"): each
//!   structure's measured foot movements ([`ohl_line::feet`]) are in the
//!   reference solve (the level-3 setup moves its points by them, the
//!   rigid-body motion and the elastic distortion: `ohl_structures_fe::feet`) and
//!   in its fingerprint; without a reference state in use, [`check_reference`]
//!   solves the structures with their foot movements alone, without loads
//!   ([`solve_feet`], automatically: [`ohl_line::reference::FeetState`]).
//! - **The reference case's wind** is taken as OHL-Design shows it
//!   ([`as_shown`]): a case without a direction of its own from the
//!   right, so a state solved by one program is current in the other
//!   (owner 2026-11-16).

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use ohl_core::budget::Budget;
use ohl_line::Line;
use ohl_line::reference::{
    FeetState, ReferenceFingerprint, ReferencePoint, ReferenceRigid, ReferenceState,
    ReferenceStatus, StableHash, feet_fingerprint, line_fingerprint,
};
use ohl_line::solve::flexible::CalculationLevel;
use ohl_structures_fe::drawn::BodyDeflection;
use ohl_weather::WeatherCase;

use crate::LoadCase;
use crate::analysis::Analyses;

/// The calculation level `line`'s reference state is solved at (phase 6,
/// 2026-11-16): 4 when its setting (Line › Criteria › Calculation
/// options, [`ohl_line::SolverOptions::structures`]) is full structures,
/// else 3 — the user chooses it with that setting.
pub fn reference_level(line: &Line) -> CalculationLevel {
    match line.solver.structures.level() {
        CalculationLevel::Full => CalculationLevel::Full,
        _ => CalculationLevel::Linear,
    }
}

/// Why a check keeps the structures as drawn: the structure loads (rigid,
/// on the safe side — owner 2026-11-15: never below rigid).
pub const LOADS_AS_DRAWN: &str = "the structure loads keep the structures as drawn, rigid: on \
    the safe side";

/// `case` as OHL-Design shows and solves it, as the reference solve takes
/// it (owner 2026-11-16): a case without a direction of its own blowing
/// from the right; one at an angle from the right's side (mirrored when
/// its side was the left); one with its own direction across the line as
/// it is from its own side. The same case in the command line and in
/// OHL-Design, so a state saved by one is not out of date in the other.
/// Only the reference solve, its fingerprint and its bodies take it: the
/// command line's other checks keep the weather file's side.
pub fn as_shown(case: &WeatherCase) -> WeatherCase {
    let mut c = case.clone();
    let right = ohl_weather::WindSide::Right;
    let side = match c.own_side() {
        Some(own) if c.wind_from == own => right,
        Some(_) => ohl_weather::WindSide::Left,
        None => c.wind_from,
    };
    if side != right {
        match c.wind_direction {
            Some(d) if d.across_side().is_some() => c.wind_from = c.wind_from.opposite(),
            Some(d) => {
                c.wind_direction = Some(d.mirrored());
                c.wind_from = right;
            }
            None => c.wind_from = right,
        }
    }
    c
}

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
    let mut weather = StableHash::default();
    weather.debug(&as_shown(case));
    ReferenceFingerprint {
        line: line_fingerprint(line),
        wires: wires.hex(),
        structures: fingerprint_structures(line, types, sections),
        case: weather.hex(),
        feet: feet_fingerprint(line),
    }
}

/// The structures' part of [`fingerprint`]: the types the line places,
/// their models (the files' bytes), poles and the cross-sections.
fn fingerprint_structures(
    line: &Line,
    types: &ohl_structures::StructureLibrary,
    sections: &ohl_sections::SectionLibrary,
) -> String {
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
    structures.hex()
}

/// Marks `line`'s reference state current or out of date against the
/// inputs now (`library`, `types`, `sections`, the weather cases
/// `weather`): current only when it was solved in the case set now, at
/// the level the reference solve takes ([`reference_level`]), and every
/// part of its fingerprint is that of the inputs ([`fingerprint`]); else
/// out of date with what changed. No state: not checked (said as not
/// solved). Returns the status set (none: no reference case).
///
/// When the line's structures have foot movements and no reference state
/// is current, the structures moved by their foot movements alone are
/// solved ([`solve_feet`]) and kept current in [`Line::feet_state`] —
/// automatically, said in the results ([`Line::reference_note`]); else
/// that is emptied.
///
/// Expected cost: [`fingerprint`]; with foot movements and no current
/// state, [`solve_feet`] (kept per inputs: once while they do not change).
pub fn check_reference(
    line: &mut Line,
    library: &ohl_cables::Library,
    types: &ohl_structures::StructureLibrary,
    sections: &ohl_sections::SectionLibrary,
    weather: &[WeatherCase],
) -> Option<ReferenceStatus> {
    let status = check_case(line, library, types, sections, weather);
    let current = status == Some(ReferenceStatus::Current);
    line.feet_state = if line.has_foot_movements() && !current {
        FeetState {
            state: Some(feet_kept(line, types, sections)),
            status: ReferenceStatus::Current,
        }
    } else {
        FeetState::default()
    };
    status
}

/// [`check_reference`] of the reference case alone.
fn check_case(
    line: &mut Line,
    library: &ohl_cables::Library,
    types: &ohl_structures::StructureLibrary,
    sections: &ohl_sections::SectionLibrary,
    weather: &[WeatherCase],
) -> Option<ReferenceStatus> {
    let level = reference_level(line).number();
    let status = {
        let r = line.reference.as_ref()?;
        match &r.state {
            None => ReferenceStatus::Unchecked,
            Some(state) => match weather.iter().find(|w| w.name == r.case) {
                _ if state.case != r.case => ReferenceStatus::Stale(format!(
                    "solved in case \"{}\", the reference case is now \"{}\"",
                    state.case, r.case
                )),
                _ if state.level != level => ReferenceStatus::Stale(format!(
                    "solved at calculation level {}, the reference solve is now at level {level} \
                     (Line › Criteria › Calculation options)",
                    state.level
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
/// `weather`) at the reference level ([`reference_level`]: 3, or 4 when
/// the line's setting is full structures) to keep as its reference state:
/// the structures set up as the checks at that level set them up
/// ([`crate::prepare_check_level`]: condensed at the stringing state,
/// their own wind and ice of the case in their deflection; level 4 their
/// full models besides), the case solved by the coupled loop (from the
/// structures as strung: a state already in use is set aside), each
/// flexible
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
    let w = &as_shown(w);
    let drawn = line.as_drawn(ohl_line::solve::flexible::STRUNG);
    let line = &*drawn;
    let started = Instant::now();
    let at = reference_level(line);
    let level = crate::prepare_check_level(
        line,
        library,
        types,
        sections,
        std::slice::from_ref(w),
        at,
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
        None => {
            return Err(format!(
                "no structure set up at calculation level {}",
                at.number()
            ));
        }
    };
    let setup = level
        .setup
        .as_ref()
        .ok_or_else(|| format!("no structure set up at calculation level {}", at.number()))?;
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
        level: at.number(),
        fingerprint: fingerprint(line, library, types, sections, w),
        points,
        rigid,
        notes: flexible.notes.clone(),
        passes: flexible.passes.len(),
        seconds: 0.0,
    };
    let drawing = Instant::now();
    let analyses = Analyses::load(types, sections);
    let (bodies, bodies_failed) = bodies_of(line, types, &analyses, &state, Some(w));
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
    let analyses = Analyses::load(types, sections);
    if state.level == FEET_LEVEL {
        return bodies_of(line, types, &analyses, state, None);
    }
    let Some(w) = weather.iter().find(|w| w.name == state.case) else {
        return (
            BTreeMap::new(),
            vec![format!("weather case \"{}\" not found", state.case)],
        );
    };
    bodies_of(line, types, &analyses, state, Some(&as_shown(w)))
}

/// [`reference_bodies`] with the models read; `w` none: the foot
/// movements alone ([`FEET_LEVEL`]: no load, the elastic structure).
fn bodies_of(
    line: &Line,
    types: &ohl_structures::StructureLibrary,
    analyses: &Analyses,
    state: &ReferenceState,
    w: Option<&WeatherCase>,
) -> (BTreeMap<String, BodyDeflection>, Vec<String>) {
    let mut by_structure: BTreeMap<&str, Vec<&ReferencePoint>> = BTreeMap::new();
    for p in &state.points {
        by_structure.entry(&p.structure).or_default().push(p);
    }
    let budget = ohl_structures_fe::default_budget();
    let case = w.map(|w| LoadCase::from_weather(&w.name));
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
            let (Some(w), Some(case)) = (w, &case) else {
                // The foot movements alone.
                return analyses
                    .deflected_body(t, Some(s), &[], &[], None, true, &budget)
                    .map_err(|e| format!("{id}: {e}"));
            };
            let body = case
                .in_structure_axes(w, line, s)
                .unwrap_or_else(|| case.clone());
            analyses
                .deflected_body(
                    t,
                    Some(s),
                    &strung,
                    &forces,
                    Some((&body, w)),
                    false,
                    &budget,
                )
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

/// The level a state of the foot movements alone is said to be solved at
/// ([`solve_feet`]): 0, no calculation level (no wire, no load).
pub const FEET_LEVEL: u8 = 0;

/// The structures of `line` with measured foot movements moved by them
/// alone (owner 2026-11-16; the state used without a reference state,
/// [`ohl_line::reference::FeetState`]): each structure with a model on
/// its elastic model without loads, its supports moved
/// (`ohl_structures_fe::feet::on_supports`; a statically indeterminate one strained);
/// a pole about its base; a structure without a model as one body (its
/// foot `all`). Per attachment point (the placed structure's points and
/// wire points) its move in the line's axes; a structure whose movements
/// cannot be taken listed with why (as drawn: NOT ON THE SAFE SIDE, said).
/// Case empty, level [`FEET_LEVEL`]; its fingerprint the line's, the
/// structures' and the foot movements'.
///
/// Expected cost: one elastic solve (a factorisation) per structure with
/// foot movements; only their types' models read.
pub fn solve_feet(
    line: &Line,
    types: &ohl_structures::StructureLibrary,
    sections: &ohl_sections::SectionLibrary,
) -> ReferenceState {
    let started = Instant::now();
    // The models of the types of the structures with foot movements only
    // (a real library's others are not read).
    let mut needed = ohl_structures::StructureLibrary::default();
    for s in &line.structures {
        if s.foot_movements
            .as_ref()
            .is_none_or(ohl_line::feet::FootMovements::is_empty)
        {
            continue;
        }
        if let Some(id) = s.structure_type.as_deref()
            && needed.get(id).is_none()
            && let (Some(t), Some(path)) = (types.get(id), types.path(id))
        {
            let _ = needed.insert(path, t.clone());
        }
    }
    let analyses = Analyses::load(&needed, sections);
    let budget = ohl_structures_fe::default_budget();
    let mut points = Vec::new();
    let mut rigid = Vec::new();
    let mut notes = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for s in &line.structures {
        if seen.contains(&s.id.as_str())
            || s.foot_movements
                .as_ref()
                .is_none_or(ohl_line::feet::FootMovements::is_empty)
        {
            continue;
        }
        seen.push(&s.id);
        let Some(t) = s.structure_type.as_deref().and_then(|t| types.get(t)) else {
            match one_body_moves(None, s) {
                Ok(moves) => points.extend(moves),
                Err(e) => rigid.push(ReferenceRigid {
                    structure: s.id.clone(),
                    reason: e,
                }),
            }
            continue;
        };
        match feet_moves(&analyses, t, s, &budget) {
            Ok((moves, n)) => {
                points.extend(moves);
                notes.extend(n.into_iter().map(|x| format!("{}: {x}", s.id)));
            }
            Err(e) => rigid.push(ReferenceRigid {
                structure: s.id.clone(),
                reason: e,
            }),
        }
    }
    let mut fingerprint = ReferenceFingerprint {
        line: line_fingerprint(line),
        feet: feet_fingerprint(line),
        ..ReferenceFingerprint::default()
    };
    fingerprint.structures = fingerprint_structures(line, types, sections);
    ReferenceState {
        case: String::new(),
        level: FEET_LEVEL,
        fingerprint,
        points,
        rigid,
        notes,
        passes: 0,
        seconds: started.elapsed().as_secs_f64(),
    }
}

/// [`solve_feet`] kept by its inputs (the line's, the structures' and the
/// foot movements' fingerprints): made once while they do not change.
fn feet_kept(
    line: &Line,
    types: &ohl_structures::StructureLibrary,
    sections: &ohl_sections::SectionLibrary,
) -> ReferenceState {
    static KEPT: std::sync::Mutex<Option<ReferenceState>> = std::sync::Mutex::new(None);
    let key = ReferenceFingerprint {
        line: line_fingerprint(line),
        feet: feet_fingerprint(line),
        structures: fingerprint_structures(line, types, sections),
        ..ReferenceFingerprint::default()
    };
    let mut kept = KEPT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(state) = kept.as_ref()
        && state.fingerprint == key
    {
        return state.clone();
    }
    let state = solve_feet(line, types, sections);
    *kept = Some(state.clone());
    state
}

/// The labels of placed structure `s`'s attachment points (its points
/// and wire points, as the wire solve names its holders; the type's when
/// the structure is not resolved).
fn labels_of(s: &ohl_line::Structure, t: Option<&ohl_structures::StructureType>) -> Vec<String> {
    let mut labels: Vec<String> = s
        .points
        .iter()
        .map(|p| p.label.clone())
        .chain(s.wires.iter().map(|w| w.label.clone()))
        .collect();
    if labels.is_empty()
        && let Some(t) = t
    {
        labels = t
            .points
            .iter()
            .map(|p| p.label.clone())
            .chain(t.wires.iter().map(|w| w.label.clone()))
            .collect();
    }
    labels.sort();
    labels.dedup();
    labels
}

/// The moves of `s`'s points (type `t`) by its foot movements alone: on
/// its model (each point at its place, u + θ × r), its pole, or as one
/// body; with what was left out. Err: why they cannot be taken.
fn feet_moves(
    analyses: &Analyses,
    t: &ohl_structures::StructureType,
    s: &ohl_line::Structure,
    budget: &Budget,
) -> Result<(Vec<ReferencePoint>, Vec<String>), String> {
    let labels = labels_of(s, Some(t));
    let to_line = |d: [f64; 3]| ohl_line::links::from_structure_axes(s.orientation_deg, d);
    let point = |label: &str, d: [f64; 3]| ReferencePoint {
        structure: s.id.clone(),
        label: label.to_owned(),
        displacement: to_line(d),
        force: [0.0; 3],
        strung: [0.0; 3],
    };
    let extension = s.extension.as_deref();
    if let Some((model, _)) = analyses.model_of(t, extension) {
        let model = model
            .as_ref()
            .map_err(|e| format!("its model could not be read: {e}"))?;
        // Its design code's plugin not found: not solved (2026-11-18).
        if let Some(e) = analyses.plugin_error_of(t, extension) {
            return Err(e.clone());
        }
        let feet = crate::feet::on_model(model, Some(s))?.unwrap_or_default();
        let u = ohl_structures_fe::condense::deflect_full(
            model,
            &ohl_structures_fe::condense::Linearisation::Unloaded,
            &[],
            &feet.prescribed,
            budget,
        )
        .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        let mut missing = Vec::new();
        for l in &labels {
            let link = crate::flexibility::placed_link(Some(s), l);
            match model.attachment_place(l, &link) {
                Some((n, offset)) => out.push(point(
                    l,
                    ohl_structures_fe::condense::place_displacement(u[n], offset),
                )),
                None => missing.push(l.clone()),
            }
        }
        let mut notes = feet.notes;
        if !missing.is_empty() {
            notes.push(format!(
                "NOT ON THE SAFE SIDE: point(s) {} on no joint of the model: kept as drawn, \
                 so the lean of the structure there is left out and the wire can come closer \
                 than shown; link the point(s) to a joint",
                missing.join(", ")
            ));
        }
        return Ok((out, notes));
    }
    if analyses.pole_of(&t.id).is_some() {
        let base = crate::feet::on_pole(Some(s))?.unwrap_or([0.0; 6]);
        let foot = ohl_line::feet::FootMovement {
            joint: ohl_line::feet::POLE_BASE.into(),
            translation: [base[0], base[1], base[2]],
            tilt_deg: [base[4].to_degrees(), (-base[3]).to_degrees()],
        };
        let out = labels
            .iter()
            .filter_map(|l| Some(point(l, foot.rigid_at(t.point_local(l, None)?))))
            .collect();
        return Ok((out, Vec::new()));
    }
    Ok((one_body_moves(Some(t), s)?, Vec::new()))
}

/// The moves of `s`'s points (type `t`, none: untyped — its own points)
/// as one body ([`crate::feet::as_one_body`]). Err: why not.
fn one_body_moves(
    t: Option<&ohl_structures::StructureType>,
    s: &ohl_line::Structure,
) -> Result<Vec<ReferencePoint>, String> {
    let Some(foot) = crate::feet::as_one_body(Some(s))? else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for l in labels_of(s, t) {
        let local = match t {
            Some(t) => t.point_local(&l, s.extension.as_deref()),
            None => s
                .point(&l)
                .map(|p| p.position)
                .or_else(|| s.wires.iter().find(|w| w.label == l).map(|w| w.position))
                .map(|p| {
                    let d = [
                        p[0] - s.position[0],
                        p[1] - s.position[1],
                        p[2] - s.position[2],
                    ];
                    ohl_line::links::to_structure_axes(s.orientation_deg, d)
                }),
        };
        let Some(r) = local else { continue };
        out.push(ReferencePoint {
            structure: s.id.clone(),
            label: l,
            displacement: ohl_line::links::from_structure_axes(s.orientation_deg, foot.rigid_at(r)),
            force: [0.0; 3],
            strung: [0.0; 3],
        });
    }
    Ok(out)
}
