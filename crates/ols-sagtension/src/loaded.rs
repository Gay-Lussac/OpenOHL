// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Spans with additional vertical loads: segmented catenary.
//!
//! Point loads split the span into segments; distributed loads change the
//! load per unit cable length \\( w_k \\) of the segments they cover. The
//! horizontal tension \\( H \\) is the same everywhere. Writing the slope as
//! \\( y' = \sinh\psi \\), within a segment \\( \psi \\) grows linearly:
//!
//! \\[ \psi(x) = \psi_s + \frac{x - x_s}{C_k}, \quad C_k = \frac{H}{w_k}, \quad
//!     \Delta y = C_k(\cosh\psi_e - \cosh\psi_s), \quad
//!     \Delta s = C_k(\sinh\psi_e - \sinh\psi_s), \\]
//!
//! and a downward point load \\( P \\) makes the slope jump:
//! \\( \sinh\psi^+ = \sinh\psi^- + P/H \\). Tension is \\( T = H\cosh\psi \\).
//!
//! For a given \\( H \\) the only unknown is the slope at the left support,
//! fixed by reaching the right support (the end elevation increases strictly
//! with it) — solved by bisection with exit clauses. The change of state then
//! iterates on \\( H \\) with the same strain model as
//! [`change_of_state`](crate::change_of_state): two nested solvers, both
//! with exit clauses.

use std::time::Duration;

use ols_core::budget::{Budget, Stop};
use ols_core::units::{Force, ForcePerLength, Length, Temperature};
use ols_wireloads::VerticalLoads;

use crate::catenary::{Catenary, SagTensionError, SpanGeometry};
use crate::change_of_state::ElasticCable;

/// Above this |ψ| the hyperbolic functions overflow or lose meaning.
const MAX_PSI: f64 = 300.0;

/// A span with its base load (cable, ice, …) and additional vertical loads.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedSpan {
    /// The span.
    pub geometry: SpanGeometry,
    /// Load per unit cable length on the whole span (weight, ice, …).
    pub base_load: ForcePerLength,
    /// Additional vertical loads (positions from the left support).
    pub loads: VerticalLoads,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Segment {
    x0: f64,
    x1: f64,
    /// Load per unit cable length.
    w: f64,
    /// Point load at `x0` (0 for the first segment).
    jump: f64,
}

/// One solved segment.
#[derive(Debug, Clone, Copy, PartialEq)]
struct SolvedSegment {
    seg: Segment,
    c: f64,
    psi_s: f64,
    psi_e: f64,
    /// Elevation at `x0`.
    y0: f64,
}

impl SolvedSegment {
    fn psi(&self, x: f64) -> f64 {
        self.psi_s + (x - self.seg.x0) / self.c
    }

    fn dy(&self, psi_a: f64, psi_b: f64) -> f64 {
        2.0 * self.c * ((psi_a + psi_b) / 2.0).sinh() * ((psi_b - psi_a) / 2.0).sinh()
    }

    fn elevation(&self, x: f64) -> f64 {
        self.y0 + self.dy(self.psi_s, self.psi(x))
    }

    fn arc(&self) -> f64 {
        2.0 * self.c
            * ((self.psi_e + self.psi_s) / 2.0).cosh()
            * ((self.psi_e - self.psi_s) / 2.0).sinh()
    }

    /// \\( \int T\,ds \\) over the segment, for horizontal tension `h`.
    fn tension_integral(&self, h: f64) -> f64 {
        let d = self.psi_e - self.psi_s;
        h * self.c * (d / 2.0 + d.sinh() * (self.psi_e + self.psi_s).cosh() / 2.0)
    }
}

impl LoadedSpan {
    /// A span without additional loads.
    pub fn unloaded(geometry: SpanGeometry, base_load: ForcePerLength) -> Self {
        Self {
            geometry,
            base_load,
            loads: VerticalLoads::default(),
        }
    }

