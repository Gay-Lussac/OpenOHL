// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! View › **Lines on the ground beside the line**: a line on each side of
//! each alignment, on the ground, a given distance away from the line —
//! from its outermost conductor in the weather case shown (the default;
//! with the wind's blowout when the case has wind, both sides when both
//! wind sides are shown) or from its axis (the alignment).
//!
//! In plan the line on a side is the outer edge of the band within the
//! distance of the conductors (or of the axis): along each span, across
//! its chord, the point farthest out on that side that is still within
//! the distance of a wire of that span ([`offset_side`]); around an
//! angle structure the cross direction turns in steps (a round outer
//! corner); the loop a sharp inner corner makes is cut off where the
//! line crosses itself ([`without_loops`]): the outer envelope, never
//! crossing itself. It is draped on the ground ([`drape`]): the active
//! terrain surface (or the ground built from the LiDAR ground classes),
//! at every edge of its triangles crossed; without a ground surface the
//! heights of the alignment's points, every 2 m.
//!
//! Display only: kept per weather case shown and made again when the
//! line, the solution, the ground or the settings change; never written
//! to a result or the project. The settings are this user's
//! ([`Settings`], remembered per project on this computer with the other
//! view choices).

use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use eframe::egui;
use ohl_terrain::Terrain;

use crate::model::Model;

/// What the distance is measured from.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum From {
    /// The outermost conductor of the weather case shown, in plan (with
    /// its blowout): every point of every wire drawn in that case.
    #[default]
    OutermostConductor,
    /// The line's axis: the alignment.
    Axis,
}

/// This user's choices (View › Lines on the ground beside the line),
/// remembered per project on this computer ([`crate::model::ViewPrefs`]).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Drawn.
    pub on: bool,
    /// The distance from the line, m.
    pub distance: f64,
    /// What it is measured from.
    pub from: From,
    /// Routes left out (none: every route; a new route is drawn).
    pub hidden_routes: BTreeSet<String>,
    /// Colour.
    pub color: [u8; 3],
    /// Width, pixels.
    pub width: f32,
    /// Dashed.
    pub dashed: bool,
    /// Also in the profile: the ground along each line (right near,
    /// left far).
    pub profile: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            on: false,
            distance: DEFAULT_DISTANCE,
            from: From::OutermostConductor,
            hidden_routes: BTreeSet::new(),
            color: COLORS[0],
            width: 2.0,
            dashed: false,
            profile: false,
        }
    }
}

impl Settings {
    /// As by default (not remembered then).
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// A hash of what the geometry depends on (not the style).
    fn geometry_key(&self, h: &mut impl Hasher) {
        self.distance.to_bits().hash(h);
        self.from.hash(h);
        self.hidden_routes.hash(h);
    }
}

/// The distance offered first, m (a display choice, no rule behind it).
pub const DEFAULT_DISTANCE: f64 = 10.0;

/// The largest distance accepted, m.
pub const MAX_DISTANCE: f64 = 2000.0;

/// The colours offered.
pub const COLORS: [[u8; 3]; 6] = [
    [255, 110, 200],
    [255, 200, 60],
    [120, 230, 255],
    [140, 255, 120],
    [255, 255, 255],
    [255, 90, 70],
];

/// The step along the axis between points of the lines, m.
pub const STEP: f64 = 2.0;

/// The most the cross direction turns between two points around an angle
/// of the axis, degrees.
const FAN_STEP_DEG: f64 = 5.0;

/// The side of the line, looking ahead along the alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    /// Left.
    Left,
    /// Right (the near side in the profile).
    Right,
}

/// One line on the ground: the alignment, its route, the side, its
/// pieces (broken where it leaves the ground or no wire is within the
/// distance), project coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct GroundLine {
    /// The route.
    pub route: String,
    /// The alignment.
    pub alignment: String,
    /// Its side.
    pub side: Side,
    /// The pieces, on the ground.
    pub runs: Vec<Vec<[f64; 3]>>,
    /// The alignment's axis (for the profile's stations).
    pub axis: Vec<[f64; 3]>,
}

/// Where the heights come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ground {
    /// The ground surface (a terrain surface or survey file).
    Surface,
    /// The ground built from the LiDAR ground classes.
    Lidar,
    /// No ground surface: the heights of the alignment's points.
    Alignment,
}

/// The lines of one weather case shown.
#[derive(Debug, Clone, PartialEq)]
pub struct Drawn {
    /// Each alignment's two lines.
    pub lines: Vec<GroundLine>,
    /// Where the heights come from.
    pub ground: Ground,
    /// What could not be drawn, in words.
    pub notes: Vec<String>,
}

/// The lines drawn: kept per weather case (a few), made again when what
/// they come from changes.
#[derive(Debug, Default)]
pub struct GroundLines {
    /// Made for: the key of each, newest last.
    cache: Vec<(u64, Arc<Drawn>)>,
    /// What the views draw now (none: nothing).
    pub drawn: Option<Arc<Drawn>>,
    /// The style the views drew it with.
    style: Option<([u8; 3], u32, bool, bool)>,
    /// How many times the lines were made (tests).
    pub made: u64,
}

/// Weather cases kept.
const CACHED: usize = 8;

impl GroundLines {
    /// Every frame: the lines of weather case `case` as `settings` say
    /// (made when not kept yet). Whether the views must draw them again.
    pub fn frame(&mut self, model: &Model, case: usize, settings: &Settings) -> bool {
        if !settings.on {
            let changed = self.drawn.is_some();
            self.drawn = None;
            self.style = None;
            return changed;
        }
        let key = key(model, case, settings);
        let drawn = match self.cache.iter().position(|(k, _)| *k == key) {
            Some(i) => {
                let entry = self.cache.remove(i);
                let d = entry.1.clone();
                self.cache.push(entry);
                d
            }
            None => {
                let d = Arc::new(compute(model, case, settings));
                self.made += 1;
                // A line changed: what was kept for the other cases goes
                // the next time they are shown (their key differs).
                self.cache.push((key, d.clone()));
                if self.cache.len() > CACHED {
                    self.cache.remove(0);
                }
                d
            }
        };
        let style = Some((
            settings.color,
            settings.width.to_bits(),
            settings.dashed,
            settings.profile,
        ));
        let changed =
            self.drawn.as_ref().is_none_or(|d| !Arc::ptr_eq(d, &drawn)) || self.style != style;
        self.drawn = Some(drawn);
        self.style = style;
        changed
    }
}

