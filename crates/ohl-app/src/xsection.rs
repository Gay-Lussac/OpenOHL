// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The cross-section view: a cut across the line, at a station of an
//! alignment (perpendicular to it; at an angle point, along the bisector)
//! or at a structure (in the plane of its cross-arms), shown in a 3D
//! panel of its own (2026-10-22). The panel is the 3D view of the line
//! (the same renderer, layers, controls, tools and picking) with only
//! what lies within ± the depth of the plane (along the line) and ± the
//! width across drawn, culled and picked: a clip box ([`Clip`]) cuts the
//! surfaces, wires and points exactly at its sides, and only the chunks
//! and point nodes meeting it are built ([`crate::view3d::View3d::clip`]).
//!
//! A cut opens in its **front view** ([`front_camera`]): orthographic,
//! looking **ahead** (along increasing stations), perpendicular to the
//! plane — the left of the line on the left, offsets across horizontal,
//! heights vertical, 1:1. It then turns, pans, zooms and flies as the 3D
//! view does; "Back to the cut view" (Home) brings the front view back.
//! Moving the cut (◀ ▶, ⏮ ⏭, comma, full stop) carries the camera with
//! it ([`carry`]). Several cuts can be open, each its own tab
//! ([`CrossSections`]); C opens the window that makes one ([`Dialog`]).
//! The main 3D view outlines every cut's slab; a click on an outline
//! brings its panel forward.
//!
//! The hover readouts are where the pointer's ray meets the plane:
//! offset, height, height above the ground and the distance to the
//! nearest conductor crossing the plane ([`contents`]). The ground for
//! that comes from the ground surface cut by the plane, else from the
//! LiDAR points of the slab, found on a grid index of the points loaded
//! ([`PointGrid`]): only the cells meeting the slab are looked at.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use eframe::egui;
use ohl_line::route::AlignmentView;
use ohl_render::{Camera, Clip};

use crate::model::Model;
use crate::scene::Layer;
use crate::selection::Selection;
use crate::view3d::{CutOutline, View3d};

/// The depth a cut opens with: what lies this far before and behind the
/// plane (along the line) is shown, m.
pub const DEFAULT_DEPTH: f64 = 10.0;

/// A station this close to an alignment point (m) is cut along the
/// bisector of the angle there, as structures stand.
const AT_VERTEX: f64 = 0.01;

/// The side of the cells of the LiDAR index, m.
const CELL: f64 = 8.0;

/// A structure whose position lies farther than this beyond the slab
/// (m) is not looked at (no structure reaches this far from its
/// position).
const STRUCTURE_REACH: f64 = 100.0;

/// Where a cut is: at a station of its alignment, or at a structure.
#[derive(Debug, Clone, PartialEq)]
pub enum At {
    /// A station (m from the alignment's start).
    Station(f64),
    /// A structure, by id.
    Structure(String),
}

/// What a cut is made of.
#[derive(Debug, Clone, PartialEq)]
pub struct CutSpec {
    /// The alignment the stations are along (and the ahead direction).
    pub alignment: String,
    /// Where.
    pub at: At,
    /// Half the thickness of what is shown, along the line, m.
    pub depth: f64,
    /// Half the width across the line, m.
    pub width: f64,
}

/// The cut's plane, vertical: a point of it and the direction looked in
/// (ahead, a unit vector in plan).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plane {
    /// A point of the plane (on the axis), plan.
    pub origin: [f64; 2],
    /// The direction looked in: ahead along the alignment.
    pub ahead: [f64; 2],
}

impl Plane {
    /// The direction to the right of the line, looking ahead.
    pub fn right(&self) -> [f64; 2] {
        [self.ahead[1], -self.ahead[0]]
    }

    /// How far `p` lies ahead of the plane, m.
    pub fn along(&self, p: [f64; 3]) -> f64 {
        (p[0] - self.origin[0]) * self.ahead[0] + (p[1] - self.origin[1]) * self.ahead[1]
    }

    /// How far `p` lies right of the axis, m (left negative).
    pub fn offset(&self, p: [f64; 3]) -> f64 {
        let r = self.right();
        (p[0] - self.origin[0]) * r[0] + (p[1] - self.origin[1]) * r[1]
    }

    /// The plan point of the plane `offset` right of the axis.
    pub fn at(&self, offset: f64) -> [f64; 2] {
        let r = self.right();
        [
            self.origin[0] + offset * r[0],
            self.origin[1] + offset * r[1],
        ]
    }

    /// The direction looked in as a bearing: degrees clockwise from
    /// north (grid north).
    pub fn bearing_deg(&self) -> f64 {
        self.ahead[0]
            .atan2(self.ahead[1])
            .to_degrees()
            .rem_euclid(360.0)
    }
}

/// A cut placed on the line.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    /// Its plane.
    pub plane: Plane,
    /// Its station on the alignment (a structure's: where it projects).
    pub station: f64,
    /// The alignment's length, m.
    pub length: f64,
    /// At an angle point of the alignment (a station cut along the
    /// bisector).
    pub angle_point: bool,
}

/// The length of a polyline in plan, m.
fn plan_length(pts: &[[f64; 3]]) -> f64 {
    pts.windows(2)
        .map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]))
        .sum()
}

/// The direction ahead at `station` along `centre` (unit, plan), and
/// whether the station is at an angle point: there, the bisector of the
/// two segments' directions (the plane across the line is then the
/// bisector of the line angle, as a structure stands there).
pub fn direction_at(centre: &[[f64; 3]], station: f64) -> Option<([f64; 2], bool)> {
    let mut segs: Vec<(f64, f64, [f64; 2])> = Vec::new();
    let mut start = 0.0;
    for w in centre.windows(2) {
        let (dx, dy) = (w[1][0] - w[0][0], w[1][1] - w[0][1]);
        let len = dx.hypot(dy);
        if len <= 0.0 {
            continue;
        }
        segs.push((start, start + len, [dx / len, dy / len]));
        start += len;
    }
    let k = segs
        .iter()
        .position(|s| station <= s.1)
        .unwrap_or(segs.len().checked_sub(1)?);
    let s = segs[k];
    let bisector = |a: [f64; 2], b: [f64; 2]| {
        let v = [a[0] + b[0], a[1] + b[1]];
        let n = v[0].hypot(v[1]);
        if n < 1e-9 { b } else { [v[0] / n, v[1] / n] }
    };
    let turns = |a: [f64; 2], b: [f64; 2]| (a[0] * b[1] - a[1] * b[0]).abs() > 1e-9;
    if station - s.0 < AT_VERTEX && k > 0 {
        let a = segs[k - 1].2;
        return Some((bisector(a, s.2), turns(a, s.2)));
    }
    if s.1 - station < AT_VERTEX && k + 1 < segs.len() {
        let b = segs[k + 1].2;
        return Some((bisector(s.2, b), turns(s.2, b)));
    }
    Some((s.2, false))
}

/// Places cut `spec` on `line` (its alignments `views`).
pub fn place(
    line: &ohl_line::Line,
    views: &[AlignmentView],
    spec: &CutSpec,
) -> Result<Placed, String> {
    let view = views
        .iter()
        .find(|v| v.id == spec.alignment)
        .ok_or_else(|| format!("alignment {} is not in the line", spec.alignment))?;
    let centre = view.centreline(line);
    let length = plan_length(&centre);
    if centre.len() < 2 || length <= 0.0 {
        return Err(format!("alignment {} has no length", spec.alignment));
    }
    match &spec.at {
        At::Station(s) => {
            let station = s.clamp(0.0, length);
            let origin = ohl_line::route::point_at(&centre, station, 0.0);
            let (ahead, angle_point) = direction_at(&centre, station)
                .ok_or_else(|| format!("alignment {} has no direction", spec.alignment))?;
            Ok(Placed {
                plane: Plane { origin, ahead },
                station,
                length,
                angle_point,
            })
        }
        At::Structure(id) => {
            let st = line
                .structure(id)
                .ok_or_else(|| format!("structure {id} is not in the line"))?;
            // Its local x axis (its cross-arms) lies in the plane: to the
            // right of the line looking ahead.
            let (_, mut ahead) = ohl_line::links::axes(st.orientation_deg);
            let (station, _) = ohl_line::route::project(&centre, st.position);
            // Looking ahead along the alignment (a structure turned
            // round: the other way).
            if let Some((d, _)) = direction_at(&centre, station)
                && d[0] * ahead[0] + d[1] * ahead[1] < 0.0
            {
                ahead = [-ahead[0], -ahead[1]];
            }
            Ok(Placed {
                plane: Plane {
                    origin: [st.position[0], st.position[1]],
                    ahead,
                },
                station,
                length,
                angle_point: false,
            })
        }
    }
}

