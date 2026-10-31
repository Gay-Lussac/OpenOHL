// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Where a field is shown (owner, 2026-10-29): **vertical cuts across the
//! line** (what happens near buildings), cuts **from above** (the grids at
//! a height above the ground, [`crate::magnetic`]) and **volumes** —
//! isosurfaces of levels in the 3D view. For any field given as a
//! function of the place, so the electric field takes the same views.
//!
//! Every value at a grid or lattice point is computed in full; only the
//! drawing between the points is interpolated ([`crate::contour`]), and a
//! volume computes its fine lattice only where a surface can pass:
//! - a coarse lattice ([`COARSE`] fine steps apart) first;
//! - then, in fine steps, every coarse cell where a level lies between its
//!   corners' values or within [`MARGIN`] of them (a surface bulging into
//!   the cell), and every cell within a coarse step of a wire piece (where
//!   a thin surface around a conductor can hide between coarse points);
//! - then, while a level crosses a face of a cell computed fine (between
//!   the face's fine values), the cell beyond that face, so a surface is
//!   followed until it closes or leaves the box.
//!
//! A surface lying entirely between coarse points away from every wire
//! (smaller than a coarse cell, and not within [`MARGIN`] of their
//! values) is not found (said in the theory page).

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

use crate::contour::{self, Mesh};

/// Progress of a run and a way to stop it (shared with another thread).
#[derive(Debug, Default)]
pub struct Progress {
    /// Set to stop: the run returns an error at its next point.
    pub cancel: AtomicBool,
    /// Work done: points computed.
    pub done: AtomicUsize,
    /// Points to compute.
    pub total: AtomicUsize,
    /// What is under way, in words (empty: points).
    pub stage: std::sync::Mutex<String>,
    /// The tiled floors' solves, apart from the points (a floor of
    /// thousands of tiles takes seconds, a point microseconds; their
    /// cost grows with the cube of the tiles): planned and solved, in the
    /// time they are expected to take ([`crate::electric::TiledFloor::seconds`],
    /// µs), and the time the solved ones took (ns).
    floors_planned: AtomicU64,
    floors_done: AtomicU64,
    floors_took: AtomicU64,
    /// The floors being solved (several at once: [`Progress::parallel_floors`]):
    /// each its token, when it began and its expected time (µs); and since
    /// when one has been under way.
    floor_now: std::sync::Mutex<FloorsNow>,
    /// The time while floors were solved (none of the points'), ns.
    floors_wall: AtomicU64,
    /// Floors solved at the same time (0 taken as 1).
    floors_at_once: AtomicUsize,
}

/// The floors under way.
#[derive(Debug, Default)]
struct FloorsNow {
    next: u64,
    running: Vec<(u64, Instant, u64)>,
    since: Option<Instant>,
}

/// The time a floor of `tiles` is expected to take, µs.
fn floor_cost(tiles: usize) -> u64 {
    (crate::electric::TiledFloor::seconds(tiles) * 1e6).ceil() as u64
}

impl Progress {
    /// Says what is under way.
    pub fn set_stage(&self, text: impl Into<String>) {
        if let Ok(mut s) = self.stage.lock() {
            *s = text.into();
        }
    }

