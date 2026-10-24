// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Distance between conductors under unequal wind: the measuring. Some
//! codes (e.g. the Swiss LeV, Annex 6, 2.2, as ESTI's directive 240
//! applies it) ask that two conductors keep a fixed distance (there C3
//! within a system, C4 between systems) when the wind blows one of them
//! at a pressure p, from none up to a case's, and the other at a share of
//! p only (there 60 % to 100 %). The distances and their rules are a
//! check plugin's (see `docs/development/phase-distance-check.md`), not
//! OpenOHL's.
//!
//! OpenOHL measures: the host solves the line at the wind levels of
//! [`levels`] (the case's temperature and ice, its wind pressure stepped,
//! from each side); here the pairs (as the distance between conductors in
//! the span pairs them) are measured at every combination of a level p
//! on one conductor and a level from `low_share`·p to p on the other,
//! both ways, on each side, and each pair keeps its closest one: the
//! distance between their axes (less the polylines' error bound, on the
//! safe side). The [`Evaluator`] (the plugin) says what is required,
//! once per pair; the check keeps the margin.
//!
//! The closest approach is exact: the shortest distance between the two
//! wires' polylines, segment against segment (owner, 2026-10-24: not at
//! stations). Fast: the line is solved once per level for every wire
//! (the levels shared by all pairs); per pair the combinations are taken
//! nearest first by the wires' boxes and those whose boxes are farther
//! than the closest found are left out, as are the pieces of a wire
//! farther than it. The levels include `low_share`·p of every step p, so
//! the largest difference in wind is always among them.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use ohl_core::budget::Budget;
use ohl_core::units::Stress;
use ohl_line::solve::SectionShape;
use ohl_weather::{WeatherCase, WindSide};

use crate::phase_distance::{
    CHUNK, Conductor, Evaluator, PairGeometry, PhaseDistanceSettings, Poly, STATION_VALUES,
    candidate_pairs, kind_of, mean_height, subconductors,
};

type P3 = [f64; 3];

/// How the measuring runs.
#[derive(Debug, Clone, PartialEq)]
pub struct UnequalWindSettings {
    /// Stations, circuits and nearby pairs as for the distance between
    /// conductors in the span.
    pub pairs: PhaseDistanceSettings,
    /// The other conductor's least share of the wind on the first.
    pub low_share: f64,
}

impl Default for UnequalWindSettings {
    fn default() -> Self {
        Self {
            pairs: PhaseDistanceSettings::default(),
            low_share: 0.6,
        }
    }
}

/// The wind pressures to solve the line at, Pa, ascending: every `step`
/// from 0 to `max` (`max` included), and `low_share` of each.
pub fn levels(max: f64, step: f64, low_share: f64) -> Result<Vec<f64>, String> {
    if !(max.is_finite() && max >= 0.0) {
        return Err(format!("the wind pressure {max} Pa is not a pressure"));
    }
    if !(step.is_finite() && step > 0.0) {
        return Err(format!("the wind step {step} Pa is not above 0"));
    }
    if !(low_share.is_finite() && (0.0..=1.0).contains(&low_share)) {
        return Err(format!("the share {low_share} is not from 0 to 1"));
    }
    let n = (max / step).ceil() as usize;
    if n > 1000 {
        return Err(format!("{n} wind steps: take a step larger than {step} Pa"));
    }
    let steps: Vec<f64> = (0..=n).map(|k| (step * k as f64).min(max)).collect();
    let mut all: Vec<f64> = steps.iter().flat_map(|p| [*p, p * low_share]).collect();
    all.sort_by(f64::total_cmp);
    // Within a millipascal: one level.
    all.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
    Ok(all)
}