/// The pieces of polyline `pts` within the slab (|along| ≤ `depth`,
/// |offset| ≤ `width`), as (offset, height).
pub fn clip(plane: &Plane, pts: &[[f64; 3]], depth: f64, width: f64) -> Vec<Vec<[f64; 2]>> {
    let mut out = Vec::new();
    let mut run: Vec<[f64; 2]> = Vec::new();
    let mut end = |run: &mut Vec<[f64; 2]>| {
        if run.len() > 1 {
            out.push(std::mem::take(run));
        } else {
            run.clear();
        }
    };
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        // Liang–Barsky: the part of a + t (b − a), t ∈ [0, 1], inside.
        let (mut lo, mut hi) = (0.0f64, 1.0f64);
        for (v0, v1, lim) in [
            (plane.along(a), plane.along(b), depth),
            (plane.offset(a), plane.offset(b), width),
        ] {
            let d = v1 - v0;
            if d.abs() < 1e-12 {
                if v0.abs() > lim {
                    (lo, hi) = (1.0, 0.0);
                }
                continue;
            }
            let (t1, t2) = ((-lim - v0) / d, (lim - v0) / d);
            lo = lo.max(t1.min(t2));
            hi = hi.min(t1.max(t2));
        }
        if lo > hi {
            end(&mut run);
            continue;
        }
        let at = |t: f64| {
            let p = [0, 1, 2].map(|k| a[k] + t * (b[k] - a[k]));
            [plane.offset(p), p[2]]
        };
        let (p, q) = (at(lo), at(hi));
        let joins = run
            .last()
            .is_some_and(|l| (l[0] - p[0]).abs() < 1e-9 && (l[1] - p[1]).abs() < 1e-9);
        if !joins {
            end(&mut run);
            run.push(p);
        }
        run.push(q);
        if hi < 1.0 {
            end(&mut run);
        }
    }
    end(&mut run);
    out
}

/// Where polyline `pts` crosses the plane within ± `width` across:
/// (offset, height) of each crossing.
pub fn crossings(plane: &Plane, pts: &[[f64; 3]], width: f64) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    for w in pts.windows(2) {
        let (sa, sb) = (plane.along(w[0]), plane.along(w[1]));
        if (sa < 0.0) == (sb < 0.0) || sa == sb {
            continue;
        }
        let t = sa / (sa - sb);
        let p = [0, 1, 2].map(|k| w[0][k] + t * (w[1][k] - w[0][k]));
        let o = plane.offset(p);
        if o.abs() <= width {
            out.push([o, p[2]]);
        }
    }
    out
}

/// A wire in the cut.
#[derive(Debug, Clone, PartialEq)]
pub struct WireCut {
    /// What a click on it selects (a span, a jumper).
    pub select: Selection,
    /// Its label: circuit and phase (or what it is).
    pub label: String,
    /// Its colour.
    pub color: [u8; 3],
    /// Where it crosses the plane: (offset, height).
    pub crossings: Vec<[f64; 2]>,
    /// Its pieces within the slab, projected: (offset, height).
    pub curve: Vec<Vec<[f64; 2]>>,
}

/// A structure in the cut, projected: (offset, height).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StructureCut {
    /// Its id.
    pub id: String,
    /// Its members (levels 2 and 3, the pole's outline), else its stick
    /// figure.
    pub lines: Vec<Vec<[f64; 2]>>,
    /// Its strings (as solved in the case shown, else as given).
    pub strings: Vec<Vec<[f64; 2]>>,
    /// Its attachment points: label, where.
    pub points: Vec<(String, [f64; 2])>,
    /// The ends of its members (levels 2 and 3): its joints.
    pub joints: Vec<[f64; 2]>,
}

/// The wires and structures of a cut (made again every frame: the cut
/// follows edits and re-solves).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Contents {
    /// The wires (each span and jumper in each case drawn).
    pub wires: Vec<WireCut>,
    /// The structures.
    pub structures: Vec<StructureCut>,
}

/// A section's label in a cut: its circuit and phase.
fn wire_label(sec: &ohl_line::Section) -> String {
    let phase = sec.phase_label.trim();
    let circuit = sec.circuit_name.trim();
    match (circuit.is_empty(), phase.is_empty()) {
        (_, true) => sec.id.clone(),
        (true, false) => phase.to_owned(),
        (false, false) => format!("{circuit} {phase}"),
    }
}

/// The wires and structures of `model` in weather case `case` (the cases
/// drawn with it, both wind sides) within the slab.
///
/// Cost: one pass over the points of every span solved (a few hundred
/// thousand on a long line: about a millisecond) and the structures
/// near the plane.
pub fn contents(model: &Model, case: usize, plane: &Plane, depth: f64, width: f64) -> Contents {
    let line = &model.line;
    let mut out = Contents::default();
    // The strings as solved, by structure: top, bottom.
    let mut strings: HashMap<&str, Vec<[[f64; 3]; 2]>> = HashMap::new();
    for sec in &line.sections {
        let color = crate::scene::rgb(&line.section_color(sec));
        let label = wire_label(sec);
        for k in model.section_cases(&sec.id, case) {
            let Some(c) = model.cases.get(k) else {
                continue;
            };
            for solved in c.drawn() {
                let Some(shape) = solved.shapes.iter().find(|x| x.section == sec.id) else {
                    continue;
                };
                for (p, states) in sec.points.iter().zip(&shape.strings) {
                    for x in states {
                        strings
                            .entry(p.structure.as_str())
                            .or_default()
                            .push([x.top, x.bottom]);
                    }
                }
                for (span, s) in shape.spans.iter().enumerate() {
                    let crossings = crossings(plane, &s.points, width);
                    let curve = clip(plane, &s.points, depth, width);
                    if crossings.is_empty() && curve.is_empty() {
                        continue;
                    }
                    out.wires.push(WireCut {
                        select: Selection::Span {
                            section: sec.id.clone(),
                            span,
                        },
                        label: label.clone(),
                        color,
                        crossings,
                        curve,
                    });
                }
            }
        }
    }
    for k in model.shown(case) {
        let Some(c) = model.cases.get(k) else {
            continue;
        };
        for solved in c.drawn() {
            for j in &solved.jumpers {
                let crossings = crossings(plane, &j.points, width);
                let curve = clip(plane, &j.points, depth, width);
                if crossings.is_empty() && curve.is_empty() {
                    continue;
                }
                let (color, _) = crate::scene::jumper_lines(line, j, &solved.bundles);
                out.wires.push(WireCut {
                    select: Selection::Jumper(j.jumper.clone()),
                    label: format!("jumper {}", j.jumper),
                    color,
                    crossings,
                    curve,
                });
            }
        }
    }
    let near = |p: [f64; 3], margin: f64| {
        plane.along(p).abs() <= depth + margin && plane.offset(p).abs() <= width + margin
    };
    for st in &line.structures {
        if !near(st.position, STRUCTURE_REACH) {
            continue;
        }
        let mut cut = StructureCut {
            id: st.id.clone(),
            ..StructureCut::default()
        };
        let solved = strings.get(st.id.as_str());
        for (part, pts) in crate::scene::structure_lines(&model.bodies, st) {
            if part == crate::scene::Part::Insulator {
                // Given strings only where none is solved.
                if solved.is_none() {
                    cut.strings.extend(clip(plane, &pts, depth, width));
                }
            } else {
                cut.lines.extend(clip(plane, &pts, depth, width));
            }
        }
        for mut ring in crate::scene::placed_pole(&model.bodies, st) {
            if let Some(first) = ring.first().copied() {
                ring.push(first);
            }
            cut.lines.extend(clip(plane, &ring, depth, width));
        }
        for s in solved.into_iter().flatten() {
            cut.strings.extend(clip(plane, s, depth, width));
        }
        for p in &st.points {
            if near(p.position, 0.0) {
                cut.points
                    .push((p.label.clone(), [plane.offset(p.position), p.position[2]]));
            }
        }
        for (_, m) in crate::scene::placed_members(&model.bodies, st) {
            for e in [m.first(), m.last()].into_iter().flatten() {
                if near(*e, 0.0) {
                    cut.joints.push([plane.offset(*e), e[2]]);
                }
            }
        }
        if !(cut.lines.is_empty() && cut.strings.is_empty() && cut.points.is_empty()) {
            out.structures.push(cut);
        }
    }
    out
}

/// What a grid index was made for: the points' address and number, the
/// model's origin.
type GridKey = (usize, usize, [u64; 3]);

