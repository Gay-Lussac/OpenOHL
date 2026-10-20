// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The wire positions the danger-tree checks look at (theory: LiDAR
//! analyses › Danger trees › Wire positions checked): each span of each
//! section in each weather case — still air as the case is, a windy case
//! blown to the left and to the right, the optional **swing arc** between
//! them, the optional **wind steps** (still air and every intermediate
//! pressure from each side), and the optional **unbalanced ice** on the
//! span alone.
//!
//! Distances are closed forms: to a solved wire (a polyline), the nearest
//! of its pieces, looked for outward from the point's place along the
//! span ([`WireShape`]); to a swing arc, [`SwingArc::nearest`].

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use ohl_core::budget::Budget;
use ohl_line::Line;
use ohl_line::solve::SectionShape;
use ohl_weather::{WeatherCase, WindSide};

use crate::checks::{Wire, wires_of};
use crate::geometry::{P3, closest_on_segment, distance};

fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn lerp(a: P3, b: P3, t: f64) -> P3 {
    [
        a[0] + t * (b[0] - a[0]),
        a[1] + t * (b[1] - a[1]),
        a[2] + t * (b[2] - a[2]),
    ]
}

fn cross(a: P3, b: P3) -> P3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn unit(a: P3) -> Option<P3> {
    let n = dot(a, a).sqrt();
    (n > 0.0 && n.is_finite()).then(|| [a[0] / n, a[1] / n, a[2] / n])
}

/// The smallest distance between the segment a–b and the horizontal
/// circle of radius `root` about `base`, and the segment's point where it
/// is found.
///
/// From a point q at horizontal distance ρ from the base and height ζ
/// above it, the circle is \\( \sqrt{(\rho - b_r)^2 + \zeta^2} \\) away.
/// Along the segment its square \\( \varphi(t) = |q - B|^2 - 2 b_r \rho +
/// b_r^2 \\) has \\( \varphi'' \ge 2|d|^2 (1 - b_r/\rho) \\) (d = b − a): it
/// is convex where the segment stays further than b_r from the base in
/// plan, and its one minimum is found by Newton steps kept inside a
/// shrinking bracket (to 10⁻¹² of the segment). Where the segment passes
/// within b_r in plan, the minimum is bracketed by the bound of a
/// distance (it changes by at most |d| per unit of t) to 10⁻⁶ m and the
/// bound returned (rare: a wire over the root ball).
pub fn segment_to_circle(a: P3, b: P3, base: P3, root: f64) -> (f64, P3) {
    let d = sub(b, a);
    let e = sub(a, base);
    let at = |t: f64| lerp(a, b, t);
    let value = |t: f64| {
        let q = sub(at(t), base);
        let rho = q[0].hypot(q[1]);
        (rho - root).hypot(q[2])
    };
    // Smallest horizontal distance from the base along the segment.
    let dh2 = d[0] * d[0] + d[1] * d[1];
    let th = if dh2 > 0.0 {
        (-(e[0] * d[0] + e[1] * d[1]) / dh2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let rho_min = (e[0] + th * d[0]).hypot(e[1] + th * d[1]);
    if rho_min > root {
        // φ'(t) and φ''(t).
        let dd = dot(d, d);
        let ed = dot(e, d);
        let deriv = |t: f64| {
            let (hx, hy) = (e[0] + t * d[0], e[1] + t * d[1]);
            let rho = hx.hypot(hy);
            let rp = (hx * d[0] + hy * d[1]) / rho;
            let rpp = (dh2 - rp * rp) / rho;
            (
                2.0 * (ed + t * dd) - 2.0 * root * rp,
                2.0 * dd - 2.0 * root * rpp,
            )
        };
        let (g0, _) = deriv(0.0);
        let (g1, _) = deriv(1.0);
        let t = if g0 >= 0.0 {
            0.0
        } else if g1 <= 0.0 {
            1.0
        } else {
            let (mut lo, mut hi) = (0.0f64, 1.0f64);
            let mut t = 0.5;
            for _ in 0..100 {
                let (g, h) = deriv(t);
                if g == 0.0 {
                    break;
                }
                if g > 0.0 {
                    hi = t;
                } else {
                    lo = t;
                }
                let next = if h > 0.0 { t - g / h } else { f64::NAN };
                let last = t;
                t = if next > lo && next < hi {
                    next
                } else {
                    0.5 * (lo + hi)
                };
                if hi - lo < 1e-12 || (t - last).abs() < 1e-14 {
                    break;
                }
            }
            t
        };
        return (value(t), at(t));
    }
    // Over the root ball: the bound of a distance.
    let len = dot(d, d).sqrt();
    let v0 = value(0.0);
    let mut best = (v0, 0.0);
    let v1 = value(1.0);
    if v1 < best.0 {
        best = (v1, 1.0);
    }
    let vh = value(th);
    if vh < best.0 {
        best = (vh, th);
    }
    let mut bound = f64::INFINITY;
    let mut open = vec![(0.0, th, v0, vh), (th, 1.0, vh, v1)];
    let mut n = 0;
    while let Some((t0, t1, f0, f1)) = open.pop() {
        let low = 0.5 * (f0 + f1) - 0.5 * (t1 - t0) * len;
        if low >= best.0 - 1e-6 || n > 10_000 {
            bound = bound.min(low);
            continue;
        }
        let tm = 0.5 * (t0 + t1);
        let fm = value(tm);
        n += 1;
        if fm < best.0 {
            best = (fm, tm);
        }
        open.push((t0, tm, f0, fm));
        open.push((tm, t1, fm, f1));
    }
    (best.0.min(bound).max(0.0), at(best.1))
}

/// Where a point is relative to a span's axis: its place along it, its
/// distance from the axis line and its direction about it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Polar {
    /// Place along the axis from A₁, m.
    pub along: f64,
    /// Distance from the axis line, m.
    pub rho: f64,
    /// Unit direction about the axis: (down, right) components (0, 0 on
    /// the axis).
    pub dir: [f64; 2],
}

/// One band of a position: how far from the axis and in which directions
/// its centre line lies over a stretch of the axis.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Band {
    /// Largest distance from the axis line, m (negative: nothing here).
    r: f64,
    /// All directions (near the axis, or spread over half a turn or more).
    all: bool,
    /// Directions from angle `a` to `b` (rad from the downward vertical,
    /// positive to the right) …
    a: f64,
    b: f64,
    /// … as unit vectors (down, right).
    lo: [f64; 2],
    hi: [f64; 2],
}

impl Band {
    const EMPTY: Band = Band {
        r: -1.0,
        all: false,
        a: f64::INFINITY,
        b: f64::NEG_INFINITY,
        lo: [1.0, 0.0],
        hi: [1.0, 0.0],
    };

    /// Takes in radius `r` and the directions `a` to `b`.
    fn add(&mut self, r: f64, a: f64, b: f64, all: bool) {
        self.r = self.r.max(r);
        self.a = self.a.min(a);
        self.b = self.b.max(b);
        // Near the axis the direction says little.
        self.all |= all
            || r < 1e-6
            || self.b - self.a >= std::f64::consts::PI
            || self.a < -2.5
            || self.b > 2.5;
        if !self.all {
            self.lo = [self.a.cos(), self.a.sin()];
            self.hi = [self.b.cos(), self.b.sin()];
        }
    }

    /// The distance in the plane square to the axis from the point to the
    /// sector of radius `r` between the directions `lo` and `hi`.
    fn distance(&self, p: &Polar) -> f64 {
        if self.r < 0.0 {
            return f64::INFINITY;
        }
        let rho = p.rho;
        if self.all || rho == 0.0 {
            return (rho - self.r).max(0.0);
        }
        let cross = |a: [f64; 2], b: [f64; 2]| a[0] * b[1] - a[1] * b[0];
        let d = p.dir;
        if cross(self.lo, d) >= 0.0 && cross(d, self.hi) >= 0.0 {
            return (rho - self.r).max(0.0);
        }
        // Outside the directions: to the nearer edge of the sector.
        let edge = |e: [f64; 2]| {
            let c = d[0] * e[0] + d[1] * e[1];
            if c <= 0.0 {
                rho
            } else if rho * c <= self.r {
                rho * cross(e, d).abs()
            } else {
                (rho * rho + self.r * self.r - 2.0 * rho * self.r * c)
                    .max(0.0)
                    .sqrt()
            }
        };
        edge(self.lo).min(edge(self.hi))
    }
}

/// Length of a band along the axis, m.
const BAND: f64 = 20.0;

/// The bands of a position (or of all of a span's positions) along the
/// span's axis, on a grid of 20 m from the axis's start (the first
/// and last bands reach on beyond the ends): a lower bound of the distance
/// from any point to the centre line, from the bands near the point's
/// place — the speed's "one bound per span before the exact distance",
/// made tighter by stretches and directions.
#[derive(Debug, Clone, PartialEq)]
pub struct Bands {
    bands: Vec<Band>,
}

