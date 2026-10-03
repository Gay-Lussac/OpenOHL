// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Clearances of solved wires.
//!
//! v1: **vertical ground clearance** — for every span of a solved section,
//! the smallest vertical distance from the wire down to the ground surface
//! (TIN), wherever the wire is (blown-out wires are checked over the ground
//! they are blown over).
//!
//! Method: the solved wire is a polyline of closely spaced points on the
//! exact catenary; under each polyline piece the ground profile is taken at
//! every TIN edge crossing ([`Terrain::profile`]), where both the ground
//! (exactly) and the wire piece (to within its chord sag) are linear, so the
//! minimum of their difference is at one of those points. The wire's chord
//! sag over a piece is \\( L^2 / (8C) \\) for a piece of length L and
//! catenary constant C — about 3 mm for 6 m pieces at C = 1500 m — and the
//! wire lies below its chords, so clearances are high by at most that.
//!
//! v2 ([`checks`], [`rules`], DESIGN_GOALS §13): **vertical and radial**
//! clearances by rule (`.olsr`) to the ground, to points of chosen classes
//! (LiDAR), to wires of other lines and to other wires of the same line.

pub mod checks;
pub mod envelope;
pub mod geometry;
pub mod rules;

pub use checks::{Hit, PairMode, PairResult, PointCheck, Wire, WireResult, WireSet, wires_of};
pub use envelope::{Envelope, EnvelopeEntry};
pub use rules::{PairVoltage, Required, Requirement, Rule, RuleSet, Target};

use ols_line::jumper::JumperShape;
use ols_line::solve::SectionShape;
use ols_terrain::Terrain;

/// Lowest clearance point of a span.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lowest {
    /// Vertical clearance, m (negative: the wire is below the ground).
    pub clearance: f64,
    /// Wire point, m.
    pub wire: [f64; 3],
    /// Ground point below it, m.
    pub ground: [f64; 3],
}

/// Vertical ground clearance of one span.
#[derive(Debug, Clone, PartialEq)]
pub struct SpanClearance {
    /// Section id.
    pub section: String,
    /// Span index in the section.
    pub span: usize,
    /// Lowest clearance (`None` if no part of the span is over the terrain).
    pub lowest: Option<Lowest>,
    /// Fraction of the span's horizontal length over the terrain, 0–1.
    pub covered: f64,
}

/// Vertical ground clearance of every span of a solved section.
///
/// Expected cost: one TIN walk per wire piece (64 pieces per span element),
/// about 0.1 ms per span for a survey with a few thousand points.
pub fn ground_clearances(shape: &SectionShape, terrain: &Terrain) -> Vec<SpanClearance> {
    shape
        .spans
        .iter()
        .enumerate()
        .map(|(s, span)| {
            let (lowest, covered) = polyline_clearance(&span.points, terrain);
            SpanClearance {
                section: shape.section.clone(),
                span: s,
                lowest,
                covered,
            }
        })
        .collect()
}

/// Vertical ground clearance of a jumper, reported as span 0 of
/// `"jumper:<id>"`.
pub fn jumper_clearance(shape: &JumperShape, terrain: &Terrain) -> SpanClearance {
    let (lowest, covered) = polyline_clearance(&shape.points, terrain);
    SpanClearance {
        section: format!("jumper:{}", shape.jumper),
        span: 0,
        lowest,
        covered,
    }
}

/// Lowest vertical ground clearance under a wire polyline, and the fraction
/// of its horizontal length over the terrain.
pub fn polyline_clearance(points: &[[f64; 3]], terrain: &Terrain) -> (Option<Lowest>, f64) {
    let mut lowest: Option<Lowest> = None;
    let (mut total, mut covered) = (0.0, 0.0);
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        let horizontal = (b[0] - a[0]).hypot(b[1] - a[1]);
        total += horizontal;
        for run in terrain.profile([a[0], a[1]], [b[0], b[1]]) {
            if let (Some(first), Some(last)) = (run.first(), run.last()) {
                covered += (last.t - first.t) * horizontal;
            }
            for g in run {
                let z = a[2] + (b[2] - a[2]) * g.t;
                let clearance = z - g.point[2];
                if lowest.is_none_or(|l| clearance < l.clearance) {
                    lowest = Some(Lowest {
                        clearance,
                        wire: [g.point[0], g.point[1], z],
                        ground: g.point,
                    });
                }
            }
        }
    }
    (lowest, if total > 0.0 { covered / total } else { 0.0 })
}

