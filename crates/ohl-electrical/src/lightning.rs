// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The **lightning shielding check** (owner, 2026-10-31; built, in active
//! development, not approved yet): whether the shielding wires keep
//! lightning off the phase conductors, in cross-sections square to each
//! span every `step` m, the wires as solved in a weather case — by a
//! **fixed shielding angle** or by the **rolling sphere** of the
//! electrogeometric model. The rules are
//! [`ohl_clearance::lightning::LightningRule`]s.
//!
//! **Cross-sections.** Each span's own vertical plane square to its chord
//! ([`cross_section::cross_section`] at a fraction of the span), at the
//! middle of each of the span's steps (n = ⌈length / step⌉ equal steps;
//! each stands for its step, the ends reaching the structures): the
//! wires where they cross it (as solved), the ground across it (the
//! terrain sampled every [`GROUND_STEP`] m, else straight between the
//! structures' feet and level across).
//!
//! **Shielding angle.** A phase is covered by a shielding wire above it
//! when the line from the wire to the phase leans from the vertical by at
//! most the rule's angle: the inner angle towards another shielding wire,
//! the outer one elsewhere. Measured to the far side of the phase's
//! bundle (its outer radius added across and taken off the height) —
//! **conservative**.
//!
//! **Rolling sphere.** A sphere of radius r (the striking distance to the
//! wires) rolls over the shielding wires and over the ground, which it
//! keeps β·r from. A phase is **exposed** when the sphere can touch its
//! surface: when some centre C of the *free region* — at least r from
//! every shielding wire's axis and at least β·r from the ground — lies
//! within r of the phase's surface. The ground's part of that region is
//! exactly the points above h(s) = max over the ground of
//! g(x) + √((βr)² − (x − s)²) (the terrain a graph); the region's point
//! nearest the phase is the phase itself, a foot on a shielding wire's
//! circle or on h, or a corner where they meet — so the test is exact,
//! with h tabulated every [`H_STEP`] m by chords (below the arcs: the
//! region taken larger, **conservative**). The shielding wires are taken
//! by their axis (the sphere may come nearer by their radius:
//! **conservative**), the phases by their surface; the other phases do
//! not shield (a strike to any phase is a failure).
//!
//! **Margin.** Shielding angle: the largest angle the phase needs along
//! the span, against the rule's. Rolling sphere: the largest striking
//! distance at which the phase is still exposed, r_max — at the phase's
//! place most exposed at the rule's distance (the deepest reach; one
//! search per phase and span, searched from [`R_MIN`] to [`R_MAX`] m by a
//! scan, then halving to 1 cm) — and the stroke current it belongs to
//! when the rule gives the distance by a current; the rule's r above
//! r_max: covered there.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use ohl_clearance::lightning::{LightningRule, Method};
use serde::Serialize;

use crate::cross_section::{self, Axis, LineSpan, Sources, StationAt};

/// The terrain's sampling across a cross-section, m.
pub const GROUND_STEP: f64 = 1.0;

/// The ground envelope h's tabulation step, m.
pub const H_STEP: f64 = 0.25;

/// The smallest striking distance the margin's search tries, m.
pub const R_MIN: f64 = 2.0;

/// The largest, m: a phase exposed even there is said "above".
pub const R_MAX: f64 = 150.0;

/// A point in a cross-section: across (to the right of the span's chord,
/// looking from its first structure) and the elevation, m.
pub type P2 = [f64; 2];

/// A wire in a cross-section: where, and its outer radius (a bundle's
/// outer radius), m.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CutWire {
    /// Across, m.
    pub s: f64,
    /// Elevation, m.
    pub z: f64,
    /// Outer radius, m.
    pub r: f64,
}

/// A phase in a cross-section and what the check found.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CutPhase {
    /// Its section.
    pub section: String,
    /// The phase.
    pub wire: CutWire,
    /// Exposed here.
    pub exposed: bool,
    /// How far it is exposed: the angle beyond the allowed (°), or how
    /// far into it the sphere reaches (m); 0 or below: covered.
    pub depth: f64,
    /// Shielding angle: the angle it needs, °. Rolling sphere: none (r_max
    /// is found per phase and span, [`Row::margin`]).
    pub margin: Option<f64>,
}

/// One cross-section of a span, as checked and as drawn.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Cut {
    /// The span's first structure.
    pub from: String,
    /// Its second.
    pub to: String,
    /// Along the span from `from`, m.
    pub along: f64,
    /// The span's first structure's position (the plane's origin).
    pub origin: [f64; 3],
    /// Along the span (plan, unit).
    pub u: [f64; 2],
    /// Across, to the right (plan, unit).
    pub right: [f64; 2],
    /// The ground across, (s, z).
    pub ground: Vec<P2>,
    /// The shielding wires.
    pub shields: Vec<CutWire>,
    /// The phases.
    pub phases: Vec<CutPhase>,
    /// The covered zone's upper boundary across (the angles' lines, or the
    /// sphere's lowest reach), (s, z): drawn only.
    pub envelope: Vec<P2>,
    /// Sphere centres touching the shielding wires and the ground (a few,
    /// drawn only).
    pub centres: Vec<P2>,
}