/// A grid index of the LiDAR points loaded (the model's): the points of
/// each [`CELL`] square, so a slab query looks only at the cells meeting
/// the slab, never at the whole cloud.
#[derive(Debug, Default)]
pub struct PointGrid {
    key: GridKey,
    cells: HashMap<(i32, i32), Vec<u32>>,
}

/// A LiDAR point of a slab: (offset, height), class, the point (mm from
/// the model's origin).
pub type SlabPoint = ([f64; 2], u8, [i32; 3]);

/// The LiDAR points and the ground of a slab.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Slab {
    /// The LiDAR points: (offset, height), class, the point.
    pub lidar: Vec<SlabPoint>,
    /// The ground cut by the plane: runs of (offset, height).
    pub ground: Vec<Vec<[f64; 2]>>,
    /// Where the ground comes from, in words.
    pub ground_from: String,
    /// How many points the query looked at (those of the cells meeting
    /// the slab).
    pub tested: usize,
}

fn grid_key(model: &Model) -> GridKey {
    (
        model.points.as_ptr() as usize,
        model.points.len(),
        model.origin.map(f64::to_bits),
    )
}

impl PointGrid {
    /// The index of the points of `model`.
    ///
    /// Cost: one pass over the points loaded (2 million: ~50 ms), once;
    /// made again when the points change.
    pub fn new(model: &Model) -> Self {
        let mut cells: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for (k, (q, _)) in model.points.iter().enumerate() {
            let cell = (
                (f64::from(q[0]) / 1000.0 / CELL).floor() as i32,
                (f64::from(q[1]) / 1000.0 / CELL).floor() as i32,
            );
            cells.entry(cell).or_default().push(k as u32);
        }
        Self {
            key: grid_key(model),
            cells,
        }
    }

    /// The points of `model` within the slab of `plane` (|along| ≤
    /// `depth`, |offset| ≤ `width`), and how many were looked at.
    pub fn slab(
        &self,
        model: &Model,
        plane: &Plane,
        depth: f64,
        width: f64,
    ) -> (Vec<SlabPoint>, usize) {
        let (o, a, r) = (plane.origin, plane.ahead, plane.right());
        // The slab's corners, relative to the model's origin.
        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        for (s, t) in [(-1.0, -1.0), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)] {
            for k in 0..2 {
                let v = o[k] - model.origin[k] + s * depth * a[k] + t * width * r[k];
                lo[k] = lo[k].min(v);
                hi[k] = hi[k].max(v);
            }
        }
        let cell = |v: f64| (v / CELL).floor() as i32;
        let half = CELL * std::f64::consts::FRAC_1_SQRT_2;
        let mut out = Vec::new();
        let mut tested = 0;
        for i in cell(lo[0])..=cell(hi[0]) {
            for j in cell(lo[1])..=cell(hi[1]) {
                let Some(items) = self.cells.get(&(i, j)) else {
                    continue;
                };
                // A cell wholly outside the slab is passed.
                let c = [
                    model.origin[0] + (f64::from(i) + 0.5) * CELL,
                    model.origin[1] + (f64::from(j) + 0.5) * CELL,
                    0.0,
                ];
                if plane.along(c).abs() > depth + half || plane.offset(c).abs() > width + half {
                    continue;
                }
                for &k in items {
                    let Some((q, class)) = model.points.get(k as usize) else {
                        continue;
                    };
                    tested += 1;
                    let p = [0, 1, 2].map(|n| model.origin[n] + f64::from(q[n]) / 1000.0);
                    if plane.along(p).abs() <= depth {
                        let off = plane.offset(p);
                        if off.abs() <= width {
                            out.push(([off, p[2]], *class, *q));
                        }
                    }
                }
            }
        }
        (out, tested)
    }
}

/// The slab of a cut: its LiDAR points (from the index) and the ground:
/// the ground surface cut by the plane, else the lowest LiDAR ground
/// point of every half metre across.
pub fn slab(model: &Model, grid: &PointGrid, plane: &Plane, depth: f64, width: f64) -> Slab {
    let (lidar, tested) = grid.slab(model, plane, depth, width);
    let mut out = Slab {
        lidar,
        tested,
        ..Slab::default()
    };
    if let Some(g) = model.ground.as_deref() {
        let (a, b) = (plane.at(-width), plane.at(width));
        out.ground = g
            .profile(a, b)
            .into_iter()
            .map(|run| {
                run.into_iter()
                    .map(|gp| [-width + 2.0 * width * gp.t, gp.point[2]])
                    .collect::<Vec<_>>()
            })
            .filter(|r| r.len() > 1)
            .collect();
        out.ground_from = if model.ground_source.is_empty() {
            "the ground surface".into()
        } else {
            // Its name (a file: its name alone).
            let name = std::path::Path::new(&model.ground_source)
                .file_name()
                .map_or_else(
                    || model.ground_source.clone(),
                    |n| n.to_string_lossy().into_owned(),
                );
            format!("the ground surface ({name})")
        };
    } else {
        const BIN: f64 = 0.5;
        let codes = model.scheme.codes_with_role("ground");
        let mut lowest: BTreeMap<i64, f64> = BTreeMap::new();
        for (p, c, _) in &out.lidar {
            if codes.contains(c) {
                let k = (p[0] / BIN).floor() as i64;
                let z = lowest.entry(k).or_insert(p[1]);
                *z = z.min(p[1]);
            }
        }
        let mut run: Vec<[f64; 2]> = Vec::new();
        let mut last: Option<i64> = None;
        for (k, z) in lowest {
            // Broken over more than 2 m without a ground point.
            if last.is_some_and(|l| (k - l) as f64 * BIN > 2.0) && !run.is_empty() {
                let r = std::mem::take(&mut run);
                if r.len() > 1 {
                    out.ground.push(r);
                }
            }
            run.push([(k as f64 + 0.5) * BIN, z]);
            last = Some(k);
        }
        if run.len() > 1 {
            out.ground.push(run);
        }
        out.ground_from = "the LiDAR ground (lowest point every 0.5 m)".into();
    }
    out
}

/// The ground's height at `offset` in the runs (none: no ground there).
pub fn ground_at(ground: &[Vec<[f64; 2]>], offset: f64) -> Option<f64> {
    for run in ground {
        for w in run.windows(2) {
            let (a, b) = (w[0], w[1]);
            let (lo, hi) = (a[0].min(b[0]), a[0].max(b[0]));
            if offset >= lo && offset <= hi {
                if hi - lo < 1e-12 {
                    return Some(a[1].min(b[1]));
                }
                let t = (offset - a[0]) / (b[0] - a[0]);
                return Some(a[1] + t * (b[1] - a[1]));
            }
        }
    }
    None
}

/// The hover readouts of a cut.
#[derive(Debug, Clone, PartialEq)]
pub struct Readout {
    /// Offset across the line, m (right positive).
    pub offset: f64,
    /// Height, m.
    pub height: f64,
    /// Height above the ground, m (none: no ground there).
    pub above_ground: Option<f64>,
    /// The nearest conductor where it crosses the plane: its label and
    /// the distance in the plane, m.
    pub nearest: Option<(String, f64)>,
}

/// What the slab was made for: the plane, depth, width (bits), the
/// index, the ground surface, a serial counted up when it is forgotten.
type SlabKey = ([u64; 6], GridKey, usize, u64);

/// The vertical field of view of a cut's camera (the main view's).
const FOV_Y_DEG: f64 = 45.0;

/// The camera of the front view of a cut: orthographic, looking ahead
/// along the line, level (perpendicular to the plane); the whole width
/// and the heights `heights` (lowest, highest) in view in a view of
/// `aspect` (width / height). Offsets across are horizontal (right of the
/// line on the right), heights vertical, at the same scale.
pub fn front_camera(plane: &Plane, heights: [f64; 2], width: f64, aspect: f64) -> Camera {
    // Room below, and more above (the panel's title is written there).
    let range = (heights[1] - heights[0]).max(5.0);
    let (z0, z1) = (heights[0] - 0.08 * range, heights[1] + 0.25 * range);
    let fov_y = FOV_Y_DEG.to_radians();
    let half = (0.5 * (z1 - z0)).max(1.1 * width / aspect.max(0.05));
    Camera {
        target: [plane.origin[0], plane.origin[1], 0.5 * (z0 + z1)],
        distance: (half / (0.5 * fov_y).tan()).max(1.0),
        // From the target to the eye: behind the plane.
        yaw: (-plane.ahead[1]).atan2(-plane.ahead[0]),
        pitch: 0.0,
        fov_y,
        exaggeration: 1.0,
        ortho: true,
    }
}

