// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Safety volumes for work near the line (theory: Safety volumes): the
//! space within a distance of the wires in every position of the weather
//! cases chosen — still air, blown out, at each wind step, anywhere on
//! the swing arc — as one closed triangle mesh per wire, per circuit or
//! per electrical system.
//!
//! The distance is a requirement by voltage ([`Requirement`]), taken at
//! the nominal voltage of each wire's system (an earthed wire at 0 kV),
//! plus an optional margin, measured from the conductor's surface (the
//! bundle's outer envelope and the polyline's bound, as every clearance).
//! The envelope is the union of
//!
//! - a **tube** around each solved wire: every point within
//!   \\( D = d + m + r_{\text{outer}} \\) of its polyline (capsules along
//!   it);
//! - the **swept arc**: every point within \\( D \\) of the swing arc's
//!   surface (the circle sections of radius \\( R(s) \\) between
//!   \\( \varphi_L(s) \\) and \\( \varphi_R(s) \\) about the span's axis,
//!   [`SwingArc`]), the surface cut in thin triangles whose distance to
//!   the closed form is added to \\( D \\);
//!
//! and its mesh ([`mesher`]) holds it whole: no point of the envelope is
//! outside the mesh, and the mesh is outside it by at most the tolerance
//! where it is smooth. Structures and jumpers are not included.
//!
//! The arc is read through [`SwingArc`]'s public closed form
//! ([`SwingArc::place`], [`SwingArc::point`]): a small adapter here, to
//! be shared once the arc code is one module.

pub mod mesher;
pub mod simplify;

#[cfg(test)]
mod system_tests;
#[cfg(test)]
mod tests;

use ohl_core::budget::Budget;
use ohl_line::Line;
use ohl_weather::WeatherCase;

use crate::geometry::{P3, closest_on_triangle};
use crate::positions::{PositionOptions, Positions, Shape, SwingArc, solve_positions};
use crate::rules::{Requirement, WindSteps};
pub use mesher::{Field, Mesh, MeshStats, Prim};
pub use simplify::SimplifyStats;

/// Added to every radius so that rounding the vertices to the millimetre
/// in an export cannot take the mesh inside the envelope, m.
pub const ROUNDING: f64 = 0.002;

/// The tolerance by default: how far the mesh may lie outside the
/// envelope where it is smooth, m.
pub const DEFAULT_TOLERANCE: f64 = 0.05;

/// The largest station step along a swing arc, m (stations are added
/// between the solved points further apart).
pub const ARC_STEP: f64 = 2.0;

/// How the wind of the windy cases is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindMode {
    /// The cases as solved: a windy case blown to the left and to the
    /// right at its full pressure.
    #[default]
    None,
    /// Also still air and every intermediate pressure ([`WindSteps`]).
    Steps,
    /// Also the swing arc between the two blowouts.
    Arc,
    /// Both.
    Both,
}

impl WindMode {
    /// Every mode.
    pub const ALL: [WindMode; 4] = [Self::None, Self::Steps, Self::Arc, Self::Both];

    /// `none`, `steps`, `arc` or `both`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Steps => "steps",
            Self::Arc => "arc",
            Self::Both => "both",
        }
    }

    /// In words.
    pub fn describe(self) -> &'static str {
        match self {
            Self::None => "blowouts at full wind",
            Self::Steps => "wind stepped",
            Self::Arc => "swing arc",
            Self::Both => "wind stepped and swing arc",
        }
    }

    /// Reads [`WindMode::as_str`].
    pub fn parse(text: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|m| m.as_str() == text.trim())
            .ok_or_else(|| format!("wind \"{text}\": use none, steps, arc or both"))
    }

    /// Whether the wind is stepped.
    pub fn stepped(self) -> bool {
        matches!(self, Self::Steps | Self::Both)
    }

    /// Whether the swing arc is taken.
    pub fn arc(self) -> bool {
        matches!(self, Self::Arc | Self::Both)
    }

    /// The positions solved, the wind stepped by `steps`.
    pub fn options(self, steps: WindSteps) -> PositionOptions {
        PositionOptions {
            swing_arc: self.arc(),
            wind_steps: self.stepped().then_some(WindSteps {
                stepped: true,
                ..steps
            }),
        }
    }
}