impl Cut {
    /// The world point of (s, z) in this cross-section.
    pub fn world(&self, p: P2) -> [f64; 3] {
        [
            self.origin[0] + self.along * self.u[0] + p[0] * self.right[0],
            self.origin[1] + self.along * self.u[1] + p[0] * self.right[1],
            p[1],
        ]
    }
}

/// One phase of one span under one rule in one weather case.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Row {
    /// Index of the rule.
    pub rule: usize,
    /// The weather case.
    pub case: String,
    /// The section.
    pub section: String,
    /// The span in its section (0 = its first).
    pub span: usize,
    /// The span's structures.
    pub from: String,
    /// Its second structure.
    pub to: String,
    /// Its circuit and phase.
    pub phase: String,
    /// The span's horizontal length, m.
    pub length: f64,
    /// The length exposed, m (each cross-section stands for its step).
    pub exposed: f64,
    /// Where it is most exposed (or nearest to it), m along the span.
    pub worst_along: f64,
    /// Its depth there ([`CutPhase::depth`]).
    pub worst_depth: f64,
    /// Shielding angle: the largest angle needed, °. Rolling sphere: r_max
    /// at `worst_along`, m (none: covered from [`R_MIN`]; [`R_MAX`]:
    /// exposed even there).
    pub margin: Option<f64>,
    /// The stroke current of that r_max, kA (a rule giving a current).
    pub current: Option<f64>,
    /// The exposed parts of the phase, as world polylines.
    pub exposed_parts: Vec<Vec<[f64; 3]>>,
}

impl Row {
    /// Whether no part is exposed.
    pub fn passes(&self) -> bool {
        self.exposed <= 0.0
    }
}

/// What a check found.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Report {
    /// One row per phase, span, rule and case; exposed first, the longest
    /// exposure first, then by margin.
    pub rows: Vec<Row>,
    /// The cross-sections (of the last rule and case checked per span:
    /// for the drawings).
    pub cuts: Vec<Cut>,
    /// What was assumed or left out.
    pub notes: Vec<String>,
}

/// A bundle's outer radius, m.
fn outer_radius(w: &cross_section::CrossWire) -> f64 {
    let d = 0.5 * w.wire.diameter;
    match (w.bundle.count, w.bundle.spacing) {
        (n, Some(a)) if n > 1 => a / (2.0 * (std::f64::consts::PI / n as f64).sin()) + d,
        _ => d,
    }
}

/// The ground envelope h: the upper boundary of the points within `rg`
/// of the ground profile `g` or below it, tabulated every [`H_STEP`] from
/// `lo` to `hi` (chords below the arcs). `g` sorted by s.
struct Envelope {
    lo: f64,
    z: Vec<f64>,
}

impl Envelope {
    fn of(g: &[P2], rg: f64, lo: f64, hi: f64) -> Self {
        let n = ((hi - lo) / H_STEP).ceil().max(1.0) as usize + 1;
        let mut z = vec![f64::NEG_INFINITY; n];
        // Each ground segment's own envelope: its points' arcs; between
        // the samples, the segment offset along its normal and its ends'
        // arcs (the offset of a polyline).
        for (k, zk) in z.iter_mut().enumerate() {
            let s = lo + k as f64 * H_STEP;
            let mut best = f64::NEG_INFINITY;
            // Segments within rg across.
            let first = g.partition_point(|p| p[0] < s - rg).saturating_sub(1);
            for w in g[first..].windows(2) {
                let (a, b) = (w[0], w[1]);
                if a[0] > s + rg {
                    break;
                }
                // The nearest point of segment ab to the vertical through
                // s: the envelope at s is the highest z with distance rg
                // to the segment — the top of the capsule around it.
                best = best.max(capsule_top(a, b, rg, s));
            }
            *zk = best;
        }
        Self { lo, z }
    }

    /// h at s (linear between the tabulated points; −∞ outside).
    fn at(&self, s: f64) -> f64 {
        let x = (s - self.lo) / H_STEP;
        if x.is_nan() || x < 0.0 || x > (self.z.len() - 1) as f64 {
            return f64::NEG_INFINITY;
        }
        let k = (x.floor() as usize).min(self.z.len().saturating_sub(2));
        let f = x - k as f64;
        if self.z.len() == 1 {
            return self.z[0];
        }
        self.z[k] * (1.0 - f) + self.z[k + 1] * f
    }

    /// Its vertices, (s, z).
    fn points(&self) -> impl Iterator<Item = P2> + '_ {
        self.z
            .iter()
            .enumerate()
            .filter(|(_, z)| z.is_finite())
            .map(|(k, z)| [self.lo + k as f64 * H_STEP, *z])
    }
}

/// The top of the capsule of radius `r` around segment ab at abscissa s
/// (−∞ when the capsule does not reach s).
fn capsule_top(a: P2, b: P2, r: f64, s: f64) -> f64 {
    let mut best = f64::NEG_INFINITY;
    for p in [a, b] {
        let dx = s - p[0];
        if dx.abs() <= r {
            best = best.max(p[1] + (r * r - dx * dx).sqrt());
        }
    }
    let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
    let len = dx.hypot(dz);
    if len > 1e-12 && dx > 0.0 {
        // The segment shifted along its upward normal by r.
        let n = [-dz / len, dx / len];
        let (a2, b2) = (
            [a[0] + r * n[0], a[1] + r * n[1]],
            [b[0] + r * n[0], b[1] + r * n[1]],
        );
        if (a2[0] <= s && s <= b2[0]) || (b2[0] <= s && s <= a2[0]) {
            let f = (s - a2[0]) / (b2[0] - a2[0]);
            best = best.max(a2[1] + f * (b2[1] - a2[1]));
        }
    }
    best
}