/// Whether `camera` shows the front view of `plane`: orthographic, level,
/// looking ahead.
pub fn is_front(camera: &Camera, plane: &Plane) -> bool {
    let yaw = (-plane.ahead[1]).atan2(-plane.ahead[0]);
    let turn = (camera.yaw - yaw).rem_euclid(std::f64::consts::TAU);
    camera.ortho
        && camera.pitch.abs() < 1e-9
        && (turn < 1e-9 || turn > std::f64::consts::TAU - 1e-9)
}

/// Moves `camera` with the cut from plane `from` to `to`: it keeps its
/// pose relative to the plane (as far ahead and across of it, turned as
/// much from it), its height moved by `dz`.
pub fn carry(camera: &mut Camera, from: &Plane, to: &Plane, dz: f64) {
    let t = camera.target;
    let (a, o) = (from.along(t), from.offset(t));
    let r = to.right();
    camera.target = [
        to.origin[0] + a * to.ahead[0] + o * r[0],
        to.origin[1] + a * to.ahead[1] + o * r[1],
        t[2] + dz,
    ];
    let bearing = |p: &Plane| p.ahead[1].atan2(p.ahead[0]);
    camera.yaw += bearing(to) - bearing(from);
}

/// Where the ray from `a` along `d` meets the plane: (offset, height);
/// none when it runs along the plane.
pub fn on_plane(plane: &Plane, a: [f64; 3], d: [f64; 3]) -> Option<[f64; 2]> {
    let towards = d[0] * plane.ahead[0] + d[1] * plane.ahead[1];
    if towards.abs() < 1e-6 {
        return None;
    }
    let t = -plane.along(a) / towards;
    let p = [0, 1, 2].map(|k| a[k] + t * d[k]);
    Some([plane.offset(p), p[2]])
}

/// One cut and its panel: a 3D view of the line clipped to the slab.
pub struct CutView {
    /// What it is.
    pub spec: CutSpec,
    /// Where it is (none: it could not be placed, see `error`).
    pub placed: Option<Placed>,
    /// Why it could not be placed.
    pub error: Option<String>,
    /// The step of ◀ ▶, m (none: twice the depth).
    pub step: Option<f64>,
    /// Its 3D view: the main view's layers, drawn, culled and picked
    /// only within the slab ([`View3d::clip`]).
    pub view: View3d,
    /// The front view is set up in the next frame (opened, "Back to the
    /// cut view", Home).
    pub to_front: bool,
    /// The overlays of the main view are taken in the next frame (all of
    /// them: opened).
    fresh: bool,
    /// The plane the camera was placed for: when the cut moves, the
    /// camera keeps its pose relative to the plane.
    plane_shown: Option<Plane>,
    /// The front view as set up, and the panel's aspect it was set up
    /// for: while the camera is still that one, a panel that settles to
    /// another shape (its first frames) gets the front view again.
    fitted: Option<(Camera, f64)>,
    /// The front view on the wires (true) or the whole cut (false); none:
    /// on the wires while a field is drawn in the panel (owner,
    /// 2026-10-31: across a valley the whole cut left the field's
    /// isolines a few pixels at the top).
    pub on_wires: Option<bool>,
    /// Whether the front view set up was the one on the wires.
    fitted_wires: bool,
    slab: Option<(SlabKey, Arc<Slab>)>,
    /// The wires and structures of the last frame.
    pub contents: Contents,
    /// The heights from the ground to the top of what the slab holds
    /// (the last frame).
    pub heights: Option<[f64; 2]>,
    /// Where the view was drawn.
    pub rect: Option<egui::Rect>,
    /// The frame it was last drawn in (its tab in front).
    pub drawn_frame: Option<u64>,
    /// What the pointer is over.
    pub readout: Option<Readout>,
}

/// What to do with a cut, from its buttons and keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// One step back (◀, comma).
    Back,
    /// One step ahead (▶, full stop).
    Ahead,
    /// To the structure before.
    PreviousStructure,
    /// To the structure after.
    NextStructure,
}

impl CutView {
    /// A view of cut `spec`: every layer to build, in the front view.
    pub fn new(spec: CutSpec) -> Self {
        let mut view = View3d::default();
        view.dirty.receive(Layer::ALL);
        Self {
            spec,
            placed: None,
            error: None,
            step: None,
            view,
            to_front: true,
            fresh: true,
            plane_shown: None,
            fitted: None,
            on_wires: None,
            fitted_wires: false,
            slab: None,
            contents: Contents::default(),
            heights: None,
            rect: None,
            drawn_frame: None,
            readout: None,
        }
    }

    /// The step of ◀ ▶, m.
    pub fn step_length(&self) -> f64 {
        self.step.unwrap_or(2.0 * self.spec.depth).max(0.1)
    }

    /// Back to the front view in the next frame.
    pub fn front(&mut self) {
        self.to_front = true;
    }

    /// The slab's box (none: not placed).
    pub fn clip(&self) -> Option<Clip> {
        let p = self.placed.as_ref()?;
        Some(Clip {
            origin: [p.plane.origin[0], p.plane.origin[1], 0.0],
            ahead: p.plane.ahead,
            depth: self.spec.depth,
            width: self.spec.width,
        })
    }

    /// The slab's outline for the main view (none: not placed or not
    /// drawn yet).
    pub fn outline(&self, id: u32) -> Option<CutOutline> {
        let clip = self.clip()?;
        let [z0, z1] = self.heights?;
        let c = clip.corners();
        let at = |k: usize, z: f64| [c[k][0], c[k][1], z];
        Some(CutOutline {
            id,
            corners: [
                at(0, z0),
                at(1, z0),
                at(2, z0),
                at(3, z0),
                at(0, z1),
                at(1, z1),
                at(2, z1),
                at(3, z1),
            ],
            label: format!("Cut {}", self.place_text().replace("structure ", "")),
        })
    }

    /// Where the cut is, in words: "structure T3" or "station 0+155.0".
    pub fn place_text(&self) -> String {
        match &self.spec.at {
            At::Structure(id) => format!("structure {id}"),
            At::Station(s) => format!(
                "station {}",
                ohl_drawing::format_station(self.placed.as_ref().map_or(*s, |p| p.station))
            ),
        }
    }

    /// Moves the cut (`views`: the line's alignments).
    pub fn move_by(&mut self, line: &ohl_line::Line, views: &[AlignmentView], how: Step) {
        let Some(p) = self.placed.clone() else {
            return;
        };
        let at = match how {
            Step::Back => At::Station((p.station - self.step_length()).max(0.0)),
            Step::Ahead => At::Station((p.station + self.step_length()).min(p.length)),
            Step::PreviousStructure | Step::NextStructure => {
                let Some(view) = views.iter().find(|v| v.id == self.spec.alignment) else {
                    return;
                };
                let centre = view.centreline(line);
                let mut on: Vec<(f64, &String)> = view
                    .run()
                    .filter_map(|id| {
                        let st = line.structure(id)?;
                        Some((ohl_line::route::project(&centre, st.position).0, id))
                    })
                    .collect();
                on.sort_by(|a, b| a.0.total_cmp(&b.0));
                let found = if how == Step::NextStructure {
                    on.iter().find(|(s, _)| *s > p.station + AT_VERTEX)
                } else {
                    on.iter().rev().find(|(s, _)| *s < p.station - AT_VERTEX)
                };
                match found {
                    Some((_, id)) => At::Structure((*id).clone()),
                    None => return,
                }
            }
        };
        self.spec.at = at;
    }
}

/// Takes the main view's overlays of `layers` into a cut's view (the
/// clearance marks, the vegetation found, the safety volumes, the wire
/// being strung, the lines on the ground).
fn take_overlays(view: &mut View3d, main: &View3d, layers: &HashSet<Layer>) {
    if layers.contains(&Layer::Clearances) {
        view.marks.clone_from(&main.marks);
        view.marks_stale = main.marks_stale;
    }
    if [Layer::GrowIn, Layer::FallIn, Layer::Isoclearance]
        .iter()
        .any(|l| layers.contains(l))
    {
        view.vegetation.clone_from(&main.vegetation);
    }
    if layers.contains(&Layer::SafetyVolume) {
        view.safety_volume.clone_from(&main.safety_volume);
    }
    if layers.contains(&Layer::MagneticField) {
        view.magnetic_field.clone_from(&main.magnetic_field);
    }
    if layers.contains(&Layer::ElectricField) {
        view.electric_field.clone_from(&main.electric_field);
    }
    if layers.contains(&Layer::Draft) {
        view.draft.clone_from(&main.draft);
        view.removed.clone_from(&main.removed);
    }
    if layers.contains(&Layer::GroundLines) {
        view.ground_lines.clone_from(&main.ground_lines);
    }
    if layers.contains(&Layer::Points) {
        view.forget_stream();
    }
}

