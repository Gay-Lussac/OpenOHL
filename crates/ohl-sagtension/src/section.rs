// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tension sections: several spans between two dead-ends.
//!
//! Suspension insulators swing until the horizontal tension is (very nearly)
//! the same in every span of a section. Two methods are provided:
//!
//! - [`ruling_span`]: the classic equivalent span
//!   \\( S_r = \sqrt{\sum S_i^3 / \sum S_i} \\), exact for level spans under
//!   the parabolic approximation; used for reporting and comparison.
//! - [`TensionSection`]: an exact equal-\\( H \\) solution. Each span keeps its
//!   own geometry (inclination included); the cable is one piece whose total
//!   unstressed length is conserved:
//!
//! \\[ \sum_i \frac{L_i(H)}{1 + \bar T_i(H)/EA} = L_0\, e^{\alpha (t - t_0)} \\]
//!
//! The cable's own weight is conserved as in
//! [`change_of_state`](crate::change_of_state): each span carries
//! \\( w / (e^{\alpha (t - t_0)} (1 + \bar T_i/EA)) \\) per metre of its
//! stretched length (one \\( H \\), a load per metre of its own per span).
//!
//! The left side decreases strictly with \\( H \\), so the root is unique and
//! found by bracketed bisection with exit clauses. With one span this is
//! exactly [`InstalledSpan`](crate::InstalledSpan). Insulator swing geometry
//! (string length changing the span lengths) is not modelled yet.

use ohl_core::budget::{Budget, Stop};
use ohl_core::units::{Force, Length, Temperature};

use crate::catenary::{SagTensionError, SpanGeometry, SpanSolution};
use crate::change_of_state::{ElasticCable, LoadState, mean_strain, span_at};

/// Classic ruling span \\( \sqrt{\sum S_i^3 / \sum S_i} \\) from the
/// horizontal span lengths.
pub fn ruling_span(spans: &[SpanGeometry]) -> Result<Length, SagTensionError> {
    if spans.is_empty() {
        return Err(SagTensionError::InvalidInput(
            "a section needs at least one span".into(),
        ));
    }
    let (cubes, sum) = spans.iter().fold((0.0, 0.0), |(c, s), g| {
        let x = g.span.si();
        (c + x * x * x, s + x)
    });
    Ok(Length::from_si((cubes / sum).sqrt()))
}

/// A cable strung through a tension section.
#[derive(Debug, Clone, PartialEq)]
pub struct TensionSection {
    /// Spans, in order along the line.
    pub spans: Vec<SpanGeometry>,
    /// Cable properties.
    pub cable: ElasticCable,
    /// Total unstressed length at the reference temperature.
    pub unstressed_length: Length,
    /// Reference temperature of the unstressed length.
    pub reference_temperature: Temperature,
}

/// Solution of a section in one state: the common horizontal tension and
/// every span's catenary.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionSolution {
    /// Horizontal tension, equal in all spans.
    pub horizontal_tension: Force,
    /// Span solutions, in order.
    pub spans: Vec<SpanSolution>,
}

impl TensionSection {
    /// Strings the cable from a known state: horizontal tension `horizontal_tension`
    /// under `state` (e.g. 28 kN at 15 °C, bare).
    pub fn from_known_state(
        spans: Vec<SpanGeometry>,
        cable: ElasticCable,
        state: LoadState,
        horizontal_tension: Force,
    ) -> Result<Self, SagTensionError> {
        state.check()?;
        let unstressed =
            Self::total_unstrained(&spans, cable, horizontal_tension.si(), &state, 1.0)?.1;
        Ok(Self {
            spans,
            cable,
            unstressed_length: Length::from_si(unstressed),
            reference_temperature: state.temperature,
        })
    }