    /// What is under way.
    pub fn stage(&self) -> String {
        self.stage.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// A tiled floor of `tiles` the run will solve (announced before the
    /// run starts on it, so the time left counts it from the start).
    pub fn plan_floor(&self, tiles: usize) {
        self.floors_planned
            .fetch_add(floor_cost(tiles), Ordering::Relaxed);
    }

    /// Floors solved `n` at the same time from now on (the time left
    /// divides the floors' by it).
    pub fn parallel_floors(&self, n: usize) {
        self.floors_at_once.store(n.max(1), Ordering::Relaxed);
    }

    /// A floor of `tiles` begins (one not planned is added): its token
    /// for [`Progress::floor_ends`].
    pub fn floor_begins(&self, tiles: usize) -> u64 {
        let cost = floor_cost(tiles);
        let Ok(mut f) = self.floor_now.lock() else {
            return 0;
        };
        let running: u64 = f.running.iter().map(|r| r.2).sum();
        let done = self.floors_done.load(Ordering::Relaxed);
        self.floors_planned
            .fetch_max(done + running + cost, Ordering::Relaxed);
        f.next += 1;
        let token = f.next;
        f.running.push((token, Instant::now(), cost));
        f.since.get_or_insert_with(Instant::now);
        token
    }

    /// The floor of `token` is solved.
    pub fn floor_ends(&self, token: u64) {
        let Ok(mut f) = self.floor_now.lock() else {
            return;
        };
        if let Some(k) = f.running.iter().position(|r| r.0 == token) {
            let (_, since, cost) = f.running.remove(k);
            self.floors_done.fetch_add(cost, Ordering::Relaxed);
            self.floors_took
                .fetch_add(since.elapsed().as_nanos() as u64, Ordering::Relaxed);
        }
        if f.running.is_empty()
            && let Some(since) = f.since.take()
        {
            self.floors_wall
                .fetch_add(since.elapsed().as_nanos() as u64, Ordering::Relaxed);
        }
    }

    /// The floors' speed: the time the solved ones took against the
    /// time expected ([`crate::electric::TiledFloor::seconds`]); none
    /// before one is solved.
    pub fn floor_speed(&self) -> Option<f64> {
        let solved = self.floors_done.load(Ordering::Relaxed);
        (solved > 0).then(|| self.floors_took.load(Ordering::Relaxed) as f64 * 1e-3 / solved as f64)
    }

    /// The time left of a run `elapsed` in, s: the points left at the
    /// speed of those computed, the floors left at the speed of those
    /// solved (before the first: as expected on this computer). None
    /// before anything is known.
    pub fn seconds_left(&self, elapsed: std::time::Duration) -> Option<f64> {
        let done = self.done.load(Ordering::Relaxed);
        let total = self.total.load(Ordering::Relaxed);
        let planned = self.floors_planned.load(Ordering::Relaxed) as f64 * 1e-6;
        let solved = self.floors_done.load(Ordering::Relaxed) as f64 * 1e-6;
        let took = self.floors_took.load(Ordering::Relaxed) as f64 * 1e-9;
        let at_once = self.floors_at_once.load(Ordering::Relaxed).max(1) as f64;
        // This computer's speed against the expected one (thermal
        // throttling, other work, floors solved side by side): from the
        // floors solved.
        let scale = if solved > 0.0 { took / solved } else { 1.0 };
        let (now, running_left, wall) = self.floor_now.lock().map_or((0.0, 0.0, 0.0), |f| {
            let now: f64 = f.running.iter().map(|r| r.2 as f64 * 1e-6).sum();
            let left: f64 = f
                .running
                .iter()
                .map(|r| {
                    let c = r.2 as f64 * 1e-6 * scale;
                    (c - r.1.elapsed().as_secs_f64()).max(0.1 * c)
                })
                .sum();
            (
                now,
                left,
                f.since.map_or(0.0, |s| s.elapsed().as_secs_f64()),
            )
        });
        let floors = ((planned - solved - now).max(0.0) * scale + running_left) / at_once;
        let floors = if planned > 0.0 { floors } else { 0.0 };
        let points_left = total.saturating_sub(done);
        let points = if points_left == 0 {
            Some(0.0)
        } else if done > 0 {
            let floor_time = self.floors_wall.load(Ordering::Relaxed) as f64 * 1e-9 + wall;
            let time = (elapsed.as_secs_f64() - floor_time).max(0.0);
            Some(time / done as f64 * points_left as f64)
        } else {
            None
        };
        match points {
            Some(p) => Some(p + floors),
            None => (planned > 0.0).then_some(floors),
        }
    }
}

/// The distance from `p` to the segment `ab`.
fn segment_distance(ab: [[f64; 3]; 2], p: [f64; 3]) -> f64 {
    let d = [
        ab[1][0] - ab[0][0],
        ab[1][1] - ab[0][1],
        ab[1][2] - ab[0][2],
    ];
    let w = [p[0] - ab[0][0], p[1] - ab[0][1], p[2] - ab[0][2]];
    let l2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    let u = if l2 > 0.0 {
        ((w[0] * d[0] + w[1] * d[1] + w[2] * d[2]) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    ((w[0] - u * d[0]).powi(2) + (w[1] - u * d[1]).powi(2) + (w[2] - u * d[2]).powi(2)).sqrt()
}

/// "stopped" once `progress` asks to stop.
fn check(progress: Option<&Progress>) -> Result<(), String> {
    match progress {
        Some(p) if p.cancel.load(Ordering::Relaxed) => Err("stopped".into()),
        _ => Ok(()),
    }
}

/// A field: its values at places (x, y, z), e.g. µT, a block of places
/// at a time (so a field summed over many sources keeps each source in
/// the processor's cache for the whole block).
pub type Field<'a, T> = &'a (dyn Fn(&[[f64; 3]]) -> Vec<T> + Sync);

/// A field of one place at a time as a [`Field`].
pub fn each<T>(f: impl Fn([f64; 3]) -> T + Sync) -> impl Fn(&[[f64; 3]]) -> Vec<T> + Sync {
    move |ps: &[[f64; 3]]| ps.iter().map(|&p| f(p)).collect()
}

/// The places a field is asked for at once ([`Field`]).
pub const BLOCK: usize = 64;

/// The threads a computation runs on.
pub fn threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .clamp(1, 16)
}

/// Values at many points on all cores (in order); `progress` counts them
/// and stops the run ("stopped"). The threads take blocks of [`BLOCK`]
/// points in turn, so a quicker core takes more of them.
pub fn compute_all<T: Copy + Default + Send>(
    points: &[[f64; 3]],
    f: Field<'_, T>,
    progress: Option<&Progress>,
) -> Result<Vec<T>, String> {
    let blocks: Vec<&[[f64; 3]]> = points.chunks(BLOCK).collect();
    let next = AtomicUsize::new(0);
    let stopped = AtomicBool::new(false);
    let done = std::sync::Mutex::new(Vec::with_capacity(blocks.len()));
    std::thread::scope(|scope| {
        for _ in 0..threads().min(blocks.len()) {
            scope.spawn(|| {
                let mut mine = Vec::new();
                loop {
                    let k = next.fetch_add(1, Ordering::Relaxed);
                    let Some(ps) = blocks.get(k) else { break };
                    if progress.is_some_and(|p| p.cancel.load(Ordering::Relaxed)) {
                        stopped.store(true, Ordering::Relaxed);
                        break;
                    }
                    mine.push((k, f(ps)));
                    if let Some(pr) = progress {
                        pr.done.fetch_add(ps.len(), Ordering::Relaxed);
                    }
                }
                done.lock().expect("not poisoned").extend(mine);
            });
        }
    });
    if stopped.load(Ordering::Relaxed) {
        return Err("stopped".into());
    }
    let mut out = vec![T::default(); points.len()];
    for (k, v) in done.into_inner().expect("not poisoned") {
        out[k * BLOCK..k * BLOCK + v.len()].copy_from_slice(&v);
    }
    Ok(out)
}

/// A vertical plane across the line: through `origin` (plan), offsets
/// along `right` (a unit vector in plan, to the right looking ahead),
/// heights as elevations.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct VerticalCut {
    /// The plane's point at offset 0 (on the axis), plan, m.
    pub origin: [f64; 2],
    /// To the right looking ahead (unit, plan).
    pub right: [f64; 2],
    /// From this offset (m; negative: to the left)…
    pub left: f64,
    /// …to this one, m.
    pub right_offset: f64,
    /// From this elevation…
    pub bottom: f64,
    /// …to this one, m.
    pub top: f64,
    /// Between the grid's points both ways, m.
    pub step: f64,
}

impl VerticalCut {
    /// The grid's offsets (columns) and elevations (rows).
    pub fn axes(&self) -> (Vec<f64>, Vec<f64>) {
        let n = |a: f64, b: f64| ((b - a) / self.step).round().max(1.0) as usize + 1;
        let cols = n(self.left, self.right_offset);
        let rows = n(self.bottom, self.top);
        let ds = (self.right_offset - self.left) / (cols - 1) as f64;
        let dz = (self.top - self.bottom) / (rows - 1) as f64;
        (
            (0..cols).map(|k| self.left + k as f64 * ds).collect(),
            (0..rows).map(|k| self.bottom + k as f64 * dz).collect(),
        )
    }