/// The cross-sections open, the window that opens them, the LiDAR index
/// they share.
#[derive(Default)]
pub struct CrossSections {
    /// The cuts open, by their tab's number.
    pub views: BTreeMap<u32, CutView>,
    next: u32,
    /// The window making a cut, when open.
    pub dialog: Option<Dialog>,
    grid: Option<Arc<PointGrid>>,
    alignments: crate::plan::AlignmentViews,
    /// Counted up when the slabs are forgotten.
    serial: u64,
}

/// What a cut's panel works with: the application's state the 3D view
/// takes, and the main 3D view (its controls, its overlays, its chunk
/// indexes are shared).
pub struct PanelCtx<'a> {
    /// The GPU (none: no image).
    pub rs: Option<&'a eframe::egui_wgpu::RenderState>,
    /// The line.
    pub model: &'a Model,
    /// The weather case shown.
    pub case: usize,
    /// The selection, shared with the other views.
    pub selection: &'a mut Selection,
    /// The layers shown.
    pub visible: &'a HashSet<Layer>,
    /// What a click may pick.
    pub snaps: &'a crate::selection::Snaps,
    /// The level-of-detail settings.
    pub lod: &'a crate::lod::LodSettings,
    /// Select is the tool held.
    pub select_held: bool,
    /// The main 3D view.
    pub main: &'a mut View3d,
    /// The Link tool's overlay (a point chosen).
    pub link: Option<crate::link_tool::LinkOverlay>,
}

impl CrossSections {
    /// Opens a cut: its tab's number.
    pub fn open(&mut self, spec: CutSpec) -> u32 {
        let id = self.next;
        self.next += 1;
        self.views.insert(id, CutView::new(spec));
        id
    }

    /// Forgets the slabs (the ground or the points changed).
    pub fn invalidate(&mut self) {
        self.serial += 1;
    }

    /// Forgets the slabs and the LiDAR index (points moved).
    pub fn invalidate_points(&mut self) {
        self.serial += 1;
        self.grid = None;
    }

    /// Hands the layers the main view marked for rebuilding since the
    /// last call on to every cut's view, with their overlays; whether
    /// there were any (and a cut open).
    pub fn hand_on(&mut self, main: &mut View3d) -> bool {
        let log = main.dirty.take_log();
        if log.is_empty() || self.views.is_empty() {
            return false;
        }
        for v in self.views.values_mut() {
            take_overlays(&mut v.view, main, &log);
            v.view.dirty.receive(log.iter().copied());
        }
        true
    }

    /// The outlines of the cuts' slabs (for the main view).
    pub fn outlines(&self) -> Vec<CutOutline> {
        self.views
            .iter()
            .filter_map(|(id, v)| v.outline(*id))
            .collect()
    }

    /// Frees the mouse held by a cut's view.
    pub fn free_mouse(&mut self, ctx: &egui::Context) {
        for v in self.views.values_mut() {
            if v.view.captured {
                View3d::set_captured(&mut v.view.captured, ctx, false);
            }
        }
    }

    /// The line's alignments.
    pub fn alignments(&mut self, line: &ohl_line::Line) -> Vec<AlignmentView> {
        self.alignments.get(line).to_vec()
    }

    /// The title of cut `id`'s tab.
    pub fn title(&self, id: u32) -> String {
        self.views.get(&id).map_or_else(
            || "Cross-section".to_owned(),
            |v| format!("Cut {}", v.place_text().replace("structure ", "")),
        )
    }

    /// The index of the points of `model` (made when missing or stale).
    pub fn grid(&mut self, model: &Model) -> Arc<PointGrid> {
        match &self.grid {
            Some(g) if g.key == grid_key(model) => g.clone(),
            _ => {
                let g = Arc::new(PointGrid::new(model));
                self.grid = Some(g.clone());
                g
            }
        }
    }

    /// Opens the window that makes a cut: at the structure selected, else
    /// mid-span of the span selected, else the start of `alignment`.
    pub fn open_dialog(
        &mut self,
        model: &Model,
        selection: &Selection,
        alignment: Option<String>,
        width: f64,
    ) {
        let views = self.alignments(&model.line);
        self.dialog = Some(Dialog::new(model, &views, selection, alignment, width));
    }

    /// Shows cut `id` in its panel: what the tool button was used on.
    pub fn ui(
        &mut self,
        id: u32,
        ui: &mut egui::Ui,
        cx: PanelCtx<'_>,
    ) -> Option<crate::selection::Use> {
        let views = self.alignments(&cx.model.line);
        let grid = self.grid(cx.model);
        let serial = self.serial;
        let Some(view) = self.views.get_mut(&id) else {
            ui.label("This cut is closed.");
            return None;
        };
        panel_ui(view, ui, cx, &views, &grid, serial)
    }
}

