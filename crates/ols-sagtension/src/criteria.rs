// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Tension criteria and the governing stringing tension.
//!
//! Engineers specify *limits* such as "≤ 20 % RTS horizontal tension at
//! 15 °C (vibration)" or "≤ 50 % RTS support tension under ice and wind".
//! The stringing tension is the highest tension at a reference state for
//! which every limit holds. Raising the stringing tension raises the tension
//! in every state, so each limit gives one maximum reference tension; the
//! governing limit is the one with the smallest maximum.
//!
//! Each evaluation is a full change of state, so this is a nested solver:
//! outer bisection on the reference tension, inner change of state. Both
//! levels have exit clauses.

use std::fmt;
use std::time::Duration;

use ols_core::budget::{Budget, Stop};
use ols_core::units::{Force, Length};

use crate::catenary::{SagTensionError, SpanGeometry, default_solver_budget};
use crate::change_of_state::{ElasticCable, LoadState};
use crate::section::{SectionSolution, TensionSection};

/// What a criterion limits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TensionLimit {
    /// Maximum support tension as a fraction of the rated tensile strength.
    SupportTensionRatio(f64),
    /// Maximum horizontal tension as a fraction of the rated tensile strength.
    HorizontalTensionRatio(f64),
    /// Maximum horizontal tension.
    HorizontalTension(Force),
    /// Maximum catenary constant \\( H / w \\) (vibration criteria).
    CatenaryConstant(Length),
}

impl fmt::Display for TensionLimit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SupportTensionRatio(r) => write!(f, "support tension ≤ {:.1} % RTS", r * 100.0),
            Self::HorizontalTensionRatio(r) => {
                write!(f, "horizontal tension ≤ {:.1} % RTS", r * 100.0)
            }
            Self::HorizontalTension(h) => {
                write!(f, "horizontal tension ≤ {:.2} kN", h.kilonewtons())
            }
            Self::CatenaryConstant(c) => write!(f, "H/w ≤ {:.1} m", c.meters()),
        }
    }
}

/// A limit applied in one load state.
#[derive(Debug, Clone, PartialEq)]
pub struct TensionCriterion {
    /// Label for reports, e.g. `"everyday (vibration)"`.
    pub label: String,
    /// Load and temperature where the limit applies.
    pub state: LoadState,
    /// The limit.
    pub limit: TensionLimit,
}

impl TensionCriterion {
    /// Value of the limited quantity and the limit, in the same unit.
    fn measure(&self, solution: &SectionSolution, rts: Force) -> (f64, f64) {
        let support = solution
            .spans
            .iter()
            .map(|s| s.tension_left().max(s.tension_right()))
            .fold(Force::ZERO, Force::max);
        let h = solution.horizontal_tension;
        match self.limit {
            TensionLimit::SupportTensionRatio(r) => (support.si(), r * rts.si()),
            TensionLimit::HorizontalTensionRatio(r) => (h.si(), r * rts.si()),
            TensionLimit::HorizontalTension(max) => (h.si(), max.si()),
            TensionLimit::CatenaryConstant(c) => ((h / self.state.load).si(), c.si()),
        }
    }
}

/// Utilisation of one criterion at the governing stringing tension.
#[derive(Debug, Clone, PartialEq)]
pub struct CriterionResult {
    /// Criterion label.
    pub label: String,
    /// Limited value / limit (1.0 for the governing criterion).
    pub utilisation: f64,
    /// Solution of the span in the criterion's state.
    pub solution: SectionSolution,
}

/// Result of the governing-tension search.
#[derive(Debug, Clone, PartialEq)]
pub struct Governing {
    /// Highest horizontal tension at the reference state meeting all limits.
    pub reference_tension: Force,
    /// Index of the governing criterion.
    pub governing: usize,
    /// Per-criterion results, in input order.
    pub criteria: Vec<CriterionResult>,
}

/// Default outer exit clauses: ~60 bisection steps expected; each runs a
/// change of state (~10 µs). 200 steps leave a wide margin (the time limit, 60 s, only guards
/// against a loop).
pub fn default_criteria_budget() -> Budget {
    Budget::new()
        .max_iterations(200)
        .time_limit(Duration::from_secs(60))
}