    /// The plan point at an offset.
    pub fn at(&self, offset: f64) -> [f64; 2] {
        [
            self.origin[0] + offset * self.right[0],
            self.origin[1] + offset * self.right[1],
        ]
    }

    /// The grid's points.
    pub fn points(&self) -> usize {
        let (c, r) = self.axes();
        c.len() * r.len()
    }
}

/// A vertical cut's values.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CutValues {
    /// The plane.
    pub cut: VerticalCut,
    /// The columns' offsets, m.
    pub offsets: Vec<f64>,
    /// The rows' elevations, m.
    pub elevations: Vec<f64>,
    /// The ground's elevation at each column (none: no ground known).
    pub ground: Vec<Option<f64>>,
    /// The values, row by row (`values[row * columns + column]`), every
    /// point computed (also under the ground; the drawing stops there).
    pub values: Vec<f64>,
}

/// The values on a vertical cut. `ground`: the terrain's elevation at a
/// plan point, when known.
pub fn vertical_cut(
    cut: &VerticalCut,
    field: Field<'_, f64>,
    ground: Option<&dyn Fn(f64, f64) -> Option<f64>>,
    progress: Option<&Progress>,
) -> Result<CutValues, String> {
    if !(cut.step > 0.0 && cut.right_offset > cut.left && cut.top > cut.bottom) {
        return Err("a cut with a step above 0 m, a width and a height".into());
    }
    let (offsets, elevations) = cut.axes();
    let mut points = Vec::with_capacity(offsets.len() * elevations.len());
    for &z in &elevations {
        for &s in &offsets {
            let p = cut.at(s);
            points.push([p[0], p[1], z]);
        }
    }
    let values = compute_all(&points, field, progress)?;
    let ground = offsets
        .iter()
        .map(|&s| {
            let p = cut.at(s);
            ground.and_then(|g| g(p[0], p[1]))
        })
        .collect();
    Ok(CutValues {
        cut: *cut,
        offsets,
        elevations,
        ground,
        values,
    })
}

/// A coloured piece of a cut: a polygon in (offset, elevation).
pub type Patch = Vec<[f64; 2]>;

impl CutValues {
    /// The isolines of `level`, (offset, elevation), cut at the ground.
    pub fn isolines(&self, level: f64) -> Vec<Vec<[f64; 2]>> {
        let (nc, nr) = (self.offsets.len(), self.elevations.len());
        let mut out = Vec::new();
        for line in contour::isolines(&self.values, nc, nr, level) {
            let pts: Vec<[f64; 2]> = line.iter().map(|p| self.place(*p)).collect();
            out.extend(self.above_ground(&pts));
        }
        out
    }

    /// How far the isolines of `level` lie off it ([`deviation`]).
    pub fn deviation(&self, level: f64, field: Field<'_, f64>) -> Option<Deviation> {
        let pts: Vec<[f64; 3]> = self
            .isolines(level)
            .iter()
            .flatten()
            .map(|q| {
                let p = self.cut.at(q[0]);
                [p[0], p[1], q[1]]
            })
            .collect();
        deviation(&pts, level, field)
    }

    /// A grid coordinate (column, row; fractional) as (offset, elevation).
    fn place(&self, p: [f64; 2]) -> [f64; 2] {
        let lerp = |v: &[f64], x: f64| {
            let k = (x.floor() as usize).min(v.len().saturating_sub(2));
            v[k] + (x - k as f64) * (v[k + 1] - v[k])
        };
        [lerp(&self.offsets, p[0]), lerp(&self.elevations, p[1])]
    }

    /// The ground's elevation at an offset (straight between the
    /// columns); none where it is not known.
    pub fn ground_at(&self, s: f64) -> Option<f64> {
        let o = &self.offsets;
        if o.len() < 2 {
            return self.ground.first().copied().flatten();
        }
        let k = o
            .partition_point(|&x| x <= s)
            .saturating_sub(1)
            .min(o.len() - 2);
        let (a, b) = (self.ground[k]?, self.ground[k + 1]?);
        let t = ((s - o[k]) / (o[k + 1] - o[k])).clamp(0.0, 1.0);
        Some(a + t * (b - a))
    }

    /// The parts of a polyline above the ground (all of it where the
    /// ground is not known).
    fn above_ground(&self, pts: &[[f64; 2]]) -> Vec<Vec<[f64; 2]>> {
        let h = |p: [f64; 2]| self.ground_at(p[0]).map_or(1.0, |g| p[1] - g);
        let mut out = Vec::new();
        let mut cur: Vec<[f64; 2]> = Vec::new();
        for w in pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let (ha, hb) = (h(a), h(b));
            let cross = |t: f64| [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])];
            match (ha >= 0.0, hb >= 0.0) {
                (true, true) => {
                    if cur.is_empty() {
                        cur.push(a);
                    }
                    cur.push(b);
                }
                (true, false) => {
                    if cur.is_empty() {
                        cur.push(a);
                    }
                    cur.push(cross(ha / (ha - hb)));
                    out.push(std::mem::take(&mut cur));
                }
                (false, true) => {
                    cur.push(cross(ha / (ha - hb)));
                    cur.push(b);
                }
                (false, false) => {}
            }
        }
        if cur.len() > 1 {
            out.push(cur);
        }
        out
    }

    /// The cells coloured by band: for each grid cell above the ground
    /// (cut at it), the polygon and the band of its centre's value — the
    /// number of `levels` (ascending) at or below it.
    pub fn bands(&self, levels: &[f64]) -> Vec<(usize, Patch)> {
        let (nc, nr) = (self.offsets.len(), self.elevations.len());
        let mut out = Vec::new();
        for r in 0..nr.saturating_sub(1) {
            for c in 0..nc.saturating_sub(1) {
                let q = [
                    self.values[r * nc + c],
                    self.values[r * nc + c + 1],
                    self.values[(r + 1) * nc + c + 1],
                    self.values[(r + 1) * nc + c],
                ];
                if q.iter().any(|x| !x.is_finite()) {
                    continue;
                }
                // The centre by the logarithms' mean (the values are
                // positive) else the mean.
                let centre = if q.iter().all(|&x| x > 0.0) {
                    (q.iter().map(|x| x.ln()).sum::<f64>() / 4.0).exp()
                } else {
                    q.iter().sum::<f64>() / 4.0
                };
                let band = levels.iter().filter(|&&l| centre >= l).count();
                if band == 0 {
                    continue;
                }
                let (s0, s1) = (self.offsets[c], self.offsets[c + 1]);
                let (z0, z1) = (self.elevations[r], self.elevations[r + 1]);
                let poly = vec![[s0, z0], [s1, z0], [s1, z1], [s0, z1]];
                let clipped = match (self.ground[c], self.ground[c + 1]) {
                    (Some(g0), Some(g1)) => clip_above(&poly, [s0, g0], [s1, g1]),
                    _ => poly,
                };
                if clipped.len() >= 3 {
                    out.push((band, clipped));
                }
            }
        }
        out
    }
}