/// What the lines of `case` are made from: the settings' geometry, the
/// line (its version, the alignments), the solution shown (its clamps
/// and low points, both wind sides), the ground.
fn key(model: &Model, case: usize, settings: &Settings) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    settings.geometry_key(&mut h);
    case.hash(&mut h);
    model.line_version.hash(&mut h);
    for p in &model.line.alignment_points {
        (&p.id, p.position.map(f64::to_bits)).hash(&mut h);
    }
    for a in &model.line.alignments {
        (&a.id, &a.route, &a.points).hash(&mut h);
    }
    if settings.from == From::OutermostConductor
        && let Some(c) = model.cases.get(case)
    {
        for s in c.drawn() {
            (s.ready, s.case.wind_from == ohl_weather::WindSide::Left).hash(&mut h);
            for shape in &s.shapes {
                shape.section.hash(&mut h);
                for p in &shape.clamps {
                    p.map(f64::to_bits).hash(&mut h);
                }
                for span in &shape.spans {
                    span.low_point.map(f64::to_bits).hash(&mut h);
                    span.mid_point.map(f64::to_bits).hash(&mut h);
                }
            }
            for j in &s.jumpers {
                j.low_point.map(f64::to_bits).hash(&mut h);
            }
        }
    }
    model
        .ground
        .as_ref()
        .map(|g| (Arc::as_ptr(g) as usize, g.num_triangles()))
        .hash(&mut h);
    h.finish()
}

/// The lines of `case`: for each alignment of the routes drawn, its two
/// sides, draped on the ground.
pub fn compute(model: &Model, case: usize, settings: &Settings) -> Drawn {
    let line = &model.line;
    let ground = match model.ground.as_deref() {
        Some(_) if model.ground_from_lidar => Ground::Lidar,
        Some(_) => Ground::Surface,
        None => Ground::Alignment,
    };
    let mut out = Drawn {
        lines: Vec::new(),
        ground,
        notes: Vec::new(),
    };
    let d = settings.distance.clamp(0.0, MAX_DISTANCE);
    let solutions: Vec<&crate::model::Solved> = model
        .cases
        .get(case)
        .map(|c| c.drawn().filter(|s| s.ready).collect())
        .unwrap_or_default();
    for view in line.alignment_views() {
        if settings.hidden_routes.contains(&view.route) {
            continue;
        }
        let axis = view.centreline(line);
        if axis.len() < 2 {
            continue;
        }
        let chain = match settings.from {
            From::Axis => Chain::along(&axis),
            From::OutermostConductor => {
                let run: Vec<&String> = view.run().collect();
                let chain = Chain::of_wires(line, &solutions, &run);
                if chain.spans.iter().all(Vec::is_empty) {
                    out.notes.push(format!(
                        "alignment {}: no wire solved in this weather case",
                        view.id
                    ));
                    continue;
                }
                chain
            }
        };
        for side in [Side::Left, Side::Right] {
            let mut runs = Vec::new();
            for piece in offset_side(&chain, d, side) {
                let piece = without_loops(&piece);
                runs.extend(drape(&piece, model.ground.as_deref(), &axis));
            }
            out.lines.push(GroundLine {
                route: view.route.clone(),
                alignment: view.id.clone(),
                side,
                runs,
                axis: axis.clone(),
            });
        }
    }
    out
}

/// A segment in plan.
pub type Seg = [[f64; 2]; 2];

/// What the lines are measured from, along an alignment: its points (the
/// structures of its run, or the alignment's own points), between each
/// two the segments of what lies in that span, at each point those of
/// what is held there (the jumpers).
#[derive(Debug, Clone, Default)]
pub struct Chain {
    /// The points, in plan.
    pub points: Vec<[f64; 2]>,
    /// Between points k and k + 1.
    pub spans: Vec<Vec<Seg>>,
    /// At point k.
    pub joints: Vec<Vec<Seg>>,
}

/// The segments of a polyline in plan (a single point: a segment of no
/// length).
fn segments(points: &[[f64; 3]], out: &mut Vec<Seg>) {
    if points.len() == 1 {
        out.push([[points[0][0], points[0][1]]; 2]);
    }
    for w in points.windows(2) {
        out.push([[w[0][0], w[0][1]], [w[1][0], w[1][1]]]);
    }
}

impl Chain {
    /// The alignment `axis` itself.
    pub fn along(axis: &[[f64; 3]]) -> Self {
        let points: Vec<[f64; 2]> = axis.iter().map(|p| [p[0], p[1]]).collect();
        let spans = points.windows(2).map(|w| vec![[w[0], w[1]]]).collect();
        Self {
            joints: vec![Vec::new(); points.len()],
            points,
            spans,
        }
    }

