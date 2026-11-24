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
use crate::geometry::{P3, closest_on_segment, cross, distance, dot, sub};

fn lerp(a: P3, b: P3, t: f64) -> P3 {
    [
        a[0] + t * (b[0] - a[0]),
        a[1] + t * (b[1] - a[1]),
        a[2] + t * (b[2] - a[2]),
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

/// The band's margin by default, m: every distance to a swing arc's band
/// is taken this much smaller (as if the conductor were this much
/// thicker), on every side. A parameter since 2026-10-21 (the owner's
/// decision; [`crate::rules::WindSteps::band_margin`],
/// [`PositionOptions::band_margin`], [`SwingArc::with_margin`]): a smaller
/// value is on the user's responsibility. **Conservative (measured)**
/// with the default levels ([`BAND_SPACING`]): the band of the solved
/// positions does not quite hold the solver's positions between them —
/// measured 2026-11-16 with steps of 10 Pa from both sides, a step's
/// centre line lies up to 0.009 m outside it on the example lines and
/// 0.063 m on a real line of 930 spans; the margin is that worst excess
/// plus an allowance (`tests/arc_band.rs`). **NOT ON THE SAFE SIDE**
/// without the levels (the band of still air and the blowouts alone, as
/// until 2026-11-16): 0.091 m on the example lines (0.089 m on
/// 2026-10-21), but up to 0.23 m at 500 Pa and 0.69 m at 1300 Pa on the
/// real line — the margin held only up to about 390 Pa there
/// ([`band_levels_note`]). Exact: a solve at every pressure.
pub const BAND_MARGIN: f64 = 0.15;

/// How far a wire may lie above its own chord before its band is taken
/// as not holding it ([`SwingArc::above_axis`]), m: 0.1 m, the order of a
/// wire's own size — a wire hanging under its weight and blown sideways
/// lies below the line between its clamps everywhere (the wind's load and
/// its weight both pull it off that line on the same side); only a wind
/// with a component along an inclined span lifts it.
pub const LIFT_TOLERANCE: f64 = 0.1;

/// A solved position or level whose sag (its largest distance from its
/// own chord) is less than this fraction of still air's marks the band
/// as not holding the wire ([`SwingArc::above_axis`]): 0.5. Measured on a
/// real line of 930 spans: along the line at 1000 Pa a steep span's wire
/// pulled towards its chord lay 0.30 m outside its band with any number
/// of levels, while a wind across the line sags a wire more, never less,
/// than still air.
pub const STRAIGHT_RATIO: f64 = 0.5;

/// A point of a solved position nearer the band's axis than this, m,
/// has no angle about it ([`SwingArc::place`]): its direction is the
/// solver's rounding (a jumper's end at the axis's own end clamp). Its
/// radius is kept; leaving its angle out moves the band by at most twice
/// this (2 µm).
pub const ON_AXIS: f64 = 1e-6;

/// The swing band's levels by default: a solved level at least every
/// this many pascals, Pa (2026-11-16, the owner's "go with spacing"; the
/// setting **band level every … Pa**, [`BandLevels`]). Measured on a
/// real line of 930 spans with 10 Pa steps from both sides (across the
/// line up to 1300 Pa, at 30° and 60°, along the line up to 1450 Pa,
/// with ice): the wire between two solved pressures lies outside the
/// band by about 0.3 mm per pascal of their spacing (0.007 m at 60 Pa
/// apart, 0.019 m at 100, 0.033 m at 130, 0.063 m at 200, 0.082 m at
/// 250, 0.091 m at 260, 0.124 m at 325, 0.167 m at 433), whatever the
/// case's pressure — so the spacing keeps the excess within the margin:
/// at most 200 Pa apart (and the rounding to 10 Pa), at most 0.063 m
/// measured, held by the 0.15 m [`BAND_MARGIN`] with an allowance of
/// 0.087 m. None (the band of still air and the blowouts alone, as until
/// 2026-11-16): up to 0.69 m outside, NOT ON THE SAFE SIDE.
pub const BAND_SPACING: f64 = 200.0;

/// The spacing of the levels of 2026-11-16 (`band_levels`, a count a
/// side): at least one every 200 Pa whatever the count asked.
pub const BAND_LEVEL_SPACING: f64 = BAND_SPACING;

/// The finest spacing a setting takes, Pa: the levels are whole 10 Pa
/// ([`BAND_LEVEL_ROUNDING`]), so finer gives no more levels.
pub const MIN_BAND_SPACING: f64 = BAND_LEVEL_ROUNDING;

/// The coarsest spacing a setting takes, Pa (a bound against a typing
/// slip; anything above the case's pressure is no level at all).
pub const MAX_BAND_SPACING: f64 = 10_000.0;

/// The most levels a band of 2026-11-16 took from each side (the old
/// count setting's bound, read from older files).
pub const MAX_BAND_LEVELS: usize = 50;

/// What the band's levels do and cost, in words (the setting's hover
/// text; measured, see the theory).
pub const BAND_LEVELS_HELP: &str = "The swing band holds still air, the full blowouts and the \
    wire solved at intermediate wind pressures from each side, at most this many pascals \
    apart (evenly between 0 and the case's pressure, rounded to 10 Pa: a 500 Pa case at \
    200 Pa gets 2 levels a side, 170 and 330 Pa; 1300 Pa gets 6). Measured on a real line \
    of 930 spans with 10 Pa steps: the wire between two levels lies outside the band by about \
    0.3 mm per pascal between them — at 200 Pa (the default) at most 0.063 m, inside the \
    0.15 m margin; coarser than 200 Pa is beyond what was measured within the margin and said \
    NOT ON THE SAFE SIDE; 0 (no levels, as before 2026-11-16) up to 0.23 m at 500 Pa and \
    0.69 m at 1300 Pa, NOT ON THE SAFE SIDE. Each level is one more solve of the line a \
    side: at 200 Pa a 500 Pa case takes 7 solves (11 with 4 levels, 3 without). On that line a \
    500 Pa case's band took 15 s without levels and 46 s with 4 levels a side (100 Pa apart); \
    its 10 Pa steps 64 s (a computer shared with other work, load about 30 on 10 cores; the \
    band's time is set by its slowest solves, run side by side). With the wind steps, the \
    levels the steps solve cost nothing.";

/// The levels are whole multiples of this, Pa (10 Pa): solves that the
/// wind steps of 10 Pa (as a span the band does not hold is checked at,
/// and as the band was measured with), and the cases differing only by
/// their pressure ([`family_key`]), share.
pub const BAND_LEVEL_ROUNDING: f64 = 10.0;

/// How a swing band's intermediate levels are chosen (2026-11-16): a
/// solved level at least every [`BandLevels::spacing`] pascals, evenly
/// between still air and the case's full pressure ([`BandLevels::at`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BandLevels {
    /// The levels lie at most this far apart, Pa (`[wind] band_spacing`;
    /// [`BAND_SPACING`] by default); 0: none, the band of still air and
    /// the full blowouts alone (as until 2026-11-16, NOT ON THE SAFE
    /// SIDE, said: [`band_levels_note`]).
    pub spacing: f64,
    /// At least this many a side: the count of a rule file, an option or
    /// a saved run of 2026-11-16 (`band_levels`), read as it was asked —
    /// with its spacing of 200 Pa the same levels as then, never fewer.
    /// `None` for every setting made since.
    pub count: Option<usize>,
}

impl BandLevels {
    /// The default: a level at least every [`BAND_SPACING`].
    pub const DEFAULT: Self = Self {
        spacing: BAND_SPACING,
        count: None,
    };

    /// No levels: the band of still air and the full blowouts alone.
    pub const NONE: Self = Self {
        spacing: 0.0,
        count: None,
    };

    /// A level at least every `spacing` Pa (0: none).
    pub fn every(spacing: f64) -> Self {
        Self {
            spacing,
            count: None,
        }
    }

    /// The setting of 2026-11-16, `n` levels a side and at least one
    /// every 200 Pa (`band_levels`; 0: none): the same levels as then.
    pub fn from_count(n: usize) -> Self {
        if n == 0 {
            Self::NONE
        } else {
            Self {
                spacing: BAND_LEVEL_SPACING,
                count: Some(n),
            }
        }
    }

    /// The levels a command line asks for: `--band-spacing` (Pa) when
    /// given, else an older script's `--band-levels` count (2026-11-16,
    /// read as it asked: [`BandLevels::from_count`]), else `otherwise`;
    /// checked ([`BandLevels::problem`]).
    pub fn from_options(
        spacing: Option<f64>,
        count: Option<usize>,
        otherwise: Self,
    ) -> Result<Self, String> {
        let out = match (spacing, count) {
            (Some(s), _) => Self::every(s),
            (None, Some(n)) => Self::from_count(n),
            (None, None) => otherwise,
        };
        match out.problem() {
            Some(e) => Err(e),
            None => Ok(out),
        }
    }

    /// Whether the band has no levels at all.
    pub fn is_none(&self) -> bool {
        self.spacing.is_nan() || self.spacing <= 0.0
    }

    /// What is wrong with it, if anything: the spacing 0 or between
    /// [`MIN_BAND_SPACING`] and [`MAX_BAND_SPACING`], the count at most
    /// [`MAX_BAND_LEVELS`].
    pub fn problem(&self) -> Option<String> {
        let s = self.spacing;
        if !(s == 0.0 || (MIN_BAND_SPACING..=MAX_BAND_SPACING).contains(&s)) {
            return Some(format!(
                "band level spacing {s} Pa: 0 (no levels) or {MIN_BAND_SPACING:.0} to \
                 {MAX_BAND_SPACING:.0} Pa"
            ));
        }
        match self.count {
            Some(n) if n > MAX_BAND_LEVELS => {
                Some(format!("band levels {n}: at most {MAX_BAND_LEVELS}"))
            }
            _ => None,
        }
    }

    /// The intermediate wind pressures of the band of a case of wind
    /// pressure `full` (Pa), increasing: k·full/(m + 1) for k = 1…m, m
    /// the count that keeps them at most [`BandLevels::spacing`] apart
    /// (⌈full / spacing⌉ − 1; at least [`BandLevels::count`] when given),
    /// each rounded to the nearest whole multiple of
    /// [`BAND_LEVEL_ROUNDING`], those above 0 and below `full` (each
    /// once). Empty for no wind or no levels.
    pub fn at(&self, full: f64) -> Vec<f64> {
        let mut out: Vec<f64> = Vec::new();
        if self.is_none() || !(full.is_finite() && full > 0.0) {
            return out;
        }
        let by_spacing = ((full / self.spacing).ceil() as usize).saturating_sub(1);
        let n = by_spacing.max(self.count.unwrap_or(0));
        for k in 1..=n {
            let p = (full * k as f64 / (n + 1) as f64 / BAND_LEVEL_ROUNDING).round()
                * BAND_LEVEL_ROUNDING;
            if p > 0.0 && p < full && !out.contains(&p) {
                out.push(p);
            }
        }
        out
    }

    /// The largest step between the solved pressures of a case of
    /// pressure `full` (still air, the levels, the full one), Pa.
    pub fn largest_gap(&self, full: f64) -> f64 {
        largest_gap(full, &self.at(full))
    }

    /// The setting in words: "every 200 Pa", "none", an older file's
    /// "4 a side (at least one every 200 Pa)".
    pub fn words(&self) -> String {
        match self.count {
            _ if self.is_none() => "none".to_owned(),
            Some(n) => format!("{n} a side (at least one every {:.0} Pa)", self.spacing),
            None => format!("every {:.0} Pa", self.spacing),
        }
    }
}

impl Default for BandLevels {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// The largest step between 0, the `levels` (increasing) and `full`, Pa.
pub fn largest_gap(full: f64, levels: &[f64]) -> f64 {
    let mut prev = 0.0;
    let mut worst: f64 = 0.0;
    for &p in levels.iter().chain(std::iter::once(&full)) {
        worst = worst.max(p - prev);
        prev = p;
    }
    worst
}

/// The intermediate wind pressures of a band of at least `n` levels for
/// a case of wind pressure `full` (Pa), as on 2026-11-16 (at least one
/// every 200 Pa): [`BandLevels::from_count`]'s.
pub fn band_levels(full: f64, n: usize) -> Vec<f64> {
    BandLevels::from_count(n).at(full)
}

/// The jumpers' band margin by default, m: **0, no widening** (the
/// owner's decision of 2026-10-21: a value of its own, entered by the
/// user for their jumpers; [`crate::rules::WindSteps::jumper_band_margin`],
/// [`PositionOptions::jumper_band_margin`], [`BandMargins::jumper`]).
/// **NOT ON THE SAFE SIDE** (said in the results; on the user's
/// responsibility; exact: the wind steps): with 0 a jumper's band
/// ([`jumper_band`]) is *not* conservative between its solved positions —
/// measured with steps of 10 Pa from both sides, a step's centre line
/// lies up to 0.307 m outside the example line's jumper's band without
/// levels (0.293 m on 2026-10-21) and 0.18 m with the default levels (a
/// level every 200 Pa); on a real line up to 2.1 m without levels, and
/// with the default levels up to 0.13 m since the jumpers are solved as
/// the wind rises and falls and those whose shape jumps between the
/// levels every 10 Pa there (2026-11-17; 0.62 m before, 1.9 m before
/// 2026-11-16: two jumpers solved in other shapes between 140 and 170 Pa,
/// [`ohl_line::jumper::JumperShape::snap`], [`crate::jumper_swaps`])
/// (`tests/arc_band.rs`). A jumper swings with its clamps and changes its
/// shape more than a span: the value that holds it is the user's to find
/// for their jumpers (the wind steps show it).
pub const JUMPER_BAND_MARGIN: f64 = 0.0;

/// The swing bands' margins, m: the spans' and the jumpers' (2026-10-21,
/// the owner's decision: separate values). Every distance to a band is
/// taken this much smaller.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BandMargins {
    /// The spans' bands ([`BAND_MARGIN`] by default).
    pub span: f64,
    /// The jumpers' bands ([`JUMPER_BAND_MARGIN`] by default).
    pub jumper: f64,
}

impl Default for BandMargins {
    fn default() -> Self {
        Self {
            span: BAND_MARGIN,
            jumper: JUMPER_BAND_MARGIN,
        }
    }
}

impl BandMargins {
    /// What is wrong with either margin, if anything: each must be a
    /// number ≥ 0.
    pub fn problem(&self) -> Option<String> {
        crate::rules::band_margin_problem(self.span).or_else(|| {
            crate::rules::band_margin_problem(self.jumper).map(|e| format!("jumper {e}"))
        })
    }
}

/// The swing arc of a span, a **band** since 2026-10-21: the span
/// swinging as one about its axis between its left and right blowout,
/// through its still-air position, at every radius from the smallest to
/// the largest of the three at each place (an annular sector in the
/// plane square to the axis, swept along it).
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
    /// Bundle radius, the largest allowance of the three wires and the
    /// band's margin ([`SwingArc::margin`]), m.
    pub outer: f64,
    /// Bundle radius, m.
    radius: f64,
    /// The largest allowance of the three wires and the band's margin, m.
    allowance: f64,
    /// The band's margin, m.
    margin: f64,
    /// At each station, the envelope of its intermediate solved positions
    /// (its **levels**, 2026-11-16: [`SwingArc::with_levels`]) in the
    /// plane square to the axis there: smallest and largest radius,
    /// smallest and largest angle. Empty: none (the band of the three
    /// positions alone).
    levels: Vec<[f64; 4]>,
    /// How many intermediate positions the band was made with.
    level_count: usize,
    /// The largest height of a position's (or a level's) point above its
    /// own chord, m (≤ 0: none above).
    lift: f64,
    /// Still air's sag: its points' largest distance from its own chord,
    /// m.
    sag_still: f64,
    /// The smallest sag of the positions and levels (as `sag_still`), m.
    sag_least: f64,
}

