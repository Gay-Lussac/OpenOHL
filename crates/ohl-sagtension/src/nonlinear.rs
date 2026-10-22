// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

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
//! (using the mean tension per span is the usual sag-tension practice).
//!
//! The cable's own weight is conserved: it is per metre of unstressed cable
//! at the known state's temperature, the length \\( L_0 (1 + \varepsilon_w) \\)
//! with \\( \varepsilon_w = \varepsilon(0, t_{known}, s_{known}) \\)
//! ([`NonlinearSection::weight_strain`]), so span \\( i \\) carries
//! \\( w (1 + \varepsilon_w)/(1 + \varepsilon_i) \\) per metre of its
//! stretched length — the additive form's equivalent of
//! \\( w e^{-\alpha\Delta t} \\). Thermal strain, creep and permanent set
//! lengthen the cable without adding weight. The
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

use ohl_cables::mechanics::{MaterialState, Mechanics};
use ohl_core::budget::{Budget, Stop};
use ohl_core::units::{Force, Length};

use crate::catenary::{SagTensionError, SpanGeometry, SpanSolution};
use crate::change_of_state::{LoadState, span_at};
use crate::section::SectionSolution;

fn mean_tension(solution: &SpanSolution) -> f64 {
    solution.tension_integral() / solution.length().si()
}

fn cable_error(e: ohl_cables::CableError) -> SagTensionError {
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
    /// The strain, from the zero-strain length, of the length that carries
    /// the cable's own weight per metre (unstressed, at the known state's
    /// temperature): span \\( i \\) carries \\( w (1 + \varepsilon_w) /
    /// (1 + \varepsilon_i) \\) per metre. Lengthening the cable without
    /// adding weight (a permanent stretch) lowers it: [`Self::lengthen`].
    pub weight_strain: f64,
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
        state.check()?;
        let weight_strain = mechanics
            .strain(Force::ZERO, state.temperature, material)
            .map_err(cable_error)?;
        let mut section = Self {
            spans,
            mechanics,
            unstressed_length: Length::ZERO,
            weight_strain,
        };
        section.unstressed_length = Length::from_si(
            section
                .at(horizontal_tension.si(), &state, material)?
                .ok_or(SagTensionError::TooSlack {
                    ratio: f64::INFINITY,
                })?
                .1,
        );
        Ok(section)
    }

    /// Lengthens the cable by a permanent `stretch` (unit strain) that adds
    /// no weight: its unstressed length grows, its weight per metre of it
    /// falls.
    pub fn lengthen(&mut self, stretch: f64) {
        self.unstressed_length = Length::from_si(self.unstressed_length.si() * (1.0 + stretch));
        self.weight_strain = (1.0 + self.weight_strain) / (1.0 + stretch) - 1.0;
    }

    /// The spans at tension `h` in `state` (each with its own weight per
    /// metre) and the unstressed length they need; `None` if too slack to
    /// compute.
    fn at(
        &self,
        h: f64,
        state: &LoadState,
        material: &MaterialState,
    ) -> Result<Option<(Vec<SpanSolution>, f64)>, SagTensionError> {
        let mut total = 0.0;
        let mut solved = Vec::with_capacity(self.spans.len());
        for &g in &self.spans {
            let (solution, stretch) = match span_at(h, state, g, |s| {
                let strain = self
                    .mechanics
                    .strain(Force::from_si(mean_tension(s)), state.temperature, material)
                    .map_err(cable_error)?;
                Ok((1.0 + strain) / (1.0 + self.weight_strain))
            }) {
                Ok(x) => x,
                Err(SagTensionError::TooSlack { .. }) => return Ok(None),
                // A trial far too slack (the catenary parameter below half
                // the span: a sag over a quarter of it) whose stretch does
                // not settle (strains far beyond any curve) is too slack,
                // not an error: the bracket moves on.
                Err(SagTensionError::InvalidInput(_))
                    if 2.0 * h < state.load.si() * g.span.si() =>
                {
                    return Ok(None);
                }
                Err(e) => return Err(e),
            };
            total += solution.length().si() / stretch / (1.0 + self.weight_strain);
            solved.push(solution);
        }
        Ok(Some((solved, total)))
    }

    /// Unstressed length the section would need at tension `h` (∞ if too
    /// slack to compute).
    fn required_length(
        &self,
        h: f64,
        state: LoadState,
        material: &MaterialState,
    ) -> Result<f64, SagTensionError> {
        Ok(self
            .at(h, &state, material)?
            .map_or(f64::INFINITY, |(_, total)| total))
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
        state.check()?;
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
        let h = 0.5 * (lo + hi);
        let (spans, _) = self
            .at(h, &state, material)?
            .ok_or(SagTensionError::TooSlack {
                ratio: f64::INFINITY,
            })?;
        Ok(SectionSolution {
            horizontal_tension: Force::from_si(h),
            spans,
        })
    }

    /// Each span's unstressed length (zero strain, the curves' reference
    /// temperature, m) in a solution of the section in `state` with the
    /// material in `material`: its length over \\( 1 + \varepsilon_i \\). They
    /// add up to the section's unstressed length.
    pub fn span_unstressed_lengths(
        &self,
        solution: &SectionSolution,
        state: &LoadState,
        material: &MaterialState,
    ) -> Result<Vec<f64>, SagTensionError> {
        solution
            .spans
            .iter()
            .map(|s| {
                let strain = self
                    .mechanics
                    .strain(Force::from_si(mean_tension(s)), state.temperature, material)
                    .map_err(cable_error)?;
                Ok(s.length().si() / (1.0 + strain))
            })
            .collect()
    }

    /// Warnings for a solution in `state`: the components strained beyond
    /// their curves' validity (`max_strain`), at the most tensioned
    /// support of each span (the strain the polynomial is evaluated at is
    /// the mean tension's, a little lower: warning from the support's is on
    /// the safe side). One line per span and component, empty within range.
    pub fn range_warnings(
        &self,
        solution: &SectionSolution,
        state: &LoadState,
        material: &MaterialState,
    ) -> Result<Vec<String>, SagTensionError> {
        let mut out = Vec::new();
        for (k, s) in solution.spans.iter().enumerate() {
            let t = s.tension_left().max(s.tension_right());
            let strain = self
                .mechanics
                .strain(t, state.temperature, material)
                .map_err(cable_error)?;
            for r in self.mechanics.beyond_range(strain, state.temperature) {
                out.push(format!("span {}: {r}", k + 1));
            }
        }
        Ok(out)
    }

    /// Length-weighted mean tension of the section in a solution, N.
    pub fn section_mean_tension(solution: &SectionSolution) -> f64 {
        let (weighted, length) = solution.spans.iter().fold((0.0, 0.0), |(w, l), s| {
            let len = s.length().si();
            (w + mean_tension(s) * len, l + len)
        });
        weighted / length
    }
}

