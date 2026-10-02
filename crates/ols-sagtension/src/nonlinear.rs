// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Sag-tension with non-linear stress-strain and creep.
//!
//! The cable model ([`Mechanics`]) gives the total strain \\( \varepsilon(T, t, s) \\)
//! for a tension \\( T \\), temperature \\( t \\) and material state \\( s \\)
//! (initial, creep curve, or final from a permanent set). A tension section
//! keeps one unstressed length \\( L_0 \\) (at zero strain, at the curves'
//! reference temperature), and each span stretches with the strain at its
//! mean tension \\( \bar T_i \\):
//!
//! \\[ \sum_i \frac{L_i(H)}{1 + \varepsilon(\bar T_i(H), t, s)} = L_0 \\]
//!
//! (using the mean tension per span is the usual sag-tension practice). The
//! left side decreases strictly with \\( H \\), so bisection with exit clauses
//! finds the unique solution. Because \\( L_0 \\) is defined at a fixed
//! reference, results do not depend on which state defined the cable.
//!
//! **Final condition** ([`final_condition`]): the cable is first strung on
//! its initial curve. The permanent set *after load* comes from the heaviest
//! load case on the initial curve; *after creep* from the creep case (usually
//! everyday temperature, bare) on the creep curve. The final state is the one
//! that leaves the cable longer; afterwards it behaves elastically along the
//! final modulus (back onto the initial curve if loaded beyond).

use ols_cables::mechanics::{MaterialState, Mechanics};
use ols_core::budget::{Budget, Stop};
use ols_core::units::{Force, Length};

use crate::catenary::{Catenary, SagTensionError, SpanGeometry, SpanSolution};
use crate::change_of_state::LoadState;
use crate::section::SectionSolution;

fn mean_tension(solution: &SpanSolution) -> f64 {
    let s = solution.geometry.span.si();
    let c = solution.catenary.parameter().si();
    let xm = solution.low_point_x().si();
    let integral = solution.catenary.horizontal_tension.si()
        * (s / 2.0 + c / 2.0 * (s / c).sinh() * ((s - 2.0 * xm) / c).cosh());
    integral / solution.length().si()
}

fn cable_error(e: ols_cables::CableError) -> SagTensionError {
    SagTensionError::InvalidInput(e.to_string())
}

/// A tension section with a non-linear cable.
#[derive(Debug, Clone, PartialEq)]
pub struct NonlinearSection {
    /// Spans in line order.
    pub spans: Vec<SpanGeometry>,
    /// Cable mechanics.
    pub mechanics: Mechanics,
    /// Unstressed length at zero strain (curves' reference temperature).
    pub unstressed_length: Length,
}

/// Which permanent set governs the final condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinalCause {
    /// Creep over the creep period.
    Creep,
    /// The heaviest load case.
    Load,
}

impl NonlinearSection {
    /// Strings the cable: horizontal tension `horizontal_tension` under
    /// `state`, with the material in state `material` (usually `Initial`).
    pub fn from_known_state(
        spans: Vec<SpanGeometry>,
        mechanics: Mechanics,
        state: LoadState,
        horizontal_tension: Force,
        material: &MaterialState,
    ) -> Result<Self, SagTensionError> {
        if spans.is_empty() {
            return Err(SagTensionError::InvalidInput(
                "a section needs at least one span".into(),
            ));
        }
        let catenary = Catenary::new(horizontal_tension, state.load)?;
        let mut total = 0.0;
        for &g in &spans {
            let solution = catenary.solve(g)?;
            let strain = mechanics
                .strain(
                    Force::from_si(mean_tension(&solution)),
                    state.temperature,
                    material,
                )
                .map_err(cable_error)?;
            total += solution.length().si() / (1.0 + strain);
        }
        Ok(Self {
            spans,
            mechanics,
            unstressed_length: Length::from_si(total),
        })
    }