/// How far the sphere of radius `r` reaches into phase `p` (its surface):
/// r + p.r less the distance from p to the nearest point of the free
/// region (≤ 0: covered).
fn sphere_depth(p: CutWire, shields: &[CutWire], h: &Envelope, r: f64) -> f64 {
    let reach = r + p.r;
    let pc = [p.s, p.z];
    let free = |c: P2| {
        c[1] >= h.at(c[0]) - 1e-9
            && shields
                .iter()
                .all(|w| (c[0] - w.s).hypot(c[1] - w.z) >= r - 1e-9)
    };
    let mut best = f64::INFINITY;
    let mut take = |c: P2| {
        let d = (c[0] - pc[0]).hypot(c[1] - pc[1]);
        if d < best && free(c) {
            best = d;
        }
    };
    take(pc);
    // Feet on the shielding wires' circles; their corners with each other.
    for (i, a) in shields.iter().enumerate() {
        let d = (pc[0] - a.s).hypot(pc[1] - a.z);
        if d > 1e-12 {
            take([a.s + r * (pc[0] - a.s) / d, a.z + r * (pc[1] - a.z) / d]);
        }
        for b in &shields[i + 1..] {
            for c in circle_circle([a.s, a.z], r, [b.s, b.z], r) {
                take(c);
            }
        }
    }
    // Feet on h and its corners with the circles, within reach.
    let pts: Vec<P2> = h
        .points()
        .filter(|q| (q[0] - pc[0]).abs() <= reach + H_STEP)
        .collect();
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        take(foot(pc, a, b));
        take(a);
        for s in shields {
            for c in circle_segment([s.s, s.z], r, a, b) {
                take(c);
            }
        }
    }
    reach - best
}

/// The point of segment ab nearest p.
fn foot(p: P2, a: P2, b: P2) -> P2 {
    let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dz * dz;
    if l2 < 1e-24 {
        return a;
    }
    let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dz) / l2).clamp(0.0, 1.0);
    [a[0] + t * dx, a[1] + t * dz]
}

/// The intersections of two circles.
fn circle_circle(a: P2, ra: f64, b: P2, rb: f64) -> Vec<P2> {
    let d = (b[0] - a[0]).hypot(b[1] - a[1]);
    if d < 1e-12 || d > ra + rb || d < (ra - rb).abs() {
        return Vec::new();
    }
    let x = (d * d + ra * ra - rb * rb) / (2.0 * d);
    let y = (ra * ra - x * x).max(0.0).sqrt();
    let (ux, uz) = ((b[0] - a[0]) / d, (b[1] - a[1]) / d);
    let m = [a[0] + x * ux, a[1] + x * uz];
    vec![
        [m[0] - y * uz, m[1] + y * ux],
        [m[0] + y * uz, m[1] - y * ux],
    ]
}

/// The intersections of a circle and segment ab.
fn circle_segment(c: P2, r: f64, a: P2, b: P2) -> Vec<P2> {
    let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
    let (fx, fz) = (a[0] - c[0], a[1] - c[1]);
    let qa = dx * dx + dz * dz;
    if qa < 1e-24 {
        return Vec::new();
    }
    let qb = 2.0 * (fx * dx + fz * dz);
    let qc = fx * fx + fz * fz - r * r;
    let disc = qb * qb - 4.0 * qa * qc;
    if disc < 0.0 {
        return Vec::new();
    }
    let sq = disc.sqrt();
    [(-qb - sq) / (2.0 * qa), (-qb + sq) / (2.0 * qa)]
        .into_iter()
        .filter(|t| (0.0..=1.0).contains(t))
        .map(|t| [a[0] + t * dx, a[1] + t * dz])
        .collect()
}

/// The angle phase `p` needs from shielding wire `w` (to the far side of
/// its bundle), degrees; 90 when not below it.
fn needed_angle(p: CutWire, w: CutWire) -> f64 {
    let dz = w.z - p.z - p.r;
    if dz <= 0.0 {
        return 90.0;
    }
    ((p.s - w.s).abs() + p.r).atan2(dz).to_degrees()
}

/// Shielding angle: (the angle beyond the allowed, °; the angle needed
/// from the wire covering it best). No shielding wire above: 90°.
fn angle_check(p: CutWire, shields: &[CutWire], outer: f64, inner: f64) -> (f64, f64) {
    let mut best = (f64::INFINITY, 90.0);
    for w in shields {
        // Towards another shielding wire: the inner angle.
        let side = (p.s - w.s).signum();
        let towards = shields
            .iter()
            .any(|o| (o.s - w.s).abs() > 1e-6 && (o.s - w.s).signum() == side);
        let allowed = if towards { inner } else { outer };
        let need = needed_angle(p, *w);
        if need - allowed < best.0 {
            best = (need - allowed, need);
        }
    }
    if best.0.is_infinite() {
        (90.0, 90.0)
    } else {
        best
    }
}

