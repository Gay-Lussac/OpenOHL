// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Catenary of a single span between two supports.
//!
//! Coordinates: the left support A is at \\( (0, 0) \\), the right support B
//! at \\( (S, h) \\), where \\( S \\) is the horizontal span length and
//! \\( h \\) the height of B above A (negative if B is lower). The cable has a
//! uniform weight \\( w \\) per unit of arc length and horizontal tension
//! \\( H \\); its catenary parameter is \\( C = H / w \\).
//!
//! With \\( a = S / 2C \\), the horizontal position of the lowest point is
//!
//! \\[ x_m = \frac{S}{2} - C \operatorname{asinh}\frac{h}{2C\sinh a} \\]
//!
//! (it can lie outside the span for steeply inclined spans) and
//!
//! \\[ y(x) = 2C \sinh\frac{x - 2x_m}{2C} \sinh\frac{x}{2C}, \qquad
//!     L = \sqrt{h^2 + (2C\sinh a)^2}, \qquad
//!     T(x) = H \cosh\frac{x - x_m}{C}. \\]
//!
//! The product forms avoid the cancellation of \\( \cosh p - \cosh q \\) for
//! taut spans. Closed form: no iteration, no exit clause needed. The inverse
//! problem ([`tension_for_sag`]) iterates and has exit clauses.

use std::fmt;
use std::time::Duration;

use ohl_core::budget::{Budget, Stop};
use ohl_core::units::{Force, ForcePerLength, Length};

/// Errors from sag-tension calculations.
#[derive(Debug, Clone, PartialEq)]
pub enum SagTensionError {
    /// An input is out of range (non-positive, not finite, …).
    InvalidInput(String),
    /// The cable is so slack that the catenary overflows `f64`.
    TooSlack {
        /// Span length divided by twice the catenary parameter.
        ratio: f64,
    },
    /// An iterative solver stopped before converging.
    NotConverged {
        /// What was being solved.
        problem: &'static str,
        /// Why it stopped (iteration limit, time limit, cancellation).
        stop: Stop,
        /// Diagnostic state at the time it stopped.
        detail: String,
    },
}

impl fmt::Display for SagTensionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(message) => write!(f, "invalid input: {message}"),
            Self::TooSlack { ratio } => write!(
                f,
                "cable too slack to compute (S/2C = {ratio:.1}); check the tension and weight"
            ),
            Self::NotConverged {
                problem,
                stop,
                detail,
            } => write!(f, "{problem} did not converge: {stop}; {detail}"),
        }
    }
}

impl std::error::Error for SagTensionError {}

fn require_positive(name: &str, value: f64) -> Result<(), SagTensionError> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(SagTensionError::InvalidInput(format!(
            "{name} must be positive and finite, got {value}"
        )))
    }
}

/// Above this value of S/2C, `sinh` overflows or loses all meaning.
const MAX_HALF_SPAN_RATIO: f64 = 300.0;

/// Geometry of a span: horizontal length and height difference.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpanGeometry {
    /// Horizontal distance between the supports, \\( S \\).
    pub span: Length,
    /// Elevation of the right support above the left one, \\( h \\).
    pub rise: Length,
}

impl SpanGeometry {
    /// Level span.
    pub fn level(span: Length) -> Self {
        Self {
            span,
            rise: Length::ZERO,
        }
    }

    /// Inclined span.
    pub fn inclined(span: Length, rise: Length) -> Self {
        Self { span, rise }
    }

    /// Straight-line distance between the supports.
    pub fn chord(&self) -> Length {
        Length::from_si(self.span.si().hypot(self.rise.si()))
    }
}

/// A cable state: horizontal tension and weight per unit length.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Catenary {
    /// Horizontal component of tension, \\( H \\) (constant along the span).
    pub horizontal_tension: Force,
    /// Weight per unit length, \\( w \\).
    pub weight: ForcePerLength,
}

impl Catenary {
    /// Creates a catenary; both values must be positive.
    pub fn new(horizontal_tension: Force, weight: ForcePerLength) -> Result<Self, SagTensionError> {
        require_positive("horizontal tension", horizontal_tension.si())?;
        require_positive("weight per length", weight.si())?;
        Ok(Self {
            horizontal_tension,
            weight,
        })
    }