impl Bands {
    /// No band yet, along an axis of `length` m.
    fn empty(length: f64) -> Self {
        let n = ((length.max(0.0) / BAND).ceil() as usize).clamp(1, 512);
        Self {
            bands: vec![Band::EMPTY; n],
        }
    }

    fn band_of(&self, s: f64) -> usize {
        ((s / BAND).floor().max(0.0) as usize).min(self.bands.len() - 1)
    }

    /// The bands of a geometry along an axis of `length` m whose extent at
    /// place s is `at(s)` (largest distance from the axis, and the
    /// directions it covers there: from and to), linear between the
    /// places `breaks` (so that its extremes over a stretch are at those
    /// places or the stretch's ends: a distance from a line is convex
    /// along a segment, a direction seen from it turns one way).
    fn new(length: f64, breaks: &[f64], at: impl Fn(f64) -> (f64, f64, f64)) -> Self {
        let mut out = Self::empty(length);
        let (lo, hi) = breaks
            .iter()
            .filter(|s| s.is_finite())
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &s| {
                (a.min(s), b.max(s))
            });
        if lo > hi {
            return out;
        }
        let n = out.bands.len();
        let mut add = |s: f64, k: usize| {
            let (r, a, b) = at(s);
            out.bands[k].add(r, a, b, false);
        };
        for &s in breaks.iter().filter(|s| s.is_finite()) {
            let k = ((s / BAND).floor().max(0.0) as usize).min(n - 1);
            add(s, k);
        }
        // The bands' inner ends within the geometry's places.
        for k in 1..n {
            let s = k as f64 * BAND;
            if s > lo && s < hi {
                add(s, k - 1);
                add(s, k);
            }
        }
        out
    }

    /// Takes in `other`'s bands (of the same axis).
    pub fn merge(&mut self, other: &Bands) {
        for (a, b) in self.bands.iter_mut().zip(&other.bands) {
            if b.r >= 0.0 {
                a.add(b.r, b.a, b.b, b.all);
            }
        }
    }

    /// A lower bound of the distance from a point at `p` to the centre
    /// line; bands further along the axis than `enough` are not looked at
    /// (the bound is then at least `enough`).
    pub fn lower(&self, p: &Polar, enough: f64) -> f64 {
        let n = self.bands.len();
        let k0 = self.band_of(p.along);
        let gap = |k: usize| {
            let a = if k == 0 {
                f64::NEG_INFINITY
            } else {
                k as f64 * BAND
            };
            let b = if k + 1 == n {
                f64::INFINITY
            } else {
                (k + 1) as f64 * BAND
            };
            if p.along < a {
                a - p.along
            } else if p.along > b {
                p.along - b
            } else {
                0.0
            }
        };
        let mut best = f64::INFINITY;
        let look = |k: usize, best: &mut f64| -> bool {
            let g = gap(k);
            if g >= best.min(enough) {
                return false;
            }
            let d = self.bands[k].distance(p);
            *best = best.min(g.hypot(d));
            true
        };
        look(k0, &mut best);
        let mut k = k0;
        while k + 1 < n && look(k + 1, &mut best) {
            k += 1;
        }
        let mut k = k0;
        while k > 0 && look(k - 1, &mut best) {
            k -= 1;
        }
        best
    }
}

/// What position of a span is checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PositionKind {
    /// The case as it is (still air, or the case's own wind side).
    Case,
    /// Blown to the left (wind from the right).
    Left,
    /// Blown to the right (wind from the left).
    Right,
    /// Anywhere on the swing arc between left and right.
    Arc,
    /// Unbalanced ice on the span, still air (or the case's wind).
    Ice,
    /// Unbalanced ice, blown to the left.
    IceLeft,
    /// Unbalanced ice, blown to the right.
    IceRight,
    /// Still air of a windy case (its wind stepped from 0).
    Still,
    /// Blown to the left at an intermediate wind pressure (its wind
    /// stepped; [`Position::pressure`]).
    StepLeft,
    /// Blown to the right at an intermediate wind pressure.
    StepRight,
}

impl PositionKind {
    /// Its name in reports: `case`, `left`, `right`, `arc`, `ice`, `ice
    /// left`, `ice right`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Case => "case",
            Self::Left => "left",
            Self::Right => "right",
            Self::Arc => "arc",
            Self::Ice => "ice",
            Self::IceLeft => "ice left",
            Self::IceRight => "ice right",
            Self::Still => "still",
            Self::StepLeft => "left (step)",
            Self::StepRight => "right (step)",
        }
    }
}

/// The axis of a span: the line through its attachment points (string
/// tops; at a dead end, the clamp).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Axis {
    /// First attachment A₁.
    pub a1: P3,
    /// Second attachment A₂.
    pub a2: P3,
    /// Unit vector A₁ → A₂.
    pub u: P3,
    /// |A₂ − A₁|, m.
    pub length: f64,
    /// The downward vertical made square to the axis.
    pub down: P3,
    /// Right, looking along the axis: `down × u`.
    pub right: P3,
}

impl Axis {
    /// The axis from `a1` to `a2` (none when they coincide).
    pub fn new(a1: P3, a2: P3) -> Option<Self> {
        let d = sub(a2, a1);
        let length = dot(d, d).sqrt();
        let u = unit(d)?;
        let k = -u[2];
        // A vertical axis: any horizontal "down".
        let down = unit([-k * u[0], -k * u[1], -1.0 - k * u[2]]).unwrap_or([1.0, 0.0, 0.0]);
        Some(Self {
            a1,
            a2,
            u,
            length,
            down,
            right: cross(down, u),
        })
    }

    /// Where `p` is relative to the axis.
    pub fn polar(&self, p: P3) -> Polar {
        let v = sub(p, self.a1);
        let along = dot(v, self.u);
        let w = [
            v[0] - along * self.u[0],
            v[1] - along * self.u[1],
            v[2] - along * self.u[2],
        ];
        let (x, y) = (dot(w, self.down), dot(w, self.right));
        let rho = x.hypot(y);
        let dir = if rho > 0.0 {
            [x / rho, y / rho]
        } else {
            [0.0, 0.0]
        };
        Polar { along, rho, dir }
    }

    /// Distance from the axis line and angle about it (from the downward
    /// vertical, positive to the right) of `p`.
    fn radius_angle(&self, p: P3) -> (f64, f64) {
        let v = sub(p, self.a1);
        let along = dot(v, self.u);
        let w = [
            v[0] - along * self.u[0],
            v[1] - along * self.u[1],
            v[2] - along * self.u[2],
        ];
        let (x, y) = (dot(w, self.down), dot(w, self.right));
        (x.hypot(y), y.atan2(x))
    }

    /// Distance along the axis from A₁ of `p`'s projection, m (not
    /// clamped).
    pub fn along(&self, p: P3) -> f64 {
        dot(sub(p, self.a1), self.u)
    }

    /// Distance from `p` to the segment A₁A₂.
    pub fn distance(&self, p: P3) -> f64 {
        distance(p, closest_on_segment(p, self.a1, self.a2).0)
    }
}

/// A solved wire with the places of its points along its span's axis.
#[derive(Debug, Clone, PartialEq)]
pub struct WireShape {
    /// The wire.
    pub wire: Wire,
    /// Each point's distance along the axis, m.
    along: Vec<f64>,
    /// The places increase along the wire (else every piece is looked at).
    monotone: bool,
    /// Bundle radius, m.
    radius: f64,
    /// [`Wire::allowance`], m.
    allowance: f64,
}

impl WireShape {
    /// `wire` placed along `axis`.
    pub fn new(wire: Wire, axis: &Axis) -> Self {
        let along: Vec<f64> = wire.points.iter().map(|p| axis.along(*p)).collect();
        let monotone = along.windows(2).all(|w| w[1] >= w[0]);
        Self {
            radius: wire.bundle_radius(),
            allowance: wire.allowance(),
            wire,
            along,
            monotone,
        }
    }

    /// Bundle radius and allowance: what is taken off a distance to the
    /// centre line for the distance to the outer surface, m.
    pub fn outer(&self) -> f64 {
        self.radius + self.allowance
    }

