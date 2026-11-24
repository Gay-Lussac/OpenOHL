// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The magnetic field on a working set with the lines near it (ADR-0013,
//! phase 10; owner 2026-11-16: "fields pulling in lines within 500 m").
//!
//! The working set's line is computed as one (its currents' phases and
//! directions together, `ohl_electrical::magnetic`): the working set's
//! sections and this line's sections whose spans can come within the
//! field distance of its wires, profiles across the working set's spans.
//! Every other line with wires within the field distance is computed on
//! its own (its sections there, its own settings and currents) and its
//! field **added by size** at each point: two lines' phase angles to each
//! other are not known, and the size of a sum is at most the sum of the
//! sizes (conservative, said).
//!
//! **NOT ON THE SAFE SIDE**: the wires farther than the field distance
//! (of this line and of others) are left out — their field at the points
//! is not in the result, so the real field can be higher; a larger field
//! distance takes them in. Said in every result, with the distance
//! ([`beyond_note`]); the tests measure what it leaves out on the
//! synthetic country.

use std::collections::{BTreeMap, BTreeSet};

use ohl_electrical::Sources;
use ohl_electrical::magnetic::{self, CircuitCurrent, StationSpec};
use ohl_line::Line;
use ohl_weather::WeatherCase;
use serde::{Deserialize, Serialize};

use crate::bound::SegmentGrid;
use crate::{Neighbour, NeighbourKind, Neighbourhood, PulledIn, Reach, WorkingSet, solve_wires};

/// What a field check computes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldSpec {
    /// Lines within this distance of the checked wires are pulled in, m.
    pub distance: f64,
    /// The profiles' height above the ground, m.
    pub height: f64,
    /// To either side of the axis, m.
    pub width: f64,
    /// Between the profile's points, m.
    pub step: f64,
    /// The current of every circuit without an operating current of its
    /// own (its line's systems), A; none: 0 A (said).
    pub amps: Option<f64>,
    /// A limit, µT (none: none checked).
    pub limit_ut: Option<f64>,
}

impl Default for FieldSpec {
    fn default() -> Self {
        Self {
            distance: crate::reach::FIELD_DISTANCE,
            height: 1.0,
            width: 50.0,
            step: 5.0,
            amps: None,
            limit_ut: None,
        }
    }
}

/// The field at one point of a profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Point {
    /// Across the span's axis, m (right of the line, looking ahead).
    pub offset: f64,
    /// Where, m.
    pub at: [f64; 3],
    /// The working set's line's field, µT.
    pub own_ut: f64,
    /// The other lines', added by size, µT.
    pub others_ut: f64,
    /// Both, µT.
    pub b_ut: f64,
}

/// A profile across a span of the working set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    /// The span's structures.
    pub from: String,
    /// Its second structure.
    pub to: String,
    /// Its points.
    pub points: Vec<Point>,
    /// The largest field on it, µT.
    pub max_ut: f64,
}

/// A scoped field check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldResult {
    /// The reach it pulled in by.
    pub reach: Reach,
    /// Per span of the working set.
    pub profiles: Vec<Profile>,
    /// The largest field, µT.
    pub max_ut: f64,
    /// The limit, µT (none: none checked).
    pub limit_ut: Option<f64>,
    /// Points at or above the limit.
    pub above_limit: usize,
    /// What it pulled in.
    pub pulled: PulledIn,
    /// What it could not do: each makes it *not all checked*.
    pub not_all_checked: Vec<String>,
    /// What it assumed (NOT ON THE SAFE SIDE: the wires beyond the field
    /// distance left out).
    pub notes: Vec<String>,
}

/// The settings of the magnetic run for `line`: the spec's profiles, the
/// spec's current for every circuit without its own.
fn settings(line: &Line, spec: &FieldSpec, stations: Vec<StationSpec>) -> magnetic::Settings {
    let currents = match spec.amps {
        Some(a) => magnetic::circuits(line)
            .into_iter()
            .filter(|c| line.system_current(&c.system).is_none())
            .map(|c| CircuitCurrent {
                circuit: c.system.clone(),
                amps: a,
                reverse: false,
            })
            .collect(),
        None => Vec::new(),
    };
    magnetic::Settings {
        height: spec.height,
        width: spec.width,
        step: spec.step,
        profiles: !stations.is_empty(),
        stations,
        currents,
        limit_ut: spec.limit_ut,
        ..magnetic::Settings::default()
    }
}

