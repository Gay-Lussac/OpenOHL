// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The closed triangle mesh around a union of thick segments and thick
//! triangles: marching tetrahedra over a sparse cubic grid, every vertex
//! put just outside the envelope by bisection (theory: Safety volumes ›
//! The mesh).
//!
//! The envelope is \\( E = \{x : f(x) \le 0\} \\) with
//! \\( f(x) = \min_i (\operatorname{dist}(x, P_i) - r_i) \\), \\( P_i \\)
//! a segment or a triangle and \\( r_i \\) its radius. The surface is the
//! level \\( f = \varepsilon \\): a grid corner is inside when
//! \\( f \le \varepsilon \\), each cube is cut into six tetrahedra (the
//! same diagonal in every cube, so neighbours share their faces' cuts)
//! and each tetrahedron cut by the level gives one or two triangles whose
//! corners lie on its edges. Each corner is found by bisection on its
//! edge and kept on the outer side: \\( f \ge \varepsilon \\) there.
//!
//! **Why the mesh holds the whole envelope** (the cell `h` chosen by the
//! caller with \\( h \le \sqrt{2 r_{\min} \varepsilon + \varepsilon^2} \\)):
//! a triangle lies in one cube, so it lies in a circle of radius at most
//! \\( h \\) (its sides are at most the cube's diagonal
//! \\( h\sqrt3 \\)); its corners are at least \\( r_i + \varepsilon \\)
//! from every point \\( c \\) of \\( P_i \\), so each of its points is at
//! least \\( \sqrt{(r_i+\varepsilon)^2 - h^2} \ge r_i \\) from \\( c \\):
//! no triangle enters any ball \\( B(c, r_i) \\), i.e. the inside of
//! \\( E \\). The cube around a point of \\( P_i \\) has every corner
//! inside (\\( f \le -r_i + h\sqrt3 \le \varepsilon \\) when
//! \\( h\sqrt3 \le r_i + \varepsilon \\)), so every part of \\( E \\)
//! is on the inner side of the mesh.
//!
//! Sparse: a block of cubes is skipped when \\( f \\) at its centre is
//! further than its half diagonal from the level (\\( f \\) changes by at
//! most the distance moved), so the work follows the surface, not the
//! volume. Blocks are meshed in parallel; the vertices on their shared
//! faces are the same (each computed from the same edge) and merged by
//! edge.

use std::collections::HashMap;

use ohl_core::budget::Budget;

use crate::geometry::{P3, closest_on_segment, closest_on_triangle, cross, dot, sub};

fn norm(a: P3) -> f64 {
    dot(a, a).sqrt()
}

/// A thick segment or a thick triangle: every point within `radius` of
/// it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Prim {
    /// The segment (its two ends) or the triangle.
    pub shape: PrimShape,
    /// Radius, m.
    pub radius: f64,
}

/// The core of a [`Prim`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PrimShape {
    /// A segment.
    Segment([P3; 2]),
    /// A triangle.
    Triangle([P3; 3]),
}

impl Prim {
    /// A thick segment.
    pub fn segment(a: P3, b: P3, radius: f64) -> Self {
        Self {
            shape: PrimShape::Segment([a, b]),
            radius,
        }
    }

    /// A thick triangle (a degenerate one: its longest side).
    pub fn triangle(a: P3, b: P3, c: P3, radius: f64) -> Self {
        let n = cross(sub(b, a), sub(c, a));
        let longest = [(a, b), (b, c), (c, a)]
            .into_iter()
            .max_by(|x, y| norm(sub(x.1, x.0)).total_cmp(&norm(sub(y.1, y.0))))
            .unwrap_or((a, b));
        let scale = norm(sub(longest.1, longest.0));
        if norm(n) <= 1e-9 * scale * scale.max(1e-12) {
            return Self::segment(longest.0, longest.1, radius);
        }
        Self {
            shape: PrimShape::Triangle([a, b, c]),
            radius,
        }
    }