    /// Catenary parameter \\( C = H / w \\).
    pub fn parameter(&self) -> Length {
        self.horizontal_tension / self.weight
    }

    /// Solves the catenary over a span.
    pub fn solve(&self, geometry: SpanGeometry) -> Result<SpanSolution, SagTensionError> {
        require_positive("span length", geometry.span.si())?;
        if !geometry.rise.si().is_finite() {
            return Err(SagTensionError::InvalidInput("rise must be finite".into()));
        }
        let c = self.parameter().si();
        let s = geometry.span.si();
        let a = s / (2.0 * c);
        if a > MAX_HALF_SPAN_RATIO {
            return Err(SagTensionError::TooSlack { ratio: a });
        }
        let h = geometry.rise.si();
        let low_point_x = s / 2.0 - c * (h / (2.0 * c * a.sinh())).asinh();
        Ok(SpanSolution {
            geometry,
            catenary: *self,
            c,
            low_point_x,
        })
    }
}

/// A catenary solved over a span. All positions are horizontal distances
/// from the left support; elevations are relative to the left support.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpanSolution {
    /// The span.
    pub geometry: SpanGeometry,
    /// The cable state.
    pub catenary: Catenary,
    c: f64,
    low_point_x: f64,
}

impl SpanSolution {
    fn y(&self, x: f64) -> f64 {
        let c = self.c;
        2.0 * c * ((x - 2.0 * self.low_point_x) / (2.0 * c)).sinh() * (x / (2.0 * c)).sinh()
    }

    /// Elevation of the cable at horizontal position `x`.
    pub fn elevation(&self, x: Length) -> Length {
        Length::from_si(self.y(x.si()))
    }

    /// Slope \\( dy/dx \\) at horizontal position `x`.
    pub fn slope(&self, x: Length) -> f64 {
        ((x.si() - self.low_point_x) / self.c).sinh()
    }

    /// Cable tension at horizontal position `x`.
    pub fn tension(&self, x: Length) -> Force {
        self.catenary.horizontal_tension * ((x.si() - self.low_point_x) / self.c).cosh()
    }

    /// Horizontal position of the lowest point of the catenary (may be
    /// outside `0..=S` on steep spans: then the lowest point of the cable is
    /// the lower support).
    pub fn low_point_x(&self) -> Length {
        Length::from_si(self.low_point_x)
    }

    /// Elevation of the lowest point of the (extended) catenary.
    pub fn low_point_elevation(&self) -> Length {
        Length::from_si(self.y(self.low_point_x))
    }

    /// Cable length between the supports.
    pub fn length(&self) -> Length {
        let half = self.geometry.span.si() / (2.0 * self.c);
        Length::from_si(self.geometry.rise.si().hypot(2.0 * self.c * half.sinh()))
    }

    /// Arc length from the left support to horizontal position `x`.
    pub fn arc_length_to(&self, x: Length) -> Length {
        let (c, x) = (self.c, x.si());
        Length::from_si(
            2.0 * c * ((x - 2.0 * self.low_point_x) / (2.0 * c)).cosh() * (x / (2.0 * c)).sinh(),
        )
    }

    /// Tension at the left support.
    pub fn tension_left(&self) -> Force {
        self.tension(Length::ZERO)
    }

    /// Tension at the right support.
    pub fn tension_right(&self) -> Force {
        self.tension(self.geometry.span)
    }

    /// Horizontal position of maximum sag: where the cable is parallel to
    /// the chord.
    pub fn sag_position(&self) -> Length {
        let chord_slope = self.geometry.rise.si() / self.geometry.span.si();
        Length::from_si(self.low_point_x + self.c * chord_slope.asinh())
    }

    /// Maximum sag: largest vertical distance between chord and cable.
    pub fn sag(&self) -> Length {
        self.sag_at(self.sag_position())
    }

    /// Vertical distance between chord and cable at horizontal position `x`.
    pub fn sag_at(&self, x: Length) -> Length {
        let chord = self.geometry.rise.si() * x.si() / self.geometry.span.si();
        Length::from_si(chord - self.y(x.si()))
    }

    /// Sag at mid-span.
    pub fn midspan_sag(&self) -> Length {
        self.sag_at(self.geometry.span / 2.0)
    }

