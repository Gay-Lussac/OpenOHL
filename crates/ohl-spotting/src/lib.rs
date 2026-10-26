// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Optimum structure spotting (Phase 5): the structures along a route that
//! cost least, with every span keeping its ground clearance and every
//! structure within its allowable spans.
//!
//! A [`Problem`] is a ground profile along the route (station, elevation),
//! the points where a structure must stand (start, end, angle points),
//! zones where none may stand, and the **candidates**: structure types and
//! extensions, each with its price, the height of its lowest wire, whether
//! it is a dead-end, and its allowable spans by line angle
//! ([`ohl_structures::Capacity::spans_at`]).
//!
//! Structures may stand at **candidate stations**: every `step` metres from
//! the start, plus the required points. A span from station *i* to *j* is
//! feasible when it is no longer than `max_span` and the wire — a catenary
//! of the given horizontal tension and weight per metre between the lowest
//! wire points of the two structures (the design, maximum-sag condition)
//! — keeps `clearance` above the ground everywhere between them. Each
//! structure's wind span (half the sum of its spans) and weight span
//! (between the low points of its spans) must be within the allowable
//! spans of its type at its line angle.
//!
//! [`spot`] solves it by **dynamic programming** over the stations, the
//! classical method of optimum spotting (Mitra & Wolfenden, *A computer
//! technique for optimizing the sites and heights of transmission line
//! towers — a dynamic programming approach*, The Computer Journal 10(4),
//! 1968; see `docs/theory/spotting.md`). The state is (station *j*, candidate at
//! *j*, previous station *i*): the cheapest chain of structures from the
//! start ending with the span *i*→*j*. Keeping the previous station makes
//! the wind span of every structure exact. The weight span needs the low
//! point of the back span, which depends on the candidate at *i* too; the
//! state keeps the low point of its cheapest way in, so a chain that is
//! only feasible through a dearer structure at *i* can be missed (rare:
//! the extensions of one type shift the low point by little).
//!
//! **Cost**: with *N* stations, *K* candidates and *R* stations within
//! `max_span` of each other, there are *N·K·R* states, each reached from
//! *K·R* states: *O(N·K²·R²)* time, of which *N·K²·R* span checks (each
//! walks the ground points of the span), and *O(N·K·R)* memory. A 10 km
//! route at 10 m steps with spans up to 500 m and 6 candidates is about
//! 10⁸ steps: seconds in a release build. Every state is one step of the
//! [`Budget`]: the run stops at its iteration or time limit, or when
//! cancelled, and says why.

pub mod route;

use std::time::Duration;

use ohl_core::budget::{Budget, Stop};
use ohl_core::units::{Force, ForcePerLength, Length};
use ohl_sagtension::{Catenary, SpanGeometry};
use ohl_structures::{AllowedSpans, Capacity};

/// A structure type (with an extension) that may be placed.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    /// Structure type id.
    pub structure_type: String,
    /// Extension (none: the type as it is).
    pub extension: Option<String>,
    /// Height of the lowest wire point above the ground, m (the clamp of
    /// a suspension string, the attachment of a strain string).
    pub height: f64,
    /// What one costs, placed (supply, foundation, erection).
    pub cost: f64,
    /// A dead-end (every attachment a strain point): only dead-ends may
    /// stand at the start and the end.
    pub dead_end: bool,
    /// Allowable spans by line angle (none: no limits).
    pub capacity: Option<Capacity>,
}

/// What a candidate may carry at a line angle: `None` if it may not stand
/// there, `Some(None)` without limits.
fn allowed(c: &Candidate, angle_deg: f64) -> Option<Option<AllowedSpans>> {
    match &c.capacity {
        Some(cap) if !cap.spans.is_empty() => cap.spans_at(angle_deg).map(Some),
        // Without Method 1 a dead-end may turn any angle, others none.
        _ => (angle_deg.abs() < 1e-6 || c.dead_end).then_some(None),
    }
}

fn within(limits: Option<AllowedSpans>, wind: f64, weight: f64) -> bool {
    limits.is_none_or(|l| {
        wind <= l.wind_span + 1e-9
            && weight <= l.weight_span + 1e-9
            && l.min_weight_span.is_none_or(|m| weight >= m - 1e-9)
    })
}