    /// Distance from `p` to the core.
    pub fn core_distance(&self, p: P3) -> f64 {
        match self.shape {
            PrimShape::Segment([a, b]) => norm(sub(p, closest_on_segment(p, a, b).0)),
            PrimShape::Triangle([a, b, c]) => norm(sub(p, closest_on_triangle(p, a, b, c))),
        }
    }

    /// Distance from `p` to its surface (negative inside).
    pub fn value(&self, p: P3) -> f64 {
        self.core_distance(p) - self.radius
    }

    /// The core's bounding box.
    fn bounds(&self) -> (P3, P3) {
        let pts: &[P3] = match &self.shape {
            PrimShape::Segment(s) => s,
            PrimShape::Triangle(t) => t,
        };
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for p in pts {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        (lo, hi)
    }

    /// Its points moved by `d`.
    pub fn moved(&self, d: P3) -> Self {
        let m = |p: P3| [p[0] + d[0], p[1] + d[1], p[2] + d[2]];
        Self {
            shape: match self.shape {
                PrimShape::Segment([a, b]) => PrimShape::Segment([m(a), m(b)]),
                PrimShape::Triangle([a, b, c]) => PrimShape::Triangle([m(a), m(b), m(c)]),
            },
            radius: self.radius,
        }
    }
}

/// A node of the bounding volume tree.
#[derive(Debug, Clone)]
struct Node {
    lo: P3,
    hi: P3,
    /// The largest radius below it.
    radius: f64,
    /// Leaf: its prims `first..first + count`; else `count` = 0 and the
    /// children are `first`, `first + 1`.
    first: u32,
    count: u32,
}

/// The prims of an envelope in a bounding volume tree: \\( f(x) \\) fast.
#[derive(Debug, Clone)]
pub struct Field {
    prims: Vec<Prim>,
    nodes: Vec<Node>,
}

/// Lower bound of the distance from `p` to the box.
fn box_distance(p: P3, lo: P3, hi: P3) -> f64 {
    let mut s = 0.0;
    for k in 0..3 {
        let d = (lo[k] - p[k]).max(p[k] - hi[k]).max(0.0);
        s += d * d;
    }
    s.sqrt()
}

/// Distance between two boxes (0 when they overlap).
fn box_box_distance(alo: P3, ahi: P3, blo: P3, bhi: P3) -> f64 {
    let mut s = 0.0;
    for k in 0..3 {
        let d = (blo[k] - ahi[k]).max(alo[k] - bhi[k]).max(0.0);
        s += d * d;
    }
    s.sqrt()
}

/// Where the segment p–q crosses the triangle, if it does.
fn segment_crosses(p: P3, q: P3, t: [P3; 3]) -> bool {
    let d = sub(q, p);
    let (e1, e2) = (sub(t[1], t[0]), sub(t[2], t[0]));
    let h = cross(d, e2);
    let det = dot(e1, h);
    if det.abs() < 1e-14 * norm(e1) * norm(e2) * norm(d).max(1e-12) {
        return false;
    }
    let s = sub(p, t[0]);
    let u = dot(s, h) / det;
    if !(0.0..=1.0).contains(&u) {
        return false;
    }
    let r = cross(s, e1);
    let v = dot(d, r) / det;
    if v < 0.0 || u + v > 1.0 {
        return false;
    }
    let w = dot(e2, r) / det;
    (0.0..=1.0).contains(&w)
}

/// Exact distance between the segment p–q and the triangle `t`: 0 when
/// they cross, else the nearest of the ends to the triangle and of the
/// segment to the triangle's sides.
pub fn segment_triangle_distance(p: P3, q: P3, t: [P3; 3]) -> f64 {
    if segment_crosses(p, q, t) {
        return 0.0;
    }
    let mut best = norm(sub(p, closest_on_triangle(p, t[0], t[1], t[2])))
        .min(norm(sub(q, closest_on_triangle(q, t[0], t[1], t[2]))));
    for k in 0..3 {
        let (a, b) = crate::geometry::closest_between_segments(p, q, t[k], t[(k + 1) % 3]);
        best = best.min(norm(sub(a, b)));
    }
    best
}

/// Exact distance between two triangles: the nearest of each one's sides
/// to the other.
pub fn triangle_triangle_distance(a: [P3; 3], b: [P3; 3]) -> f64 {
    let mut best = f64::INFINITY;
    for k in 0..3 {
        best = best.min(segment_triangle_distance(a[k], a[(k + 1) % 3], b));
        best = best.min(segment_triangle_distance(b[k], b[(k + 1) % 3], a));
    }
    best
}

impl Field {
    /// The tree of `prims`.
    pub fn new(mut prims: Vec<Prim>) -> Self {
        let mut nodes = Vec::new();
        if !prims.is_empty() {
            nodes.push(Node {
                lo: [0.0; 3],
                hi: [0.0; 3],
                radius: 0.0,
                first: 0,
                count: 0,
            });
            let n = prims.len();
            Self::build(&mut prims, &mut nodes, 0, 0, n);
        }
        Self { prims, nodes }
    }