/// A line's wires as the field's sources: solved in `case`.
pub(crate) fn solved(
    part: &Line,
    library: &ohl_cables::Library,
    case: &WeatherCase,
) -> (
    Vec<ohl_line::solve::SectionShape>,
    Vec<ohl_line::jumper::JumperShape>,
    Vec<String>,
) {
    let mut failed = Vec::new();
    let mut shapes = Vec::new();
    for (k, r) in ohl_line::solve::solve_line(part, library, case)
        .into_iter()
        .enumerate()
    {
        match r {
            Ok(s) => shapes.push(s),
            Err(e) => failed.push(format!("section {}: {e}", part.sections[k].id)),
        }
    }
    let jumpers = ohl_line::jumper::solve_jumpers(part, library, case, &shapes)
        .into_iter()
        .filter_map(Result::ok)
        .collect();
    (shapes, jumpers, failed)
}

/// One line's field computed: its profiles (at `stations`) and its wires.
pub fn line_field(
    part: &Line,
    library: &ohl_cables::Library,
    case: &WeatherCase,
    spec: &FieldSpec,
    stations: Vec<StationSpec>,
) -> Result<(magnetic::MagneticField, magnetic::Wires3d, Vec<String>), String> {
    let (shapes, jumpers, failed) = solved(part, library, case);
    let src = Sources {
        line: part,
        library,
        shapes: &shapes,
        temperature: case.temperature,
        ground: None,
        jumpers: &jumpers,
    };
    let (field, wires) = magnetic::run_keeping(&src, &settings(part, spec, stations), None)?;
    Ok((field, wires.unwrap_or_default(), failed))
}

/// The spans of `sections` of `line` as profile stations (at the lowest
/// point of each span's lowest phase conductor).
pub fn stations_of(line: &Line, sections: &BTreeSet<String>) -> Vec<StationSpec> {
    let mut out: Vec<StationSpec> = Vec::new();
    for s in line.sections.iter().filter(|s| sections.contains(&s.id)) {
        for w in s.points.windows(2) {
            let st = StationSpec {
                from: w[0].structure.clone(),
                to: w[1].structure.clone(),
                fraction: None,
            };
            if !out.iter().any(|x| {
                (x.from == st.from && x.to == st.to) || (x.from == st.to && x.to == st.from)
            }) {
                out.push(st);
            }
        }
    }
    out
}

/// The profiles' points with the other lines' fields added (µT, by size).
pub fn add_by_size(
    own: &magnetic::MagneticField,
    others: &[&magnetic::Wires3d],
    limit: Option<f64>,
) -> (Vec<Profile>, f64, usize) {
    let mut profiles = Vec::new();
    let mut max: f64 = 0.0;
    let mut above = 0;
    for p in &own.profiles {
        let at: Vec<[f64; 3]> = p.points.iter().map(|q| [q.x, q.y, q.z]).collect();
        let mut extra = vec![0.0; at.len()];
        for w in others {
            for (e, (b, _)) in extra.iter_mut().zip(w.field_block(&at)) {
                *e += b * 1e6;
            }
        }
        let points: Vec<Point> = p
            .points
            .iter()
            .zip(&extra)
            .map(|(q, e)| Point {
                offset: q.offset,
                at: [q.x, q.y, q.z],
                own_ut: q.b_ut,
                others_ut: *e,
                b_ut: q.b_ut + e,
            })
            .collect();
        let m = points.iter().map(|q| q.b_ut).fold(0.0, f64::max);
        max = max.max(m);
        if let Some(l) = limit {
            above += points.iter().filter(|q| q.b_ut >= l).count();
        }
        profiles.push(Profile {
            from: p.from.clone(),
            to: p.to.clone(),
            points,
            max_ut: m,
        });
    }
    (profiles, max, above)
}

/// Another line near a working set, its wires as the field's sources.
#[derive(Debug, Clone)]
pub struct OtherLine {
    /// The model (line).
    pub model: String,
    /// Its sections near (whose spans can come within the field distance
    /// of the working set's wires).
    pub sections: Vec<String>,
    /// Its wires solved in the case, with their currents.
    pub wires: magnetic::Wires3d,
}