/// A point where a structure must stand.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RequiredPoint {
    /// Station, m.
    pub station: f64,
    /// Line angle there, degrees.
    pub angle_deg: f64,
}

/// A spotting problem along one route.
#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    /// Ground profile: (station, elevation), m, stations rising; the
    /// ground is linear between the points. The route runs from the first
    /// to the last station.
    pub ground: Vec<[f64; 2]>,
    /// Points where a structure must stand besides the start and the end
    /// (angle points of the alignment).
    pub required: Vec<RequiredPoint>,
    /// Station ranges where no structure may stand, m.
    pub forbidden: Vec<[f64; 2]>,
    /// Candidates.
    pub candidates: Vec<Candidate>,
    /// Distance between candidate stations, m.
    pub step: f64,
    /// Longest span, m.
    pub max_span: f64,
    /// Shortest span, m.
    pub min_span: f64,
    /// Smallest vertical distance from the wire to the ground, m.
    pub clearance: f64,
    /// Horizontal tension of the wire in the design condition, N.
    pub horizontal_tension: f64,
    /// Weight of the wire per metre in that condition, N/m.
    pub weight: f64,
    /// Cost of each span (stringing labour).
    pub span_cost: f64,
    /// Cost per metre of wire length of a span (all its wires).
    pub wire_cost_per_m: f64,
}

/// A structure placed.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    /// Station, m.
    pub station: f64,
    /// Ground elevation there, m.
    pub ground: f64,
    /// Candidate (index in [`Problem::candidates`]).
    pub candidate: usize,
    /// Line angle, degrees.
    pub angle_deg: f64,
    /// Wind span, m.
    pub wind_span: f64,
    /// Weight span, m.
    pub weight_span: f64,
}

/// A span of the result.
#[derive(Debug, Clone, PartialEq)]
pub struct SpanResult {
    /// Horizontal length, m.
    pub length: f64,
    /// Length of the wire, m.
    pub wire_length: f64,
    /// Smallest vertical distance from the wire to the ground, m.
    pub clearance: f64,
    /// Station where it is, m.
    pub clearance_at: f64,
}

/// A spotting result.
#[derive(Debug, Clone, PartialEq)]
pub struct Spotting {
    /// Structures in station order.
    pub structures: Vec<Placed>,
    /// Spans between them.
    pub spans: Vec<SpanResult>,
    /// Cost of the structures.
    pub structure_cost: f64,
    /// Cost of the spans (labour).
    pub span_cost: f64,
    /// Cost of the wire.
    pub wire_cost: f64,
    /// Everything.
    pub total: f64,
    /// Candidate stations looked at.
    pub stations: usize,
    /// States of the dynamic programme.
    pub states: u64,
    /// Time taken.
    pub elapsed: Duration,
}

/// Why spotting found nothing.
#[derive(Debug, Clone, PartialEq)]
pub enum SpotError {
    /// The problem is not well posed.
    Invalid(String),
    /// No placement keeps every limit.
    Infeasible(String),
    /// An exit clause stopped it.
    Stopped(Stop),
}

impl std::fmt::Display for SpotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(m) => write!(f, "spotting: {m}"),
            Self::Infeasible(m) => write!(f, "spotting found no placement: {m}"),
            Self::Stopped(s) => write!(f, "spotting {s}"),
        }
    }
}

impl std::error::Error for SpotError {}

/// Exit clauses of a spotting run: 500 million states or 5 minutes.
pub fn default_budget() -> Budget {
    Budget::new()
        .max_iterations(500_000_000)
        .time_limit(Duration::from_secs(300))
}

/// One span checked.
#[derive(Debug, Clone, Copy)]
struct SpanCheck {
    /// Horizontal position of the catenary's low point from the back
    /// structure, m.
    low_x: f64,
    wire_length: f64,
    clearance: f64,
    clearance_at: f64,
}

/// A candidate station.
#[derive(Debug, Clone)]
struct Station {
    s: f64,
    z: f64,
    angle: f64,
    /// A structure must stand here.
    required: bool,
    /// Candidates allowed here and their limits.
    allowed: Vec<(usize, Option<AllowedSpans>)>,
}