    /// The pieces in the order to look at them from place `t` along the
    /// axis, each with a lower bound of the distance to it (its places'
    /// gap from `t`): nearest first while the places increase.
    fn pieces(&self, t: f64) -> impl Iterator<Item = (usize, f64)> + '_ {
        let n = self.wire.points.len().saturating_sub(1);
        let start = if self.monotone && n > 0 {
            self.along.partition_point(|&s| s <= t).clamp(1, n) - 1
        } else {
            0
        };
        let gap = move |k: usize| {
            let (a, b) = (self.along[k], self.along[k + 1]);
            if !self.monotone {
                0.0
            } else if t < a {
                a - t
            } else if t > b {
                t - b
            } else {
                0.0
            }
        };
        // start, start+1, start−1, start+2, start−2, …
        let mut up = start;
        let mut down = start;
        let mut first = true;
        let mut flip = false;
        std::iter::from_fn(move || {
            if n == 0 {
                return None;
            }
            if first {
                first = false;
                return Some((start, gap(start)));
            }
            for _ in 0..2 {
                flip = !flip;
                if flip && up + 1 < n {
                    up += 1;
                    return Some((up, gap(up)));
                }
                if !flip && down > 0 {
                    down -= 1;
                    return Some((down, gap(down)));
                }
            }
            None
        })
    }

    /// The nearest point of its centre line to `p`: (distance, point).
    /// Pieces further along the axis than the best found are not looked
    /// at (the distance is at least the gap of the places).
    pub fn nearest_axis(&self, p: P3, t: f64) -> (f64, P3) {
        let mut best = (f64::INFINITY, p);
        let mut up_done = false;
        let mut down_done = false;
        let start = self.pieces(t).next().map_or(0, |x| x.0);
        for (k, gap) in self.pieces(t) {
            if self.monotone {
                let side_up = k >= start;
                if (side_up && up_done) || (!side_up && down_done) {
                    continue;
                }
                if gap >= best.0 {
                    if side_up {
                        up_done = true;
                    } else {
                        down_done = true;
                    }
                    if up_done && down_done {
                        break;
                    }
                    continue;
                }
            }
            let (a, b) = (self.wire.points[k], self.wire.points[k + 1]);
            let (q, _) = closest_on_segment(p, a, b);
            let d = distance(p, q);
            if d < best.0 {
                best = (d, q);
            }
        }
        best
    }

    /// The smallest distance between its centre line and the horizontal
    /// circle of radius `root` about `base` (a tree's hinge circle; `t`:
    /// the base's place along the axis), and the wire point where it is
    /// found. Pieces whose places are further than the best (or `limit`)
    /// plus `root` are not looked at; a result from `limit` up only says
    /// so. Each piece by [`segment_to_circle`].
    pub fn nearest_circle(&self, base: P3, root: f64, t: f64, limit: f64) -> (f64, P3) {
        let mut best = (f64::INFINITY, base);
        let start = self.pieces(t).next().map_or(0, |x| x.0);
        let (mut up_done, mut down_done) = (false, false);
        for (k, gap) in self.pieces(t) {
            if self.monotone {
                let side_up = k >= start;
                if (side_up && up_done) || (!side_up && down_done) {
                    continue;
                }
                if gap - root >= best.0.min(limit) {
                    if side_up {
                        up_done = true;
                    } else {
                        down_done = true;
                    }
                    if up_done && down_done {
                        break;
                    }
                    continue;
                }
            }
            let (a, b) = (self.wire.points[k], self.wire.points[k + 1]);
            let (d, q) = segment_to_circle(a, b, base, root);
            if d < best.0 {
                best = (d, q);
            }
        }
        best
    }

    /// The distance from `p` to the wire's surface (the nearest
    /// subconductor's, less [`Wire::allowance`]), looked at only where a
    /// piece's centre line minus the bundle radius and the allowance is
    /// within `reach`: (distance, point on the axis of the subconductor);
    /// infinite beyond.
    pub fn nearest_surface(&self, p: P3, t: f64, reach: f64) -> (f64, P3) {
        let (radius, off) = (self.radius, self.allowance);
        let mut best = (f64::INFINITY, p);
        let start = self.pieces(t).next().map_or(0, |x| x.0);
        let (mut up_done, mut down_done) = (false, false);
        for (k, gap) in self.pieces(t) {
            if self.monotone {
                let side_up = k >= start;
                if (side_up && up_done) || (!side_up && down_done) {
                    continue;
                }
                let lower = gap - radius - off;
                if lower > reach || lower >= best.0 {
                    if side_up {
                        up_done = true;
                    } else {
                        down_done = true;
                    }
                    if up_done && down_done {
                        break;
                    }
                    continue;
                }
            }
            let (a, b) = (self.wire.points[k], self.wire.points[k + 1]);
            let (q, _) = closest_on_segment(p, a, b);
            if distance(p, q) - radius - off <= reach {
                let mut d = if radius > 0.0 {
                    self.wire.nearest(p, a, b)
                } else {
                    (distance(p, q), q)
                };
                d.0 -= off;
                if d.0 < best.0 {
                    best = d;
                }
            }
        }
        best
    }

    /// Its bands along `axis`: its points at each of its places and at
    /// the bands' ends (a polyline not running steadily along the axis:
    /// one band of its spread, every direction).
    fn bands(&self, axis: &Axis) -> Bands {
        if !self.monotone {
            let spread = self.spread(axis);
            return Bands::new(axis.length, &self.along, |_| (spread, -4.0, 4.0));
        }
        Bands::new(axis.length, &self.along, |s| {
            let (r, a) = axis.radius_angle(self.at(s));
            (r, a, a)
        })
    }

    /// The largest distance of its points from the axis, m.
    fn spread(&self, axis: &Axis) -> f64 {
        self.wire
            .points
            .iter()
            .map(|p| axis.distance(*p))
            .fold(0.0, f64::max)
    }

    /// Its point at place `s` along the axis (the pieces' places must
    /// increase), the ends beyond.
    fn at(&self, s: f64) -> P3 {
        let n = self.along.len();
        let k = self.along.partition_point(|&a| a <= s);
        if k == 0 {
            return self.wire.points[0];
        }
        if k >= n {
            return self.wire.points[n - 1];
        }
        let (a, b) = (self.along[k - 1], self.along[k]);
        let t = if b > a { (s - a) / (b - a) } else { 0.0 };
        lerp(self.wire.points[k - 1], self.wire.points[k], t)
    }
}

/// The swing arc of a span: the span swinging as one about its axis
/// between its left and right blowout, through its still-air position,
/// at the largest of the three radii at each place.
#[derive(Debug, Clone, PartialEq)]
pub struct SwingArc {
    /// The span's axis.
    pub axis: Axis,
    /// The downward vertical made square to the axis.
    down: P3,
    /// Right, looking along the axis: `down × u`.
    right: P3,
    /// Stations: places along the axis (every point of the three wires),
    /// m, increasing.
    pub stations: Vec<f64>,
    /// At each station, the wire blown left, in still air and blown right
    /// (each in the plane square to the axis there).
    pub points: Vec<[P3; 3]>,
    /// Bundle radius and the largest allowance of the three wires, m.
    pub outer: f64,
    /// Bundle radius, m.
    radius: f64,
    /// The largest allowance of the three wires, m.
    allowance: f64,
}

/// Where a point lies relative to an arc, at its place along the axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArcPlace {
    /// Centre C(s) on the axis.
    pub centre: P3,
    /// Arc radius R = max(R_L, R_0, R_R), m.
    pub radius: f64,
    /// Arc ends' angles (from the downward vertical, positive to the
    /// right), rad: the smallest and the largest of φ_L, φ_0, φ_R.
    pub from: f64,
    /// See `from`.
    pub to: f64,
}

impl SwingArc {
    /// The arc of `left`, `still` and `right` along `axis` (none when one
    /// of them does not run steadily along the axis).
    pub fn new(axis: Axis, left: &WireShape, still: &WireShape, right: &WireShape) -> Option<Self> {
        if !(left.monotone && still.monotone && right.monotone) {
            return None;
        }
        let down = unit({
            let g = [0.0, 0.0, -1.0];
            let k = dot(g, axis.u);
            [
                g[0] - k * axis.u[0],
                g[1] - k * axis.u[1],
                g[2] - k * axis.u[2],
            ]
        })?;
        let right_dir = cross(down, axis.u);
        let mut stations: Vec<f64> = left
            .along
            .iter()
            .chain(&still.along)
            .chain(&right.along)
            .copied()
            .filter(|s| s.is_finite())
            .collect();
        stations.sort_by(f64::total_cmp);
        stations.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        if stations.len() < 2 {
            return None;
        }
        let points = stations
            .iter()
            .map(|&s| [left.at(s), still.at(s), right.at(s)])
            .collect();
        Some(Self {
            axis,
            down,
            right: right_dir,
            stations,
            points,
            outer: left.radius + left.allowance.max(still.allowance).max(right.allowance),
            radius: left.radius,
            allowance: left.allowance.max(still.allowance).max(right.allowance),
        })
    }