/// One volume per wire, per circuit or per system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Grouping {
    /// Per wire: a circuit's phase through all its sections.
    #[default]
    Wire,
    /// Per circuit: its phases together.
    Circuit,
    /// Per electrical system: its circuits together (earthed wires, which
    /// have none: per circuit).
    System,
}

impl Grouping {
    /// Every grouping.
    pub const ALL: [Grouping; 3] = [Self::Wire, Self::Circuit, Self::System];

    /// `wire`, `circuit` or `system`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Wire => "wire",
            Self::Circuit => "circuit",
            Self::System => "system",
        }
    }

    /// Reads [`Grouping::as_str`].
    pub fn parse(text: &str) -> Result<Self, String> {
        match text.trim() {
            "wire" => Ok(Self::Wire),
            "circuit" => Ok(Self::Circuit),
            "system" => Ok(Self::System),
            _ => Err(format!("per \"{text}\": use wire, circuit or system")),
        }
    }
}

/// A wire's electrical system: its id and the name a volume takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemOf {
    /// The system's id.
    pub id: String,
    /// Its name (its id when it has none).
    pub name: String,
}

/// The system of every section (by id) and every jumper (`jumper:<id>`,
/// its `from` section's) of `line`; earthed sections are left out. An
/// invalid line (contradicting assignments) gives none.
pub fn system_map(line: &Line) -> std::collections::BTreeMap<String, SystemOf> {
    let mut out = std::collections::BTreeMap::new();
    let Ok(phases) = line.section_phases() else {
        return out;
    };
    let of = |k: usize| {
        let s = &line.systems[k];
        SystemOf {
            id: s.id.clone(),
            name: if s.name.is_empty() {
                s.id.clone()
            } else {
                s.name.clone()
            },
        }
    };
    for (id, a) in &phases {
        if let Some((k, _)) = a {
            out.insert(id.clone(), of(*k));
        }
    }
    for j in &line.jumpers {
        if let Some(Some((k, _))) = line.jumper_section(j).and_then(|s| phases.get(&s.id)) {
            out.insert(format!("jumper:{}", j.id), of(*k));
        }
    }
    out
}

/// What a safety volume is made of.
#[derive(Debug, Clone, PartialEq)]
pub struct SafetyParams {
    /// The distance from the conductor's surface, by voltage or not (no
    /// default: the user's or a preset's).
    pub distance: Requirement,
    /// Added to the distance, m.
    pub margin: f64,
    /// How far the mesh may lie outside the envelope where it is smooth,
    /// m.
    pub tolerance: f64,
    /// One volume per wire, per circuit or per system.
    pub grouping: Grouping,
    /// The circuits taken (empty: all).
    pub circuits: Vec<String>,
    /// The systems taken, by id (empty: all; when given, earthed wires
    /// are left out).
    pub systems: Vec<String>,
    /// The spans taken: (section id, span index from 0) (empty: all). A
    /// span taken brings every wire on it; a jumper comes along when the
    /// spans at both its ends are taken (continuity).
    pub spans: Vec<(String, usize)>,
    /// The system of each section and jumper ([`system_map`]): filled by
    /// [`run`] from the line; needed by [`build`] for the systems taken
    /// and the grouping per system.
    pub system_of: std::collections::BTreeMap<String, SystemOf>,
}

impl Default for SafetyParams {
    fn default() -> Self {
        Self {
            // Empty: to be entered (no safety distance is built in).
            distance: Requirement::Fixed(0.0),
            margin: 0.0,
            tolerance: DEFAULT_TOLERANCE,
            grouping: Grouping::Wire,
            circuits: Vec::new(),
            systems: Vec::new(),
            spans: Vec::new(),
            system_of: Default::default(),
        }
    }
}

impl SafetyParams {
    /// What is wrong with them, if anything.
    pub fn problem(&self) -> Option<String> {
        if let Some(p) = self.distance.problem() {
            return Some(format!("distance: {p}"));
        }
        if !(self.margin.is_finite() && self.margin >= 0.0) {
            return Some("the margin must be ≥ 0 m".into());
        }
        if !(self.tolerance.is_finite() && (0.005..=1.0).contains(&self.tolerance)) {
            return Some("the tolerance must be between 0.005 and 1 m".into());
        }
        None
    }
}