/// A length for a scale bar about `target` points long at `m` metres per
/// point: 1, 2 or 5 × a power of ten.
fn scale_length(m: f64, target: f64) -> f64 {
    let want = (m * target).max(1e-6);
    let p = 10f64.powf(want.log10().floor());
    [1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .map(|k| k * p)
        .rfind(|l| *l <= want)
        .unwrap_or(p)
}

/// The bar of a cut's panel: move the cut, its depth and width, back to
/// the front view, the projection. What to move it by.
fn bar_ui(ui: &mut egui::Ui, view: &mut CutView) -> Option<Step> {
    let mut step = None;
    ui.horizontal_wrapped(|ui| {
        for (text, hover, how) in [
            ("⏮", "To the structure before", Step::PreviousStructure),
            ("◀", "One step back along the line (comma)", Step::Back),
            (
                "▶",
                "One step ahead along the line (full stop)",
                Step::Ahead,
            ),
            ("⏭", "To the next structure", Step::NextStructure),
        ] {
            if ui.button(text).on_hover_text(hover).clicked() {
                step = Some(how);
            }
        }
        ui.label("step");
        let mut s = view.step_length();
        if ui
            .add(
                egui::DragValue::new(&mut s)
                    .range(0.1..=10_000.0)
                    .speed(0.5)
                    .max_decimals(1)
                    .suffix(" m"),
            )
            .on_hover_text("How far ◀ ▶ move the cut (default: twice the depth)")
            .changed()
        {
            view.step = Some(s);
        }
        ui.separator();
        ui.label("depth ±");
        crate::style::field(
            ui,
            egui::DragValue::new(&mut view.spec.depth)
                .range(0.1..=1000.0)
                .speed(0.5)
                .max_decimals(1)
                .suffix(" m"),
        )
        .on_hover_text("What lies this far before and behind the plane, along the line, is shown");
        ui.label("width ±");
        crate::style::field(
            ui,
            egui::DragValue::new(&mut view.spec.width)
                .range(1.0..=5000.0)
                .speed(0.5)
                .max_decimals(1)
                .suffix(" m"),
        )
        .on_hover_text("What lies this far either side of the line is shown");
        ui.separator();
        if ui
            .button("Back to the cut view")
            .on_hover_text(
                "Looking ahead along the line, perpendicular to the plane, orthographic, \
                 the whole width in view (Home)",
            )
            .clicked()
        {
            view.front();
        }
        // The front view on the wires or the whole cut (on the wires by
        // default while a field is drawn).
        let field =
            !view.view.electric_field.cuts.is_empty() || !view.view.magnetic_field.cuts.is_empty();
        let wires = view.on_wires.unwrap_or(field);
        let (text, hover) = if wires {
            (
                "Whole cut",
                "The front view of the whole cut: from the ground to the top of the wires, \
                 the whole width",
            )
        } else {
            (
                "Zoom to the wires",
                "The front view on the conductors crossing the cut, with 15 m around them \
                 (the default while a field is drawn here)",
            )
        };
        if ui.button(text).on_hover_text(hover).clicked() {
            view.on_wires = Some(!wires);
            view.front();
        }
        if let Some(c) = view.view.camera.as_mut() {
            for (ortho, text, hover) in [
                (
                    true,
                    "Orthographic",
                    "One scale everywhere: offsets and heights measured as drawn",
                ),
                (false, "Perspective", "As the eye sees it, as the 3D view"),
            ] {
                if ui
                    .selectable_label(c.ortho == ortho, text)
                    .on_hover_text(hover)
                    .clicked()
                {
                    c.ortho = ortho;
                }
            }
        }
    });
    step
}

fn panel_ui(
    view: &mut CutView,
    ui: &mut egui::Ui,
    cx: PanelCtx<'_>,
    views: &[AlignmentView],
    grid: &PointGrid,
    serial: u64,
) -> Option<crate::selection::Use> {
    let PanelCtx {
        rs,
        model,
        case,
        selection,
        visible,
        snaps,
        lod,
        select_held,
        main,
        link,
    } = cx;
    let mut step = bar_ui(ui, view);
    // , and . step the cut, Home goes back to the front view, while the
    // pointer is over the panel.
    let over = view
        .rect
        .zip(ui.input(|i| i.pointer.latest_pos()))
        .is_some_and(|(r, p)| r.contains(p));
    if over && !ui.ctx().text_edit_focused() {
        ui.input_mut(|i| {
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Comma) {
                step = Some(Step::Back);
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Period) {
                step = Some(Step::Ahead);
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Home) {
                view.to_front = true;
            }
        });
    }
    if let Some(how) = step {
        view.move_by(&model.line, views, how);
    }
    match place(&model.line, views, &view.spec) {
        Ok(p) => {
            view.placed = Some(p);
            view.error = None;
        }
        Err(e) => {
            view.placed = None;
            view.error = Some(e);
        }
    }
    let Some(placed) = view.placed.clone() else {
        ui.colored_label(
            egui::Color32::from_rgb(230, 120, 100),
            format!(
                "Cross-section: {}",
                view.error.as_deref().unwrap_or("not placed")
            ),
        );
        return None;
    };
    let plane = placed.plane;
    let (depth, width) = (view.spec.depth, view.spec.width);
    // The slab (its LiDAR points from the index, the ground cut by the
    // plane): made again when the cut moves or the points change.
    let key: SlabKey = (
        [
            plane.origin[0].to_bits(),
            plane.origin[1].to_bits(),
            plane.ahead[0].to_bits(),
            plane.ahead[1].to_bits(),
            depth.to_bits(),
            width.to_bits(),
        ],
        grid.key,
        model
            .ground
            .as_ref()
            .map_or(0, |g| std::sync::Arc::as_ptr(g) as usize),
        serial,
    );
    if view.slab.as_ref().is_none_or(|(k, _)| *k != key) {
        view.slab = Some((key, Arc::new(slab(model, grid, &plane, depth, width))));
    }
    let slab = view.slab.as_ref().expect("made above").1.clone();
    view.contents = contents(model, case, &plane, depth, width);
    view.heights = Some(heights(&slab, &view.contents));
    // What the main view holds: its controls, its overlays, its chunk
    // indexes.
    view.view.controls = main.controls.clone();
    if !Rc::ptr_eq(&view.view.indexes, &main.indexes) {
        view.view.indexes = main.indexes.clone();
    }
    if std::mem::take(&mut view.fresh) {
        take_overlays(&mut view.view, main, &Layer::ALL.into_iter().collect());
    }
    view.view.measure.clone_from(&main.measure);
    view.view.handles.target.clone_from(&main.handles.target);
    view.view.move_held = main.move_held;
    view.view.link = link;
    view.view.dirty.take_log();
    // The camera: the front view, else carried with the cut.
    let clip = view.clip();
    view.view.clip = clip;
    let heights = view.heights.unwrap_or([0.0, 50.0]);
    // The view's shape: as drawn last (the panel's room before that).
    let avail = view.rect.map_or_else(|| ui.available_size(), |r| r.size());
    let aspect = f64::from(avail.x) / f64::from(avail.y.max(1.0));
    // The front view untouched, the panel of another shape now: again.
    let settled = view
        .fitted
        .is_some_and(|(c, a)| Some(c) == view.view.camera && (a / aspect - 1.0).abs() > 0.01);
    // On the wires while a field is drawn (unless chosen otherwise); a
    // field arriving after the panel opened re-fits it, the camera
    // untouched.
    let field =
        !view.view.electric_field.cuts.is_empty() || !view.view.magnetic_field.cuts.is_empty();
    let wires = view.on_wires.unwrap_or(field);
    let untouched = view
        .fitted
        .is_some_and(|(c, _)| Some(c) == view.view.camera);
    let fit = if wires {
        wire_box(&view.contents)
    } else {
        None
    };
    if view.to_front
        || view.view.camera.is_none()
        || settled
        || (untouched && view.fitted_wires != fit.is_some())
    {
        let (h, w) = fit.unwrap_or((heights, width));
        let camera = front_camera(&plane, h, w, aspect);
        view.view.set_camera(camera);
        view.fitted = Some((camera, aspect));
        view.fitted_wires = fit.is_some();
        view.to_front = false;
        view.plane_shown = Some(plane);
    } else if let Some(old) = view.plane_shown
        && old != plane
    {
        let ground = |p: &Plane| {
            model
                .ground
                .as_deref()
                .and_then(|g| g.height(p.origin[0], p.origin[1]))
        };
        let dz = match (ground(&old), ground(&plane)) {
            (Some(a), Some(b)) => b - a,
            _ => 0.0,
        };
        if let Some(c) = view.view.camera.as_mut() {
            carry(c, &old, &plane, dz);
        }
        view.plane_shown = Some(plane);
    }
    let used = view.view.ui(
        ui,
        rs,
        model,
        case,
        selection,
        visible,
        snaps,
        lod,
        select_held,
    );
    // Its controls' speed (Ctrl + wheel) kept for every view.
    main.controls.speed = view.view.controls.speed;
    let Some(rect) = view.view.rect else {
        return used;
    };
    view.rect = Some(rect);
    view.drawn_frame = Some(ui.ctx().cumulative_frame_nr());
    let Some(camera) = view.view.camera else {
        return used;
    };
    let painter = ui.painter_at(rect);
    let size = [f64::from(rect.width()), f64::from(rect.height())];
    paint_frame(&painter, rect, &camera, &plane, width, heights);
    // The title: where, depth, direction, case; how it is looked at.
    let case_name = model
        .cases
        .get(case)
        .map_or_else(String::new, |c| format!(" — case {}", c.case.name));
    let at = match &view.spec.at {
        At::Structure(id) => format!(
            "structure {id} (station {})",
            ohl_drawing::format_station(placed.station)
        ),
        At::Station(_) => format!(
            "station {}{}",
            ohl_drawing::format_station(placed.station),
            if placed.angle_point {
                " (alignment point: along the bisector)"
            } else {
                ""
            }
        ),
    };
    let text = |at: egui::Pos2, s: String, size: f32, gray: u8| {
        let galley = painter.layout_no_wrap(
            s,
            egui::FontId::proportional(size),
            egui::Color32::from_gray(gray),
        );
        painter.rect_filled(
            egui::Rect::from_min_size(at, galley.size()).expand(2.0),
            2.0,
            egui::Color32::from_black_alpha(150),
        );
        painter.galley(at, galley, egui::Color32::from_gray(gray));
    };
    text(
        rect.left_top() + egui::vec2(8.0, 6.0),
        format!(
            "Cross-section at {at}, alignment {} — depth ±{depth:.1} m, width ±{width:.1} m, \
             looking ahead (bearing {:.0}°){case_name}",
            view.spec.alignment,
            plane.bearing_deg()
        ),
        11.5,
        200,
    );
    let how = if is_front(&camera, &plane) {
        "Front view, looking ahead (increasing stations): left of the line ◀  ▶ right of the \
         line; heights 1:1, orthographic"
            .to_owned()
    } else {
        format!(
            "Turned ({}): Home or Back to the cut view returns to the front view",
            if camera.ortho {
                "orthographic"
            } else {
                "perspective"
            }
        )
    };
    text(
        rect.left_top() + egui::vec2(8.0, 24.0),
        format!("{how}; ground: {}", slab.ground_from),
        10.5,
        170,
    );
    // Hover readouts: where the pointer's ray meets the plane.
    let pointer = ui
        .input(|i| i.pointer.latest_pos())
        .filter(|p| rect.contains(*p));
    view.readout = pointer.and_then(|pos| {
        let local = [f64::from(pos.x - rect.min.x), f64::from(pos.y - rect.min.y)];
        let (a, d) = camera.ray(local, size)?;
        let [offset, height] = on_plane(&plane, a, d)?;
        let nearest = view
            .contents
            .wires
            .iter()
            .flat_map(|w| w.crossings.iter().map(move |p| (w, p)))
            .map(|(w, p)| (w.label.clone(), (p[0] - offset).hypot(p[1] - height)))
            .min_by(|a, b| a.1.total_cmp(&b.1));
        Some(Readout {
            offset,
            height,
            above_ground: ground_at(&slab.ground, offset).map(|g| height - g),
            nearest,
        })
    });
    if let Some(r) = &view.readout {
        let side = if r.offset < 0.0 { "left" } else { "right" };
        let mut s = format!(
            "in the plane: offset {:.2} m {side}, height {:.2} m",
            r.offset.abs(),
            r.height
        );
        if let Some(g) = r.above_ground {
            s += &format!(", {g:.2} m above the ground");
        }
        if let Some((label, d)) = &r.nearest {
            s += &format!(", {d:.2} m to {label}");
        }
        text(rect.left_bottom() + egui::vec2(8.0, -22.0), s, 11.5, 220);
    }
    used
}