/// Spans whose lowest clearance is below `required` (m), lowest first.
pub fn violations(clearances: &[SpanClearance], required: f64) -> Vec<&SpanClearance> {
    let mut found: Vec<&SpanClearance> = clearances
        .iter()
        .filter(|c| c.lowest.is_some_and(|l| l.clearance < required))
        .collect();
    found.sort_by(|a, b| {
        let key = |c: &SpanClearance| c.lowest.map_or(f64::INFINITY, |l| l.clearance);
        key(a).total_cmp(&key(b))
    });
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use ols_core::units::Temperature;
    use ols_line::Line;
    use ols_terrain::SurveyPoint;
    use ols_weather::WeatherCase;
    use std::path::Path;

    fn solved(temperature: f64) -> Vec<SectionShape> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let line =
            Line::load(&root.join("examples/synthetic-line/synthetic-line.ols.json")).unwrap();
        let library = ols_cables::Library::load(&root.join("libraries/cables")).unwrap();
        let case = WeatherCase::still("t", Temperature::from_celsius(temperature));
        ols_line::solve::solve_line(&line, &library, &case)
            .into_iter()
            .map(Result::unwrap)
            .collect()
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
        let terrain = flat(480.0);
        for shape in &shapes {
            for (c, span) in ground_clearances(shape, &terrain).iter().zip(&shape.spans) {
                let lowest = c.lowest.unwrap();
                // Over flat ground the lowest clearance is at the wire's low
                // point (within the polyline chord sag, mm).
                assert!(
                    (lowest.clearance - (span.low_point[2] - 480.0)).abs() < 5e-3,
                    "{c:?} vs {:?}",
                    span.low_point
                );
                assert!(lowest.clearance <= span.low_point[2] - 480.0 + 1e-9);
                assert!((c.covered - 1.0).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn heat_reduces_clearance_and_violations_are_sorted() {
        let terrain = flat(500.0);
        let all = |t: f64| -> Vec<SpanClearance> {
            solved(t)
                .iter()
                .flat_map(|s| ground_clearances(s, &terrain))
                .collect()
        };
        let (cold, hot) = (all(-20.0), all(80.0));
        for (c, h) in cold.iter().zip(&hot) {
            assert!(h.lowest.unwrap().clearance < c.lowest.unwrap().clearance);
        }
        let found = violations(&hot, 18.0);
        assert!(!found.is_empty());
        for w in found.windows(2) {
            assert!(w[0].lowest.unwrap().clearance <= w[1].lowest.unwrap().clearance);
        }
        assert!(violations(&hot, -100.0).is_empty());
    }

    #[test]
    fn example_survey_hill_is_the_critical_point() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let terrain =
            Terrain::load(&root.join("examples/synthetic-line/synthetic-survey.csv")).unwrap();
        assert_eq!(terrain.skipped_breaklines(), 0);
        for (temperature, range) in [(15.0, 6.0..7.0), (80.0, 4.0..5.5)] {
            let all: Vec<SpanClearance> = solved(temperature)
                .iter()
                .flat_map(|s| ground_clearances(s, &terrain))
                .collect();
            assert!(all.iter().all(|c| (c.covered - 1.0).abs() < 1e-9));
            let worst = violations(&all, f64::INFINITY)[0];
            let lowest = worst.lowest.unwrap();
            // The slack L2 wire over the 8 m hill under span T2–T3.
            assert_eq!((worst.section.as_str(), worst.span), ("S2", 1));
            assert!(
                range.contains(&lowest.clearance),
                "{temperature} °C: {lowest:?}"
            );
            assert!((lowest.wire[0] - 470.0).abs() < 30.0);
        }
    }

    #[test]
    fn jumper_clearance_over_flat_ground() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let line =
            Line::load(&root.join("examples/synthetic-line/synthetic-line.ols.json")).unwrap();
        let library = ols_cables::Library::load(&root.join("libraries/cables")).unwrap();
        let case = WeatherCase::still("t", Temperature::from_celsius(15.0));
        let shapes = solved(15.0);
        let jumper = ols_line::jumper::solve_jumpers(&line, &library, &case, &shapes)
            .remove(0)
            .unwrap();
        let c = jumper_clearance(&jumper, &flat(480.0));
        assert_eq!(c.section, "jumper:J-T3-branch");
        let lowest = c.lowest.unwrap();
        assert!((lowest.clearance - (jumper.low_point[2] - 480.0)).abs() < 5e-3);
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
        let shapes = solved(15.0);
        let c = ground_clearances(&shapes[0], &terrain);
        assert!(
            (c[0].covered - 160.0 / 310.0).abs() < 0.01,
            "{}",
            c[0].covered
        );
        assert!(c[0].lowest.is_some());
        assert!(c[2].lowest.is_none() && c[2].covered == 0.0);
    }
}
