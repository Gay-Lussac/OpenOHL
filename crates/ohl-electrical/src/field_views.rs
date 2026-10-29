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

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::contour::{self, Mesh};

/// Progress of a run and a way to stop it (shared with another thread).
#[derive(Debug, Default)]
pub struct Progress {
    /// Set to stop: the run returns an error at its next point.
    pub cancel: AtomicBool,
    /// Points computed.
    pub done: AtomicUsize,
    /// Points to compute.
    pub total: AtomicUsize,
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

/// A box along a stretch of the line: from `start` along `ahead` (unit,
/// plan) for `length`, `width` to either side, elevations `bottom` to
/// `top`.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct VolumeBox {
    /// Start on the axis, plan, m.
    pub start: [f64; 2],
    /// Ahead (unit, plan).
    pub ahead: [f64; 2],
    /// Along, m.
    pub length: f64,
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
    /// of [`COARSE`] plus one each.
    pub fn dims(&self) -> [usize; 3] {
        let n = |len: f64| {
            let cells = (len / self.step).ceil().max(1.0) as usize;
            cells.div_ceil(COARSE) * COARSE + 1
        };
        [
            n(self.length),
            n(2.0 * self.width),
            n(self.top - self.bottom),
        ]
    }

    /// A lattice point's place (lattice coordinates may be fractional).
    pub fn place(&self, p: [f64; 3]) -> [f64; 3] {
        let (t, s, z) = (
            p[0] * self.step,
            -self.width + p[1] * self.step,
            self.bottom + p[2] * self.step,
        );
        // Right of ahead: (ahead.y, −ahead.x).
        [
            self.start[0] + t * self.ahead[0] + s * self.ahead[1],
            self.start[1] + t * self.ahead[1] - s * self.ahead[0],
            z,
        ]
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
}

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
        && bounds.length > 0.0
        && bounds.width > 0.0
        && bounds.top > bounds.bottom)
    {
        return Err("a volume with a step above 0 m, a length, a width and a height".into());
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
    // Near a wire piece: the cells its box (grown by a coarse step)
    // meets, in lattice coordinates.
    let coarse = bounds.step * COARSE as f64;
    let local = |p: [f64; 3]| {
        let (dx, dy) = (p[0] - bounds.start[0], p[1] - bounds.start[1]);
        let t = dx * bounds.ahead[0] + dy * bounds.ahead[1];
        let s = dx * bounds.ahead[1] - dy * bounds.ahead[0];
        [t, s + bounds.width, p[2] - bounds.bottom]
    };
    for piece in pieces {
        let (a, b) = (local(piece[0]), local(piece[1]));
        let mut range = [(0usize, 0usize); 3];
        let mut inside = true;
        for ax in 0..3 {
            let lo = (a[ax].min(b[ax]) - coarse) / coarse;
            let hi = (a[ax].max(b[ax]) + coarse) / coarse;
            if hi < 0.0 || lo > cells[ax] as f64 {
                inside = false;
                break;
            }
            range[ax] = (
                lo.floor().max(0.0) as usize,
                (hi.ceil().max(0.0) as usize).min(cells[ax].saturating_sub(1)),
            );
        }
        if !inside {
            continue;
        }
        for k in range[2].0..=range[2].1 {
            for j in range[1].0..=range[1].1 {
                for i in range[0].0..=range[0].1 {
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
    let surfaces: Vec<(f64, Mesh)> = levels
        .iter()
        .map(|&l| {
            let mut m = contour::isosurface(d, &value, l);
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
        bounds: *bounds,
        surfaces,
        deviation,
        computed,
        full,
        above,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
            start: [0.0, 0.0],
            ahead: [1.0, 0.0],
            length: 40.0,
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
}