/// A polygon's part above the line through `a` and `b` (Sutherland–
/// Hodgman with one edge).
fn clip_above(poly: &[[f64; 2]], a: [f64; 2], b: [f64; 2]) -> Patch {
    let h = |p: [f64; 2]| {
        let t = if (b[0] - a[0]).abs() > 1e-12 {
            (p[0] - a[0]) / (b[0] - a[0])
        } else {
            0.0
        };
        p[1] - (a[1] + t * (b[1] - a[1]))
    };
    let mut out = Vec::new();
    for k in 0..poly.len() {
        let (p, q) = (poly[k], poly[(k + 1) % poly.len()]);
        let (hp, hq) = (h(p), h(q));
        if hp >= 0.0 {
            out.push(p);
        }
        if (hp >= 0.0) != (hq >= 0.0) {
            let t = hp / (hp - hq);
            out.push([p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1])]);
        }
    }
    out
}

/// A volume's coarse lattice is this many fine steps apart.
pub const COARSE: usize = 4;

/// A coarse cell is computed fine when a level lies within this factor of
/// its corners' values (below the least, above the largest): a surface
/// that bulges into a cell without passing between its corners is found
/// (away from the wires a field changes smoothly over a coarse cell).
pub const MARGIN: f64 = 1.1;

/// A line along the ground in plan: points (x, y, the ground's elevation
/// there), joined straight. A place on it is a station `t` along it and an
/// offset `s` to its right; across a bend the offset follows the
/// bisector, mitred (the offset point lies `s` from both straights), and
/// along a straight the direction turns evenly from one end's to the
/// other's — so a lattice laid on it bends with the line without gaps or
/// overlaps at its angles (owner, 2026-10-29: continuity between tiles).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Path {
    /// The points (x, y, ground elevation).
    pub points: Vec<[f64; 3]>,
    /// Station of each point, m.
    stations: Vec<f64>,
    /// The offset's direction at each point (mitred: longer than 1 at a
    /// bend).
    normals: Vec<[f64; 2]>,
}

impl Path {
    /// A path through `points` (at least two, no two at one place).
    pub fn new(points: Vec<[f64; 3]>) -> Result<Self, String> {
        if points.len() < 2 {
            return Err("a path of at least two points".into());
        }
        let mut stations = vec![0.0];
        let mut dirs = Vec::new();
        for w in points.windows(2) {
            let (dx, dy) = (w[1][0] - w[0][0], w[1][1] - w[0][1]);
            let l = dx.hypot(dy);
            if l < 1e-6 {
                return Err("two points of the path at one place".into());
            }
            stations.push(stations.last().copied().unwrap_or(0.0) + l);
            dirs.push([dx / l, dy / l]);
        }
        let right = |d: [f64; 2]| [d[1], -d[0]];
        let mut normals = Vec::with_capacity(points.len());
        normals.push(right(dirs[0]));
        for k in 1..points.len() - 1 {
            let (a, b) = (right(dirs[k - 1]), right(dirs[k]));
            let m = [a[0] + b[0], a[1] + b[1]];
            let c = m[0] * a[0] + m[1] * a[1];
            if c.abs() < 1e-9 {
                return Err("the path turns back on itself".into());
            }
            // Mitred: its part along either straight's normal is 1.
            normals.push([m[0] / c, m[1] / c]);
        }
        normals.push(right(dirs[dirs.len() - 1]));
        Ok(Self {
            points,
            stations,
            normals,
        })
    }

    /// Its length, m.
    pub fn length(&self) -> f64 {
        self.stations.last().copied().unwrap_or(0.0)
    }

    /// The straight a station lies on and how far along it (0–1).
    fn piece(&self, t: f64) -> (usize, f64) {
        let n = self.points.len();
        let k = self
            .stations
            .partition_point(|&x| x <= t)
            .saturating_sub(1)
            .min(n - 2);
        let f = (t - self.stations[k]) / (self.stations[k + 1] - self.stations[k]);
        (k, f.clamp(0.0, 1.0))
    }

    /// The plan point at station `t`, offset `s` to the right.
    pub fn place(&self, t: f64, s: f64) -> [f64; 2] {
        let (k, f) = self.piece(t);
        let (a, b) = (self.points[k], self.points[k + 1]);
        let (na, nb) = (self.normals[k], self.normals[k + 1]);
        [
            a[0] + f * (b[0] - a[0]) + s * (na[0] + f * (nb[0] - na[0])),
            a[1] + f * (b[1] - a[1]) + s * (na[1] + f * (nb[1] - na[1])),
        ]
    }

    /// [`Path::place`], beyond the path's ends too: on along its first or
    /// last straight.
    pub fn place_beyond(&self, t: f64, s: f64) -> [f64; 2] {
        let len = self.length();
        let (k, base, extra) = if t < 0.0 {
            (0, 0.0, t)
        } else if t > len {
            (self.points.len() - 2, len, t - len)
        } else {
            return self.place(t, s);
        };
        let (a, b) = (self.points[k], self.points[k + 1]);
        let l = (b[0] - a[0]).hypot(b[1] - a[1]);
        let d = [(b[0] - a[0]) / l, (b[1] - a[1]) / l];
        let p = self.place(base, s);
        [p[0] + extra * d[0], p[1] + extra * d[1]]
    }