    fn build(prims: &mut [Prim], nodes: &mut Vec<Node>, at: usize, from: usize, to: usize) {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        let mut radius = 0.0f64;
        let mut clo = [f64::INFINITY; 3];
        let mut chi = [f64::NEG_INFINITY; 3];
        for p in &prims[from..to] {
            let (a, b) = p.bounds();
            for k in 0..3 {
                lo[k] = lo[k].min(a[k]);
                hi[k] = hi[k].max(b[k]);
                let c = 0.5 * (a[k] + b[k]);
                clo[k] = clo[k].min(c);
                chi[k] = chi[k].max(c);
            }
            radius = radius.max(p.radius);
        }
        nodes[at].lo = lo;
        nodes[at].hi = hi;
        nodes[at].radius = radius;
        if to - from <= 4 {
            nodes[at].first = from as u32;
            nodes[at].count = (to - from) as u32;
            return;
        }
        let axis = (0..3)
            .max_by(|&a, &b| (chi[a] - clo[a]).total_cmp(&(chi[b] - clo[b])))
            .unwrap_or(0);
        let mid = (from + to) / 2;
        let centre = |p: &Prim| {
            let (a, b) = p.bounds();
            a[axis] + b[axis]
        };
        prims[from..to].select_nth_unstable_by(mid - from, |x, y| centre(x).total_cmp(&centre(y)));
        let left = nodes.len();
        for _ in 0..2 {
            nodes.push(Node {
                lo: [0.0; 3],
                hi: [0.0; 3],
                radius: 0.0,
                first: 0,
                count: 0,
            });
        }
        nodes[at].first = left as u32;
        nodes[at].count = 0;
        Self::build(prims, nodes, left, from, mid);
        Self::build(prims, nodes, left + 1, mid, to);
    }

    /// The prims.
    pub fn prims(&self) -> &[Prim] {
        &self.prims
    }

    /// \\( \min(f(p), \text{cap}) \\): the distance to the envelope's
    /// surface (negative inside), or `cap` when it is at least that.
    pub fn value(&self, p: P3, cap: f64) -> f64 {
        let mut best = cap;
        if self.nodes.is_empty() {
            return best;
        }
        let mut stack: Vec<u32> = Vec::with_capacity(64);
        stack.push(0);
        while let Some(k) = stack.pop() {
            let n = &self.nodes[k as usize];
            if box_distance(p, n.lo, n.hi) - n.radius >= best {
                continue;
            }
            if n.count > 0 {
                for q in &self.prims[n.first as usize..(n.first + n.count) as usize] {
                    best = best.min(q.value(p));
                }
            } else {
                let (a, b) = (n.first, n.first + 1);
                // The nearer child first.
                let da = {
                    let c = &self.nodes[a as usize];
                    box_distance(p, c.lo, c.hi) - c.radius
                };
                let db = {
                    let c = &self.nodes[b as usize];
                    box_distance(p, c.lo, c.hi) - c.radius
                };
                if da <= db {
                    stack.push(b);
                    stack.push(a);
                } else {
                    stack.push(a);
                    stack.push(b);
                }
            }
        }
        best
    }