    /// The arc at place `s` along the axis (clamped to the stations).
    pub fn place(&self, s: f64) -> ArcPlace {
        let n = self.stations.len();
        let s = s.clamp(self.stations[0], self.stations[n - 1]);
        let k = self.stations.partition_point(|&a| a <= s).clamp(1, n - 1);
        let (a, b) = (self.stations[k - 1], self.stations[k]);
        let t = if b > a { (s - a) / (b - a) } else { 0.0 };
        let centre = [
            self.axis.a1[0] + s * self.axis.u[0],
            self.axis.a1[1] + s * self.axis.u[1],
            self.axis.a1[2] + s * self.axis.u[2],
        ];
        let (mut radius, mut from, mut to) = (0.0f64, f64::INFINITY, f64::NEG_INFINITY);
        for i in 0..3 {
            let p = lerp(self.points[k - 1][i], self.points[k][i], t);
            let v = sub(p, centre);
            let (r, phi) = self.polar(v);
            radius = radius.max(r);
            from = from.min(phi);
            to = to.max(phi);
        }
        ArcPlace {
            centre,
            radius,
            from,
            to,
        }
    }

    /// (radius, angle) of `v` in the plane square to the axis.
    fn polar(&self, v: P3) -> (f64, f64) {
        let k = dot(v, self.axis.u);
        let w = [
            v[0] - k * self.axis.u[0],
            v[1] - k * self.axis.u[1],
            v[2] - k * self.axis.u[2],
        ];
        (
            dot(w, w).sqrt(),
            dot(w, self.right).atan2(dot(w, self.down)),
        )
    }

    /// The arc's point at angle `phi`, radius `r` about `centre`.
    pub fn point(&self, centre: P3, r: f64, phi: f64) -> P3 {
        let (c, s) = (phi.cos(), phi.sin());
        [
            centre[0] + r * (c * self.down[0] + s * self.right[0]),
            centre[1] + r * (c * self.down[1] + s * self.right[1]),
            centre[2] + r * (c * self.down[2] + s * self.right[2]),
        ]
    }

    /// The distance from `q` to the arc (to the wire's centre line on
    /// it): `q` projected on the axis at s_q (clamped to the stations),
    /// at radius ρ_q and angle φ_q in the plane there; within the arc's
    /// angles |ρ_q − R| (and the part along the axis beyond its ends),
    /// else the nearer of its ends. (distance, the arc's point).
    pub fn nearest(&self, q: P3) -> (f64, P3) {
        let place = self.place(self.axis.along(q));
        let v = sub(q, place.centre);
        let along = dot(v, self.axis.u);
        let (rho, phi) = self.polar(v);
        if place.from <= phi && phi <= place.to {
            let p = self.point(place.centre, place.radius, phi);
            ((rho - place.radius).hypot(along), p)
        } else {
            let a = self.point(place.centre, place.radius, place.from);
            let b = self.point(place.centre, place.radius, place.to);
            let (da, db) = (distance(q, a), distance(q, b));
            if da <= db { (da, a) } else { (db, b) }
        }
    }

    /// Its bands along its axis: the arc's radius and angles at each
    /// station and at the bands' ends.
    fn bands(&self) -> Bands {
        Bands::new(self.axis.length, &self.stations, |s| {
            let p = self.place(s);
            (p.radius, p.from, p.to)
        })
    }

    /// The largest distance of the arc from its axis, m.
    fn spread(&self) -> f64 {
        self.stations
            .iter()
            .map(|&s| self.place(s).radius)
            .fold(0.0, f64::max)
    }
}

/// The geometry of a position.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// A solved wire.
    Wire(WireShape),
    /// A swing arc.
    Arc(SwingArc),
}

impl Shape {
    /// What is taken off a centre-line distance for the outer surface, m.
    pub fn outer(&self) -> f64 {
        match self {
            Shape::Wire(w) => w.outer(),
            Shape::Arc(a) => a.outer,
        }
    }

    /// A centre-line distance `d` made a distance to the outer surface:
    /// the bundle radius, then the allowance, taken off.
    pub fn off_surface(&self, d: f64) -> f64 {
        match self {
            Shape::Wire(w) => d - w.radius - w.allowance,
            Shape::Arc(a) => d - a.radius - a.allowance,
        }
    }

    /// The nearest point of the centre line (on the arc: of the arc) to
    /// `p` at place `t` along the axis: (distance, point).
    pub fn nearest_axis(&self, p: P3, t: f64) -> (f64, P3) {
        match self {
            Shape::Wire(w) => w.nearest_axis(p, t),
            Shape::Arc(a) => a.nearest(p),
        }
    }

    /// The smallest distance between its centre line (an arc: the arc)
    /// and the hinge circle of radius `root` about `base` (`t`: the base's
    /// place along the axis), with the point of the position where it is
    /// found. A wire: exact ([`WireShape::nearest_circle`]; from `limit`
    /// up a bound only); an arc: within [`crate::fall::CIRCLE_TOLERANCE`]
    /// below ([`crate::fall::min_over_circle`]).
    pub fn nearest_circle(&self, base: P3, root: f64, t: f64, limit: f64) -> (f64, P3) {
        if root <= 0.0 {
            return self.nearest_axis(base, t);
        }
        match self {
            Shape::Wire(w) => w.nearest_circle(base, root, t, limit),
            // To its tolerance whatever the limit: the same bound for the
            // same tree however the points are shared out.
            Shape::Arc(a) => {
                let _ = limit;
                crate::fall::min_over_circle(base, root, f64::INFINITY, |c| a.nearest(c))
            }
        }
    }

    /// The distance to the surface within `reach` (infinite beyond):
    /// a wire's nearest subconductor less its allowance; an arc's centre
    /// line less the bundle radius and the allowance.
    pub fn nearest_surface(&self, p: P3, t: f64, reach: f64) -> (f64, P3) {
        match self {
            Shape::Wire(w) => w.nearest_surface(p, t, reach),
            Shape::Arc(a) => {
                let (d, q) = a.nearest(p);
                let d = d - a.outer;
                if d <= reach {
                    (d, q)
                } else {
                    (f64::INFINITY, p)
                }
            }
        }
    }
}

/// One position of one span in one case.
#[derive(Debug, Clone, PartialEq)]
pub struct Position {
    /// The span ([`Positions::spans`]).
    pub span: usize,
    /// The weather case ([`Positions::cases`]).
    pub case: usize,
    /// What position.
    pub kind: PositionKind,
    /// Its geometry.
    pub shape: Shape,
    /// The largest distance of its centre line from the span's axis, m.
    pub spread: f64,
    /// Its bands along the axis (lower bounds of distances to it).
    pub bands: Bands,
    /// The wind pressure it was solved at when not the case's own (a
    /// step of a stepped wind), Pa.
    pub pressure: Option<f64>,
}

/// A span of a section, with every position checked.
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    /// The span's wire as first solved (section, span, circuit, phase,
    /// bundle…).
    pub wire: Wire,
    /// Its axis.
    pub axis: Axis,
    /// Its positions ([`Positions::positions`]).
    pub positions: Vec<usize>,
    /// Every position lies within this of the axis (with its outer
    /// surface), m: the capsule of the span.
    pub capsule: f64,
    /// The bands of all its positions together.
    pub bands: Bands,
    /// The largest outer radius of its positions, m.
    pub outer: f64,
}

/// The spans and positions of a run.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Positions {
    /// The weather cases, by name.
    pub cases: Vec<String>,
    /// The spans.
    pub spans: Vec<Span>,
    /// The positions.
    pub positions: Vec<Position>,
    /// What could not be done (sections not solved, arcs left out…).
    pub notes: Vec<String>,
}

impl Positions {
    /// One position per wire, the case as it is: the vegetation check of
    /// one solved case (the axis from the wire's ends).
    pub fn of_wires(case: &str, wires: Vec<Wire>) -> Self {
        let mut out = Self {
            cases: vec![case.to_owned()],
            ..Self::default()
        };
        for wire in wires {
            let (Some(a), Some(b)) = (wire.points.first(), wire.points.last()) else {
                continue;
            };
            let axis = Axis::new(*a, *b).unwrap_or(Axis {
                a1: *a,
                a2: *b,
                u: [1.0, 0.0, 0.0],
                length: 0.0,
                down: [0.0, 0.0, -1.0],
                right: [0.0, -1.0, 0.0],
            });
            out.add_span(wire.clone(), axis);
            let n = out.spans.len() - 1;
            out.add(
                n,
                0,
                PositionKind::Case,
                Shape::Wire(WireShape::new(wire, &axis)),
            );
        }
        out
    }