    /// Unstressed length the section would need at tension `h` (∞ if too
    /// slack to compute).
    fn required_length(
        &self,
        h: f64,
        state: LoadState,
        material: &MaterialState,
    ) -> Result<f64, SagTensionError> {
        let catenary = Catenary::new(Force::from_si(h), state.load)?;
        let mut total = 0.0;
        for &g in &self.spans {
            let solution = match catenary.solve(g) {
                Ok(s) => s,
                Err(SagTensionError::TooSlack { .. }) => return Ok(f64::INFINITY),
                Err(e) => return Err(e),
            };
            let strain = self
                .mechanics
                .strain(
                    Force::from_si(mean_tension(&solution)),
                    state.temperature,
                    material,
                )
                .map_err(cable_error)?;
            total += solution.length().si() / (1.0 + strain);
        }
        Ok(total)
    }

    /// Solves the section under `state` with the material in `material`.
    ///
    /// Expected cost: ~60 bisection steps × spans × (catenary + strain
    /// inversion ~60 halvings) — about 1 ms for 10 spans.
    pub fn solve(
        &self,
        state: LoadState,
        material: &MaterialState,
        budget: &Budget,
    ) -> Result<SectionSolution, SagTensionError> {
        let target = self.unstressed_length.si();
        let too_low = |h: f64| -> Result<bool, SagTensionError> {
            Ok(self.required_length(h, state, material)? > target)
        };
        let mut run = budget.start();
        let not_converged = |stop: Stop, lo: f64, hi: f64| SagTensionError::NotConverged {
            problem: "non-linear section",
            stop,
            detail: format!(
                "tension bracket [{lo:.6e}, {hi:.6e}] N at {:.1} °C, {:.3} N/m, {material:?}",
                state.temperature.celsius(),
                state.load.newtons_per_meter()
            ),
        };
        let (mut lo, mut hi) = (1_000.0, 1_000.0);
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
        let catenary = Catenary::new(Force::from_si(0.5 * (lo + hi)), state.load)?;
        Ok(SectionSolution {
            horizontal_tension: catenary.horizontal_tension,
            spans: self
                .spans
                .iter()
                .map(|&g| catenary.solve(g))
                .collect::<Result<_, _>>()?,
        })
    }

    /// Length-weighted mean tension of the section in a solution.
    fn section_mean_tension(solution: &SectionSolution) -> f64 {
        let (weighted, length) = solution.spans.iter().fold((0.0, 0.0), |(w, l), s| {
            let len = s.length().si();
            (w + mean_tension(s) * len, l + len)
        });
        weighted / length
    }
}