/// Both final material states of a strung section, and the one that
/// leaves the cable longer at the creep (working) tension.
#[derive(Debug, Clone, PartialEq)]
pub struct FinalStates {
    /// The permanent set after creep (the creep curve at the creep case).
    pub after_creep: MaterialState,
    /// The permanent set after the heaviest load (the initial curve at the
    /// load case).
    pub after_load: MaterialState,
    /// Which leaves the cable longer at the creep case's tension (one
    /// state for the whole section, as [`final_condition`] gives it).
    pub longer: FinalCause,
}

impl FinalStates {
    /// The material state of a cause.
    pub fn state(&self, cause: FinalCause) -> &MaterialState {
        match cause {
            FinalCause::Creep => &self.after_creep,
            FinalCause::Load => &self.after_load,
        }
    }
}

/// The final state of one weather case: both solutions and the governing
/// one ([`governing_final`]).
#[derive(Debug, Clone, PartialEq)]
pub struct FinalCase {
    /// The section after creep.
    pub after_creep: SectionSolution,
    /// The section after the heaviest load.
    pub after_load: SectionSolution,
    /// The governing one: the longer cable in this case (the lower
    /// horizontal tension, so the larger sags).
    pub governing: FinalCause,
}

impl FinalCase {
    /// The governing solution.
    pub fn solution(&self) -> &SectionSolution {
        match self.governing {
            FinalCause::Creep => &self.after_creep,
            FinalCause::Load => &self.after_load,
        }
    }
}

/// Both final states of a strung section: the permanent sets after creep
/// (`creep` state on the creep curve) and after the heaviest load
/// (`max_load` state on the initial curve), and the longer of the two at
/// the creep tension.
pub fn final_states(
    section: &NonlinearSection,
    creep: LoadState,
    max_load: LoadState,
    budget: &Budget,
) -> Result<FinalStates, SagTensionError> {
    let (after_creep, after_load, longer) = sets(section, creep, max_load, budget)?;
    Ok(FinalStates {
        after_creep,
        after_load,
        longer,
    })
}