    /// The spans at tension `h` in `state` (each its own weight per metre,
    /// [`span_at`]) and their total unstressed length at the reference
    /// temperature; `thermal` is \\( e^{\alpha (t - t_0)} \\).
    fn total_unstrained(
        spans: &[SpanGeometry],
        cable: ElasticCable,
        h: f64,
        state: &LoadState,
        thermal: f64,
    ) -> Result<(Vec<SpanSolution>, f64), SagTensionError> {
        if spans.is_empty() {
            return Err(SagTensionError::InvalidInput(
                "a section needs at least one span".into(),
            ));
        }
        let mut total = 0.0;
        let mut solved = Vec::with_capacity(spans.len());
        for &geometry in spans {
            let (solution, stretch) = span_at(h, state, geometry, |s| {
                Ok(thermal * (1.0 + mean_strain(s, cable)))
            })?;
            total += solution.length().si() / stretch;
            solved.push(solution);
        }
        Ok((solved, total))
    }

    /// Each span's unstressed length (at the reference temperature, m) in
    /// a solution of the section in `state`: its length over its stretch
    /// \\( e^{\alpha (t - t_0)} (1 + \bar T_i/EA) \\). They add up to the
    /// section's unstressed length.
    pub fn span_unstressed_lengths(
        &self,
        solution: &SectionSolution,
        state: &LoadState,
    ) -> Vec<f64> {
        let thermal = (self.cable.thermal_expansion
            * (state.temperature - self.reference_temperature).kelvin())
        .exp();
        solution
            .spans
            .iter()
            .map(|s| s.length().si() / (thermal * (1.0 + mean_strain(s, self.cable))))
            .collect()
    }