    fn segments(&self) -> Result<Vec<Segment>, SagTensionError> {
        let span = self.geometry.span.si();
        if !(span.is_finite() && span > 0.0) {
            return Err(SagTensionError::InvalidInput(format!(
                "span must be positive, got {span}"
            )));
        }
        // Breakpoints: supports, interior point loads, distributed load ends.
        let mut breaks = vec![0.0, span];
        breaks.extend(self.loads.point.iter().map(|(x, _)| x.si()));
        for (a, b, _) in &self.loads.distributed {
            breaks.extend([a.si(), b.si()]);
        }
        breaks.retain(|x| (0.0..=span).contains(x));
        breaks.sort_by(f64::total_cmp);
        breaks.dedup_by(|a, b| (*a - *b).abs() <= 1e-9 * span);

        let mut segments = Vec::with_capacity(breaks.len() - 1);
        for pair in breaks.windows(2) {
            let (x0, x1) = (pair[0], pair[1]);
            let mid = 0.5 * (x0 + x1);
            let w = self.base_load.si()
                + self
                    .loads
                    .distributed
                    .iter()
                    .filter(|(a, b, _)| a.si() <= mid && mid <= b.si())
                    .map(|(_, _, w)| w.si())
                    .sum::<f64>();
            if !(w.is_finite() && w > 0.0) {
                return Err(SagTensionError::InvalidInput(format!(
                    "net load per length must be positive between x = {x0:.3} m and {x1:.3} m, got {w} N/m"
                )));
            }
            // Point loads on the supports go straight into the support.
            let jump = if x0 > 0.0 {
                self.loads
                    .point
                    .iter()
                    .filter(|(x, _)| (x.si() - x0).abs() <= 1e-9 * span)
                    .map(|(_, p)| p.si())
                    .sum()
            } else {
                0.0
            };
            segments.push(Segment { x0, x1, w, jump });
        }
        Ok(segments)
    }

    /// Integrates the shape from the left support with angle `psi0`.
    fn shoot(segments: &[Segment], h: f64, psi0: f64) -> Option<Vec<SolvedSegment>> {
        let mut out = Vec::with_capacity(segments.len());
        let (mut psi, mut y) = (psi0, 0.0);
        for &seg in segments {
            let psi_s = if out.is_empty() {
                psi0
            } else {
                (psi.sinh() + seg.jump / h).asinh()
            };
            let c = h / seg.w;
            let psi_e = psi_s + (seg.x1 - seg.x0) / c;
            if !(psi_s.abs() <= MAX_PSI && psi_e.abs() <= MAX_PSI) {
                return None;
            }
            let solved = SolvedSegment {
                seg,
                c,
                psi_s,
                psi_e,
                y0: y,
            };
            y += solved.dy(psi_s, psi_e);
            psi = psi_e;
            out.push(solved);
        }
        Some(out)
    }