    /// The wires of `solutions` along the structures `run` (in order):
    /// between each two, every wire of a span between them (each
    /// subconductor of a bundle); at each, the jumpers with an end there.
    pub fn of_wires(
        line: &ohl_line::Line,
        solutions: &[&crate::model::Solved],
        run: &[&String],
    ) -> Self {
        let mut points = Vec::new();
        let mut at = std::collections::HashMap::new();
        for id in run {
            if let Some(s) = line.structure(id) {
                at.insert(id.as_str(), points.len());
                points.push([s.position[0], s.position[1]]);
            }
        }
        let n = points.len();
        let mut spans = vec![Vec::new(); n.saturating_sub(1)];
        let mut joints = vec![Vec::new(); n];
        for s in solutions {
            for shape in &s.shapes {
                let Some(section) = line.sections.iter().find(|x| x.id == shape.section) else {
                    continue;
                };
                for (k, span) in shape.spans.iter().enumerate() {
                    let (Some(a), Some(b)) = (section.points.get(k), section.points.get(k + 1))
                    else {
                        continue;
                    };
                    let (Some(&i), Some(&j)) =
                        (at.get(a.structure.as_str()), at.get(b.structure.as_str()))
                    else {
                        continue;
                    };
                    // Consecutive structures of the run only.
                    if i.abs_diff(j) != 1 {
                        continue;
                    }
                    let out = &mut spans[i.min(j)];
                    match s.bundles.spans.get(&(shape.section.clone(), k)) {
                        Some(subs) if !subs.is_empty() => {
                            for sub in subs {
                                segments(sub, out);
                            }
                        }
                        _ => segments(&span.points, out),
                    }
                }
            }
            for j in &s.jumpers {
                let Some(def) = line.jumpers.iter().find(|x| x.id == j.jumper) else {
                    continue;
                };
                let end = |e: &ohl_line::JumperEnd| match e {
                    ohl_line::JumperEnd::Wire(p) => at.get(p.structure.as_str()).copied(),
                    ohl_line::JumperEnd::Tap { .. } => None,
                };
                let Some(k) = end(&def.from).or_else(|| end(&def.to)) else {
                    continue;
                };
                match s.bundles.jumpers.get(&j.jumper) {
                    Some(subs) if !subs.is_empty() => {
                        for sub in subs {
                            segments(sub, &mut joints[k]);
                        }
                    }
                    _ => segments(&j.points, &mut joints[k]),
                }
            }
        }
        Self {
            points,
            spans,
            joints,
        }
    }
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn len(a: [f64; 2]) -> f64 {
    a[0].hypot(a[1])
}

/// Distance in plan from `p` to the segment `s` (tests).
#[cfg(test)]
fn point_segment(p: [f64; 2], s: [[f64; 2]; 2]) -> f64 {
    let e = sub(s[1], s[0]);
    let l2 = dot(e, e);
    let t = if l2 > 0.0 {
        (dot(sub(p, s[0]), e) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    len(sub(p, [s[0][0] + e[0] * t, s[0][1] + e[1] * t]))
}

/// Whether two segments cross (or touch); where along `a` (0–1).
fn crossing(a: [[f64; 2]; 2], b: [[f64; 2]; 2]) -> Option<f64> {
    let r = sub(a[1], a[0]);
    let s = sub(b[1], b[0]);
    let den = cross(r, s);
    if den.abs() < 1e-12 * (len(r) * len(s)).max(1e-300) {
        return None;
    }
    let q = sub(b[0], a[0]);
    let t = cross(q, s) / den;
    let u = cross(q, r) / den;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some(t)
}

/// The largest `t` with the point `p0 + t·n` (`n` a unit vector) within
/// `d` of the segment `s`: the far end of the line's piece inside the
/// segment's capsule; none when the line passes farther than `d`.
pub fn farthest_within(p0: [f64; 2], n: [f64; 2], s: [[f64; 2]; 2], d: f64) -> Option<f64> {
    let mut best: Option<f64> = None;
    let mut take = |t: f64| best = Some(best.map_or(t, |b: f64| b.max(t)));
    // The discs at its ends.
    for q in s {
        let w = sub(q, p0);
        let along = dot(w, n);
        let off2 = dot(w, w) - along * along;
        if off2 <= d * d {
            take(along + (d * d - off2).max(0.0).sqrt());
        }
    }
    // The band along it: 0 ≤ e·(p − q0) ≤ ℓ, |m·(p − q0)| ≤ d.
    let e = sub(s[1], s[0]);
    let l = len(e);
    if l > 0.0 {
        let e = [e[0] / l, e[1] / l];
        let m = [-e[1], e[0]];
        let w = sub(p0, s[0]);
        let (mut lo, mut hi) = (f64::NEG_INFINITY, f64::INFINITY);
        for (c0, c1, min, max) in [
            (dot(e, w), dot(e, n), 0.0, l),
            (dot(m, w), dot(m, n), -d, d),
        ] {
            // min ≤ c0 + t·c1 ≤ max.
            if c1.abs() < 1e-15 {
                if c0 < min || c0 > max {
                    return best;
                }
            } else {
                let (a, b) = ((min - c0) / c1, (max - c0) / c1);
                lo = lo.max(a.min(b));
                hi = hi.min(a.max(b));
            }
        }
        if lo <= hi && hi.is_finite() {
            take(hi);
        }
    }
    best
}

/// The sources of a span, by bins along its chord.
struct Binned<'a> {
    sources: Vec<&'a Seg>,
    bin: f64,
    bins: Vec<Vec<u32>>,
}

impl<'a> Binned<'a> {
    /// `sources` along the chord from `a` (direction `u`, `length` long):
    /// each in the bins within `d` of its extent along the chord.
    fn new(
        a: [f64; 2],
        u: [f64; 2],
        length: f64,
        sources: impl IntoIterator<Item = &'a Seg>,
        d: f64,
    ) -> Self {
        let bin = (d / 2.0).max(4.0);
        let n = ((length / bin).ceil() as usize).max(1);
        let mut bins = vec![Vec::new(); n];
        let mut kept = Vec::new();
        for s in sources {
            let a0 = dot(sub(s[0], a), u);
            let a1 = dot(sub(s[1], a), u);
            let (lo, hi) = (a0.min(a1) - d, a0.max(a1) + d);
            if hi < 0.0 || lo > length {
                continue;
            }
            let first = ((lo.max(0.0) / bin) as usize).min(n - 1);
            let last = ((hi.min(length) / bin) as usize).min(n - 1);
            for b in &mut bins[first..=last] {
                b.push(kept.len() as u32);
            }
            kept.push(s);
        }
        Self {
            sources: kept,
            bin,
            bins,
        }
    }

    /// Those that may lie within the distance of the cross line at `a`
    /// along the chord.
    fn at(&self, a: f64) -> impl Iterator<Item = &'a Seg> + '_ {
        let b = ((a.max(0.0) / self.bin) as usize).min(self.bins.len() - 1);
        self.bins[b].iter().map(|&i| self.sources[i as usize])
    }
}

/// The farthest point out (largest `t`) along `p0 + t·n` within `d` of
/// any of `sources`.
fn reach<'a>(
    p0: [f64; 2],
    n: [f64; 2],
    sources: impl IntoIterator<Item = &'a Seg>,
    d: f64,
) -> Option<f64> {
    sources
        .into_iter()
        .filter_map(|s| farthest_within(p0, n, *s, d))
        .reduce(f64::max)
}

/// The plan line on `side` of `chain`, `d` out from what it holds: along
/// each span, across its chord (every [`STEP`] at most), the farthest
/// point out within `d` of the span's wires (the outer edge of the band
/// within `d` of them; when every wire lies on the other side, nearer
/// than the axis); at each point where the chain turns away from that
/// side, the cross direction turns about it in steps of at most 5°
/// (a round outer corner, from the wires of both spans and the jumpers
/// there). Its pieces, broken where a span holds nothing. At a turn
/// towards that side the two spans' lines meet crossing
/// ([`without_loops`] cuts the loop off).
pub fn offset_side(chain: &Chain, d: f64, side: Side) -> Vec<Vec<[f64; 2]>> {
    // Left of the direction u is (−u_y, u_x).
    let normal = |u: [f64; 2]| match side {
        Side::Left => [-u[1], u[0]],
        Side::Right => [u[1], -u[0]],
    };
    let mut pieces: Vec<Vec<[f64; 2]>> = Vec::new();
    let mut current: Vec<[f64; 2]> = Vec::new();
    let mut put = |p: Option<[f64; 2]>, current: &mut Vec<[f64; 2]>| match p {
        Some(p) => {
            if current
                .last()
                .is_none_or(|q| (q[0] - p[0]).hypot(q[1] - p[1]) > 1e-6)
            {
                current.push(p);
            }
        }
        None => {
            if current.len() > 1 {
                pieces.push(std::mem::take(current));
            } else {
                current.clear();
            }
        }
    };
    let pts = &chain.points;
    let none: Vec<Seg> = Vec::new();
    let joint = |k: usize| chain.joints.get(k).unwrap_or(&none);
    // The spans with a length: (k, direction, length).
    let chords: Vec<(usize, [f64; 2], f64)> = (0..pts.len().saturating_sub(1))
        .filter_map(|k| {
            let e = sub(pts[k + 1], pts[k]);
            let l = len(e);
            (l > 1e-9).then(|| (k, [e[0] / l, e[1] / l], l))
        })
        .collect();
    for (c, &(k, u, length)) in chords.iter().enumerate() {
        let span = chain.spans.get(k).unwrap_or(&none);
        if span.is_empty() {
            put(None, &mut current);
            continue;
        }
        let n = normal(u);
        let a = pts[k];
        let binned = Binned::new(
            a,
            u,
            length,
            span.iter().chain(joint(k)).chain(joint(k + 1)),
            d,
        );
        let steps = ((length / STEP).ceil() as usize).max(1);
        for i in 0..=steps {
            let s = length * i as f64 / steps as f64;
            let p0 = [a[0] + u[0] * s, a[1] + u[1] * s];
            let t = reach(p0, n, binned.at(s), d);
            put(
                t.map(|t| [p0[0] + n[0] * t, p0[1] + n[1] * t]),
                &mut current,
            );
        }
        // The turn at its end, on the outer side only.
        let Some(&(k1, u1, _)) = chords.get(c + 1) else {
            continue;
        };
        let next = chain.spans.get(k1).unwrap_or(&none);
        if next.is_empty() {
            continue;
        }
        let n1 = normal(u1);
        let turn = cross(n, n1).atan2(dot(n, n1));
        let outer = match side {
            Side::Left => turn < 0.0,
            Side::Right => turn > 0.0,
        };
        if !outer {
            continue;
        }
        let v = pts[k + 1];
        let around: Vec<&Seg> = span
            .iter()
            .chain(next)
            .chain((k + 1..=k1).flat_map(joint))
            .collect();
        let parts = ((turn.abs().to_degrees() / FAN_STEP_DEG).ceil() as usize).max(1);
        for j in 1..parts {
            let ang = turn * j as f64 / parts as f64;
            let (cs, sn) = (ang.cos(), ang.sin());
            let dir = [n[0] * cs - n[1] * sn, n[0] * sn + n[1] * cs];
            let t = reach(v, dir, around.iter().copied(), d);
            put(
                t.map(|t| [v[0] + dir[0] * t, v[1] + dir[1] * t]),
                &mut current,
            );
        }
    }
    put(None, &mut current);
    pieces
}

/// `line` without the loops it makes where it crosses itself (the inner
/// side of a sharp angle): from each piece, the next crossing farthest
/// along is joined directly. The result never crosses itself.
pub fn without_loops(line: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let n = line.len();
    if n < 4 {
        return line.to_vec();
    }
    // A grid of the segments.
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    let mut total = 0.0;
    for (k, p) in line.iter().enumerate() {
        for i in 0..2 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
        if k > 0 {
            total += len(sub(*p, line[k - 1]));
        }
    }
    let cell = (total / n as f64 * 4.0).max(1.0);
    let cols = (((hi[0] - lo[0]) / cell) as usize + 1).min(4096);
    let rows = (((hi[1] - lo[1]) / cell) as usize + 1).min(4096);
    let cell_of = |p: [f64; 2]| {
        (
            (((p[0] - lo[0]) / cell) as usize).min(cols - 1),
            (((p[1] - lo[1]) / cell) as usize).min(rows - 1),
        )
    };
    let mut grid: std::collections::HashMap<(usize, usize), Vec<usize>> = Default::default();
    for k in 0..n - 1 {
        let (a, b) = (cell_of(line[k]), cell_of(line[k + 1]));
        for i in a.0.min(b.0)..=a.0.max(b.0) {
            for j in a.1.min(b.1)..=a.1.max(b.1) {
                grid.entry((i, j)).or_default().push(k);
            }
        }
    }
    let mut out = vec![line[0]];
    let mut k = 0;
    let mut start = line[0];
    while k < n - 1 {
        let seg = [start, line[k + 1]];
        let (a, b) = (cell_of(seg[0]), cell_of(seg[1]));
        let mut best: Option<(usize, f64, [f64; 2])> = None;
        for i in a.0.min(b.0)..=a.0.max(b.0) {
            for j in a.1.min(b.1)..=a.1.max(b.1) {
                for &m in grid.get(&(i, j)).map_or(&[][..], Vec::as_slice) {
                    if m < k + 2 || best.is_some_and(|x| x.0 >= m) {
                        continue;
                    }
                    let other = [line[m], line[m + 1]];
                    if let Some(t) = crossing(seg, other) {
                        let x = [
                            seg[0][0] + (seg[1][0] - seg[0][0]) * t,
                            seg[0][1] + (seg[1][1] - seg[0][1]) * t,
                        ];
                        best = Some((m, t, x));
                    }
                }
            }
        }
        match best {
            Some((m, _, x)) => {
                out.push(x);
                start = x;
                k = m;
            }
            None => {
                out.push(line[k + 1]);
                k += 1;
                start = line[k];
            }
        }
    }
    out.dedup_by(|a, b| (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-9);
    out
}

/// `plan` on the ground: on `ground`, a point where it crosses each edge
/// of its triangles (the ground is flat between them), broken where it
/// leaves the surface; without one, every [`STEP`] at the height of the
/// alignment `axis` (its points' heights, linear along it) where it is
/// beside it.
pub fn drape(plan: &[[f64; 2]], ground: Option<&Terrain>, axis: &[[f64; 3]]) -> Vec<Vec<[f64; 3]>> {
    let mut runs: Vec<Vec<[f64; 3]>> = Vec::new();
    let mut current: Vec<[f64; 3]> = Vec::new();
    let flush = |current: &mut Vec<[f64; 3]>, runs: &mut Vec<Vec<[f64; 3]>>| {
        if current.len() > 1 {
            runs.push(std::mem::take(current));
        } else {
            current.clear();
        }
    };
    let push = |current: &mut Vec<[f64; 3]>, p: [f64; 3]| {
        if current
            .last()
            .is_none_or(|q| (q[0] - p[0]).hypot(q[1] - p[1]) > 1e-9)
        {
            current.push(p);
        }
    };
    match ground {
        Some(g) => {
            for w in plan.windows(2) {
                let pieces = g.profile(w[0], w[1]);
                if pieces.is_empty() {
                    flush(&mut current, &mut runs);
                    continue;
                }
                for (i, run) in pieces.iter().enumerate() {
                    let joins = i == 0 && run.first().is_some_and(|p| p.t < 1e-9);
                    if !joins {
                        flush(&mut current, &mut runs);
                    }
                    for p in run {
                        push(&mut current, p.point);
                    }
                }
                if pieces
                    .last()
                    .and_then(|r| r.last())
                    .is_none_or(|p| p.t < 1.0 - 1e-9)
                {
                    flush(&mut current, &mut runs);
                }
            }
        }
        None => {
            let height = |p: [f64; 2]| axis_height(axis, p);
            for w in plan.windows(2) {
                let l = len(sub(w[1], w[0]));
                let steps = ((l / STEP).ceil() as usize).max(1);
                for i in 0..=steps {
                    let f = i as f64 / steps as f64;
                    let p = [
                        w[0][0] + (w[1][0] - w[0][0]) * f,
                        w[0][1] + (w[1][1] - w[0][1]) * f,
                    ];
                    push(&mut current, [p[0], p[1], height(p)]);
                }
            }
        }
    }
    flush(&mut current, &mut runs);
    runs
}

/// The height of the alignment `axis` at the station of `p` (linear
/// between its points).
fn axis_height(axis: &[[f64; 3]], p: [f64; 2]) -> f64 {
    let (st, _) = ohl_line::route::project(axis, [p[0], p[1], 0.0]);
    let mut start = 0.0;
    for w in axis.windows(2) {
        let l = (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]);
        if st <= start + l || l == 0.0 {
            let f = if l > 0.0 {
                ((st - start) / l).clamp(0.0, 1.0)
            } else {
                0.0
            };
            return w[0][2] + (w[1][2] - w[0][2]) * f;
        }
        start += l;
    }
    axis.last().map_or(0.0, |p| p[2])
}

/// The lines in the 3D view: each piece a little above the ground (so it
/// is not hidden in it), dashed when chosen.
pub fn scene(origin: [f64; 3], drawn: Option<&Drawn>, settings: &Settings) -> ohl_render::Scene {
    let mut s = ohl_render::Scene::new(origin);
    let Some(d) = drawn else {
        return s;
    };
    let [r, g, b] = settings.color;
    for l in &d.lines {
        for run in &l.runs {
            let lifted: Vec<[f64; 3]> = run.iter().map(|p| [p[0], p[1], p[2] + 0.2]).collect();
            if settings.dashed {
                for dash in crate::wire_style::dashes(&lifted, &[6.0, 4.0], 1.0) {
                    s.polyline(&dash, [r, g, b, 255], settings.width);
                }
            } else {
                s.polyline(&lifted, [r, g, b, 255], settings.width);
            }
        }
    }
    s
}

/// The lines in the plan view (those in view).
pub fn paint_plan(
    painter: &egui::Painter,
    rect: egui::Rect,
    to_screen: &impl Fn([f64; 3]) -> egui::Pos2,
    drawn: Option<&Drawn>,
    settings: &Settings,
) {
    let Some(d) = drawn else {
        return;
    };
    let [r, g, b] = settings.color;
    let stroke = egui::Stroke::new(settings.width, egui::Color32::from_rgb(r, g, b));
    let near = rect.expand(50.0);
    for l in &d.lines {
        for run in &l.runs {
            let pts = crate::plan::thin_px(run.iter().map(|p| to_screen(*p)));
            if pts.len() < 2 || !pts.iter().any(|p| near.contains(*p)) {
                continue;
            }
            if settings.dashed {
                painter.extend(egui::Shape::dashed_line(&pts, stroke, 8.0, 5.0));
            } else {
                painter.add(egui::Shape::line(pts, stroke));
            }
        }
    }
}

/// The lines along the profile of an alignment drawn from station `at`:
/// (side, pieces of (station, height)) — points beside the axis only.
pub fn along(drawn: &Drawn, alignment: &str, at: f64) -> Vec<(Side, Vec<Vec<[f64; 2]>>)> {
    let mut out = Vec::new();
    for l in drawn.lines.iter().filter(|l| l.alignment == alignment) {
        let mut pieces = Vec::new();
        for run in &l.runs {
            let mut piece: Vec<[f64; 2]> = Vec::new();
            for p in run {
                let (st, off) = ohl_line::route::project(&l.axis, *p);
                let back = ohl_line::route::point_at(&l.axis, st, off);
                if (back[0] - p[0]).hypot(back[1] - p[1]) < 0.5 {
                    piece.push([at + st, p[2]]);
                } else if piece.len() > 1 {
                    pieces.push(std::mem::take(&mut piece));
                } else {
                    piece.clear();
                }
            }
            if piece.len() > 1 {
                pieces.push(piece);
            }
        }
        out.push((l.side, pieces));
    }
    out
}

/// The menu (View › Lines on the ground beside the line): whether the
/// settings changed.
pub fn menu(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    routes: &[String],
    drawn: Option<&Drawn>,
) -> bool {
    let before = settings.clone();
    ui.label(
        egui::RichText::new(
            "A line each side of the line, on the ground, at the weather case shown (yours \
             only; display only)",
        )
        .small()
        .weak(),
    );
    ui.checkbox(&mut settings.on, "Show the lines");
    ui.horizontal(|ui| {
        ui.label("Distance");
        ui.add(
            egui::DragValue::new(&mut settings.distance)
                .range(0.0..=MAX_DISTANCE)
                .speed(0.5)
                .max_decimals(2)
                .suffix(" m"),
        );
    });
    ui.label(egui::RichText::new("Measured from").small().weak());
    ui.radio_value(
        &mut settings.from,
        From::OutermostConductor,
        "the outermost conductor (with its blowout)",
    )
    .on_hover_text(
        "In plan, every point of every wire drawn in the weather case shown — swung out by \
         the wind when the case has wind, both sides when both are shown — on each side its \
         own outermost conductor",
    );
    ui.radio_value(
        &mut settings.from,
        From::Axis,
        "the line's axis (alignment)",
    );
    if routes.len() > 1 {
        ui.label(egui::RichText::new("Routes").small().weak());
        for r in routes {
            let mut on = !settings.hidden_routes.contains(r);
            if ui.checkbox(&mut on, r).changed() {
                if on {
                    settings.hidden_routes.remove(r);
                } else {
                    settings.hidden_routes.insert(r.clone());
                }
            }
        }
    }
    ui.label(egui::RichText::new("Style").small().weak());
    ui.horizontal(|ui| {
        for c in COLORS {
            let col = egui::Color32::from_rgb(c[0], c[1], c[2]);
            let chosen = settings.color == c;
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::click());
            ui.painter().rect_filled(rect.shrink(2.0), 2.0, col);
            if chosen {
                ui.painter().rect_stroke(
                    rect,
                    2.0,
                    egui::Stroke::new(1.5, ui.visuals().strong_text_color()),
                    egui::StrokeKind::Inside,
                );
            }
            if resp.on_hover_text("This colour").clicked() {
                settings.color = c;
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("Width");
        ui.add(
            egui::DragValue::new(&mut settings.width)
                .range(1.0..=6.0)
                .speed(0.1)
                .max_decimals(1)
                .suffix(" px"),
        );
        ui.checkbox(&mut settings.dashed, "Dashed");
    });
    ui.checkbox(
        &mut settings.profile,
        "In the profile too (right side near, left side far, dashed)",
    );
    if settings.on
        && let Some(d) = drawn
    {
        let from = match d.ground {
            Ground::Surface => "on the ground surface",
            Ground::Lidar => "on the ground from the LiDAR points",
            Ground::Alignment => "no ground surface: at the heights of the alignment's points",
        };
        ui.label(egui::RichText::new(from).small().weak());
        for n in &d.notes {
            ui.label(egui::RichText::new(n).small().weak());
        }
    }
    *settings != before
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(z: f64) -> Terrain {
        Terrain::from_xyz(
            &[
                [-500.0, -500.0, z],
                [500.0, -500.0, z],
                [500.0, 500.0, z],
                [-500.0, 500.0, z],
            ],
            None,
        )
        .unwrap()
    }

    /// One span from `a` to `b` holding `wires`.
    fn span(a: [f64; 2], b: [f64; 2], wires: &[Seg]) -> Chain {
        Chain {
            points: vec![a, b],
            spans: vec![wires.to_vec()],
            joints: vec![Vec::new(); 2],
        }
    }

    /// The axis as its own chain.
    fn axis_chain(axis: &[[f64; 2]]) -> Chain {
        let a: Vec<[f64; 3]> = axis.iter().map(|p| [p[0], p[1], 0.0]).collect();
        Chain::along(&a)
    }

    fn assert_simple(line: &[[f64; 2]], what: &str) {
        for i in 0..line.len() - 1 {
            for j in i + 2..line.len() - 1 {
                let c = crossing([line[i], line[i + 1]], [line[j], line[j + 1]]);
                assert!(c.is_none(), "{what}: {i} × {j}");
            }
        }
    }

    /// Hand check: a straight span along x from 0 to 300 m, its wire 5 m
    /// left of the axis (y = 5); 10 m away: the left line at y = 5 + 10 =
    /// 15, the right one at y = 5 − 10 = −5 (the outermost conductor on
    /// the right is the same wire); with a second wire 5 m right of the
    /// axis, ±15 m. On flat ground at 400 m, the lines at 400 m.
    #[test]
    fn a_straight_span_gives_lines_at_the_distance_from_its_wire() {
        let wire = [[[0.0, 5.0], [300.0, 5.0]]];
        let chain = span([0.0, 0.0], [300.0, 0.0], &wire);
        let g = flat(400.0);
        let axis3 = [[0.0, 0.0, 400.0], [300.0, 0.0, 400.0]];
        for (side, y) in [(Side::Left, 15.0), (Side::Right, -5.0)] {
            let pieces = offset_side(&chain, 10.0, side);
            assert_eq!(pieces.len(), 1);
            let line = without_loops(&pieces[0]);
            assert!(line[0][0].abs() < 1e-9 && (line.last().unwrap()[0] - 300.0).abs() < 1e-9);
            for p in &line {
                assert!((p[1] - y).abs() < 1e-9, "{side:?}: {p:?}");
            }
            // Every 2 m.
            assert_eq!(line.len(), 151);
            let draped = drape(&line, Some(&g), &axis3);
            assert_eq!(draped.len(), 1);
            for p in &draped[0] {
                assert!((p[2] - 400.0).abs() < 1e-9 && (p[1] - y).abs() < 1e-9);
            }
        }
        let wires = [[[0.0, 5.0], [300.0, 5.0]], [[0.0, -5.0], [300.0, -5.0]]];
        let chain = span([0.0, 0.0], [300.0, 0.0], &wires);
        let left = offset_side(&chain, 10.0, Side::Left);
        let right = offset_side(&chain, 10.0, Side::Right);
        assert!(left[0].iter().all(|p| (p[1] - 15.0).abs() < 1e-9));
        assert!(right[0].iter().all(|p| (p[1] + 15.0).abs() < 1e-9));
    }

    /// From the axis: ±d.
    #[test]
    fn from_the_axis_the_lines_are_at_the_distance() {
        let chain = axis_chain(&[[0.0, 0.0], [100.0, 0.0]]);
        let l = offset_side(&chain, 7.5, Side::Left);
        let r = offset_side(&chain, 7.5, Side::Right);
        assert!(l[0].iter().all(|p| (p[1] - 7.5).abs() < 1e-9));
        assert!(r[0].iter().all(|p| (p[1] + 7.5).abs() < 1e-9));
    }

    /// A sharp angle (a 120° turn to the left): no line crosses itself;
    /// every point is `d` from the source (on the band's edge, none
    /// nearer); the outer corner round about the angle point, the inner
    /// one where the two sides meet.
    #[test]
    fn an_angle_gives_no_self_intersection() {
        let axis = [[0.0, 0.0], [200.0, 0.0], [100.0, 173.205]];
        let src: Vec<Seg> = axis.windows(2).map(|w| [w[0], w[1]]).collect();
        let d = 20.0;
        let chain = axis_chain(&axis);
        for side in [Side::Left, Side::Right] {
            let pieces = offset_side(&chain, d, side);
            assert_eq!(pieces.len(), 1, "{side:?}");
            let raw = &pieces[0];
            let line = without_loops(raw);
            assert_simple(&line, &format!("{side:?}"));
            for p in &line {
                let near = src
                    .iter()
                    .map(|s| point_segment(*p, *s))
                    .fold(f64::INFINITY, f64::min);
                assert!((near - d).abs() < 1e-6, "{side:?}: {p:?} at {near}");
            }
            if side == Side::Left {
                // The inner side: the loop cut off where the two lines
                // cross, d from both legs: on the bisector of the 60°
                // between them, d / sin 30° = 40 m from the angle point,
                // i.e. x = 200 − 20·√3 = 165.36, y = 20.
                assert!(line.len() < raw.len(), "the inner loop cut");
                let x = 200.0 - 20.0 * 3f64.sqrt();
                assert!(
                    line.iter()
                        .any(|p| (p[0] - x).abs() < 1e-3 && (p[1] - 20.0).abs() < 1e-6),
                    "{line:?}"
                );
            } else {
                assert_eq!(line.len(), raw.len());
                let around = line
                    .iter()
                    .filter(|p| ((p[0] - 200.0).hypot(p[1]) - d).abs() < 1e-6)
                    .count();
                // 120° in steps of 5°: 23 points between the two legs'
                // ends, and those ends.
                assert!(around >= 25, "{around}");
            }
        }
    }

    /// Blowout: the wire swung out 3.2 m mid-span moves the lines by as
    /// much there.
    #[test]
    fn the_lines_follow_the_blowout_of_the_outermost_conductor() {
        let still = [[[0.0, 5.0], [150.0, 5.0]], [[150.0, 5.0], [300.0, 5.0]]];
        // Swung to the left: 5 m at the clamps, 8.2 m in the middle.
        let blown = [[[0.0, 5.0], [150.0, 8.2]], [[150.0, 8.2], [300.0, 5.0]]];
        let at_mid = |src: &[Seg], side| {
            let p = offset_side(&span([0.0, 0.0], [300.0, 0.0], src), 10.0, side);
            p[0].iter()
                .find(|q| (q[0] - 150.0).abs() < 1e-9)
                .copied()
                .unwrap()[1]
        };
        assert!((at_mid(&still, Side::Left) - 15.0).abs() < 1e-9);
        assert!((at_mid(&still, Side::Right) + 5.0).abs() < 1e-9);
        // 8.2 + 10.
        assert!((at_mid(&blown, Side::Left) - 18.2).abs() < 1e-9);
        // The right side: the same wire, blown away from it: 8.2 − 10 =
        // −1.8; the bands of the two pieces, meeting at an angle of
        // 2 × 1.2°, reach 2.5 mm beyond it there.
        assert!((at_mid(&blown, Side::Right) + 1.8).abs() < 0.005);
    }

    /// Two spans at an angle structure (a 30° turn to the right) with a
    /// wire 6 m each side of the axis: no self-intersection, every point
    /// 10 m from the nearest wire.
    #[test]
    fn an_angle_structure_with_wires() {
        let a = [0.0, 0.0];
        let b = [250.0, 0.0];
        let t = 30f64.to_radians();
        let c = [b[0] + 250.0 * t.cos(), -250.0 * t.sin()];
        // Wires along each span, ±6 m off its chord, meeting on the
        // bisector at the structure.
        let off = |p: [f64; 2], n: [f64; 2], o: f64| [p[0] + n[0] * o, p[1] + n[1] * o];
        let n1 = [0.0, 1.0];
        let n2 = [t.sin(), t.cos()];
        let bis = {
            let s = [n1[0] + n2[0], n1[1] + n2[1]];
            let l = s[0].hypot(s[1]);
            [s[0] / l, s[1] / l]
        };
        let k = 6.0 / 15f64.to_radians().cos();
        let mut spans = vec![Vec::new(), Vec::new()];
        for o in [-6.0f64, 6.0] {
            let at_b = off(b, bis, k * o.signum());
            spans[0].push([off(a, n1, o), at_b]);
            spans[1].push([at_b, off(c, n2, o)]);
        }
        let chain = Chain {
            points: vec![a, b, c],
            spans,
            joints: vec![Vec::new(); 3],
        };
        for side in [Side::Left, Side::Right] {
            let pieces = offset_side(&chain, 10.0, side);
            assert_eq!(pieces.len(), 1);
            let line = without_loops(&pieces[0]);
            assert_simple(&line, &format!("{side:?}"));
            for p in &line {
                let near = chain
                    .spans
                    .iter()
                    .flatten()
                    .map(|s| point_segment(*p, *s))
                    .fold(f64::INFINITY, f64::min);
                assert!((near - 10.0).abs() < 1e-6, "{side:?}: {p:?} at {near}");
            }
        }
    }

    /// Draped on a TIN: every point's height is the surface's there; a
    /// point at each edge crossed (flat between them); broken where it
    /// leaves the surface.
    #[test]
    fn the_lines_take_the_heights_of_the_tin() {
        let mut pts = Vec::new();
        for i in -4..=16 {
            for j in -4..=4 {
                let (x, y) = (f64::from(i) * 25.0, f64::from(j) * 25.0);
                let bump = if (i + j) % 3 == 0 { 2.0 } else { 0.0 };
                pts.push([x, y, 100.0 + 0.1 * x + 0.3 * y.abs() + bump]);
            }
        }
        let g = Terrain::from_xyz(&pts, None).unwrap();
        let plan = [[0.0, 15.0], [123.0, 17.0], [250.0, 15.0]];
        let runs = drape(&plan, Some(&g), &[]);
        assert_eq!(runs.len(), 1);
        let run = &runs[0];
        assert!(run.len() > 20, "{}", run.len());
        for p in run {
            let z = g.height(p[0], p[1]).unwrap();
            assert!((p[2] - z).abs() < 1e-6, "{p:?} {z}");
        }
        for w in run.windows(2) {
            let m = [(w[0][0] + w[1][0]) / 2.0, (w[0][1] + w[1][1]) / 2.0];
            let z = g.height(m[0], m[1]).unwrap();
            assert!((z - (w[0][2] + w[1][2]) / 2.0).abs() < 1e-6);
        }
        let runs = drape(&[[0.0, 15.0], [600.0, 15.0]], Some(&g), &[]);
        assert_eq!(runs.len(), 1);
        assert!(runs[0].last().unwrap()[0] <= 400.0 + 1e-6);
    }

    /// Without a ground surface: the alignment's heights, linear between
    /// its points, every 2 m.
    #[test]
    fn without_ground_the_alignments_heights() {
        let axis = [[0.0, 0.0, 400.0], [100.0, 0.0, 420.0]];
        let runs = drape(&[[0.0, 10.0], [100.0, 10.0]], None, &axis);
        assert_eq!(runs[0].len(), 51);
        for p in &runs[0] {
            assert!((p[2] - (400.0 + 0.2 * p[0])).abs() < 1e-9);
        }
    }

    /// A span holding nothing breaks the line.
    #[test]
    fn a_span_without_wires_breaks_the_line() {
        let w = |y0: f64, x0: f64, x1: f64| [[x0, y0], [x1, y0]];
        let chain = Chain {
            points: vec![[0.0, 0.0], [100.0, 0.0], [200.0, 0.0], [300.0, 0.0]],
            spans: vec![
                vec![w(5.0, 0.0, 100.0)],
                Vec::new(),
                vec![w(5.0, 200.0, 300.0)],
            ],
            joints: vec![Vec::new(); 4],
        };
        assert_eq!(offset_side(&chain, 10.0, Side::Left).len(), 2);
    }
}