    /// Solves the section at a load and temperature.
    ///
    /// Expected cost: ~60 bisection steps × one catenary per span (about
    /// 60 µs for 20 spans). Stops with [`SagTensionError::NotConverged`]
    /// when the budget runs out.
    pub fn solve(
        &self,
        state: LoadState,
        budget: &Budget,
    ) -> Result<SectionSolution, SagTensionError> {
        state.check()?;
        let thermal = (self.cable.thermal_expansion
            * (state.temperature - self.reference_temperature).kelvin())
        .exp();
        let target = self.unstressed_length.si();
        let at = |h: f64| Self::total_unstrained(&self.spans, self.cable, h, &state, thermal);
        // True when the trial tension is too low (cable longer than needed),
        // including "too slack to compute".
        let too_low = |h: f64| -> Result<bool, SagTensionError> {
            match at(h) {
                Ok((_, total)) => Ok(total > target),
                Err(SagTensionError::TooSlack { .. }) => Ok(true),
                Err(other) => Err(other),
            }
        };
        let mut run = budget.start();
        let not_converged = |stop: Stop, lo: f64, hi: f64| SagTensionError::NotConverged {
            problem: "tension section",
            stop,
            detail: format!(
                "{} spans, tension bracket [{lo:.6e}, {hi:.6e}] N at {:.1} °C, {:.3} N/m",
                self.spans.len(),
                state.temperature.celsius(),
                state.load.newtons_per_meter()
            ),
        };
        let start = (self.cable.axial_stiffness.si() * 1e-3).max(1.0);
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
        let h = 0.5 * (lo + hi);
        Ok(SectionSolution {
            horizontal_tension: Force::from_si(h),
            spans: at(h)?.0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catenary::default_solver_budget;
    use crate::change_of_state::InstalledSpan;
    use ohl_core::units::ForcePerLength;

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

    /// Drake (own weight 15.97 N/m) under `load` (the rest ice), its
    /// weight per metre given at `weight_at` °C (the reference).
    fn state_ref(load: f64, celsius: f64, weight_at: f64) -> LoadState {
        let w = 15.97 * (-drake().thermal_expansion * (weight_at - 15.0)).exp();
        LoadState::loaded(
            ForcePerLength::from_newtons_per_meter(w),
            ForcePerLength::from_newtons_per_meter(load - 15.97),
            ForcePerLength::ZERO,
            Temperature::from_celsius(celsius),
        )
    }

    fn state(load: f64, celsius: f64) -> LoadState {
        state_ref(load, celsius, 15.0)
    }

    fn level(spans: &[f64]) -> Vec<SpanGeometry> {
        spans
            .iter()
            .map(|&s| SpanGeometry::level(Length::from_meters(s)))
            .collect()
    }

    #[test]
    fn ruling_span_formula() {
        // sqrt((200³ + 300³ + 400³) / 900) = sqrt(99e6 / 900)
        let rs = ruling_span(&level(&[200.0, 300.0, 400.0])).unwrap();
        close(rs.meters(), (99.0e6_f64 / 900.0).sqrt(), 1e-12);
        close(
            ruling_span(&level(&[350.0])).unwrap().meters(),
            350.0,
            1e-15,
        );
    }

    #[test]
    fn one_span_section_equals_installed_span() {
        let geometry =
            SpanGeometry::inclined(Length::from_meters(450.0), Length::from_meters(35.0));
        let h = Force::from_kilonewtons(22.0);
        let section =
            TensionSection::from_known_state(vec![geometry], drake(), state(15.97, 15.0), h)
                .unwrap();
        let span =
            InstalledSpan::from_known_state(geometry, drake(), state(15.97, 15.0), h).unwrap();
        let budget = default_solver_budget();
        for s in [state(15.97, 100.0), state(32.0, -20.0)] {
            close(
                section
                    .solve(s, &budget)
                    .unwrap()
                    .horizontal_tension
                    .newtons(),
                span.solve(s, &budget)
                    .unwrap()
                    .catenary
                    .horizontal_tension
                    .newtons(),
                1e-10,
            );
        }
    }

    #[test]
    fn equal_spans_behave_like_one_span() {
        let h = Force::from_kilonewtons(28.0);
        let three =
            TensionSection::from_known_state(level(&[300.0; 3]), drake(), state(15.97, 15.0), h)
                .unwrap();
        let one = TensionSection::from_known_state(level(&[300.0]), drake(), state(15.97, 15.0), h)
            .unwrap();
        let budget = default_solver_budget();
        close(
            three
                .solve(state(15.97, 75.0), &budget)
                .unwrap()
                .horizontal_tension
                .newtons(),
            one.solve(state(15.97, 75.0), &budget)
                .unwrap()
                .horizontal_tension
                .newtons(),
            1e-10,
        );
    }

    #[test]
    fn exact_section_is_close_to_ruling_span_for_level_spans() {
        let spans = level(&[220.0, 310.0, 380.0, 260.0]);
        let rs = ruling_span(&spans).unwrap();
        let h = Force::from_kilonewtons(28.0);
        let budget = default_solver_budget();
        let section =
            TensionSection::from_known_state(spans, drake(), state(15.97, 15.0), h).unwrap();
        let ruling = InstalledSpan::from_known_state(
            SpanGeometry::level(rs),
            drake(),
            state(15.97, 15.0),
            h,
        )
        .unwrap();
        let exact = section
            .solve(state(15.97, 75.0), &budget)
            .unwrap()
            .horizontal_tension;
        let approx = ruling
            .solve(state(15.97, 75.0), &budget)
            .unwrap()
            .catenary
            .horizontal_tension;
        // Same physics up to catenary-vs-parabola effects: within 0.5 %.
        close(exact.newtons(), approx.newtons(), 5e-3);
    }

    #[test]
    fn path_independent() {
        let spans = vec![
            SpanGeometry::inclined(Length::from_meters(300.0), Length::from_meters(20.0)),
            SpanGeometry::inclined(Length::from_meters(420.0), Length::from_meters(-45.0)),
        ];
        let budget = default_solver_budget();
        let section = TensionSection::from_known_state(
            spans.clone(),
            drake(),
            state(15.97, 15.0),
            Force::from_kilonewtons(25.0),
        )
        .unwrap();
        let hot = section.solve(state(15.97, 80.0), &budget).unwrap();
        // Defined again at 80 °C: the same cable weighs less per metre there.
        let again = TensionSection::from_known_state(
            spans,
            drake(),
            state_ref(15.97, 80.0, 80.0),
            hot.horizontal_tension,
        )
        .unwrap();
        close(
            again
                .solve(state_ref(15.97, 15.0, 80.0), &budget)
                .unwrap()
                .horizontal_tension
                .kilonewtons(),
            25.0,
            1e-8,
        );
    }

    #[test]
    fn rejects_empty_section() {
        assert!(ruling_span(&[]).is_err());
        assert!(
            TensionSection::from_known_state(
                vec![],
                drake(),
                state(15.97, 15.0),
                Force::from_kilonewtons(20.0)
            )
            .is_err()
        );
    }
}