    /// The ground's elevation at station `t` (straight between the
    /// points).
    pub fn ground(&self, t: f64) -> f64 {
        let (k, f) = self.piece(t);
        self.points[k][2] + f * (self.points[k + 1][2] - self.points[k][2])
    }

    /// The plan distance from `p` to the path's line (its straights).
    pub fn distance(&self, p: [f64; 2]) -> f64 {
        self.points
            .windows(2)
            .map(|w| {
                let (dx, dy) = (w[1][0] - w[0][0], w[1][1] - w[0][1]);
                let l2 = dx * dx + dy * dy;
                let u = (((p[0] - w[0][0]) * dx + (p[1] - w[0][1]) * dy) / l2).clamp(0.0, 1.0);
                (p[0] - w[0][0] - u * dx).hypot(p[1] - w[0][1] - u * dy)
            })
            .fold(f64::INFINITY, f64::min)
    }

    /// Whether offsets up to `width` either side stay unfolded: the
    /// places at a station move ahead with it everywhere across.
    pub fn unfolded(&self, width: f64) -> bool {
        (0..self.points.len() - 1).all(|k| {
            let l = self.stations[k + 1] - self.stations[k];
            let d = [
                (self.points[k + 1][0] - self.points[k][0]) / l,
                (self.points[k + 1][1] - self.points[k][1]) / l,
            ];
            let dn = [
                (self.normals[k + 1][0] - self.normals[k][0]) / l,
                (self.normals[k + 1][1] - self.normals[k][1]) / l,
            ];
            [-width, width]
                .iter()
                .all(|&s| d[0] * (d[0] + s * dn[0]) + d[1] * (d[1] + s * dn[1]) > 0.1)
        })
    }
}

/// A volume along a path: `width` to either side of it, elevations
/// `bottom` to `top`, the fine lattice `step` apart (stations, offsets,
/// heights).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct VolumeBox {
    /// Along what.
    pub path: Path,
    /// To either side, m.
    pub width: f64,
    /// From this elevation…
    pub bottom: f64,
    /// …to this one, m.
    pub top: f64,
    /// The fine lattice's step, m.
    pub step: f64,
}

impl VolumeBox {
    /// The fine lattice's points per axis (along, across, up), a multiple
    /// of [`COARSE`] plus one each; the last station is the path's end
    /// (the lattice's steps along it a little shorter than `step`).
    pub fn dims(&self) -> [usize; 3] {
        let n = |len: f64| {
            let cells = (len / self.step).ceil().max(1.0) as usize;
            cells.div_ceil(COARSE) * COARSE + 1
        };
        [
            n(self.path.length()),
            n(2.0 * self.width),
            n(self.top - self.bottom),
        ]
    }

    /// A lattice point's place (lattice coordinates may be fractional).
    pub fn place(&self, p: [f64; 3]) -> [f64; 3] {
        let along = self.path.length() / (self.dims()[0] - 1) as f64;
        let xy = self
            .path
            .place(p[0] * along, -self.width + p[1] * self.step);
        [xy[0], xy[1], self.bottom + p[2] * self.step]
    }

    /// Points of the full fine lattice and of the coarse one.
    pub fn counts(&self) -> (usize, usize) {
        let d = self.dims();
        let c = d.map(|n| (n - 1) / COARSE + 1);
        (d[0] * d[1] * d[2], c[0] * c[1] * c[2])
    }
}

/// Vertices checked per surface or isoline set.
pub const CHECKED: usize = 200;

/// How far a drawing lies off its level: the field computed at points of
/// it, the relative difference to the level.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Deviation {
    /// The points checked.
    pub points: usize,
    /// Half of them lie closer than this (a fraction: 0.01 = 1 %).
    pub median: f64,
    /// Nine in ten lie closer than this.
    pub p90: f64,
    /// The largest.
    pub max: f64,
}

/// The field at up to [`CHECKED`] of `points` (spread evenly) against the
/// level.
pub fn deviation(points: &[[f64; 3]], level: f64, field: Field<'_, f64>) -> Option<Deviation> {
    if points.is_empty() || level.is_nan() || level <= 0.0 {
        return None;
    }
    let every = points.len().div_ceil(CHECKED).max(1);
    let picked: Vec<[f64; 3]> = points.iter().step_by(every).copied().collect();
    let mut d: Vec<f64> = compute_all(&picked, field, None)
        .ok()?
        .iter()
        .map(|v| (v - level).abs() / level)
        .collect();
    d.sort_by(f64::total_cmp);
    let at = |q: f64| d[((d.len() - 1) as f64 * q).round() as usize];
    Some(Deviation {
        points: d.len(),
        median: at(0.5),
        p90: at(0.9),
        max: at(1.0),
    })
}

/// A volume's surfaces and what was computed.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct VolumeValues {
    /// The box.
    pub bounds: VolumeBox,
    /// Per level asked: the level and its surface (world coordinates).
    pub surfaces: Vec<(f64, Mesh)>,
    /// Per level: how far the drawn surface lies off its level, the
    /// field computed at up to [`CHECKED`] of its vertices
    /// ([`Deviation`]); none without vertices.
    pub deviation: Vec<Option<Deviation>>,
    /// Points computed.
    pub computed: usize,
    /// Points of the full fine lattice.
    pub full: usize,
    /// Per level: the share of the coarse lattice's points at or above it
    /// (1: the level lies beyond the box everywhere, 0: not reached).
    pub above: Vec<f64>,
    /// Per level: the thin, short pieces of surface left out
    /// ([`THIN_PIECE`], [`SHORT_PIECE`]; owner, 2026-10-31: beads along an
    /// earth wire, a tube centimetres thick the lattice met here and
    /// there).
    pub left_out: Vec<usize>,
}

