// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The mesh made coarser, only outward (theory: Safety volumes › Fewer
//! triangles): edges collapsed one end onto the other, the flattest first
//! (quadric error), each collapse kept only when
//!
//! - the surface stays a closed manifold (the link condition, no
//!   triangle folded or doubled);
//! - every new triangle is **out of the envelope** — at least its radius
//!   from every prim's core, by exact distances
//!   ([`Field::triangle_clear`]);
//! - every new triangle stays near it: \\( f \\) at its centroid at most
//!   `out_max`, and the quadric error (the sum of
//!   the squared distances to the planes merged) at most `tau²`.
//!
//! The result is then checked as a whole: closed, no triangle in the
//! envelope, and one point of every prim inside it (each part of the
//! envelope holds whole prims, so each part is inside). A mesh failing
//! the check is not used (the caller keeps the finer one).

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashSet};

use ohl_core::budget::Budget;

use super::mesher::{Field, Mesh, PrimShape};
use crate::geometry::P3;

fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: P3, b: P3) -> P3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn mid(a: P3, b: P3) -> P3 {
    [
        0.5 * (a[0] + b[0]),
        0.5 * (a[1] + b[1]),
        0.5 * (a[2] + b[2]),
    ]
}

/// The radius of the smallest circle holding the triangle: half its
/// longest side when that side's angle is obtuse, else its circumradius.
fn enclosing_radius(t: [P3; 3]) -> f64 {
    let l2 = |a: P3, b: P3| {
        let d = sub(a, b);
        dot(d, d)
    };
    let (a2, b2, c2) = (l2(t[1], t[2]), l2(t[2], t[0]), l2(t[0], t[1]));
    let m = a2.max(b2).max(c2);
    if 2.0 * m >= a2 + b2 + c2 {
        return 0.5 * m.sqrt();
    }
    let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
    let area2 = dot(n, n).sqrt();
    if area2 <= 0.0 {
        return 0.5 * m.sqrt();
    }
    (a2 * b2 * c2).sqrt() / (2.0 * area2)
}

/// A plane quadric (the upper triangle of a symmetric 4×4).
type Quadric = [f64; 10];

/// The quadric of the triangle's plane, weighted by its area, and the
/// area.
fn plane_quadric(a: P3, b: P3, c: P3) -> (Quadric, f64) {
    let n = cross(sub(b, a), sub(c, a));
    let l = dot(n, n).sqrt();
    if l <= 0.0 {
        return ([0.0; 10], 0.0);
    }
    let area = 0.5 * l;
    let n = [n[0] / l, n[1] / l, n[2] / l];
    let d = -dot(n, a);
    let v = [n[0], n[1], n[2], d];
    let mut q = [0.0; 10];
    let mut k = 0;
    for i in 0..4 {
        for j in i..4 {
            q[k] = area * v[i] * v[j];
            k += 1;
        }
    }
    (q, area)
}

fn quadric_error(q: &Quadric, p: P3) -> f64 {
    let v = [p[0], p[1], p[2], 1.0];
    let mut e = 0.0;
    let mut k = 0;
    for i in 0..4 {
        for j in i..4 {
            let w = if i == j { 1.0 } else { 2.0 };
            e += w * q[k] * v[i] * v[j];
            k += 1;
        }
    }
    e.max(0.0)
}

/// A collapse waiting: its cost, `from` onto `to`, the stamps of both.
#[derive(Debug, Clone, Copy)]
struct Candidate {
    cost: f64,
    from: u32,
    to: u32,
    stamps: (u32, u32),
}

impl PartialEq for Candidate {
    fn eq(&self, o: &Self) -> bool {
        self.cost == o.cost
    }
}
impl Eq for Candidate {}
impl PartialOrd for Candidate {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Candidate {
    fn cmp(&self, o: &Self) -> Ordering {
        // The smallest cost first.
        o.cost.total_cmp(&self.cost)
    }
}

/// What the simplification did.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SimplifyStats {
    /// Triangles before.
    pub before: usize,
    /// Triangles after.
    pub after: usize,
    /// Collapses refused because a triangle would enter the envelope.
    pub refused_inside: u64,
    /// The check of the whole result passed.
    pub checked: bool,
}

