// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Distance between conductors in the span: the measuring. A code gives
//! the distance required by a formula of the local sag and string length
//! (e.g. the Swiss LeV, Annex 6, 2.2.1: \\( z = k \sqrt{f + l_k} + C
//! \\)); its values and its rules are a check plugin's (see
//! `docs/development/phase-distance-check.md`), not OpenOHL's.
//!
//! OpenOHL measures: it pairs the conductors hanging between the same
//! two structures (and, asked, those coming near elsewhere); along each,
//! every `spacing` of its length, it takes the closest point on the
//! other and there the distance, the system angle β (of the line joining
//! the points, from the vertical), each conductor's local sag f
//! (vertically below its chord) and l_k (the suspension string's length,
//! linear between its ends). For conductors joined by interphase spacers
//! in the span it gives each one's largest sag between its spacer
//! clamps. The [`Evaluator`] (the plugin) says what is required at each
//! station; the check keeps each pair's worst station (the smallest
//! margin d − z) and the length of conductor below the requirement.

use std::collections::BTreeSet;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use ohl_core::budget::Budget;
use ohl_line::solve::SectionShape;
use ohl_line::{Fields, Line, Structure};

use crate::checks::{Wire, wires_of};

type P3 = [f64; 3];

/// Numbers per station handed to the [`Evaluator`]: the distance (m),
/// β (°), the local sag of each conductor (m), l_k of each (m).
pub const STATION_VALUES: usize = 6;

/// How the measuring runs.
#[derive(Debug, Clone, PartialEq)]
pub struct PhaseDistanceSettings {
    /// Stations every this length along a conductor, m.
    pub spacing: f64,
    /// The circuits checked (none: all); a pair when both its conductors'
    /// circuits are.
    pub circuits: Option<BTreeSet<String>>,
    /// Also pairs of conductors not between the same two structures
    /// (another line's, the next span's) where they come within
    /// `max_distance`.
    pub nearby: bool,
    /// Nearby pairs: measured where closer than this, m (keep it above
    /// the largest distance required, about 12 m at 380 kV).
    pub max_distance: f64,
}

impl Default for PhaseDistanceSettings {
    fn default() -> Self {
        Self {
            spacing: 1.0,
            circuits: None,
            nearby: false,
            max_distance: 20.0,
        }
    }
}

/// A conductor as the check sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct Conductor {
    /// The wire (section span).
    pub wire: Wire,
    /// Diameter of its cable, mm.
    pub diameter_mm: f64,
    /// Mass of its cable, kg/m.
    pub mass_kg_m: f64,
    /// l_k at its first and last point, m.
    pub lk: [f64; 2],
    /// Its section's electrical system.
    pub system: Option<String>,
    /// Its section's plugin columns' values (e.g. the circuit's
    /// insulation level).
    pub fields: Fields,
    /// The interphase spacers clamped on it in this span: spacer id,
    /// where (as solved).
    pub spacers: Vec<(String, P3)>,
}

/// A pair of conductors as measured, for the [`Evaluator`].
#[derive(Debug, Clone, Copy)]
pub struct PairGeometry<'a> {
    /// The first conductor.
    pub a: &'a Conductor,
    /// The second.
    pub b: &'a Conductor,
    /// Within one electrical system, or towards an earth wire.
    pub same_system: bool,
    /// Both hang between the same two structures.
    pub same_span: bool,
    /// Mean conductor height of the span, m above sea level.
    pub altitude: f64,
    /// Spacers join them in the span: the largest sag of each between
    /// its clamps on the span (the spacers' and the span's ends), m.
    pub spacer_sag: Option<[f64; 2]>,
}

/// What an [`Evaluator`] requires along a pair.
#[derive(Debug, Clone, PartialEq)]
pub struct Required {
    /// `per_station` numbers per station: the distance required (m),
    /// then the evaluator's own (kept with the worst station).
    pub values: Vec<f64>,
    /// Numbers per station (at least 1).
    pub per_station: usize,
    /// The rule applied, in words.
    pub rule: String,
}

/// What a code requires: the distance at each station of a pair
/// (implemented by a check plugin).
pub trait Evaluator {
    /// For `pair`, from `stations` ([`STATION_VALUES`] numbers each).
    fn evaluate(&mut self, pair: &PairGeometry, stations: &[f64]) -> Result<Required, String>;
}

/// The worst station of a pair.
#[derive(Debug, Clone, PartialEq)]
pub struct Station {
    /// The point on the first conductor and on the second.
    pub at: [P3; 2],
    /// Where along each conductor, m of its length from its first point
    /// (near 0 or its length: at a clamp, where f is about 0).
    pub along: [f64; 2],
    /// Distance between them, m.
    pub distance: f64,
    /// Required, m.
    pub required: f64,
    /// Local sag of each conductor, m.
    pub sag: [f64; 2],
    /// l_k of each conductor there, m.
    pub lk: [f64; 2],
    /// System angle β, degrees from the vertical.
    pub beta: f64,
    /// The evaluator's own numbers there (after the distance required).
    pub values: Vec<f64>,
}

impl Station {
    /// The distance less the required one, m (negative: a violation).
    pub fn margin(&self) -> f64 {
        self.distance - self.required
    }
}