/// One volume: a wire's or a circuit's.
#[derive(Debug, Clone, PartialEq)]
pub struct SafetyVolume {
    /// Its name: `circuit phase`, `circuit` or (per system) the system's
    /// name.
    pub name: String,
    /// Circuit (per system: its circuits, comma-separated).
    pub circuit: String,
    /// Its phases.
    pub phases: Vec<String>,
    /// Its sections.
    pub sections: Vec<String>,
    /// Nominal voltage of its system, kV (none: earthed).
    pub kv: Option<f64>,
    /// The distance d at that voltage, m (the largest, if its wires
    /// differ).
    pub distance: f64,
    /// The margin added, m.
    pub margin: f64,
    /// The largest conductor outer radius (bundle, surface, polyline
    /// bound), m.
    pub outer: f64,
    /// Positions taken.
    pub positions: usize,
    /// The largest distance of an arc cell's triangles from the closed
    /// form, doubled and added to that cell's radius, m (0 without arcs).
    pub arc_error: f64,
    /// The median of those, m.
    pub arc_error_median: f64,
    /// How far the mesh lies outside the envelope (the arcs' cells as
    /// grown), measured at every vertex, side middle and centroid, with
    /// the rounding allowance, m.
    pub allowance: f64,
    /// What the mesher did.
    pub stats: MeshStats,
    /// What the simplification did.
    pub simplified: SimplifyStats,
    /// The mesh, in the line's coordinates.
    pub mesh: Mesh,
    /// Time to mesh it, s.
    pub seconds: f64,
    /// Of which: the fine mesh, its simplification (with the check), s.
    pub timings: [f64; 2],
}

/// The volumes of a run.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SafetyVolumes {
    /// The volumes.
    pub volumes: Vec<SafetyVolume>,
    /// The weather cases, by name.
    pub cases: Vec<String>,
    /// Wire positions taken.
    pub positions: usize,
    /// Notes (sections not solved, arcs left out…).
    pub notes: Vec<String>,
}

/// The prims of one swing arc, of radius `radius`: the arc's surface cut
/// in triangles (rails at equal fractions of the angle, at every station
/// and every [`ARC_STEP`] between), each cell's two triangles grown by
/// twice the largest distance found from the closed form to them (at the
/// middle of the cell and of its sides). Returns the prims, the largest
/// and the median of those doubled distances.
pub fn arc_prims(arc: &SwingArc, radius: f64, tolerance: f64) -> (Vec<Prim>, f64, f64) {
    // Stations: the arc's own, refined.
    let mut stations: Vec<f64> = Vec::new();
    for w in arc.stations.windows(2) {
        let n = ((w[1] - w[0]) / ARC_STEP).ceil().max(1.0) as usize;
        for k in 0..n {
            stations.push(w[0] + (w[1] - w[0]) * k as f64 / n as f64);
        }
    }
    if let Some(&last) = arc.stations.last() {
        stations.push(last);
    }
    let places: Vec<_> = stations.iter().map(|&s| arc.place(s)).collect();
    let r_max = places.iter().map(|p| p.radius).fold(0.0, f64::max);
    let sweep = places.iter().map(|p| p.to - p.from).fold(0.0, f64::max);
    // Rails: a chord's sag R(1 − cos δ/2) of at most a quarter of the
    // tolerance.
    let sag = 0.25 * tolerance;
    let delta = if r_max > sag {
        2.0 * (1.0 - sag / r_max).acos()
    } else {
        std::f64::consts::PI
    };
    let rails = ((sweep / delta.max(1e-6)).ceil() as usize).clamp(1, 4000);
    let point = |k: usize, t: f64| {
        let p = &places[k];
        arc.point(p.centre, p.radius, p.from + t * (p.to - p.from))
    };
    let exact = |s: f64, t: f64| {
        let p = arc.place(s);
        arc.point(p.centre, p.radius, p.from + t * (p.to - p.from))
    };
    let mut prims = Vec::new();
    let mut errors = Vec::new();
    for k in 0..places.len().saturating_sub(1) {
        let s_mid = 0.5 * (stations[k] + stations[k + 1]);
        for j in 0..rails {
            let (t0, t1) = (j as f64 / rails as f64, (j + 1) as f64 / rails as f64);
            let (a, b, c, d) = (
                point(k, t0),
                point(k + 1, t0),
                point(k + 1, t1),
                point(k, t1),
            );
            let pair = [[a, b, c], [a, c, d]];
            // The closed form at the cell's middle and its sides'
            // middles, against the two triangles.
            let tm = 0.5 * (t0 + t1);
            let mut worst = 0.0f64;
            for q in [
                exact(s_mid, tm),
                exact(s_mid, t0),
                exact(s_mid, t1),
                exact(stations[k], tm),
                exact(stations[k + 1], tm),
            ] {
                let d = pair
                    .iter()
                    .map(|t| {
                        let c = closest_on_triangle(q, t[0], t[1], t[2]);
                        crate::geometry::distance(q, c)
                    })
                    .fold(f64::INFINITY, f64::min);
                worst = worst.max(d);
            }
            let e = 2.0 * worst;
            errors.push(e);
            for [x, y, z] in pair {
                prims.push(Prim::triangle(x, y, z, radius + e));
            }
        }
    }
    errors.sort_by(f64::total_cmp);
    let max = errors.last().copied().unwrap_or(0.0);
    let median = errors.get(errors.len() / 2).copied().unwrap_or(0.0);
    (prims, max, median)
}