struct State<'a> {
    field: &'a Field,
    v: Vec<P3>,
    q: Vec<Quadric>,
    /// The area each quadric holds.
    qa: Vec<f64>,
    /// Vertices on a part's border: never moved.
    locked: Vec<bool>,
    stamp: Vec<u32>,
    alive_v: Vec<bool>,
    tris: Vec<[u32; 3]>,
    alive_t: Vec<bool>,
    of: Vec<Vec<u32>>,
    tau2: f64,
    out_max: f64,
    refused_inside: u64,
    /// f at each vertex.
    fv: Vec<f64>,
    /// The smallest prim radius.
    r_min: f64,
}

impl State<'_> {
    fn neighbours(&self, u: u32) -> HashSet<u32> {
        let mut s = HashSet::new();
        for &t in &self.of[u as usize] {
            for &x in &self.tris[t as usize] {
                if x != u {
                    s.insert(x);
                }
            }
        }
        s
    }

    /// The collapse of the edge a–b onto its cheaper end (none when an
    /// end is locked): its cost the mean squared distance to the planes
    /// merged (area-weighted).
    fn candidate(&self, a: u32, b: u32) -> Option<Candidate> {
        if self.locked[a as usize] || self.locked[b as usize] {
            return None;
        }
        let mut q = self.q[a as usize];
        for (x, y) in q.iter_mut().zip(&self.q[b as usize]) {
            *x += y;
        }
        let area = (self.qa[a as usize] + self.qa[b as usize]).max(1e-12);
        let (ea, eb) = (
            quadric_error(&q, self.v[b as usize]) / area,
            quadric_error(&q, self.v[a as usize]) / area,
        );
        let (from, to, cost) = if ea <= eb { (a, b, ea) } else { (b, a, eb) };
        Some(Candidate {
            cost,
            from,
            to,
            stamps: (self.stamp[from as usize], self.stamp[to as usize]),
        })
    }

    /// Tries the collapse; true when made.
    fn collapse(&mut self, c: &Candidate) -> bool {
        let (u, v) = (c.from, c.to);
        if !self.alive_v[u as usize] || !self.alive_v[v as usize] {
            return false;
        }
        if c.cost > self.tau2 {
            return false;
        }
        let shared: Vec<u32> = self.of[u as usize]
            .iter()
            .copied()
            .filter(|&t| self.tris[t as usize].contains(&v))
            .collect();
        if shared.len() != 2 {
            return false;
        }
        // Link condition: the common neighbours are the two opposite
        // corners only.
        let (nu, nv) = (self.neighbours(u), self.neighbours(v));
        let common = nu.intersection(&nv).count();
        if common != 2 || nu.len() < 3 || nv.len() < 3 {
            return false;
        }
        // The new triangles.
        let mut changed: Vec<(u32, [u32; 3])> = Vec::new();
        for &t in &self.of[u as usize] {
            let tri = self.tris[t as usize];
            if tri.contains(&v) {
                continue;
            }
            let new = tri.map(|x| if x == u { v } else { x });
            let [a, b, cc] = new.map(|x| self.v[x as usize]);
            let n_new = cross(sub(b, a), sub(cc, a));
            let [a0, b0, c0] = tri.map(|x| self.v[x as usize]);
            let n_old = cross(sub(b0, a0), sub(c0, a0));
            let (ln, lo) = (dot(n_new, n_new).sqrt(), dot(n_old, n_old).sqrt());
            if ln <= 1e-12 || dot(n_new, n_old) < 0.5 * ln * lo {
                return false;
            }
            changed.push((t, new));
        }
        // No triangle twice.
        for (_, new) in &changed {
            let mut key = *new;
            key.sort_unstable();
            for &t in &self.of[v as usize] {
                let mut k2 = self.tris[t as usize];
                k2.sort_unstable();
                if k2 == key {
                    return false;
                }
            }
        }
        // Near the envelope, then out of it.
        for (_, new) in &changed {
            let [a, b, cc] = new.map(|x| self.v[x as usize]);
            let centre = [
                (a[0] + b[0] + cc[0]) / 3.0,
                (a[1] + b[1] + cc[1]) / 3.0,
                (a[2] + b[2] + cc[2]) / 3.0,
            ];
            if self.field.value(centre, self.out_max + 1.0) > self.out_max {
                return false;
            }
        }
        for (_, new) in &changed {
            // Out by the balls of its corners (cheap, sufficient), else by
            // exact distances.
            let level = new
                .iter()
                .map(|&x| self.fv[x as usize])
                .fold(f64::INFINITY, f64::min);
            let corners = new.map(|x| self.v[x as usize]);
            let rho = enclosing_radius(corners);
            if level > 0.0 && rho * rho <= 2.0 * self.r_min * level + level * level {
                continue;
            }
            if !self.field.triangle_clear(corners) {
                self.refused_inside += 1;
                return false;
            }
        }
        // Made.
        for &t in &shared {
            self.alive_t[t as usize] = false;
            for &x in &self.tris[t as usize] {
                self.of[x as usize].retain(|&y| y != t);
            }
        }
        for (t, new) in &changed {
            self.tris[*t as usize] = *new;
            self.of[v as usize].push(*t);
        }
        self.of[u as usize].clear();
        self.alive_v[u as usize] = false;
        let qu = self.q[u as usize];
        for (x, y) in self.q[v as usize].iter_mut().zip(&qu) {
            *x += y;
        }
        self.qa[v as usize] += self.qa[u as usize];
        // Only v's quadric changed: its edges are pushed again; others
        // keep their place (their geometry is checked when taken).
        self.stamp[v as usize] += 1;
        true
    }
}

