// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Clearances of solved wires (DESIGN_GOALS §13): **vertical and radial**
//! clearances by rule (`.olsr`, [`rules`]) to the ground, to points of
//! chosen classes (LiDAR), to wires of other lines and to other wires of
//! the same line ([`checks`]); the checks of one weather case as the
//! command line and the application run them ([`case`]); the lowest
//! clearances over several cases ([`envelope`]).
//!
//! Further checks of solved wires: galloping ellipses between the wires of
//! a span ([`galloping`]), blown-out wires against the right of way
//! ([`blowout`]), vegetation encroachment and fall-in ([`vegetation`]),
//! and as-built wires fitted to LiDAR points and compared with the model
//! ([`wirefit`]).
//!
//! Vertical ground clearance, for example, is found under every piece of
//! the solved wire polyline (closely spaced points on the exact catenary)
//! at every TIN edge crossing ([`Terrain::profile`]), where both the ground
//! (exactly) and the wire piece (to within its chord sag) are linear, so
//! the minimum of their difference is at one of those points. The wire's
//! chord sag over a piece is \\( L^2 / (8C) \\) for a piece of length L and
//! catenary constant C — about 3 mm for 6 m pieces at C = 1500 m — and the
//! wire lies below its chords, so clearances are high by at most that.
//!
//! [`Terrain::profile`]: ols_terrain::Terrain::profile

pub mod blowout;
pub mod case;
pub mod checks;
pub mod envelope;
pub mod galloping;
pub mod geometry;
pub mod phase_distance;
pub mod rules;
pub mod vegetation;
pub mod wirefit;
pub mod zone;

pub use case::{
    CaseTargets, Outcome, RuleCheck, Skipped, applies, check_case, lowest, voltages_of,
};
pub use checks::{Hit, PairMode, PairResult, PointCheck, Wire, WireResult, WireSet, wires_of};
pub use envelope::{Envelope, EnvelopeEntry};
pub use rules::Wires;
pub use rules::{
    CircuitFilter, PairVoltage, Required, Requirement, Rule, RuleSet, Target, WindSides, WindSteps,
};
pub use zone::{Measure, Zone, ZoneFinding, ZoneReport, ZoneRule};

#[cfg(test)]
mod tests {
    use super::*;
    use ols_core::units::Temperature;
    use ols_line::Line;
    use ols_line::solve::SectionShape;
    use ols_terrain::{SurveyPoint, Terrain};
    use ols_weather::WeatherCase;
    use std::path::Path;