/// What a volume's level left out ([`VolumeValues::left_out`]), in
/// words after its other notes (empty: nothing).
pub fn left_out_text(pieces: usize) -> String {
    if pieces == 0 {
        String::new()
    } else {
        format!(
            "; {pieces} thin piece(s) left out (at most {THIN_PIECE} steps across, \
             {SHORT_PIECE} long: a tube thinner than the lattice, e.g. round an earth wire, \
             met here and there; a finer step resolves it)"
        )
    }
}

/// A piece of an isosurface at most this many steps of its lattice across
/// in two directions and at most [`SHORT_PIECE`] long is left out of a
/// volume: a stretch of a tube thinner than the lattice resolves (round an
/// earth wire at a high level, met only where a lattice point happens to
/// lie inside it), drawn as scattered beads. On a real line the beads
/// along an earth wire at 5 kV/m and 2 m steps were 2–3 m thick and
/// 4–21 m long; the surface round the conductors, 650 m. Said with each
/// volume; a finer step resolves them.
pub const THIN_PIECE: f64 = 2.0;

/// See [`THIN_PIECE`]: the longest piece left out, steps.
pub const SHORT_PIECE: f64 = 20.0;

/// The isosurfaces of `levels` in a box. `pieces`: the wires' straight
/// pieces (a coarse cell within a coarse step of one is computed fine).
pub fn volume(
    bounds: &VolumeBox,
    levels: &[f64],
    field: Field<'_, f64>,
    pieces: &[[[f64; 3]; 2]],
    progress: Option<&Progress>,
) -> Result<VolumeValues, String> {
    if !(bounds.step > 0.0
        && bounds.path.length() > 0.0
        && bounds.width > 0.0
        && bounds.top > bounds.bottom)
    {
        return Err("a volume with a step above 0 m, a length, a width and a height".into());
    }
    if !bounds.path.unfolded(bounds.width) {
        return Err(format!(
            "±{} m folds over itself at a sharp line angle: a smaller width",
            bounds.width
        ));
    }
    let d = bounds.dims();
    let full = d[0] * d[1] * d[2];
    if full > 400_000_000 {
        return Err(format!(
            "{full} lattice points: too many (a coarser step or a smaller box)"
        ));
    }
    let id = |i: usize, j: usize, k: usize| (k * d[1] + j) * d[0] + i;
    let mut values = vec![f64::NAN; full];
    // The coarse lattice.
    let c = d.map(|n| (n - 1) / COARSE + 1);
    let mut todo: Vec<usize> = Vec::new();
    for k in 0..c[2] {
        for j in 0..c[1] {
            for i in 0..c[0] {
                todo.push(id(i * COARSE, j * COARSE, k * COARSE));
            }
        }
    }
    // Added to the progress's points (the fine ones once known).
    let add_total = |n: usize| {
        if let Some(p) = progress {
            p.total.fetch_add(n, Ordering::Relaxed);
        }
    };
    add_total(todo.len());
    let coords = |n: usize| {
        let i = n % d[0];
        let j = (n / d[0]) % d[1];
        let k = n / (d[0] * d[1]);
        [i as f64, j as f64, k as f64]
    };
    let compute = |todo: &[usize], values: &mut Vec<f64>| -> Result<(), String> {
        let pts: Vec<[f64; 3]> = todo.iter().map(|&n| bounds.place(coords(n))).collect();
        let v = compute_all(&pts, field, progress)?;
        for (&n, x) in todo.iter().zip(v) {
            values[n] = x;
        }
        Ok(())
    };
    compute(&todo, &mut values)?;
    // The coarse cells to refine.
    let cells = [c[0] - 1, c[1] - 1, c[2] - 1];
    let cid = |i: usize, j: usize, k: usize| (k * cells[1] + j) * cells[0] + i;
    let mut flag = vec![false; cells[0] * cells[1] * cells[2]];
    for k in 0..cells[2] {
        for j in 0..cells[1] {
            for i in 0..cells[0] {
                let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
                for corner in 0..8 {
                    let v = values[id(
                        (i + (corner & 1)) * COARSE,
                        (j + ((corner >> 1) & 1)) * COARSE,
                        (k + ((corner >> 2) & 1)) * COARSE,
                    )];
                    lo = lo.min(v);
                    hi = hi.max(v);
                }
                if levels.iter().any(|&l| lo / MARGIN <= l && l <= hi * MARGIN) {
                    flag[cid(i, j, k)] = true;
                }
            }
        }
    }
    // Near a wire piece: the cells whose centre lies within two coarse
    // steps of one (pieces found through a plan grid of their middles).
    let coarse = bounds.step * COARSE as f64;
    let cell = 2.0 * coarse;
    let mut hash: std::collections::HashMap<(i64, i64), Vec<usize>> =
        std::collections::HashMap::new();
    let mut reach: f64 = 0.0;
    for (n, piece) in pieces.iter().enumerate() {
        let m = [
            0.5 * (piece[0][0] + piece[1][0]),
            0.5 * (piece[0][1] + piece[1][1]),
        ];
        let half = 0.5
            * ((piece[1][0] - piece[0][0]).powi(2)
                + (piece[1][1] - piece[0][1]).powi(2)
                + (piece[1][2] - piece[0][2]).powi(2))
            .sqrt();
        reach = reach.max(half);
        hash.entry(((m[0] / cell).floor() as i64, (m[1] / cell).floor() as i64))
            .or_default()
            .push(n);
    }
    let near = 2.0 * coarse;
    let span = ((near + reach) / cell).ceil() as i64;
    for k in 0..cells[2] {
        for j in 0..cells[1] {
            for i in 0..cells[0] {
                if flag[cid(i, j, k)] {
                    continue;
                }
                let c = bounds.place([
                    (i * COARSE) as f64 + 0.5 * COARSE as f64,
                    (j * COARSE) as f64 + 0.5 * COARSE as f64,
                    (k * COARSE) as f64 + 0.5 * COARSE as f64,
                ]);
                let (hx, hy) = ((c[0] / cell).floor() as i64, (c[1] / cell).floor() as i64);
                let mut hit = false;
                'search: for gx in hx - span..=hx + span {
                    for gy in hy - span..=hy + span {
                        for &n in hash.get(&(gx, gy)).into_iter().flatten() {
                            if segment_distance(pieces[n], c) <= near {
                                hit = true;
                                break 'search;
                            }
                        }
                    }
                }
                if hit {
                    flag[cid(i, j, k)] = true;
                }
            }
        }
    }
    // Their fine points; then, while a level crosses a face of a cell
    // computed fine, the cell beyond it too (the surface goes on there).
    let mut new_cells: Vec<[usize; 3]> = Vec::new();
    for k in 0..cells[2] {
        for j in 0..cells[1] {
            for i in 0..cells[0] {
                if flag[cid(i, j, k)] {
                    new_cells.push([i, j, k]);
                }
            }
        }
    }
    while !new_cells.is_empty() {
        check(progress)?;
        let mut fine = Vec::new();
        for &[i, j, k] in &new_cells {
            for fk in k * COARSE..=(k + 1) * COARSE {
                for fj in j * COARSE..=(j + 1) * COARSE {
                    for fi in i * COARSE..=(i + 1) * COARSE {
                        let n = id(fi, fj, fk);
                        if values[n].is_nan() {
                            values[n] = f64::INFINITY; // queued
                            fine.push(n);
                        }
                    }
                }
            }
        }
        add_total(fine.len());
        compute(&fine, &mut values)?;
        let mut next = Vec::new();
        for &c in &new_cells {
            for axis in 0..3 {
                for side in [0usize, 1] {
                    let mut beyond = c;
                    if side == 0 {
                        if c[axis] == 0 {
                            continue;
                        }
                        beyond[axis] -= 1;
                    } else {
                        if c[axis] + 1 >= cells[axis] {
                            continue;
                        }
                        beyond[axis] += 1;
                    }
                    if flag[cid(beyond[0], beyond[1], beyond[2])] {
                        continue;
                    }
                    // The face's fine values.
                    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
                    let fixed = (c[axis] + side) * COARSE;
                    let (a1, a2) = ((axis + 1) % 3, (axis + 2) % 3);
                    for u in c[a1] * COARSE..=(c[a1] + 1) * COARSE {
                        for w in c[a2] * COARSE..=(c[a2] + 1) * COARSE {
                            let mut f = [0usize; 3];
                            f[axis] = fixed;
                            f[a1] = u;
                            f[a2] = w;
                            let v = values[id(f[0], f[1], f[2])];
                            lo = lo.min(v);
                            hi = hi.max(v);
                        }
                    }
                    if levels.iter().any(|&l| lo < l && l <= hi) {
                        flag[cid(beyond[0], beyond[1], beyond[2])] = true;
                        next.push(beyond);
                    }
                }
            }
        }
        new_cells = next;
    }
    let computed = values.iter().filter(|v| !v.is_nan()).count();
    let coarse_values: Vec<f64> = (0..c[2])
        .flat_map(|k| (0..c[1]).flat_map(move |j| (0..c[0]).map(move |i| (i, j, k))))
        .map(|(i, j, k)| values[id(i * COARSE, j * COARSE, k * COARSE)])
        .collect();
    let above = levels
        .iter()
        .map(|&l| {
            coarse_values.iter().filter(|&&v| v >= l).count() as f64
                / coarse_values.len().max(1) as f64
        })
        .collect();
    // The surfaces: in the cells computed fine (a fine cell with a corner
    // not computed is skipped by the contouring).
    let value = |i: usize, j: usize, k: usize| values[id(i, j, k)];
    let mut left_out = Vec::new();
    let surfaces: Vec<(f64, Mesh)> = levels
        .iter()
        .map(|&l| {
            let (mut m, dropped) =
                contour::isosurface(d, &value, l).without_thin_pieces(THIN_PIECE, SHORT_PIECE);
            left_out.push(dropped);
            for v in &mut m.vertices {
                *v = bounds.place(*v);
            }
            (l, m)
        })
        .collect();
    let deviation = surfaces
        .iter()
        .map(|(l, m)| deviation(&m.vertices, *l, field))
        .collect();
    Ok(VolumeValues {
        bounds: bounds.clone(),
        surfaces,
        deviation,
        computed,
        full,
        above,
        left_out,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The time left counts the floors still to solve, not only the
    /// points (owner, 2026-10-30: "announcing 25 seconds since 5
    /// minutes"): a floor solved, its points computed, nine planned
    /// floors still say about nine floors' time.
    #[test]
    fn the_time_left_counts_the_floors_to_come() {
        use std::time::Duration;
        let p = Progress::default();
        p.total.store(1000, Ordering::Relaxed);
        for _ in 0..10 {
            p.plan_floor(3700);
        }
        // Before anything: the floors as expected.
        let first = p.seconds_left(Duration::ZERO).unwrap();
        let expected = 10.0 * crate::electric::TiledFloor::seconds(3700);
        assert!(
            (first - expected).abs() < 0.01 * expected,
            "{first} {expected}"
        );
        let token = p.floor_begins(3700);
        std::thread::sleep(Duration::from_millis(20));
        p.floor_ends(token);
        p.done.store(100, Ordering::Relaxed);
        // The floor took 20 ms against its expected time: the nine left
        // at that speed, and the points left at theirs.
        let left = p.seconds_left(Duration::from_millis(25)).unwrap();
        let per_floor = p.floor_speed().unwrap() * crate::electric::TiledFloor::seconds(3700);
        assert!(left > 8.0 * per_floor, "{left} against {per_floor} a floor");
        assert!(left < 12.0 * per_floor + 0.1, "{left}");
        // Without floors: the points' rate alone, as before.
        let q = Progress::default();
        q.total.store(100, Ordering::Relaxed);
        q.done.store(50, Ordering::Relaxed);
        let l = q.seconds_left(Duration::from_secs(10)).unwrap();
        assert!((l - 10.0).abs() < 1e-9);
    }

    /// One straight wire along x at 20 m: its field falls as 1/r, so the
    /// cut's isoline of 1/r₀ is a circle of radius r₀ around it, cut at
    /// the ground (z = 0).
    #[test]
    fn a_cut_across_a_straight_wire() {
        let f = |p: [f64; 3]| 1.0 / (p[1].powi(2) + (p[2] - 20.0).powi(2)).sqrt().max(1e-6);
        let cut = VerticalCut {
            origin: [0.0, 0.0],
            right: [0.0, -1.0],
            left: -40.0,
            right_offset: 40.0,
            bottom: -5.0,
            top: 50.0,
            step: 0.5,
        };
        let flat = |_x: f64, _y: f64| Some(0.0);
        let v = vertical_cut(&cut, &each(f), Some(&flat), None).unwrap();
        // r₀ = 10 m: a closed circle above the ground.
        let lines = v.isolines(0.1);
        assert_eq!(lines.len(), 1);
        for p in &lines[0] {
            let r = (p[0].powi(2) + (p[1] - 20.0).powi(2)).sqrt();
            assert!((r - 10.0).abs() < 0.02, "{r}");
        }
        // r₀ = 25 m: cut at the ground, every point above it.
        let lines = v.isolines(1.0 / 25.0);
        assert!(!lines.is_empty());
        for l in &lines {
            for p in l {
                assert!(p[1] >= -1e-9, "{p:?}");
            }
        }
        // Bands: the cells within 10 m of the wire in band 2 of [0.04, 0.1].
        let bands = v.bands(&[0.04, 0.1]);
        assert!(bands.iter().any(|(b, _)| *b == 2));
        assert!(bands.iter().all(|(_, p)| p.iter().all(|q| q[1] >= -1e-9)));
    }

    /// A volume around a straight wire: the surface of 1/r₀ a tube of
    /// radius r₀ (also when r₀ is smaller than a coarse step: the cells
    /// near the wire are computed fine), and far fewer points computed
    /// than the full lattice.
    #[test]
    fn a_volume_around_a_straight_wire() {
        let f = |p: [f64; 3]| 1.0 / (p[1].powi(2) + (p[2] - 30.0).powi(2)).sqrt().max(1e-6);
        let b = VolumeBox {
            path: Path::new(vec![[0.0, 0.0, 0.0], [40.0, 0.0, 0.0]]).unwrap(),
            width: 40.0,
            bottom: 0.0,
            top: 60.0,
            step: 0.5,
        };
        let pieces = [[[-10.0, 0.0, 30.0], [50.0, 0.0, 30.0]]];
        let v = volume(&b, &[1.0 / 1.2, 1.0 / 15.0], &each(f), &pieces, None).unwrap();
        assert!(v.computed * 3 < v.full, "{} of {}", v.computed, v.full);
        for (level, m) in &v.surfaces {
            let r0 = 1.0 / level;
            assert!(!m.triangles.is_empty(), "level {level}");
            for p in &m.vertices {
                let r = (p[1].powi(2) + (p[2] - 30.0).powi(2)).sqrt();
                assert!((r - r0).abs() < 0.05 * r0 + 0.05, "{r} vs {r0}");
            }
        }
    }

    /// A path's places at a bend: the same on either side of the bend's
    /// station, the offset point `s` from both straights (mitred).
    #[test]
    fn a_path_bends_without_a_gap() {
        let p = Path::new(vec![
            [0.0, 0.0, 10.0],
            [100.0, 0.0, 20.0],
            [200.0, 100.0, 0.0],
        ])
        .unwrap();
        let t = 100.0;
        for s in [-30.0, 0.0, 30.0] {
            let (a, b) = (p.place(t - 1e-9, s), p.place(t + 1e-9, s));
            assert!((a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6);
        }
        let q = p.place(t, 30.0);
        // From the first straight (y = 0) and the second (through
        // (100, 0) at 45°), both 30 m.
        assert!((q[1].abs() - 30.0).abs() < 1e-9, "{q:?}");
        let d2 = ((q[0] - 100.0) - q[1]).abs() / 2f64.sqrt();
        assert!((d2 - 30.0).abs() < 1e-9, "{d2}");
        assert!((p.ground(150.0) - (20.0 - 20.0 * (50.0 / (100.0 * 2f64.sqrt())))).abs() < 1e-9);
        assert!(p.unfolded(50.0));
        assert!(!p.unfolded(1000.0));
    }

    /// A volume along a bent path around a wire bent alike: one tube,
    /// closed but at the path's two ends (no seam at the bend).
    #[test]
    fn a_volume_bends_with_its_path() {
        let path = Path::new(vec![[0.0, 0.0, 0.0], [60.0, 0.0, 0.0], [110.0, 40.0, 0.0]]).unwrap();
        let wire: Vec<[f64; 3]> = vec![[0.0, 0.0, 30.0], [60.0, 0.0, 30.0], [110.0, 40.0, 30.0]];
        let pieces: Vec<[[f64; 3]; 2]> = wire.windows(2).map(|w| [w[0], w[1]]).collect();
        let f = |p: [f64; 3]| {
            let d = pieces
                .iter()
                .map(|s| segment_distance(*s, p))
                .fold(f64::INFINITY, f64::min);
            1.0 / d.max(1e-6)
        };
        let b = VolumeBox {
            path,
            width: 20.0,
            bottom: 10.0,
            top: 50.0,
            step: 1.0,
        };
        let v = volume(&b, &[1.0 / 5.0], &each(f), &pieces, None).unwrap();
        let m = &v.surfaces[0].1;
        assert!(m.triangles.len() > 500);
        let mut edges = std::collections::HashMap::new();
        for t in &m.triangles {
            for e in 0..3 {
                let (a, c) = (t[e], t[(e + 1) % 3]);
                *edges.entry((a.min(c), a.max(c))).or_insert(0) += 1;
            }
        }
        let near_end = |p: [f64; 3]| {
            [wire[0], wire[2]].iter().any(|e| {
                ((p[0] - e[0]).powi(2) + (p[1] - e[1]).powi(2) + (p[2] - e[2]).powi(2)).sqrt() < 6.5
            })
        };
        for ((a, c), n) in &edges {
            if *n == 1 {
                let (p, q) = (m.vertices[*a as usize], m.vertices[*c as usize]);
                assert!(near_end(p) && near_end(q), "a hole at {p:?}");
            }
        }
    }
}