/// The share of the tolerance taken by the mesher's level (the rest is
/// the simplification's).
const LEVEL_SHARE: f64 = 0.6;

/// The largest f at every vertex, side middle and centroid of `mesh`.
fn outward(mesh: &Mesh, field: &Field) -> f64 {
    let mut worst = 0.0f64;
    let f = |p: P3| field.value(p, f64::INFINITY);
    for v in &mesh.vertices {
        worst = worst.max(f(*v));
    }
    for t in &mesh.triangles {
        let [a, b, c] = mesh.corners(t);
        let m = |x: P3, y: P3| {
            [
                0.5 * (x[0] + y[0]),
                0.5 * (x[1] + y[1]),
                0.5 * (x[2] + y[2]),
            ]
        };
        worst = worst
            .max(f(m(a, b)))
            .max(f(m(b, c)))
            .max(f(m(c, a)))
            .max(f([
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ]));
    }
    worst
}

/// A wire's group: its circuit (an empty circuit: its section) and, per
/// wire, its phase; per system its system's name (earthed: its circuit).
fn group_of(
    wire: &crate::checks::Wire,
    grouping: Grouping,
    system: Option<&SystemOf>,
) -> (String, Option<String>) {
    let circuit = if wire.circuit.is_empty() {
        wire.section.clone()
    } else {
        wire.circuit.clone()
    };
    match grouping {
        Grouping::Circuit => (circuit, None),
        Grouping::Wire => (circuit, Some(wire.phase.clone())),
        Grouping::System => (system.map_or(circuit, |s| s.name.clone()), None),
    }
}

/// A span as typed (`SECTION:N`, N from 1, as shown) read as (section,
/// index from 0).
pub fn parse_span(text: &str) -> Result<(String, usize), String> {
    let bad = || format!("span \"{text}\": write SECTION:N (N from 1)");
    let (section, n) = text.trim().rsplit_once(':').ok_or_else(bad)?;
    let n: usize = n.trim().parse().map_err(|_| bad())?;
    if section.is_empty() || n == 0 {
        return Err(bad());
    }
    Ok((section.to_owned(), n - 1))
}

/// Whether `wire` is in the spans taken: a span's wire when its section
/// and index are taken; a jumper when both its ends (its first two
/// [`crate::checks::Wire::ends`]) are ends of spans taken (`ends`).
fn span_taken(
    wire: &crate::checks::Wire,
    spans: &[(String, usize)],
    ends: &std::collections::BTreeSet<&str>,
) -> bool {
    if spans.is_empty() {
        return true;
    }
    if wire.section.starts_with("jumper:") {
        return wire.ends.len() >= 2 && wire.ends[..2].iter().all(|e| ends.contains(e.as_str()));
    }
    spans
        .iter()
        .any(|(s, k)| *s == wire.section && *k == wire.span)
}