    /// Adds a span; returns its index.
    pub fn add_span(&mut self, wire: Wire, axis: Axis) -> usize {
        self.spans.push(Span {
            wire,
            bands: Bands::empty(axis.length),
            axis,
            positions: Vec::new(),
            capsule: 0.0,
            outer: 0.0,
        });
        self.spans.len() - 1
    }

    /// Adds a position of span `span`.
    pub fn add(&mut self, span: usize, case: usize, kind: PositionKind, shape: Shape) {
        self.add_at(span, case, kind, shape, None);
    }

    /// Adds a position of span `span` solved at wind pressure `pressure`
    /// (Pa; none: the case's own).
    pub fn add_at(
        &mut self,
        span: usize,
        case: usize,
        kind: PositionKind,
        shape: Shape,
        pressure: Option<f64>,
    ) {
        let axis = self.spans[span].axis;
        let spread = match &shape {
            Shape::Wire(w) => w.spread(&axis),
            Shape::Arc(a) => a.spread(),
        };
        let bands = match &shape {
            Shape::Wire(w) => w.bands(&axis),
            Shape::Arc(a) => a.bands(),
        };
        let s = &mut self.spans[span];
        s.capsule = s.capsule.max(spread + shape.outer());
        s.outer = s.outer.max(shape.outer());
        s.bands.merge(&bands);
        s.positions.push(self.positions.len());
        self.positions.push(Position {
            span,
            case,
            kind,
            shape,
            spread,
            bands,
            pressure,
        });
    }

    /// The span of `section`, span index `span`.
    pub fn span_of(&self, section: &str, span: usize) -> Option<usize> {
        self.spans
            .iter()
            .position(|s| s.wire.section == section && s.wire.span == span)
    }

    /// "case, position" of position `k`.
    pub fn describe(&self, k: usize) -> String {
        let p = &self.positions[k];
        match p.pressure {
            Some(q) => format!("{}, {} at {q:.0} Pa", self.cases[p.case], p.kind.name()),
            None => format!("{}, {}", self.cases[p.case], p.kind.name()),
        }
    }
}

/// What positions to solve beyond each case as it is.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PositionOptions {
    /// The swing arc of each span of a windy case.
    pub swing_arc: bool,
    /// The wind of each windy case stepped ([`crate::rules::WindSteps`],
    /// as the clearance windows step it): still air and every
    /// intermediate pressure from 0 up to the case's, from the sides it
    /// names (the case's full pressure is checked from both sides in any
    /// case). Alongside the swing arc or instead of it.
    pub wind_steps: Option<crate::rules::WindSteps>,
}

/// The attachment of section point `i` of `shape` a span swings about:
/// at a point inside the section, the top of its one string hanging below
/// it; else (the section's ends, a V-string, a wire fixed to the
/// structure) its clamp.
fn attachment(shape: &SectionShape, i: usize) -> P3 {
    let clamp = shape.clamps[i];
    let last = shape.clamps.len().saturating_sub(1);
    if i == 0 || i >= last {
        return clamp;
    }
    match shape.strings.get(i).map(Vec::as_slice) {
        // A hanging string, however far a wind swings it.
        Some([s]) if s.bottom[2] < s.top[2] => s.top,
        _ => clamp,
    }
}

/// The axis of span `k` of `shape`.
fn span_axis(shape: &SectionShape, k: usize) -> Option<Axis> {
    Axis::new(attachment(shape, k), attachment(shape, k + 1))
}

/// The case with its wind from `side`.
fn blown(case: &WeatherCase, side: WindSide) -> WeatherCase {
    WeatherCase {
        wind_from: side,
        ..case.clone()
    }
}

/// The case without wind.
fn still(case: &WeatherCase) -> WeatherCase {
    WeatherCase {
        wind_pressure: ohl_core::units::Stress::from_si(0.0),
        ..case.clone()
    }
}

/// Whether `case` has wind.
pub fn is_windy(case: &WeatherCase) -> bool {
    case.wind_pressure.si() > 0.0
}

/// Runs `jobs` on every core: `run(k)` for each k < `jobs`, stopping at
/// the first error or when `budget` runs out; results in order.
pub(crate) fn parallel<T: Send>(
    jobs: usize,
    budget: &Budget,
    run: impl Fn(usize) -> Result<T, String> + Sync,
) -> Result<Vec<T>, String> {
    let next = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let error: Mutex<Option<String>> = Mutex::new(None);
    let results: Mutex<Vec<Option<T>>> = Mutex::new((0..jobs).map(|_| None).collect());
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(jobs)
        .max(1);
    let clock = budget.start();
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                loop {
                    if stop.load(Ordering::Relaxed) {
                        return;
                    }
                    if let Err(e) = clock.check() {
                        stop.store(true, Ordering::Relaxed);
                        *error.lock().expect("not poisoned") = Some(e.to_string());
                        return;
                    }
                    let k = next.fetch_add(1, Ordering::Relaxed);
                    if k >= jobs {
                        return;
                    }
                    match run(k) {
                        Ok(v) => results.lock().expect("not poisoned")[k] = Some(v),
                        Err(e) => {
                            stop.store(true, Ordering::Relaxed);
                            *error.lock().expect("not poisoned") = Some(e);
                            return;
                        }
                    }
                }
            });
        }
    });
    if let Some(e) = error.into_inner().expect("not poisoned") {
        return Err(e);
    }
    Ok(results
        .into_inner()
        .expect("not poisoned")
        .into_iter()
        .map(|r| r.expect("every job ran"))
        .collect())
}

/// Solves every case of `cases` — a still case as it is, a windy one from
/// the left and from the right (and in still air for the swing arc; with
/// the wind stepped, in still air and at every intermediate pressure from
/// the sides chosen) — in parallel, and gathers the positions of every
/// span. Sections that do not solve are left out (noted).
///
/// Cost: one line solve per case and side, each typically well under a
/// second; exit clause: `budget` (its time limit, checked between
/// solves).
pub fn solve_positions(
    line: &Line,
    library: &ohl_cables::Library,
    cases: &[WeatherCase],
    options: PositionOptions,
    budget: &Budget,
) -> Result<Positions, String> {
    // Jobs: (case, kind, the case solved, its pressure when stepped).
    let mut jobs: Vec<(usize, PositionKind, WeatherCase, Option<f64>)> = Vec::new();
    for (c, case) in cases.iter().enumerate() {
        if is_windy(case) {
            jobs.push((c, PositionKind::Left, blown(case, WindSide::Right), None));
            jobs.push((c, PositionKind::Right, blown(case, WindSide::Left), None));
            if let Some(steps) = &options.wind_steps {
                // Still air (checked, and the arc's middle) and every
                // pressure between 0 and the full one.
                jobs.push((c, PositionKind::Still, still(case), Some(0.0)));
                let full = case.wind_pressure.si();
                for p in steps.pressures(full, crate::zone::MAX_STEPS) {
                    if p <= 0.0 || p >= full {
                        continue;
                    }
                    for &side in steps.sides.sides() {
                        let kind = match side {
                            WindSide::Right => PositionKind::StepLeft,
                            WindSide::Left => PositionKind::StepRight,
                        };
                        let mut at = blown(case, side);
                        at.wind_pressure = ohl_core::units::Stress::from_si(p);
                        jobs.push((c, kind, at, Some(p)));
                    }
                }
            } else if options.swing_arc {
                // Still air: for the arc only.
                jobs.push((c, PositionKind::Case, still(case), None));
            }
        } else {
            jobs.push((c, PositionKind::Case, case.clone(), None));
        }
    }
    let fe = ohl_wirefe::default_fe_budget();
    let solved = parallel(jobs.len(), budget, |k| {
        let (_, _, case, _) = &jobs[k];
        Ok(ohl_line::solve::solve_line_with(line, library, case, &fe))
    })?;
    let mut out = Positions {
        cases: cases.iter().map(|c| c.name.clone()).collect(),
        ..Positions::default()
    };
    // Per case: its solved shapes by kind (and pressure).
    type Solved = (PositionKind, Option<f64>, Vec<SectionShape>);
    let mut by_case: Vec<Vec<Solved>> = vec![Vec::new(); cases.len()];
    for ((c, kind, case, pressure), results) in jobs.iter().zip(solved) {
        let mut shapes = Vec::new();
        for (section, r) in line.sections.iter().zip(results) {
            match r {
                Ok(s) => shapes.push(s),
                Err(e) => out.notes.push(format!(
                    "case \"{}\" ({}): section {} not solved: {e}",
                    case.name,
                    kind.name(),
                    section.id
                )),
            }
        }
        by_case[*c].push((*kind, *pressure, shapes));
    }
    for (c, variants) in by_case.iter().enumerate() {
        let windy = is_windy(&cases[c]);
        // Still air of a windy case: for the arc, not checked itself.
        type Kept<'a> = (PositionKind, Vec<Wire>, &'a Vec<SectionShape>, Option<f64>);
        let mut kept: Vec<Kept> = Vec::new();
        for (kind, pressure, shapes) in variants {
            kept.push((*kind, wires_of(line, shapes, &[]), shapes, *pressure));
        }
        let mut span_ids: Vec<Vec<Option<usize>>> = Vec::new();
        for (kind, wires, shapes, pressure) in &kept {
            let mut ids = Vec::new();
            for w in wires {
                let id = match out.span_of(&w.section, w.span) {
                    Some(id) => Some(id),
                    None => shapes
                        .iter()
                        .find(|s| s.section == w.section)
                        .and_then(|s| span_axis(s, w.span))
                        .map(|axis| out.add_span(w.clone(), axis)),
                };
                ids.push(id);
                let Some(id) = id else { continue };
                if windy && *kind == PositionKind::Case {
                    continue;
                }
                let axis = out.spans[id].axis;
                out.add_at(
                    id,
                    c,
                    *kind,
                    Shape::Wire(WireShape::new(w.clone(), &axis)),
                    *pressure,
                );
            }
            span_ids.push(ids);
        }
        if !(windy && options.swing_arc) {
            continue;
        }
        let find = |k: PositionKind| kept.iter().position(|(kind, ..)| *kind == k);
        let (Some(l), Some(s), Some(r)) = (
            find(PositionKind::Left),
            find(PositionKind::Case).or_else(|| find(PositionKind::Still)),
            find(PositionKind::Right),
        ) else {
            continue;
        };
        let mut left_out = 0;
        for (w, id) in kept[s].1.iter().zip(&span_ids[s]) {
            let Some(id) = *id else { continue };
            let pick = |k: usize| {
                kept[k]
                    .1
                    .iter()
                    .find(|x| x.section == w.section && x.span == w.span)
            };
            let (Some(wl), Some(wr)) = (pick(l), pick(r)) else {
                left_out += 1;
                continue;
            };
            let axis = out.spans[id].axis;
            let shape = |x: &Wire| WireShape::new(x.clone(), &axis);
            match SwingArc::new(axis, &shape(wl), &shape(w), &shape(wr)) {
                Some(arc) => out.add(id, c, PositionKind::Arc, Shape::Arc(arc)),
                None => left_out += 1,
            }
        }
        if left_out > 0 {
            out.notes.push(format!(
                "case \"{}\": {left_out} spans without a swing arc (a wire not solved, or not running steadily along its span)",
                cases[c].name
            ));
        }
    }
    Ok(out)
}