/// One part simplified: its triangles (local vertex ids) with the locked
/// vertices kept; returns the triangles left and the collapses refused
/// for entering the envelope.
#[allow(clippy::too_many_arguments)]
fn simplify_part(
    vertices: Vec<P3>,
    triangles: Vec<[u32; 3]>,
    locked: Vec<bool>,
    fv: Vec<f64>,
    field: &Field,
    tau: f64,
    out_max: f64,
    r_min: f64,
    budget: &Budget,
) -> (Vec<[u32; 3]>, u64) {
    let n = vertices.len();
    let mut s = State {
        field,
        v: vertices,
        q: vec![[0.0; 10]; n],
        qa: vec![0.0; n],
        locked,
        stamp: vec![0; n],
        alive_v: vec![true; n],
        alive_t: vec![true; triangles.len()],
        tris: triangles,
        of: vec![Vec::new(); n],
        tau2: tau * tau,
        out_max,
        refused_inside: 0,
        fv,
        r_min,
    };
    for k in 0..s.tris.len() {
        let t = s.tris[k];
        let [a, b, c] = t.map(|x| s.v[x as usize]);
        let (q, area) = plane_quadric(a, b, c);
        for &x in &t {
            s.of[x as usize].push(k as u32);
            for (y, z) in s.q[x as usize].iter_mut().zip(&q) {
                *y += z;
            }
            s.qa[x as usize] += area;
        }
    }
    let mut heap = BinaryHeap::new();
    for t in &s.tris {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            if a < b
                && let Some(c) = s.candidate(a, b)
            {
                heap.push(c);
            }
        }
    }
    let clock = budget.start();
    let mut steps = 0u64;
    while let Some(c) = heap.pop() {
        steps += 1;
        if steps.is_multiple_of(4096) && clock.check().is_err() {
            break;
        }
        if c.cost > s.tau2 {
            break;
        }
        if c.stamps != (s.stamp[c.from as usize], s.stamp[c.to as usize]) {
            continue;
        }
        if s.collapse(&c) {
            let v = c.to;
            for nb in s.neighbours(v) {
                if let Some(c) = s.candidate(v, nb) {
                    heap.push(c);
                }
            }
        }
    }
    let left = s
        .tris
        .iter()
        .zip(&s.alive_t)
        .filter(|(_, a)| **a)
        .map(|(t, _)| *t)
        .collect();
    (left, s.refused_inside)
}

/// The size of the parts simplified in parallel, m.
pub const PART: f64 = 24.0;

