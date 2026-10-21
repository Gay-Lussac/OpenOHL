// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Change of state of a single span: linear elastic and thermal behaviour.
//!
//! A cable strung in a span has a fixed **unstressed length** \\( L_0 \\) at
//! a reference temperature \\( t_0 \\). Under another temperature \\( t \\)
//! and load (weight per length \\( w \\)), its length is
//!
//! \\[ L = L_0 \, e^{\alpha (t - t_0)} \left( 1 + \frac{\bar T}{EA} \right),
//!     \qquad \bar T = \frac{1}{L} \int_0^L T \, ds \\]
//!
//! and must equal the geometric length of the catenary through the supports. For
//! a catenary with horizontal tension \\( H \\):
//!
//! \\[ \int_0^L T\,ds = H \left[ \frac{S}{2} + \frac{C}{2} \sinh\frac{S}{C}
//!     \cosh\frac{S - 2x_m}{C} \right]. \\]
//!
//! The thermal factor \\( e^{\alpha \Delta t} \\) is the exact integral of
//! \\( dL / (L\,dT) = \alpha \\), and thermal and elastic factors multiply.
//! The common textbook form \\( 1 + \alpha\Delta t + \bar T/EA \\) agrees to
//! second order (~1e-6 in strain) but is not path independent: defining the
//! cable at 15 °C and solving at 75 °C, then defining it from that result and
//! solving back at 15 °C, gave 28.009 kN instead of 28 kN, because the small
//! slack of a span magnifies length errors into tension errors. The
//! multiplicative form makes every state lead to the same unstressed length.
//!
//! The mismatch decreases strictly with \\( H \\) (the catenary gets shorter,
//! the cable gets longer), so there is exactly one solution. It is found by
//! bracketed bisection with exit clauses.
//!
//! **The cable's own weight is conserved.** Its weight per metre
//! ([`LoadState::weight`]) is per metre of *unstressed* cable at the
//! reference temperature, so a span weighs \\( w L_0 \\) in every state.
//! The catenary carries its load per metre of stretched cable, so the own
//! weight is spread over the span's length: \\( w L_0 / L = w / k \\) per
//! metre, with the stretch \\( k = e^{\alpha (t - t_0)} (1 + \bar T/EA) \\);
//! ice and wind stay per metre of the real (stretched) conductor. As
//! \\( \bar T \\) depends on the load, \\( k \\) is a fixed point found for
//! each trial tension (it changes the load by ~10⁻³ and the
//! mean tension by ~10⁻⁵ of that, so two or three passes settle it).
//!
//! Non-linear stress-strain behaviour and creep are separate models (next
//! steps); this module is the linear-elastic baseline.

use ohl_core::budget::{Budget, Stop};
use ohl_core::units::{Force, ForcePerLength, Length, Temperature};

use crate::catenary::{Catenary, SagTensionError, SpanGeometry, SpanSolution};

/// Linear-elastic properties of a cable used by the change of state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ElasticCable {
    /// Axial stiffness \\( EA \\).
    pub axial_stiffness: Force,
    /// Thermal expansion coefficient, 1/K.
    pub thermal_expansion: f64,
}

/// Load and temperature of a cable state.
///
/// `load` is the resultant per metre of conductor; `weight` is the part of
/// it that is the cable's own weight (vertical), `wind` its horizontal
/// part; the rest of the vertical load is ice (or another load per metre of
/// conductor). The own weight is per metre of unstressed cable at the
/// reference temperature (that of the known state): the solvers keep the
/// span's total weight, spreading it over the stretched length
/// ([`LoadState::load_at`]). Ice and wind act per metre of the real
/// conductor and are not scaled.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoadState {
    /// Resultant load per unit length (weight, plus ice and wind when present).
    pub load: ForcePerLength,
    /// The cable's own weight per unit length, part of `load` (vertical):
    /// per metre of unstressed cable at the reference temperature. Zero:
    /// none of the load is scaled.
    pub weight: ForcePerLength,
    /// The horizontal (wind) part of `load`.
    pub wind: ForcePerLength,
    /// Cable temperature.
    pub temperature: Temperature,
}