/// A pair of conductors checked.
#[derive(Debug, Clone, PartialEq)]
pub struct PhaseDistancePair {
    /// First conductor (index in those checked).
    pub a: usize,
    /// Second conductor.
    pub b: usize,
    /// Within one electrical system, or towards an earth wire.
    pub same_system: bool,
    /// Nominal voltage, the higher of the two, kV.
    pub kv: f64,
    /// Both hang between the same two structures (else a nearby pair).
    pub same_span: bool,
    /// Mean conductor height of the span, m above sea level.
    pub altitude: f64,
    /// The largest sags between spacers, when spacers join them.
    pub spacer_sag: Option<[f64; 2]>,
    /// The rule the evaluator applied.
    pub rule: String,
    /// The worst station.
    pub worst: Station,
    /// Length of conductor below the requirement, m: on the conductor
    /// and subconductor where it is longest (each stretch counted once,
    /// not per subconductor pair or measuring direction).
    pub length_below: f64,
}

/// l_k at the end of a conductor at `key` (`structure:set`): the length
/// of a pure suspension I-string; a V, a tension string or a wire clamped
/// to the structure: 0; a string below a V: its length; a TAK (tension
/// strings from a node on a suspension string): that suspension string's
/// length.
pub fn lk_at(line: &Line, key: &str) -> f64 {
    let Some((structure, set)) = key.rsplit_once(':') else {
        return 0.0;
    };
    let (Some(s), Ok(set)) = (line.structure(structure), set.parse::<u32>()) else {
        return 0.0;
    };
    let Some(w) = s.wire_at_set(set) else {
        return 0.0;
    };
    lk_of(s, &w.label)
}

fn lk_of(s: &Structure, wire: &str) -> f64 {
    let holding: Vec<_> = s.strings_to(wire).collect();
    let [x] = holding.as_slice() else {
        // None (clamped to the structure) or a V.
        return 0.0;
    };
    let length = s.string_length(x).unwrap_or(0.0);
    if !x.strain {
        // An I-string, or the string below a V.
        return length;
    }
    // A tension string: from a node (a TAK) hanging on one suspension
    // string, that string's length; else 0.
    if s.point(&x.top).is_none() && s.wire(&x.top).is_some() {
        let above: Vec<_> = s.strings_to(&x.top).collect();
        if let [y] = above.as_slice()
            && !y.strain
        {
            return s.string_length(y).unwrap_or(0.0);
        }
    }
    0.0
}

/// The conductors of a solved line: every section span (not jumpers),
/// with its cable's diameter and mass, its ends' l_k and the spacers on
/// it.
pub fn conductors(
    line: &Line,
    shapes: &[SectionShape],
    library: &ohl_cables::Library,
) -> Vec<Conductor> {
    wires_of(line, shapes, &[])
        .into_iter()
        .map(|wire| {
            let section = line.sections.iter().find(|s| s.id == wire.section);
            let cable = section.and_then(|s| library.get(&s.cable));
            let lk = match wire.ends.as_slice() {
                [a, b] => [lk_at(line, a), lk_at(line, b)],
                _ => [0.0, 0.0],
            };
            // Its span points holding spacers, where solved.
            let solved = shapes.iter().find(|s| s.section == wire.section);
            let spacers = section
                .into_iter()
                .flat_map(|s| &s.span_points)
                .filter(|p| p.span == wire.span)
                .flat_map(|p| {
                    let at = solved.and_then(|x| {
                        x.span_points
                            .iter()
                            .find(|(l, _)| *l == p.label)
                            .map(|(_, q)| *q)
                    });
                    line.spacers
                        .iter()
                        .filter(|x| {
                            [&x.a, &x.b]
                                .iter()
                                .any(|e| e.section == wire.section && e.point == p.label)
                        })
                        .filter_map(move |x| at.map(|q| (x.id.clone(), q)))
                })
                .collect();
            Conductor {
                diameter_mm: cable.map_or(0.0, |c| c.diameter.si() * 1000.0),
                mass_kg_m: cable.map_or(0.0, |c| c.weight().si() / 9.806_65),
                lk,
                system: section.and_then(|s| s.system.clone()),
                fields: section.map(|s| s.fields.clone()).unwrap_or_default(),
                spacers,
                wire,
            }
        })
        .collect()
}

/// The largest sag of `points` between consecutive clamps `at` (points
/// of it; with its ends): vertically below the chord of each stretch, m.
fn largest_sag_between(points: &[P3], at: &[P3]) -> f64 {
    let (Some(f), Some(l)) = (points.first(), points.last()) else {
        return 0.0;
    };
    let flat = (l[0] - f[0]).hypot(l[1] - f[1]);
    if flat <= 0.0 {
        return 0.0;
    }
    let t =
        |p: &P3| ((p[0] - f[0]) * (l[0] - f[0]) + (p[1] - f[1]) * (l[1] - f[1])) / (flat * flat);
    let mut clamps: Vec<(f64, f64)> = std::iter::once(*f)
        .chain(at.iter().copied())
        .chain(std::iter::once(*l))
        .map(|p| (t(&p), p[2]))
        .collect();
    clamps.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut sag = 0.0f64;
    for w in clamps.windows(2) {
        let ((t0, z0), (t1, z1)) = (w[0], w[1]);
        if t1 - t0 <= 1e-12 {
            continue;
        }
        for p in points {
            let s = t(p);
            if s > t0 && s < t1 {
                let chord = z0 + (z1 - z0) * (s - t0) / (t1 - t0);
                sag = sag.max(chord - p[2]);
            }
        }
    }
    sag
}

/// l_k at share `t` of a conductor's length, linear between its ends.
fn lk_linear(ends: [f64; 2], t: f64) -> f64 {
    ends[0] + (ends[1] - ends[0]) * t
}