/// The mesh made coarser (see the module); the original when the result
/// fails its check. `tau`: the largest mean distance to the planes merged,
/// m; `out_max`: the largest f at a new triangle's centroid, m. In parts
/// of [`PART`] m simplified in parallel (their borders kept), twice, the
/// second time the parts shifted by half. Cost: about the number of
/// collapses times the checks of the triangles around them; exit clause:
/// `budget` (each part stops; the mesh so far is checked and used).
pub fn simplify(
    mesh: &Mesh,
    field: &Field,
    tau: f64,
    out_max: f64,
    budget: &Budget,
) -> (Mesh, SimplifyStats) {
    let r_min = field
        .prims()
        .iter()
        .map(|p| p.radius)
        .fold(f64::INFINITY, f64::min);
    let fv: Vec<f64> = mesh
        .vertices
        .iter()
        .map(|p| field.value(*p, f64::INFINITY))
        .collect();
    let mut tris = mesh.triangles.clone();
    let mut refused = 0u64;
    for pass in 0..2 {
        let shift = if pass == 0 { 0.0 } else { 0.5 * PART };
        // Each triangle's part, by its first corner.
        let part_of = |t: &[u32; 3]| {
            let p = mesh.vertices[t[0] as usize];
            [0, 1, 2].map(|k| ((p[k] + shift) / PART).floor() as i64)
        };
        let mut parts: std::collections::HashMap<[i64; 3], Vec<usize>> = Default::default();
        for (k, t) in tris.iter().enumerate() {
            parts.entry(part_of(t)).or_default().push(k);
        }
        // A vertex in triangles of two parts is locked.
        let mut owner: Vec<Option<[i64; 3]>> = vec![None; mesh.vertices.len()];
        let mut locked = vec![false; mesh.vertices.len()];
        for t in &tris {
            let p = part_of(t);
            for &x in t {
                match owner[x as usize] {
                    None => owner[x as usize] = Some(p),
                    Some(o) if o != p => locked[x as usize] = true,
                    _ => {}
                }
            }
        }
        let list: Vec<Vec<usize>> = parts.into_values().collect();
        let done = crate::positions::parallel(list.len(), budget, |k| {
            let mut local: std::collections::HashMap<u32, u32> = Default::default();
            let mut verts = Vec::new();
            let mut lock = Vec::new();
            let mut f = Vec::new();
            let mut global = Vec::new();
            let mut ts = Vec::with_capacity(list[k].len());
            for &i in &list[k] {
                ts.push(tris[i].map(|x| {
                    *local.entry(x).or_insert_with(|| {
                        verts.push(mesh.vertices[x as usize]);
                        lock.push(locked[x as usize]);
                        f.push(fv[x as usize]);
                        global.push(x);
                        (verts.len() - 1) as u32
                    })
                }));
            }
            let (left, r) = simplify_part(verts, ts, lock, f, field, tau, out_max, r_min, budget);
            Ok((
                left.into_iter()
                    .map(|t| t.map(|x| global[x as usize]))
                    .collect::<Vec<_>>(),
                r,
            ))
        });
        let Ok(done) = done else { break };
        tris = Vec::new();
        for (left, r) in done {
            tris.extend(left);
            refused += r;
        }
    }
    // Compacted.
    let mut index = vec![u32::MAX; mesh.vertices.len()];
    let mut out = Mesh::default();
    for t in &tris {
        let tri = t.map(|x| {
            if index[x as usize] == u32::MAX {
                index[x as usize] = out.vertices.len() as u32;
                out.vertices.push(mesh.vertices[x as usize]);
            }
            index[x as usize]
        });
        out.triangles.push(tri);
    }
    let mut stats = SimplifyStats {
        before: mesh.triangles.len(),
        after: out.triangles.len(),
        refused_inside: refused,
        checked: false,
    };
    if check(&out, field) {
        stats.checked = true;
        (out, stats)
    } else {
        stats.after = mesh.triangles.len();
        (mesh.clone(), stats)
    }
}

/// The whole mesh checked: closed, every triangle out of the envelope,
/// and a point of every prim inside it (in parallel).
pub fn check(mesh: &Mesh, field: &Field) -> bool {
    if mesh.open_edges() != 0 {
        return false;
    }
    const CHUNK: usize = 4096;
    let budget = Budget::new();
    let n = mesh.triangles.len().div_ceil(CHUNK);
    let clear = crate::positions::parallel(n, &budget, |k| {
        let end = ((k + 1) * CHUNK).min(mesh.triangles.len());
        Ok(mesh.triangles[k * CHUNK..end]
            .iter()
            .all(|t| field.triangle_clear(mesh.corners(t))))
    });
    if !clear.is_ok_and(|v| v.iter().all(|x| *x)) {
        return false;
    }
    let rays = Rays::new(mesh);
    let prims = field.prims();
    let n = prims.len().div_ceil(CHUNK);
    let inside = crate::positions::parallel(n, &budget, |k| {
        let end = ((k + 1) * CHUNK).min(prims.len());
        Ok(prims[k * CHUNK..end].iter().all(|p| {
            let at = match p.shape {
                PrimShape::Segment([a, b]) => mid(a, b),
                PrimShape::Triangle([a, b, c]) => [
                    (a[0] + b[0] + c[0]) / 3.0,
                    (a[1] + b[1] + c[1]) / 3.0,
                    (a[2] + b[2] + c[2]) / 3.0,
                ],
            };
            rays.inside(at)
        }))
    });
    inside.is_ok_and(|v| v.iter().all(|x| *x))
}