    /// Whether the triangle `t` stays out of the envelope's inside: at
    /// least its radius from every prim's core (exact distances).
    pub fn triangle_clear(&self, t: [P3; 3]) -> bool {
        if self.nodes.is_empty() {
            return true;
        }
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for p in &t {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        let mut stack: Vec<u32> = Vec::with_capacity(64);
        stack.push(0);
        while let Some(k) = stack.pop() {
            let n = &self.nodes[k as usize];
            if box_box_distance(lo, hi, n.lo, n.hi) >= n.radius {
                continue;
            }
            if n.count > 0 {
                for q in &self.prims[n.first as usize..(n.first + n.count) as usize] {
                    let (a, b) = q.bounds();
                    if box_box_distance(lo, hi, a, b) >= q.radius {
                        continue;
                    }
                    let d = match q.shape {
                        PrimShape::Segment([p0, p1]) => segment_triangle_distance(p0, p1, t),
                        PrimShape::Triangle(u) => triangle_triangle_distance(u, t),
                    };
                    if d < q.radius {
                        return false;
                    }
                }
            } else {
                stack.push(n.first);
                stack.push(n.first + 1);
            }
        }
        true
    }

    /// The bounding box of the envelope (the cores grown by their radii).
    pub fn bounds(&self) -> Option<(P3, P3)> {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for p in &self.prims {
            let (a, b) = p.bounds();
            for k in 0..3 {
                lo[k] = lo[k].min(a[k] - p.radius);
                hi[k] = hi[k].max(b[k] + p.radius);
            }
        }
        lo[0].is_finite().then_some((lo, hi))
    }
}

/// A closed triangle mesh.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mesh {
    /// Vertices.
    pub vertices: Vec<P3>,
    /// Triangles (vertex indices), counter-clockwise seen from outside.
    pub triangles: Vec<[u32; 3]>,
}

impl Mesh {
    /// Its corners.
    pub fn corners(&self, t: &[u32; 3]) -> [P3; 3] {
        t.map(|i| self.vertices[i as usize])
    }

    /// The volume enclosed, m³ (the divergence theorem, about its first
    /// vertex).
    pub fn volume(&self) -> f64 {
        let Some(&o) = self.vertices.first() else {
            return 0.0;
        };
        self.triangles
            .iter()
            .map(|t| {
                let [a, b, c] = self.corners(t);
                dot(sub(a, o), cross(sub(b, o), sub(c, o))) / 6.0
            })
            .sum()
    }

    /// Its surface area, m².
    pub fn area(&self) -> f64 {
        self.triangles
            .iter()
            .map(|t| {
                let [a, b, c] = self.corners(t);
                0.5 * norm(cross(sub(b, a), sub(c, a)))
            })
            .sum()
    }

    /// Edges not matched by the same edge the other way round in exactly
    /// one other triangle (0: closed and consistently turned).
    pub fn open_edges(&self) -> usize {
        let mut count: HashMap<(u32, u32), i32> = HashMap::new();
        for t in &self.triangles {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                if a < b {
                    *count.entry((a, b)).or_default() += 1;
                } else {
                    *count.entry((b, a)).or_default() -= 1;
                }
            }
        }
        // Each undirected edge: once each way.
        let mut uses: HashMap<(u32, u32), u32> = HashMap::new();
        for t in &self.triangles {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                *uses.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        count.values().filter(|&&c| c != 0).count() + uses.values().filter(|&&u| u != 2).count()
    }

    /// Lowest and highest corner.
    pub fn bounds(&self) -> Option<(P3, P3)> {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for v in &self.vertices {
            for k in 0..3 {
                lo[k] = lo[k].min(v[k]);
                hi[k] = hi[k].max(v[k]);
            }
        }
        lo[0].is_finite().then_some((lo, hi))
    }