/// r_max: the largest striking distance from [`R_MIN`] to [`R_MAX`] at
/// which phase `p` is exposed (none: covered at every one; `R_MAX`:
/// exposed even there). `envelope(r)` gives h for r.
fn r_max(p: CutWire, shields: &[CutWire], envelope: &dyn Fn(f64) -> Envelope) -> Option<f64> {
    let exposed = |r: f64| sphere_depth(p, shields, &envelope(r), r) > 0.0;
    // A scan by 10 %, from the top: the largest exposed one, then halving.
    let mut hi = R_MAX;
    if exposed(hi) {
        return Some(R_MAX);
    }
    let mut lo = None;
    while hi > R_MIN {
        let next = (hi / 1.1).max(R_MIN);
        if exposed(next) {
            lo = Some(next);
            break;
        }
        hi = next;
        if next <= R_MIN {
            break;
        }
    }
    let mut lo = lo?;
    while hi - lo > 0.01 {
        let mid = 0.5 * (lo + hi);
        if exposed(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some(lo)
}

/// The covered zone's upper boundary across a cross-section, for the
/// drawings: shielding angle — each wire's lines at its angles down to the
/// ground (the inner ones to where they meet); rolling sphere — the
/// lowest reach of spheres resting on the wires and the ground.
fn envelope_line(
    method: Method,
    r: f64,
    shields: &[CutWire],
    ground: &[P2],
    h: Option<&Envelope>,
) -> (Vec<P2>, Vec<P2>) {
    let g_at = |s: f64| {
        let k = ground.partition_point(|p| p[0] < s);
        match (k.checked_sub(1).and_then(|i| ground.get(i)), ground.get(k)) {
            (Some(a), Some(b)) if b[0] > a[0] => a[1] + (b[1] - a[1]) * (s - a[0]) / (b[0] - a[0]),
            (Some(a), _) => a[1],
            (_, Some(b)) => b[1],
            _ => 0.0,
        }
    };
    if shields.is_empty() || ground.len() < 2 {
        return (Vec::new(), Vec::new());
    }
    let (lo, hi) = (ground[0][0], ground[ground.len() - 1][0]);
    match method {
        Method::Angle { outer, inner } => {
            let mut ws: Vec<CutWire> = shields.to_vec();
            ws.sort_by(|a, b| a.s.total_cmp(&b.s));
            // Down a line from w leaning `deg` to side `dir` to the ground.
            let down = |w: CutWire, deg: f64, dir: f64| {
                let t = deg.to_radians().tan();
                let mut d = 0.0;
                while d < 2000.0 {
                    let s = w.s + dir * d * t;
                    if s < lo || s > hi || w.z - d <= g_at(s) {
                        return [s.clamp(lo, hi), (w.z - d).max(g_at(s.clamp(lo, hi)))];
                    }
                    d += 0.5;
                }
                [w.s, w.z - d]
            };
            let mut line = vec![down(ws[0], outer, -1.0)];
            for (k, w) in ws.iter().enumerate() {
                line.push([w.s, w.z]);
                if let Some(n) = ws.get(k + 1) {
                    // The inner lines meet: tan(a)·(d1) + tan(a)·(d2) = gap.
                    let t = inner.to_radians().tan();
                    let gap = n.s - w.s;
                    // w.z − d1 = n.z − d2, d1·t + d2·t = gap.
                    let d1 = (gap / t + (w.z - n.z)) / 2.0;
                    let meet = [w.s + d1 * t, w.z - d1];
                    line.push(if meet[1] > g_at(meet[0]) {
                        meet
                    } else {
                        down(*w, inner, 1.0)
                    });
                }
            }
            line.push(down(ws[ws.len() - 1], outer, 1.0));
            (line, Vec::new())
        }
        Method::Sphere { .. } => {
            let Some(h) = h else {
                return (Vec::new(), Vec::new());
            };
            // Centres on the free region's boundary: on the wires' circles
            // (every 2°) and on h, kept where free.
            let free = |c: P2| {
                c[1] >= h.at(c[0]) - 1e-6
                    && shields
                        .iter()
                        .all(|w| (c[0] - w.s).hypot(c[1] - w.z) >= r - 1e-6)
            };
            let mut centres: Vec<P2> = Vec::new();
            for w in shields {
                for k in 0..180 {
                    let a = (k as f64 * 2.0).to_radians();
                    let c = [w.s + r * a.cos(), w.z + r * a.sin()];
                    if free(c) {
                        centres.push(c);
                    }
                }
            }
            for c in h.points().step_by(4) {
                if free(c) {
                    centres.push(c);
                }
            }
            let mut line = Vec::new();
            let mut s = lo;
            while s <= hi {
                let mut z = f64::INFINITY;
                for c in &centres {
                    let dx = s - c[0];
                    if dx.abs() <= r {
                        z = z.min(c[1] - (r * r - dx * dx).sqrt());
                    }
                }
                if z.is_finite() {
                    line.push([s, z.max(g_at(s))]);
                }
                s += 1.0;
            }
            // A few centres drawn: those touching two things (local lows).
            let shown: Vec<P2> = centres
                .iter()
                .copied()
                .filter(|c| {
                    let near = shields
                        .iter()
                        .filter(|w| ((c[0] - w.s).hypot(c[1] - w.z) - r).abs() < 0.05)
                        .count();
                    near >= 2 || (near == 1 && (c[1] - h.at(c[0])).abs() < 0.3)
                })
                .collect();
            (line, shown)
        }
    }
}

/// One phase `p` in one cross-section with `shields` over the ground
/// profile `ground` (sorted across, wide enough for the sphere's reach and
/// the margin's search) by `method`: how far it is exposed (≤ 0: covered,
/// [`CutPhase::depth`]) and its margin (the angle it needs, °, or r_max,
/// m) — as the check takes each place (the validation cases' entry).
pub fn check_place(
    p: CutWire,
    shields: &[CutWire],
    ground: &[P2],
    method: Method,
) -> (f64, Option<f64>) {
    match method {
        Method::Angle { outer, inner } => {
            let (beyond, need) = angle_check(p, shields, outer, inner);
            (beyond, Some(need))
        }
        Method::Sphere {
            distance,
            ground_factor,
        } => {
            let r = distance.metres();
            let envelope = |rr: f64| {
                let gg = ground_factor * rr;
                Envelope::of(ground, gg, p.s - rr - p.r - 1.0, p.s + rr + p.r + 1.0)
            };
            (
                sphere_depth(p, shields, &envelope(r), r),
                r_max(p, shields, &envelope),
            )
        }
    }
}

/// Checks the spans of `src` under `rule` (index `index`) in weather
/// case `case` (the wires of `src.shapes` solved in it). `done` counts
/// the spans finished; `cancel` stops it between spans.
pub fn check(
    src: &Sources<'_>,
    rule: &LightningRule,
    index: usize,
    case: &str,
    done: &AtomicUsize,
    cancel: &AtomicBool,
) -> Report {
    let line = src.line;
    let spans = cross_section::line_spans(line);
    let protects: Vec<bool> = line.sections.iter().map(|s| rule.protects(s)).collect();
    let shields_of: Vec<bool> = line.sections.iter().map(|s| rule.shields(s)).collect();
    let section_index = |id: &str| line.sections.iter().position(|s| s.id == id);
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(spans.len().max(1));
    let next = AtomicUsize::new(0);
    let results: Vec<SpanResult> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut out = Vec::new();
                    loop {
                        let k = next.fetch_add(1, Ordering::Relaxed);
                        if k >= spans.len() || cancel.load(Ordering::Relaxed) {
                            break;
                        }
                        let (rows, cuts, notes) = check_span(
                            src,
                            rule,
                            index,
                            case,
                            &spans[k],
                            &protects,
                            &shields_of,
                            &section_index,
                        );
                        out.push((k, rows, cuts, notes));
                        done.fetch_add(1, Ordering::Relaxed);
                    }
                    out
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap_or_default())
            .collect()
    });
    let mut results = results;
    results.sort_by_key(|r| r.0);
    let mut report = Report::default();
    for (_, rows, cuts, notes) in results {
        report.rows.extend(rows);
        report.cuts.extend(cuts);
        for n in notes {
            if !report.notes.contains(&n) {
                report.notes.push(n);
            }
        }
    }
    // The wires not reaching the cross-sections next to their structures:
    // one note (owner, 2026-10-31: one warning, not a long list).
    const SHORT: &str = ": the wire does not reach the cross-sections next to its structures (its strings do), left out there";
    let short: Vec<String> = report
        .notes
        .iter()
        .filter_map(|n| n.strip_suffix(SHORT).map(str::to_owned))
        .collect();
    if !short.is_empty() {
        report.notes.retain(|n| !n.ends_with(SHORT));
        let shown: Vec<&str> = short.iter().take(5).map(String::as_str).collect();
        report.notes.push(format!(
            "{} wire span(s) do not reach the cross-sections next to their structures (their strings do) and are left out there — a phase there is checked without them (conservative for a shielding wire): {}{}",
            short.len(),
            shown.join(", "),
            if short.len() > 5 { ", …" } else { "" }
        ));
    }
    report
}