/// Unbalanced ice on a span: what the span checked carries more than its
/// neighbours. The span is solved in a weather case of its own, the
/// section's other spans in the section's
/// ([`ohl_line::solve::SpanCases`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnbalancedIce {
    /// The case's ice on the span alone: the span in the case, the
    /// section's other spans in the case without its ice (cases without
    /// ice are left out).
    CaseIce,
    /// The case on every span, and the span in the case with this much
    /// more radial ice, m ([`WeatherCase::with_more_ice_thickness`]).
    ExtraThickness(f64),
    /// The case on every span, and the span in the case with this much
    /// more ice load per metre on each wire (each subconductor, as a
    /// weather case's ice load), N/m
    /// ([`WeatherCase::with_more_ice_load`]).
    ExtraLoad(f64),
    /// The case as it is, and this concentrated load at mid-span, N (the
    /// whole bundle's).
    Concentrated(f64),
}

impl UnbalancedIce {
    /// The section's case and the span's for `case` on a wire of bare
    /// diameter `diameter` (none: the case is left out).
    pub fn cases(
        self,
        case: &WeatherCase,
        diameter: ohl_core::units::Length,
    ) -> Option<(WeatherCase, WeatherCase)> {
        use ohl_core::units::{ForcePerLength, Length};
        match self {
            UnbalancedIce::CaseIce => case.has_ice().then(|| (case.without_ice(), case.clone())),
            UnbalancedIce::ExtraThickness(t) => Some((
                case.clone(),
                case.with_more_ice_thickness(Length::from_si(t), diameter),
            )),
            UnbalancedIce::ExtraLoad(q) => Some((
                case.clone(),
                case.with_more_ice_load(ForcePerLength::from_si(q), diameter),
            )),
            UnbalancedIce::Concentrated(_) => Some((case.clone(), case.clone())),
        }
    }
}