/// The final state of the section in weather case `state`
/// (2026-10-22): both final states solved, and the one that leaves the
/// cable longer **in this case** governs — the lower horizontal tension,
/// so the larger sags. Which one that is can change with the case (with
/// some curves creep governs when hot, load when cold). A tie (within
/// \\( 10^{-9} \\) of the tension: both on the initial curve, at the
/// heaviest case) is given to the load.
pub fn governing_final(
    section: &NonlinearSection,
    states: &FinalStates,
    state: LoadState,
    budget: &Budget,
) -> Result<FinalCase, SagTensionError> {
    let after_creep = section.solve(state, &states.after_creep, budget)?;
    let after_load = section.solve(state, &states.after_load, budget)?;
    let (hc, hl) = (
        after_creep.horizontal_tension.si(),
        after_load.horizontal_tension.si(),
    );
    let governing = if hc < hl * (1.0 - 1e-9) {
        FinalCause::Creep
    } else {
        FinalCause::Load
    };
    Ok(FinalCase {
        after_creep,
        after_load,
        governing,
    })
}

/// Final material state of a strung section: the longer of the permanent
/// sets after creep (`creep` state on the creep curve) and after the heaviest
/// load (`max_load` state on the initial curve), compared at the creep
/// tension (one state for every case; [`governing_final`] chooses per
/// case).
pub fn final_condition(
    section: &NonlinearSection,
    creep: LoadState,
    max_load: LoadState,
    budget: &Budget,
) -> Result<(MaterialState, FinalCause), SagTensionError> {
    let (after_creep, after_load, cause) = sets(section, creep, max_load, budget)?;
    Ok(match cause {
        FinalCause::Creep => (after_creep, cause),
        FinalCause::Load => (after_load, cause),
    })
}

/// The permanent sets after creep and after load, and which is longer at
/// the creep tension.
fn sets(
    section: &NonlinearSection,
    creep: LoadState,
    max_load: LoadState,
    budget: &Budget,
) -> Result<(MaterialState, MaterialState, FinalCause), SagTensionError> {
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
    // Compared at the creep (everyday) tension.
    let working = Force::from_si(NonlinearSection::section_mean_tension(&crept));
    let after_creep = m
        .permanent_set(working, creep.temperature, &MaterialState::Creep)
        .map_err(cable_error)?;
    let longer = m
        .longer(after_creep.clone(), after_load.clone(), working)
        .map_err(cable_error)?;
    let cause = if longer == after_creep {
        FinalCause::Creep
    } else {
        FinalCause::Load
    };
    Ok((after_creep, after_load, cause))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catenary::default_solver_budget;
    use crate::change_of_state::{ElasticCable, InstalledSpan};
    use ohl_cables::Library;
    use ohl_core::units::{ForcePerLength, Temperature};
    use std::path::Path;

    const SYNTHETIC: &str = include_str!("../tests/data/synthetic-cable.ohlw");

    fn cable() -> ohl_cables::Cable {
        let mut library = Library::default();
        library
            .add_toml(Path::new("synthetic.toml"), SYNTHETIC)
            .unwrap();
        library.get("synthetic").unwrap().clone()
    }

    /// The synthetic cable's own weight 15.7 N/m under `load` (the rest
    /// ice).
    fn state(load: f64, celsius: f64) -> LoadState {
        state_w(15.7, load, celsius)
    }

    fn state_w(weight: f64, load: f64, celsius: f64) -> LoadState {
        LoadState::loaded(
            ForcePerLength::from_newtons_per_meter(weight),
            ForcePerLength::from_newtons_per_meter(load - weight),
            ForcePerLength::ZERO,
            Temperature::from_celsius(celsius),
        )
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
        // Defined again at 80 °C: the same cable, its weight per metre of
        // unstressed cable at 80 °C.
        let zero = |t: f64| {
            m.strain(
                Force::ZERO,
                Temperature::from_celsius(t),
                &MaterialState::Initial,
            )
            .unwrap()
        };
        let w80 = 15.7 * (1.0 + zero(15.0)) / (1.0 + zero(80.0));
        let again = NonlinearSection::from_known_state(
            level(350.0),
            m,
            state_w(w80, w80, 80.0),
            hot.horizontal_tension,
            &MaterialState::Initial,
        )
        .unwrap();
        assert!(
            (again.unstressed_length.si() / s.unstressed_length.si() - 1.0).abs() < 1e-12,
            "{} vs {}",
            again.unstressed_length,
            s.unstressed_length
        );
        let back = again
            .solve(state_w(w80, w80, 15.0), &MaterialState::Initial, &budget)
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