impl LoadState {
    /// The bare cable: its own weight per metre.
    pub fn bare(weight: ForcePerLength, temperature: Temperature) -> Self {
        Self {
            load: weight,
            weight,
            wind: ForcePerLength::ZERO,
            temperature,
        }
    }

    /// The cable's own weight, an extra vertical load per metre of
    /// conductor (ice) and a wind load per metre across it.
    pub fn loaded(
        weight: ForcePerLength,
        ice: ForcePerLength,
        wind: ForcePerLength,
        temperature: Temperature,
    ) -> Self {
        let vertical = weight.si() + ice.si();
        Self {
            load: ForcePerLength::from_si(vertical.hypot(wind.si())),
            weight,
            wind,
            temperature,
        }
    }

    /// The resultant load per metre of a cable stretched by `stretch` (its
    /// length over the unstressed length at the reference temperature that
    /// carries [`weight`](Self::weight) per metre): the own weight spread
    /// over the longer cable, \\( w / k \\); ice and wind as given.
    pub fn load_at(&self, stretch: f64) -> ForcePerLength {
        let (w, h) = (self.weight.si(), self.wind.si());
        if w == 0.0 {
            return self.load;
        }
        let vertical = if h == 0.0 {
            self.load.si()
        } else {
            (self.load.si() - h).sqrt() * (self.load.si() + h).sqrt()
        };
        // The ice: the rest of the vertical load (none below rounding).
        let ice = (vertical - w).max(0.0);
        ForcePerLength::from_si((ice + w / stretch).hypot(h))
    }

    /// Checks that `weight` and `wind` are parts of `load` (to 10⁻⁶ of it).
    pub(crate) fn check(&self) -> Result<(), SagTensionError> {
        let (q, w, h) = (self.load.si(), self.weight.si(), self.wind.si());
        let ok = [q, w, h].iter().all(|x| x.is_finite() && *x >= 0.0)
            && h <= q
            // Inputs rounded to some digits: within 10⁻⁶ of the load.
            && w <= (q - h).sqrt() * (q + h).sqrt() + 1e-6 * q;
        if ok {
            Ok(())
        } else {
            Err(SagTensionError::InvalidInput(format!(
                "load {q} N/m must include the own weight {w} N/m and the wind {h} N/m"
            )))
        }
    }
}

/// Passes of the fixed point on a span's stretch ([`span_at`]); it settles
/// in two or three.
const STRETCH_PASSES: usize = 50;

/// The catenary of span `geometry` at horizontal tension `h` in `state`,
/// the cable's own weight spread over its stretched length: per metre
/// \\( w / k \\), where `stretch` gives \\( k \\) (the span's length over
/// the length carrying \\( w \\) per metre) from the solution. A fixed
/// point on \\( k \\) (exit clause: [`STRETCH_PASSES`] passes). Returns the
/// solution and its \\( k \\).
pub(crate) fn span_at(
    h: f64,
    state: &LoadState,
    geometry: SpanGeometry,
    stretch: impl Fn(&SpanSolution) -> Result<f64, SagTensionError>,
) -> Result<(SpanSolution, f64), SagTensionError> {
    let mut k = 1.0;
    for _ in 0..STRETCH_PASSES {
        let solution = Catenary::new(Force::from_si(h), state.load_at(k))?.solve(geometry)?;
        let next = stretch(&solution)?;
        if !(next.is_finite() && next > 0.0) {
            return Err(SagTensionError::InvalidInput(format!(
                "stretch {next} of the cable is not positive"
            )));
        }
        if state.weight.si() == 0.0 || (next - k).abs() <= 4.0 * f64::EPSILON * next {
            return Ok((solution, next));
        }
        k = next;
    }
    Err(SagTensionError::InvalidInput(format!(
        "the stretch of the cable (its own weight spread over it) did not settle in \
         {STRETCH_PASSES} passes at H = {h:.3} N"
    )))
}