/// The line solved at the wind levels of [`levels`] up to `case`'s
/// pressure by `step` (Pa): its temperature and ice kept, still air
/// once, the other levels from each side. `solve` solves the line in a
/// case (its sections not solved: notes); the levels solved side by side
/// on the cores. The conductors with [`crate::phase_distance::conductors`].
pub fn solve_levels(
    line: &ohl_line::Line,
    library: &ohl_cables::Library,
    case: &WeatherCase,
    step: f64,
    low_share: f64,
    solve: impl Fn(&WeatherCase) -> (Vec<SectionShape>, Vec<String>) + Sync,
) -> Result<(Vec<WindLevel>, Vec<String>), String> {
    let pressures = levels(case.wind_pressure.si(), step, low_share)?;
    let mut cases: Vec<WeatherCase> = Vec::new();
    for side in [WindSide::Right, WindSide::Left] {
        for &p in &pressures {
            if p > 0.0 || side == WindSide::Right {
                let mut c = case.clone();
                c.wind_pressure = Stress::from_pascals(p);
                c.wind_from = side;
                cases.push(c);
            }
        }
    }
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(cases.len())
        .max(1);
    let next = AtomicUsize::new(0);
    let mut solved: Vec<(usize, WindLevel, Vec<String>)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut mine = Vec::new();
                    loop {
                        let k = next.fetch_add(1, Ordering::Relaxed);
                        let Some(c) = cases.get(k) else {
                            return mine;
                        };
                        let (shapes, failed) = solve(c);
                        let level = WindLevel {
                            pressure: c.wind_pressure.si(),
                            side: c.wind_from,
                            conductors: crate::phase_distance::conductors(line, &shapes, library),
                        };
                        let notes = failed
                            .into_iter()
                            .map(|f| {
                                format!(
                                    "at {:.0} Pa from the {}: not solved: {f}",
                                    level.pressure,
                                    level.side.as_str()
                                )
                            })
                            .collect();
                        mine.push((k, level, notes));
                    }
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|w| w.join().expect("a wind level solve panicked"))
            .collect()
    });
    solved.sort_by_key(|(k, _, _)| *k);
    let mut notes = Vec::new();
    let mut out = Vec::new();
    for (_, level, n) in solved {
        notes.extend(n);
        out.push(level);
    }
    Ok((out, notes))
}

/// The line solved at one wind level.
#[derive(Debug, Clone, PartialEq)]
pub struct WindLevel {
    /// Wind pressure, Pa.
    pub pressure: f64,
    /// The side it comes from.
    pub side: WindSide,
    /// The conductors as solved ([`crate::phase_distance::conductors`]).
    pub conductors: Vec<Conductor>,
}

/// A pair of conductors checked under unequal wind.
#[derive(Debug, Clone, PartialEq)]
pub struct UnequalWindPair {
    /// First conductor (index in the conductors of the levels).
    pub a: usize,
    /// Second conductor.
    pub b: usize,
    /// Within one electrical system, or towards an earth wire.
    pub same_system: bool,
    /// Nominal voltage, the higher of the two, kV.
    pub kv: f64,
    /// Both hang between the same two structures (else a nearby pair).
    pub same_span: bool,
    /// Mean conductor height of the span in still air, m above sea level.
    pub altitude: f64,
    /// The rule the evaluator applied.
    pub rule: String,
    /// The closest points, on the first conductor and on the second.
    pub at: [P3; 2],
    /// Where along each, m of its length from its first point.
    pub along: [f64; 2],
    /// Distance between their axes there, m, less both polylines' error
    /// bound.
    pub distance: f64,
    /// Required, m.
    pub required: f64,
    /// The wind pressure on each, Pa.
    pub pressure: [f64; 2],
    /// The side the wind came from.
    pub side: WindSide,
    /// The levels of each (indices in the levels checked): the
    /// conductors there are the pair as measured.
    pub levels: [usize; 2],
    /// The two wires as measured, each at its wind: the subconductor of
    /// each the closest points are on (the conductor itself when single),
    /// to draw the pair.
    pub wires: [Vec<P3>; 2],
    /// The evaluator's own numbers (after the distance required).
    pub values: Vec<f64>,
}

impl UnequalWindPair {
    /// The distance less the required one, m (negative: a violation).
    pub fn margin(&self) -> f64 {
        self.distance - self.required
    }
}