    /// Shape of the cable for horizontal tension `horizontal_tension`.
    ///
    /// Expected cost: ~60 bisection steps × segments. Stops with
    /// [`SagTensionError::NotConverged`] when the budget runs out.
    pub fn shape(
        &self,
        horizontal_tension: Force,
        budget: &Budget,
    ) -> Result<LoadedSolution, SagTensionError> {
        let h = horizontal_tension.si();
        if !(h.is_finite() && h > 0.0) {
            return Err(SagTensionError::InvalidInput(format!(
                "horizontal tension must be positive, got {h} N"
            )));
        }
        let segments = self.segments()?;
        let rise = self.geometry.rise.si();
        // End elevation minus target; None (overflow) counts as "too far" in
        // the direction of the overflowing angle.
        let end_error = |psi0: f64| -> f64 {
            match Self::shoot(&segments, h, psi0) {
                Some(s) => {
                    let last = s.last().expect("at least one segment");
                    last.y0 + last.dy(last.psi_s, last.psi_e) - rise
                }
                None => psi0.signum() * f64::INFINITY,
            }
        };

        let mut run = budget.start();
        let not_converged = |stop: Stop, lo: f64, hi: f64| SagTensionError::NotConverged {
            problem: "loaded span shape",
            stop,
            detail: format!("start angle bracket [{lo:.6e}, {hi:.6e}] at H = {h:.6e} N"),
        };
        // Start from the unloaded catenary's angle at the left support.
        let guess = Catenary::new(horizontal_tension, self.base_load)
            .and_then(|c| c.solve(self.geometry))
            .map_or(0.0, |s| s.slope(Length::ZERO).asinh());
        let (mut lo, mut hi) = (guess - 0.1, guess + 0.1);
        while end_error(lo) > 0.0 {
            run.step().map_err(|stop| not_converged(stop, lo, hi))?;
            lo -= 2.0 * (hi - lo);
            if lo < -MAX_PSI {
                return Err(SagTensionError::TooSlack {
                    ratio: f64::INFINITY,
                });
            }
        }
        while end_error(hi) < 0.0 {
            run.step().map_err(|stop| not_converged(stop, lo, hi))?;
            hi += 2.0 * (hi - lo);
            if hi > MAX_PSI {
                return Err(SagTensionError::TooSlack {
                    ratio: f64::INFINITY,
                });
            }
        }
        while hi - lo > 1e-14 * (1.0 + lo.abs().max(hi.abs())) {
            run.step().map_err(|stop| not_converged(stop, lo, hi))?;
            let mid = 0.5 * (lo + hi);
            if end_error(mid) > 0.0 {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        let segments =
            Self::shoot(&segments, h, 0.5 * (lo + hi)).ok_or(SagTensionError::TooSlack {
                ratio: f64::INFINITY,
            })?;
        Ok(LoadedSolution {
            geometry: self.geometry,
            horizontal_tension,
            segments,
        })
    }

    /// Unstressed length of the cable at its own temperature when strung at
    /// horizontal tension `horizontal_tension` under these loads.
    pub fn unstrained_length(
        &self,
        horizontal_tension: Force,
        cable: ElasticCable,
        budget: &Budget,
    ) -> Result<Length, SagTensionError> {
        let solution = self.shape(horizontal_tension, budget)?;
        let length = solution.length().si();
        let mean_strain = solution.tension_integral() / length / cable.axial_stiffness.si();
        Ok(Length::from_si(length / (1.0 + mean_strain)))
    }

    /// Change of state: solves this loaded span for a cable of known
    /// unstressed length (at `reference_temperature`) at `temperature`.
    ///
    /// Two nested solvers: `outer` bounds the iteration on \\( H \\) and
    /// `inner` each shape solve. Expected cost ~1 ms.
    pub fn solve_state(
        &self,
        cable: ElasticCable,
        unstressed_length: Length,
        reference_temperature: Temperature,
        temperature: Temperature,
        outer: &Budget,
        inner: &Budget,
    ) -> Result<LoadedSolution, SagTensionError> {
        let target = unstressed_length.si()
            * (cable.thermal_expansion * (temperature - reference_temperature).kelvin()).exp();
        let too_low = |h: f64| -> Result<bool, SagTensionError> {
            match self.unstrained_length(Force::from_si(h), cable, inner) {
                Ok(length) => Ok(length.si() > target),
                Err(SagTensionError::TooSlack { .. }) => Ok(true),
                Err(other) => Err(other),
            }
        };
        let mut run = outer.start();
        let not_converged = |stop: Stop, lo: f64, hi: f64| SagTensionError::NotConverged {
            problem: "loaded span change of state",
            stop,
            detail: format!(
                "tension bracket [{lo:.6e}, {hi:.6e}] N at {:.1} °C",
                temperature.celsius()
            ),
        };
        let start = (cable.axial_stiffness.si() * 1e-3).max(1.0);
        let (mut lo, mut hi) = (start, start);
        while !too_low(lo)? {
            run.step().map_err(|stop| not_converged(stop, lo, hi))?;
            lo /= 2.0;
        }
        while too_low(hi)? {
            run.step().map_err(|stop| not_converged(stop, lo, hi))?;
            hi *= 2.0;
        }
        while hi - lo > 1e-12 * hi {
            run.step().map_err(|stop| not_converged(stop, lo, hi))?;
            let mid = 0.5 * (lo + hi);
            if too_low(mid)? {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        self.shape(Force::from_si(0.5 * (lo + hi)), inner)
    }
}

/// Default exit clauses for the outer change-of-state iteration on loaded
/// spans: ~60 steps expected, each a full shape solve (~15 µs).
pub fn default_loaded_budget() -> Budget {
    Budget::new()
        .max_iterations(200)
        .time_limit(Duration::from_secs(60))
}

/// Shape of a loaded span.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedSolution {
    /// The span.
    pub geometry: SpanGeometry,
    /// Horizontal tension (same in all segments).
    pub horizontal_tension: Force,
    segments: Vec<SolvedSegment>,
}

impl LoadedSolution {
    fn segment_at(&self, x: f64) -> &SolvedSegment {
        self.segments
            .iter()
            .find(|s| x <= s.seg.x1)
            .unwrap_or_else(|| self.segments.last().expect("at least one segment"))
    }

    /// Elevation at horizontal position `x` (relative to the left support).
    pub fn elevation(&self, x: Length) -> Length {
        Length::from_si(self.segment_at(x.si()).elevation(x.si()))
    }

    /// Tension at horizontal position `x` (just left of a point load).
    pub fn tension(&self, x: Length) -> Force {
        self.horizontal_tension * self.segment_at(x.si()).psi(x.si()).cosh()
    }

    /// Tension at the left support.
    pub fn tension_left(&self) -> Force {
        self.horizontal_tension * self.segments[0].psi_s.cosh()
    }

    /// Tension at the right support.
    pub fn tension_right(&self) -> Force {
        self.horizontal_tension
            * self
                .segments
                .last()
                .expect("at least one segment")
                .psi_e
                .cosh()
    }

    /// Cable length.
    pub fn length(&self) -> Length {
        Length::from_si(self.segments.iter().map(SolvedSegment::arc).sum())
    }

    /// \\( \int_0^L T\,ds \\), N·m.
    pub fn tension_integral(&self) -> f64 {
        let h = self.horizontal_tension.si();
        self.segments.iter().map(|s| s.tension_integral(h)).sum()
    }

    /// Total vertical load carried: cable loads plus interior point loads.
    pub fn total_vertical_load(&self) -> Force {
        Force::from_si(
            self.segments
                .iter()
                .map(|s| s.seg.w * s.arc() + s.seg.jump)
                .sum(),
        )
    }

    /// Vertical reactions (upward) at the left and right supports.
    pub fn vertical_reactions(&self) -> (Force, Force) {
        let h = self.horizontal_tension;
        let last = self.segments.last().expect("at least one segment");
        (h * -self.segments[0].psi_s.sinh(), h * last.psi_e.sinh())
    }

    /// Position and value of the maximum sag (vertical distance from the
    /// chord). The maximum is either where the slope equals the chord slope
    /// inside a segment, or at a segment boundary (under a point load).
    pub fn max_sag(&self) -> (Length, Length) {
        let span = self.geometry.span.si();
        let chord_slope = self.geometry.rise.si() / span;
        let psi_chord = chord_slope.asinh();
        let sag_at = |s: &SolvedSegment, x: f64| chord_slope * x - s.elevation(x);
        let mut best = (0.0, f64::NEG_INFINITY);
        for s in &self.segments {
            let mut candidates = vec![s.seg.x0, s.seg.x1];
            if (s.psi_s..=s.psi_e).contains(&psi_chord) {
                candidates.push(s.seg.x0 + (psi_chord - s.psi_s) * s.c);
            }
            for x in candidates {
                let d = sag_at(s, x);
                if d > best.1 {
                    best = (x, d);
                }
            }
        }
        (Length::from_si(best.0), Length::from_si(best.1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catenary::default_solver_budget;
    use crate::change_of_state::{InstalledSpan, LoadState};

    fn close(actual: f64, expected: f64, rel_tol: f64) {
        assert!(
            ((actual - expected) / expected).abs() <= rel_tol,
            "actual {actual}, expected {expected}"
        );
    }

    fn m(v: f64) -> Length {
        Length::from_meters(v)
    }

    fn w(v: f64) -> ForcePerLength {
        ForcePerLength::from_newtons_per_meter(v)
    }

    fn with_point(geometry: SpanGeometry, x: f64, p: f64) -> LoadedSpan {
        LoadedSpan {
            geometry,
            base_load: w(15.97),
            loads: VerticalLoads {
                point: vec![(m(x), Force::from_newtons(p))],
                distributed: vec![],
            },
        }
    }

    #[test]
    fn no_loads_equals_plain_catenary() {
        let geometry = SpanGeometry::inclined(m(450.0), m(35.0));
        let h = Force::from_kilonewtons(22.0);
        let loaded = LoadedSpan::unloaded(geometry, w(15.97))
            .shape(h, &default_solver_budget())
            .unwrap();
        let plain = Catenary::new(h, w(15.97)).unwrap().solve(geometry).unwrap();
        close(loaded.length().meters(), plain.length().meters(), 1e-12);
        close(loaded.max_sag().1.meters(), plain.sag().meters(), 1e-9);
        close(
            loaded.tension_left().newtons(),
            plain.tension_left().newtons(),
            1e-10,
        );
        close(
            loaded.tension_right().newtons(),
            plain.tension_right().newtons(),
            1e-10,
        );
    }

    #[test]
    fn midspan_point_load_matches_closed_form() {
        // Symmetric: just left of mid-span sinh ψ⁻ = −P/2H; at the support
        // ψ_A = ψ⁻ − (S/2)/C; mid-span sag = C (cosh ψ_A − cosh ψ⁻).
        let (s, h, p, wv) = (300.0, 28_000.0, 2_000.0, 15.97);
        let solution = with_point(SpanGeometry::level(m(s)), s / 2.0, p)
            .shape(Force::from_newtons(h), &default_solver_budget())
            .unwrap();
        let c = h / wv;
        let psi_minus = (-p / (2.0 * h)).asinh();
        let psi_a = psi_minus - (s / 2.0) / c;
        let sag = c * (psi_a.cosh() - psi_minus.cosh());
        let (x, d) = solution.max_sag();
        close(x.meters(), s / 2.0, 1e-12);
        close(d.meters(), sag, 1e-10);
        close(solution.tension_left().newtons(), h * psi_a.cosh(), 1e-10);
    }

    #[test]
    fn vertical_equilibrium() {
        // Upward support reactions carry the cable load plus the point loads.
        let mut span = with_point(SpanGeometry::inclined(m(400.0), m(-30.0)), 120.0, 900.0);
        span.loads
            .point
            .push((m(310.0), Force::from_newtons(400.0)));
        span.loads.distributed.push((m(50.0), m(200.0), w(12.0)));
        let solution = span
            .shape(Force::from_kilonewtons(25.0), &default_solver_budget())
            .unwrap();
        let (va, vb) = solution.vertical_reactions();
        close(
            (va + vb).newtons(),
            solution.total_vertical_load().newtons(),
            1e-10,
        );
        assert!((solution.elevation(m(400.0)).meters() + 30.0).abs() < 1e-8);
    }

    #[test]
    fn full_span_distributed_load_adds_to_weight() {
        let geometry = SpanGeometry::inclined(m(300.0), m(10.0));
        let span = LoadedSpan {
            geometry,
            base_load: w(15.97),
            loads: VerticalLoads {
                point: vec![],
                distributed: vec![(m(0.0), m(300.0), w(10.0))],
            },
        };
        let h = Force::from_kilonewtons(30.0);
        let loaded = span.shape(h, &default_solver_budget()).unwrap();
        let plain = Catenary::new(h, w(25.97)).unwrap().solve(geometry).unwrap();
        close(loaded.max_sag().1.meters(), plain.sag().meters(), 1e-9);
    }

    #[test]
    fn change_of_state_without_loads_matches_installed_span() {
        let geometry = SpanGeometry::level(m(300.0));
        let cable = ElasticCable {
            axial_stiffness: Force::from_si(34.64e6),
            thermal_expansion: 18.9e-6,
        };
        let known = LoadState {
            load: w(15.97),
            temperature: Temperature::from_celsius(15.0),
        };
        let h0 = Force::from_kilonewtons(28.0);
        let span = LoadedSpan::unloaded(geometry, w(15.97));
        let inner = default_solver_budget();
        let l0 = span.unstrained_length(h0, cable, &inner).unwrap();
        let hot = span
            .solve_state(
                cable,
                l0,
                known.temperature,
                Temperature::from_celsius(75.0),
                &default_loaded_budget(),
                &inner,
            )
            .unwrap();
        let reference = InstalledSpan::from_known_state(geometry, cable, known, h0)
            .unwrap()
            .solve(
                LoadState {
                    load: w(15.97),
                    temperature: Temperature::from_celsius(75.0),
                },
                &inner,
            )
            .unwrap();
        close(
            hot.horizontal_tension.newtons(),
            reference.catenary.horizontal_tension.newtons(),
            1e-9,
        );
    }

    #[test]
    fn added_marker_increases_tension_and_sag() {
        let geometry = SpanGeometry::level(m(300.0));
        let cable = ElasticCable {
            axial_stiffness: Force::from_si(34.64e6),
            thermal_expansion: 18.9e-6,
        };
        let inner = default_solver_budget();
        let bare = LoadedSpan::unloaded(geometry, w(15.97));
        let t15 = Temperature::from_celsius(15.0);
        let l0 = bare
            .unstrained_length(Force::from_kilonewtons(28.0), cable, &inner)
            .unwrap();
        let with_marker = with_point(geometry, 150.0, 70.0)
            .solve_state(cable, l0, t15, t15, &default_loaded_budget(), &inner)
            .unwrap();
        assert!(with_marker.horizontal_tension > Force::from_kilonewtons(28.0));
        assert!(with_marker.max_sag().1 > m(6.4204));
    }

    #[test]
    fn rejects_negative_net_load_and_reports_exit() {
        let mut span = LoadedSpan::unloaded(SpanGeometry::level(m(300.0)), w(15.97));
        span.loads.distributed.push((m(0.0), m(100.0), w(-20.0)));
        assert!(matches!(
            span.shape(Force::from_kilonewtons(20.0), &default_solver_budget()),
            Err(SagTensionError::InvalidInput(_))
        ));
        let error = with_point(SpanGeometry::level(m(300.0)), 100.0, 500.0)
            .shape(
                Force::from_kilonewtons(20.0),
                &Budget::new().max_iterations(3),
            )
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("loaded span shape did not converge"),
            "{error}"
        );
    }
}
