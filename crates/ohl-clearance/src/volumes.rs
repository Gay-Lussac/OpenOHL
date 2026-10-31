// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **The volumes on the strings** placed on the solved strings and
//! measured (roadmap O6, second part; owner 2026-10-19, built
//! 2026-10-25): the shapes of an insulator assembly
//! ([`ohl_structures::StringVolumes`]) as primitives in project
//! coordinates ([`Convex`]: sphere, cylinder or cone, box — convex — and
//! the torus, the one shape that is not; and the points, segments and
//! triangles they are measured against), the distance between two of them
//! ([`separation`]: GJK) and a mesh of each for the views
//! ([`Convex::mesh`]).
//!
//! **Placing a string's shapes** ([`place`]), from its solved top and
//! wire point (the axis, top to wire point), in the string's own axes —
//! *axis* (top to wire point), *across* and *along*:
//!
//! - A **suspension string**: the shapes' axis takes the solved string's
//!   swing across (its angle from straight down in the structure's x–z
//!   plane) and along (y–z plane) where that freedom is **free**; where it
//!   is **blocked** it keeps the angle the string has in still air (as
//!   placed in the line). *Across* is the structure's x axis (right of the
//!   line) where the **twist** is blocked, else the horizontal across the
//!   conductor at the wire point (as solved: towards its neighbouring
//!   clamps); made square to the axis; *along* = across × axis (ahead for a
//!   string hanging straight down). A string pointing less than about 3°
//!   below level is not swung by its freedoms (its axis as solved).
//! - A **strain string**: its axis is the conductor's pull, as solved
//!   (the freedoms are not used); *across* is level, square to the axis,
//!   to the structure's right; *along* is square to both, upwards.
//! - Each shape's centre is on the **solved** axis at its distance from
//!   the top (a fraction of the solved length, or metres), moved by its
//!   offsets along *across* and *along*: the freedoms set the shapes'
//!   orientation, not where the string is. Its own axis (cylinder, cone,
//!   box length, a torus's axis: the ring square to the string) lies
//!   along the axis as the freedoms give it.
//! - **Several strings in one assembly**: the line models them as one
//!   string, on their middle; each string's copy of a shape is moved by
//!   that string's offset (`strings`, `arrangement`, `spacing`) in
//!   *across* and *along*, parallel to the modelled string; a **shared**
//!   shape (a yoke plate) is placed once, on the modelled axis. The
//!   strings' own tops and their yokes' geometry are not modelled
//!   (approximate: each copy parallel to the modelled string).
//!
//! **Distances** ([`separation`]): the Gilbert–Johnson–Keerthi algorithm
//! on the two convex shapes' support functions — exact for points,
//! segments, triangles and boxes (polytopes: it ends in finitely many
//! steps), converging on the curved ones (sphere: exact, as its centre
//! less its radius; cylinder, cone: iterated). It returns a **lower
//! bound** of the distance within 1 nm of it (conservative: never more
//! than the exact distance), 0 when they touch or overlap (a sphere's
//! radius beyond that: negative).
//!
//! **A torus** (a grading ring; owner 2026-10-31, "the builder window with
//! a torus") is not convex: it is its centre circle (radius *R*, the
//! major radius) grown by its tube's radius *r*. The distance from a point
//! or a sphere to it is exact, in closed form (from the point:
//! √((ρ − R)² + h²) − r, ρ its distance from the ring's axis, h its height
//! over the ring's plane). Against anything else (a segment, a triangle, a box,
//! a cylinder or cone, another torus) the circle is taken as the polygon
//! of `n` chords inscribed in it ([`torus_chords`]), each chord a convex
//! segment measured by GJK and grown by r + s, where s = R (1 − cos(π/n))
//! is the chords' **sagitta** (the farthest a point of the circle lies from
//! its chord). Every point of the torus lies within r of the circle, so
//! within r + s of a chord: the distance found is **never more** than the
//! exact one (**conservative**), and at most 2 s less. `n` is chosen so
//! that s ≤ 0.25 mm ([`TORUS_SAGITTA`]): the distance is within 0.5 mm
//! below the exact one (at least 24 chords); between two tori, both by
//! their chords, within 1 mm.

use crate::geometry::{P3, cross, dot, sub};

fn add(a: P3, b: P3) -> P3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: P3, k: f64) -> P3 {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn norm(a: P3) -> f64 {
    dot(a, a).sqrt()
}

fn unit(a: P3) -> Option<P3> {
    let n = norm(a);
    (n > 1e-12).then(|| scale(a, 1.0 / n))
}

/// A convex shape in project coordinates, m.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Convex {
    /// A point.
    Point(P3),
    /// A segment.
    Segment(P3, P3),
    /// A triangle.
    Triangle([P3; 3]),
    /// A sphere.
    Sphere {
        /// Its centre.
        centre: P3,
        /// Its radius.
        radius: f64,
    },
    /// A cylinder (equal radii) or a cone, truncated or not, from `a` to
    /// `b` on its axis.
    Frustum {
        /// The centre of its first end.
        a: P3,
        /// The centre of its other end.
        b: P3,
        /// The radius at `a`.
        ra: f64,
        /// The radius at `b`.
        rb: f64,
    },
    /// A box.
    Cuboid {
        /// Its centre.
        centre: P3,
        /// Its three axes (unit, square to each other).
        axes: [P3; 3],
        /// Its half sizes along them.
        half: [f64; 3],
    },
    /// A torus — not convex (the one exception): a circle of radius
    /// `major` about `axis` through `centre`, grown by `minor` (its tube's
    /// radius), measured as the module says.
    Torus {
        /// The centre of its ring (in its hole).
        centre: P3,
        /// Its axis, square to the ring's plane (unit).
        axis: P3,
        /// The radius of its centre circle, m.
        major: f64,
        /// The radius of its tube, m.
        minor: f64,
    },
}

/// The largest sagitta of the chords a torus's circle is measured by, m
/// (the distance found is at most twice it below the exact one).
pub const TORUS_SAGITTA: f64 = 0.000_25;

/// The fewest chords of a torus's circle.
const TORUS_MIN_CHORDS: usize = 24;

/// The most chords of a torus's circle (a ring of up to 50 m keeps its
/// sagitta below 0.25 mm with them).
const TORUS_MAX_CHORDS: usize = 2048;