/// The closest approach of two conductors: the distance between the
/// axes, the points, where along each (m of its length), and which
/// subconductor of each.
#[derive(Debug, Clone, Copy)]
struct Approach {
    d: f64,
    at: [P3; 2],
    along: [f64; 2],
    sub: [usize; 2],
}

/// The gap between two boxes (0 when they overlap), m.
fn box_gap(a: (P3, P3), b: (P3, P3)) -> f64 {
    (0..3)
        .map(|k| (b.0[k] - a.1[k]).max(a.0[k] - b.1[k]).max(0.0).powi(2))
        .sum::<f64>()
        .sqrt()
}

/// The closest points of segments p1–q1 and p2–q2: (distance, the
/// points, their parameters from 0 to 1).
fn segments(p1: P3, q1: P3, p2: P3, q2: P3) -> (f64, P3, P3, f64, f64) {
    let sub = |a: P3, b: P3| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let dot = |a: P3, b: P3| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let at = |p: P3, d: P3, t: f64| [p[0] + d[0] * t, p[1] + d[1] * t, p[2] + d[2] * t];
    let (d1, d2, r) = (sub(q1, p1), sub(q2, p2), sub(p1, p2));
    let (a, e, f) = (dot(d1, d1), dot(d2, d2), dot(d2, r));
    let eps = 1e-18;
    let (s, t) = if a <= eps && e <= eps {
        (0.0, 0.0)
    } else if a <= eps {
        (0.0, (f / e).clamp(0.0, 1.0))
    } else {
        let c = dot(d1, r);
        if e <= eps {
            ((-c / a).clamp(0.0, 1.0), 0.0)
        } else {
            let b = dot(d1, d2);
            let denom = a * e - b * b;
            let mut s = if denom > eps {
                ((b * f - c * e) / denom).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let mut t = (b * s + f) / e;
            if t < 0.0 {
                t = 0.0;
                s = (-c / a).clamp(0.0, 1.0);
            } else if t > 1.0 {
                t = 1.0;
                s = ((b - c) / a).clamp(0.0, 1.0);
            }
            (s, t)
        }
    };
    let (c1, c2) = (at(p1, d1, s), at(p2, d2, t));
    let d = dot(sub(c1, c2), sub(c1, c2)).sqrt();
    (d, c1, c2, s, t)
}

/// The exact closest approach of two conductors (each its subconductors'
/// polylines), closer than `cutoff`: segment against segment, the pieces'
/// boxes nearest first, those farther than the closest found left out.
fn approach(first: &[Poly], second: &[Poly], cutoff: f64) -> Option<Approach> {
    let mut best: Option<Approach> = None;
    let limit = |best: &Option<Approach>| best.map_or(cutoff, |b| b.d.min(cutoff));
    for (fa, a) in first.iter().enumerate() {
        for (fb, b) in second.iter().enumerate() {
            let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
            for (u, ba) in a.boxes.iter().enumerate() {
                for (v, bb) in b.boxes.iter().enumerate() {
                    let g = box_gap(*ba, *bb);
                    if g < limit(&best) {
                        pairs.push((g, u, v));
                    }
                }
            }
            pairs.sort_by(|x, y| x.0.total_cmp(&y.0));
            for (g, u, v) in pairs {
                if g >= limit(&best) {
                    break;
                }
                let ra = u * CHUNK..((u + 1) * CHUNK).min(a.points.len() - 1);
                let rb = v * CHUNK..((v + 1) * CHUNK).min(b.points.len() - 1);
                for k in ra {
                    for m in rb.clone() {
                        let (d, c1, c2, s, t) =
                            segments(a.points[k], a.points[k + 1], b.points[m], b.points[m + 1]);
                        if d < limit(&best) {
                            let along = |p: &Poly, k: usize, s: f64| {
                                p.run[k] + (p.run[k + 1] - p.run[k]) * s
                            };
                            best = Some(Approach {
                                d,
                                at: [c1, c2],
                                along: [along(a, k, s), along(b, m, t)],
                                sub: [fa, fb],
                            });
                        }
                    }
                }
            }
        }
    }
    best
}

/// Checks the pairs of conductors under unequal wind: `levels` are the
/// line solved at the pressures of [`levels`] (still air once, or from
/// each side), each with the same conductors in the same order. Paired
/// as [`crate::phase_distance::check`] pairs them (in still air; nearby
/// pairs where they come within its distance at any level). What is
/// required comes from an [`Evaluator`] (one call per pair, one station:
/// the distance, β, no sag, no string), made by `make` for each core.
/// Exit clause: the budget's time limit and cancellation (checked per
/// pair).
pub fn check<E: Evaluator>(
    settings: &UnequalWindSettings,
    levels: &[WindLevel],
    budget: &Budget,
    make: impl Fn() -> Result<E, String> + Sync,
) -> Result<Vec<UnequalWindPair>, String> {
    let run = budget.start();
    let Some(still) = levels
        .iter()
        .min_by(|a, b| a.pressure.total_cmp(&b.pressure))
    else {
        return Ok(Vec::new());
    };
    let n = still.conductors.len();
    for l in levels {
        let same = l.conductors.len() == n
            && l.conductors
                .iter()
                .zip(&still.conductors)
                .all(|(a, b)| a.wire.section == b.wire.section && a.wire.span == b.wire.span);
        if !same {
            return Err(format!(
                "the line at {} Pa from the {} has other conductors than in still air",
                l.pressure,
                l.side.as_str()
            ));
        }
    }
    // Per level and conductor its subconductors' polylines.
    let subs: Vec<Vec<Vec<Vec<P3>>>> = levels
        .iter()
        .map(|l| {
            l.conductors
                .iter()
                .map(|c| subconductors(&c.wire))
                .collect()
        })
        .collect();
    let polys: Vec<Vec<Vec<Poly>>> = subs
        .iter()
        .map(|l| {
            l.iter()
                .map(|s| {
                    s.iter()
                        .filter(|p| p.len() >= 2)
                        .map(|p| Poly::new(p))
                        .collect()
                })
                .collect()
        })
        .collect();
    // Each conductor's box at each level.
    let union = |a: (P3, P3), b: (P3, P3)| {
        (
            [0, 1, 2].map(|k| a.0[k].min(b.0[k])),
            [0, 1, 2].map(|k| a.1[k].max(b.1[k])),
        )
    };
    let level_bounds: Vec<Vec<Option<(P3, P3)>>> = polys
        .iter()
        .map(|l| {
            l.iter()
                .map(|ps| ps.iter().map(Poly::bounds).reduce(union))
                .collect()
        })
        .collect();
    // Each conductor's box over all levels.
    let bounds: Vec<Option<(P3, P3)>> = (0..n)
        .map(|c| {
            polys
                .iter()
                .flat_map(|l| l[c].iter().map(Poly::bounds))
                .reduce(|(a, b), (c, d)| {
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
    let filled: Vec<bool> = (0..n)
        .map(|c| polys.iter().all(|l| !l[c].is_empty()))
        .collect();
    let candidates = candidate_pairs(&settings.pairs, &still.conductors, &filled, apart);
    // The combinations: (level of the first, level of the second), on the
    // same side (still air counts for both), the one at p, the other
    // from low_share·p to p.
    let still_level = |l: &WindLevel| l.pressure <= 0.0;
    let mut combos: Vec<(usize, usize)> = Vec::new();
    for (x, lx) in levels.iter().enumerate() {
        for (y, ly) in levels.iter().enumerate() {
            let sided = still_level(lx) || still_level(ly) || lx.side == ly.side;
            let (hi, lo) = if lx.pressure >= ly.pressure {
                (lx.pressure, ly.pressure)
            } else {
                (ly.pressure, lx.pressure)
            };
            if sided && lo >= settings.low_share * hi - 1e-3 {
                combos.push((x, y));
            }
        }
    }
    let measure = |evaluator: &mut E,
                   i: usize,
                   j: usize,
                   same_span: bool|
     -> Result<Option<UnequalWindPair>, String> {
        let (a, b) = (&still.conductors[i], &still.conductors[j]);
        let (same_system, kv) = kind_of(a, b);
        let altitude = mean_height(a, b);
        // The combinations nearest first by the wires' boxes; those
        // farther than the closest found need no measuring.
        let cutoff = if same_span {
            f64::INFINITY
        } else {
            settings.pairs.max_distance
        };
        let mut order: Vec<(f64, usize)> = combos
            .iter()
            .enumerate()
            .filter_map(|(k, &(x, y))| Some((box_gap(level_bounds[x][i]?, level_bounds[y][j]?), k)))
            .collect();
        order.sort_by(|p, q| p.0.total_cmp(&q.0));
        let mut best: Option<(Approach, usize)> = None;
        for (gap, k) in order {
            let limit = best.map_or(cutoff, |b| b.0.d);
            if gap >= limit {
                break;
            }
            let (x, y) = combos[k];
            if let Some(r) = approach(&polys[x][i], &polys[y][j], limit) {
                best = Some((r, k));
            }
        }
        let Some((Approach { d, at, along, sub }, k)) = best else {
            return Ok(None);
        };
        let (x, y) = combos[k];
        let axes = d - a.wire.polyline_error - b.wire.polyline_error;
        let dz = (at[0][2] - at[1][2]).abs();
        let beta = if d > 0.0 {
            (dz / d).clamp(0.0, 1.0).acos().to_degrees()
        } else {
            0.0
        };
        let pair = PairGeometry {
            a,
            b,
            same_system,
            same_span,
            altitude,
            spacer_sag: None,
        };
        let mut row = [0.0; STATION_VALUES];
        row[0] = axes;
        row[1] = beta;
        let req = evaluator.evaluate(&pair, &row)?;
        let per = req.per_station;
        if per == 0 || req.values.len() != per {
            return Err(format!(
                "the check gave {} numbers for 1 station ({per} each)",
                req.values.len()
            ));
        }
        let side = if still_level(&levels[x]) {
            levels[y].side
        } else {
            levels[x].side
        };
        Ok(Some(UnequalWindPair {
            a: i,
            b: j,
            same_system,
            kv,
            same_span,
            altitude,
            rule: req.rule,
            at,
            along,
            distance: axes,
            required: req.values[0],
            pressure: [levels[x].pressure, levels[y].pressure],
            side,
            levels: [x, y],
            wires: [
                polys[x][i][sub[0]].points.to_vec(),
                polys[y][j][sub[1]].points.to_vec(),
            ],
            values: req.values[1..].to_vec(),
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
    let mut found: Vec<(usize, UnequalWindPair)> = std::thread::scope(|scope| {
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
                            stop(format!("unequal wind: {why}"));
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
            .flat_map(|w| w.join().expect("an unequal wind worker panicked"))
            .collect()
    });
    if let Some(stop) = stopped.into_inner().expect("not poisoned") {
        return Err(stop);
    }
    found.sort_by_key(|(k, _)| *k);
    Ok(found.into_iter().map(|(_, pair)| pair).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phase_distance::tests::{TestCode, conductor};

    #[test]
    fn levels_step_to_the_case_and_take_the_share() {
        let l = levels(100.0, 40.0, 0.6).unwrap();
        // 0, 40, 80, 100 and 60 % of each: 24, 48, 60.
        assert_eq!(l, vec![0.0, 24.0, 40.0, 48.0, 60.0, 80.0, 100.0]);
        assert!(levels(100.0, 0.0, 0.6).is_err());
        assert!(levels(f64::NAN, 10.0, 0.6).is_err());
        assert!(levels(100.0, 10.0, 1.5).is_err());
        assert_eq!(levels(0.0, 10.0, 0.6).unwrap(), vec![0.0]);
    }

    /// Two conductors 6 m apart side by side; the wind swings each across
    /// the line at mid-span (a parabola along the span) by 8 mm per Pa,
    /// the second (stiffer) by 4 mm per Pa.
    fn level(pressure: f64, side: WindSide) -> WindLevel {
        let mut a = conductor("C1", 1, 0.0, 30.0, 0.0, 10.0, [0.0, 0.0]);
        let mut b = conductor("C1", 2, 6.0, 30.0, 0.0, 10.0, [0.0, 0.0]);
        for (c, k) in [(&mut a, 0.008), (&mut b, 0.004)] {
            for p in &mut c.wire.points {
                let t = p[0] / 300.0;
                p[1] += k * pressure * side.to_left() * 4.0 * t * (1.0 - t);
            }
        }
        WindLevel {
            pressure,
            side,
            conductors: vec![a, b],
        }
    }

    fn solved(max: f64, step: f64) -> Vec<WindLevel> {
        let ps = levels(max, step, 0.6).unwrap();
        let mut out = Vec::new();
        for side in [WindSide::Right, WindSide::Left] {
            for &p in &ps {
                if p > 0.0 || side == WindSide::Right {
                    out.push(level(p, side));
                }
            }
        }
        out
    }

    /// The closest: from the right (the first blown towards the second),
    /// the first at the case's 500 Pa, the second at 60 % of it: at
    /// mid-span 6 + 0.004·300 − 0.008·500 = 3.2 m (from the left at
    /// best 6 − 0.004·500 + 0.008·300 = 6.4 m).
    #[test]
    fn the_closest_is_at_the_largest_difference() {
        let levels = solved(500.0, 100.0);
        let pairs = check(
            &UnequalWindSettings::default(),
            &levels,
            &Budget::new(),
            || Ok(TestCode),
        )
        .unwrap();
        assert_eq!(pairs.len(), 1);
        let p = &pairs[0];
        let expected = 3.2;
        assert!((p.distance - expected).abs() < 1e-6, "{p:?}");
        assert!((p.along[0] - 150.0).abs() < 1.0, "{:?}", p.along);
        // The first (y = 0) blown to the left (+y, towards the second) at
        // 500 Pa, the second at 300 Pa.
        assert_eq!(p.side, WindSide::Right);
        assert_eq!(p.pressure, [500.0, 300.0]);
        // Required from the evaluator's one station: no sag, no string.
        assert!((p.required - 2.64).abs() < 1e-9, "{}", p.required);
        assert!(p.margin() > 0.0);
    }

    /// A step that does not reach the case: the case's wind still taken.
    #[test]
    fn the_case_wind_is_reached() {
        let levels = solved(450.0, 100.0);
        let p = &check(
            &UnequalWindSettings::default(),
            &levels,
            &Budget::new(),
            || Ok(TestCode),
        )
        .unwrap()[0];
        assert_eq!(p.pressure, [450.0, 270.0]);
    }

    /// Bundles of two (0.4 m apart across): the closest approach between
    /// the nearest subconductors, exact (3.2 m less the two half widths at
    /// mid-span), its points on the wires handed back to draw (owner,
    /// 2026-10-24: the wires drawn went beside the line measured).
    #[test]
    fn the_points_are_on_the_wires_drawn() {
        let mut levels = solved(500.0, 100.0);
        for l in &mut levels {
            for c in &mut l.conductors {
                c.wire.bundle = vec![[-0.2, 0.0], [0.2, 0.0]];
            }
        }
        let p = &check(
            &UnequalWindSettings::default(),
            &levels,
            &Budget::new(),
            || Ok(TestCode),
        )
        .unwrap()[0];
        assert!((p.distance - (3.2 - 0.4)).abs() < 1e-6, "{}", p.distance);
        for k in 0..2 {
            let on = p.wires[k]
                .windows(2)
                .any(|w| segments(w[0], w[1], p.at[k], p.at[k]).0 < 1e-9);
            assert!(on, "point {k} {:?} not on its wire", p.at[k]);
        }
    }

    #[test]
    fn other_conductors_at_a_level_are_refused() {
        let mut levels = solved(100.0, 50.0);
        levels[1].conductors.pop();
        assert!(
            check(
                &UnequalWindSettings::default(),
                &levels,
                &Budget::new(),
                || { Ok(TestCode) }
            )
            .is_err()
        );
    }
}