/// The structures a wire hangs between, sorted.
fn span_key(w: &Wire) -> Option<[String; 2]> {
    if w.ends.len() != 2 || w.section.starts_with("jumper:") {
        return None;
    }
    let s = |e: &str| e.rsplit_once(':').map_or(e, |x| x.0).to_owned();
    let (a, b) = (s(&w.ends[0]), s(&w.ends[1]));
    Some(if a <= b { [a, b] } else { [b, a] })
}

/// A conductor's polylines: the centre line, or each subconductor of a
/// bundle (offsets across and up around it).
fn subconductors(w: &Wire) -> Vec<Vec<P3>> {
    let pts = &w.points;
    if w.bundle.len() <= 1 || pts.len() < 2 {
        return vec![pts.clone()];
    }
    let (f, l) = (pts[0], pts[pts.len() - 1]);
    let flat = (l[0] - f[0]).hypot(l[1] - f[1]);
    let right = if flat > 0.0 {
        [(l[1] - f[1]) / flat, -(l[0] - f[0]) / flat]
    } else {
        [1.0, 0.0]
    };
    w.bundle
        .iter()
        .map(|o| {
            pts.iter()
                .map(|p| [p[0] + right[0] * o[0], p[1] + right[1] * o[0], p[2] + o[1]])
                .collect()
        })
        .collect()
}

fn dist(a: P3, b: P3) -> f64 {
    (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f64>().sqrt()
}

/// A polyline with its piece lengths and the boxes of runs of pieces,
/// for fast closest-point searches.
struct Poly<'a> {
    points: &'a [P3],
    /// Length from the first point to each point, m.
    run: Vec<f64>,
    /// Bounding box (min, max) of each run of `CHUNK` pieces.
    boxes: Vec<(P3, P3)>,
}

/// Pieces per box.
const CHUNK: usize = 8;

impl<'a> Poly<'a> {
    fn new(points: &'a [P3]) -> Self {
        let mut run = vec![0.0];
        for w in points.windows(2) {
            run.push(run[run.len() - 1] + dist(w[0], w[1]));
        }
        let boxes = (0..points.len().saturating_sub(1))
            .step_by(CHUNK)
            .map(|k| {
                let ps = &points[k..(k + CHUNK + 1).min(points.len())];
                let mut lo = ps[0];
                let mut hi = ps[0];
                for p in ps {
                    for c in 0..3 {
                        lo[c] = lo[c].min(p[c]);
                        hi[c] = hi[c].max(p[c]);
                    }
                }
                (lo, hi)
            })
            .collect();
        Self { points, run, boxes }
    }

    fn total(&self) -> f64 {
        self.run[self.run.len() - 1]
    }