/// The chords a torus of centre circle `major` is measured by: their
/// count n, chosen so that the sagitta R (1 − cos(π/n)) ≤
/// [`TORUS_SAGITTA`], and that sagitta, m.
pub fn torus_chords(major: f64) -> (usize, f64) {
    let sagitta = |n: usize| major * (1.0 - (std::f64::consts::PI / n as f64).cos());
    let mut n = TORUS_MIN_CHORDS;
    if major > TORUS_SAGITTA {
        let angle = (1.0 - TORUS_SAGITTA / major).acos();
        n = n.max((std::f64::consts::PI / angle).ceil() as usize);
    }
    let n = n.min(TORUS_MAX_CHORDS);
    (n, sagitta(n).max(0.0))
}

/// The points of a circle (centre `c`, unit axis `e`, radius `r`) at `n`
/// equal steps, the first again at the end.
fn circle_points(c: P3, e: P3, r: f64, n: usize) -> Vec<P3> {
    let (u, v) = square_to(e);
    (0..=n)
        .map(|j| {
            let ph = std::f64::consts::TAU * (j % n) as f64 / n as f64;
            add(c, add(scale(u, r * ph.cos()), scale(v, r * ph.sin())))
        })
        .collect()
}

/// The exact distance from point `p` to the surface of a torus (centre
/// `c`, unit axis `e`, radii `major`, `minor`; negative inside its tube)
/// and its nearest point on the centre circle.
fn point_torus(p: P3, c: P3, e: P3, major: f64, minor: f64) -> (f64, P3) {
    let d = sub(p, c);
    let h = dot(d, e);
    let radial = sub(d, scale(e, h));
    let rho = norm(radial);
    // On the axis every point of the circle is as near: any one.
    let towards = unit(radial).unwrap_or_else(|| square_to(e).0);
    let on_circle = add(c, scale(towards, major));
    ((rho - major).hypot(h) - minor, on_circle)
}

/// The point of a disc (centre `c`, unit normal `e`, radius `r`) farthest
/// along `d`.
fn disc_support(c: P3, e: P3, r: f64, d: P3) -> P3 {
    let side = sub(d, scale(e, dot(d, e)));
    match unit(side) {
        Some(s) if r > 0.0 => add(c, scale(s, r)),
        _ => c,
    }
}

impl Convex {
    /// The point of the shape without its rounding (a sphere: its centre)
    /// farthest along `d`.
    fn core_support(&self, d: P3) -> P3 {
        match *self {
            Convex::Point(p) | Convex::Sphere { centre: p, .. } => p,
            Convex::Segment(a, b) => {
                if dot(b, d) > dot(a, d) {
                    b
                } else {
                    a
                }
            }
            Convex::Triangle(t) => {
                let mut best = t[0];
                for p in &t[1..] {
                    if dot(*p, d) > dot(best, d) {
                        best = *p;
                    }
                }
                best
            }
            Convex::Frustum { a, b, ra, rb } => {
                let Some(e) = unit(sub(b, a)) else {
                    return a;
                };
                let (pa, pb) = (disc_support(a, e, ra, d), disc_support(b, e, rb, d));
                if dot(pb, d) > dot(pa, d) { pb } else { pa }
            }
            Convex::Cuboid { centre, axes, half } => {
                let mut p = centre;
                for k in 0..3 {
                    let s = if dot(axes[k], d) >= 0.0 { 1.0 } else { -1.0 };
                    p = add(p, scale(axes[k], s * half[k]));
                }
                p
            }
            // Its convex hull (not used for distances: [`separation`]
            // measures a torus by its chords).
            Convex::Torus {
                centre,
                axis,
                major,
                minor,
            } => {
                let ring = disc_support(centre, axis, major, d);
                match unit(d) {
                    Some(u) => add(ring, scale(u, minor)),
                    None => ring,
                }
            }
        }
    }

    /// Its rounding: a sphere's radius (it is its centre grown by it).
    fn margin(&self) -> f64 {
        match *self {
            Convex::Sphere { radius, .. } => radius,
            _ => 0.0,
        }
    }

    /// A point inside it (its centre; a torus: a point of its tube's
    /// centre line, its own centre being in its hole).
    pub fn centre(&self) -> P3 {
        match *self {
            Convex::Point(p)
            | Convex::Sphere { centre: p, .. }
            | Convex::Cuboid { centre: p, .. } => p,
            Convex::Torus {
                centre,
                axis,
                major,
                ..
            } => add(centre, scale(square_to(axis).0, major)),
            Convex::Segment(a, b) | Convex::Frustum { a, b, .. } => scale(add(a, b), 0.5),
            Convex::Triangle(t) => scale(add(add(t[0], t[1]), t[2]), 1.0 / 3.0),
        }
    }