/// Where a point lies relative to an arc, at its place along the axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArcPlace {
    /// Centre C(s) on the axis.
    pub centre: P3,
    /// The band's outer radius R = max(R_L, R_0, R_R), m.
    pub radius: f64,
    /// The band's inner radius r = min(R_L, R_0, R_R), m.
    pub inner: f64,
    /// Arc ends' angles (from the downward vertical, positive to the
    /// right), rad: the smallest and the largest of φ_L, φ_0, φ_R.
    pub from: f64,
    /// See `from`.
    pub to: f64,
}

/// A point's vertical clearance to a swing arc ([`SwingArc::vertical`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArcVertical {
    /// The point is above the arc's highest over it (a rule "down to the
    /// ground" then takes the arc as inside the point's column).
    pub above: bool,
    /// The vertical distance to the arc's centre line (0 inside its
    /// height range), m.
    pub gap: f64,
    /// The arc's point it is measured to.
    pub point: P3,
    /// Its angle about the axis (rad from the downward vertical, positive
    /// to the right).
    pub angle: f64,
}

impl SwingArc {
    /// The arc of `left`, `still` and `right` along `axis` (none when one
    /// of them does not run steadily along the axis), widened by the
    /// default margin [`BAND_MARGIN`].
    pub fn new(axis: Axis, left: &WireShape, still: &WireShape, right: &WireShape) -> Option<Self> {
        Self::with_margin(axis, left, still, right, BAND_MARGIN)
    }

    /// As [`SwingArc::new`], widened by the band's margin `margin` (m,
    /// ≥ 0; a negative or non-finite one is taken as 0).
    pub fn with_margin(
        axis: Axis,
        left: &WireShape,
        still: &WireShape,
        right: &WireShape,
        margin: f64,
    ) -> Option<Self> {
        let margin = if margin.is_finite() {
            margin.max(0.0)
        } else {
            0.0
        };
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
        Some(
            Self {
                axis,
                down,
                right: right_dir,
                stations,
                points,
                outer: left.radius
                    + left.allowance.max(still.allowance).max(right.allowance)
                    + margin,
                radius: left.radius,
                allowance: left.allowance.max(still.allowance).max(right.allowance) + margin,
                margin,
                levels: Vec::new(),
                level_count: 0,
                lift: f64::NEG_INFINITY,
                sag_still: 0.0,
                sag_least: f64::INFINITY,
            }
            .lifted(&[left, still, right], Some(1)),
        )
    }

    /// The band with its lift ([`SwingArc::lift`]) raised to that of
    /// `wires` — each one's points' largest height above its own chord
    /// (the line between its first and its last point), square to the
    /// axis — and its smallest sag lowered to theirs (the points' largest
    /// distance from that chord); `still`: which of them is still air.
    fn lifted(mut self, wires: &[&WireShape], still: Option<usize>) -> Self {
        for (k, w) in wires.iter().enumerate() {
            let pts = &w.wire.points;
            let (Some(&a), Some(&b)) = (pts.first(), pts.last()) else {
                continue;
            };
            let d = sub(b, a);
            let dd = dot(d, d);
            if dd <= 0.0 {
                continue;
            }
            let mut sag = 0.0f64;
            for &p in pts {
                let t = dot(sub(p, a), d) / dd;
                let q = [a[0] + t * d[0], a[1] + t * d[1], a[2] + t * d[2]];
                self.lift = self.lift.max(-dot(sub(p, q), self.down));
                sag = sag.max(distance(p, q));
            }
            if still == Some(k) {
                self.sag_still = sag;
            }
            self.sag_least = self.sag_least.min(sag);
        }
        self
    }

    /// As [`SwingArc::with_margin`], the band also holding `levels`: the
    /// span's solved positions at intermediate wind pressures (2026-11-16;
    /// [`band_levels`], or the wind steps when solved), from either side.
    /// At each station (the places of the three positions' points) the
    /// band's radii and angles are the smallest and largest of the three
    /// positions' and of every level's point there (each level's point at
    /// the station's place along the axis, on its polyline); between
    /// stations the three positions' points are interpolated as before and
    /// the levels' smallest and largest radius and angle linearly. None
    /// when a position or a level does not run steadily along the axis.
    pub fn with_levels(
        axis: Axis,
        left: &WireShape,
        still: &WireShape,
        right: &WireShape,
        levels: &[&WireShape],
        margin: f64,
    ) -> Option<Self> {
        let mut arc = Self::with_margin(axis, left, still, right, margin)?;
        if levels.is_empty() {
            return Some(arc);
        }
        if levels.iter().any(|w| !w.monotone) {
            return None;
        }
        arc.levels = arc
            .stations
            .iter()
            .map(|&s| {
                let centre = arc.centre(s);
                let mut e = [f64::INFINITY, 0.0, f64::INFINITY, f64::NEG_INFINITY];
                for w in levels {
                    let (r, phi) = arc.polar(sub(w.at(s), centre));
                    e = [e[0].min(r), e[1].max(r), e[2], e[3]];
                    if r > ON_AXIS {
                        e = [e[0], e[1], e[2].min(phi), e[3].max(phi)];
                    }
                }
                if e[2] > e[3] {
                    // Every level on the axis there: no angle of their own.
                    e[2] = f64::INFINITY;
                    e[3] = f64::NEG_INFINITY;
                }
                e
            })
            .collect();
        arc.level_count = levels.len();
        let allowance = levels
            .iter()
            .map(|w| w.allowance)
            .fold(arc.allowance - arc.margin, f64::max);
        arc.allowance = allowance + arc.margin;
        arc.outer = arc.radius + arc.allowance;
        Some(arc.lifted(levels, None))
    }

    /// How many intermediate solved positions (levels) the band holds
    /// besides still air and the blowouts ([`SwingArc::with_levels`]).
    pub fn level_count(&self) -> usize {
        self.level_count
    }

    /// The largest height of a solved position's (or a level's) centre
    /// line above its own chord — the straight line between its two ends
    /// (its clamps), measured square to the span's axis, m (≤ 0: the wire
    /// stays below its chord everywhere). Its own chord, not the span's
    /// axis: a dead end's clamp moves with the wind, and the wire near it
    /// with it.
    pub fn lift(&self) -> f64 {
        self.lift
    }

    /// The smallest sag of its solved positions and levels — each one's
    /// largest distance from its own chord — over still air's, 1 when
    /// none is smaller (a wind across the line sags the wire more in the
    /// plane of its load; one along an inclined span pulls it towards its
    /// chord).
    pub fn sag_ratio(&self) -> f64 {
        if self.sag_still > 0.0 {
            (self.sag_least / self.sag_still).min(1.0)
        } else {
            1.0
        }
    }

    /// Whether the band does not hold the wire because the wind pulls it
    /// **to or above its chord** (2026-11-16): a solved position or a
    /// level more than [`LIFT_TOLERANCE`] above the line between its
    /// clamps somewhere ([`SwingArc::lift`]), or pulled towards that line
    /// to less than [`STRAIGHT_RATIO`] of still air's sag
    /// ([`SwingArc::sag_ratio`]). The band is a swing about the axis; a
    /// wire the wind pulls to or through its chord (a steep span with the
    /// wind along the line at a high pressure) passes near the axis, where
    /// its angle turns fast with the pressure, and the band does not hold
    /// it between its levels.
    pub fn above_axis(&self) -> bool {
        self.lift > LIFT_TOLERANCE || self.sag_ratio() < STRAIGHT_RATIO
    }