/// The heights from the ground to the top of a slab's content: lowest,
/// highest (the LiDAR points when nothing else is there; 0–50 m when the
/// slab is empty).
/// The heights and the half width around the conductors crossing the
/// plane, with room around them (15 m below and either side, 10 m
/// above): the front view on the wires. None: no wire crosses it.
fn wire_box(c: &Contents) -> Option<([f64; 2], f64)> {
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    for p in c.wires.iter().flat_map(|w| w.crossings.iter()) {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    lo[0].is_finite().then(|| {
        (
            [lo[1] - 15.0, hi[1] + 10.0],
            (lo[0].abs().max(hi[0].abs()) + 15.0).max(20.0),
        )
    })
}

fn heights(slab: &Slab, c: &Contents) -> [f64; 2] {
    let mut zs: Vec<f64> = slab
        .ground
        .iter()
        .flatten()
        .chain(
            c.wires
                .iter()
                .flat_map(|w| w.crossings.iter().chain(w.curve.iter().flatten())),
        )
        .chain(c.structures.iter().flat_map(|s| s.lines.iter().flatten()))
        .map(|p| p[1])
        .collect();
    if zs.is_empty() {
        zs = slab.lidar.iter().map(|p| p.0[1]).collect();
    }
    let (z0, z1) = zs
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), z| {
            (a.min(*z), b.max(*z))
        });
    if z0.is_finite() {
        [z0, z1]
    } else {
        [0.0, 50.0]
    }
}

/// Draws over a cut's image: the slab's sides (its width either side of
/// the line), the line's axis, the axes (offset, height, ahead) and a
/// scale bar.
fn paint_frame(
    painter: &egui::Painter,
    rect: egui::Rect,
    camera: &Camera,
    plane: &Plane,
    width: f64,
    heights: [f64; 2],
) {
    let size = [f64::from(rect.width()), f64::from(rect.height())];
    let project = camera.projector(size);
    let screen =
        |p: [f64; 3]| project(p).map(|(q, _)| rect.min + egui::vec2(q[0] as f32, q[1] as f32));
    let in_plane = |o: f64, z: f64| {
        let p = plane.at(o);
        [p[0], p[1], z]
    };
    // The width's edges and the axis, in the plane, from the ground to the
    // top.
    let [z0, z1] = heights;
    let pad = (z1 - z0).max(5.0) * 0.08;
    for (o, color) in [
        (-width, egui::Color32::from_rgb(120, 100, 75)),
        (width, egui::Color32::from_rgb(120, 100, 75)),
        (0.0, egui::Color32::from_rgb(70, 90, 120)),
    ] {
        if let (Some(a), Some(b)) = (screen(in_plane(o, z0 - pad)), screen(in_plane(o, z1 + pad))) {
            painter.extend(egui::Shape::dashed_line(
                &[a, b],
                egui::Stroke::new(1.0, color),
                6.0,
                4.0,
            ));
        }
    }
    // The axes: offset (right), height (up), ahead; drawn from a corner.
    let origin = rect.right_bottom() + egui::vec2(-60.0, -46.0);
    let t = camera.target;
    if let Some(c) = screen(t) {
        let r = plane.right();
        for (dir, label, color) in [
            (
                [r[0], r[1], 0.0],
                "offset",
                egui::Color32::from_rgb(230, 90, 90),
            ),
            (
                [0.0, 0.0, 1.0],
                "height",
                egui::Color32::from_rgb(90, 140, 255),
            ),
            (
                [plane.ahead[0], plane.ahead[1], 0.0],
                "ahead",
                egui::Color32::from_rgb(90, 200, 110),
            ),
        ] {
            let m = camera.metres_per_pixel(size[1]) * 30.0;
            let Some(e) = screen([0, 1, 2].map(|k| t[k] + m * dir[k])) else {
                continue;
            };
            let v = e - c;
            let tip = origin + v;
            if v.length() < 3.0 {
                // Looking along it: a dot (into the view).
                painter.circle_stroke(origin, 4.0, egui::Stroke::new(1.5, color));
                painter.text(
                    origin + egui::vec2(6.0, 6.0),
                    egui::Align2::LEFT_TOP,
                    label,
                    egui::FontId::proportional(10.0),
                    color,
                );
                continue;
            }
            painter.arrow(origin, v, egui::Stroke::new(2.0, color));
            painter.text(
                tip + v.normalized() * 6.0,
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(10.0),
                color,
            );
        }
    }
    // The scale bar: at the target's distance (orthographic: anywhere).
    let m = camera.metres_per_pixel(size[1]);
    let len = scale_length(m, 120.0);
    let px = (len / m) as f32;
    let a = rect.left_bottom() + egui::vec2(10.0, -40.0);
    let b = a + egui::vec2(px, 0.0);
    let white = egui::Color32::from_gray(220);
    painter.line_segment([a, b], egui::Stroke::new(2.0, white));
    for x in [a, b] {
        painter.line_segment(
            [x + egui::vec2(0.0, -4.0), x + egui::vec2(0.0, 4.0)],
            egui::Stroke::new(1.5, white),
        );
    }
    let label = if len >= 1.0 {
        format!("{len:.0} m")
    } else {
        format!("{len} m")
    };
    painter.text(
        b + egui::vec2(6.0, 0.0),
        egui::Align2::LEFT_CENTER,
        if camera.ortho {
            label
        } else {
            format!("{label} (at the target)")
        },
        egui::FontId::proportional(10.5),
        white,
    );
}

/// The window that makes a cut (C, View › Cross-section…, a structure's
/// menu, the profile's right-click).
#[derive(Debug, Clone, PartialEq)]
pub struct Dialog {
    /// At a structure (else at a station).
    pub at_structure: bool,
    /// The alignment of the station.
    pub alignment: String,
    /// The station typed, m.
    pub station: f64,
    /// The structure chosen.
    pub structure: Option<String>,
    /// The search text of the structure list.
    pub search: String,
    /// Half the thickness along the line, m.
    pub depth: f64,
    /// Half the width across, m.
    pub width: f64,
    /// The selection last seen: a structure clicked in a view while the
    /// window is open is taken.
    seen: Selection,
}

/// The alignment of `views` that holds structure `id`: `preferred` when
/// it does, else the first that does.
fn alignment_of(views: &[AlignmentView], id: &str, preferred: &str) -> Option<String> {
    let holds = |v: &&AlignmentView| v.structures.iter().any(|s| s == id);
    views
        .iter()
        .filter(holds)
        .find(|v| v.id == preferred)
        .or_else(|| views.iter().find(holds))
        .map(|v| v.id.clone())
}

impl Dialog {
    /// The window, filled from the selection: a structure (at it), a span
    /// (mid-span), else the start of `alignment` (none: the first).
    pub fn new(
        model: &Model,
        views: &[AlignmentView],
        selection: &Selection,
        alignment: Option<String>,
        width: f64,
    ) -> Self {
        let line = &model.line;
        let alignment = alignment
            .filter(|a| views.iter().any(|v| &v.id == a))
            .or_else(|| views.first().map(|v| v.id.clone()))
            .unwrap_or_default();
        let mut d = Self {
            at_structure: false,
            alignment,
            station: 0.0,
            structure: None,
            search: String::new(),
            depth: DEFAULT_DEPTH,
            width,
            seen: selection.clone(),
        };
        match selection {
            Selection::Structure(id) => {
                d.at_structure = true;
                d.structure = Some(id.clone());
                if let Some(a) = alignment_of(views, id, &d.alignment) {
                    d.alignment = a;
                }
            }
            Selection::Span { section, span } => {
                let ends = line
                    .sections
                    .iter()
                    .find(|s| &s.id == section)
                    .and_then(|s| {
                        Some((
                            s.points.get(*span)?.structure.clone(),
                            s.points.get(span + 1)?.structure.clone(),
                        ))
                    });
                if let Some((a, b)) = ends
                    && let Some(al) = alignment_of(views, &a, &d.alignment)
                    && let Some(v) = views.iter().find(|v| v.id == al)
                {
                    let centre = v.centreline(line);
                    let at = |id: &str| {
                        line.structure(id)
                            .map(|st| ohl_line::route::project(&centre, st.position).0)
                    };
                    if let (Some(sa), Some(sb)) = (at(&a), at(&b)) {
                        d.alignment = al;
                        d.station = 0.5 * (sa + sb);
                    }
                }
            }
            _ => {}
        }
        d
    }