/// Final material state of a strung section: the longer of the permanent
/// sets after creep (`creep` state on the creep curve) and after the heaviest
/// load (`max_load` state on the initial curve).
pub fn final_condition(
    section: &NonlinearSection,
    creep: LoadState,
    max_load: LoadState,
    budget: &Budget,
) -> Result<(MaterialState, FinalCause), SagTensionError> {
    let m = &section.mechanics;
    let loaded = section.solve(max_load, &MaterialState::Initial, budget)?;
    let after_load = m
        .permanent_set(
            Force::from_si(NonlinearSection::section_mean_tension(&loaded)),
            max_load.temperature,
            &MaterialState::Initial,
        )
        .map_err(cable_error)?;
    let crept = section.solve(creep, &MaterialState::Creep, budget)?;
    let after_creep = m
        .permanent_set(
            Force::from_si(NonlinearSection::section_mean_tension(&crept)),
            creep.temperature,
            &MaterialState::Creep,
        )
        .map_err(cable_error)?;
    let longer = m
        .longer(after_creep.clone(), after_load)
        .map_err(cable_error)?;
    let cause = if longer == after_creep {
        FinalCause::Creep
    } else {
        FinalCause::Load
    };
    Ok((longer, cause))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catenary::default_solver_budget;
    use crate::change_of_state::{ElasticCable, InstalledSpan};
    use ols_cables::Library;
    use ols_core::units::{ForcePerLength, Temperature};
    use std::path::Path;

    const SYNTHETIC: &str = include_str!("../tests/data/synthetic-cable.olsw");

    fn cable() -> ols_cables::Cable {
        let mut library = Library::default();
        library
            .add_toml(Path::new("synthetic.toml"), SYNTHETIC)
            .unwrap();
        library.get("synthetic").unwrap().clone()
    }

    fn state(load: f64, celsius: f64) -> LoadState {
        LoadState {
            load: ForcePerLength::from_newtons_per_meter(load),
            temperature: Temperature::from_celsius(celsius),
        }
    }

    fn level(s: f64) -> Vec<SpanGeometry> {
        vec![SpanGeometry::level(Length::from_meters(s))]
    }

    #[test]
    fn linear_cable_matches_linear_solver() {
        let mut c = cable();
        c.outer.stress_strain = None;
        c.core.as_mut().unwrap().stress_strain = None;
        let m = Mechanics::from_cable(&c).unwrap();
        let budget = default_solver_budget();
        let nl = NonlinearSection::from_known_state(
            level(300.0),
            m,
            state(15.7, 15.0),
            Force::from_kilonewtons(25.0),
            &MaterialState::Initial,
        )
        .unwrap();
        let hot_nl = nl
            .solve(state(15.7, 75.0), &MaterialState::Initial, &budget)
            .unwrap();
        let lin = InstalledSpan::from_known_state(
            level(300.0)[0],
            ElasticCable {
                axial_stiffness: c.axial_stiffness(),
                thermal_expansion: c.thermal_expansion(),
            },
            state(15.7, 15.0),
            Force::from_kilonewtons(25.0),
        )
        .unwrap();
        let hot_lin = lin.solve(state(15.7, 75.0), &budget).unwrap();
        // Same physics; the linear solver uses exp(αΔt) and a stiffness-weighted
        // α while the component model adds strains per component: agreement to
        // second order in strain.
        let (a, b) = (
            hot_nl.horizontal_tension.newtons(),
            hot_lin.catenary.horizontal_tension.newtons(),
        );
        assert!(((a - b) / b).abs() < 2e-3, "{a} vs {b}");
    }

    #[test]
    fn path_independent() {
        let m = Mechanics::from_cable(&cable()).unwrap();
        let budget = default_solver_budget();
        let s = NonlinearSection::from_known_state(
            level(350.0),
            m.clone(),
            state(15.7, 15.0),
            Force::from_kilonewtons(25.0),
            &MaterialState::Initial,
        )
        .unwrap();
        let hot = s
            .solve(state(15.7, 80.0), &MaterialState::Initial, &budget)
            .unwrap();
        let again = NonlinearSection::from_known_state(
            level(350.0),
            m,
            state(15.7, 80.0),
            hot.horizontal_tension,
            &MaterialState::Initial,
        )
        .unwrap();
        let back = again
            .solve(state(15.7, 15.0), &MaterialState::Initial, &budget)
            .unwrap();
        assert!(
            (back.horizontal_tension.kilonewtons() - 25.0).abs() < 1e-7,
            "{}",
            back.horizontal_tension
        );
    }

    #[test]
    fn final_condition_lowers_tension_and_raises_sag() {
        let m = Mechanics::from_cable(&cable()).unwrap();
        let budget = default_solver_budget();
        let s = NonlinearSection::from_known_state(
            level(300.0),
            m,
            state(15.7, 15.0),
            Force::from_kilonewtons(25.0),
            &MaterialState::Initial,
        )
        .unwrap();
        let (fin, _cause) =
            final_condition(&s, state(15.7, 15.0), state(30.0, -10.0), &budget).unwrap();
        let initial = s
            .solve(state(15.7, 15.0), &MaterialState::Initial, &budget)
            .unwrap();
        let final_ = s.solve(state(15.7, 15.0), &fin, &budget).unwrap();
        assert!(final_.horizontal_tension < initial.horizontal_tension);
        assert!(final_.spans[0].sag() > initial.spans[0].sag());
    }
}