    /// Bounding box of the whole polyline.
    fn bounds(&self) -> (P3, P3) {
        self.boxes.iter().fold(
            ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]),
            |(lo, hi), (a, b)| {
                (
                    [0, 1, 2].map(|c| lo[c].min(a[c])),
                    [0, 1, 2].map(|c| hi[c].max(b[c])),
                )
            },
        )
    }

    /// Stations every `spacing` of its 3D length, its ends included:
    /// (point, length from its first point, length it stands for).
    fn stations(&self, spacing: f64) -> Vec<(P3, f64, f64)> {
        let total = self.total();
        if total <= 0.0 {
            return vec![(self.points[0], 0.0, 0.0)];
        }
        let n = (total / spacing.max(1e-3)).ceil().max(1.0) as usize;
        let step = total / n as f64;
        let mut k = 0;
        (0..=n)
            .map(|i| {
                let s = step * i as f64;
                while k + 2 < self.points.len() && self.run[k + 1] < s {
                    k += 1;
                }
                let l = self.run[k + 1] - self.run[k];
                let t = if l > 0.0 {
                    ((s - self.run[k]) / l).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let (a, b) = (self.points[k], self.points[k + 1]);
                let weight = if i == 0 || i == n { step / 2.0 } else { step };
                ([0, 1, 2].map(|c| a[c] + (b[c] - a[c]) * t), s, weight)
            })
            .collect()
    }

    /// The closest point to `p`: (point, length from the first point,
    /// distance); none farther than `cutoff`.
    fn closest(&self, p: P3, cutoff: f64) -> Option<(P3, f64, f64)> {
        let mut order: Vec<(f64, usize)> = self
            .boxes
            .iter()
            .enumerate()
            .map(|(i, (lo, hi))| {
                let d2: f64 = (0..3)
                    .map(|c| (lo[c] - p[c]).max(p[c] - hi[c]).max(0.0).powi(2))
                    .sum();
                (d2.sqrt(), i)
            })
            .filter(|(d, _)| *d <= cutoff)
            .collect();
        order.sort_by(|x, y| x.0.total_cmp(&y.0));
        let mut best: Option<(P3, f64, f64)> = None;
        for (bound, i) in order {
            if best.is_some_and(|b| bound >= b.2) {
                break;
            }
            let start = i * CHUNK;
            let end = (start + CHUNK).min(self.points.len() - 1);
            for k in start..end {
                let (a, b) = (self.points[k], self.points[k + 1]);
                let d = [0, 1, 2].map(|c| b[c] - a[c]);
                let dd: f64 = d.iter().map(|x| x * x).sum();
                let t = if dd > 0.0 {
                    ((0..3).map(|c| (p[c] - a[c]) * d[c]).sum::<f64>() / dd).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let q = [0, 1, 2].map(|c| a[c] + d[c] * t);
                let e = dist(p, q);
                if e <= cutoff && best.is_none_or(|b| e < b.2) {
                    let s = self.run[k] + (self.run[k + 1] - self.run[k]) * t;
                    best = Some((q, s, e));
                }
            }
        }
        best
    }
}

/// The sag of a polyline at `p` (a point of it): vertically below its
/// chord (the straight line between its ends), m; 0 at the ends.
fn local_sag(points: &[P3], p: P3) -> f64 {
    let (f, l) = (points[0], points[points.len() - 1]);
    let flat = (l[0] - f[0]).hypot(l[1] - f[1]);
    if flat <= 0.0 {
        return 0.0;
    }
    let t = (((p[0] - f[0]) * (l[0] - f[0]) + (p[1] - f[1]) * (l[1] - f[1])) / (flat * flat))
        .clamp(0.0, 1.0);
    (f[2] + (l[2] - f[2]) * t - p[2]).max(0.0)
}

/// Checks the pairs of `conductors` hanging between the same two
/// structures, and with [`PhaseDistanceSettings::nearby`] those coming
/// within its distance elsewhere. Not paired: jumpers, two earth wires,
/// two conductors of the same circuit and phase (one potential), two
/// sharing a clamp. What is required comes from an [`Evaluator`] (one
/// call per pair), made by `make` for each core the pairs are measured
/// on. Exit clause: the budget's time limit and cancellation (checked per
/// pair).
pub fn check<E: Evaluator>(
    settings: &PhaseDistanceSettings,
    conductors: &[Conductor],
    budget: &Budget,
    make: impl Fn() -> Result<E, String> + Sync,
) -> Result<Vec<PhaseDistancePair>, String> {
    let run = budget.start();
    let ticked = |c: &Conductor| {
        settings
            .circuits
            .as_ref()
            .is_none_or(|set| set.contains(&c.wire.circuit))
    };
    let keys: Vec<Option<[String; 2]>> = conductors.iter().map(|c| span_key(&c.wire)).collect();
    let subs: Vec<Vec<Vec<P3>>> = conductors.iter().map(|c| subconductors(&c.wire)).collect();
    let polys: Vec<Vec<Poly>> = subs
        .iter()
        .map(|s| {
            s.iter()
                .filter(|p| p.len() >= 2)
                .map(|p| Poly::new(p))
                .collect()
        })
        .collect();
    let bounds: Vec<Option<(P3, P3)>> = polys
        .iter()
        .map(|ps| {
            ps.iter().map(Poly::bounds).reduce(|(a, b), (c, d)| {
                (
                    [0, 1, 2].map(|k| a[k].min(c[k])),
                    [0, 1, 2].map(|k| b[k].max(d[k])),
                )
            })
        })
        .collect();
    let apart = |i: usize, j: usize| match (bounds[i], bounds[j]) {
        (Some((a, b)), Some((c, d))) => (0..3)
            .map(|k| (c[k] - b[k]).max(a[k] - d[k]).max(0.0).powi(2))
            .sum::<f64>()
            .sqrt(),
        _ => f64::INFINITY,
    };
    // The pairs to measure (cheap tests first).
    let mut candidates = Vec::new();
    for i in 0..conductors.len() {
        let Some(key) = &keys[i] else { continue };
        if !ticked(&conductors[i]) || polys[i].is_empty() {
            continue;
        }
        for j in i + 1..conductors.len() {
            let Some(other) = &keys[j] else { continue };
            if !ticked(&conductors[j]) || polys[j].is_empty() {
                continue;
            }
            let same_span = other == key;
            if !same_span && (!settings.nearby || apart(i, j) > settings.max_distance) {
                continue;
            }
            let (a, b) = (&conductors[i], &conductors[j]);
            if !a.wire.phase.is_empty()
                && a.wire.circuit == b.wire.circuit
                && a.wire.phase == b.wire.phase
            {
                // One potential.
                continue;
            }
            if a.wire.ends.iter().any(|e| b.wire.ends.contains(e)) {
                // Connected at a clamp (the next span of the same wire).
                continue;
            }
            if a.wire.kv().unwrap_or(0.0).max(b.wire.kv().unwrap_or(0.0)) <= 0.0 {
                // Two earth wires: nothing to keep apart.
                continue;
            }
            candidates.push((i, j, same_span));
        }
    }
    // A station measured: (direction, polyline of the first conductor,
    // station on it), the length it stands for, the points and where
    // along each (in the pair's order).
    struct Measured {
        group: (bool, usize, usize),
        weight: f64,
        at: [P3; 2],
        along: [f64; 2],
    }
    let measure = |evaluator: &mut E,
                   i: usize,
                   j: usize,
                   same_span: bool|
     -> Result<Option<PhaseDistancePair>, String> {
        let (a, b) = (&conductors[i], &conductors[j]);
        // Within one electrical system and towards earth (an earth wire:
        // no potential); else between systems.
        let earth = a.wire.potential.is_none() || b.wire.potential.is_none();
        let same_system = earth
            || match (&a.system, &b.system) {
                (Some(x), Some(y)) => x == y,
                // No system given: by the circuit names.
                _ => a.wire.circuit == b.wire.circuit,
            };
        let kv = a.wire.kv().unwrap_or(0.0).max(b.wire.kv().unwrap_or(0.0));
        let all: Vec<f64> = a
            .wire
            .points
            .iter()
            .chain(&b.wire.points)
            .map(|p| p[2])
            .collect();
        let altitude = all.iter().sum::<f64>() / all.len().max(1) as f64;
        // Spacers between these two in the span: each one's largest sag
        // between its clamps.
        let joined: Vec<&String> = a
            .spacers
            .iter()
            .filter(|(id, _)| b.spacers.iter().any(|(x, _)| x == id))
            .map(|(id, _)| id)
            .collect();
        let spacer_sag = (same_span && !joined.is_empty()).then(|| {
            [a, b].map(|c| {
                let at: Vec<P3> = c
                    .spacers
                    .iter()
                    .filter(|(id, _)| joined.contains(&id))
                    .map(|(_, p)| *p)
                    .collect();
                largest_sag_between(&c.wire.points, &at)
            })
        });
        // Same span: every station; nearby: where within the distance.
        let cutoff = if same_span {
            f64::INFINITY
        } else {
            settings.max_distance
        };
        let mut rows: Vec<f64> = Vec::new();
        let mut measured: Vec<Measured> = Vec::new();
        // Stations on each conductor in turn, the closest point on each
        // subconductor of the other.
        for swap in [false, true] {
            let (fi, si) = if swap { (j, i) } else { (i, j) };
            let (cf, cs) = (&conductors[fi], &conductors[si]);
            for (n, first) in polys[fi].iter().enumerate() {
                for (k, (p, sp, weight)) in first.stations(settings.spacing).into_iter().enumerate()
                {
                    for second in &polys[si] {
                        let Some((q, sq, d)) = second.closest(p, cutoff) else {
                            continue;
                        };
                        let dz = (p[2] - q[2]).abs();
                        let beta = if d > 0.0 {
                            (dz / d).clamp(0.0, 1.0).acos().to_degrees()
                        } else {
                            0.0
                        };
                        let fp = local_sag(first.points, p);
                        let fq = local_sag(second.points, q);
                        let lp = lk_linear(cf.lk, sp / first.total().max(1e-12));
                        let lq = lk_linear(cs.lk, sq / second.total().max(1e-12));
                        let (at, along, f, l) = if swap {
                            ([q, p], [sq, sp], [fq, fp], [lq, lp])
                        } else {
                            ([p, q], [sp, sq], [fp, fq], [lp, lq])
                        };
                        rows.extend([d, beta, f[0], f[1], l[0], l[1]]);
                        measured.push(Measured {
                            group: (swap, n, k),
                            weight,
                            at,
                            along,
                        });
                    }
                }
            }
        }
        if measured.is_empty() {
            return Ok(None);
        }
        let pair = PairGeometry {
            a,
            b,
            same_system,
            same_span,
            altitude,
            spacer_sag,
        };
        let req = evaluator.evaluate(&pair, &rows)?;
        let per = req.per_station;
        if per == 0 || req.values.len() != per * measured.len() {
            return Err(format!(
                "the check gave {} numbers for {} stations ({per} each)",
                req.values.len(),
                measured.len()
            ));
        }
        let station = |m: usize| {
            let r = &rows[m * STATION_VALUES..(m + 1) * STATION_VALUES];
            let v = &req.values[m * per..(m + 1) * per];
            Station {
                at: measured[m].at,
                along: measured[m].along,
                distance: r[0],
                required: v[0],
                sag: [r[2], r[3]],
                lk: [r[4], r[5]],
                beta: r[1],
                values: v[1..].to_vec(),
            }
        };
        let margin = |m: usize| rows[m * STATION_VALUES] - req.values[m * per];
        // Per station the closest subconductor (the smallest margin); the
        // length below per conductor and subconductor measured from.
        let mut worst: Option<usize> = None;
        let mut length_below = 0.0f64;
        let mut below = 0.0;
        let mut m = 0;
        while m < measured.len() {
            let g = measured[m].group;
            let mut best = m;
            let mut e = m + 1;
            while e < measured.len() && measured[e].group == g {
                if margin(e) < margin(best) {
                    best = e;
                }
                e += 1;
            }
            if margin(best) < 0.0 {
                below += measured[best].weight;
            }
            if worst.is_none_or(|w| margin(best) < margin(w)) {
                worst = Some(best);
            }
            // The end of a polyline's stations.
            if e == measured.len() || (measured[e].group.0, measured[e].group.1) != (g.0, g.1) {
                length_below = length_below.max(below);
                below = 0.0;
            }
            m = e;
        }
        Ok(worst.map(|w| PhaseDistancePair {
            a: i,
            b: j,
            same_system,
            kv,
            same_span,
            altitude,
            spacer_sag,
            rule: req.rule.clone(),
            worst: station(w),
            length_below,
        }))
    };
    // The pairs shared out over the cores, in their order.
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(candidates.len())
        .max(1);
    let next = AtomicUsize::new(0);
    let stopped: Mutex<Option<String>> = Mutex::new(None);
    let stop = |why: String| {
        let mut s = stopped.lock().expect("not poisoned");
        if s.is_none() {
            *s = Some(why);
        }
    };
    let mut found: Vec<(usize, PhaseDistancePair)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut mine = Vec::new();
                    let mut evaluator = match make() {
                        Ok(e) => e,
                        Err(e) => {
                            stop(e);
                            return mine;
                        }
                    };
                    loop {
                        if let Err(why) = run.check() {
                            stop(format!("phase distances: {why}"));
                            return mine;
                        }
                        if stopped.lock().expect("not poisoned").is_some() {
                            return mine;
                        }
                        let k = next.fetch_add(1, Ordering::Relaxed);
                        let Some(&(i, j, same_span)) = candidates.get(k) else {
                            return mine;
                        };
                        match measure(&mut evaluator, i, j, same_span) {
                            Ok(Some(pair)) => mine.push((k, pair)),
                            Ok(None) => {}
                            Err(e) => {
                                stop(e);
                                return mine;
                            }
                        }
                    }
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|w| w.join().expect("a phase distance worker panicked"))
            .collect()
    });
    if let Some(stop) = stopped.into_inner().expect("not poisoned") {
        return Err(stop);
    }
    found.sort_by_key(|(k, _)| *k);
    Ok(found.into_iter().map(|(_, pair)| pair).collect())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use ohl_line::electrical::{Potential, SystemKind};
    use ohl_line::{InsulatorString, StructurePoint, WirePoint};

    /// A test code (LeV-like, its numbers for the tests only): z =
    /// k·√(f + l_k) + C with k from 0.70 (β 0°) to 0.60 (β 90°), C 2.64 m
    /// within a system, 3.37 m between, +14 % per 1000 m above 1000 m;
    /// with spacers k·√d_f + C, at least 0.90 m. The station's numbers
    /// after z: k of each, C.
    pub(crate) struct TestCode;

    impl Evaluator for TestCode {
        fn evaluate(&mut self, pair: &PairGeometry, s: &[f64]) -> Result<Required, String> {
            let c = if pair.same_system { 2.64 } else { 3.37 }
                * (1.0 + 0.14 * ((pair.altitude - 1000.0).max(0.0) / 1000.0));
            let mut values = Vec::new();
            for r in s.chunks(STATION_VALUES) {
                let k = 0.70 - 0.10 * (r[1] / 90.0).clamp(0.0, 1.0);
                let z = match pair.spacer_sag {
                    Some(df) => (k * df[0].sqrt()).max(k * df[1].sqrt()) + c,
                    None => (k * (r[2] + r[4]).sqrt()).max(k * (r[3] + r[5]).sqrt()) + c,
                };
                values.extend([z.max(0.9), k, k, c]);
            }
            Ok(Required {
                values,
                per_station: 4,
                rule: if pair.spacer_sag.is_some() {
                    "spacers"
                } else {
                    "span"
                }
                .into(),
            })
        }
    }

    fn kv(u: f64) -> Option<Potential> {
        Some(Potential {
            nominal_kv: u,
            kind: SystemKind::Ac,
            frequency_hz: Some(50.0),
            to_earth: [u / 3f64.sqrt(), 0.0],
        })
    }

    /// A conductor between T1 and T2 along x (300 m), its clamps at
    /// (0, y, z) and (300, y, z + rise), sagging `sag` at mid-span
    /// (a parabola below the chord), 301 points.
    fn conductor(
        circuit: &str,
        set: u32,
        y: f64,
        z: f64,
        rise: f64,
        sag: f64,
        lk: [f64; 2],
    ) -> Conductor {
        let points = (0..=300)
            .map(|i| {
                let x = f64::from(i);
                let t = x / 300.0;
                [x, y, z + rise * t - 4.0 * sag * t * (1.0 - t)]
            })
            .collect();
        Conductor {
            wire: Wire {
                section: format!("S{set}"),
                span: 0,
                circuit: circuit.into(),
                phase: String::new(),
                potential: kv(380.0),
                bundle: Vec::new(),
                points,
                ends: vec![format!("T1:{set}"), format!("T2:{set}")],
            },
            diameter_mm: 30.0,
            mass_kg_m: 3.0,
            lk,
            system: Some(circuit.into()),
            fields: Fields::new(),
            spacers: Vec::new(),
        }
    }

    /// The default settings (the conductors are at the low level).
    fn low() -> PhaseDistanceSettings {
        PhaseDistanceSettings::default()
    }

    fn run(c: &[Conductor], s: &PhaseDistanceSettings) -> Vec<PhaseDistancePair> {
        check(s, c, &Budget::new(), || Ok(TestCode)).unwrap()
    }

    /// One conductor 6 m above the other (β = 0°), 380 kV, the same
    /// circuit, 12 m sag, 4.5 m suspension strings: at mid-span
    /// z = 0.70·√16.5 + 2.64 = 5.483 m against 6 m — the worst station;
    /// at the clamps z = 0.70·√4.5 + 2.64.
    #[test]
    fn one_above_the_other() {
        let a = conductor("C1", 1, 0.0, 30.0, 0.0, 12.0, [4.5, 4.5]);
        let b = conductor("C1", 2, 0.0, 24.0, 0.0, 12.0, [4.5, 4.5]);
        let pairs = run(&[a, b], &low());
        assert_eq!(pairs.len(), 1);
        let w = &pairs[0].worst;
        assert!((w.distance - 6.0).abs() < 1e-9);
        assert!(w.beta.abs() < 1e-6);
        assert!((w.sag[0] - 12.0).abs() < 1e-9, "{:?}", w.sag);
        let z = 0.70 * 16.5f64.sqrt() + 2.64;
        assert!((w.required - z).abs() < 1e-9, "{} {z}", w.required);
        assert_eq!(pairs[0].length_below, 0.0);
        // 5 m apart: below around mid-span only.
        let a = conductor("C1", 1, 0.0, 30.0, 0.0, 12.0, [4.5, 4.5]);
        let b = conductor("C1", 2, 0.0, 25.0, 0.0, 12.0, [4.5, 4.5]);
        let p = &run(&[a, b], &low())[0];
        assert!(p.worst.margin() < 0.0 && p.length_below > 0.0);
        assert!(
            p.length_below < 300.0,
            "not at the clamps: {}",
            p.length_below
        );
    }

    /// The length below the requirement: 5 m apart, f = 12 m, l_k = 4.5 m
    /// at both ends: z > 5 m where 48·t(1 − t) + 4.5 > (2.36 / 0.70)²,
    /// t from 0.177 to 0.823, about 194 m of the 300 m — counted once,
    /// not per measuring direction.
    #[test]
    fn length_below() {
        let a = conductor("C1", 1, 0.0, 30.0, 0.0, 12.0, [4.5, 4.5]);
        let b = conductor("C1", 2, 0.0, 25.0, 0.0, 12.0, [4.5, 4.5]);
        let p = &run(&[a, b], &low())[0];
        let t = 0.5 - (0.25 - (((2.36f64 / 0.70).powi(2) - 4.5) / 48.0)).sqrt();
        let expected = 300.0 * (1.0 - 2.0 * t);
        assert!(
            (p.length_below - expected).abs() < 3.0,
            "{} {expected}",
            p.length_below
        );
        // Where along: the worst at mid-span.
        assert!(
            (p.worst.along[0] - 150.0).abs() < 2.0,
            "{:?}",
            p.worst.along
        );
    }

    /// Nearby pairs: a parallel line 6 m to the side on its own
    /// structures is checked only when asked, within the distance; two
    /// conductors of one circuit and phase, or sharing a clamp, never.
    #[test]
    fn nearby_pairs() {
        let a = conductor("C1", 1, 0.0, 30.0, 0.0, 12.0, [4.5, 4.5]);
        let mut b = conductor("C2", 1, 6.0, 30.0, 0.0, 12.0, [4.5, 4.5]);
        b.wire.ends = vec!["T3:1".into(), "T4:1".into()];
        assert!(run(&[a.clone(), b.clone()], &low()).is_empty());
        let near = PhaseDistanceSettings {
            nearby: true,
            ..low()
        };
        let pairs = run(&[a.clone(), b.clone()], &near);
        assert_eq!(pairs.len(), 1);
        assert!(!pairs[0].same_span && !pairs[0].same_system);
        assert!((pairs[0].worst.distance - 6.0).abs() < 1e-6);
        let far = PhaseDistanceSettings {
            max_distance: 5.0,
            ..near.clone()
        };
        assert!(run(&[a.clone(), b.clone()], &far).is_empty());
        // One circuit and phase: one potential.
        let (mut a1, mut b1) = (a.clone(), b.clone());
        a1.wire.phase = "L1".into();
        b1.wire.phase = "L1".into();
        b1.wire.circuit = "C1".into();
        b1.system = Some("C1".into());
        assert!(run(&[a1, b1], &near).is_empty());
        // Sharing a clamp: the next span.
        let mut c = b.clone();
        c.wire.ends = vec!["T2:1".into(), "T4:1".into()];
        assert!(run(&[a, c], &near).is_empty());
    }

    /// Side by side (β = 90°): k = 0.60; two circuits: C₂.
    #[test]
    fn side_by_side_and_two_circuits() {
        let a = conductor("C1", 1, 0.0, 30.0, 0.0, 12.0, [4.5, 4.5]);
        let b = conductor("C2", 2, 8.0, 30.0, 0.0, 12.0, [4.5, 4.5]);
        let p = &run(&[a, b], &low())[0];
        assert!(!p.same_system);
        assert!((p.worst.beta - 90.0).abs() < 1e-6);
        let z = 0.60 * 16.5f64.sqrt() + 3.37;
        assert!(
            (p.worst.required - z).abs() < 1e-9,
            "{} {z}",
            p.worst.required
        );
    }

    /// l_k linear between the ends: a tension string at one (0), a 3 m
    /// suspension string at the other: 1.5 m at mid-span.
    #[test]
    fn lk_is_linear_between_the_ends() {
        let a = conductor("C1", 1, 0.0, 30.0, 0.0, 12.0, [0.0, 3.0]);
        let b = conductor("C1", 2, 0.0, 24.0, 0.0, 12.0, [0.0, 3.0]);
        let p = &run(&[a, b], &low())[0];
        // l_k at the worst point: 3 m × its share of the length from the
        // tension end (the worst is a little past mid-span: f + l_k peaks
        // at t = 51/96, where l_k ≈ 1.59 m).
        let at = p.worst.at[0][0] / 300.0;
        assert!(
            (p.worst.lk[0] - 3.0 * at).abs() < 0.01,
            "{:?} at {at}",
            p.worst.lk
        );
        assert!(p.worst.lk[0] > 1.5 && p.worst.lk[0] < 1.65);
        // The larger of the two conductors' terms.
        let k = &p.worst.values[..2];
        let term = |i: usize| k[i] * (p.worst.sag[i] + p.worst.lk[i]).sqrt();
        assert!(p.worst.beta < 1.0 && (k[0] - 0.70).abs() < 0.002);
        let z = term(0).max(term(1)) + 2.64;
        assert!((p.worst.required - z).abs() < 1e-9);
        // Mid-span itself: l_k 1.5 exactly.
        assert!((lk_linear([0.0, 3.0], 0.5) - 1.5).abs() < 1e-12);
    }

    /// Above 1000 m: C grows by 14 % per 1000 m of the mean conductor
    /// height; circuits not ticked are not checked.
    #[test]
    fn altitude_and_circuits() {
        let a = conductor("C1", 1, 0.0, 2030.0, 0.0, 12.0, [4.5, 4.5]);
        let b = conductor("C1", 2, 0.0, 2024.0, 0.0, 12.0, [4.5, 4.5]);
        let p = &run(&[a.clone(), b.clone()], &low())[0];
        // Mean height 2027 − 2·12·(2/3)… the parabola's mean: 2019 m.
        let f = 1.0 + 0.14 * (p.altitude - 1000.0) / 1000.0;
        assert!((p.worst.values[2] - 2.64 * f).abs() < 1e-9);
        assert!(p.altitude > 2000.0 && p.altitude < 2030.0, "{}", p.altitude);
        let s = PhaseDistanceSettings {
            circuits: Some(BTreeSet::from(["C2".to_owned()])),
            ..low()
        };
        assert!(run(&[a, b], &s).is_empty());
    }

    /// The string kinds: an I-string its length; a V, a tension string,
    /// a wire clamped to the structure 0; the string below a V its
    /// length; a TAK its suspension string's length.
    #[test]
    fn lk_by_string_kind() {
        let p = |label: &str, z: f64| StructurePoint {
            description: String::new(),
            label: label.into(),
            position: [0.0, 0.0, z],
        };
        let w = |label: &str, z: f64| WirePoint {
            label: label.into(),
            position: [0.0, 0.0, z],
            ..Default::default()
        };
        let s = |label: &str, top: &str, wire: &str, strain: bool| InsulatorString {
            label: label.into(),
            top: top.into(),
            wire: wire.into(),
            strain,
            ..Default::default()
        };
        let st = Structure {
            id: "T".into(),
            position: [0.0; 3],
            orientation_deg: 0.0,
            structure_type: None,
            extension: None,
            description: String::new(),
            height_adjust: 0.0,
            points: vec![p("A", 30.0), p("B", 30.0)],
            strings: vec![
                s("I", "A", "WI", false),
                s("Va", "A", "WV", false),
                s("Vb", "B", "WV", false),
                s("T", "A", "WT", true),
                s("UVa", "A", "N", false),
                s("UVb", "B", "N", false),
                s("Below", "N", "WB", false),
                s("Sus", "A", "K", false),
                s("Ta", "K", "WK", true),
            ],
            wires: vec![
                w("WI", 27.0),
                w("WV", 26.0),
                w("WT", 30.0),
                w("N", 27.0),
                w("WB", 25.5),
                w("K", 28.0),
                w("WK", 28.0),
                w("WC", 20.0),
            ],
            omit: Vec::new(),
            omit_sets: Vec::new(),
            members: Vec::new(),
            routes: Vec::new(),
            fields: Default::default(),
        };
        let mut st = st;
        // The tension string runs along: its wire point 3 m away.
        st.wires[2].position = [3.0, 0.0, 30.0];
        st.wires[6].position = [3.0, 0.0, 28.0];
        assert!((lk_of(&st, "WI") - 3.0).abs() < 1e-9);
        assert_eq!(lk_of(&st, "WV"), 0.0);
        assert_eq!(lk_of(&st, "WT"), 0.0);
        assert!((lk_of(&st, "WB") - 1.5).abs() < 1e-9);
        assert!((lk_of(&st, "WK") - 2.0).abs() < 1e-9);
        assert_eq!(lk_of(&st, "WC"), 0.0);
    }

    /// Two conductors joined by a spacer at mid-span: each one's largest
    /// sag between its clamps (a quarter of the sag for a parabola halved),
    /// handed on; the rule follows. A spacer on one only: not joined.
    #[test]
    fn spacers_give_the_sag_between_their_clamps() {
        let mut a = conductor("C1", 1, 0.0, 30.0, 0.0, 12.0, [4.5, 4.5]);
        let mut b = conductor("C1", 2, 0.0, 24.0, 0.0, 12.0, [4.5, 4.5]);
        a.spacers = vec![("SP".into(), [150.0, 0.0, 18.0])];
        b.spacers = vec![("SP".into(), [150.0, 0.0, 12.0])];
        let p = &run(&[a.clone(), b.clone()], &low())[0];
        let df = p.spacer_sag.expect("joined");
        // 12 m at mid-span, the half spans' parabolas sag 3 m.
        assert!(
            (df[0] - 3.0).abs() < 0.01 && (df[1] - 3.0).abs() < 0.01,
            "{df:?}"
        );
        assert_eq!(p.rule, "spacers");
        let k = p.worst.values[0];
        assert!((p.worst.required - (k * df[0].max(df[1]).sqrt() + 2.64)).abs() < 1e-9);
        b.spacers[0].0 = "other".into();
        let p = &run(&[a, b], &low())[0];
        assert!(p.spacer_sag.is_none() && p.rule == "span");
        // Two spacers at thirds: the largest of the three stretches.
        let pts: Vec<P3> = (0..=300)
            .map(|i| {
                let t = f64::from(i) / 300.0;
                [f64::from(i), 0.0, 30.0 - 48.0 * t * (1.0 - t)]
            })
            .collect();
        let at = |x: usize| pts[x];
        let s = largest_sag_between(&pts, &[at(100), at(200)]);
        // A parabola's sag over a third of the span: 12/9 m.
        assert!((s - 12.0 / 9.0).abs() < 0.01, "{s}");
    }

    /// An evaluator's error stops the check and is told.
    #[test]
    fn an_evaluator_error_is_told() {
        struct Broken;
        impl Evaluator for Broken {
            fn evaluate(&mut self, _: &PairGeometry, _: &[f64]) -> Result<Required, String> {
                Err("no values for 380 kV".into())
            }
        }
        let a = conductor("C1", 1, 0.0, 30.0, 0.0, 12.0, [4.5, 4.5]);
        let b = conductor("C1", 2, 0.0, 24.0, 0.0, 12.0, [4.5, 4.5]);
        let e = check(&low(), &[a, b], &Budget::new(), || Ok(Broken)).unwrap_err();
        assert!(e.contains("380 kV"), "{e}");
    }
}