/// A cable installed in a span, defined by its unstressed length.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InstalledSpan {
    /// The span.
    pub geometry: SpanGeometry,
    /// Cable properties.
    pub cable: ElasticCable,
    /// Unstressed length at the reference temperature.
    pub unstressed_length: Length,
    /// Reference temperature of the unstressed length.
    pub reference_temperature: Temperature,
}

impl InstalledSpan {
    /// Installs a cable from one known state, e.g. "28 kN horizontal tension
    /// at 15 °C, bare conductor". The state's temperature becomes the
    /// reference temperature, and its own weight per metre is per metre of
    /// the unstressed cable there.
    pub fn from_known_state(
        geometry: SpanGeometry,
        cable: ElasticCable,
        state: LoadState,
        horizontal_tension: Force,
    ) -> Result<Self, SagTensionError> {
        check_cable(cable)?;
        state.check()?;
        let (solution, stretch) = span_at(horizontal_tension.si(), &state, geometry, |s| {
            Ok(1.0 + mean_strain(s, cable))
        })?;
        Ok(Self {
            geometry,
            cable,
            unstressed_length: Length::from_si(solution.length().si() / stretch),
            reference_temperature: state.temperature,
        })
    }

    /// The span at trial tension `h` in `state`, and its stretch over the
    /// unstressed length at the reference temperature.
    fn at(&self, state: &LoadState, h: f64) -> Result<(SpanSolution, f64), SagTensionError> {
        let thermal = (self.cable.thermal_expansion
            * (state.temperature - self.reference_temperature).kelvin())
        .exp();
        span_at(h, state, self.geometry, |s| {
            Ok(thermal * (1.0 + mean_strain(s, self.cable)))
        })
    }

    /// Length mismatch (catenary length − cable length) for a trial tension.
    /// Positive: tension too low. `None` if the trial catenary is invalid
    /// (e.g. too slack to compute), which means "tension too low".
    fn mismatch(&self, state: &LoadState, h: f64) -> Option<f64> {
        let (solution, stretch) = self.at(state, h).ok()?;
        Some(solution.length().si() - self.unstressed_length.si() * stretch)
    }