    /// Total of tension along the cable, \\( \int_0^L T\,ds \\), in N·m.
    pub fn tension_integral(&self) -> f64 {
        let s = self.geometry.span.si();
        let c = self.catenary.parameter().si();
        let xm = self.low_point_x().si();
        self.catenary.horizontal_tension.si()
            * (s / 2.0 + c / 2.0 * (s / c).sinh() * ((s - 2.0 * xm) / c).cosh())
    }
}

/// Default exit clauses for [`tension_for_sag`]: bisection halves the
/// bracket each step, so ~60 steps reach `f64` precision; 200 steps leave a
/// wide margin (the time limit, 10 s, only guards against a loop).
pub fn default_solver_budget() -> Budget {
    Budget::new()
        .max_iterations(200)
        .time_limit(Duration::from_secs(10))
}

/// Finds the horizontal tension that gives a maximum sag of `sag` on a span.
///
/// Sag decreases monotonically with tension, so the solver brackets the
/// solution around the parabolic estimate \\( H_0 = w S^2 / 8D \\) and
/// bisects until the bracket is within 1e-12 relative. It stops with
/// [`SagTensionError::NotConverged`] when the budget runs out.
pub fn tension_for_sag(
    geometry: SpanGeometry,
    weight: ForcePerLength,
    sag: Length,
    budget: &Budget,
) -> Result<Catenary, SagTensionError> {
    require_positive("span length", geometry.span.si())?;
    require_positive("weight per length", weight.si())?;
    require_positive("sag", sag.si())?;
    let target = sag.si();
    let sag_for = |h: f64| -> f64 {
        Catenary::new(Force::from_si(h), weight)
            .and_then(|cat| cat.solve(geometry))
            .map_or(f64::INFINITY, |s| s.sag().si())
    };

    let mut run = budget.start();
    let not_converged = |stop: Stop, lo: f64, hi: f64| SagTensionError::NotConverged {
        problem: "tension for sag",
        stop,
        detail: format!("tension bracket [{lo:.6e}, {hi:.6e}] N"),
    };

    let estimate = weight.si() * geometry.span.si().powi(2) / (8.0 * target);
    let (mut lo, mut hi) = (estimate / 2.0, estimate * 2.0);
    // Widen the bracket until sag(lo) > target > sag(hi).
    while sag_for(lo) <= target {
        run.step().map_err(|stop| not_converged(stop, lo, hi))?;
        lo /= 2.0;
    }
    while sag_for(hi) >= target {
        run.step().map_err(|stop| not_converged(stop, lo, hi))?;
        hi *= 2.0;
    }
    while hi - lo > 1e-12 * hi {
        run.step().map_err(|stop| not_converged(stop, lo, hi))?;
        let mid = 0.5 * (lo + hi);
        if sag_for(mid) > target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Catenary::new(Force::from_si(0.5 * (lo + hi)), weight)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: f64, expected: f64, rel_tol: f64) {
        assert!(
            ((actual - expected) / expected).abs() <= rel_tol,
            "actual {actual}, expected {expected}"
        );
    }

    fn m(v: f64) -> Length {
        Length::from_meters(v)
    }

    fn drake_like(h_kn: f64) -> Catenary {
        Catenary::new(
            Force::from_kilonewtons(h_kn),
            ForcePerLength::from_newtons_per_meter(15.97),
        )
        .unwrap()
    }

    #[test]
    fn level_span_matches_textbook_formulas() {
        let sol = drake_like(28.0)
            .solve(SpanGeometry::level(m(300.0)))
            .unwrap();
        let c: f64 = 28_000.0 / 15.97;
        // Level span: sag = C (cosh(S/2C) - 1), L = 2C sinh(S/2C), at mid-span.
        close(sol.sag().meters(), c * ((150.0 / c).cosh() - 1.0), 1e-12);
        close(sol.length().meters(), 2.0 * c * (150.0 / c).sinh(), 1e-12);
        close(sol.sag_position().meters(), 150.0, 1e-12);
        close(sol.low_point_x().meters(), 150.0, 1e-12);
        close(
            sol.tension_left().newtons(),
            sol.tension_right().newtons(),
            1e-15,
        );
    }

    #[test]
    fn supports_are_on_the_curve() {
        for rise in [-80.0, -5.0, 0.0, 12.0, 150.0] {
            let sol = drake_like(25.0)
                .solve(SpanGeometry::inclined(m(400.0), m(rise)))
                .unwrap();
            assert!(sol.elevation(m(0.0)).meters().abs() < 1e-9);
            assert!(
                (sol.elevation(m(400.0)).meters() - rise).abs() < 1e-9,
                "rise {rise}"
            );
        }
    }

    #[test]
    fn support_tension_difference_is_weight_times_rise() {
        // Exact catenary identity: T_B - T_A = w h.
        let sol = drake_like(30.0)
            .solve(SpanGeometry::inclined(m(350.0), m(40.0)))
            .unwrap();
        let diff = sol.tension_right().newtons() - sol.tension_left().newtons();
        close(diff, 15.97 * 40.0, 1e-9);
    }

    #[test]
    fn arc_length_and_slope_are_consistent() {
        let sol = drake_like(20.0)
            .solve(SpanGeometry::inclined(m(500.0), m(-30.0)))
            .unwrap();
        close(
            sol.arc_length_to(m(500.0)).meters(),
            sol.length().meters(),
            1e-12,
        );
        // Slope at the sag position equals the chord slope.
        close(sol.slope(sol.sag_position()), -30.0 / 500.0, 1e-9);
        assert!(sol.length() > sol.geometry.chord());
    }

    #[test]
    fn taut_span_tends_to_parabola() {
        // Very high tension: catenary sag ≈ w S² / (8 H cos θ), θ = chord angle.
        // The weight acts per unit of cable length, and each horizontal metre
        // of an inclined span carries 1/cos θ metres of cable.
        let parabola = |span: f64, rise: f64| {
            let cos_theta = span / span.hypot(rise);
            15.97 * span.powi(2) / (8.0 * 5_000_000.0 * cos_theta)
        };
        for rise in [0.0, 20.0, -60.0] {
            let sol = drake_like(5_000.0)
                .solve(SpanGeometry::inclined(m(300.0), m(rise)))
                .unwrap();
            close(sol.sag().meters(), parabola(300.0, rise), 1e-5);
        }
    }

    #[test]
    fn steep_span_low_point_outside() {
        let sol = drake_like(10.0)
            .solve(SpanGeometry::inclined(m(200.0), m(120.0)))
            .unwrap();
        assert!(sol.low_point_x().meters() < 0.0);
        assert!(sol.tension_right() > sol.tension_left());
    }

    #[test]
    fn rejects_bad_input() {
        assert!(Catenary::new(Force::ZERO, ForcePerLength::from_newtons_per_meter(1.0)).is_err());
        assert!(
            drake_like(10.0)
                .solve(SpanGeometry::level(m(-1.0)))
                .is_err()
        );
        let slack = Catenary::new(
            Force::from_newtons(1.0),
            ForcePerLength::from_newtons_per_meter(20.0),
        )
        .unwrap();
        assert!(matches!(
            slack.solve(SpanGeometry::level(m(1000.0))),
            Err(SagTensionError::TooSlack { .. })
        ));
    }

    #[test]
    fn tension_for_sag_inverts_sag() {
        let w = ForcePerLength::from_newtons_per_meter(15.97);
        for (span, rise, h_kn) in [(300.0, 0.0, 28.0), (450.0, 35.0, 22.0), (120.0, -10.0, 5.0)] {
            let geometry = SpanGeometry::inclined(m(span), m(rise));
            let sag = drake_like(h_kn).solve(geometry).unwrap().sag();
            let found = tension_for_sag(geometry, w, sag, &default_solver_budget()).unwrap();
            close(found.horizontal_tension.kilonewtons(), h_kn, 1e-9);
        }
    }

    #[test]
    fn tension_for_sag_reports_why_it_stops() {
        let budget = Budget::new().max_iterations(5);
        let error = tension_for_sag(
            SpanGeometry::level(m(300.0)),
            ForcePerLength::from_newtons_per_meter(15.97),
            m(6.4),
            &budget,
        )
        .unwrap_err();
        let SagTensionError::NotConverged { stop, detail, .. } = &error else {
            panic!("{error}");
        };
        assert!(matches!(stop, Stop::IterationLimit { iterations: 5, .. }));
        assert!(detail.contains("bracket"), "{error}");
    }
}