impl Problem {
    fn check(&self) -> Result<(), SpotError> {
        let bad = |m: &str| Err(SpotError::Invalid(m.into()));
        let positive = |x: f64| x.is_finite() && x > 0.0;
        if self.ground.len() < 2 || self.ground.windows(2).any(|w| !positive(w[1][0] - w[0][0])) {
            return bad("the ground profile needs two points or more, stations rising");
        }
        if !positive(self.step) {
            return bad("the step must be above 0 m");
        }
        // A tiny step would make (and allocate) more stations than any
        // route needs: refused before the stations are made (code review
        // 2026-10-26).
        let length = self.ground[self.ground.len() - 1][0] - self.ground[0][0];
        let stations = length / self.step;
        if stations.is_nan() || stations > MAX_STATIONS as f64 {
            return Err(SpotError::Invalid(format!(
                "the step {} m gives {stations:.0} stations on the route's {length:.0} m; at \
                 most {MAX_STATIONS}: give a larger step",
                self.step
            )));
        }
        if !positive(self.max_span) || self.min_span < 0.0 || self.min_span >= self.max_span {
            return bad("the spans must be 0 ≤ min < max");
        }
        if !(positive(self.horizontal_tension) && positive(self.weight)) {
            return bad("tension and weight must be above 0");
        }
        if self.candidates.is_empty() {
            return bad("no candidate structures");
        }
        if !self.candidates.iter().any(|c| c.dead_end) {
            return bad("no dead-end among the candidates (needed at the start and the end)");
        }
        Ok(())
    }

    /// Ground elevation at a station (linear; the ends held beyond).
    pub fn ground_at(&self, s: f64) -> f64 {
        let g = &self.ground;
        let k = g.partition_point(|p| p[0] <= s);
        if k == 0 {
            return g[0][1];
        }
        if k >= g.len() {
            return g[g.len() - 1][1];
        }
        let (a, b) = (g[k - 1], g[k]);
        a[1] + (b[1] - a[1]) * (s - a[0]) / (b[0] - a[0])
    }

    /// The span from station `si` (wire at `zi`) to `sj` (wire at `zj`);
    /// `None` if the clearance falls below `stop_below` on the way (when
    /// given) or the catenary cannot be solved.
    fn span(
        &self,
        si: f64,
        zi: f64,
        sj: f64,
        zj: f64,
        stop_below: Option<f64>,
    ) -> Option<SpanCheck> {
        let length = sj - si;
        let sol = Catenary::new(
            Force::from_si(self.horizontal_tension),
            ForcePerLength::from_si(self.weight),
        )
        .and_then(|c| {
            c.solve(SpanGeometry::inclined(
                Length::from_si(length),
                Length::from_si(zj - zi),
            ))
        })
        .ok()?;
        let c = self.horizontal_tension / self.weight;
        let low_x = sol.low_point_x().si();
        let wire = |x: f64| zi + sol.elevation(Length::from_si(x)).si();
        let mut best = (f64::INFINITY, si);
        let g = &self.ground;
        let mut k = g.partition_point(|p| p[0] <= si).saturating_sub(1);
        while k + 1 < g.len() && g[k][0] < sj {
            let (a, b) = (g[k], g[k + 1]);
            let (x0, x1) = (a[0].max(si) - si, b[0].min(sj) - si);
            if x1 >= x0 {
                let m = (b[1] - a[1]) / (b[0] - a[0]);
                // Wire minus ground is convex on a linear piece of ground:
                // least where the wire's slope is the ground's.
                let x = (low_x + c * m.asinh()).clamp(x0, x1);
                let d = wire(x) - (a[1] + m * (x + si - a[0]));
                if d < best.0 {
                    best = (d, si + x);
                    if stop_below.is_some_and(|need| d < need) {
                        return None;
                    }
                }
            }
            k += 1;
        }
        Some(SpanCheck {
            low_x,
            wire_length: sol.length().si(),
            clearance: best.0,
            clearance_at: best.1,
        })
    }