    fn example() -> (Line, ols_cables::Library) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let line =
            Line::load(&root.join("examples/synthetic-line/synthetic-line.ols.json")).unwrap();
        let library = ols_cables::Library::load(&root.join("libraries/cables")).unwrap();
        (line, library)
    }

    fn solved(temperature: f64) -> Vec<SectionShape> {
        let (line, library) = example();
        let case = WeatherCase::still("t", Temperature::from_celsius(temperature));
        ols_line::solve::solve_line(&line, &library, &case)
            .into_iter()
            .map(Result::unwrap)
            .collect()
    }

    /// The spans' wires at `temperature` and their ground clearances
    /// (vertical, `required` m).
    fn ground(temperature: f64, terrain: &Terrain, required: f64) -> Vec<(Wire, WireResult)> {
        let (line, _) = example();
        let wires = wires_of(&line, &solved(temperature), &[]);
        let rule = &RuleSet::ground(required).rules[0];
        let results = checks::ground_check(rule, &wires, terrain);
        wires.into_iter().zip(results).collect()
    }

    /// The LeV distance between conductors on the synthetic line at
    /// 10 °C, still air: every pair between the same two structures is
    /// checked once, with its terms consistent (z from them); the
    /// circuits ticked limit the pairs.
    #[test]
    fn phase_distances_on_the_example() {
        use crate::phase_distance::{PhaseDistanceCode, PhaseDistanceSettings, check, conductors};
        let (line, library) = example();
        let code = PhaseDistanceCode::parse(include_str!(
            "../../../libraries/phase-distance/lev-ch.olspd"
        ))
        .unwrap();
        let shapes = solved(10.0);
        let cs = conductors(&line, &shapes, &library);
        let settings = PhaseDistanceSettings::default();
        let budget = ols_core::budget::Budget::new();
        let pairs = check(&code, &settings, &cs, &budget).unwrap();
        assert!(!pairs.is_empty());
        for p in &pairs {
            let w = &p.worst;
            let term = |i: usize| w.k[i] * (w.sag[i] + w.lk[i]).sqrt();
            assert!((w.required - (term(0).max(term(1)) + w.c)).abs() < 1e-9);
            assert!(w.distance > 0.0 && (0.0..=90.0).contains(&w.beta));
            assert!(w.k.iter().all(|k| (0.6..=0.95).contains(k)));
            // Between the same two structures.
            let ends = |i: usize| {
                let mut e: Vec<String> = cs[i]
                    .wire
                    .ends
                    .iter()
                    .map(|k| k.rsplit_once(':').unwrap().0.to_owned())
                    .collect();
                e.sort();
                e
            };
            assert_eq!(ends(p.a), ends(p.b));
        }
        // No pair twice.
        let mut keys: Vec<(usize, usize)> = pairs.iter().map(|p| (p.a, p.b)).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), pairs.len());
        // One circuit ticked: only its pairs (and its earth wires').
        let first = cs
            .iter()
            .find(|c| c.wire.potential.is_some())
            .unwrap()
            .wire
            .circuit
            .clone();
        let one = PhaseDistanceSettings {
            circuits: Some(std::iter::once(first.clone()).collect()),
            ..PhaseDistanceSettings::default()
        };
        let some = check(&code, &one, &cs, &budget).unwrap();
        assert!(some.len() <= pairs.len());
        assert!(
            some.iter()
                .all(|p| cs[p.a].wire.circuit == first && cs[p.b].wire.circuit == first)
        );
    }

    /// Lowest subconductor below the centre line, m (0 for one wire).
    fn bundle_low(w: &Wire) -> f64 {
        w.bundle.iter().map(|o| o[1]).fold(0.0, f64::min)
    }

    /// Flat ground at `z` over the whole example line.
    fn flat(z: f64) -> Terrain {
        let p = |x: f64, y: f64| SurveyPoint {
            x,
            y,
            z,
            code: String::new(),
            line: String::new(),
        };
        Terrain::from_points(
            &[
                p(-100.0, -200.0),
                p(1400.0, -200.0),
                p(1400.0, 700.0),
                p(-100.0, 700.0),
            ],
            None,
        )
        .unwrap()
    }

    #[test]
    fn flat_ground_clearance_is_the_lowest_wire_point() {
        let shapes = solved(15.0);
        let all = ground(15.0, &flat(480.0), 0.0);
        let spans = shapes.iter().flat_map(|s| &s.spans);
        assert_eq!(all.len(), spans.clone().count());
        for ((w, r), span) in all.iter().zip(spans) {
            let lowest = r.vertical.unwrap().distance;
            let expected = span.low_point[2] + bundle_low(w) - 480.0;
            // Over flat ground the lowest clearance is at the wire's low
            // point (within the polyline chord sag, mm).
            assert!(
                (lowest - expected).abs() < 5e-3,
                "{w:?}: {lowest} vs {expected}"
            );
            assert!(lowest <= expected + 1e-9);
        }
    }

    /// On ground sloping across the line, a band beside the wire finds
    /// the higher ground on the uphill side: the clearance drops by the
    /// slope times the band's width across.
    #[test]
    fn ground_within_a_band_beside_the_wire() {
        let (line, _) = example();
        let wires = wires_of(&line, &solved(15.0), &[]);
        let p = |x: f64, y: f64| SurveyPoint {
            x,
            y,
            z: 480.0 + 0.05 * y,
            code: String::new(),
            line: String::new(),
        };
        let terrain = Terrain::from_points(
            &[
                p(-100.0, -200.0),
                p(1400.0, -200.0),
                p(1400.0, 700.0),
                p(-100.0, 700.0),
            ],
            None,
        )
        .unwrap();
        let mut rule = RuleSet::ground(0.0).rules[0].clone();
        assert_eq!(rule.vertical_band, 0.0);
        let low = |rule: &Rule| {
            checks::ground_check(rule, &wires[..1], &terrain)[0]
                .vertical
                .unwrap()
                .distance
        };
        let straight = low(&rule);
        rule.vertical_band = 2.0;
        let banded = low(&rule);
        let w = &wires[0].points;
        let (a, b) = (w[0], w[w.len() - 1]);
        let across_y = ((b[0] - a[0]) / (b[0] - a[0]).hypot(b[1] - a[1])).abs();
        let drop = straight - banded;
        assert!((drop - 0.05 * 2.0 * across_y).abs() < 2e-3, "{drop}");
        // Slices along the line: never more than straight under the wire.
        rule.vertical_band = 0.0;
        rule.vertical_depth = 1.0;
        assert!(low(&rule) <= straight);
    }

    #[test]
    fn heat_reduces_clearance() {
        let terrain = flat(500.0);
        let (cold, hot) = (ground(-20.0, &terrain, 18.0), ground(80.0, &terrain, 18.0));
        for ((_, c), (_, h)) in cold.iter().zip(&hot) {
            assert!(h.vertical.unwrap().distance < c.vertical.unwrap().distance);
        }
        assert!(hot.iter().any(|(_, r)| r.violates()));
        assert!(
            !ground(80.0, &terrain, -100.0)
                .iter()
                .any(|(_, r)| r.violates())
        );
    }

    #[test]
    fn example_survey_hill_is_the_critical_point() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let terrain =
            Terrain::load(&root.join("examples/synthetic-line/synthetic-survey.csv")).unwrap();
        assert_eq!(terrain.skipped_breaklines(), 0);
        for (temperature, range) in [(15.0, 6.0..7.0), (80.0, 4.0..5.5)] {
            let all = ground(temperature, &terrain, 0.0);
            assert!(all.iter().all(|(_, r)| r.vertical.is_some()));
            let (worst, r) = all
                .iter()
                .min_by(|a, b| {
                    let d = |r: &WireResult| r.vertical.unwrap().distance;
                    d(&a.1).total_cmp(&d(&b.1))
                })
                .unwrap();
            let lowest = r.vertical.unwrap();
            // The slack L2 wire over the 8 m hill under span T2–T3.
            assert_eq!((worst.section.as_str(), worst.span), ("S2", 1));
            assert!(
                range.contains(&lowest.distance),
                "{temperature} °C: {lowest:?}"
            );
            assert!((lowest.wire[0] - 470.0).abs() < 30.0);
        }
    }

    #[test]
    fn jumper_clearance_over_flat_ground() {
        let (line, library) = example();
        let case = WeatherCase::still("t", Temperature::from_celsius(15.0));
        let shapes = solved(15.0);
        let jumpers: Vec<_> = ols_line::jumper::solve_jumpers(&line, &library, &case, &shapes)
            .into_iter()
            .map(Result::unwrap)
            .collect();
        let wires = wires_of(&line, &[], &jumpers);
        assert_eq!(wires[0].section, "jumper:J-T3-branch");
        let rule = &RuleSet::ground(0.0).rules[0];
        let r = &checks::ground_check(rule, &wires[..1], &flat(480.0))[0];
        let expected = jumpers[0].low_point[2] + bundle_low(&wires[0]) - 480.0;
        assert!((r.vertical.unwrap().distance - expected).abs() < 5e-3);
    }

    #[test]
    fn spans_outside_the_survey_have_no_clearance() {
        let p = |x: f64, y: f64| SurveyPoint {
            x,
            y,
            z: 480.0,
            code: String::new(),
            line: String::new(),
        };
        // Survey covers x < 160 only: half of the first span of S1 and S2.
        let terrain = Terrain::from_points(
            &[
                p(-100.0, -50.0),
                p(160.0, -50.0),
                p(160.0, 50.0),
                p(-100.0, 50.0),
            ],
            None,
        )
        .unwrap();
        let all = ground(15.0, &terrain, 0.0);
        assert_eq!((all[0].0.section.as_str(), all[0].0.span), ("S1", 0));
        assert!(all[0].1.vertical.is_some());
        assert_eq!(all[2].0.span, 2);
        assert!(all[2].1.vertical.is_none());
    }
}