/// Adds the unbalanced-ice positions of spans `spans` (indices of
/// `positions.spans`) in every case: each span solved with its section on
/// its own, the span in its own weather case and the other spans in the
/// section's ([`UnbalancedIce::cases`]; a concentrated load added as a
/// point load at mid-span), in parallel, per span and case. A windy case
/// is solved from the left and from the right. Returns the number of
/// solves.
///
/// The span's case is a full weather case: its ice weight, the wind on
/// its iced diameter, its surface radius the iced one.
///
/// Exit clause: `budget` (its time limit, between solves).
pub fn add_unbalanced_ice(
    positions: &mut Positions,
    line: &Line,
    library: &ohl_cables::Library,
    cases: &[WeatherCase],
    mode: UnbalancedIce,
    spans: &[usize],
    budget: &Budget,
) -> Result<usize, String> {
    use ohl_line::legacy::SpanLoad;
    use ohl_line::solve::SpanCases;
    // Jobs: (span, case, kind, the case solved).
    let mut jobs: Vec<(usize, usize, PositionKind, WeatherCase)> = Vec::new();
    for &id in spans {
        for (c, case) in cases.iter().enumerate() {
            if mode == UnbalancedIce::CaseIce && !case.has_ice() {
                continue;
            }
            if is_windy(case) {
                jobs.push((id, c, PositionKind::IceLeft, blown(case, WindSide::Right)));
                jobs.push((id, c, PositionKind::IceRight, blown(case, WindSide::Left)));
            } else {
                jobs.push((id, c, PositionKind::Ice, case.clone()));
            }
        }
    }
    let fe = ohl_wirefe::default_fe_budget();
    let solved = parallel(jobs.len(), budget, |k| {
        let (id, _, _, case) = &jobs[k];
        let span = &positions.spans[*id];
        let Some(section) = line.sections.iter().find(|s| s.id == span.wire.section) else {
            return Ok(None);
        };
        let Some(cable) = library.get(&section.cable) else {
            return Ok(None);
        };
        let Some((base, own)) = mode.cases(case, cable.diameter) else {
            return Ok(None);
        };
        let mut section = section.clone();
        if let UnbalancedIce::Concentrated(force) = mode {
            section.add_point_load(&SpanLoad {
                span: span.wire.span,
                fraction: 0.5,
                vertical: force,
                label: "unbalanced ice".into(),
                ..SpanLoad::default()
            });
        }
        let span_cases = SpanCases::uniform(base).with_span(span.wire.span, own);
        let shape =
            ohl_line::solve::solve_section_span_cases(line, &section, cable, &span_cases, &fe)
                .map_err(|e| e.to_string());
        Ok(Some(shape))
    })?;
    let mut count = 0;
    let mut failed = Vec::new();
    for ((id, c, kind, _), r) in jobs.iter().zip(solved) {
        let span = &positions.spans[*id];
        let shape = match r {
            Some(Ok(shape)) => shape,
            Some(Err(e)) => {
                failed.push(e);
                continue;
            }
            None => {
                failed.push(format!("section {} not solved", span.wire.section));
                continue;
            }
        };
        count += 1;
        let Some(s) = shape.spans.get(span.wire.span) else {
            failed.push(format!(
                "section {}: no span {}",
                span.wire.section, span.wire.span
            ));
            continue;
        };
        let wire = Wire {
            points: s.points.clone(),
            surface_radius: s.surface_radius,
            polyline_error: s.polyline_error,
            ..span.wire.clone()
        };
        let axis = span.axis;
        positions.add(*id, *c, *kind, Shape::Wire(WireShape::new(wire, &axis)));
    }
    if !failed.is_empty() {
        failed.sort();
        failed.dedup();
        positions.notes.push(format!(
            "unbalanced ice: {} solves left out ({})",
            jobs.len() - count,
            failed.join("; ")
        ));
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(points: Vec<P3>) -> Wire {
        Wire {
            section: "S".into(),
            span: 0,
            circuit: "c".into(),
            phase: "L1".into(),
            potential: None,
            bundle: vec![],
            points,
            ends: vec![],
            surface_radius: 0.0,
            polyline_error: 0.0,
        }
    }

    /// A level 100 m span along x hanging from attachments at z = 20: a
    /// parabola of sag 4 m, blown sideways by angle `phi` (positive to the
    /// right, -y) about the axis, the sag `sag`.
    fn hanging(phi: f64, sag: f64) -> Vec<P3> {
        (0..=20)
            .map(|k| {
                let x = k as f64 * 5.0;
                let r = 4.0 * sag * (x / 100.0) * (1.0 - x / 100.0);
                [x, -r * phi.sin(), 20.0 - r * phi.cos()]
            })
            .collect()
    }

    fn axis() -> Axis {
        Axis::new([0.0, 0.0, 20.0], [100.0, 0.0, 20.0]).unwrap()
    }

    #[test]
    fn the_nearest_piece_is_found_from_the_place_along_the_span() {
        let ax = axis();
        let w = WireShape::new(wire(hanging(0.0, 4.0)), &ax);
        // Every point near or far: the same as looking at every piece.
        for q in [
            [50.0, 3.0, 12.0],
            [-20.0, 0.0, 0.0],
            [130.0, 40.0, 50.0],
            [7.0, -2.0, 19.0],
            [93.0, 1.0, 17.0],
        ] {
            let brute = w
                .wire
                .points
                .windows(2)
                .map(|s| distance(q, closest_on_segment(q, s[0], s[1]).0))
                .fold(f64::INFINITY, f64::min);
            let (d, _) = w.nearest_axis(q, ax.along(q));
            assert_eq!(d, brute, "{q:?}");
            let (ds, _) = w.nearest_surface(q, ax.along(q), f64::INFINITY);
            assert_eq!(ds, brute, "{q:?}");
        }
    }

    /// Hand-built span: still sag 4 m, blown ±40° with sag 5 m. At
    /// mid-span (s = 50 m) the arc has R = 5, from −40° to +40°. A point
    /// straight below the axis 8 m down is at φ = 0, ρ = 8: 3 m from the
    /// arc. A point 6 m out to the right at the axis height (φ = 90°)
    /// is beyond the arc's end: distance to the end (5 sin 40°, 5 cos 40°
    /// below) = √((6 − 3.214)² + 3.830²) = 4.736.
    #[test]
    fn the_swing_arc_by_hand() {
        let ax = axis();
        let phi = 40f64.to_radians();
        let left = WireShape::new(wire(hanging(-phi, 5.0)), &ax);
        let still = WireShape::new(wire(hanging(0.0, 4.0)), &ax);
        let right = WireShape::new(wire(hanging(phi, 5.0)), &ax);
        let arc = SwingArc::new(ax, &left, &still, &right).unwrap();
        let place = arc.place(50.0);
        assert!((place.radius - 5.0).abs() < 1e-12);
        assert!((place.from + phi).abs() < 1e-12 && (place.to - phi).abs() < 1e-12);
        let (d, p) = arc.nearest([50.0, 0.0, 12.0]);
        assert!((d - 3.0).abs() < 1e-12, "{d}");
        assert!(distance(p, [50.0, 0.0, 15.0]) < 1e-12);
        // To the right is −y.
        let (d, _) = arc.nearest([50.0, -6.0, 20.0]);
        let end = [5.0 * phi.sin(), 5.0 * phi.cos()];
        assert!((d - (6.0 - end[0]).hypot(end[1])).abs() < 1e-12, "{d}");
        assert!((d - 4.736).abs() < 1e-3);
        // Inside the arc at 25 m: R = 4·5·0.25·0.75 = 3.75, a point 1 m
        // below the axis at φ = 20° is 2.75 m from it.
        let a = 20f64.to_radians();
        let q = [25.0, -a.sin(), 20.0 - a.cos()];
        let (d, _) = arc.nearest(q);
        assert!((d - 2.75).abs() < 1e-12, "{d}");
        // Beyond the span's end: the end's plane, the part along the axis
        // counted.
        let (d, _) = arc.nearest([-3.0, 0.0, 16.0]);
        assert!((d - 5.0).abs() < 1e-12, "{d}");
    }

    /// The bands never claim more than the exact distance: random points
    /// around a blown wire and a swing arc.
    #[test]
    fn bands_are_lower_bounds() {
        let ax = axis();
        let phi = 35f64.to_radians();
        let left = WireShape::new(wire(hanging(-phi, 5.0)), &ax);
        let still = WireShape::new(wire(hanging(0.0, 4.0)), &ax);
        let right = WireShape::new(wire(hanging(phi, 5.0)), &ax);
        let arc = SwingArc::new(ax, &left, &still, &right).unwrap();
        let shapes = [
            (Shape::Wire(left.clone()), left.bands(&ax)),
            (Shape::Wire(right.clone()), right.bands(&ax)),
            (Shape::Arc(arc.clone()), arc.bands()),
        ];
        let mut s = 7u64;
        let mut rnd = || {
            s = s
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (s >> 11) as f64 / (1u64 << 53) as f64
        };
        for _ in 0..20_000 {
            let q = [rnd() * 140.0 - 20.0, rnd() * 40.0 - 20.0, rnd() * 40.0];
            let p = ax.polar(q);
            for (shape, bands) in &shapes {
                let (d, _) = shape.nearest_axis(q, p.along);
                let lb = bands.lower(&p, f64::INFINITY);
                assert!(lb <= d + 1e-9, "{q:?}: bound {lb} > {d}");
            }
        }
        // And they are tight where the wire is: a point 2 m right of the
        // right wire at mid-span.
        let q = [
            50.0,
            -5.0 * phi.sin() - 2.0 * phi.cos(),
            20.0 - 5.0 * phi.cos() + 2.0 * phi.sin(),
        ];
        let lb = shapes[1].1.lower(&ax.polar(q), f64::INFINITY);
        assert!(lb > 1.0, "{lb}");
    }

    /// The distance between a segment and a hinge circle against a brute
    /// force over both.
    #[test]
    fn segment_to_circle_against_brute_force() {
        let base = [0.0, 0.0, 0.0];
        for (a, b, r) in [
            ([10.0, -20.0, 15.0], [12.0, 20.0, 14.0], 2.0),
            ([3.0, -5.0, 8.0], [3.0, 5.0, 8.0], 2.5),
            ([1.0, -1.0, 6.0], [-1.0, 1.0, 6.0], 2.0),
            ([30.0, 5.0, 20.0], [36.0, 6.0, 19.0], 3.0),
        ] {
            let (d, q) = segment_to_circle(a, b, base, r);
            let mut brute = f64::INFINITY;
            for i in 0..=2000 {
                let t = i as f64 / 2000.0;
                let p = lerp(a, b, t);
                for j in 0..720 {
                    let psi = j as f64 * std::f64::consts::TAU / 720.0;
                    let c = [r * psi.cos(), r * psi.sin(), 0.0];
                    brute = brute.min(distance(p, c));
                }
            }
            assert!(d <= brute + 1e-9, "{d} {brute}");
            assert!(brute - d < 0.01, "{d} {brute}");
            let rho = q[0].hypot(q[1]);
            assert!(((rho - r).hypot(q[2]) - d).abs() < 1e-6 || d == 0.0);
        }
    }

    /// The example line: a windy case solved left and right with its
    /// swing arcs; the glaze-ice case with its ice on one span alone sags
    /// that span further than the ice on every span (the strings swing
    /// into it), and an extra load further still than the case.
    #[test]
    fn positions_of_the_example_line() {
        use std::path::Path;
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let line =
            Line::load(&root.join("examples/synthetic-line/synthetic-line.ohl.json")).unwrap();
        let library = ohl_cables::Library::load(&root.join("libraries/cables")).unwrap();
        let cases = ohl_weather::WeatherCases::load(
            &root.join("examples/synthetic-line/weather-cases.ohlwc"),
        )
        .unwrap()
        .select(&["high wind".into(), "glaze ice".into()], &[])
        .unwrap();
        let budget = Budget::new();
        let mut p = solve_positions(
            &line,
            &library,
            &cases,
            PositionOptions {
                swing_arc: true,
                ..PositionOptions::default()
            },
            &budget,
        )
        .unwrap();
        let n = p.spans.len();
        let count =
            |p: &Positions, k: PositionKind| p.positions.iter().filter(|x| x.kind == k).count();
        assert_eq!(count(&p, PositionKind::Left), n);
        assert_eq!(count(&p, PositionKind::Right), n);
        assert_eq!(count(&p, PositionKind::Arc), n);
        assert_eq!(count(&p, PositionKind::Case), n);
        // Left is blown to the left: its points on the left of the
        // span's axis.
        let s = p.span_of("S1", 1).unwrap();
        let low = |p: &Positions, k: PositionKind| {
            p.spans[s]
                .positions
                .iter()
                .map(|&i| &p.positions[i])
                .filter(|x| x.kind == k)
                .map(|x| match &x.shape {
                    Shape::Wire(w) => w
                        .wire
                        .points
                        .iter()
                        .map(|q| q[2])
                        .fold(f64::INFINITY, f64::min),
                    Shape::Arc(_) => f64::NAN,
                })
                .next()
                .unwrap()
        };
        let axis = p.spans[s].axis;
        let mid = |k: PositionKind| {
            p.spans[s]
                .positions
                .iter()
                .map(|&i| &p.positions[i])
                .find(|x| x.kind == k)
                .and_then(|x| match &x.shape {
                    Shape::Wire(w) => Some(w.at(0.5 * axis.length)),
                    Shape::Arc(_) => None,
                })
                .unwrap()
        };
        let side = |q: P3| dot(sub(q, axis.a1), axis.right);
        assert!(side(mid(PositionKind::Left)) < -0.5 && side(mid(PositionKind::Right)) > 0.5);
        let iced = low(&p, PositionKind::Case);
        add_unbalanced_ice(
            &mut p,
            &line,
            &library,
            &cases,
            UnbalancedIce::CaseIce,
            &[s],
            &budget,
        )
        .unwrap();
        assert!(p.notes.is_empty(), "{:?}", p.notes);
        let unbalanced = low(&p, PositionKind::Ice);
        assert!(unbalanced < iced - 0.01, "{unbalanced} {iced}");
        let glaze: Vec<WeatherCase> = cases
            .iter()
            .filter(|c| c.name == "glaze ice")
            .cloned()
            .collect();
        let mut q =
            solve_positions(&line, &library, &glaze, PositionOptions::default(), &budget).unwrap();
        assert_eq!(q.span_of("S1", 1), Some(s));
        add_unbalanced_ice(
            &mut q,
            &line,
            &library,
            &glaze,
            UnbalancedIce::ExtraLoad(5.0),
            &[s],
            &budget,
        )
        .unwrap();
        assert!(low(&q, PositionKind::Ice) < iced - 0.01);
    }

    /// The case's ice on one span as a weather case of its own: rime ice
    /// with wind (500 kg/m³, the iced diameter 2.8 times the bare one) on
    /// S1's second span of the example, the other spans bare in the same
    /// wind. The iced span takes the wind on its iced diameter: blown at
    /// least as far as with ice on every span (before 2026-10-20, with the
    /// ice as a spread weight and the wind on the bare diameter, it was
    /// blown 3.7 m against 6.0 m); its radius is the iced one. Extra ice
    /// as a thickness grows that radius by it.
    #[test]
    fn unbalanced_ice_takes_the_wind_on_its_iced_diameter() {
        use std::path::Path;
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let line =
            Line::load(&root.join("examples/synthetic-line/synthetic-line.ohl.json")).unwrap();
        let library = ohl_cables::Library::load(&root.join("libraries/cables")).unwrap();
        let cases = ohl_weather::WeatherCases::load(
            &root.join("examples/synthetic-line/weather-cases.ohlwc"),
        )
        .unwrap()
        .select(&["rime ice + wind".into()], &[])
        .unwrap();
        let budget = Budget::new();
        let solve = |mode: UnbalancedIce| {
            let mut p =
                solve_positions(&line, &library, &cases, PositionOptions::default(), &budget)
                    .unwrap();
            let s = p.span_of("S1", 1).unwrap();
            add_unbalanced_ice(&mut p, &line, &library, &cases, mode, &[s], &budget).unwrap();
            assert!(p.notes.is_empty(), "{:?}", p.notes);
            (p, s)
        };
        // The largest offset across the axis, and the radius.
        let out = |p: &Positions, s: usize, k: PositionKind| {
            let axis = p.spans[s].axis;
            let x = p.spans[s]
                .positions
                .iter()
                .map(|&i| &p.positions[i])
                .find(|x| x.kind == k)
                .unwrap();
            let Shape::Wire(w) = &x.shape else {
                panic!("a wire")
            };
            let side = w
                .wire
                .points
                .iter()
                .map(|q| dot(sub(*q, axis.a1), axis.right).abs())
                .fold(0.0, f64::max);
            (side, w.wire.surface_radius)
        };
        let (p, s) = solve(UnbalancedIce::CaseIce);
        let (case, r) = out(&p, s, PositionKind::Left);
        let (ice, r_ice) = out(&p, s, PositionKind::IceLeft);
        assert!(ice > case, "{ice} {case}");
        assert_eq!(r, r_ice);
        let (p, s) = solve(UnbalancedIce::ExtraThickness(0.005));
        let (_, r_more) = out(&p, s, PositionKind::IceRight);
        assert!((r_more - r - 0.005).abs() < 1e-12, "{r_more} {r}");
    }

    /// The wind stepped (the example line's windy case, steps of a third
    /// of its pressure P, both sides): still air, P/3 and 2P/3 from each
    /// side, and the full pressure from each side — per span 1 + 4 + 2
    /// positions. By hand: a wire hangs in the plane of its load, at
    /// tan φ = q_wind / q_weight from the vertical about the chord between
    /// its clamps, with the wind load q_wind = p · d ∝ p: so
    /// tan φ(p) / tan φ(P) = p / P, 1/3 and 2/3 (to 0.02: the chord is
    /// measured across the span's axis).
    #[test]
    fn wind_steps_of_the_example_line() {
        use std::path::Path;
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let line =
            Line::load(&root.join("examples/synthetic-line/synthetic-line.ohl.json")).unwrap();
        let library = ohl_cables::Library::load(&root.join("libraries/cables")).unwrap();
        let cases = ohl_weather::WeatherCases::load(
            &root.join("examples/synthetic-line/weather-cases.ohlwc"),
        )
        .unwrap()
        .select(&["high wind".into()], &[])
        .unwrap();
        let full = cases[0].wind_pressure.si();
        assert!(full > 0.0);
        let steps = crate::rules::WindSteps {
            stepped: true,
            step: full / 3.0 + 1e-9,
            sides: crate::rules::WindSides::Both,
        };
        let p = solve_positions(
            &line,
            &library,
            &cases,
            PositionOptions {
                swing_arc: false,
                wind_steps: Some(steps),
            },
            &Budget::new(),
        )
        .unwrap();
        let n = p.spans.len();
        let count = |k: PositionKind| p.positions.iter().filter(|x| x.kind == k).count();
        assert_eq!(count(PositionKind::Still), n);
        assert_eq!(count(PositionKind::StepLeft), 2 * n);
        assert_eq!(count(PositionKind::StepRight), 2 * n);
        assert_eq!(
            (count(PositionKind::Left), count(PositionKind::Right)),
            (n, n)
        );
        assert_eq!(
            (count(PositionKind::Arc), count(PositionKind::Case)),
            (0, 0)
        );
        // The blowout angle at mid-span of S1 span 1, by pressure.
        let s = p.span_of("S1", 1).unwrap();
        let axis = p.spans[s].axis;
        let tan = |kind: PositionKind, pressure: Option<f64>| {
            let x = p.spans[s]
                .positions
                .iter()
                .map(|&i| &p.positions[i])
                .find(|x| {
                    x.kind == kind
                        && pressure.is_none_or(|q| x.pressure.is_some_and(|v| (v - q).abs() < 1.0))
                })
                .unwrap();
            let Shape::Wire(w) = &x.shape else { panic!() };
            // About the chord between its clamps: the wire's own plane.
            let (a, b) = (w.wire.points[0], *w.wire.points.last().unwrap());
            let q = sub(w.at(0.5 * axis.length), lerp(a, b, 0.5));
            dot(q, axis.right).abs() / dot(q, axis.down)
        };
        let t_full = tan(PositionKind::Left, None);
        assert!(t_full > 0.1, "{t_full}");
        for (k, kind) in [
            (1.0, PositionKind::StepLeft),
            (2.0, PositionKind::StepRight),
        ] {
            let ratio = tan(kind, Some(k * full / 3.0)) / t_full;
            println!("tan φ at {k}/3 of the pressure: {ratio:.3} of the full one");
            assert!((ratio - k / 3.0).abs() < 0.05, "{ratio}");
        }
        assert!(tan(PositionKind::Still, None) < 0.01 * t_full);
        assert_eq!(
            p.describe(
                p.spans[s]
                    .positions
                    .iter()
                    .copied()
                    .find(|&i| p.positions[i].kind == PositionKind::StepLeft)
                    .unwrap()
            ),
            format!("high wind, left (step) at {:.0} Pa", full / 3.0)
        );
    }

    #[test]
    fn positions_of_wires_keep_one_per_wire() {
        let p = Positions::of_wires("hot", vec![wire(hanging(0.0, 4.0))]);
        assert_eq!((p.spans.len(), p.positions.len()), (1, 1));
        assert!((p.spans[0].capsule - 4.0).abs() < 1e-12);
        assert_eq!(p.describe(0), "hot, case");
    }
}