    fn stations(&self) -> Result<Vec<Station>, SpotError> {
        let (start, end) = (self.ground[0][0], self.ground[self.ground.len() - 1][0]);
        let mut points: Vec<(f64, f64, bool)> = vec![(start, 0.0, true), (end, 0.0, true)];
        for r in &self.required {
            if !(r.station > start && r.station < end) {
                return Err(SpotError::Invalid(format!(
                    "required point at {:.1} m is not inside the route",
                    r.station
                )));
            }
            points.push((r.station, r.angle_deg, true));
        }
        let n = ((end - start) / self.step).floor() as usize;
        for k in 1..=n {
            points.push((start + k as f64 * self.step, 0.0, false));
        }
        // Required points first among equal stations, then one per station.
        points.sort_by(|a, b| a.0.total_cmp(&b.0).then(b.2.cmp(&a.2)));
        points.dedup_by(|b, a| (b.0 - a.0).abs() < 1e-6);
        let forbidden = |s: f64| self.forbidden.iter().any(|f| s > f[0] && s < f[1]);
        let last = points.len() - 1;
        let mut out = Vec::new();
        for (k, (s, angle, required)) in points.into_iter().enumerate() {
            if forbidden(s) {
                if required {
                    return Err(SpotError::Invalid(format!(
                        "a structure must stand at {s:.1} m, in a forbidden zone"
                    )));
                }
                continue;
            }
            let end_point = k == 0 || k == last;
            let allowed: Vec<_> = self
                .candidates
                .iter()
                .enumerate()
                .filter(|(_, c)| !end_point || c.dead_end)
                .filter_map(|(i, c)| allowed(c, angle).map(|l| (i, l)))
                .collect();
            if allowed.is_empty() && required {
                return Err(SpotError::Infeasible(format!(
                    "no candidate may stand at {s:.1} m (line angle {angle:.1}°)"
                )));
            }
            out.push(Station {
                s,
                z: self.ground_at(s),
                angle,
                required,
                allowed,
            });
        }
        Ok(out)
    }

    /// Checks a placement — (station, candidate) in station order, from
    /// the start to the end, angles from [`Problem::required`] — and gives
    /// its cost, or the first limit it breaks.
    pub fn evaluate(&self, placement: &[(f64, usize)]) -> Result<Spotting, String> {
        self.check().map_err(|e| e.to_string())?;
        let angle = |s: f64| {
            self.required
                .iter()
                .find(|r| (r.station - s).abs() < 1e-6)
                .map_or(0.0, |r| r.angle_deg)
        };
        let n = placement.len();
        let (start, end) = (self.ground[0][0], self.ground[self.ground.len() - 1][0]);
        if n < 2 || (placement[0].0 - start).abs() > 1e-6 || (placement[n - 1].0 - end).abs() > 1e-6
        {
            return Err("a placement runs from the start to the end".into());
        }
        for r in &self.required {
            if !placement.iter().any(|p| (p.0 - r.station).abs() < 1e-6) {
                return Err(format!(
                    "no structure at the required point {:.1} m",
                    r.station
                ));
            }
        }
        if let Some(p) = placement
            .iter()
            .find(|p| self.forbidden.iter().any(|f| p.0 > f[0] && p.0 < f[1]))
        {
            return Err(format!("{:.1} m is in a forbidden zone", p.0));
        }
        let mut checks = Vec::new();
        for w in placement.windows(2) {
            let ((si, ki), (sj, kj)) = (w[0], w[1]);
            let (ci, cj) = (&self.candidates[ki], &self.candidates[kj]);
            let length = sj - si;
            if length > self.max_span + 1e-9 || length < self.min_span - 1e-9 {
                return Err(format!(
                    "span {si:.1}–{sj:.1} m: {length:.1} m is out of range"
                ));
            }
            let span = self
                .span(
                    si,
                    self.ground_at(si) + ci.height,
                    sj,
                    self.ground_at(sj) + cj.height,
                    None,
                )
                .ok_or_else(|| format!("span {si:.1}–{sj:.1} m: no catenary"))?;
            if span.clearance < self.clearance - 1e-9 {
                return Err(format!(
                    "span {si:.1}–{sj:.1} m: clearance {:.2} m at {:.1} m",
                    span.clearance, span.clearance_at
                ));
            }
            checks.push((length, span));
        }
        for (k, &(s, c)) in placement.iter().enumerate() {
            let cand = &self.candidates[c];
            if (k == 0 || k == n - 1) && !cand.dead_end {
                return Err(format!("{s:.1} m: the ends need dead-ends"));
            }
            let limits = allowed(cand, angle(s)).ok_or_else(|| {
                format!("{s:.1} m: {} may not turn this angle", cand.structure_type)
            })?;
            let (wind, weight) = spans_at(&checks, k);
            if !within(limits, wind, weight) {
                return Err(format!(
                    "{s:.1} m: wind span {wind:.1} m or weight span {weight:.1} m over the limits of {}",
                    cand.structure_type
                ));
            }
        }
        Ok(self.assemble(placement, &checks, angle))
    }