    /// Whether `p` is inside (a ray's crossings counted; tests and
    /// checks, linear in the triangles).
    pub fn contains(&self, p: P3) -> bool {
        // A direction unlikely to graze an edge.
        let d = [0.8660254037844386, 0.4226182617406994, 0.2672612419124244];
        let mut inside = false;
        for t in &self.triangles {
            let [a, b, c] = self.corners(t);
            // Möller–Trumbore.
            let (e1, e2) = (sub(b, a), sub(c, a));
            let h = cross(d, e2);
            let det = dot(e1, h);
            if det.abs() < 1e-15 {
                continue;
            }
            let s = sub(p, a);
            let u = dot(s, h) / det;
            if !(0.0..=1.0).contains(&u) {
                continue;
            }
            let q = cross(s, e1);
            let v = dot(d, q) / det;
            if v < 0.0 || u + v > 1.0 {
                continue;
            }
            if dot(e2, q) / det > 0.0 {
                inside = !inside;
            }
        }
        inside
    }

    /// The outline seen from above: every edge between a triangle facing
    /// up and one facing down (in plan, the footprint's outline and the
    /// folds of the surface).
    pub fn silhouette(&self) -> Vec<[P3; 2]> {
        let mut faces: HashMap<(u32, u32), (bool, u8)> = HashMap::new();
        let mut out = Vec::new();
        for t in &self.triangles {
            let [a, b, c] = self.corners(t);
            let up = cross(sub(b, a), sub(c, a))[2] >= 0.0;
            for k in 0..3 {
                let (x, y) = (t[k], t[(k + 1) % 3]);
                let key = (x.min(y), x.max(y));
                match faces.get(&key) {
                    Some(&(other, _)) if other != up => {
                        out.push([self.vertices[x as usize], self.vertices[y as usize]]);
                    }
                    _ => {}
                }
                faces.entry(key).or_insert((up, 0));
            }
        }
        out
    }
}

/// What the mesher did.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MeshStats {
    /// Grid cell, m.
    pub cell: f64,
    /// Level ε of the surface, m.
    pub level: f64,
    /// Values of f computed.
    pub evaluations: u64,
    /// Cubes the surface passes through.
    pub cells: u64,
    /// The smallest f at a vertex less ε (≥ 0 by construction), m.
    pub vertex_min: f64,
    /// The largest f at a vertex less ε, m.
    pub vertex_max: f64,
}

/// Cube corners by bit (x = 1, y = 2, z = 4).
fn corner_offset(bit: usize) -> [i64; 3] {
    [
        (bit & 1) as i64,
        ((bit >> 1) & 1) as i64,
        ((bit >> 2) & 1) as i64,
    ]
}

/// The six tetrahedra of a cube around its diagonal 0–7 (Kuhn): the same
/// cut of each face in every cube.
const TETS: [[usize; 4]; 6] = [
    [0, 1, 3, 7],
    [0, 1, 5, 7],
    [0, 2, 3, 7],
    [0, 2, 6, 7],
    [0, 4, 5, 7],
    [0, 4, 6, 7],
];

/// Grid corner key.
fn key(c: [i64; 3]) -> u64 {
    ((c[0] as u64) << 42) | ((c[1] as u64) << 21) | (c[2] as u64)
}

/// Edge key: the two corners' keys, the smaller first.
fn edge_key(a: [i64; 3], b: [i64; 3]) -> u128 {
    let (ka, kb) = (key(a), key(b));
    let (lo, hi) = if ka < kb { (ka, kb) } else { (kb, ka) };
    (u128::from(lo) << 64) | u128::from(hi)
}

/// The grid: origin, cell, size in cells.
#[derive(Debug, Clone, Copy)]
struct Grid {
    origin: P3,
    cell: f64,
    size: [i64; 3],
}