    /// The cut it makes (none: no structure chosen, no alignment).
    pub fn spec(&self, views: &[AlignmentView]) -> Option<CutSpec> {
        if self.alignment.is_empty() {
            return None;
        }
        let (alignment, at) = if self.at_structure {
            let id = self.structure.clone()?;
            (
                alignment_of(views, &id, &self.alignment).unwrap_or_else(|| self.alignment.clone()),
                At::Structure(id),
            )
        } else {
            (self.alignment.clone(), At::Station(self.station))
        };
        Some(CutSpec {
            alignment,
            at,
            depth: self.depth,
            width: self.width,
        })
    }

    /// A structure selected in a view since the last frame is taken (and
    /// the window turns to a structure).
    pub fn follow(&mut self, selection: &Selection) {
        if *selection != self.seen {
            if let Selection::Structure(id) = selection {
                self.at_structure = true;
                self.structure = Some(id.clone());
            }
            self.seen = selection.clone();
        }
    }
}

/// What the window's buttons asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogOut {
    /// Nothing yet.
    None,
    /// Open the cut.
    Open,
    /// Close the window.
    Close,
}

/// The window's content.
pub fn dialog_ui(
    ui: &mut egui::Ui,
    d: &mut Dialog,
    line: &ohl_line::Line,
    views: &[AlignmentView],
) -> DialogOut {
    let mut out = DialogOut::None;
    ui.horizontal(|ui| {
        ui.radio_value(&mut d.at_structure, false, "At a station");
        ui.radio_value(&mut d.at_structure, true, "At a structure");
    });
    egui::Grid::new("xsection-dialog")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("Alignment");
            let label = |v: &AlignmentView| {
                if v.route == v.id {
                    v.id.clone()
                } else {
                    format!("{} (route {})", v.id, v.route)
                }
            };
            let shown = views
                .iter()
                .find(|v| v.id == d.alignment)
                .map_or_else(|| d.alignment.clone(), label);
            egui::ComboBox::from_id_salt("xsection-alignment")
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    for v in views {
                        ui.selectable_value(&mut d.alignment, v.id.clone(), label(v));
                    }
                })
                .response
                .on_hover_text("The stations are along it; the cut looks ahead along it");
            ui.end_row();
            if !d.at_structure {
                ui.label("Station");
                let length = views
                    .iter()
                    .find(|v| v.id == d.alignment)
                    .map_or(0.0, |v| plan_length(&v.centreline(line)));
                ui.horizontal(|ui| {
                    crate::style::field(
                        ui,
                        egui::DragValue::new(&mut d.station)
                            .range(0.0..=length.max(0.0))
                            .speed(1.0)
                            .max_decimals(2)
                            .suffix(" m"),
                    )
                    .on_hover_text(
                        "From the alignment's start; the plane is perpendicular to the \
                         alignment there (at an alignment point: along the bisector)",
                    );
                    ui.label(
                        egui::RichText::new(format!(
                            "{} of {}",
                            ohl_drawing::format_station(d.station),
                            ohl_drawing::format_station(length)
                        ))
                        .weak(),
                    );
                });
                ui.end_row();
            }
            ui.label("Depth ±");
            crate::style::field(
                ui,
                egui::DragValue::new(&mut d.depth)
                    .range(0.1..=1000.0)
                    .speed(0.5)
                    .max_decimals(1)
                    .suffix(" m"),
            )
            .on_hover_text(
                "Half the thickness of what is shown, along the line (default ±10 m): what lies \
                 this far before and behind the plane is projected onto it",
            );
            ui.end_row();
            ui.label("Width ±");
            crate::style::field(
                ui,
                egui::DragValue::new(&mut d.width)
                    .range(1.0..=5000.0)
                    .speed(0.5)
                    .max_decimals(1)
                    .suffix(" m"),
            )
            .on_hover_text("Half the width across the line (default: the line width)");
            ui.end_row();
        });
    if d.at_structure {
        ui.label(
            egui::RichText::new(
                "Pick one in the list, or click one in a view while this window is open; the \
                 plane holds its cross-arms",
            )
            .small()
            .weak(),
        );
        ui.horizontal(|ui| {
            ui.label("🔍");
            ui.add(
                egui::TextEdit::singleline(&mut d.search)
                    .hint_text("search structures")
                    .desired_width(200.0),
            );
        });
        let items: Vec<crate::tick_list::Item> = line
            .structures
            .iter()
            .map(|s| {
                let text = match &s.structure_type {
                    Some(t) => format!("{}  ({t})", s.id),
                    None => s.id.clone(),
                };
                crate::tick_list::Item::new(s.id.clone(), text)
            })
            .collect();
        let shown = crate::tick_list::filtered(&items, &d.search);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("xsection-structures")
                .max_height(160.0)
                .auto_shrink([false, false])
                .show_rows(ui, 18.0, shown.len(), |ui, rows| {
                    for k in rows {
                        let it = &items[shown[k]];
                        let on = d.structure.as_deref() == Some(it.key.as_str());
                        if ui.selectable_label(on, &it.label).clicked() {
                            d.structure = Some(it.key.clone());
                        }
                    }
                });
        });
    }
    let ready = d.spec(views).is_some();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(ready, egui::Button::new("Open cut"))
            .on_hover_text("A cross-section view of it, a tab of its own (Enter)")
            .clicked()
        {
            out = DialogOut::Open;
        }
        if ui.button("Close").clicked() {
            out = DialogOut::Close;
        }
    });
    out
}

#[cfg(test)]
mod unit {
    use super::*;

    fn plane_east() -> Plane {
        Plane {
            origin: [0.0, 0.0],
            ahead: [1.0, 0.0],
        }
    }

    /// Looking east, the right of the line is south: a point 6 m south
    /// is 6 m right, one 3 m north 3 m left.
    #[test]
    fn offsets_are_right_positive_looking_ahead() {
        let p = plane_east();
        assert!((p.offset([5.0, -6.0, 0.0]) - 6.0).abs() < 1e-12);
        assert!((p.offset([5.0, 3.0, 0.0]) + 3.0).abs() < 1e-12);
        assert!((p.along([5.0, 3.0, 0.0]) - 5.0).abs() < 1e-12);
        assert!((p.bearing_deg() - 90.0).abs() < 1e-9);
        let at = p.at(2.0);
        assert!((at[0]).abs() < 1e-12 && (at[1] + 2.0).abs() < 1e-12);
    }

    /// A straight wire from x = −20 to 20 crosses at x = 0 once; the slab
    /// of ±5 m keeps 10 m of it.
    #[test]
    fn wires_cross_and_are_clipped_to_the_slab() {
        let p = plane_east();
        let w = [[-20.0, -6.0, 30.0], [0.5, -6.0, 31.0], [20.0, -6.0, 32.0]];
        let c = crossings(&p, &w, 40.0);
        assert_eq!(c.len(), 1);
        assert!((c[0][0] - 6.0).abs() < 1e-12);
        // Height at x = 0 on the first piece: 30 + 20/20.5.
        assert!((c[0][1] - (30.0 + 20.0 / 20.5)).abs() < 1e-9, "{c:?}");
        let pieces = clip(&p, &w, 5.0, 40.0);
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0].len(), 3);
        // Outside the width: nothing.
        assert!(crossings(&p, &w, 5.0).is_empty());
        assert!(clip(&p, &w, 5.0, 5.0).is_empty());
    }

    /// At an alignment point the direction is the bisector; between, the
    /// segment's.
    #[test]
    fn the_direction_at_an_angle_point_is_the_bisector() {
        let c = [[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [200.0, 100.0, 0.0]];
        let (d, angle) = direction_at(&c, 50.0).unwrap();
        assert!(!angle && (d[0] - 1.0).abs() < 1e-12);
        let (d, angle) = direction_at(&c, 100.0).unwrap();
        assert!(angle);
        let b = 22.5f64.to_radians();
        assert!((d[0] - b.cos()).abs() < 1e-12 && (d[1] - b.sin()).abs() < 1e-12);
    }

    #[test]
    fn the_ground_is_interpolated() {
        let g = vec![vec![[-10.0, 1.0], [0.0, 2.0], [10.0, 0.0]]];
        assert!((ground_at(&g, -5.0).unwrap() - 1.5).abs() < 1e-12);
        assert!((ground_at(&g, 5.0).unwrap() - 1.0).abs() < 1e-12);
        assert!(ground_at(&g, 11.0).is_none());
    }
}