/// The volumes of the positions `positions` (solved by
/// [`solve_positions`]) by `params`.
///
/// Cost: per volume, about the area of its surface over the cell squared
/// values of the distance, each a tree search over its prims; exit
/// clause: `budget`.
pub fn build(
    positions: &Positions,
    params: &SafetyParams,
    budget: &Budget,
) -> Result<SafetyVolumes, String> {
    if let Some(p) = params.problem() {
        return Err(p);
    }
    let eps = params.tolerance;
    // The groups, in the order met.
    struct Group {
        key: (String, Option<String>),
        circuits: Vec<String>,
        phases: Vec<String>,
        sections: Vec<String>,
        kv: Option<f64>,
        distance: f64,
        outer: f64,
        positions: usize,
        prims: Vec<Prim>,
        arc_error: f64,
        arc_median: Vec<f64>,
        smallest: f64,
    }
    let mut groups: Vec<Group> = Vec::new();
    let mut notes = positions.notes.clone();
    // The ends of the spans taken: the jumpers between them come along.
    let ends: std::collections::BTreeSet<&str> = positions
        .spans
        .iter()
        .map(|s| &s.wire)
        .filter(|w| {
            !w.section.starts_with("jumper:")
                && params
                    .spans
                    .iter()
                    .any(|(s, k)| *s == w.section && *k == w.span)
        })
        .flat_map(|w| w.ends.iter().map(String::as_str))
        .collect();
    for p in &positions.positions {
        let span = &positions.spans[p.span];
        let wire = &span.wire;
        if !span_taken(wire, &params.spans, &ends) {
            continue;
        }
        if !params.circuits.is_empty() && !params.circuits.contains(&wire.circuit) {
            continue;
        }
        let system = params.system_of.get(&wire.section);
        if !params.systems.is_empty() && system.is_none_or(|s| !params.systems.contains(&s.id)) {
            continue;
        }
        let kv = wire.kv().unwrap_or(0.0);
        let d = params.distance.at(kv).map_err(|e| {
            format!(
                "{} (section {}, {} kV): {e}",
                wire.circuit, wire.section, kv
            )
        })?;
        let key = group_of(wire, params.grouping, system);
        let g = match groups.iter().position(|g| g.key == key) {
            Some(i) => i,
            None => {
                groups.push(Group {
                    key: key.clone(),
                    circuits: Vec::new(),
                    phases: Vec::new(),
                    sections: Vec::new(),
                    kv: wire.kv(),
                    distance: 0.0,
                    outer: 0.0,
                    positions: 0,
                    prims: Vec::new(),
                    arc_error: 0.0,
                    arc_median: Vec::new(),
                    smallest: f64::INFINITY,
                });
                groups.len() - 1
            }
        };
        let g = &mut groups[g];
        if !g.circuits.contains(&wire.circuit) {
            g.circuits.push(wire.circuit.clone());
        }
        if !g.phases.contains(&wire.phase) {
            g.phases.push(wire.phase.clone());
        }
        if !g.sections.contains(&wire.section) {
            g.sections.push(wire.section.clone());
        }
        g.distance = g.distance.max(d);
        g.positions += 1;
        // The radius without the rounding allowance (the proof's r).
        let base = d + params.margin;
        match &p.shape {
            Shape::Wire(w) => {
                let outer = w.outer();
                g.outer = g.outer.max(outer);
                let r = base + outer;
                g.smallest = g.smallest.min(r);
                for s in w.wire.points.windows(2) {
                    g.prims.push(Prim::segment(s[0], s[1], r + ROUNDING));
                }
                if w.wire.points.len() == 1 {
                    g.prims.push(Prim::segment(
                        w.wire.points[0],
                        w.wire.points[0],
                        r + ROUNDING,
                    ));
                }
            }
            Shape::Arc(a) => {
                g.outer = g.outer.max(a.outer);
                let r = base + a.outer;
                g.smallest = g.smallest.min(r);
                let (prims, error, median) = arc_prims(a, r + ROUNDING, eps);
                g.arc_error = g.arc_error.max(error);
                g.arc_median.push(median);
                g.prims.extend(prims);
            }
        }
    }
    if groups.is_empty() && !params.spans.is_empty() {
        return Err("no wire on the spans chosen (in the circuits or systems chosen)".into());
    }
    if groups.is_empty() {
        return Err(
            match (params.circuits.is_empty(), params.systems.is_empty()) {
                (true, true) => "no wire position to build a volume around".into(),
                (false, true) => format!("no wire of the circuits {}", params.circuits.join(", ")),
                (true, false) => format!("no wire of the systems {}", params.systems.join(", ")),
                (false, false) => format!(
                    "no wire of the circuits {} in the systems {}",
                    params.circuits.join(", "),
                    params.systems.join(", ")
                ),
            },
        );
    }
    let mut volumes = Vec::new();
    for mut g in groups {
        let t = std::time::Instant::now();
        // Local coordinates (from a whole metre near the first prim) for
        // the mesher; the mesh is moved back.
        let base = match g.prims.first().map(|p| p.shape) {
            Some(mesher::PrimShape::Segment([a, _]))
            | Some(mesher::PrimShape::Triangle([a, ..])) => a.map(f64::round),
            None => [0.0; 3],
        };
        let field = Field::new(
            g.prims
                .iter()
                .map(|p| p.moved([-base[0], -base[1], -base[2]]))
                .collect(),
        );
        let r = g.smallest;
        // The cell that keeps every triangle out of the envelope (see
        // the mesher), the rounding of an export allowed for.
        // The level takes 60 % of the tolerance, the simplification the
        // rest.
        let level = LEVEL_SHARE * eps;
        let cell = ((2.0 * r * level + level * level).sqrt() - ROUNDING)
            .min(0.99 * (r + level) / 3f64.sqrt())
            .max(1e-3);
        let (fine, stats) = mesher::mesh(&field, level, cell, budget)?;
        let t_mesh = t.elapsed().as_secs_f64();
        let (mut mesh, simplified) =
            simplify::simplify(&fine, &field, (1.0 - LEVEL_SHARE) * eps, eps, budget);
        let t_simplify = t.elapsed().as_secs_f64() - t_mesh;
        drop(fine);
        let outward = outward(&mesh, &field);
        for v in &mut mesh.vertices {
            for k in 0..3 {
                v[k] += base[k];
            }
        }
        if mesh.triangles.is_empty() {
            notes.push(format!("{}: no surface found", g.key.0));
        }
        let name = match &g.key.1 {
            Some(phase) if !phase.is_empty() => format!("{} {phase}", g.key.0),
            _ => g.key.0.clone(),
        };
        volumes.push(SafetyVolume {
            name,
            // Per system: the circuits it holds.
            circuit: if params.grouping == Grouping::System {
                g.circuits.join(", ")
            } else {
                g.key.0.clone()
            },
            phases: g.phases,
            sections: g.sections,
            kv: g.kv,
            distance: g.distance,
            margin: params.margin,
            outer: g.outer,
            positions: g.positions,
            arc_error: g.arc_error,
            arc_error_median: {
                g.arc_median.sort_by(f64::total_cmp);
                g.arc_median
                    .get(g.arc_median.len() / 2)
                    .copied()
                    .unwrap_or(0.0)
            },
            allowance: outward + ROUNDING,
            stats,
            simplified,
            mesh,
            seconds: t.elapsed().as_secs_f64(),
            timings: [t_mesh, t_simplify],
        });
    }
    Ok(SafetyVolumes {
        volumes,
        cases: positions.cases.clone(),
        positions: positions.positions.len(),
        notes,
    })
}