    /// Its axis-aligned bounds (low, high corner).
    pub fn bounds(&self) -> (P3, P3) {
        let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        let mut grow = |p: P3, r: [f64; 3]| {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k] - r[k]);
                hi[k] = hi[k].max(p[k] + r[k]);
            }
        };
        match *self {
            Convex::Point(p) => grow(p, [0.0; 3]),
            Convex::Segment(a, b) => {
                grow(a, [0.0; 3]);
                grow(b, [0.0; 3]);
            }
            Convex::Triangle(t) => t.into_iter().for_each(|p| grow(p, [0.0; 3])),
            Convex::Sphere { centre, radius } => grow(centre, [radius; 3]),
            Convex::Frustum { a, b, ra, rb } => {
                // A disc's extent along each axis: r √(1 − e_k²).
                let e = unit(sub(b, a)).unwrap_or([0.0, 0.0, 1.0]);
                let ext = |r: f64| e.map(|x| r * (1.0 - x * x).max(0.0).sqrt());
                grow(a, ext(ra));
                grow(b, ext(rb));
            }
            Convex::Cuboid { centre, axes, half } => {
                let r = [0, 1, 2].map(|k| (0..3).map(|i| axes[i][k].abs() * half[i]).sum());
                grow(centre, r);
            }
            Convex::Torus {
                centre,
                axis,
                major,
                minor,
            } => {
                // Its circle's extent along each axis, grown by the tube.
                let r = axis.map(|x| major * (1.0 - x * x).max(0.0).sqrt() + minor);
                grow(centre, r);
            }
        }
        (lo, hi)
    }

    /// Triangles of its surface for drawing: `segments` around a curved
    /// one (at least 6). Points, segments: none; a triangle: itself.
    pub fn mesh(&self, segments: usize) -> Vec<[P3; 3]> {
        let n = segments.max(6);
        let mut out = Vec::new();
        match *self {
            Convex::Point(_) | Convex::Segment(..) => {}
            Convex::Triangle(t) => out.push(t),
            Convex::Sphere { centre, radius } => {
                let stacks = n / 2;
                let at = |i: usize, j: usize| {
                    let th = std::f64::consts::PI * i as f64 / stacks as f64;
                    let ph = std::f64::consts::TAU * j as f64 / n as f64;
                    add(
                        centre,
                        [
                            radius * th.sin() * ph.cos(),
                            radius * th.sin() * ph.sin(),
                            radius * th.cos(),
                        ],
                    )
                };
                for i in 0..stacks {
                    for j in 0..n {
                        let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
                        if i > 0 {
                            out.push([a, b, d]);
                        }
                        if i + 1 < stacks {
                            out.push([b, c, d]);
                        }
                    }
                }
            }
            Convex::Frustum { a, b, ra, rb } => {
                let Some(e) = unit(sub(b, a)) else {
                    return out;
                };
                let (u, v) = square_to(e);
                let ring = |c: P3, r: f64, j: usize| {
                    let ph = std::f64::consts::TAU * j as f64 / n as f64;
                    add(c, add(scale(u, r * ph.cos()), scale(v, r * ph.sin())))
                };
                for j in 0..n {
                    let (p, q) = (ring(a, ra, j), ring(a, ra, j + 1));
                    let (s, t) = (ring(b, rb, j), ring(b, rb, j + 1));
                    out.push([p, s, q]);
                    out.push([q, s, t]);
                    if ra > 0.0 {
                        out.push([a, q, p]);
                    }
                    if rb > 0.0 {
                        out.push([b, s, t]);
                    }
                }
            }
            Convex::Cuboid { centre, axes, half } => {
                let corner = |i: usize| {
                    let mut p = centre;
                    for (k, axis) in axes.iter().enumerate() {
                        let s = if i & (1 << k) == 0 { -1.0 } else { 1.0 };
                        p = add(p, scale(*axis, s * half[k]));
                    }
                    p
                };
                let faces = [
                    [0, 1, 3, 2],
                    [4, 5, 7, 6],
                    [0, 1, 5, 4],
                    [2, 3, 7, 6],
                    [0, 2, 6, 4],
                    [1, 3, 7, 5],
                ];
                for f in faces {
                    out.push([corner(f[0]), corner(f[1]), corner(f[2])]);
                    out.push([corner(f[0]), corner(f[2]), corner(f[3])]);
                }
            }
            Convex::Torus {
                centre,
                axis,
                major,
                minor,
            } => {
                // Twice the steps around the ring as around its tube.
                let (u, v) = square_to(axis);
                let (around, tube) = (2 * n, n);
                let at = |i: usize, j: usize| {
                    let ph = std::f64::consts::TAU * i as f64 / around as f64;
                    let th = std::f64::consts::TAU * j as f64 / tube as f64;
                    let out_dir = add(scale(u, ph.cos()), scale(v, ph.sin()));
                    let ring = add(centre, scale(out_dir, major));
                    add(
                        ring,
                        add(
                            scale(out_dir, minor * th.cos()),
                            scale(axis, minor * th.sin()),
                        ),
                    )
                };
                for i in 0..around {
                    for j in 0..tube {
                        let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
                        out.push([a, b, c]);
                        out.push([a, c, d]);
                    }
                }
            }
        }
        out
    }
}

/// Two unit vectors square to unit `e` and to each other.
fn square_to(e: P3) -> (P3, P3) {
    let t = if e[2].abs() < 0.9 {
        [0.0, 0.0, 1.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let u = unit(cross(e, t)).unwrap_or([1.0, 0.0, 0.0]);
    (u, cross(e, u))
}

/// How far apart two convex shapes are.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Separation {
    /// The distance between their surfaces, m: a lower bound within 1 nm
    /// of it (conservative); 0 touching or overlapping (a sphere's radius
    /// into the other: negative).
    pub distance: f64,
    /// The nearest point of the first, m.
    pub on_a: P3,
    /// The nearest point of the second, m.
    pub on_b: P3,
}

/// A simplex vertex: the point of the difference, and of each shape.
type Vertex = (P3, P3, P3);

/// The point of the simplex `s` (1 to 4 vertices) nearest the origin: the
/// vertices of the smallest face holding it and their weights; `None`:
/// the origin is inside (a tetrahedron).
fn nearest_on_simplex(s: &[Vertex]) -> Option<(Vec<Vertex>, Vec<f64>)> {
    match s.len() {
        1 => Some((s.to_vec(), vec![1.0])),
        2 => Some(on_segment(s[0], s[1])),
        3 => Some(on_triangle(s[0], s[1], s[2])),
        _ => on_tetrahedron(s),
    }
}

fn on_segment(a: Vertex, b: Vertex) -> (Vec<Vertex>, Vec<f64>) {
    let ab = sub(b.0, a.0);
    let l2 = dot(ab, ab);
    let t = if l2 > 0.0 { -dot(a.0, ab) / l2 } else { 0.0 };
    if t <= 0.0 {
        (vec![a], vec![1.0])
    } else if t >= 1.0 {
        (vec![b], vec![1.0])
    } else {
        (vec![a, b], vec![1.0 - t, t])
    }
}

/// The weights' point.
fn point_of(v: &[Vertex], w: &[f64]) -> P3 {
    v.iter()
        .zip(w)
        .fold([0.0; 3], |acc, (x, k)| add(acc, scale(x.0, *k)))
}

/// Ericson 5.1.5 with the origin as the point, by region.
fn on_triangle(a: Vertex, b: Vertex, c: Vertex) -> (Vec<Vertex>, Vec<f64>) {
    let (ab, ac, ap) = (sub(b.0, a.0), sub(c.0, a.0), scale(a.0, -1.0));
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return (vec![a], vec![1.0]);
    }
    let bp = scale(b.0, -1.0);
    let (d3, d4) = (dot(ab, bp), dot(ac, bp));
    if d3 >= 0.0 && d4 <= d3 {
        return (vec![b], vec![1.0]);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let t = d1 / (d1 - d3);
        return (vec![a, b], vec![1.0 - t, t]);
    }
    let cp = scale(c.0, -1.0);
    let (d5, d6) = (dot(ab, cp), dot(ac, cp));
    if d6 >= 0.0 && d5 <= d6 {
        return (vec![c], vec![1.0]);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let t = d2 / (d2 - d6);
        return (vec![a, c], vec![1.0 - t, t]);
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let t = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return (vec![b, c], vec![1.0 - t, t]);
    }
    let sum = va + vb + vc;
    if sum.abs() <= 1e-300 || sum.is_nan() {
        // A flat triangle: its nearest edge.
        return [on_segment(a, b), on_segment(b, c), on_segment(a, c)]
            .into_iter()
            .min_by(|x, y| norm(point_of(&x.0, &x.1)).total_cmp(&norm(point_of(&y.0, &y.1))))
            .expect("three edges");
    }
    let (v, w) = (vb / sum, vc / sum);
    (vec![a, b, c], vec![1.0 - v - w, v, w])
}