    /// Solves the span at another load and temperature.
    ///
    /// Expected cost: ~60 bisection steps of a closed-form catenary, each
    /// two or three times for the stretch (a few tens of microseconds).
    /// Stops with [`SagTensionError::NotConverged`] when the budget runs
    /// out, reporting the tension bracket reached.
    pub fn solve(
        &self,
        state: LoadState,
        budget: &Budget,
    ) -> Result<SpanSolution, SagTensionError> {
        if !(state.load.si().is_finite() && state.load.si() > 0.0) {
            return Err(SagTensionError::InvalidInput(format!(
                "load per length must be positive, got {}",
                state.load
            )));
        }
        state.check()?;
        let too_low = |h: f64| self.mismatch(&state, h).is_none_or(|m| m > 0.0);
        let mut run = budget.start();
        let not_converged = |stop: Stop, lo: f64, hi: f64| SagTensionError::NotConverged {
            problem: "change of state",
            stop,
            detail: format!(
                "tension bracket [{lo:.6e}, {hi:.6e}] N at {:.1} °C, {:.3} N/m",
                state.temperature.celsius(),
                state.load.newtons_per_meter()
            ),
        };

        // Start from the elastic modulus scale and widen both ways.
        let start = (self.cable.axial_stiffness.si() * 1e-3).max(1.0);
        let (mut lo, mut hi) = (start, start);
        while !too_low(lo) {
            run.step().map_err(|stop| not_converged(stop, lo, hi))?;
            lo /= 2.0;
        }
        while too_low(hi) {
            run.step().map_err(|stop| not_converged(stop, lo, hi))?;
            hi *= 2.0;
        }
        while hi - lo > 1e-12 * hi {
            run.step().map_err(|stop| not_converged(stop, lo, hi))?;
            let mid = 0.5 * (lo + hi);
            if too_low(mid) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        Ok(self.at(&state, 0.5 * (lo + hi))?.0)
    }
}

/// Mean elastic strain \\( \bar T / EA \\) of a solved span.
pub(crate) fn mean_strain(solution: &SpanSolution, cable: ElasticCable) -> f64 {
    solution.tension_integral() / solution.length().si() / cable.axial_stiffness.si()
}

fn check_cable(cable: ElasticCable) -> Result<(), SagTensionError> {
    let ea = cable.axial_stiffness.si();
    if !(ea.is_finite() && ea > 0.0) {
        return Err(SagTensionError::InvalidInput(format!(
            "axial stiffness must be positive, got {ea} N"
        )));
    }
    if !cable.thermal_expansion.is_finite() {
        return Err(SagTensionError::InvalidInput(
            "thermal expansion must be finite".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catenary::default_solver_budget;

    fn close(actual: f64, expected: f64, rel_tol: f64) {
        assert!(
            ((actual - expected) / expected).abs() <= rel_tol,
            "actual {actual}, expected {expected}"
        );
    }

    fn drake() -> ElasticCable {
        ElasticCable {
            axial_stiffness: Force::from_si(34.64e6),
            thermal_expansion: 18.9e-6,
        }
    }

    /// Drake's own weight, N/m.
    const W: f64 = 15.97;

    /// Drake bare (`load` = its weight) or with ice (the rest).
    fn state(load: f64, celsius: f64) -> LoadState {
        state_with(W, load, celsius)
    }

    /// A cable of own weight `weight` under `load` (the rest ice).
    fn state_with(weight: f64, load: f64, celsius: f64) -> LoadState {
        LoadState::loaded(
            ForcePerLength::from_newtons_per_meter(weight),
            ForcePerLength::from_newtons_per_meter(load - weight),
            ForcePerLength::ZERO,
            Temperature::from_celsius(celsius),
        )
    }

    fn installed(rise: f64) -> InstalledSpan {
        InstalledSpan::from_known_state(
            SpanGeometry::inclined(Length::from_meters(300.0), Length::from_meters(rise)),
            drake(),
            state(15.97, 15.0),
            Force::from_kilonewtons(28.0),
        )
        .unwrap()
    }

    #[test]
    fn same_state_gives_same_tension() {
        for rise in [0.0, 40.0] {
            let solution = installed(rise)
                .solve(state(15.97, 15.0), &default_solver_budget())
                .unwrap();
            close(
                solution.catenary.horizontal_tension.kilonewtons(),
                28.0,
                1e-9,
            );
        }
    }

    #[test]
    fn heat_lowers_tension_and_load_raises_it() {
        let span = installed(0.0);
        let budget = default_solver_budget();
        let hot = span.solve(state(15.97, 75.0), &budget).unwrap();
        let cold_iced = span.solve(state(30.0, -20.0), &budget).unwrap();
        assert!(hot.catenary.horizontal_tension.kilonewtons() < 28.0);
        assert!(hot.sag() > Length::from_meters(6.42));
        assert!(cold_iced.catenary.horizontal_tension.kilonewtons() > 28.0);
    }

    #[test]
    fn rigid_cable_keeps_its_length() {
        // With a huge EA and no thermal expansion, the cable length stays
        // equal to the installed length whatever the load.
        let geometry = SpanGeometry::level(Length::from_meters(300.0));
        let rigid = ElasticCable {
            axial_stiffness: Force::from_si(1e15),
            thermal_expansion: 0.0,
        };
        let span = InstalledSpan::from_known_state(
            geometry,
            rigid,
            state(15.97, 15.0),
            Force::from_kilonewtons(28.0),
        )
        .unwrap();
        let loaded = span
            .solve(state(40.0, 60.0), &default_solver_budget())
            .unwrap();
        close(
            loaded.length().meters(),
            span.unstressed_length.meters(),
            1e-9,
        );
        // Same length and shape => same sag; H scales with the load.
        close(
            loaded.catenary.horizontal_tension.kilonewtons(),
            28.0 * 40.0 / 15.97,
            1e-6,
        );
    }

    #[test]
    fn states_are_path_independent() {
        // Installing from any solved state gives the same unstressed length
        // (and the same cable: its weight per metre of unstressed cable at
        // the new reference temperature, 75 °C, is less by exp(alpha dt)).
        let span = installed(25.0);
        let budget = default_solver_budget();
        let hot = span.solve(state(15.97, 75.0), &budget).unwrap();
        let w75 = W * (-drake().thermal_expansion * 60.0).exp();
        let again = InstalledSpan::from_known_state(
            span.geometry,
            drake(),
            state_with(w75, w75, 75.0),
            hot.catenary.horizontal_tension,
        )
        .unwrap();
        close(
            again.unstressed_length.meters(),
            span.unstressed_length.meters() * (drake().thermal_expansion * 60.0).exp(),
            1e-12,
        );
        let back = again.solve(state_with(w75, w75, 15.0), &budget).unwrap();
        close(back.catenary.horizontal_tension.kilonewtons(), 28.0, 1e-8);
    }

    /// The span weighs the same in every state: its own weight per metre of
    /// stretched cable times its length is \\( w L_0 \\) (hot, cold, iced:
    /// the ice per metre of the real cable on top).
    #[test]
    fn total_weight_is_conserved() {
        let budget = default_solver_budget();
        for rise in [0.0, 40.0] {
            let span = installed(rise);
            let total = W * span.unstressed_length.meters();
            for (load, t) in [(15.97, 15.0), (15.97, 150.0), (15.97, -20.0), (30.0, -5.0)] {
                let s = span.solve(state(load, t), &budget).unwrap();
                let own = s.catenary.weight.newtons_per_meter() - (load - W);
                close(own * s.length().meters(), total, 1e-13);
            }
        }
        // Before: the weight per metre of stretched cable stayed W, so at
        // 150 °C the span weighed 0.25 % more than when strung.
        let span = installed(0.0);
        let hot = span.solve(state(15.97, 150.0), &budget).unwrap();
        assert!(hot.catenary.weight.newtons_per_meter() < W * (1.0 - 2.5e-3));
    }

    /// Wind stays as given; the own weight is scaled in the vertical part.
    #[test]
    fn load_at_scales_only_the_own_weight() {
        let s = LoadState::loaded(
            ForcePerLength::from_si(15.0),
            ForcePerLength::from_si(5.0),
            ForcePerLength::from_si(12.0),
            Temperature::from_celsius(0.0),
        );
        close(s.load.si(), 20.0f64.hypot(12.0), 1e-15);
        close(s.load_at(1.0).si(), s.load.si(), 1e-15);
        close(s.load_at(1.5).si(), 15.0f64.hypot(12.0), 1e-14);
        let none = LoadState {
            weight: ForcePerLength::ZERO,
            ..s
        };
        assert_eq!(none.load_at(2.0), s.load);
        // A weight larger than the vertical load is refused.
        let bad = LoadState {
            weight: ForcePerLength::from_si(30.0),
            ..s
        };
        assert!(bad.check().is_err());
    }

    #[test]
    fn exit_clause_reports_state() {
        let error = installed(0.0)
            .solve(state(15.97, 75.0), &Budget::new().max_iterations(3))
            .unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("change of state did not converge"),
            "{message}"
        );
        assert!(message.contains("75.0 °C"), "{message}");
    }
}