/// Solves `cases` on `line` (the wind by `wind` and `steps`) and builds
/// the volumes.
///
/// Cost: the solves of [`solve_positions`], then [`build`]; exit clause:
/// `budget`.
pub fn run(
    line: &Line,
    library: &ohl_cables::Library,
    cases: &[WeatherCase],
    wind: WindMode,
    steps: WindSteps,
    params: &SafetyParams,
    budget: &Budget,
) -> Result<SafetyVolumes, String> {
    if let Some(p) = params.problem() {
        return Err(p);
    }
    if cases.is_empty() {
        return Err("no weather case chosen".into());
    }
    // The systems taken by id or name; each wire's system from the line.
    let mut params = params.clone();
    for s in &mut params.systems {
        match line
            .systems
            .iter()
            .find(|x| x.id == *s)
            .or_else(|| line.systems.iter().find(|x| x.name == *s))
        {
            Some(x) => *s = x.id.clone(),
            None => return Err(format!("no system \"{s}\" in the line")),
        }
    }
    for (section, k) in &params.spans {
        let n = line
            .sections
            .iter()
            .find(|s| s.id == *section)
            .map(|s| s.points.len().saturating_sub(1))
            .ok_or_else(|| format!("no section \"{section}\" in the line"))?;
        if *k >= n {
            return Err(format!(
                "section {section} has {n} span{}: no span {}",
                if n == 1 { "" } else { "s" },
                k + 1
            ));
        }
    }
    params.system_of = system_map(line);
    let params = &params;
    let positions = solve_positions(line, library, cases, wind.options(steps), budget)?;
    build(&positions, params, budget)
}