/// A span's index, its rows, cross-sections and notes.
type SpanResult = (usize, Vec<Row>, Vec<Cut>, Vec<String>);

/// One span: its cross-sections, its rows.
#[allow(clippy::too_many_arguments)]
fn check_span(
    src: &Sources<'_>,
    rule: &LightningRule,
    index: usize,
    case: &str,
    span: &LineSpan,
    protects: &[bool],
    shields_of: &[bool],
    section_index: &dyn Fn(&str) -> Option<usize>,
) -> (Vec<Row>, Vec<Cut>, Vec<String>) {
    let mut notes = Vec::new();
    let axis = match Axis::of(src.line, span) {
        Ok(a) => a,
        Err(e) => return (Vec::new(), Vec::new(), vec![e]),
    };
    // Spans without a protected phase: nothing to check.
    if !span.wires.iter().any(|&(k, _)| protects[k]) {
        return (Vec::new(), Vec::new(), notes);
    }
    let n = (axis.length / rule.step).ceil().max(1.0) as usize;
    let (r, rg) = match rule.method {
        Method::Sphere {
            distance,
            ground_factor,
        } => (distance.metres(), ground_factor * distance.metres()),
        Method::Angle { .. } => (0.0, 0.0),
    };
    let sphere = matches!(rule.method, Method::Sphere { .. });
    // Per phase (section, span index): per station (along, CutPhase,
    // world point).
    type Station = (f64, CutPhase, [f64; 3], Vec<CutWire>);
    let mut per_phase: std::collections::BTreeMap<(String, usize), Vec<Station>> =
        Default::default();
    let mut cuts = Vec::new();
    // At the middle of each step (never on a clamp): each stands for one
    // step.
    for k in 0..n {
        let f = (k as f64 + 0.5) / n as f64;
        let cs = match cross_section::cross_section_reached(src, span, StationAt::Fraction(f)) {
            Ok(c) => c,
            Err(e) => {
                notes.push(e);
                continue;
            }
        };
        for nt in &cs.notes {
            if !notes.contains(nt) {
                notes.push(nt.clone());
            }
        }
        let t = f * axis.length;
        let mut shields = Vec::new();
        let mut phases = Vec::new();
        for w in &cs.wires {
            let Some(i) = section_index(&w.section) else {
                continue;
            };
            let cw = CutWire {
                s: w.offset,
                z: w.elevation,
                r: outer_radius(w),
            };
            if shields_of[i] {
                shields.push(cw);
            } else if protects[i] && w.phase.is_some() {
                phases.push((w.section.clone(), w.span, cw));
            }
        }
        if phases.is_empty() {
            continue;
        }
        // The ground across: wide enough for the sphere's reach (and the
        // margin's largest sphere), else for the drawings.
        let (smin, smax) = phases
            .iter()
            .map(|p| p.2.s)
            .chain(shields.iter().map(|w| w.s))
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), s| {
                (a.min(s), b.max(s))
            });
        let reach_rule = if sphere { r + rg + 2.0 } else { 0.0 };
        let reach = if sphere { reach_rule.max(40.0) } else { 60.0 };
        let (lo, hi) = (smin - reach, smax + reach);
        let mut ground = Vec::new();
        let mut s = lo;
        while s <= hi + 1e-9 {
            ground.push([s, axis.ground(src.ground, t, s)]);
            s += GROUND_STEP;
        }
        let drawn = |g: &[P2]| -> Vec<P2> {
            let (a, b) = (smin - reach_rule.max(40.0), smax + reach_rule.max(40.0));
            g.iter()
                .copied()
                .filter(|p| p[0] >= a && p[0] <= b)
                .collect()
        };
        let h_rule =
            sphere.then(|| Envelope::of(&ground, rg, smin - r - rg - 2.0, smax + r + rg + 2.0));
        let mut cut_phases = Vec::new();
        for (section, j, p) in &phases {
            let (depth, margin) = match rule.method {
                Method::Angle { outer, inner } => {
                    let (beyond, need) = angle_check(*p, &shields, outer, inner);
                    (beyond, Some(need))
                }
                Method::Sphere { .. } => {
                    let h = h_rule.as_ref().expect("made for the sphere");
                    (sphere_depth(*p, &shields, h, r), None)
                }
            };
            let exposed = depth > 1e-6;
            let cp = CutPhase {
                section: section.clone(),
                wire: *p,
                exposed,
                depth,
                margin,
            };
            per_phase.entry((section.clone(), *j)).or_default().push((
                t,
                cp.clone(),
                [0.0; 3],
                shields.clone(),
            ));
            cut_phases.push(cp);
        }
        let shown_ground = drawn(&ground);
        let (envelope, centres) =
            envelope_line(rule.method, r, &shields, &shown_ground, h_rule.as_ref());
        let cut = Cut {
            from: span.from.clone(),
            to: span.to.clone(),
            along: t,
            origin: axis.a,
            u: axis.u,
            right: axis.right,
            ground: shown_ground,
            shields,
            phases: cut_phases,
            envelope,
            centres,
        };
        // The world points of the phases at this cut.
        for (section, j, p) in &phases {
            if let Some(v) = per_phase.get_mut(&(section.clone(), *j))
                && let Some(last) = v.last_mut()
            {
                last.2 = cut.world([p.s, p.z]);
            }
        }
        cuts.push(cut);
    }
    // Each cross-section stands for half the way to its neighbours; the
    // first and last reach the structures.
    let weight = |ts: &[f64], k: usize| {
        let a = if k == 0 {
            0.0
        } else {
            0.5 * (ts[k - 1] + ts[k])
        };
        let b = if k + 1 == ts.len() {
            axis.length
        } else {
            0.5 * (ts[k] + ts[k + 1])
        };
        (b - a).max(0.0)
    };
    let mut rows = Vec::new();
    for ((section, j), stations) in per_phase {
        let label = src
            .line
            .sections
            .iter()
            .find(|s| s.id == section)
            .map(|s| {
                if s.circuit_name.is_empty() {
                    s.phase_label.clone()
                } else {
                    format!("{} {}", s.circuit_name, s.phase_label)
                }
            })
            .unwrap_or_default();
        let m = stations.len();
        let ts: Vec<f64> = stations.iter().map(|s| s.0).collect();
        let exposed: f64 = stations
            .iter()
            .enumerate()
            .filter(|(_, s)| s.1.exposed)
            .map(|(k, _)| weight(&ts, k))
            .fold(0.0, |a, b| a + b);
        let worst = stations
            .iter()
            .max_by(|a, b| a.1.depth.total_cmp(&b.1.depth))
            .expect("a station");
        let margin = match rule.method {
            Method::Angle { .. } => stations
                .iter()
                .filter_map(|s| s.1.margin)
                .max_by(f64::total_cmp),
            Method::Sphere { .. } => {
                // At the deepest place: the ground there sampled as wide
                // as the largest sphere reaches.
                let (t, cp, _, shields) = worst;
                let p = cp.wire;
                let beta = rg / r.max(1e-9);
                let reach = R_MAX * (1.0 + beta) + p.r + 2.0;
                let mut ground = Vec::new();
                let mut s = p.s - reach;
                while s <= p.s + reach + 1e-9 {
                    ground.push([s, axis.ground(src.ground, *t, s)]);
                    s += GROUND_STEP;
                }
                let envelope = |rr: f64| {
                    let gg = beta * rr;
                    Envelope::of(&ground, gg, p.s - rr - p.r - 1.0, p.s + rr + p.r + 1.0)
                };
                r_max(p, shields, &envelope)
            }
        };
        let current = match (rule.method, margin) {
            (Method::Sphere { distance, .. }, Some(rm)) => distance.current_of(rm),
            _ => None,
        };
        // The exposed runs as polylines (a lone station: a short piece
        // around it).
        let mut parts: Vec<Vec<[f64; 3]>> = Vec::new();
        let mut run: Vec<[f64; 3]> = Vec::new();
        for (k, s) in stations.iter().enumerate() {
            if s.1.exposed {
                run.push(s.2);
            }
            if (!s.1.exposed || k == m - 1) && !run.is_empty() {
                if run.len() == 1 {
                    run.push([
                        run[0][0] + 0.2 * axis.u[0],
                        run[0][1] + 0.2 * axis.u[1],
                        run[0][2],
                    ]);
                }
                parts.push(std::mem::take(&mut run));
            }
        }
        rows.push(Row {
            rule: index,
            case: case.to_owned(),
            section,
            span: j,
            from: span.from.clone(),
            to: span.to.clone(),
            phase: label,
            length: axis.length,
            exposed,
            worst_along: worst.0,
            worst_depth: worst.1.depth,
            margin,
            current,
            exposed_parts: parts,
        });
    }
    (rows, cuts, notes)
}