impl Grid {
    fn at(&self, c: [i64; 3]) -> P3 {
        [
            self.origin[0] + c[0] as f64 * self.cell,
            self.origin[1] + c[1] as f64 * self.cell,
            self.origin[2] + c[2] as f64 * self.cell,
        ]
    }
}

/// A block's part of the mesh: vertices by edge, triangles.
#[derive(Default)]
struct Part {
    keys: Vec<u128>,
    vertices: Vec<P3>,
    triangles: Vec<[u32; 3]>,
    evaluations: u64,
    cells: u64,
}

struct BlockMesher<'a> {
    field: &'a Field,
    grid: Grid,
    level: f64,
    corners: HashMap<u64, f64>,
    edges: HashMap<u128, u32>,
    part: Part,
}

impl BlockMesher<'_> {
    fn corner(&mut self, c: [i64; 3]) -> f64 {
        let k = key(c);
        if let Some(&v) = self.corners.get(&k) {
            return v;
        }
        // Exactly: whether a corner is inside decides the triangles.
        let v = self.field.value(self.grid.at(c), f64::INFINITY);
        self.part.evaluations += 1;
        self.corners.insert(k, v);
        v
    }

    /// The vertex on the edge from corner `a` (inside) to `b` (outside):
    /// bisection, kept on the outer side.
    fn vertex(&mut self, a: [i64; 3], fa: f64, b: [i64; 3], fb: f64) -> u32 {
        let k = edge_key(a, b);
        if let Some(&i) = self.edges.get(&k) {
            return i;
        }
        // Always searched from the inside corner (whether a corner is
        // inside is the same in every block): the same point from every
        // block.
        let (pa, pb) = (self.grid.at(a), self.grid.at(b));
        let at = |t: f64| {
            [
                pa[0] + t * (pb[0] - pa[0]),
                pa[1] + t * (pb[1] - pa[1]),
                pa[2] + t * (pb[2] - pa[2]),
            ]
        };
        let (mut t0, mut f0, mut t1, mut f1) = (0.0f64, fa, 1.0f64, fb);
        let len = self.grid.cell;
        for i in 0..48 {
            if (t1 - t0) * len < 1e-5 || f1 - self.level < 1e-4 {
                break;
            }
            // Secant, every third step halved (a bracket always shrinks).
            let mut t = if i % 3 == 2 || f1 <= f0 {
                0.5 * (t0 + t1)
            } else {
                t0 + (self.level - f0) / (f1 - f0) * (t1 - t0)
            };
            let span = t1 - t0;
            t = t.clamp(t0 + 0.02 * span, t1 - 0.02 * span);
            let f = self.field.value(at(t), f64::INFINITY);
            self.part.evaluations += 1;
            if f > self.level {
                (t1, f1) = (t, f);
            } else {
                (t0, f0) = (t, f);
            }
        }
        let i = self.part.vertices.len() as u32;
        self.part.vertices.push(at(t1));
        self.part.keys.push(k);
        self.edges.insert(k, i);
        i
    }

    fn cell(&mut self, c: [i64; 3]) {
        let mut pos = [[0i64; 3]; 8];
        let mut val = [0.0f64; 8];
        let mut ins = 0u8;
        for (b, (p, v)) in pos.iter_mut().zip(val.iter_mut()).enumerate() {
            let o = corner_offset(b);
            *p = [c[0] + o[0], c[1] + o[1], c[2] + o[2]];
            *v = self.corner(*p);
            if *v <= self.level {
                ins |= 1 << b;
            }
        }
        if ins == 0 || ins == 0xff {
            return;
        }
        self.part.cells += 1;
        for tet in TETS {
            let inside: Vec<usize> = tet
                .iter()
                .copied()
                .filter(|&b| ins & (1 << b) != 0)
                .collect();
            let outside: Vec<usize> = tet
                .iter()
                .copied()
                .filter(|&b| ins & (1 << b) == 0)
                .collect();
            let edge = |i: usize, o: usize, m: &mut Self| -> (u32, [i64; 3]) {
                // The vertex, and twice the edge's midpoint (exact, for
                // the orientation).
                let v = m.vertex(pos[i], val[i], pos[o], val[o]);
                (v, [0, 1, 2].map(|k| pos[i][k] + pos[o][k]))
            };
            // Twice the centroids' difference, out − in (scaled by both
            // counts so it stays whole).
            let toward = {
                let (ni, no) = (inside.len() as i64, outside.len() as i64);
                let mut d = [0i64; 3];
                for k in 0..3 {
                    let si: i64 = inside.iter().map(|&b| pos[b][k]).sum();
                    let so: i64 = outside.iter().map(|&b| pos[b][k]).sum();
                    d[k] = 2 * (so * ni - si * no);
                }
                d
            };
            match (inside.len(), outside.len()) {
                (1, 3) => {
                    let i = inside[0];
                    let a = edge(i, outside[0], self);
                    let b = edge(i, outside[1], self);
                    let c = edge(i, outside[2], self);
                    self.emit(a, b, c, toward);
                }
                (3, 1) => {
                    let o = outside[0];
                    let a = edge(inside[0], o, self);
                    let b = edge(inside[1], o, self);
                    let c = edge(inside[2], o, self);
                    self.emit(a, b, c, toward);
                }
                (2, 2) => {
                    let (i1, i2, o1, o2) = (inside[0], inside[1], outside[0], outside[1]);
                    let q0 = edge(i1, o1, self);
                    let q1 = edge(i1, o2, self);
                    let q2 = edge(i2, o2, self);
                    let q3 = edge(i2, o1, self);
                    self.emit(q0, q1, q2, toward);
                    self.emit(q0, q2, q3, toward);
                }
                _ => {}
            }
        }
    }

    /// A triangle turned to face `toward` (outward), by its midpoints
    /// (exact integers).
    fn emit(
        &mut self,
        a: (u32, [i64; 3]),
        b: (u32, [i64; 3]),
        c: (u32, [i64; 3]),
        toward: [i64; 3],
    ) {
        if a.0 == b.0 || b.0 == c.0 || a.0 == c.0 {
            return;
        }
        let u = [0, 1, 2].map(|k| b.1[k] - a.1[k]);
        let v = [0, 1, 2].map(|k| c.1[k] - a.1[k]);
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let s = n[0] * toward[0] + n[1] * toward[1] + n[2] * toward[2];
        if s >= 0 {
            self.part.triangles.push([a.0, b.0, c.0]);
        } else {
            self.part.triangles.push([a.0, c.0, b.0]);
        }
    }

    /// The cubes of the block at `c` of `size` cells (a power of 2) the
    /// surface may pass through.
    fn block(&mut self, c: [i64; 3], size: i64) {
        if (0..3).any(|k| c[k] >= self.grid.size[k]) {
            return;
        }
        if size == 1 {
            self.cell(c);
            return;
        }
        if prune(
            self.field,
            &self.grid,
            self.level,
            c,
            size,
            &mut self.part.evaluations,
        ) {
            return;
        }
        let h = size / 2;
        for b in 0..8 {
            let o = corner_offset(b);
            self.block([c[0] + o[0] * h, c[1] + o[1] * h, c[2] + o[2] * h], h);
        }
    }
}