    fn assemble(
        &self,
        placement: &[(f64, usize)],
        checks: &[(f64, SpanCheck)],
        angle: impl Fn(f64) -> f64,
    ) -> Spotting {
        let structures: Vec<Placed> = placement
            .iter()
            .enumerate()
            .map(|(k, &(s, c))| {
                let (wind_span, weight_span) = spans_at(checks, k);
                Placed {
                    station: s,
                    ground: self.ground_at(s),
                    candidate: c,
                    angle_deg: angle(s),
                    wind_span,
                    weight_span,
                }
            })
            .collect();
        let spans: Vec<SpanResult> = checks
            .iter()
            .map(|(length, c)| SpanResult {
                length: *length,
                wire_length: c.wire_length,
                clearance: c.clearance,
                clearance_at: c.clearance_at,
            })
            .collect();
        let structure_cost = placement
            .iter()
            .map(|&(_, c)| self.candidates[c].cost)
            .sum();
        let span_cost = self.span_cost * spans.len() as f64;
        let wire_cost = self.wire_cost_per_m * spans.iter().map(|s| s.wire_length).sum::<f64>();
        Spotting {
            structures,
            spans,
            structure_cost,
            span_cost,
            wire_cost,
            total: structure_cost + span_cost + wire_cost,
            stations: 0,
            states: 0,
            elapsed: Duration::ZERO,
        }
    }
}

/// Wind and weight span of structure `k` from its spans' (length, check).
fn spans_at(checks: &[(f64, SpanCheck)], k: usize) -> (f64, f64) {
    let back = k.checked_sub(1).map(|b| checks[b]);
    let ahead = checks.get(k);
    let wind = (back.map_or(0.0, |b| b.0) + ahead.map_or(0.0, |a| a.0)) / 2.0;
    let weight = back.map_or(0.0, |(l, c)| l - c.low_x) + ahead.map_or(0.0, |(_, c)| c.low_x);
    (wind, weight)
}

const NONE: u32 = u32::MAX;

/// The most stations a route is divided into by its step (a limit of the
/// computation, not of the design: 1 000 km at 1 m).
pub const MAX_STATIONS: usize = 1_000_000;