/// The lines near a working set for its field, as the network's field
/// check selects and computes them ([`other_lines`]).
#[derive(Debug, Clone, Default)]
pub struct OtherLines {
    /// The working set's wires not solved (each makes it *not all
    /// checked*).
    pub wires_failed: Vec<String>,
    /// This line's sections near (computed with the working set as one).
    pub own_near: BTreeSet<String>,
    /// Every other line near, each computed on its own (its sections
    /// there, its own settings and currents).
    pub lines: Vec<OtherLine>,
    /// The other lines' sections pulled in.
    pub pulled: PulledIn,
    /// What could not be done for the other lines (each makes it *not all
    /// checked*).
    pub not_all_checked: Vec<String>,
    /// What was assumed for them.
    pub notes: Vec<String>,
}

/// The sections near a working set: whose spans can come within a
/// distance of its wires ([`near_sections`]).
#[derive(Debug, Clone, Default)]
pub struct Near {
    /// The working set's wires not solved (each makes it *not all
    /// checked*).
    pub wires_failed: Vec<String>,
    /// This line's sections near.
    pub own_near: BTreeSet<String>,
    /// Every other line's sections near, by line.
    pub others: BTreeMap<String, BTreeSet<String>>,
}

/// The sections of every line whose spans can come within `distance` of
/// the wires of working set `ws` solved in `case`, from `net`: the
/// network's selection for its field checks (the magnetic field's
/// [`other_lines`], the electric field's
/// [`crate::electric::joint_lines`]).
pub fn near_sections(
    net: &mut dyn Neighbourhood,
    ws: &WorkingSet,
    case: &WeatherCase,
    distance: f64,
) -> Result<Near, String> {
    if !(distance.is_finite() && distance >= 0.0) {
        return Err(format!("a field distance of {distance} m"));
    }
    let mut out = Near::default();
    let ws_sections: BTreeSet<String> = ws.sections.iter().cloned().collect();
    let library = net.library(&ws.model)?;
    // The working set's wires: where the distance is measured from.
    let part = net.part(&ws.model, &[], &ws.sections)?;
    let (own_wires, failed) =
        solve_wires(&part, &library, case, &ws_sections, Some(&BTreeSet::new()));
    out.wires_failed
        .extend(failed.into_iter().map(|f| format!("{f}: not computed")));
    let mut grid = SegmentGrid::new(100.0);
    for w in &own_wires {
        grid.add_polyline(&w.points);
    }
    let bounds = grid
        .bounds()
        .ok_or("the working set's wires have no place")?;
    let margin = net.max_margin();
    let area = bounds.grown(distance + 2.0 * margin);
    let mut by_model: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (m, s) in net.sections_in(&area) {
        by_model.entry(m).or_default().push(s);
    }
    for (m, secs) in &mut by_model {
        secs.sort();
        secs.dedup();
        for (s, cs) in net.chords(m, secs) {
            if cs.iter().any(|c| {
                grid.nearest(c.mid(), c.mid(), distance + c.margin)
                    .is_some()
            }) {
                out.others.entry(m.clone()).or_default().insert(s);
            }
        }
    }
    out.own_near = out.others.remove(&ws.model).unwrap_or_default();
    Ok(out)
}

/// The lines whose wires can come within `spec.distance` of the wires of
/// working set `ws` in `case`, from `net`: this line's sections near, and
/// every other line's sections near computed on its own as the field's
/// sources (see the module's notes). The network's field check
/// ([`fields`]) adds them by size to the working set's profiles;
/// OHL-Design's Magnetic field window on an offline working set's file
/// adds them to its own outputs (2026-11-24) — the same selection, the
/// same sources.
pub fn other_lines(
    net: &mut dyn Neighbourhood,
    ws: &WorkingSet,
    case: &WeatherCase,
    spec: &FieldSpec,
) -> Result<OtherLines, String> {
    let near = near_sections(net, ws, case, spec.distance)?;
    let mut out = OtherLines {
        wires_failed: near.wires_failed,
        ..OtherLines::default()
    };
    let keep = near.others;
    out.own_near = near.own_near;
    // The other lines, each on its own.
    for (m, secs) in keep {
        let list: Vec<String> = secs.iter().cloned().collect();
        let got = net
            .part(&m, &[], &list)
            .and_then(|p| net.library(&m).map(|l| (p, l)));
        let (p, lib) = match got {
            Ok(x) => x,
            Err(e) => {
                out.not_all_checked.push(format!(
                    "line {m}: {} section(s) near not read ({e}): their field left out",
                    list.len()
                ));
                continue;
            }
        };
        match line_field(&p, &lib, case, spec, Vec::new()) {
            Ok((f, wires, failed)) => {
                for x in failed {
                    out.not_all_checked
                        .push(format!("line {m}: {x}: its field left out"));
                }
                if f.notes.iter().any(|n| n.contains("no current given")) {
                    out.notes.push(format!(
                        "line {m}: a circuit without a current (0 A taken: its field is missing, \
                         so the real field can be higher) — give the check's current or the \
                         systems' operating currents"
                    ));
                }
                for s in &secs {
                    out.pulled
                        .read
                        .insert(Neighbour::new(&m, NeighbourKind::Section, s));
                    out.pulled
                        .used
                        .insert(Neighbour::new(&m, NeighbourKind::Section, s));
                }
                out.lines.push(OtherLine {
                    model: m,
                    sections: list,
                    wires,
                });
            }
            Err(e) => out
                .not_all_checked
                .push(format!("line {m}: its field not computed ({e}): left out")),
        }
    }
    Ok(out)
}

