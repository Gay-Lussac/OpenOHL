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
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoadState {
    /// Resultant load per unit length (weight, plus ice and wind when present).
    pub load: ForcePerLength,
    /// Cable temperature.
    pub temperature: Temperature,
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
    /// at 15 °C, bare conductor".
    pub fn from_known_state(
        geometry: SpanGeometry,
        cable: ElasticCable,
        state: LoadState,
        horizontal_tension: Force,
    ) -> Result<Self, SagTensionError> {
        check_cable(cable)?;
        let solution = Catenary::new(horizontal_tension, state.load)?.solve(geometry)?;
        let length = solution.length().si();
        let mean_strain = solution.tension_integral() / length / cable.axial_stiffness.si();
        Ok(Self {
            geometry,
            cable,
            unstressed_length: Length::from_si(length / (1.0 + mean_strain)),
            reference_temperature: state.temperature,
        })
    }

    /// Length mismatch (catenary length − cable length) for a trial tension.
    /// Positive: tension too low. `None` if the trial catenary is invalid
    /// (e.g. too slack to compute), which means "tension too low".
    fn mismatch(&self, state: LoadState, h: f64) -> Option<f64> {
        let solution = Catenary::new(Force::from_si(h), state.load)
            .and_then(|c| c.solve(self.geometry))
            .ok()?;
        let length = solution.length().si();
        let thermal = (self.cable.thermal_expansion
            * (state.temperature - self.reference_temperature).kelvin())
        .exp();
        let elastic = solution.tension_integral() / length / self.cable.axial_stiffness.si();
        Some(length - self.unstressed_length.si() * thermal * (1.0 + elastic))
    }

    /// Solves the span at another load and temperature.
    ///
    /// Expected cost: ~60 bisection steps of a closed-form catenary (a few
    /// microseconds). Stops with [`SagTensionError::NotConverged`] when the
    /// budget runs out, reporting the tension bracket reached.
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
        let too_low = |h: f64| self.mismatch(state, h).is_none_or(|m| m > 0.0);
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
        Catenary::new(Force::from_si(0.5 * (lo + hi)), state.load)?.solve(self.geometry)
    }
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

    fn state(load: f64, celsius: f64) -> LoadState {
        LoadState {
            load: ForcePerLength::from_newtons_per_meter(load),
            temperature: Temperature::from_celsius(celsius),
        }
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
        // Installing from any solved state gives the same unstressed length.
        let span = installed(25.0);
        let budget = default_solver_budget();
        let hot = span.solve(state(15.97, 75.0), &budget).unwrap();
        let again = InstalledSpan::from_known_state(
            span.geometry,
            drake(),
            state(15.97, 75.0),
            hot.catenary.horizontal_tension,
        )
        .unwrap();
        let back = again.solve(state(15.97, 15.0), &budget).unwrap();
        close(back.catenary.horizontal_tension.kilonewtons(), 28.0, 1e-8);
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