/// Finds the stringing tension at `reference` for a tension section (one or
/// more spans) that satisfies every criterion, with at least one exactly at
/// its limit. Support tension limits apply to the most loaded support of the
/// section.
pub fn governing_tension(
    spans: &[SpanGeometry],
    cable: ElasticCable,
    rated_tensile_strength: Force,
    reference: LoadState,
    criteria: &[TensionCriterion],
    budget: &Budget,
) -> Result<Governing, SagTensionError> {
    if criteria.is_empty() {
        return Err(SagTensionError::InvalidInput(
            "no tension criteria given".into(),
        ));
    }
    let inner = default_solver_budget();
    let solve_at = |h_ref: f64, state: LoadState| -> Result<SectionSolution, SagTensionError> {
        TensionSection::from_known_state(spans.to_vec(), cable, reference, Force::from_si(h_ref))?
            .solve(state, &inner)
    };
    // > 0 when the criterion is violated. Too slack to compute = far below limits.
    let excess = |h_ref: f64, criterion: &TensionCriterion| -> Result<f64, SagTensionError> {
        match solve_at(h_ref, criterion.state) {
            Ok(solution) => {
                let (value, limit) = criterion.measure(&solution, rated_tensile_strength);
                Ok(value / limit - 1.0)
            }
            Err(SagTensionError::TooSlack { .. }) => Ok(-1.0),
            Err(other) => Err(other),
        }
    };

    let mut run = budget.start();
    let mut best: Option<(f64, usize)> = None;
    for (index, criterion) in criteria.iter().enumerate() {
        let not_converged = |stop: Stop, lo: f64, hi: f64| SagTensionError::NotConverged {
            problem: "governing tension",
            stop,
            detail: format!(
                "criterion \"{}\" ({}): reference tension bracket [{lo:.6e}, {hi:.6e}] N",
                criterion.label, criterion.limit
            ),
        };
        let start = 0.1 * rated_tensile_strength.si();
        let (mut lo, mut hi) = (start, start);
        while excess(lo, criterion)? > 0.0 {
            run.step().map_err(|stop| not_converged(stop, lo, hi))?;
            lo /= 2.0;
        }
        while excess(hi, criterion)? <= 0.0 {
            run.step().map_err(|stop| not_converged(stop, lo, hi))?;
            hi *= 2.0;
        }
        while hi - lo > 1e-10 * hi {
            run.step().map_err(|stop| not_converged(stop, lo, hi))?;
            let mid = 0.5 * (lo + hi);
            if excess(mid, criterion)? > 0.0 {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        if best.is_none_or(|(h, _)| lo < h) {
            best = Some((lo, index));
        }
    }

    let (reference_tension, governing) = best.expect("criteria is not empty");
    let criteria = criteria
        .iter()
        .map(|criterion| {
            let solution = solve_at(reference_tension, criterion.state)?;
            let (value, limit) = criterion.measure(&solution, rated_tensile_strength);
            Ok(CriterionResult {
                label: criterion.label.clone(),
                utilisation: value / limit,
                solution,
            })
        })
        .collect::<Result<Vec<_>, SagTensionError>>()?;
    Ok(Governing {
        reference_tension: Force::from_si(reference_tension),
        governing,
        criteria,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ols_core::units::{ForcePerLength, Temperature};

    fn state(load: f64, celsius: f64) -> LoadState {
        LoadState {
            load: ForcePerLength::from_newtons_per_meter(load),
            temperature: Temperature::from_celsius(celsius),
        }
    }

    fn drake() -> ElasticCable {
        ElasticCable {
            axial_stiffness: Force::from_si(34.64e6),
            thermal_expansion: 18.9e-6,
        }
    }

    fn criteria(ice_limit: f64) -> Vec<TensionCriterion> {
        vec![
            TensionCriterion {
                label: "everyday".into(),
                state: state(15.97, 15.0),
                limit: TensionLimit::HorizontalTensionRatio(0.20),
            },
            TensionCriterion {
                label: "ice + wind".into(),
                state: state(32.0, -20.0),
                limit: TensionLimit::SupportTensionRatio(ice_limit),
            },
        ]
    }

    fn run(geometry: SpanGeometry, ice_limit: f64) -> Governing {
        governing_tension(
            &[geometry],
            drake(),
            Force::from_kilonewtons(138.8),
            state(15.97, 15.0),
            &criteria(ice_limit),
            &default_criteria_budget(),
        )
        .unwrap()
    }

    #[test]
    fn governing_criterion_is_at_its_limit_and_others_below() {
        for ice_limit in [0.30, 0.50] {
            let g = run(SpanGeometry::level(Length::from_meters(300.0)), ice_limit);
            let governing = &g.criteria[g.governing];
            assert!(
                (governing.utilisation - 1.0).abs() < 1e-8,
                "{}",
                governing.utilisation
            );
            for c in &g.criteria {
                assert!(c.utilisation <= 1.0 + 1e-8, "{} {}", c.label, c.utilisation);
            }
        }
    }

    #[test]
    fn tighter_ice_limit_changes_governing_case() {
        let geometry = SpanGeometry::level(Length::from_meters(300.0));
        // Generous ice limit: everyday 20 % RTS governs, reference tension = 0.2 RTS.
        let loose = run(geometry, 0.60);
        assert_eq!(loose.governing, 0);
        assert!((loose.reference_tension.kilonewtons() - 0.2 * 138.8).abs() < 1e-6);
        // Strict ice limit: the ice case governs and the stringing tension drops.
        let strict = run(geometry, 0.30);
        assert_eq!(strict.governing, 1);
        assert!(strict.reference_tension < loose.reference_tension);
    }

    #[test]
    fn section_limits_use_the_most_loaded_support() {
        let spans = [
            SpanGeometry::inclined(Length::from_meters(250.0), Length::from_meters(10.0)),
            SpanGeometry::inclined(Length::from_meters(420.0), Length::from_meters(60.0)),
        ];
        let g = governing_tension(
            &spans,
            drake(),
            Force::from_kilonewtons(138.8),
            state(15.97, 15.0),
            &criteria(0.30),
            &default_criteria_budget(),
        )
        .unwrap();
        let ice = &g.criteria[1].solution;
        let highest = ice.spans[1].tension_right();
        assert!((highest.kilonewtons() / (0.30 * 138.8) - g.criteria[1].utilisation).abs() < 1e-9);
    }

    #[test]
    fn exit_clause_names_the_criterion() {
        let error = governing_tension(
            &[SpanGeometry::level(Length::from_meters(300.0))],
            drake(),
            Force::from_kilonewtons(138.8),
            state(15.97, 15.0),
            &criteria(0.5),
            &Budget::new().max_iterations(4),
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("criterion \"everyday\""),
            "{error}"
        );
    }
}