    /// The point on the axis at place `s`.
    fn centre(&self, s: f64) -> P3 {
        [
            self.axis.a1[0] + s * self.axis.u[0],
            self.axis.a1[1] + s * self.axis.u[1],
            self.axis.a1[2] + s * self.axis.u[2],
        ]
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
        let (mut radius, mut inner) = (0.0f64, f64::INFINITY);
        let (mut from, mut to) = (f64::INFINITY, f64::NEG_INFINITY);
        for i in 0..3 {
            let p = lerp(self.points[k - 1][i], self.points[k][i], t);
            let v = sub(p, centre);
            let (r, phi) = self.polar(v);
            radius = radius.max(r);
            inner = inner.min(r);
            // A point on the axis has no angle (its direction is the
            // solver's rounding: at a jumper's end clamp, 2026-11-16, it
            // turned a band through the top).
            if r > ON_AXIS {
                from = from.min(phi);
                to = to.max(phi);
            }
        }
        if let (Some(a), Some(b)) = (self.levels.get(k - 1), self.levels.get(k)) {
            let at = |i: usize| a[i] + t * (b[i] - a[i]);
            inner = inner.min(at(0));
            radius = radius.max(at(1));
            // Levels on the axis at either station: none of their angles.
            if a[2] <= a[3] && b[2] <= b[3] {
                from = from.min(at(2));
                to = to.max(at(3));
            } else if a[2] <= a[3] {
                from = from.min(a[2]);
                to = to.max(a[3]);
            } else if b[2] <= b[3] {
                from = from.min(b[2]);
                to = to.max(b[3]);
            }
        }
        if from > to {
            // Every point on the axis here.
            (from, to) = (0.0, 0.0);
        }
        ArcPlace {
            centre,
            radius,
            inner,
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

    /// The distance from `q` to the band (to the wire's centre line
    /// anywhere on it): `q` projected on the axis at s_q (clamped to the
    /// stations), at radius ρ_q and angle φ_q in the plane there; within
    /// the band's angles the radial gap — 0 for r ≤ ρ_q ≤ R, r − ρ_q
    /// inside it, ρ_q − R outside it — (and the part along the axis
    /// beyond its ends), else the nearer of its two end edges (the radial
    /// segments from r to R at φ_L and φ_R). (distance, the band's
    /// point).
    pub fn nearest(&self, q: P3) -> (f64, P3) {
        let place = self.place(self.axis.along(q));
        let v = sub(q, place.centre);
        let along = dot(v, self.axis.u);
        let (rho, phi) = self.polar(v);
        if place.from <= phi && phi <= place.to {
            let r = rho.clamp(place.inner, place.radius);
            let p = self.point(place.centre, r, phi);
            ((rho - r).hypot(along), p)
        } else {
            let edge = |a: f64| {
                let (p, _) = closest_on_segment(
                    q,
                    self.point(place.centre, place.inner, a),
                    self.point(place.centre, place.radius, a),
                );
                (distance(q, p), p)
            };
            let (a, b) = (edge(place.from), edge(place.to));
            if a.0 <= b.0 { a } else { b }
        }
    }

    /// The band's largest thickness R − r over its stations, m.
    pub fn thickness(&self) -> f64 {
        self.stations
            .iter()
            .map(|&s| {
                let p = self.place(s);
                p.radius - p.inner
            })
            .fold(0.0, f64::max)
    }

    /// The largest allowance of its three wires ([`Wire::allowance`]) and
    /// the band's margin ([`SwingArc::margin`]), m.
    pub fn allowance(&self) -> f64 {
        self.allowance
    }

    /// [`SwingArc::outer`] measured vertically, m: a vertical through the
    /// wire meets its surface `outer / cos β` from the axis, β the
    /// slope of the span's axis (as the still-air vertical,
    /// [`crate::checks`]); a level span: `outer`.
    pub fn vertical_outer(&self) -> f64 {
        let cos = self.axis.u[0].hypot(self.axis.u[1]);
        if cos > 1e-9 {
            self.outer / cos
        } else {
            self.outer
        }
    }

    /// The band's margin it was made with, m ([`BAND_MARGIN`] by
    /// default).
    pub fn margin(&self) -> f64 {
        self.margin
    }

    /// The angle about the axis of `p` (rad from the downward vertical,
    /// positive to the right, looking along the axis).
    pub fn angle_of(&self, p: P3) -> f64 {
        self.polar(sub(p, self.axis.a1)).1
    }

    /// Its largest radius over the stations, and the smallest and largest
    /// angle of its ends (rad).
    pub fn extent(&self) -> (f64, f64, f64) {
        self.stations.iter().map(|&s| self.place(s)).fold(
            (0.0f64, f64::INFINITY, f64::NEG_INFINITY),
            |(r, a, b), p| (r.max(p.radius), a.min(p.from), b.max(p.to)),
        )
    }

    /// The centre line of the wire swung to angle `phi` (rad) at the
    /// fraction `t` of the way from the band's inner radius (0) to its
    /// outer one (1): at every station the band's point at `phi`, clamped
    /// to its angles there — a polyline lying on that surface of the band
    /// at every station.
    pub fn line_at_radius(&self, phi: f64, t: f64) -> Vec<P3> {
        self.stations
            .iter()
            .map(|&s| {
                let p = self.place(s);
                let r = p.inner + t * (p.radius - p.inner);
                self.point(p.centre, r, phi.clamp(p.from, p.to))
            })
            .collect()
    }

    /// The centre line of the wire swung the fraction `f` (0: still air,
    /// 1: the full blowout) of its way to side `side` (0: blown to the
    /// left, 2: to the right) as the same wind on every span swings it
    /// (2026-10-28, wire to wire on the swing arc): at every station the
    /// angle φ about the axis with tan φ = tan φ₀ + f (tan φ₁ − tan φ₀)
    /// (the wind's share of the load grows with the pressure, the weight
    /// stays) and the radius r₀ + f (r₁ − r₀), from the still-air (φ₀, r₀)
    /// and the full blowout's (φ₁, r₁) points there.
    pub fn swung(&self, side: usize, f: f64) -> Vec<P3> {
        self.stations
            .iter()
            .zip(&self.points)
            .map(|(&s, p)| {
                let centre = [
                    self.axis.a1[0] + s * self.axis.u[0],
                    self.axis.a1[1] + s * self.axis.u[1],
                    self.axis.a1[2] + s * self.axis.u[2],
                ];
                let (r0, a0) = self.polar(sub(p[1], centre));
                let (r1, a1) = self.polar(sub(p[side], centre));
                let phi = (a0.tan() + f * (a1.tan() - a0.tan())).atan();
                self.point(centre, r0 + f * (r1 - r0), phi)
            })
            .collect()
    }

    /// A bound on how far any point of the wire moves while f runs from
    /// 0 to 1 in [`SwingArc::swung`] at its fastest, m: at every station
    /// r_max |tan φ₁ − tan φ₀| + |r₁ − r₀| (dφ/df = Δ / (1 + tan² φ) ≤ Δ,
    /// Δ = tan φ₁ − tan φ₀), the largest; a step of f of 1/n moves no point
    /// more than this over n.
    pub fn swing_reach(&self, side: usize) -> f64 {
        self.stations
            .iter()
            .zip(&self.points)
            .map(|(&s, p)| {
                let centre = [
                    self.axis.a1[0] + s * self.axis.u[0],
                    self.axis.a1[1] + s * self.axis.u[1],
                    self.axis.a1[2] + s * self.axis.u[2],
                ];
                let (r0, a0) = self.polar(sub(p[1], centre));
                let (r1, a1) = self.polar(sub(p[side], centre));
                r0.max(r1) * (a1.tan() - a0.tan()).abs() + (r1 - r0).abs()
            })
            .fold(0.0, f64::max)
    }

    /// The angles of the circle of radius `r` at `place` whose point lies
    /// within `band` across the line of `y` (its offset to the right of
    /// the axis, m): up to three intervals (the lower half of the circle,
    /// and the two upper quarters an arc rarely reaches), each within the
    /// band's ends.
    fn angles_within(place: &ArcPlace, r: f64, y: f64, band: f64) -> Vec<(f64, f64)> {
        use std::f64::consts::PI;
        if r <= 0.0 {
            // On the axis: every angle is the same point.
            return if y.abs() <= band {
                vec![(place.from, place.to)]
            } else {
                Vec::new()
            };
        }
        let (a, b) = ((y - band) / r, (y + band) / r);
        if a > 1.0 || b < -1.0 {
            return Vec::new();
        }
        let (ta, tb) = (a.max(-1.0).asin(), b.min(1.0).asin());
        [(ta, tb), (PI - tb, PI - ta), (-PI - tb, -PI - ta)]
            .into_iter()
            .map(|(lo, hi)| (lo.max(place.from), hi.min(place.to)))
            .filter(|(lo, hi)| lo <= hi)
            .collect()
    }

    /// The lowest (`low`) or highest point of the band over the plan
    /// position `h` m along the axis's plan direction from A₁ and `y` m to
    /// its right, within `band` across: (point, angle). At a station s
    /// the band's point at radius ρ and angle φ lies
    /// \\( \rho\cos\varphi\,u_z \\) further along in plan than
    /// \\( s\,|u_h| \\) and \\( \rho\cos\varphi\,|u_h| \\) below the centre
    /// (\\( u_h, u_z \\): the axis's horizontal and vertical parts); its
    /// height is lowest where \\( x = \rho\cos\varphi \\) is largest.
    /// Over the band's cross-section (the annular sector r ≤ ρ ≤ R,
    /// φ_L ≤ φ ≤ φ_R, cut to the strip within `band` of `y`) x is linear
    /// in the plane, so its extremes lie on the boundary: on the two
    /// circles (the admissible angles' ends, and φ = 0) or on the two end
    /// edges (where the strip cuts them). The station whose point lies
    /// over `h` is found by fixed-point steps
    /// \\( s \leftarrow (h - x\,u_z)/|u_h| \\) (each a
    /// contraction by about \\( |dR/ds|\,|u_z|/|u_h| \\), well below 1 on
    /// a span), to 10⁻⁶ m. `None`: no point of the arc there (beyond the
    /// span's ends, or farther than `band` across), or a vertical axis.
    fn extreme_over(&self, h: f64, y: f64, band: f64, low: bool) -> Option<(P3, f64)> {
        let u = self.axis.u;
        let uh = u[0].hypot(u[1]);
        if uh < 1e-9 {
            return None;
        }
        let uz = u[2];
        // (radius, angle) of the band's lowest or highest admissible
        // point at a place.
        let pick = |place: &ArcPlace| -> Option<(f64, f64)> {
            let mut best: Option<(f64, f64, f64)> = None;
            let mut take = |r: f64, phi: f64| {
                // Lowest: x = ρ cos φ largest; highest: smallest.
                let x = r * phi.cos();
                let key = if low { -x } else { x };
                if best.is_none_or(|(k, ..)| key < k) {
                    best = Some((key, r, phi));
                }
            };
            for r in [place.inner, place.radius] {
                for (lo, hi) in Self::angles_within(place, r, y, band) {
                    take(r, lo);
                    take(r, hi);
                    if lo <= 0.0 && 0.0 <= hi {
                        take(r, 0.0);
                    }
                }
            }
            // The end edges where the strip cuts them.
            for a in [place.from, place.to] {
                let sa = a.sin();
                if sa.abs() < 1e-12 {
                    continue;
                }
                let (r0, r1) = ((y - band) / sa, (y + band) / sa);
                let (lo, hi) = (r0.min(r1).max(place.inner), r0.max(r1).min(place.radius));
                if lo <= hi {
                    take(lo, a);
                    take(hi, a);
                }
            }
            best.map(|b| (b.1, b.2))
        };
        let n = self.stations.len();
        let (first, last) = (self.stations[0], self.stations[n - 1]);
        let mut s = h / uh;
        let mut rp = (0.0, 0.0);
        for _ in 0..50 {
            let place = self.place(s);
            rp = pick(&place)?;
            let next = (h - rp.0 * rp.1.cos() * uz) / uh;
            let done = (next - s).abs() < 1e-6;
            s = next;
            if done {
                break;
            }
        }
        if s < first - 1e-6 || s > last + 1e-6 {
            return None;
        }
        let place = self.place(s);
        let (r, phi) = pick(&place).unwrap_or(rp);
        Some((self.point(place.centre, r, phi), phi))
    }

    /// The vertical clearance of `q` to the band (to the centre line
    /// anywhere on it): the height of the band's lowest point above it,
    /// or `q`'s height above the band's highest point, over `q`'s plan
    /// position — counting the band's points within `band` across the line and within half of
    /// `depth` along it — 0 where `q` lies between the two. With a depth,
    /// the lowest height over the slice is bounded from the heights at
    /// its ends and middle, \\( f_0 - \max(|f_0 - f_-|, |f_+ - f_0|) \\)
    /// and the ends themselves (exact where the height is convex along
    /// the line, as a hanging wire's; the highest mirrored). `None`: no
    /// point of the arc over `q` within the band.
    pub fn vertical(&self, q: P3, band: f64, depth: f64) -> Option<ArcVertical> {
        let u = self.axis.u;
        let uh = u[0].hypot(u[1]);
        if uh < 1e-9 {
            return None;
        }
        let v = sub(q, self.axis.a1);
        let h = (v[0] * u[0] + v[1] * u[1]) / uh;
        let y = dot(v, self.right);
        let low = self.extreme_over(h, y, band, true)?;
        let high = self.extreme_over(h, y, band, false)?;
        let half = depth.max(0.0) / 2.0;
        let slice = |f0: f64, at: &dyn Fn(f64) -> Option<f64>, lowest: bool| -> f64 {
            if half <= 0.0 {
                return f0;
            }
            let mut out = f0;
            for d in [-half, half] {
                if let Some(f) = at(h + d) {
                    let tangent = 2.0 * f0 - f;
                    out = if lowest {
                        out.min(f).min(tangent)
                    } else {
                        out.max(f).max(tangent)
                    };
                }
            }
            out
        };
        let zl = slice(
            low.0[2],
            &|hh| self.extreme_over(hh, y, band, true).map(|p| p.0[2]),
            true,
        );
        let zh = slice(
            high.0[2],
            &|hh| self.extreme_over(hh, y, band, false).map(|p| p.0[2]),
            false,
        );
        let z = q[2];
        Some(if z <= zl {
            ArcVertical {
                above: false,
                gap: zl - z,
                point: low.0,
                angle: low.1,
            }
        } else if z >= zh {
            ArcVertical {
                above: true,
                gap: z - zh,
                point: high.0,
                angle: high.1,
            }
        } else {
            ArcVertical {
                above: false,
                gap: 0.0,
                point: low.0,
                angle: low.1,
            }
        })
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
    /// Per case, its own wind direction at an angle as given (none:
    /// across the line, or not known): said by [`Positions::describe`]
    /// for its blown positions ([`from_side`]).
    pub directions: Vec<Option<ohl_weather::WindDirection>>,
    /// The spans.
    pub spans: Vec<Span>,
    /// The positions.
    pub positions: Vec<Position>,
    /// What could not be done (sections not solved, arcs left out…).
    pub notes: Vec<String>,
    /// The line solves asked for and made (each distinct solve once,
    /// [`solve_key`]; 2026-11-16).
    pub solves: (usize, usize),
}

impl Positions {
    /// Only the positions the structure and section overrides let their
    /// span take (owner, 2026-10-25): a span keeps a case's positions
    /// when its section's and its ends' groups give that case. The
    /// spans' bounds stay those of all their positions (bounds only: as
    /// wide or wider).
    pub fn retain_cases(&mut self, line: &Line, filter: &ohl_weather::CaseFilter) {
        if filter.is_none() {
            return;
        }
        let takes: Vec<bool> = self
            .positions
            .iter()
            .map(|p| {
                let w = &self.spans[p.span].wire;
                let case = &self.cases[p.case];
                match line.sections.iter().find(|s| s.id == w.section) {
                    Some(sec) => {
                        let end = |k: usize| sec.points.get(k).map_or("", |x| x.structure.as_str());
                        filter.takes_span(&sec.id, [end(w.span), end(w.span + 1)], case)
                    }
                    None => true,
                }
            })
            .collect();
        let mut new_index = vec![None; self.positions.len()];
        let mut kept = Vec::new();
        for (k, p) in std::mem::take(&mut self.positions).into_iter().enumerate() {
            if takes[k] {
                new_index[k] = Some(kept.len());
                kept.push(p);
            }
        }
        self.positions = kept;
        for s in &mut self.spans {
            s.positions = s.positions.iter().filter_map(|k| new_index[*k]).collect();
        }
    }

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

    /// "case, position" of position `k`; a blown position of a case
    /// with a wind at an angle with its direction: "storm, left (from
    /// 30°)".
    pub fn describe(&self, k: usize) -> String {
        let p = &self.positions[k];
        let side = match p.kind {
            PositionKind::Left | PositionKind::StepLeft | PositionKind::IceLeft => {
                Some(WindSide::Right)
            }
            PositionKind::Right | PositionKind::StepRight | PositionKind::IceRight => {
                Some(WindSide::Left)
            }
            _ => None,
        };
        let kind = match (self.directions.get(p.case).copied().flatten(), side) {
            (Some(d), Some(side)) => {
                format!("{} ({})", p.kind.name(), direction_from(d, side).words())
            }
            _ => p.kind.name().to_owned(),
        };
        match p.pressure {
            Some(q) => format!("{}, {kind} at {q:.0} Pa", self.cases[p.case]),
            None => format!("{}, {kind}", self.cases[p.case]),
        }
    }
}

/// What positions to solve beyond each case as it is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionOptions {
    /// The swing arc of each span of a windy case.
    pub swing_arc: bool,
    /// The wind of each windy case stepped ([`crate::rules::WindSteps`],
    /// as the clearance windows step it): still air and every
    /// intermediate pressure from 0 up to the case's, from the sides it
    /// names (the case's full pressure is checked from both sides in any
    /// case). Alongside the swing arc or instead of it.
    pub wind_steps: Option<crate::rules::WindSteps>,
    /// The jumpers too, at their solved positions in every solve (blown
    /// left and right, every step; still air when the arc is off or the
    /// jumper has no band) and, with `swing_arc`, on a swing band of
    /// their own (2026-10-21): the safety volumes'. Each jumper is a span
    /// of its own (section `jumper:<id>`, span 0), its axis from its first
    /// to its last point (its two end clamps as first solved).
    pub jumpers: bool,
    /// The spans' swing band margin, m ([`BAND_MARGIN`] by default; the
    /// user's, 2026-10-21): every distance to a span's band taken this
    /// much smaller.
    pub band_margin: f64,
    /// The jumpers' swing band margin, m ([`JUMPER_BAND_MARGIN`], 0, by
    /// default; the user's, 2026-10-21): every distance to a jumper's
    /// band taken this much smaller.
    pub jumper_band_margin: f64,
    /// The swing bands' intermediate levels from each side (2026-11-16,
    /// first a count, then a spacing, [`BAND_SPACING`] by default:
    /// [`BandLevels::at`]): each a solve of the case at that pressure,
    /// held by the band, not checked as a position itself. With the wind
    /// steps, every step is a level too (no more solves).
    pub band_levels: BandLevels,
}

impl Default for PositionOptions {
    fn default() -> Self {
        Self {
            swing_arc: false,
            wind_steps: None,
            jumpers: false,
            band_margin: BAND_MARGIN,
            jumper_band_margin: JUMPER_BAND_MARGIN,
            band_levels: BandLevels::DEFAULT,
        }
    }
}

/// A jumper's swing band (2026-10-21, the owner's decision): its
/// positions blown to the left, in still air and blown to the right as a
/// span's ([`SwingArc`]), about the axis through its two end clamps — its
/// first and last points blown to the left (else in still air) —, from
/// the smallest to the largest radius at each place, widened by the
/// jumpers' band margin `margin` (m; [`JUMPER_BAND_MARGIN`], 0, by
/// default since 2026-10-21: not conservative between the blowouts). A side not given takes the still air.
/// None when the ends coincide or a position does not run steadily along
/// the axis (the jumper is then taken at its solved positions).
pub fn jumper_band(
    left: Option<&Wire>,
    still: &Wire,
    right: Option<&Wire>,
    margin: f64,
) -> Option<SwingArc> {
    jumper_band_levels(left, still, right, &[], margin)
}

/// [`jumper_band`] also holding `levels`, the jumper solved at
/// intermediate wind pressures ([`SwingArc::with_levels`], 2026-11-16).
pub fn jumper_band_levels(
    left: Option<&Wire>,
    still: &Wire,
    right: Option<&Wire>,
    levels: &[&Wire],
    margin: f64,
) -> Option<SwingArc> {
    let ends = left.unwrap_or(still);
    let axis = Axis::new(*ends.points.first()?, *ends.points.last()?)?;
    let shape = |w: &Wire| WireShape::new(w.clone(), &axis);
    let mid = shape(still);
    let l = left.map_or_else(|| mid.clone(), shape);
    let r = right.map_or_else(|| mid.clone(), shape);
    let levels: Vec<WireShape> = levels.iter().map(|w| shape(w)).collect();
    let levels: Vec<&WireShape> = levels.iter().collect();
    SwingArc::with_levels(axis, &l, &mid, &r, &levels, margin)
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
pub(crate) fn span_axis(shape: &SectionShape, k: usize) -> Option<Axis> {
    Axis::new(attachment(shape, k), attachment(shape, k + 1))
}

/// The case with its wind from `side` (the side it blows from, looking
/// ahead along the line): a case without a direction, or with its own
/// across the line, the across rule from that side. A wind at an angle
/// θ (owner 2026-11-15/16, as the desktop application draws it): from
/// the right θ when it comes from the right half (0° < θ < 180°), else
/// its mirror about the line, 360° − θ; from the left the other one — so
/// that the two sides are the direction and its mirror, each blowing the
/// wires to the side named. Along the line (0°, 180°) it has no other
/// side: the case as it is from both ([`one_side_only`]). Until
/// 2026-11-16 only the side (`wind_from`) was set, which a wind at an
/// angle does not read: both sides were the case as it is, its mirror
/// never checked.
pub fn from_side(case: &WeatherCase, side: WindSide) -> WeatherCase {
    let mut c = case.clone();
    if let Some(d) = case.oblique() {
        c.wind_direction = Some(direction_from(d, side));
    }
    c.wind_from = side;
    c
}

/// The side a wind at an angle comes from: the right for
/// 0° ≤ θ ≤ 180°, else the left.
fn half_of(d: ohl_weather::WindDirection) -> WindSide {
    if d.degrees() > 180.0 {
        WindSide::Left
    } else {
        WindSide::Right
    }
}

/// A wind at an angle `d` from `side` ([`from_side`]): `d` when it comes
/// from that side, else its mirror (360° − θ); along the line `d`.
pub fn direction_from(d: ohl_weather::WindDirection, side: WindSide) -> ohl_weather::WindDirection {
    if d.along() || half_of(d) == side {
        d
    } else {
        d.mirrored()
    }
}

/// The side a case's own direction blows from: across the line its own
/// side; at an angle the right for 0° ≤ θ ≤ 180°, else the left; none
/// without a direction (or without wind).
pub fn own_from(case: &WeatherCase) -> Option<WindSide> {
    case.own_side().or_else(|| case.oblique().map(half_of))
}

/// Whether the case's wind has one side only: along the line (from ahead,
/// 0°, or from behind, 180°: its own mirror) — checked once, not from
/// each side ([`from_side`] gives the same case).
pub fn one_side_only(case: &WeatherCase) -> bool {
    case.oblique().is_some_and(|d| d.along())
}

/// The sides a windy case is solved from at its full pressure: both, the
/// right first; one along the line ([`one_side_only`]).
fn full_sides(case: &WeatherCase) -> &'static [WindSide] {
    if one_side_only(case) {
        &[WindSide::Right]
    } else {
        &[WindSide::Right, WindSide::Left]
    }
}

/// The case with its wind from `side` ([`from_side`]).
fn blown(case: &WeatherCase, side: WindSide) -> WeatherCase {
    from_side(case, side)
}

/// The position of a case blown from `side` at its full pressure: blown
/// to the other side.
fn blown_kind(side: WindSide) -> PositionKind {
    match side {
        WindSide::Right => PositionKind::Left,
        WindSide::Left => PositionKind::Right,
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
    // Line solves inside the jobs: each on its share of the cores.
    let inner = (std::thread::available_parallelism().map_or(1, |n| n.get()) / threads).max(1);
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
                    match ohl_line::jumper::with_threads(inner, || run(k)) {
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

/// The case as a solve reads it (2026-11-16): without its name,
/// description and tags; across the line (no direction at an angle,
/// [`WeatherCase::oblique`]) without its direction, which the solve does
/// not read there (its side, `wind_from`, is the across rule's) — so the
/// two copies of one wind across the line, from 90° and from 270°, blown
/// from the same side are one solve.
fn as_solved(case: &WeatherCase) -> WeatherCase {
    let mut k = case.clone();
    k.name.clear();
    k.description.clear();
    k.tags.clear();
    if k.oblique().is_none() {
        k.wind_direction = None;
    }
    // Without wind (2026-11-17): its side and direction are not read (a
    // jumper's falling wind back to still air from either side, the
    // still air of the case: one line solve).
    if k.wind_pressure.si() == 0.0 {
        k.wind_direction = None;
        k.wind_from = WindSide::Right;
    }
    k
}

/// The key of a line solve (2026-11-16): the weather case as the solve
/// reads it (its `Debug` text: every number exactly, each field; the
/// name, description and tags left out, a direction across the line too).
/// Two solves of one key are the same solve, bit for bit: done once
/// ([`solve_positions`], [`crate::zone::solve_states`]).
pub fn solve_key(case: &WeatherCase) -> String {
    format!("{:?}", as_solved(case))
}

/// The **family** of a windy case (2026-11-16): its [`solve_key`] without
/// its wind pressure. Cases of one family differ by their pressure alone
/// (the same temperature, ice, wind side and direction, air density…):
/// the solves of one at a pressure are the solves of another at that
/// pressure, so the wind steps of the one of largest pressure hold every
/// other's (the 500 and 600 Pa cases are steps of the 800 Pa case's).
/// A wind from another direction (along the line, at an angle) is another
/// family, solved on its own.
pub fn family_key(case: &WeatherCase) -> String {
    let mut k = as_solved(case);
    k.wind_pressure = ohl_core::units::Stress::from_si(0.0);
    format!("{k:?}")
}

/// One line solve a run asks for.
struct Job {
    /// The case (index in the cases).
    case: usize,
    /// The position it gives.
    kind: PositionKind,
    /// The case as solved (its side and pressure set).
    solve: WeatherCase,
    /// Its wind pressure when not the case's own (a step or a level), Pa.
    pressure: Option<f64>,
    /// Solved for the swing band only (a level), not checked itself.
    band_only: bool,
    /// The side its wind blows from (none: still air of the case).
    side: Option<WindSide>,
    /// The case's own pressure when solved below it from a side (a step,
    /// a level, still air from a side): its stiff jumpers also as the wind
    /// falls from there (2026-11-17, [`ohl_line::jumper::WindPath`]).
    top: Option<f64>,
    /// Gives the jumpers' shapes as the wind falls back to still air from
    /// its side alone (2026-11-17; the still air of the case is its own
    /// job): their other shapes, nothing else.
    fallen_only: bool,
}

/// The sections of one line solve: those solved, and why others were not.
struct SectionSolve {
    shapes: Vec<SectionShape>,
    failed: Vec<String>,
}

/// One solve made: the sections solved, the wires, what failed, and what
/// the jumpers met along the wind.
struct Solve {
    sections: std::sync::Arc<SectionSolve>,
    wires: Vec<Wire>,
    failed: Vec<String>,
    /// The jumpers that snapped over to another stable shape as the wind
    /// rose, and at what pressure (Pa; [`ohl_line::jumper::JumperShape::snap`]).
    snaps: Vec<(String, f64)>,
    /// The jumpers whose falling wind was solved (2026-11-17; the same
    /// shape or another, [`ohl_line::jumper::Falling`]).
    fell: Vec<String>,
    /// The jumpers' other shapes as the wind falls ([`Falling::Other`]).
    fallen: Vec<(String, Vec<P3>)>,
    /// The jumpers with a second stable shape in still air
    /// ([`ohl_line::jumper::JumperShape::other_still`]).
    other_still: Vec<(String, Vec<P3>, f64)>,
    /// The jumps of each jumper on its way along the wind, Pa
    /// ([`ohl_line::jumper::JumperShape::jumps`]).
    jumps: Vec<(String, Vec<f64>)>,
    /// The jumpers as the wind rose to the pressure solved.
    jumpers: Vec<ohl_line::jumper::JumperShape>,
}

/// The solves of a run, each once ([`solve_key`]; the jumpers' falling
/// wind a solve of its own on the sections of the same line solve).
#[derive(Default)]
struct Solves {
    keys: std::collections::HashMap<String, usize>,
    made: Vec<Solve>,
    asked: usize,
    sections: std::collections::HashMap<String, std::sync::Arc<SectionSolve>>,
}

/// What a solve of the jumpers looks for along the wind (2026-11-17): as
/// the wind falls from `top`, and in still air (no `top`) their other
/// shapes.
fn wind_path(case: &WeatherCase, top: Option<f64>) -> ohl_line::jumper::WindPath {
    ohl_line::jumper::WindPath {
        falling_from: top,
        still_shapes: top.is_none() && case.wind_pressure.si() == 0.0,
    }
}

/// What the jumpers solved `solved` met along the wind, for a [`Solve`]:
/// (snaps, those whose falling wind was solved, their other shapes as it
/// falls, their other still-air shapes, their jumps).
#[allow(clippy::type_complexity)]
pub(crate) fn jumper_paths(
    solved: &[ohl_line::jumper::JumperShape],
) -> (
    Vec<(String, f64)>,
    Vec<String>,
    Vec<(String, Vec<P3>)>,
    Vec<(String, Vec<P3>, f64)>,
    Vec<(String, Vec<f64>)>,
) {
    use ohl_line::jumper::Falling;
    let snaps = solved
        .iter()
        .filter_map(|j| Some((j.jumper.clone(), j.snap?)))
        .collect();
    let fell = solved
        .iter()
        .filter(|j| matches!(j.falling, Falling::Same | Falling::Other(_)))
        .map(|j| j.jumper.clone())
        .collect();
    let fallen = solved
        .iter()
        .filter_map(|j| match &j.falling {
            Falling::Other(p) => Some((j.jumper.clone(), p.clone())),
            _ => None,
        })
        .collect();
    let other = solved
        .iter()
        .filter_map(|j| {
            Some((
                j.jumper.clone(),
                j.other_still.clone()?,
                j.other_still_apart()?,
            ))
        })
        .collect();
    let jumps = solved
        .iter()
        .filter(|j| !j.jumps.is_empty())
        .map(|j| (j.jumper.clone(), j.jumps.clone()))
        .collect();
    (snaps, fell, fallen, other, jumps)
}

impl Solves {
    /// Solves the jobs whose key was not solved yet (in parallel): the
    /// sections of each line solve once, then the jumpers of each job's
    /// key on them; the solve of each job (index in `made`).
    fn run(
        &mut self,
        line: &Line,
        library: &ohl_cables::Library,
        jobs: &[Job],
        jumpers: bool,
        budget: &Budget,
    ) -> Result<Vec<usize>, String> {
        let mut of_job = Vec::with_capacity(jobs.len());
        let mut new: Vec<(String, &Job)> = Vec::new();
        for j in jobs {
            let next = self.made.len() + new.len();
            let section = solve_key(&j.solve);
            let key = format!("{section}|{:?}|{}", j.top, j.fallen_only);
            let k = *self.keys.entry(key).or_insert_with(|| {
                new.push((section, j));
                next
            });
            of_job.push(k);
        }
        self.asked += jobs.len();
        // The line solves not made yet, each once.
        let mut lines: Vec<(&String, &WeatherCase)> = Vec::new();
        for (section, j) in &new {
            if !self.sections.contains_key(section) && !lines.iter().any(|(k, _)| *k == section) {
                lines.push((section, &j.solve));
            }
        }
        let fe = ohl_wirefe::default_fe_budget();
        let solved = parallel(lines.len(), budget, |k| {
            let results = ohl_line::solve::solve_line_memo(line, library, lines[k].1, &fe);
            let mut shapes = Vec::new();
            let mut failed = Vec::new();
            for (section, r) in line.sections.iter().zip(results) {
                match r {
                    Ok(s) => shapes.push(s),
                    Err(e) => failed.push(format!("section {} not solved: {e}", section.id)),
                }
            }
            Ok(SectionSolve { shapes, failed })
        })?;
        for ((key, _), s) in lines.iter().zip(solved) {
            self.sections.insert((*key).clone(), std::sync::Arc::new(s));
        }
        let sections = &self.sections;
        let made = parallel(new.len(), budget, |k| {
            let (key, job) = &new[k];
            let sections = sections[key].clone();
            let mut failed = if job.fallen_only {
                // Said by the still air's own solve.
                Vec::new()
            } else {
                sections.failed.clone()
            };
            let mut solved_jumpers = Vec::new();
            if jumpers && !line.jumpers.is_empty() {
                let results = ohl_line::jumper::solve_jumpers_on(
                    line,
                    library,
                    &job.solve,
                    &sections.shapes,
                    wind_path(&job.solve, job.top),
                );
                for (def, r) in line.jumpers.iter().zip(results) {
                    match r {
                        Ok(j) => solved_jumpers.push(j),
                        Err(e) if !job.fallen_only => {
                            failed.push(format!("jumper {} not solved: {e}", def.id));
                        }
                        Err(_) => {}
                    }
                }
            }
            let fallen: Vec<ohl_line::jumper::JumperShape> = solved_jumpers
                .iter()
                .filter_map(ohl_line::jumper::JumperShape::fallen)
                .collect();
            let wires = if job.fallen_only {
                wires_of(line, &[], &fallen)
            } else {
                // The shapes rising first (each jumper's band and span
                // are made from them), then the other shapes falling.
                let mut all = solved_jumpers.clone();
                all.extend(fallen);
                wires_of(line, &sections.shapes, &all)
            };
            let (snaps, fell, fallen, other_still, jumps) = jumper_paths(&solved_jumpers);
            Ok(Solve {
                sections,
                wires,
                failed,
                snaps,
                fell,
                fallen,
                other_still,
                jumps,
                jumpers: solved_jumpers,
            })
        })?;
        self.made.extend(made);
        Ok(of_job)
    }
}

/// The position of a step (or level) blown from `side`: blown to the
/// other side.
fn step_kind(side: WindSide) -> PositionKind {
    match side {
        WindSide::Right => PositionKind::StepLeft,
        WindSide::Left => PositionKind::StepRight,
    }
}

/// What the results of a run with the swing band say when its solved
/// pressures lie further apart than measured within the margin
/// (`gap`: the largest step between still air, the levels and the full
/// pressure of any case banded, Pa; more than [`BAND_SPACING`] and the
/// rounding): not on the safe side — without levels (`levels` none: the
/// band of still air and the full blowouts alone, as until 2026-11-16),
/// or with levels set further apart than 200 Pa. None within it.
pub fn band_levels_note(levels: &BandLevels, gap: f64) -> Option<String> {
    if gap.is_nan() || gap <= BAND_SPACING + BAND_LEVEL_ROUNDING {
        return None;
    }
    Some(if levels.is_none() {
        "NOT ON THE SAFE SIDE: the swing band without levels (still air and the full blowouts \
         alone): between them a wire can lie outside the band by more than its margin \
         (measured on a real line of 930 spans: up to 0.23 m at 500 Pa and 0.69 m at 1300 Pa \
         across the line), so a clearance can be smaller than shown. Set a band level every \
         200 Pa (the default), or tick the wind steps."
            .to_owned()
    } else {
        format!(
            "NOT ON THE SAFE SIDE: swing band levels up to {gap:.0} Pa apart, more than the \
             200 Pa measured within the band's margin (on a real line of 930 spans a wire lies \
             outside the band by about 0.3 mm per pascal between its solved pressures: 0.12 m \
             at 325 Pa, 0.17 m at 433 Pa apart), so a clearance can be smaller than shown. Set \
             a band level every 200 Pa (the default) or less, or tick the wind steps."
        )
    })
}

/// What a case's results say of the jumpers that went over to another
/// stable shape as the wind rose (`snaps`: jumper id, the lowest pressure
/// it snapped at, Pa; [`ohl_line::jumper::JumperShape::snap`],
/// 2026-11-16): not on the safe side — a jumper is taken in the shape it
/// reaches as the wind rises from still air, and one that snapped over
/// keeps its new shape as the wind falls again, which these results do not
/// hold below that pressure. None without such jumpers.
pub fn jumper_snap_note(case: &str, snaps: &[(String, f64)]) -> Option<String> {
    if snaps.is_empty() {
        return None;
    }
    let mut snaps = snaps.to_vec();
    snaps.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    let list: Vec<String> = snaps
        .iter()
        .map(|(j, p)| format!("{j} at about {p:.0} Pa"))
        .collect();
    Some(format!(
        "NOT ON THE SAFE SIDE: case \"{case}\": {} jumper(s) went over to another stable shape \
         as the wind rose ({}): taken in the shape they reach as the wind rises from still air; \
         as it falls again they keep the new one, which these results do not hold below that \
         pressure, so a clearance there can be smaller than shown (check them with that wind \
         from the other side, or their shape)",
        snaps.len(),
        list.join(", ")
    ))
}

/// The jumpers of `snaps` (jumper id, the lowest pressure it snapped at)
/// whose shape as the wind falls again the solves below the case's
/// pressure `below` hold (2026-11-17): (pressure, the jumpers whose
/// falling wind it solved, whether it is still air reached falling) of
/// each — still air reached falling solved, and every solve below the
/// snap with its falling wind solved for the jumper. (Held, not held.)
#[allow(clippy::type_complexity)]
pub fn falling_covers(
    snaps: &[(String, f64)],
    below: &[(f64, &Vec<String>, bool)],
) -> (Vec<(String, f64)>, Vec<(String, f64)>) {
    let mut held = Vec::new();
    let mut open = Vec::new();
    for (j, p) in snaps {
        let fell = |x: &(f64, &Vec<String>, bool)| x.1.contains(j);
        let ok = below.iter().any(|x| x.2 && fell(x)) && below.iter().all(|x| x.0 >= *p || fell(x));
        if ok {
            held.push((j.clone(), *p));
        } else {
            open.push((j.clone(), *p));
        }
    }
    (held, open)
}

/// What a case's results say of the jumpers that snapped over as the wind
/// rose and are checked in the shapes they keep as it falls again
/// (2026-11-17; [`falling_covers`]). None without such jumpers.
pub fn jumper_snap_held_note(case: &str, snaps: &[(String, f64)]) -> Option<String> {
    if snaps.is_empty() {
        return None;
    }
    let mut snaps = snaps.to_vec();
    snaps.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    let list: Vec<String> = snaps
        .iter()
        .map(|(j, p)| format!("{j} at about {p:.0} Pa"))
        .collect();
    Some(format!(
        "case \"{case}\": {} jumper(s) went over to another stable shape as the wind rose ({}): \
         checked in the shapes they reach as the wind rises from still air and in those they \
         keep as it falls again, at every pressure solved",
        snaps.len(),
        list.join(", ")
    ))
}

/// What the results say of jumpers with a second stable shape in still air
/// (2026-11-17; `others`: jumper id, the most its points lie from the
/// shape checked, m, and whether a falling wind reaches it — then checked
/// in the band). The shape checked in still air is the one a jumper takes
/// hung from its clamps (solved from the hinged catenary): which of its
/// shapes a real one is in depends on how it was hung and on the winds
/// since. Not reached by any wind solved: not on the safe side. None
/// without such jumpers.
pub fn other_still_notes(others: &[(String, f64, bool)]) -> Vec<String> {
    let mut out = Vec::new();
    let list = |reached: bool| -> Vec<String> {
        let mut l: Vec<&(String, f64, bool)> = others.iter().filter(|x| x.2 == reached).collect();
        l.sort_by(|a, b| a.0.cmp(&b.0));
        l.iter()
            .map(|(j, d, _)| format!("{j}, up to {d:.2} m apart"))
            .collect()
    };
    let open = list(false);
    if !open.is_empty() {
        out.push(format!(
            "NOT ON THE SAFE SIDE: {} jumper(s) have a second stable shape in still air (the loop \
             lying to the other side; {}): only the shape a jumper takes hung from its clamps \
             (solved from the hinged catenary, as installed) is checked, not the other — which \
             one a real jumper is in depends on how it was hung and on the winds since. Check \
             its shape on site, or the other shape by hand",
            open.len(),
            open.join("; ")
        ));
    }
    let reached = list(true);
    if !reached.is_empty() {
        out.push(format!(
            "{} jumper(s) have a second stable shape in still air ({}): it is the shape they keep \
             once a wind from a side has fallen again — checked with their swing bands",
            reached.len(),
            reached.join("; ")
        ));
    }
    out
}

/// The swaps between neighbouring solved pressures of each side of a
/// case's jobs `list` ([`crate::jumper_swaps::cross_swaps`]; still air
/// below both sides' first pressure), the pairs in parallel.
fn cross_of<'a>(
    line: &Line,
    library: &ohl_cables::Library,
    jobs: &[Job],
    list: &[usize],
    solve_of: &(dyn Fn(usize) -> &'a Solve + Sync),
    budget: &Budget,
) -> Result<Vec<crate::jumper_swaps::Swap>, String> {
    let still = list
        .iter()
        .position(|&j| jobs[j].side.is_none() && jobs[j].solve.wind_pressure.si() == 0.0);
    let mut pairs: Vec<(WindSide, usize, usize)> = Vec::new();
    for side in [WindSide::Right, WindSide::Left] {
        let mut at: Vec<(f64, usize)> = list
            .iter()
            .enumerate()
            .filter(|(_, j)| jobs[**j].side == Some(side) && !jobs[**j].fallen_only)
            .map(|(i, j)| (jobs[*j].solve.wind_pressure.si(), i))
            .collect();
        if at.is_empty() {
            continue;
        }
        if let Some(s) = still {
            at.push((0.0, s));
        }
        at.sort_by(|a, b| a.0.total_cmp(&b.0));
        for w in at.windows(2) {
            pairs.push((side, w[0].1, w[1].1));
        }
    }
    let found = parallel(pairs.len(), budget, |k| {
        let (side, lo, up) = pairs[k];
        let of = |i: usize| crate::jumper_swaps::Solved {
            pressure: jobs[list[i]]
                .pressure
                .unwrap_or(jobs[list[i]].solve.wind_pressure.si()),
            case: &jobs[list[i]].solve,
            shapes: &solve_of(i).sections.shapes,
            jumpers: &solve_of(i).jumpers,
        };
        Ok(crate::jumper_swaps::cross_swaps(
            line,
            library,
            side,
            &of(lo),
            &of(up),
        ))
    })?;
    Ok(found.into_iter().flatten().collect())
}

/// Whether two shapes of a jumper are one: their points nowhere further
/// apart than [`ohl_line::jumper::OTHER_STILL_APART`].
pub(crate) fn same_shape(a: &[P3], b: &[P3]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| distance(*x, *y) <= ohl_line::jumper::OTHER_STILL_APART)
}

/// The solves a run asks for, in order ([`solve_positions`]).
fn jobs_of(cases: &[WeatherCase], options: &PositionOptions) -> Vec<Job> {
    let mut jobs: Vec<Job> = Vec::new();
    let job = |case, kind, solve, pressure, band_only| Job {
        case,
        kind,
        solve,
        pressure,
        band_only,
        side: None,
        top: None,
        fallen_only: false,
    };
    // A job below the case's pressure from `side`: its jumpers also as
    // the wind falls from the case's own (2026-11-17).
    let below = |mut j: Job, side: WindSide, full: f64| {
        j.side = Some(side);
        j.top = Some(full);
        j
    };
    for (c, case) in cases.iter().enumerate() {
        if !is_windy(case) {
            jobs.push(job(c, PositionKind::Case, case.clone(), None, false));
            continue;
        }
        for &side in full_sides(case) {
            let mut j = job(c, blown_kind(side), blown(case, side), None, false);
            j.side = Some(side);
            jobs.push(j);
        }
        let full = case.wind_pressure.si();
        let mut stepped: Vec<(PositionKind, f64)> = Vec::new();
        // The sides solved below the full pressure: the jumpers' falling
        // wind back to still air from each.
        let mut fall_sides: Vec<WindSide> = Vec::new();
        if let Some(steps) = &options.wind_steps {
            // Still air (checked, and the arc's middle) and every
            // pressure between 0 and the full one.
            jobs.push(job(c, PositionKind::Still, still(case), Some(0.0), false));
            // A case with its own direction (across the line or at an
            // angle), stepped from one side: its own side, the
            // direction alone (never turned to the side asked for;
            // owner 2026-11-15); from both, the direction and its
            // mirror; along the line, once.
            let sides: Vec<WindSide> = match own_from(case) {
                _ if one_side_only(case) => vec![WindSide::Right],
                Some(own) if steps.sides != crate::rules::WindSides::Both => vec![own],
                _ => steps.sides.sides().to_vec(),
            };
            for p in steps.pressures(full, crate::zone::MAX_STEPS) {
                if p <= 0.0 || p >= full {
                    continue;
                }
                for &side in &sides {
                    let mut at = blown(case, side);
                    at.wind_pressure = ohl_core::units::Stress::from_si(p);
                    jobs.push(below(
                        job(c, step_kind(side), at, Some(p), false),
                        side,
                        full,
                    ));
                    stepped.push((step_kind(side), p));
                }
            }
            fall_sides.extend(sides);
        } else if options.swing_arc {
            // Still air: for the arc only.
            jobs.push(job(c, PositionKind::Case, still(case), None, false));
        }
        if options.swing_arc {
            // The band's levels (2026-11-16): those the steps do not
            // solve already.
            for &side in full_sides(case) {
                for p in options.band_levels.at(full) {
                    if stepped.contains(&(step_kind(side), p)) {
                        continue;
                    }
                    let mut at = blown(case, side);
                    at.wind_pressure = ohl_core::units::Stress::from_si(p);
                    jobs.push(below(
                        job(c, step_kind(side), at, Some(p), true),
                        side,
                        full,
                    ));
                }
                if !fall_sides.contains(&side) {
                    fall_sides.push(side);
                }
            }
        }
        // Still air reached as the wind falls from each side (2026-11-17):
        // the jumpers' shapes then (their other shapes alone), held by the
        // band, checked with the steps.
        if options.jumpers {
            for side in fall_sides {
                let mut at = blown(case, side);
                at.wind_pressure = ohl_core::units::Stress::from_si(0.0);
                let mut j = below(
                    job(
                        c,
                        step_kind(side),
                        at,
                        Some(0.0),
                        options.wind_steps.is_none(),
                    ),
                    side,
                    full,
                );
                j.fallen_only = true;
                jobs.push(j);
            }
        }
    }
    jobs
}

/// How many line solves [`solve_positions`] asks for with `options` and how
/// many it makes, each distinct solve once ([`solve_key`]; 2026-11-16) —
/// before the steps of spans the band does not hold, which it adds when it
/// finds them. Nothing is solved here.
pub fn solve_count(cases: &[WeatherCase], options: &PositionOptions) -> (usize, usize) {
    let jobs = jobs_of(cases, options);
    let keys: std::collections::HashSet<String> =
        jobs.iter().map(|j| solve_key(&j.solve)).collect();
    (jobs.len(), keys.len())
}

/// Solves every case of `cases` — a still case as it is, a windy one from
/// the left and from the right (and in still air and at the swing band's
/// levels for the swing band; with the wind stepped, in still air and at
/// every intermediate pressure from the sides chosen) — in parallel, and
/// gathers the positions of every span. Sections that do not solve are
/// left out (noted).
///
/// Each solve is made once (2026-11-16, [`solve_key`]): cases differing
/// by their wind pressure alone ([`family_key`]) share their solves at
/// the same pressures — the steps of the one of largest pressure are the
/// others' steps and full blowouts — bit for bit what solving each alone
/// gives.
///
/// A span whose band does not hold its wire (the wind lifts it above its
/// axis, [`SwingArc::above_axis`], or a position does not run steadily
/// along it) is also checked at its wind steps (2026-11-16: every
/// [`BAND_LEVEL_ROUNDING`], 10 Pa, from both sides, solved for its case
/// when the steps are not; noted).
///
/// Cost: one line solve per distinct case, side and pressure, each
/// typically well under a second; exit clause: `budget` (its time limit,
/// checked between solves).
pub fn solve_positions(
    line: &Line,
    library: &ohl_cables::Library,
    cases: &[WeatherCase],
    options: PositionOptions,
    budget: &Budget,
) -> Result<Positions, String> {
    let jobs = jobs_of(cases, &options);
    let job = |case, kind, solve, pressure, band_only| Job {
        case,
        kind,
        solve,
        pressure,
        band_only,
        side: None,
        top: None,
        fallen_only: false,
    };
    let mut solves = Solves::default();
    let of_job = solves.run(line, library, &jobs, options.jumpers, budget)?;
    let mut out = Positions {
        cases: cases.iter().map(|c| c.name.clone()).collect(),
        directions: cases.iter().map(WeatherCase::oblique).collect(),
        ..Positions::default()
    };
    for (j, job) in jobs.iter().enumerate() {
        for f in &solves.made[of_job[j]].failed {
            out.notes.push(format!(
                "case \"{}\" ({}): {f}",
                job.solve.name,
                job.kind.name()
            ));
        }
    }
    // The spans by (section, span).
    let mut span_index: std::collections::HashMap<(String, usize), usize> =
        std::collections::HashMap::new();
    let mut by_case: Vec<Vec<usize>> = vec![Vec::new(); cases.len()];
    for (j, job) in jobs.iter().enumerate() {
        by_case[job.case].push(j);
    }
    // Per case, the spans the band does not hold.
    let mut unheld: Vec<(usize, Vec<usize>)> = Vec::new();
    // The largest step between the solved pressures of a case banded.
    let mut band_gap: f64 = 0.0;
    for (c, list) in by_case.iter().enumerate() {
        let windy = is_windy(&cases[c]);
        let solve_of = |i: usize| &solves.made[of_job[list[i]]];
        let mut span_ids: Vec<Vec<Option<usize>>> = Vec::new();
        for (i, &j) in list.iter().enumerate() {
            let job = &jobs[j];
            let solve = solve_of(i);
            let mut ids = Vec::new();
            for w in &solve.wires {
                let jumper = w.section.starts_with("jumper:");
                let key = (w.section.clone(), w.span);
                let id = match span_index.get(&key) {
                    Some(&id) => Some(id),
                    // A jumper: about the line through its ends.
                    None if jumper => match (w.points.first(), w.points.last()) {
                        (Some(a), Some(b)) => {
                            Axis::new(*a, *b).map(|axis| out.add_span(w.clone(), axis))
                        }
                        _ => None,
                    },
                    None => solve
                        .sections
                        .shapes
                        .iter()
                        .find(|s| s.section == w.section)
                        .and_then(|s| span_axis(s, w.span))
                        .map(|axis| out.add_span(w.clone(), axis)),
                };
                if let Some(id) = id {
                    span_index.insert(key, id);
                }
                ids.push(id);
                let Some(id) = id else { continue };
                // Still air of a windy case solved for the arc, and the
                // band's levels: for the band only (a jumper's too; kept
                // below for a jumper without one).
                if (windy && job.kind == PositionKind::Case) || job.band_only {
                    continue;
                }
                let axis = out.spans[id].axis;
                out.add_at(
                    id,
                    c,
                    job.kind,
                    Shape::Wire(WireShape::new(w.clone(), &axis)),
                    job.pressure,
                );
            }
            span_ids.push(ids);
        }
        if !(windy && options.swing_arc) {
            continue;
        }
        let find = |k: PositionKind| list.iter().position(|&j| jobs[j].kind == k);
        // (A wind along the line has one side: its band from still air to
        // that blowout.)
        let (Some(l), Some(s), Some(r)) = (
            find(PositionKind::Left),
            find(PositionKind::Case).or_else(|| find(PositionKind::Still)),
            find(PositionKind::Right).or_else(|| find(PositionKind::Left)),
        ) else {
            continue;
        };
        // Jumpers whose shape jumps between the band's pressures
        // (2026-11-17): solved every 10 Pa over that interval alone, their
        // bands holding those shapes too — unless the wind is stepped at
        // 10 Pa already.
        let mut swapped: Vec<(usize, Wire)> = Vec::new();
        let fine = options
            .wind_steps
            .as_ref()
            .is_some_and(|w| w.step <= crate::jumper_swaps::SWAP_STEP + 1e-9);
        if options.jumpers && !fine {
            let full = cases[c].wind_pressure.si();
            let mut solved: Vec<(WindSide, f64)> =
                vec![(WindSide::Right, 0.0), (WindSide::Left, 0.0)];
            let mut swaps = Vec::new();
            for (i, &j) in list.iter().enumerate() {
                let Some(side) = jobs[j].side else { continue };
                if !jobs[j].fallen_only {
                    solved.push((side, jobs[j].pressure.unwrap_or(full)));
                }
                for (id, at) in &solve_of(i).jumps {
                    for &x in at {
                        swaps.push(crate::jumper_swaps::Swap {
                            side,
                            jumper: id.clone(),
                            at: x,
                        });
                    }
                }
            }
            // Neighbouring solved pressures whose jumper shapes do not
            // lead to each other.
            swaps.extend(cross_of(line, library, &jobs, list, &solve_of, budget)?);
            let wanted = crate::jumper_swaps::swap_pressures(&solved, &swaps);
            if !wanted.is_empty() {
                let case = &cases[c];
                let more = crate::jumper_swaps::solve_swaps(
                    line,
                    library,
                    &wanted,
                    full,
                    |side, p| {
                        let mut at = blown(case, side);
                        at.wind_pressure = ohl_core::units::Stress::from_si(p);
                        at
                    },
                    budget,
                )?;
                for m in &more {
                    for f in &m.failed {
                        out.notes.push(format!(
                            "case \"{}\" ({:.0} Pa from the {}, a jumper's band): {f}",
                            case.name,
                            m.pressure,
                            m.side.as_str()
                        ));
                    }
                    for w in wires_of(line, &[], &m.jumpers) {
                        if let Some(&id) = span_index.get(&(w.section.clone(), w.span)) {
                            swapped.push((id, w));
                        }
                    }
                }
                if let Some(n) = crate::jumper_swaps::swaps_note(&case.name, &wanted) {
                    out.notes.push(n);
                }
            }
        }
        // The levels of each span: every intermediate solve (the band's
        // levels, and the steps when solved).
        let mut levels_of: std::collections::HashMap<usize, Vec<&Wire>> =
            std::collections::HashMap::new();
        for (id, w) in &swapped {
            levels_of.entry(*id).or_default().push(w);
        }
        for (i, &j) in list.iter().enumerate() {
            if !matches!(
                jobs[j].kind,
                PositionKind::StepLeft | PositionKind::StepRight
            ) {
                continue;
            }
            for (w, id) in solve_of(i).wires.iter().zip(&span_ids[i]) {
                if let Some(id) = id {
                    levels_of.entry(*id).or_default().push(w);
                }
            }
        }
        let (mut left_out, mut jumpers_out) = (0, 0);
        let mut not_held: Vec<usize> = Vec::new();
        let mut lifted = 0;
        // Still air solved for the band only (not a step).
        let still_only = jobs[list[s]].kind == PositionKind::Case;
        for (w, id) in solve_of(s).wires.iter().zip(&span_ids[s]) {
            let Some(id) = *id else { continue };
            // A jumper: its own band about the line through its end
            // clamps (2026-10-21); without one, its still air as solved.
            let jumper = w.section.starts_with("jumper:");
            let pick = |k: usize| {
                solve_of(k)
                    .wires
                    .iter()
                    .find(|x| x.section == w.section && x.span == w.span)
            };
            let levels: &[&Wire] = levels_of.get(&id).map_or(&[], Vec::as_slice);
            let axis = out.spans[id].axis;
            let shape = |x: &Wire| WireShape::new(x.clone(), &axis);
            let arc = match (pick(l), pick(r)) {
                // About its end clamps blown to the left: the span's axis
                // (it was made from that solve, the first).
                (Some(wl), Some(wr)) if jumper => {
                    jumper_band_levels(Some(wl), w, Some(wr), levels, options.jumper_band_margin)
                }
                (Some(wl), Some(wr)) => {
                    let shapes: Vec<WireShape> = levels.iter().map(|x| shape(x)).collect();
                    let refs: Vec<&WireShape> = shapes.iter().collect();
                    SwingArc::with_levels(
                        axis,
                        &shape(wl),
                        &shape(w),
                        &shape(wr),
                        &refs,
                        options.band_margin,
                    )
                }
                _ => None,
            };
            match arc {
                Some(arc) => {
                    if !jumper && arc.above_axis() {
                        lifted += 1;
                        not_held.push(id);
                    }
                    band_gap =
                        band_gap.max(options.band_levels.largest_gap(cases[c].wind_pressure.si()));
                    out.add(id, c, PositionKind::Arc, Shape::Arc(arc));
                }
                None if jumper => {
                    jumpers_out += 1;
                    if still_only {
                        out.add_at(id, c, PositionKind::Still, Shape::Wire(shape(w)), None);
                    }
                }
                None => {
                    left_out += 1;
                    not_held.push(id);
                }
            }
        }
        let stepped = options.wind_steps.is_some();
        let checked = if stepped {
            "checked at the wind steps solved"
        } else {
            "checked at their wind steps (every 10 Pa from both sides) instead"
        };
        // Jumpers that snapped over as the wind rose (2026-11-16): checked
        // in the shape they keep as it falls again where every solve
        // below the snap solved their falling wind (2026-11-17), else said.
        let mut snaps: Vec<(String, f64)> = Vec::new();
        for i in 0..list.len() {
            for (j, p) in &solve_of(i).snaps {
                match snaps.iter_mut().find(|(k, _)| k == j) {
                    Some(x) => x.1 = x.1.min(*p),
                    None => snaps.push((j.clone(), *p)),
                }
            }
        }
        let below: Vec<(f64, &Vec<String>, bool)> = (0..list.len())
            .filter(|&i| jobs[list[i]].top.is_some())
            .map(|i| {
                let job = &jobs[list[i]];
                (
                    job.pressure.unwrap_or(0.0),
                    &solve_of(i).fell,
                    job.fallen_only,
                )
            })
            .collect();
        let (held, open) = falling_covers(&snaps, &below);
        if let Some(n) = jumper_snap_note(&cases[c].name, &open) {
            out.notes.push(n);
        }
        if let Some(n) = jumper_snap_held_note(&cases[c].name, &held) {
            out.notes.push(n);
        }
        if jumpers_out > 0 {
            out.notes.push(format!(
                "case \"{}\": {jumpers_out} jumpers without a swing band (not solved blown to both sides, or not running steadily along the line through their ends): taken at their solved positions",
                cases[c].name
            ));
        }
        if left_out > 0 {
            out.notes.push(format!(
                "case \"{}\": {left_out} spans without a swing arc (a wire not solved, or not running steadily along its span): {checked}",
                cases[c].name
            ));
        }
        if lifted > 0 {
            out.notes.push(format!(
                "case \"{}\": {lifted} spans whose wire the wind pulls to or above the line between its clamps (the swing band does not hold such a wire between its levels): {checked}",
                cases[c].name
            ));
        }
        if !not_held.is_empty() && !stepped {
            unheld.push((c, not_held));
        }
    }
    // Jumpers with a second stable shape in still air (2026-11-17).
    let mut others: Vec<(String, f64, bool)> = Vec::new();
    for list in &by_case {
        let fallen: Vec<&(String, Vec<P3>)> = list
            .iter()
            .filter(|&&j| jobs[j].fallen_only)
            .flat_map(|&j| &solves.made[of_job[j]].fallen)
            .collect();
        for &j in list {
            for (id, other, apart) in &solves.made[of_job[j]].other_still {
                let reached = fallen.iter().any(|(k, f)| k == id && same_shape(f, other));
                match others.iter_mut().find(|x| &x.0 == id) {
                    Some(x) => {
                        x.1 = x.1.max(*apart);
                        x.2 &= reached;
                    }
                    None => others.push((id.clone(), *apart, reached)),
                }
            }
        }
    }
    out.notes.extend(other_still_notes(&others));
    if options.wind_steps.is_none()
        && let Some(n) = band_levels_note(&options.band_levels, band_gap)
    {
        out.notes.push(n);
    }
    // The spans the band does not hold: their wind steps (2026-11-16).
    if !unheld.is_empty() {
        let steps = crate::rules::WindSteps {
            stepped: true,
            step: BAND_LEVEL_ROUNDING,
            ..Default::default()
        };
        let mut more: Vec<Job> = Vec::new();
        for (c, _) in &unheld {
            let case = &cases[*c];
            more.push(job(*c, PositionKind::Still, still(case), Some(0.0), false));
            let full = case.wind_pressure.si();
            for p in steps.pressures(full, crate::zone::MAX_STEPS) {
                if p <= 0.0 || p >= full {
                    continue;
                }
                for &side in full_sides(case) {
                    let mut at = blown(case, side);
                    at.wind_pressure = ohl_core::units::Stress::from_si(p);
                    more.push(job(*c, step_kind(side), at, Some(p), false));
                }
            }
        }
        let of_more = solves.run(line, library, &more, options.jumpers, budget)?;
        for (j, job) in more.iter().enumerate() {
            let Some((_, spans)) = unheld.iter().find(|(c, _)| *c == job.case) else {
                continue;
            };
            for w in &solves.made[of_more[j]].wires {
                let Some(&id) = span_index.get(&(w.section.clone(), w.span)) else {
                    continue;
                };
                if !spans.contains(&id) {
                    continue;
                }
                let axis = out.spans[id].axis;
                out.add_at(
                    id,
                    job.case,
                    job.kind,
                    Shape::Wire(WireShape::new(w.clone(), &axis)),
                    job.pressure,
                );
            }
        }
    }
    out.solves = (solves.asked, solves.sections.len());
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
                for &side in full_sides(case) {
                    let kind = match side {
                        WindSide::Right => PositionKind::IceLeft,
                        WindSide::Left => PositionKind::IceRight,
                    };
                    jobs.push((id, c, kind, blown(case, side)));
                }
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
    /// mid-span (s = 50 m) the band runs from r = 4 (still air) to R = 5
    /// (blown), from −40° to +40°. A point straight below the axis 8 m
    /// down is at φ = 0, ρ = 8: 3 m outside the band. A point 6 m out to
    /// the right at the axis height (φ = 90°) is beyond the band's end:
    /// its nearest point is on the end edge at 40° (radii 4…5); projected
    /// on the edge's direction (cos 40°, sin 40°) it lies at
    /// 6 sin 40° = 3.857 < 4, so the nearest point is the edge's inner end
    /// (4 cos 40° = 3.064 below, 4 sin 40° = 2.571 right):
    /// √(3.064² + (6 − 2.571)²) = 4.598 (the arc alone: 4.736 to its
    /// end at R = 5).
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
        assert!((place.inner - 4.0).abs() < 1e-12);
        assert!((place.from + phi).abs() < 1e-12 && (place.to - phi).abs() < 1e-12);
        let (d, p) = arc.nearest([50.0, 0.0, 12.0]);
        assert!((d - 3.0).abs() < 1e-12, "{d}");
        assert!(distance(p, [50.0, 0.0, 15.0]) < 1e-12);
        // To the right is −y.
        let (d, _) = arc.nearest([50.0, -6.0, 20.0]);
        let inner_end = [4.0 * phi.cos(), 4.0 * phi.sin()];
        assert!(
            (d - inner_end[0].hypot(6.0 - inner_end[1])).abs() < 1e-12,
            "{d}"
        );
        assert!((d - 4.598).abs() < 1e-3, "{d}");
        // Within the band (ρ = 4.5 at φ = 10°): 0.
        let a = 10f64.to_radians();
        let (d, _) = arc.nearest([50.0, -4.5 * a.sin(), 20.0 - 4.5 * a.cos()]);
        assert!(d.abs() < 1e-12, "{d}");
        // Between the axis and the band at 25 m: R = 4·5·0.25·0.75 =
        // 3.75, r = 4·4·0.25·0.75 = 3.0; a point 1 m below the axis at
        // φ = 20° is r − 1 = 2.0 m from it (the arc alone: 2.75).
        let a = 20f64.to_radians();
        let q = [25.0, -a.sin(), 20.0 - a.cos()];
        let (d, _) = arc.nearest(q);
        assert!((d - 2.0).abs() < 1e-12, "{d}");
        // Beyond the span's end: the end's plane, the part along the axis
        // counted.
        let (d, _) = arc.nearest([-3.0, 0.0, 16.0]);
        assert!((d - 5.0).abs() < 1e-12, "{d}");
        // Vertically, under mid-span within 0.5 m across the axis: the
        // band's lowest point is at R = 5 straight down (15 m); its
        // highest within the strip |ρ sin φ| ≤ 0.5 is on the inner circle
        // at sin φ = 0.5/4 (the 40° edges cross the strip only at
        // ρ = 0.5 / sin 40° = 0.778 < r, outside the band):
        // 20 − 4 cos(asin 0.125) = 16.031 m.
        let v = arc.vertical([50.0, 0.0, 10.0], 0.5, 0.0).unwrap();
        assert!((v.gap - 5.0).abs() < 1e-9, "{v:?}");
        assert!(!v.above);
        let v = arc.vertical([50.0, 0.0, 16.5], 0.5, 0.0).unwrap();
        assert!(
            v.above,
            "above the band's highest: inside a column down to the ground"
        );
        let top = 20.0 - 4.0 * (0.125f64).asin().cos();
        assert!((top - 16.031).abs() < 1e-3);
        assert!((v.gap - (16.5 - top)).abs() < 1e-9, "{v:?} {top}");
        let v = arc.vertical([50.0, 0.0, 15.5], 0.5, 0.0).unwrap();
        assert_eq!(v.gap, 0.0);
        // The radius taken off vertically (2026-11-08): r on a level span;
        // on an axis sloping at cos β = 0.8, r / 0.8 (30 mm → 37.5 mm).
        let mut sloped = arc.clone();
        sloped.outer = 0.03;
        assert!((sloped.vertical_outer() - 0.03).abs() < 1e-15);
        sloped.axis.u = [0.8, 0.0, 0.6];
        assert!((sloped.vertical_outer() - 0.0375).abs() < 1e-15);
    }

    /// Jumpers that snapped over as the wind rose (2026-11-16): said per
    /// case, lowest pressure first, not on the safe side; nothing without.
    #[test]
    fn a_jumper_that_snapped_over_is_said() {
        assert!(jumper_snap_note("storm", &[]).is_none());
        let n = jumper_snap_note("storm", &[("J2".into(), 1484.4), ("J1".into(), 620.0)]).unwrap();
        assert!(
            n.starts_with("NOT ON THE SAFE SIDE: case \"storm\": 2 jumper(s)"),
            "{n}"
        );
        assert!(
            n.contains("(J1 at about 620 Pa, J2 at about 1484 Pa)"),
            "{n}"
        );
    }

    /// A snapped jumper is held by the falling wind (2026-11-17) when
    /// still air reached falling solved its falling wind and every solve
    /// below its snap did too; a solve below it without (its falling wind
    /// failed), or no still air reached falling: still said not on the
    /// safe side. Held ones are said, as checked.
    #[test]
    fn a_snapped_jumper_is_held_where_the_falling_wind_was_solved() {
        let j = vec!["J1".to_owned()];
        let none: Vec<String> = Vec::new();
        let snaps = [("J1".to_owned(), 150.0)];
        // Still air falling, 100 Pa (both with J1), 200 Pa (above: no
        // matter).
        let all = [(0.0, &j, true), (100.0, &j, false), (200.0, &none, false)];
        let (held, open) = falling_covers(&snaps, &all);
        assert_eq!((held.len(), open.len()), (1, 0));
        let n = jumper_snap_held_note("storm", &held).unwrap();
        assert!(
            !n.contains("NOT ON THE SAFE SIDE") && n.contains("J1 at about 150 Pa"),
            "{n}"
        );
        // The 100 Pa solve without its falling wind.
        let gap = [(0.0, &j, true), (100.0, &none, false)];
        assert_eq!(falling_covers(&snaps, &gap).1.len(), 1);
        // No still air reached falling.
        let no_still = [(100.0, &j, false)];
        assert_eq!(falling_covers(&snaps, &no_still).1.len(), 1);
        assert!(jumper_snap_held_note("storm", &[]).is_none());
    }

    /// Jumpers with a second stable shape in still air (2026-11-17): not
    /// reached by any wind solved, not on the safe side; reached (the
    /// still air after a wind from a side), said as checked.
    #[test]
    fn a_second_still_air_shape_is_said() {
        assert!(other_still_notes(&[]).is_empty());
        let n = other_still_notes(&[("J1".into(), 2.61, false), ("J2".into(), 0.4, true)]);
        assert_eq!(n.len(), 2);
        assert!(
            n[0].starts_with("NOT ON THE SAFE SIDE: 1 jumper(s)"),
            "{}",
            n[0]
        );
        assert!(n[0].contains("J1, up to 2.61 m apart"), "{}", n[0]);
        assert!(n[0].contains("hung from its clamps"), "{}", n[0]);
        assert!(n[1].contains("J2, up to 0.40 m apart") && !n[1].contains("NOT ON"));
    }

    /// The band's levels (2026-11-16): at most `spacing` apart, evenly,
    /// k·full/(m + 1) rounded to whole 10 Pa, each once, strictly between
    /// 0 and the full pressure; an older file's count (2026-11-16) gives
    /// the same levels as then.
    #[test]
    fn band_levels_are_evenly_spaced_rounded_fractions() {
        let every = BandLevels::DEFAULT;
        assert_eq!(every.spacing, 200.0);
        // ⌈500/200⌉ − 1 = 2: 166.7 → 170, 333.3 → 330.
        assert_eq!(every.at(500.0), vec![170.0, 330.0]);
        assert_eq!(every.at(300.0), vec![150.0]);
        assert_eq!(every.at(200.0), Vec::<f64>::new());
        // From 1000 Pa up the same as the default of 2026-11-16 (4 a
        // side, at least one every 200 Pa).
        for full in [1000.0, 1300.0, 1450.0, 2400.0, 1001.0] {
            assert_eq!(every.at(full), band_levels(full, 4), "{full}");
        }
        assert_eq!(every.at(1000.0), vec![200.0, 400.0, 600.0, 800.0]);
        assert_eq!(
            every.at(1300.0),
            vec![190.0, 370.0, 560.0, 740.0, 930.0, 1110.0]
        );
        // Below 1000 Pa fewer than then (4 a side).
        assert_eq!(band_levels(500.0, 4), vec![100.0, 200.0, 300.0, 400.0]);
        // The old count: at least one every 200 Pa whatever asked.
        assert_eq!(
            band_levels(1300.0, 3),
            vec![190.0, 370.0, 560.0, 740.0, 930.0, 1110.0]
        );
        assert_eq!(band_levels(1000.0, 3), vec![200.0, 400.0, 600.0, 800.0]);
        assert_eq!(BandLevels::from_count(0), BandLevels::NONE);
        // At most the spacing apart (and the rounding), however set.
        for spacing in [10.0, 60.0, 100.0, 150.0, 200.0, 250.0, 333.0] {
            for full in [30.0, 95.0, 300.0, 595.0, 650.0, 1450.0, 2400.0] {
                let l = BandLevels::every(spacing);
                let v = l.at(full);
                assert!(
                    l.largest_gap(full) <= spacing + 10.0,
                    "{spacing} {full} {v:?}"
                );
                assert!(v.windows(2).all(|w| w[0] < w[1]), "{v:?}");
            }
        }
        assert_eq!(
            BandLevels::every(100.0).at(595.0),
            vec![100.0, 200.0, 300.0, 400.0, 500.0]
        );
        // Few pressures below the full one: each once.
        assert_eq!(BandLevels::every(10.0).at(30.0), vec![10.0, 20.0]);
        assert_eq!(band_levels(30.0, 8), vec![10.0, 20.0]);
        assert!(BandLevels::NONE.at(500.0).is_empty());
        assert!(every.at(0.0).is_empty());
        assert!(every.at(f64::NAN).is_empty());
        // The levels are steps of 10 Pa, bit for bit.
        for p in BandLevels::every(50.0).at(1450.0) {
            let k = (p / 10.0).round() as i64;
            assert_eq!(p, k as f64 * 10.0);
        }
        // The largest step: still air to the first level, … to the full.
        assert_eq!(largest_gap(500.0, &[170.0, 330.0]), 170.0);
        assert_eq!(largest_gap(500.0, &[]), 500.0);
        // The settings' bounds.
        assert!(BandLevels::DEFAULT.problem().is_none());
        assert!(BandLevels::NONE.problem().is_none());
        assert!(BandLevels::every(5.0).problem().is_some());
        assert!(BandLevels::every(-1.0).problem().is_some());
        assert!(BandLevels::every(f64::NAN).problem().is_some());
        assert!(BandLevels::from_count(51).problem().is_some());
        // Said where the solved pressures lie further apart than measured.
        assert!(band_levels_note(&BandLevels::DEFAULT, 210.0).is_none());
        assert!(band_levels_note(&BandLevels::NONE, 150.0).is_none());
        let n = band_levels_note(&BandLevels::NONE, 500.0).unwrap();
        assert!(
            n.starts_with("NOT ON THE SAFE SIDE: the swing band without levels"),
            "{n}"
        );
        let n = band_levels_note(&BandLevels::every(300.0), 300.0).unwrap();
        assert!(n.contains("up to 300 Pa apart"), "{n}");
    }

    /// A level inside the three positions' radii pulls the band's inner
    /// radius in (2026-11-16): still air sag 4 m, blown ±40° with sag 5 m,
    /// a level at 20° with sag 3.5 m — at mid-span r = 3.5, R = 5, the
    /// angles still ±40°; a level beyond 40° widens them. A level lifted
    /// above the axis marks the band ([`SwingArc::above_axis`]).
    #[test]
    fn the_band_holds_its_levels() {
        let ax = axis();
        let phi = 40f64.to_radians();
        let left = WireShape::new(wire(hanging(-phi, 5.0)), &ax);
        let still = WireShape::new(wire(hanging(0.0, 4.0)), &ax);
        let right = WireShape::new(wire(hanging(phi, 5.0)), &ax);
        let level = WireShape::new(wire(hanging(20f64.to_radians(), 3.5)), &ax);
        let arc = SwingArc::with_levels(ax, &left, &still, &right, &[&level], 0.0).unwrap();
        assert_eq!(arc.level_count(), 1);
        let place = arc.place(50.0);
        assert!((place.inner - 3.5).abs() < 1e-12, "{place:?}");
        assert!((place.radius - 5.0).abs() < 1e-12);
        assert!((place.from + phi).abs() < 1e-12 && (place.to - phi).abs() < 1e-12);
        // Between stations (52.5 m: halfway between the points at 50 and
        // 55 m), the levels' radius linearly: 3.5 · 4·0.525·0.475 / 1 …
        let r = |sag: f64, x: f64| 4.0 * sag * (x / 100.0) * (1.0 - x / 100.0);
        let p = arc.place(52.5);
        let lerp = (r(3.5, 50.0) + r(3.5, 55.0)) / 2.0;
        assert!((p.inner - lerp).abs() < 1e-12, "{p:?} {lerp}");
        // The level's own point there lies within the band.
        let a = 20f64.to_radians();
        let q = [52.5, -r(3.5, 52.5) * a.sin(), 20.0 - r(3.5, 52.5) * a.cos()];
        assert!(arc.nearest(q).0 < 1e-3, "{}", arc.nearest(q).0);
        // Without the level the point lies inside the inner radius.
        let plain = SwingArc::with_margin(ax, &left, &still, &right, 0.0).unwrap();
        assert!(plain.nearest(q).0 > 0.4);
        assert!(!arc.above_axis() && arc.lift() < 0.0 + 1e-9);
        let wide = WireShape::new(wire(hanging(60f64.to_radians(), 5.0)), &ax);
        let arc = SwingArc::with_levels(ax, &left, &still, &right, &[&wide], 0.0).unwrap();
        assert!((arc.place(50.0).to - 60f64.to_radians()).abs() < 1e-12);
        // A level 2 m above the axis at mid-span.
        let up = WireShape::new(wire(hanging(std::f64::consts::PI, 2.0)), &ax);
        let arc = SwingArc::with_levels(ax, &left, &still, &right, &[&up], 0.0).unwrap();
        assert!(arc.above_axis());
        assert!((arc.lift() - 2.0).abs() < 1e-12, "{}", arc.lift());
        // A level pulled towards its chord: sag 1.5 m against still air's
        // 4 m (less than half): the band does not hold it either.
        let taut = WireShape::new(wire(hanging(0.3, 1.5)), &ax);
        let arc = SwingArc::with_levels(ax, &left, &still, &right, &[&taut], 0.0).unwrap();
        assert!(
            (arc.sag_ratio() - 1.5 / 4.0).abs() < 1e-12,
            "{}",
            arc.sag_ratio()
        );
        assert!(arc.above_axis());
        let plain = SwingArc::with_levels(ax, &left, &still, &right, &[&level], 0.0).unwrap();
        assert!((plain.sag_ratio() - 3.5 / 4.0).abs() < 1e-12);
    }

    /// Cases differing by their wind pressure alone share their solves
    /// (2026-11-16): the example line's "high wind" at its pressure and a
    /// copy at 3/5 of it (rounded to 10 Pa), stepped by 10 Pa from both
    /// sides with the swing band — the smaller case's positions and band
    /// bit for bit those of solving it alone, the solves those of the
    /// larger case alone.
    #[test]
    fn cases_of_one_family_share_their_solves() {
        use std::path::Path;
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let line =
            Line::load(&root.join("examples/synthetic-line/synthetic-line.ohl.json")).unwrap();
        let library = ohl_cables::Library::load(&root.join("libraries/cables")).unwrap();
        let big = ohl_weather::WeatherCases::load(
            &root.join("examples/synthetic-line/weather-cases.ohlwc"),
        )
        .unwrap()
        .select(&["high wind".into()], &[])
        .unwrap()
        .remove(0);
        let full = big.wind_pressure.si();
        let mut small = big.clone();
        small.name = "smaller wind".into();
        small.wind_pressure = ohl_core::units::Stress::from_si((full * 0.6 / 10.0).round() * 10.0);
        assert_eq!(family_key(&small), family_key(&big));
        assert_ne!(solve_key(&small), solve_key(&big));
        let mut other = big.clone();
        other.temperature = ohl_core::units::Temperature::from_celsius(41.0);
        assert_ne!(family_key(&other), family_key(&big));
        let options = PositionOptions {
            swing_arc: true,
            wind_steps: Some(crate::rules::WindSteps {
                stepped: true,
                step: 10.0,
                sides: crate::rules::WindSides::Both,
                ..Default::default()
            }),
            jumpers: true,
            ..Default::default()
        };
        let alone = solve_positions(
            &line,
            &library,
            std::slice::from_ref(&small),
            options,
            &Budget::new(),
        )
        .unwrap();
        let big_alone = solve_positions(
            &line,
            &library,
            std::slice::from_ref(&big),
            options,
            &Budget::new(),
        )
        .unwrap();
        let both = solve_positions(
            &line,
            &library,
            &[small.clone(), big.clone()],
            options,
            &Budget::new(),
        )
        .unwrap();
        // Every solve of the smaller case is one of the larger's.
        assert_eq!(both.solves.1, big_alone.solves.1, "{:?}", both.solves);
        assert_eq!(both.solves.0, alone.solves.0 + big_alone.solves.0);
        assert_eq!(
            solve_count(&[small.clone(), big.clone()], &options),
            both.solves
        );
        // The smaller case's positions (and band) as alone, bit for bit:
        // its first solve gives the spans' axes in both runs.
        let of = |p: &Positions,
                  case: usize|
         -> Vec<(String, usize, PositionKind, Option<u64>, Shape)> {
            p.positions
                .iter()
                .filter(|x| x.case == case)
                .map(|x| {
                    let w = &p.spans[x.span].wire;
                    (
                        w.section.clone(),
                        w.span,
                        x.kind,
                        x.pressure.map(f64::to_bits),
                        x.shape.clone(),
                    )
                })
                .collect()
        };
        let a = of(&alone, 0);
        assert!(a.len() > 100, "{}", a.len());
        assert!(a.iter().any(|x| x.2 == PositionKind::Arc));
        assert!(
            a == of(&both, 0),
            "the smaller case differs from solving it alone"
        );
        // The larger case's wire positions as alone too (its bands about
        // the axes of the run's first solve: compared by their wires).
        let wires = |v: Vec<(String, usize, PositionKind, Option<u64>, Shape)>| {
            v.into_iter()
                .filter(|x| matches!(x.4, Shape::Wire(_)))
                .collect::<Vec<_>>()
                .into_iter()
                .map(|x| match x.4 {
                    Shape::Wire(w) => (x.0, x.1, x.2, x.3, w.wire.points),
                    Shape::Arc(_) => unreachable!(),
                })
                .collect::<Vec<_>>()
        };
        assert!(wires(of(&big_alone, 0)) == wires(of(&both, 1)));
        // The two copies of one wind across the line, from 90° and from
        // 270°, share every solve (each is solved from both sides, and
        // blown from one side the two are the same solve: the solve reads
        // the side across the line, not the direction): each solved alone
        // gives the other's positions bit for bit (still air too: without
        // wind its side is not in the key since 2026-11-17).
        let band = PositionOptions {
            swing_arc: true,
            ..Default::default()
        };
        let right = WeatherCase {
            wind_direction: Some(ohl_weather::WindDirection::Right),
            wind_from: WindSide::Right,
            ..big.clone()
        };
        let left = WeatherCase {
            name: "the mirror copy".into(),
            wind_direction: Some(ohl_weather::WindDirection::Left),
            wind_from: WindSide::Left,
            ..big.clone()
        };
        let one = solve_count(std::slice::from_ref(&right), &band);
        // (Every blown solve, and still air: its side unread without wind
        // and out of the key since 2026-11-17 — before, one more solve.)
        assert_eq!(solve_count(&[right.clone(), left.clone()], &band).1, one.1);
        let r = solve_positions(&line, &library, &[right], band, &Budget::new()).unwrap();
        let l = solve_positions(&line, &library, &[left], band, &Budget::new()).unwrap();
        assert!(wires(of(&r, 0)) == wires(of(&l, 0)));
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
    /// The overrides keep a span's positions of the cases its section
    /// takes only, every span's position list renumbered (owner,
    /// 2026-10-25).
    #[test]
    fn positions_keep_the_cases_their_spans_take() {
        use std::path::Path;
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let line =
            Line::load(&root.join("examples/synthetic-line/synthetic-line.ohl.json")).unwrap();
        let library = ohl_cables::Library::load(&root.join("libraries/cables")).unwrap();
        let cases = ohl_weather::WeatherCases::load(
            &root.join("examples/synthetic-line/weather-cases.ohlwc"),
        )
        .unwrap()
        .select(&["everyday".into(), "glaze ice".into()], &[])
        .unwrap();
        let mut p = solve_positions(
            &line,
            &library,
            &cases,
            PositionOptions::default(),
            &Budget::new(),
        )
        .unwrap();
        let before = p.positions.len();
        let groups = vec![ohl_weather::CaseGroup {
            name: "warm".into(),
            description: String::new(),
            cases: vec!["everyday".into()],
            ..Default::default()
        }];
        let warm = ["warm".to_owned()];
        let filter = ohl_weather::CaseFilter {
            sections: Some(ohl_weather::OwnCases::new([("S1", &warm[..])], &groups)),
            structures: None,
        };
        p.retain_cases(&line, &filter);
        assert!(p.positions.len() < before);
        for s in &p.spans {
            for &k in &s.positions {
                let x = &p.positions[k];
                assert!(std::ptr::eq(&p.spans[x.span], s), "renumbered");
                if s.wire.section == "S1" {
                    assert_eq!(p.cases[x.case], "everyday");
                }
            }
        }
    }

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
            arc: false,
            ..Default::default()
        };
        let p = solve_positions(
            &line,
            &library,
            &cases,
            PositionOptions {
                swing_arc: false,
                wind_steps: Some(steps),
                jumpers: false,
                ..Default::default()
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

    /// A wind at an angle from each side (2026-11-16): from the right the
    /// direction coming from the right half, from the left its mirror
    /// (360° − θ) — until then both sides were the case as it is (only the
    /// side was set, which a wind at an angle does not read), its mirror
    /// never checked. Along the line one side only; across and without a
    /// direction as before (the side). On the example line a 30° case's
    /// two blowouts lie to the two sides of the axis, the mirror the
    /// 330° case solved as such bit for bit; a 0° case gives one blowout
    /// and its arc.
    #[test]
    fn a_wind_at_an_angle_is_checked_with_its_mirror() {
        use ohl_weather::WindDirection;
        use std::path::Path;
        let base = ohl_weather::WeatherCase {
            wind_pressure: ohl_core::units::Stress::from_pascals(600.0),
            ..WeatherCase::still("w", ohl_core::units::Temperature::from_celsius(10.0))
        };
        let at = |a: f64| base.with_direction(WindDirection::Angle(a));
        let dir = |c: &WeatherCase| c.oblique().map(WindDirection::degrees);
        for (a, right, left) in [
            (30.0, 30.0, 330.0),
            (330.0, 30.0, 330.0),
            (150.0, 150.0, 210.0),
        ] {
            assert_eq!(dir(&from_side(&at(a), WindSide::Right)), Some(right), "{a}");
            assert_eq!(dir(&from_side(&at(a), WindSide::Left)), Some(left), "{a}");
        }
        for a in [0.0, 180.0] {
            assert!(one_side_only(&at(a)));
            let dir_of = |side| from_side(&at(a), side).oblique();
            assert_eq!(dir_of(WindSide::Left), dir_of(WindSide::Right));
        }
        assert_eq!(own_from(&at(30.0)), Some(WindSide::Right));
        assert_eq!(own_from(&at(200.0)), Some(WindSide::Left));
        assert_eq!(own_from(&base), None);
        // Across and without a direction: the side, as before.
        let left = from_side(&base, WindSide::Left);
        assert_eq!(
            (left.wind_from, left.wind_direction),
            (WindSide::Left, None)
        );
        let own = base.with_direction(WindDirection::Angle(270.0));
        assert_eq!(own_from(&own), None);
        let own = ohl_weather::WeatherCase {
            wind_direction: Some(WindDirection::Left),
            wind_from: WindSide::Left,
            ..base.clone()
        };
        assert_eq!(own_from(&own), Some(WindSide::Left));
        assert_eq!(from_side(&own, WindSide::Right).wind_from, WindSide::Right);
        assert!(!one_side_only(&own) && !one_side_only(&base));

        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let line =
            Line::load(&root.join("examples/synthetic-line/synthetic-line.ohl.json")).unwrap();
        let library = ohl_cables::Library::load(&root.join("libraries/cables")).unwrap();
        let options = PositionOptions {
            swing_arc: true,
            ..Default::default()
        };
        let p = solve_positions(
            &line,
            &library,
            &[at(30.0), at(0.0)],
            options,
            &Budget::new(),
        )
        .unwrap();
        assert!(p.notes.is_empty(), "{:?}", p.notes);
        let s = p.span_of("S1", 1).unwrap();
        let of = |case: usize, kind: PositionKind| {
            p.spans[s]
                .positions
                .iter()
                .copied()
                .find(|&i| p.positions[i].case == case && p.positions[i].kind == kind)
        };
        // The largest offset to the right of the axis, signed.
        let offset = |i: usize| match &p.positions[i].shape {
            Shape::Wire(w) => w
                .wire
                .points
                .iter()
                .map(|q| {
                    let a = &p.spans[s].axis;
                    let v = sub(*q, a.a1);
                    dot(v, a.right)
                })
                .fold(0.0f64, |m, y| if y.abs() > m.abs() { y } else { m }),
            Shape::Arc(_) => unreachable!(),
        };
        let (l, r) = (
            of(0, PositionKind::Left).unwrap(),
            of(0, PositionKind::Right).unwrap(),
        );
        assert!(
            offset(l) < -0.5 && offset(r) > 0.5,
            "{} {}",
            offset(l),
            offset(r)
        );
        assert_eq!(p.describe(l), "w, left (from 30°)");
        assert_eq!(p.describe(r), "w, right (from 330°)");
        // The mirror: the 330° case solved as such, bit for bit.
        let mirror = solve_positions(
            &line,
            &library,
            &[at(330.0)],
            PositionOptions::default(),
            &Budget::new(),
        )
        .unwrap();
        let s2 = mirror.span_of("S1", 1).unwrap();
        let k = mirror.spans[s2]
            .positions
            .iter()
            .copied()
            .find(|&i| mirror.positions[i].kind == PositionKind::Right)
            .unwrap();
        assert_eq!(mirror.positions[k].shape, p.positions[r].shape);
        assert!(of(0, PositionKind::Arc).is_some());
        // Along the line: one blowout, its arc.
        assert!(of(1, PositionKind::Left).is_some() && of(1, PositionKind::Right).is_none());
        assert!(of(1, PositionKind::Arc).is_some());
        assert_eq!(
            p.describe(of(1, PositionKind::Left).unwrap()),
            "w, left (from 0° (from ahead: blowing back))"
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