/// Sorts the rows: exposed first, the longest exposure first, then the
/// nearest to exposure (the largest depth).
pub fn sort_rows(rows: &mut [Row]) {
    rows.sort_by(|a, b| {
        b.exposed
            .total_cmp(&a.exposed)
            .then(b.worst_depth.total_cmp(&a.worst_depth))
    });
}

/// The rows as CSV.
pub fn csv(rows: &[Row], rules: &[LightningRule]) -> String {
    use std::fmt::Write;
    let mut out = String::from(
        "rule,method,case,section,span,from,to,phase,span_length_m,exposed_m,worst_along_m,worst_depth,margin,current_ka,status\n",
    );
    for r in rows {
        let rule = &rules[r.rule];
        let _ = writeln!(
            out,
            "{},{},{},{},{},{},{},{},{:.2},{:.2},{:.2},{:.4},{},{},{}",
            ohl_core::text::csv_field(&rule.name),
            rule.method.as_str(),
            ohl_core::text::csv_field(&r.case),
            ohl_core::text::csv_field(&r.section),
            r.span + 1,
            ohl_core::text::csv_field(&r.from),
            ohl_core::text::csv_field(&r.to),
            ohl_core::text::csv_field(&r.phase),
            r.length,
            r.exposed,
            r.worst_along,
            r.worst_depth,
            r.margin.map_or(String::new(), |m| format!("{m:.3}")),
            r.current.map_or(String::new(), |c| format!("{c:.2}")),
            if r.passes() { "covered" } else { "exposed" },
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(s: f64, z: f64) -> CutWire {
        CutWire { s, z, r: 0.0 }
    }

    fn flat(z: f64) -> Vec<P2> {
        (-400..=400).map(|k| [k as f64, z]).collect()
    }

    /// Level ground: h is the ground lifted by β·r exactly (chords of a
    /// straight line are the line).
    #[test]
    fn the_ground_envelope_of_level_ground_is_the_ground_lifted() {
        let h = Envelope::of(&flat(100.0), 30.0, -50.0, 50.0);
        for s in [-50.0, -12.3, 0.0, 49.9] {
            assert!((h.at(s) - 130.0).abs() < 1e-9, "{}", h.at(s));
        }
    }

    /// One shielding wire over level ground: the exact depth against a
    /// brute force over a fine grid of sphere centres (never less, and
    /// within the grid's step).
    #[test]
    fn the_exact_depth_matches_a_brute_force() {
        let (r, rg) = (25.0, 22.0);
        let shields = [wire(0.0, 30.0), wire(9.0, 31.0)];
        let g: Vec<P2> = (-300..=300)
            .map(|k| [k as f64, 0.05 * k as f64 + if k > 20 { -3.0 } else { 0.0 }])
            .collect();
        let h = Envelope::of(&g, rg, -150.0, 150.0);
        let step = 0.05;
        let mut seen = (0, 0);
        for p in [
            CutWire {
                s: 5.0,
                z: 22.0,
                r: 0.2,
            },
            wire(14.0, 20.0),
            wire(-6.0, 21.0),
            wire(3.0, 27.0),
            wire(30.0, 12.0),
        ] {
            let exact = sphere_depth(p, &shields, &h, r);
            let mut brute = f64::NEG_INFINITY;
            let n = ((r + p.r) / step) as i64 + 1;
            for i in -n..=n {
                for k in -n..=n {
                    let c = [p.s + i as f64 * step, p.z + k as f64 * step];
                    let free = c[1] >= h.at(c[0])
                        && shields.iter().all(|w| (c[0] - w.s).hypot(c[1] - w.z) >= r);
                    if free {
                        brute = brute.max(r + p.r - (c[0] - p.s).hypot(c[1] - p.z));
                    }
                }
            }
            // Exposed by one, exposed by the other, by the same depth to
            // the grid's step (covered: the depth is only a sign — the
            // grid stops at the sphere's reach).
            let (e, b) = (exact > 0.0, brute > 0.0);
            if b {
                assert!(exact >= brute - 1e-9, "{p:?}: {exact} < {brute}");
            }
            if e {
                assert!(exact - brute < 2.0 * step, "{p:?}: {exact} vs {brute}");
            }
            assert!(
                e == b || exact.abs() < 2.0 * step,
                "{p:?}: {exact} vs {brute}"
            );
            if e {
                seen.0 += 1;
            } else {
                seen.1 += 1;
            }
        }
        assert!(seen.0 >= 2 && seen.1 >= 1, "exposed and covered: {seen:?}");
    }

    /// Two wires close together cover a phase between and below them; the
    /// same phase under one of them alone at a larger gap is exposed for a
    /// small sphere.
    #[test]
    fn two_wires_cover_the_phase_between_them() {
        let h = Envelope::of(&flat(0.0), 20.0, -200.0, 200.0);
        let shields = [wire(-5.0, 35.0), wire(5.0, 35.0)];
        assert!(sphere_depth(wire(0.0, 30.0), &shields, &h, 20.0) <= 0.0);
        let far = [wire(-30.0, 35.0), wire(30.0, 35.0)];
        assert!(sphere_depth(wire(0.0, 30.0), &far, &h, 20.0) > 0.0);
    }

    /// r_max of a phase beside one wire: exposed below, covered above,
    /// found to 1 cm; exposure grows as r shrinks.
    #[test]
    fn r_max_is_the_edge_of_exposure() {
        let shields = [wire(0.0, 30.0)];
        let g = flat(0.0);
        let p = wire(6.0, 22.0);
        let env = |rr: f64| Envelope::of(&g, rr, -400.0, 400.0);
        let rm = r_max(p, &shields, &env).expect("exposed for some r");
        assert!(rm > R_MIN && rm < R_MAX);
        assert!(sphere_depth(p, &shields, &env(rm - 0.05), rm - 0.05) > 0.0);
        assert!(sphere_depth(p, &shields, &env(rm + 0.05), rm + 0.05) <= 0.0);
    }

    /// r_max beside one wire over level ground, β = 1, against the
    /// classical construction: the sphere resting on the ground (centre
    /// at height r) that touches both the shielding wire S and the phase
    /// P, x_S + √(r² − (r − z_S)²) = x_P + √(r² − (r − z_P)²), solved
    /// here by halving on its own.
    #[test]
    fn r_max_beside_one_wire_is_the_classical_one() {
        let (s, p) = (wire(0.0, 32.0), wire(5.0, 24.0));
        let f = |r: f64| {
            (r * r - (r - s.z) * (r - s.z)).sqrt() - p.s - (r * r - (r - p.z) * (r - p.z)).sqrt()
        };
        // f < 0 below r_max (the centre resting beside the wire is nearer
        // in than the phase's own: within r of the phase), f > 0 above.
        let (mut lo, mut hi) = (16.5, 150.0);
        assert!(f(lo) < 0.0 && f(hi) > 0.0);
        while hi - lo > 1e-9 {
            let m = 0.5 * (lo + hi);
            if f(m) < 0.0 {
                lo = m;
            } else {
                hi = m;
            }
        }
        let g = flat(0.0);
        let env = |rr: f64| Envelope::of(&g, rr, -400.0, 400.0);
        let got = r_max(p, &[s], &env).expect("exposed below r_max");
        assert!((got - lo).abs() < 0.02, "{got} vs {lo}");
    }

    /// Shielding angle: the angle to a phase, inner towards the other
    /// wire, outer elsewhere; the phase's radius counted against it.
    #[test]
    fn the_shielding_angle_needed() {
        let shields = [wire(-4.0, 30.0), wire(4.0, 30.0)];
        // Outside, 30° from the right wire: 10 m down, 5.77 m out.
        let p = wire(4.0 + 10.0 * 30f64.to_radians().tan(), 20.0);
        let (beyond, need) = angle_check(p, &shields, 30.0, 45.0);
        assert!((need - 30.0).abs() < 1e-9 && beyond.abs() < 1e-9);
        // Between them, 40° inwards from the right wire: within the inner
        // 45°.
        let q = wire(4.0 - 10.0 * 40f64.to_radians().tan(), 20.0);
        assert!(angle_check(q, &shields, 30.0, 45.0).0 < 0.0);
        // The bundle's radius makes it need more.
        let big = CutWire { r: 0.3, ..p };
        assert!(angle_check(big, &shields, 30.0, 45.0).0 > 0.0);
        // No wire above: 90°.
        assert_eq!(angle_check(wire(0.0, 40.0), &shields, 30.0, 30.0).1, 90.0);
    }
}