/// A mesh's triangles in a grid of columns along x (rays cast along x).
struct Rays<'a> {
    mesh: &'a Mesh,
    lo: [f64; 2],
    cell: f64,
    size: [usize; 2],
    cells: Vec<Vec<u32>>,
}

impl<'a> Rays<'a> {
    fn new(mesh: &'a Mesh) -> Self {
        let (lo, hi) = mesh.bounds().unwrap_or(([0.0; 3], [0.0; 3]));
        let area = ((hi[1] - lo[1]) * (hi[2] - lo[2])).max(1e-9);
        let cell = (area / (mesh.triangles.len().max(1) as f64))
            .sqrt()
            .max(1e-3)
            * 2.0;
        let size = [
            ((hi[1] - lo[1]) / cell) as usize + 1,
            ((hi[2] - lo[2]) / cell) as usize + 1,
        ];
        let mut cells = vec![Vec::new(); size[0] * size[1]];
        for (k, t) in mesh.triangles.iter().enumerate() {
            let c = mesh.corners(t);
            let (mut a, mut b) = ([usize::MAX; 2], [0usize; 2]);
            for p in &c {
                for (j, axis) in [1usize, 2].iter().enumerate() {
                    let i = (((p[*axis] - [lo[1], lo[2]][j]) / cell) as usize).min(size[j] - 1);
                    a[j] = a[j].min(i);
                    b[j] = b[j].max(i);
                }
            }
            for i in a[0]..=b[0] {
                for j in a[1]..=b[1] {
                    cells[i * size[1] + j].push(k as u32);
                }
            }
        }
        Self {
            mesh,
            lo: [lo[1], lo[2]],
            cell,
            size,
            cells,
        }
    }

    /// Whether `p` is inside: crossings of the ray towards +x counted,
    /// by three nearby rays (the majority: a ray grazing an edge is
    /// outvoted).
    fn inside(&self, p: P3) -> bool {
        let votes = [[0.0, 0.0], [1.3e-4, 0.7e-4], [-0.9e-4, 1.1e-4]]
            .iter()
            .filter(|o| self.parity([p[0], p[1] + o[0], p[2] + o[1]]))
            .count();
        votes >= 2
    }

    fn parity(&self, p: P3) -> bool {
        let (y, z) = (p[1], p[2]);
        if y < self.lo[0] || z < self.lo[1] {
            return false;
        }
        let (i, j) = (
            ((y - self.lo[0]) / self.cell) as usize,
            ((z - self.lo[1]) / self.cell) as usize,
        );
        if i >= self.size[0] || j >= self.size[1] {
            return false;
        }
        let mut inside = false;
        for &k in &self.cells[i * self.size[1] + j] {
            let [a, b, c] = self.mesh.corners(&self.mesh.triangles[k as usize]);
            // The triangle in (y, z): p inside it, and where it is in x.
            let (ay, az, by, bz, cy, cz) =
                (a[1] - y, a[2] - z, b[1] - y, b[2] - z, c[1] - y, c[2] - z);
            let w0 = by * cz - bz * cy;
            let w1 = cy * az - cz * ay;
            let w2 = ay * bz - az * by;
            let s = w0 + w1 + w2;
            if s == 0.0 {
                continue;
            }
            let all_pos = w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0;
            let all_neg = w0 <= 0.0 && w1 <= 0.0 && w2 <= 0.0;
            if !(all_pos || all_neg) {
                continue;
            }
            let x = (w0 * a[0] + w1 * b[0] + w2 * c[0]) / s;
            if x > p[0] {
                inside = !inside;
            }
        }
        inside
    }
}