fn on_tetrahedron(s: &[Vertex]) -> Option<(Vec<Vertex>, Vec<f64>)> {
    let faces = [(0, 1, 2, 3), (0, 1, 3, 2), (0, 2, 3, 1), (1, 2, 3, 0)];
    let mut best: Option<(Vec<Vertex>, Vec<f64>, f64)> = None;
    let mut outside = false;
    for (i, j, k, o) in faces {
        let n = cross(sub(s[j].0, s[i].0), sub(s[k].0, s[i].0));
        let side_origin = -dot(n, s[i].0);
        let side_other = dot(n, sub(s[o].0, s[i].0));
        // The origin beyond this face (from the fourth vertex), or the
        // tetrahedron flat: the face is a candidate.
        let flat = side_other.abs() <= 1e-18 * dot(n, n).sqrt().max(1e-300);
        if !flat && side_origin * side_other >= 0.0 {
            continue;
        }
        outside = true;
        let (v, w) = on_triangle(s[i], s[j], s[k]);
        let d = norm(point_of(&v, &w));
        if best.as_ref().is_none_or(|b| d < b.2) {
            best = Some((v, w, d));
        }
    }
    if !outside {
        return None;
    }
    best.map(|(v, w, _)| (v, w))
}

/// How near the shapes come: the absolute tolerance, m.
const TOLERANCE: f64 = 1e-9;
/// The most steps (a lower bound is kept when it ends there).
const MAX_STEPS: usize = 200;

/// The distance between shapes `a` and `b`: convex ones by GJK (van den
/// Bergen's form, on their support functions; a sphere as its centre
/// grown by its radius) — exact for polytopes, within a tolerance
/// (`TOLERANCE`, 1 nm) for curved ones —; a torus as the module says
/// (exact against a point or a sphere, else by its chords, at most
/// 2 × [`TORUS_SAGITTA`] below; two tori 4 ×). Never more than the exact
/// distance.
pub fn separation(a: &Convex, b: &Convex) -> Separation {
    match (*a, *b) {
        (Convex::Torus { .. }, _) => torus_separation(a, b),
        (_, Convex::Torus { .. }) => {
            let s = torus_separation(b, a);
            Separation {
                distance: s.distance,
                on_a: s.on_b,
                on_b: s.on_a,
            }
        }
        _ => gjk(a, b, a.margin(), b.margin()),
    }
}

/// The distance from torus `t` to shape `other` (a torus too): exact
/// against a point or a sphere, else the least over its chords, each
/// grown by the tube and the sagitta ([`torus_chords`]; conservative, as
/// the module says).
fn torus_separation(t: &Convex, other: &Convex) -> Separation {
    let Convex::Torus {
        centre,
        axis,
        major,
        minor,
    } = *t
    else {
        unreachable!("a torus")
    };
    let towards = |from: P3, to: P3, by: f64| match unit(sub(to, from)) {
        Some(d) => add(from, scale(d, by)),
        None => from,
    };
    if let Convex::Point(p) | Convex::Sphere { centre: p, .. } = *other {
        let r = other.margin();
        let (d, ring) = point_torus(p, centre, axis, major, minor);
        return Separation {
            distance: d - r,
            on_a: towards(ring, p, minor),
            on_b: towards(p, ring, r),
        };
    }
    let (n, sagitta) = torus_chords(major);
    let ring = circle_points(centre, axis, major, n);
    // The other's chords too when it is a torus (its own grown likewise).
    let pieces: Vec<(Convex, f64)> = match *other {
        Convex::Torus {
            centre: c2,
            axis: e2,
            major: big2,
            minor: r2,
        } => {
            let (n2, s2) = torus_chords(big2);
            circle_points(c2, e2, big2, n2)
                .windows(2)
                .map(|w| (Convex::Segment(w[0], w[1]), r2 + s2))
                .collect()
        }
        o => vec![(o, o.margin())],
    };
    let bounds: Vec<(P3, P3)> = pieces.iter().map(|p| p.0.bounds()).collect();
    let mut best: Option<Separation> = None;
    for w in ring.windows(2) {
        let chord = Convex::Segment(w[0], w[1]);
        let (lo, hi) = chord.bounds();
        for ((piece, margin), (plo, phi)) in pieces.iter().zip(&bounds) {
            // A piece whose box is farther than the best so far cannot be
            // nearer (the boxes' gap is a lower bound of the cores').
            if let Some(b) = &best {
                let gap = (0..3)
                    .map(|k| (plo[k] - hi[k]).max(lo[k] - phi[k]).max(0.0))
                    .map(|g| g * g)
                    .sum::<f64>()
                    .sqrt();
                if gap - minor - sagitta - margin >= b.distance {
                    continue;
                }
            }
            let s = gjk(&chord, piece, minor + sagitta, *margin);
            if best.as_ref().is_none_or(|b| s.distance < b.distance) {
                best = Some(s);
            }
        }
    }
    best.expect("a torus has chords")
}