/// The least-cost placement of structures (see the crate documentation).
pub fn spot(problem: &Problem, budget: &Budget) -> Result<Spotting, SpotError> {
    problem.check()?;
    let mut run = budget.start();
    let st = problem.stations()?;
    let n = st.len();
    // Stations within reach behind each station, not beyond a required one.
    let mut first = 0;
    let preds: Vec<Vec<usize>> = (0..n)
        .map(|j| {
            let from = first;
            if st[j].required {
                first = j;
            }
            (from..j)
                .filter(|&i| {
                    let l = st[j].s - st[i].s;
                    l <= problem.max_span + 1e-9 && l >= problem.min_span - 1e-9
                })
                .collect()
        })
        .collect();
    // Per station, per (allowed candidate a, predecessor p): best cost,
    // where it came from (a, p at the predecessor), and the distance from
    // the back span's low point to this station.
    let mut value: Vec<Vec<f64>> = Vec::with_capacity(n);
    let mut back: Vec<Vec<(u32, u32)>> = Vec::with_capacity(n);
    let mut low_to_here: Vec<Vec<f64>> = Vec::with_capacity(n);
    let span_cost = |c: &SpanCheck| problem.span_cost + problem.wire_cost_per_m * c.wire_length;
    // The start: one state per allowed candidate.
    value.push(
        st[0]
            .allowed
            .iter()
            .map(|&(c, _)| problem.candidates[c].cost)
            .collect(),
    );
    back.push(vec![(NONE, NONE); st[0].allowed.len()]);
    low_to_here.push(vec![0.0; st[0].allowed.len()]);
    let mut states = 0u64;
    for j in 1..n {
        let pj = preds[j].len();
        let width = st[j].allowed.len() * pj;
        let mut v = vec![f64::INFINITY; width];
        let mut b = vec![(NONE, NONE); width];
        let mut lo = vec![0.0; width];
        for (a, &(cj, _)) in st[j].allowed.iter().enumerate() {
            let zj = st[j].z + problem.candidates[cj].height;
            for (p, &i) in preds[j].iter().enumerate() {
                run.step().map_err(SpotError::Stopped)?;
                states += 1;
                let length = st[j].s - st[i].s;
                let mut best = (f64::INFINITY, (NONE, NONE), 0.0);
                for (ai, &(ci, limits)) in st[i].allowed.iter().enumerate() {
                    let zi = st[i].z + problem.candidates[ci].height;
                    let Some(span) =
                        problem.span(st[i].s, zi, st[j].s, zj, Some(problem.clearance))
                    else {
                        continue;
                    };
                    let add = span_cost(&span);
                    if i == 0 {
                        let total = value[0][ai] + add;
                        if total < best.0 && within(limits, length / 2.0, span.low_x) {
                            best = (total, (ai as u32, NONE), length - span.low_x);
                        }
                        continue;
                    }
                    let pi_n = preds[i].len();
                    for (pi, &h) in preds[i].iter().enumerate() {
                        let prev = value[i][ai * pi_n + pi];
                        if !prev.is_finite() || prev + add >= best.0 {
                            continue;
                        }
                        let wind = (st[i].s - st[h].s + length) / 2.0;
                        let weight = low_to_here[i][ai * pi_n + pi] + span.low_x;
                        if within(limits, wind, weight) {
                            best = (prev + add, (ai as u32, pi as u32), length - span.low_x);
                        }
                    }
                }
                if best.0.is_finite() {
                    let k = a * pj + p;
                    v[k] = best.0 + problem.candidates[cj].cost;
                    b[k] = best.1;
                    lo[k] = best.2;
                }
            }
        }
        value.push(v);
        back.push(b);
        low_to_here.push(lo);
    }
    // The end: its own spans checked.
    let last = n - 1;
    let pl = preds[last].len();
    let mut end = (f64::INFINITY, 0, 0);
    for (a, &(_, limits)) in st[last].allowed.iter().enumerate() {
        for (p, &i) in preds[last].iter().enumerate() {
            let k = a * pl + p;
            let total = value[last][k];
            let length = st[last].s - st[i].s;
            if total < end.0 && within(limits, length / 2.0, low_to_here[last][k]) {
                end = (total, a, p);
            }
        }
    }
    if !end.0.is_finite() {
        return Err(SpotError::Infeasible(
            "every chain of spans breaks the clearance, the span limits or the structures' \
             allowable spans (try a smaller step, longer spans, taller extensions or more types)"
                .into(),
        ));
    }
    // Back from the end.
    let mut chain = vec![(last, end.1)];
    let (mut j, mut a, mut p) = (last, end.1, end.2 as u32);
    while j > 0 {
        let i = preds[j][p as usize];
        let (ai, pi) = back[j][a * preds[j].len() + p as usize];
        chain.push((i, ai as usize));
        (j, a, p) = (i, ai as usize, pi);
    }
    chain.reverse();
    let placement: Vec<(f64, usize)> = chain
        .iter()
        .map(|&(k, a)| (st[k].s, st[k].allowed[a].0))
        .collect();
    let checks: Vec<(f64, SpanCheck)> = placement
        .windows(2)
        .map(|w| {
            let (ci, cj) = (&problem.candidates[w[0].1], &problem.candidates[w[1].1]);
            let zi = problem.ground_at(w[0].0) + ci.height;
            let zj = problem.ground_at(w[1].0) + cj.height;
            let c = problem
                .span(w[0].0, zi, w[1].0, zj, None)
                .expect("checked while solving");
            (w[1].0 - w[0].0, c)
        })
        .collect();
    let angle = |s: f64| {
        st.iter()
            .find(|x| (x.s - s).abs() < 1e-9)
            .map_or(0.0, |x| x.angle)
    };
    let mut out = problem.assemble(&placement, &checks, angle);
    out.stations = n;
    out.states = states;
    out.elapsed = run.elapsed();
    Ok(out)
}

#[cfg(test)]
mod tests;