/// The note said whenever other lines' fields are added by size.
pub const ADDED_BY_SIZE: &str = "the other lines' fields added by size at each point (their \
                                 phase angles to this line's are not known: conservative)";

/// The note said with every scoped field: the wires beyond the field
/// distance left out (NOT ON THE SAFE SIDE) — [`beyond_words`].
pub fn beyond_note(distance: f64) -> String {
    beyond_words(
        &format!("the wires farther than {distance:.0} m from the working set's wires"),
        "a larger field distance takes them in",
    )
}

/// The words of every "wires beyond the field distance" note, magnetic
/// and electric alike (owner 2026-11-24: every NOT ON THE SAFE SIDE says
/// why): `left_out`, which wires are left out; why that is not on the
/// safe side (their field is missing, so the real field can be higher);
/// `way_out`, what takes them in.
pub fn beyond_words(left_out: &str, way_out: &str) -> String {
    format!(
        "NOT ON THE SAFE SIDE: {left_out} are left out (the field distance; owner 2026-11-16): \
         their field is not in the result, so the real field can be higher; {way_out}"
    )
}

/// The field of working set `ws` in weather case `case`, the lines within
/// `spec.distance` pulled in from `net` (see the module's notes).
///
/// Expected cost: the working set's line near it solved and computed at
/// the profiles; each other line's sections near solved and its wires
/// summed at the same points.
pub fn fields(
    net: &mut dyn Neighbourhood,
    ws: &WorkingSet,
    case: &WeatherCase,
    spec: &FieldSpec,
) -> Result<FieldResult, String> {
    let others = other_lines(net, ws, case, spec)?;
    let mut not_all = others.wires_failed.clone();
    let mut notes = vec![beyond_note(spec.distance)];
    let ws_sections: BTreeSet<String> = ws.sections.iter().cloned().collect();
    let library = net.library(&ws.model)?;
    let mut pulled = PulledIn::default();
    // The working set's line, with its sections near.
    let mut own_sections: BTreeSet<String> = others.own_near.clone();
    for s in own_sections.iter().filter(|s| !ws_sections.contains(*s)) {
        pulled
            .read
            .insert(Neighbour::new(&ws.model, NeighbourKind::Section, s));
        pulled
            .used
            .insert(Neighbour::new(&ws.model, NeighbourKind::Section, s));
    }
    own_sections.extend(ws_sections.iter().cloned());
    let list: Vec<String> = own_sections.into_iter().collect();
    let own_part = net.part(&ws.model, &[], &list)?;
    let stations = stations_of(&own_part, &ws_sections);
    let (own, _, failed) = line_field(&own_part, &library, case, spec, stations)?;
    not_all.extend(
        failed
            .into_iter()
            .map(|f| format!("line {}: {f}: its field left out", ws.model)),
    );
    notes.extend(own.notes.iter().cloned());
    not_all.extend(others.not_all_checked.iter().cloned());
    notes.extend(others.notes.iter().cloned());
    pulled.read.extend(others.pulled.read.iter().cloned());
    pulled.used.extend(others.pulled.used.iter().cloned());
    if !others.lines.is_empty() {
        notes.push(ADDED_BY_SIZE.into());
    }
    let refs: Vec<&magnetic::Wires3d> = others.lines.iter().map(|l| &l.wires).collect();
    let (profiles, max_ut, above_limit) = add_by_size(&own, &refs, spec.limit_ut);
    notes.sort();
    notes.dedup();
    Ok(FieldResult {
        reach: Reach::Lines {
            metres: spec.distance,
        },
        profiles,
        max_ut,
        limit_ut: spec.limit_ut,
        above_limit,
        pulled,
        not_all_checked: not_all,
        notes,
    })
}