/// GJK between convex shapes `a` and `b` grown by `ma` and `mb` (their
/// cores by [`Convex::core_support`]; never a torus).
fn gjk(a: &Convex, b: &Convex, ma: f64, mb: f64) -> Separation {
    let support = |d: P3| -> Vertex {
        let (pa, pb) = (a.core_support(scale(d, -1.0)), b.core_support(d));
        (sub(pa, pb), pa, pb)
    };
    let first = {
        let (pa, pb) = (a.centre(), b.centre());
        (sub(pa, pb), pa, pb)
    };
    let mut simplex: Vec<Vertex> = vec![first];
    let mut weights = vec![1.0];
    let mut v = first.0;
    let mut lower: f64 = 0.0;
    let mut touching = false;
    for _ in 0..MAX_STEPS {
        let vv = dot(v, v);
        if vv <= 1e-24 {
            touching = true;
            break;
        }
        let vn = vv.sqrt();
        // The support against v: every point of a − b lies at least v·w/|v|
        // along v — a lower bound of the distance.
        let w = support(v);
        lower = lower.max(dot(v, w.0) / vn);
        if vn - lower <= TOLERANCE {
            break;
        }
        if simplex
            .iter()
            .any(|s| norm(sub(s.0, w.0)) <= 1e-15 * (1.0 + vn))
        {
            break;
        }
        simplex.push(w);
        match nearest_on_simplex(&simplex) {
            None => {
                touching = true;
                break;
            }
            Some((s, k)) => {
                v = point_of(&s, &k);
                simplex = s;
                weights = k;
            }
        }
    }
    let mut on_a = simplex
        .iter()
        .zip(&weights)
        .fold([0.0; 3], |acc, (s, k)| add(acc, scale(s.1, *k)));
    let mut on_b = simplex
        .iter()
        .zip(&weights)
        .fold([0.0; 3], |acc, (s, k)| add(acc, scale(s.2, *k)));
    let core = if touching { 0.0 } else { lower.max(0.0) };
    // A sphere's surface: its radius towards the other.
    if let Some(d) = unit(sub(on_b, on_a)).filter(|_| !touching) {
        on_a = add(on_a, scale(d, ma));
        on_b = sub(on_b, scale(d, mb));
    }
    Separation {
        distance: core - ma - mb,
        on_a,
        on_b,
    }
}

/// A solved string, for placing its shapes ([`place`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StringAt {
    /// Its top as solved, m.
    pub top: P3,
    /// Its wire point as solved, m.
    pub bottom: P3,
    /// Its top and wire point in still air (as placed in the line), m.
    pub still: (P3, P3),
    /// A strain string (its axis the conductor's pull).
    pub strain: bool,
    /// Its structure's local x axis (right of the line), degrees
    /// counter-clockwise from east.
    pub orientation_deg: f64,
    /// The conductor's direction in plan at its wire point, as solved
    /// (none: not known — the structure's axes taken).
    pub wire_direction: Option<[f64; 2]>,
}

/// One shape placed on a solved string.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    /// The shape's name.
    pub name: String,
    /// Several strings in one assembly: which string's copy (from 0);
    /// none: one string, or a shared shape.
    pub copy: Option<usize>,
    /// Its electrical state.
    pub state: ohl_structures::ElectricalState,
    /// Where it is.
    pub convex: Convex,
}

/// The string's own axes as the freedoms give them: (across, along,
/// axis), unit vectors, and its solved axis and length. None: the string
/// has no length.
fn frame(v: &ohl_structures::StringVolumes, s: &StringAt) -> Option<([P3; 3], P3, f64)> {
    let ([c, n], [a0, a1]) = ohl_line::links::axes(s.orientation_deg);
    let (x, y, z) = ([c, n, 0.0], [a0, a1, 0.0], [0.0, 0.0, 1.0]);
    let d = sub(s.bottom, s.top);
    let length = norm(d);
    let u = unit(d)?;
    if s.strain {
        // Across: level, square to the axis, to the right; along: up.
        let mut across = unit(cross(z, u)).unwrap_or(x);
        if dot(across, x) < 0.0 {
            across = scale(across, -1.0);
        }
        let along = unit(sub(z, scale(u, dot(z, u))))
            .map(|a| unit(sub(a, scale(across, dot(a, across)))).unwrap_or(a))
            .unwrap_or_else(|| cross(across, u));
        return Some(([across, along, u], u, length));
    }
    // The swings across and along: the solved one where free, the still
    // air one where blocked (a string less than ~3° below level: as
    // solved).
    let angles = |w: P3| (dot(w, x).atan2(-dot(w, z)), dot(w, y).atan2(-dot(w, z)));
    let axis = match unit(sub(s.still.1, s.still.0)) {
        Some(u0) if -dot(u, z) > 0.05 && -dot(u0, z) > 0.05 => {
            let (mut a, mut b) = angles(u);
            let (a0, b0) = angles(u0);
            if !v.swing_across.is_free() {
                a = a0;
            }
            if !v.swing_along.is_free() {
                b = b0;
            }
            unit(add(
                add(scale(x, a.tan()), scale(y, b.tan())),
                scale(z, -1.0),
            ))
            .unwrap_or(u)
        }
        _ => u,
    };
    // Across: the structure's x (twist blocked), else across the conductor.
    let mut r = x;
    if v.twist.is_free()
        && let Some(w) = s.wire_direction.and_then(|w| unit([w[0], w[1], 0.0]))
    {
        r = [w[1], -w[0], 0.0];
        if dot(r, x) < 0.0 {
            r = scale(r, -1.0);
        }
    }
    let across = unit(sub(r, scale(axis, dot(r, axis))))
        .or_else(|| unit(sub(y, scale(axis, dot(y, axis)))))
        .unwrap_or(x);
    let along = cross(across, axis);
    Some(([across, along, axis], u, length))
}

/// The shape `s` with its centre at `c`, its axes `e` (across, along,
/// axis).
fn convex_of(s: &ohl_structures::StringShape, c: P3, e: [P3; 3]) -> Convex {
    use ohl_structures::ShapeKind;
    let half = scale(e[2], s.length / 2.0);
    match s.kind {
        ShapeKind::Sphere => Convex::Sphere {
            centre: c,
            radius: s.diameter / 2.0,
        },
        ShapeKind::Cylinder => Convex::Frustum {
            a: sub(c, half),
            b: add(c, half),
            ra: s.diameter / 2.0,
            rb: s.diameter / 2.0,
        },
        ShapeKind::Cone => Convex::Frustum {
            a: sub(c, half),
            b: add(c, half),
            ra: s.diameter / 2.0,
            rb: s.end_diameter / 2.0,
        },
        ShapeKind::Box => Convex::Cuboid {
            centre: c,
            axes: e,
            half: [s.width / 2.0, s.depth / 2.0, s.length / 2.0],
        },
        // Around the string's axis, square to it.
        ShapeKind::Torus => {
            let (major, minor) = s.torus_radii().unwrap_or((0.0, 0.0));
            Convex::Torus {
                centre: c,
                axis: e[2],
                major,
                minor,
            }
        }
    }
}