/// Whether the block at `c` of `size` cells lies wholly on one side of
/// the level (f within its half diagonal of the value at its centre).
fn prune(field: &Field, grid: &Grid, level: f64, c: [i64; 3], size: i64, evals: &mut u64) -> bool {
    let half = size as f64 * 0.5;
    let centre = [
        grid.origin[0] + (c[0] as f64 + half) * grid.cell,
        grid.origin[1] + (c[1] as f64 + half) * grid.cell,
        grid.origin[2] + (c[2] as f64 + half) * grid.cell,
    ];
    let reach = half * grid.cell * 3f64.sqrt() * (1.0 + 1e-9);
    *evals += 1;
    let v = field.value(centre, level + reach + 1e-9);
    v > level + reach || v + reach < level
}

/// The mesh of the level ε = `level` of `field` on a grid of cubes of
/// `cell` (see the module for the choice that makes it hold the whole
/// envelope). Cost: proportional to the area of the surface over
/// `cell`²; exit clause: `budget` (checked between blocks).
pub fn mesh(
    field: &Field,
    level: f64,
    cell: f64,
    budget: &Budget,
) -> Result<(Mesh, MeshStats), String> {
    let Some((lo, hi)) = field.bounds() else {
        return Ok((Mesh::default(), MeshStats::default()));
    };
    if !(cell > 0.0 && cell.is_finite() && level >= 0.0) {
        return Err(format!("mesh: cell {cell} m, level {level} m"));
    }
    // Two cells and the level beyond the envelope: every corner of the
    // grid's border is outside.
    let pad = level + 2.0 * cell;
    let origin = [lo[0] - pad, lo[1] - pad, lo[2] - pad];
    let size = [0, 1, 2].map(|k| (((hi[k] + pad) - origin[k]) / cell).ceil() as i64 + 1);
    if size.iter().any(|&n| n >= (1 << 21)) {
        return Err(format!(
            "the volume spans {:.0} × {:.0} × {:.0} m: too large for cells of {cell:.3} m",
            hi[0] - lo[0],
            hi[1] - lo[1],
            hi[2] - lo[2]
        ));
    }
    let grid = Grid { origin, cell, size };
    // The blocks meshed in parallel: 16 cells a side, found from the root.
    const BLOCK: i64 = 16;
    let mut root = BLOCK;
    while (0..3).any(|k| root < size[k]) {
        root *= 2;
    }
    let mut blocks = Vec::new();
    let mut evals = 0u64;
    let mut stack = vec![([0i64; 3], root)];
    let clock = budget.start();
    while let Some((c, s)) = stack.pop() {
        clock.check().map_err(|e| e.to_string())?;
        if (0..3).any(|k| c[k] >= size[k]) {
            continue;
        }
        if prune(field, &grid, level, c, s, &mut evals) {
            continue;
        }
        if s == BLOCK {
            blocks.push(c);
            continue;
        }
        let h = s / 2;
        for b in 0..8 {
            let o = corner_offset(b);
            stack.push(([c[0] + o[0] * h, c[1] + o[1] * h, c[2] + o[2] * h], h));
        }
    }
    blocks.sort_unstable();
    let parts = crate::positions::parallel(blocks.len(), budget, |k| {
        let mut m = BlockMesher {
            field,
            grid,
            level,
            corners: HashMap::new(),
            edges: HashMap::new(),
            part: Part::default(),
        };
        m.block(blocks[k], BLOCK);
        Ok(m.part)
    })?;
    // Merged by edge.
    let mut index: HashMap<u128, u32> = HashMap::new();
    let mut out = Mesh::default();
    let mut stats = MeshStats {
        cell,
        level,
        evaluations: evals,
        vertex_min: f64::INFINITY,
        vertex_max: f64::NEG_INFINITY,
        ..MeshStats::default()
    };
    for part in parts {
        stats.evaluations += part.evaluations;
        stats.cells += part.cells;
        let map: Vec<u32> = part
            .keys
            .iter()
            .zip(&part.vertices)
            .map(|(k, v)| {
                *index.entry(*k).or_insert_with(|| {
                    out.vertices.push(*v);
                    (out.vertices.len() - 1) as u32
                })
            })
            .collect();
        out.triangles
            .extend(part.triangles.iter().map(|t| t.map(|i| map[i as usize])));
    }
    for v in &out.vertices {
        let f = field.value(*v, f64::INFINITY) - level;
        stats.vertex_min = stats.vertex_min.min(f);
        stats.vertex_max = stats.vertex_max.max(f);
    }
    if out.vertices.is_empty() {
        stats.vertex_min = 0.0;
        stats.vertex_max = 0.0;
    }
    Ok((out, stats))
}