/// The shapes `shapes` (an assembly's, its string's own merged in:
/// [`ohl_structures::StringVolumes::with_overrides`]) placed on the solved
/// string `at` by the freedoms and strings of `volumes`, as the module
/// says: a copy on each string of the assembly, a shared one once.
pub fn place(
    volumes: &ohl_structures::StringVolumes,
    shapes: &[ohl_structures::StringShape],
    at: &StringAt,
) -> Vec<Placed> {
    let Some((e, u, length)) = frame(volumes, at) else {
        return Vec::new();
    };
    let offsets = volumes.string_offsets();
    let several = offsets.len() > 1;
    let mut out = Vec::new();
    for s in shapes {
        let copies: Vec<(Option<usize>, [f64; 2])> = if several && !s.shared {
            offsets
                .iter()
                .enumerate()
                .map(|(k, o)| (Some(k), *o))
                .collect()
        } else {
            vec![(None, [0.0; 2])]
        };
        for (copy, o) in copies {
            let along = s.at.metres(length);
            let c = add(
                add(at.top, scale(u, along)),
                add(
                    scale(e[0], o[0] + s.offset[0]),
                    scale(e[1], o[1] + s.offset[1]),
                ),
            );
            out.push(Placed {
                name: s.name.clone(),
                copy,
                state: s.state,
                convex: convex_of(s, c, e),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ohl_structures::{Along, ElectricalState, ShapeKind, StringShape, StringVolumes};

    fn d(a: Convex, b: Convex) -> f64 {
        separation(&a, &b).distance
    }

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-8, "{a} != {b}");
    }

    /// A point, a segment and a triangle against each primitive: the
    /// distances by hand (and never above them).
    #[test]
    fn each_primitive_against_points_segments_and_triangles() {
        // A sphere of radius 1 at the origin.
        let sphere = Convex::Sphere {
            centre: [0.0; 3],
            radius: 1.0,
        };
        close(d(sphere, Convex::Point([3.0, 0.0, 0.0])), 2.0);
        // A segment passing 3 m from the centre: 2 m.
        close(
            d(sphere, Convex::Segment([3.0, -5.0, 0.0], [3.0, 5.0, 0.0])),
            2.0,
        );
        // A triangle in the plane z = 4 above it: 3 m.
        let tri = Convex::Triangle([[-1.0, -1.0, 4.0], [2.0, -1.0, 4.0], [0.0, 2.0, 4.0]]);
        close(d(sphere, tri), 3.0);
        // Overlapping: the radius beyond the centre's distance, negative.
        assert!(d(sphere, Convex::Point([0.5, 0.0, 0.0])) < 0.0);

        // A cylinder of radius 0.5 on the z axis from 0 to 2.
        let cyl = Convex::Frustum {
            a: [0.0; 3],
            b: [0.0, 0.0, 2.0],
            ra: 0.5,
            rb: 0.5,
        };
        // Beside its side: 2 − 0.5.
        close(d(cyl, Convex::Point([2.0, 0.0, 1.0])), 1.5);
        // Above its end: 3 − 2.
        close(d(cyl, Convex::Point([0.2, 0.1, 3.0])), 1.0);
        // Off its rim: from (0.5, 0, 2) to (1.5, 0, 3): √2.
        close(d(cyl, Convex::Point([1.5, 0.0, 3.0])), 2f64.sqrt());
        // A segment crossing above, level: 1.
        close(
            d(cyl, Convex::Segment([-4.0, 0.3, 3.0], [4.0, 0.3, 3.0])),
            1.0,
        );
        // A slanted triangle beside its side: the plane x = 2 → 1.5.
        let side = Convex::Triangle([[2.0, -1.0, -1.0], [2.0, 1.0, 0.0], [2.0, 0.0, 3.0]]);
        close(d(cyl, side), 1.5);

        // A cone: radius 1 at z = 0, a point at z = 2 (apex).
        let cone = Convex::Frustum {
            a: [0.0; 3],
            b: [0.0, 0.0, 2.0],
            ra: 1.0,
            rb: 0.0,
        };
        // Above the apex: 1.
        close(d(cone, Convex::Point([0.0, 0.0, 3.0])), 1.0);
        // Square to its side through (0.5, 0, 1): the side's normal is
        // (2, 0, 1)/√5; 1 m out along it.
        let n = [2.0 / 5f64.sqrt(), 0.0, 1.0 / 5f64.sqrt()];
        close(d(cone, Convex::Point([0.5 + n[0], 0.0, 1.0 + n[2]])), 1.0);
        // Under its base: 0.5.
        close(
            d(cone, Convex::Segment([-0.3, -2.0, -0.5], [-0.3, 2.0, -0.5])),
            0.5,
        );

        // A box 2 × 1 × 4 (half 1, 0.5, 2), turned 90° about z.
        let boxed = Convex::Cuboid {
            centre: [0.0; 3],
            axes: [[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            half: [1.0, 0.5, 2.0],
        };
        // Along x its half size is 0.5: a point at x = 3 → 2.5.
        close(d(boxed, Convex::Point([3.0, 0.0, 0.0])), 2.5);
        // Off a corner (0.5, 1, 2) to (1.5, 2, 3): √3.
        close(d(boxed, Convex::Point([1.5, 2.0, 3.0])), 3f64.sqrt());
        // A triangle above: z = 5 → 3.
        let above = Convex::Triangle([[-5.0, -5.0, 5.0], [5.0, -5.0, 5.0], [0.0, 5.0, 5.0]]);
        close(d(boxed, above), 3.0);
        // A segment through it: 0.
        close(
            d(boxed, Convex::Segment([-3.0, 0.0, 0.0], [3.0, 0.0, 0.0])),
            0.0,
        );
    }

    /// A box against many segments (a fixed pseudo-random set) and a
    /// cylinder against many points: the same as the exact segment–triangle
    /// distance to the box's faces, and as the cylinder's closed form —
    /// never above, within 1e-8 m.
    #[test]
    fn against_other_exact_distances() {
        let mut seed = 7u64;
        let mut r = || {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            ((seed >> 11) as f64 / (1u64 << 53) as f64) * 8.0 - 4.0
        };
        let c = 0.6f64.cos();
        let s = 0.6f64.sin();
        let boxed = Convex::Cuboid {
            centre: [0.3, -0.2, 0.1],
            axes: [[c, s, 0.0], [-s, c, 0.0], [0.0, 0.0, 1.0]],
            half: [0.7, 0.4, 1.1],
        };
        let faces = boxed.mesh(6);
        for _ in 0..300 {
            let (p, q) = ([r(), r(), r()], [r(), r(), r()]);
            let gjk = d(boxed, Convex::Segment(p, q));
            let exact = faces
                .iter()
                .map(|t| {
                    let (a, b) = crate::geometry::closest_segment_triangle(p, q, t[0], t[1], t[2]);
                    norm(sub(a, b))
                })
                .fold(f64::INFINITY, f64::min);
            // Inside the box (no face near): 0 from GJK, its faces farther.
            assert!(gjk <= exact + 1e-9, "{gjk} > {exact}");
            if gjk > 0.0 {
                assert!((gjk - exact).abs() < 1e-8, "{gjk} {exact}");
            }
        }
        let cyl = Convex::Frustum {
            a: [0.0, 0.0, -1.0],
            b: [0.0, 0.0, 1.0],
            ra: 0.8,
            rb: 0.8,
        };
        for _ in 0..300 {
            let p = [r(), r(), r()];
            let radial = (p[0].hypot(p[1]) - 0.8).max(0.0);
            let axial = (p[2].abs() - 1.0).max(0.0);
            let exact = radial.hypot(axial);
            let gjk = d(cyl, Convex::Point(p));
            assert!(gjk <= exact + 1e-12, "{gjk} > {exact}");
            assert!((gjk - exact).abs() < 1e-8, "{p:?}: {gjk} {exact}");
        }
    }

    /// A ring of centre circle 1 m, tube 0.2 m (radius 0.1), level at the
    /// origin.
    fn ring() -> Convex {
        Convex::Torus {
            centre: [0.0; 3],
            axis: [0.0, 0.0, 1.0],
            major: 1.0,
            minor: 0.1,
        }
    }

    /// Never above `exact`, at most twice the sagitta (and `slack`) below.
    fn safe(found: f64, exact: f64, slack: f64) {
        let s = TORUS_SAGITTA;
        assert!(found <= exact + 1e-9, "{found} > {exact}: not conservative");
        assert!(found >= exact - 2.0 * s - slack - 1e-9, "{found} ≪ {exact}");
    }

    /// The torus by hand: a point and a sphere exactly (closed form); a
    /// segment through its hole, a plane over it, a box beside it and
    /// another ring over it never farther than by hand and at most twice
    /// the sagitta nearer; the sagitta at most 0.25 mm.
    #[test]
    fn torus_by_hand() {
        let t = ring();
        let (n, s) = torus_chords(1.0);
        assert!(s <= TORUS_SAGITTA && n >= TORUS_MIN_CHORDS, "{n} {s}");
        // Beside it: 3 − 1 − 0.1.
        close(d(t, Convex::Point([3.0, 0.0, 0.0])), 1.9);
        // On its axis 2 m up: √(1 + 4) − 0.1.
        close(d(Convex::Point([0.0, 0.0, 2.0]), t), 5f64.sqrt() - 0.1);
        // A sphere of 0.5 m in its hole: 1 − 0.1 − 0.5.
        let ball = Convex::Sphere {
            centre: [0.0; 3],
            radius: 0.5,
        };
        close(d(t, ball), 0.4);
        // Inside the tube: negative.
        assert!(d(t, Convex::Point([1.05, 0.0, 0.0])) < 0.0);
        // A segment down its axis, through the hole: 1 − 0.1.
        safe(
            d(t, Convex::Segment([0.0, 0.0, -5.0], [0.0, 0.0, 5.0])),
            0.9,
            0.0,
        );
        // A triangle in the plane z = 0.5 over it: 0.5 − 0.1.
        let over = Convex::Triangle([[-5.0, -5.0, 0.5], [5.0, -5.0, 0.5], [0.0, 5.0, 0.5]]);
        safe(d(t, over), 0.4, 0.0);
        // A box whose face is at x = 1.75: 1.75 − 1.1.
        let beside = Convex::Cuboid {
            centre: [2.0, 0.0, 0.0],
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            half: [0.25, 3.0, 3.0],
        };
        safe(d(t, beside), 0.65, 0.0);
        // The same ring 1 m higher: 1 − 0.2 (both rings' chords).
        let higher = Convex::Torus {
            centre: [0.0, 0.0, 1.0],
            axis: [0.0, 0.0, 1.0],
            major: 1.0,
            minor: 0.1,
        };
        let s2 = separation(&t, &higher);
        safe(s2.distance, 0.8, 2.0 * TORUS_SAGITTA);
        // The witness points on the two surfaces, about 0.8 apart.
        assert!((norm(sub(s2.on_a, s2.on_b)) - 0.8).abs() < 0.002);
        // A ring of 0.5 m through its hole, centred on its circle (a
        // chain's links): every point of the one circle 0.5 from the
        // other's, 0.5 − 0.2 between the surfaces.
        let link = Convex::Torus {
            centre: [1.0, 0.0, 0.0],
            axis: [0.0, 1.0, 0.0],
            major: 0.5,
            minor: 0.1,
        };
        safe(d(t, link), 0.3, 2.0 * TORUS_SAGITTA);
        // Its box and its mesh: every vertex on its surface.
        let (lo, hi) = t.bounds();
        close(lo[0], -1.1);
        close(hi[2], 0.1);
        for tri in t.mesh(12) {
            for p in tri {
                close(d(t, Convex::Point(p)), 0.0);
            }
        }
        // A point inside it (on its tube's centre line).
        assert!(d(t, Convex::Point(t.centre())) < -0.099);
    }

    /// The torus against many segments (a fixed pseudo-random set): never
    /// above the least distance of 20 000 points along each (each exact,
    /// in closed form), and at most twice the sagitta and the sampling's
    /// step below it.
    #[test]
    fn torus_against_sampled_segments() {
        let mut seed = 11u64;
        let mut r = || {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            ((seed >> 11) as f64 / (1u64 << 53) as f64) * 6.0 - 3.0
        };
        let t = Convex::Torus {
            centre: [0.2, -0.1, 0.3],
            axis: unit([0.3, -0.2, 1.0]).unwrap(),
            major: 1.0,
            minor: 0.1,
        };
        for _ in 0..60 {
            let (p, q) = ([r(), r(), r()], [r(), r(), r()]);
            let steps = 20_000;
            let sampled = (0..=steps)
                .map(|k| {
                    let x = add(p, scale(sub(q, p), k as f64 / steps as f64));
                    d(t, Convex::Point(x))
                })
                .fold(f64::INFINITY, f64::min);
            let found = d(t, Convex::Segment(p, q));
            // The sampling's own error: half a step (the distance changes
            // at most as fast as the point moves).
            let half_step = norm(sub(q, p)) / steps as f64 / 2.0;
            if sampled > half_step {
                safe(found, sampled + 1e-12, half_step);
            } else {
                assert!(found <= sampled + 1e-9, "{found} > {sampled}");
            }
        }
    }

    /// A grading ring on a string hanging straight down: around the
    /// string's axis at the wire end, its axis plumb.
    #[test]
    fn torus_placed_around_the_string() {
        let at = StringAt {
            top: [0.0, 0.0, 30.0],
            bottom: [0.0, 0.0, 27.0],
            still: ([0.0, 0.0, 30.0], [0.0, 0.0, 27.0]),
            strain: false,
            orientation_deg: 0.0,
            wire_direction: None,
        };
        let mut ring = StringShape::new("ring", ShapeKind::Torus, ElectricalState::Electrified);
        ring.at = Along::Metres(2.8);
        ring.diameter = 0.6;
        ring.tube = 0.06;
        let p = place(&StringVolumes::default(), &[ring], &at);
        let Convex::Torus {
            centre,
            axis,
            major,
            minor,
        } = p[0].convex
        else {
            panic!()
        };
        close(norm(sub(centre, [0.0, 0.0, 27.2])), 0.0);
        close(axis[2].abs(), 1.0);
        close(major, 0.27);
        close(minor, 0.03);
        // The string's axis passes through the hole: 0.27 − 0.03 from it.
        safe(
            d(p[0].convex, Convex::Segment(at.top, at.bottom)),
            0.24,
            0.0,
        );
    }

    /// Shapes against shapes: two spheres, a sphere and a cylinder, two
    /// boxes, the witness points on their surfaces.
    #[test]
    fn shapes_against_shapes() {
        let a = Convex::Sphere {
            centre: [0.0; 3],
            radius: 0.5,
        };
        let b = Convex::Sphere {
            centre: [3.0, 4.0, 0.0],
            radius: 1.0,
        };
        let s = separation(&a, &b);
        close(s.distance, 3.5);
        close(norm(s.on_a), 0.5);
        close(norm(sub(s.on_b, [3.0, 4.0, 0.0])), 1.0);
        let cyl = Convex::Frustum {
            a: [5.0, -1.0, 0.0],
            b: [5.0, 1.0, 0.0],
            ra: 0.25,
            rb: 0.25,
        };
        close(d(a, cyl), 5.0 - 0.5 - 0.25);
        let unit_box = |c: P3| Convex::Cuboid {
            centre: c,
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            half: [0.5; 3],
        };
        close(d(unit_box([0.0; 3]), unit_box([2.0, 0.0, 0.0])), 1.0);
        close(d(unit_box([0.0; 3]), unit_box([0.5, 0.5, 0.5])), 0.0);
    }

    /// A suspension string 3 m long hanging straight down, swung 30°
    /// across: shapes on the solved axis, the across axis turned with the
    /// swing (free) or kept (blocked); two strings side by side 0.4 m
    /// apart, a shared shape once; a strain string level ahead: across to
    /// the right, along up.
    #[test]
    fn shapes_placed_on_a_solved_string() {
        let top = [0.0, 0.0, 30.0];
        let swing = 30f64.to_radians();
        let bottom = [3.0 * swing.sin(), 0.0, 30.0 - 3.0 * swing.cos()];
        let at = StringAt {
            top,
            bottom,
            still: (top, [0.0, 0.0, 27.0]),
            strain: false,
            orientation_deg: 0.0,
            wire_direction: None,
        };
        let mut clamp = StringShape::new("clamp", ShapeKind::Box, ElectricalState::Electrified);
        clamp.at = Along::Fraction(1.0);
        clamp.width = 0.4;
        clamp.depth = 0.2;
        clamp.length = 0.1;
        let mut horn = StringShape::new("horn", ShapeKind::Sphere, ElectricalState::Grounded);
        horn.at = Along::Metres(0.0);
        horn.offset = [0.5, 0.0];
        let free = StringVolumes::default();
        let p = place(&free, &[clamp.clone(), horn.clone()], &at);
        assert_eq!(p.len(), 2);
        let Convex::Cuboid { centre, axes, .. } = p[0].convex else {
            panic!()
        };
        close(norm(sub(centre, bottom)), 0.0);
        // Its axis along the string, its across square to it.
        close(dot(axes[2], scale(sub(bottom, top), 1.0 / 3.0)), 1.0);
        close(dot(axes[0], axes[2]), 0.0);
        // The horn 0.5 m off the top, across the swung string.
        let Convex::Sphere { centre: h, .. } = p[1].convex else {
            panic!()
        };
        close(h[0], 0.5 * swing.cos());
        close(h[2], 30.0 + 0.5 * swing.sin());
        // The swing across blocked: the shapes keep hanging plumb, but
        // stay on the solved axis.
        let blocked = StringVolumes {
            swing_across: ohl_structures::Freedom::Blocked,
            ..Default::default()
        };
        let p = place(&blocked, &[clamp.clone(), horn.clone()], &at);
        let Convex::Cuboid { centre, axes, .. } = p[0].convex else {
            panic!()
        };
        close(norm(sub(centre, bottom)), 0.0);
        close(axes[2][2], -1.0);
        let Convex::Sphere { centre: h, .. } = p[1].convex else {
            panic!()
        };
        close(norm(sub(h, [0.5, 0.0, 30.0])), 0.0);
        // Two strings across, 0.4 m apart: two horns, one shared clamp.
        let mut clamp_shared = clamp.clone();
        clamp_shared.shared = true;
        let two = StringVolumes {
            strings: 2,
            spacing: 0.4,
            ..Default::default()
        };
        let still = StringAt {
            bottom: [0.0, 0.0, 27.0],
            ..at
        };
        let p = place(&two, &[clamp_shared, horn], &still);
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].copy, None);
        let xs: Vec<f64> = p[1..]
            .iter()
            .map(|x| match x.convex {
                Convex::Sphere { centre, .. } => centre[0],
                _ => panic!(),
            })
            .collect();
        close(xs[0], -0.2 + 0.5);
        close(xs[1], 0.2 + 0.5);
        // A strain string level towards the east, its structure turned
        // 90° (local x, the right of the line, north): across north,
        // along up.
        let strain = StringAt {
            top: [0.0, 0.0, 20.0],
            bottom: [3.0, 0.0, 20.0],
            still: ([0.0, 0.0, 20.0], [3.0, 0.0, 20.0]),
            strain: true,
            orientation_deg: 90.0,
            wire_direction: None,
        };
        let mut up = StringShape::new("up", ShapeKind::Sphere, ElectricalState::Grounded);
        up.at = Along::Fraction(0.0);
        up.offset = [0.0, 0.3];
        let p = place(&free, &[up], &strain);
        let Convex::Sphere { centre, .. } = p[0].convex else {
            panic!()
        };
        close(norm(sub(centre, [0.0, 0.0, 20.3])), 0.0);
    }
}
