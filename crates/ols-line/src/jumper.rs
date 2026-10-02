// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Jumpers (DESIGN_GOALS §12): slack cables connecting two attachment
//! points — the ends of two sections at a dead-end structure, or a tap from
//! a branch to a through wire at a Y pylon.
//!
//! - **Ends**: the solved clamp position of each end when the attachment is
//!   a point of a solved section (strain assemblies settle with their
//!   conductor, so the jumper follows them), otherwise the attachment's
//!   wire point.
//! - **Cable**: one exact elastic catenary element carrying the weather
//!   load (weight, ice, wind horizontal and perpendicular to the jumper's
//!   chord). Its unstressed length is the given cut length, or the length
//!   that gives the given sag in still air, scaled by
//!   \\( e^{\alpha \Delta t} \\) from its temperature.
//! - **Support string** (optional): a tension-only string from a point on
//!   the structure holding the jumper at a fraction of its length; the
//!   jumper is then two elements and the clamp is found by the Newton
//!   solver of `ols-wirefe`.
//!
//! A jumper's tension (a few hundred N) is negligible for the sections it
//! connects, so sections are solved first and jumpers between their
//! solved ends.

use nalgebra::Vector3;
use ols_cables::Cable;
use ols_core::budget::Budget;
use ols_weather::WeatherCase;
use ols_wirefe::catenary_element::default_element_budget;
use ols_wirefe::{CableElement, CableProperties, Model, Node, TrussElement, default_fe_budget};

use crate::solve::{DEFAULT_INSULATOR_STIFFNESS, SectionShape, p, span_load, v};
use crate::{Jumper, Line, LineError, Point, SectionPoint};

/// Solved geometry of a jumper.
#[derive(Debug, Clone, PartialEq)]
pub struct JumperShape {
    /// Jumper id.
    pub jumper: String,
    /// Points along the jumper, `from` to `to`.
    pub points: Vec<Point>,
    /// Unstressed length used (at the weather case temperature), m.
    pub unstressed_length: f64,
    /// Largest vertical distance below the end-to-end chord, m.
    pub sag: f64,
    /// Lowest point.
    pub low_point: Point,
    /// Largest tension, N.
    pub max_tension: f64,
    /// Support string: its top and the clamp on the jumper, and its force, N.
    pub support: Option<(Point, Point, f64)>,
}

/// Where a jumper end is: the solved clamp of the first section through
/// that attachment, otherwise the attachment's wire point.
fn end_position(
    line: &Line,
    shapes: &[SectionShape],
    point: &SectionPoint,
) -> Result<Vector3<f64>, LineError> {
    for shape in shapes {
        let Some(section) = line.sections.iter().find(|s| s.id == shape.section) else {
            continue;
        };
        if let Some(k) = section.points.iter().position(|q| q == point)
            && let Some(clamp) = shape.clamps.get(k)
        {
            return Ok(v(*clamp));
        }
    }
    line.attachment(point)
        .map(|a| v(a.wire_point))
        .ok_or_else(|| LineError::Invalid(format!("no attachment {point:?}")))
}

/// Largest vertical distance of `points` below the chord from `a` to `b`
/// (chord height interpolated by horizontal position along it).
fn sag_below_chord(points: &[Vector3<f64>], a: Vector3<f64>, b: Vector3<f64>) -> f64 {
    let chord = b - a;
    let h2 = chord.xy().norm_squared();
    points
        .iter()
        .map(|q| {
            let t = if h2 > 0.0 {
                ((q - a).xy().dot(&chord.xy()) / h2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            a.z + chord.z * t - q.z
        })
        .fold(0.0, f64::max)
}

/// Shape of one element in still air (absolute points).
fn element_shape(
    props: &CableProperties,
    a: Vector3<f64>,
    d: Vector3<f64>,
) -> Result<(Vec<Vector3<f64>>, f64), String> {
    let state = props
        .solve(d, None, &default_element_budget())
        .map_err(|e| e.to_string())?;
    let points = props
        .shape(d, &state, 64)
        .into_iter()
        .map(|q| a + q)
        .collect();
    Ok((points, state.tension_i.max(state.tension_j)))
}

/// Unstressed length giving `sag` in still air between `a` and `b`, by
/// bisection on the (increasing) sag–length relation.
///
/// Expected cost: ~50 element solves (~1 µs each in release); exit clauses:
/// the budget's iteration limit and a 20× chord upper bound.
fn length_for_sag(
    props: CableProperties,
    a: Vector3<f64>,
    b: Vector3<f64>,
    sag: f64,
    budget: &Budget,
) -> Result<f64, String> {
    let d = b - a;
    let chord = d.norm();
    let sag_of = |l0: f64| -> Result<f64, String> {
        let props = CableProperties {
            unstressed_length: l0,
            ..props
        };
        Ok(sag_below_chord(&element_shape(&props, a, d)?.0, a, b))
    };
    let mut low = chord * (1.0 + 1e-5);
    if sag_of(low)? >= sag {
        return Err(format!(
            "a sag of {sag} m is less than the taut jumper's ({:.3} m)",
            sag_of(low)?
        ));
    }
    let mut high = chord * 1.1;
    while sag_of(high)? < sag {
        low = high;
        high *= 1.5;
        if high > 20.0 * chord {
            return Err(format!(
                "a sag of {sag} m needs more than 20× the chord length"
            ));
        }
    }
    let mut run = budget.start();
    while high - low > 1e-7 * chord {
        run.step()
            .map_err(|stop| format!("sag to length: {stop}"))?;
        let mid = 0.5 * (low + high);
        if sag_of(mid)? < sag {
            low = mid;
        } else {
            high = mid;
        }
    }
    Ok(0.5 * (low + high))
}

/// Solves one jumper in a weather case, between the solved section ends.
pub fn solve_jumper(
    line: &Line,
    jumper: &Jumper,
    shapes: &[SectionShape],
    cable: &Cable,
    case: &WeatherCase,
) -> Result<JumperShape, LineError> {
    let invalid = |m: String| LineError::Invalid(format!("jumper {}: {m}", jumper.id));
    let a = end_position(line, shapes, &jumper.from).map_err(|e| invalid(e.to_string()))?;
    let b = end_position(line, shapes, &jumper.to).map_err(|e| invalid(e.to_string()))?;
    let d = b - a;
    if d.xy().norm() < 0.1 {
        return Err(invalid(
            "ends less than 0.1 m apart horizontally (a vertical jumper is not supported yet)"
                .into(),
        ));
    }
    let ea = cable.axial_stiffness().si();
    let still = CableProperties {
        unstressed_length: 1.0,
        axial_stiffness: ea,
        load: Vector3::new(0.0, 0.0, -cable.weight().si()),
    };
    let reference = match (jumper.length, jumper.sag) {
        (Some(length), _) => length,
        (None, Some(sag)) => {
            let budget = Budget::new().max_iterations(200);
            length_for_sag(still, a, b, sag, &budget).map_err(invalid)?
        }
        (None, None) => return Err(invalid("needs a length or a sag".into())),
    };
    let l0 = reference
        * (cable.thermal_expansion() * (case.temperature.celsius() - jumper.temperature)).exp();
    let load = span_load(case, cable, d);
    let props = CableProperties {
        unstressed_length: l0,
        axial_stiffness: ea,
        load,
    };

    let (points, max_tension, support) = match &jumper.support {
        None => {
            let (points, t) = element_shape(&props, a, d).map_err(invalid)?;
            (points, t, None)
        }
        Some(s) => {
            let top = v(s.point);
            let mut model = Model::default();
            model.nodes.push(Node::fixed(a));
            model.nodes.push(Node::fixed(b));
            model.nodes.push(Node::fixed(top));
            // Start the clamp below the string top.
            model
                .nodes
                .push(Node::free(top - Vector3::new(0.0, 0.0, s.length)));
            for (i, j, length) in [(0, 3, l0 * s.fraction), (3, 1, l0 * (1.0 - s.fraction))] {
                model.cables.push(CableElement {
                    i,
                    j,
                    properties: CableProperties {
                        unstressed_length: length,
                        ..props
                    },
                });
            }
            model.trusses.push(TrussElement {
                i: 2,
                j: 3,
                unstressed_length: s.length,
                axial_stiffness: DEFAULT_INSULATOR_STIFFNESS,
                tension_only: true,
                weight: s.weight,
            });
            let solution = model
                .solve(&default_fe_budget())
                .map_err(|e| invalid(e.to_string()))?;
            let pos = &solution.positions;
            let mut points: Vec<Vector3<f64>> = Vec::new();
            let mut max_tension: f64 = 0.0;
            for (e, element) in model.cables.iter().enumerate() {
                let state = &solution.cables[e];
                max_tension = max_tension.max(state.tension_i).max(state.tension_j);
                let d = pos[element.j] - pos[element.i];
                let skip = usize::from(!points.is_empty());
                points.extend(
                    element
                        .properties
                        .shape(d, state, 64)
                        .into_iter()
                        .skip(skip)
                        .map(|q| pos[element.i] + q),
                );
            }
            (
                points,
                max_tension,
                Some((s.point, p(pos[3]), solution.truss_forces[0])),
            )
        }
    };
    let low = *points
        .iter()
        .min_by(|x, y| x.z.total_cmp(&y.z))
        .expect("points");
    Ok(JumperShape {
        jumper: jumper.id.clone(),
        sag: sag_below_chord(&points, a, b),
        points: points.iter().map(|q| p(*q)).collect(),
        unstressed_length: l0,
        low_point: p(low),
        max_tension,
        support,
    })
}

/// Solves every jumper of the line between the solved sections `shapes`.
pub fn solve_jumpers(
    line: &Line,
    library: &ols_cables::Library,
    case: &WeatherCase,
    shapes: &[SectionShape],
) -> Vec<Result<JumperShape, LineError>> {
    line.jumpers
        .iter()
        .map(|jumper| {
            let cable = library.get(&jumper.cable).ok_or_else(|| {
                LineError::Invalid(format!(
                    "jumper {}: cable \"{}\" not in the libraries",
                    jumper.id, jumper.cable
                ))
            })?;
            solve_jumper(line, jumper, shapes, cable, case)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::JumperSupport;
    use crate::solve::solve_line;
    use ols_core::units::{Stress, Temperature};
    use std::path::Path;

    fn library() -> ols_cables::Library {
        ols_cables::Library::load(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
        )
        .unwrap()
    }

    fn still(t: f64) -> WeatherCase {
        WeatherCase::still("t", Temperature::from_celsius(t))
    }

    fn solve_all(line: &Line, case: &WeatherCase) -> (Vec<SectionShape>, Vec<JumperShape>) {
        let lib = library();
        let shapes: Vec<_> = solve_line(line, &lib, case)
            .into_iter()
            .map(Result::unwrap)
            .collect();
        let jumpers = solve_jumpers(line, &lib, case, &shapes)
            .into_iter()
            .map(Result::unwrap)
            .collect();
        (shapes, jumpers)
    }

    #[test]
    fn example_tap_jumper_has_its_sag_and_follows_the_clamps() {
        // The example joins the branch (S3, strain at T3) to the main L1
        // wire (S1, suspension clamp at T3) with a jumper of 1.5 m sag.
        let line = crate::tests::example();
        let (shapes, jumpers) = solve_all(&line, &still(15.0));
        let j = &jumpers[0];
        assert!((j.sag - 1.5).abs() < 1e-5, "{}", j.sag);
        // Ends at the solved clamps: S3's first point, S1's T3 point.
        let (first, last) = (v(j.points[0]), v(*j.points.last().unwrap()));
        assert!((first - v(shapes[2].clamps[0])).norm() < 1e-6);
        assert!((last - v(shapes[0].clamps[2])).norm() < 1e-6);
        // Heat lengthens the jumper: more sag.
        let (_, hot) = solve_all(&line, &still(80.0));
        assert!(hot[0].sag > j.sag + 0.01, "{} vs {}", hot[0].sag, j.sag);
        // Wind blows it sideways.
        let mut windy = still(15.0);
        windy.wind_pressure = Stress::from_pascals(500.0);
        let (_, blown) = solve_all(&line, &windy);
        let shift = (v(blown[0].low_point) - v(j.low_point)).xy().norm();
        assert!(shift > 0.2, "{shift}");
    }

    #[test]
    fn given_length_reproduces_the_sag_length() {
        let mut line = crate::tests::example();
        let (_, jumpers) = solve_all(&line, &still(15.0));
        // Cutting the jumper to the length found from the sag gives that sag.
        line.jumpers[0].sag = None;
        line.jumpers[0].length = Some(jumpers[0].unstressed_length);
        let (_, again) = solve_all(&line, &still(15.0));
        assert!((again[0].sag - 1.5).abs() < 1e-6, "{}", again[0].sag);
        line.validate().unwrap();
    }

    #[test]
    fn support_string_lifts_the_jumper() {
        let mut line = crate::tests::example();
        let (_, free) = solve_all(&line, &still(15.0));
        let length = free[0].unstressed_length;
        // A 1 m string 2 m above the free jumper's half-length point (the
        // 32nd of 64 points of its single element), holding it there: the
        // string lifts that point by 1 m. (Above the lowest point instead,
        // which is nearer the lower end, the fraction-0.5 point could not
        // reach without stretching the far half by 23 %: the first version
        // of this test did that.)
        let low = free[0].points[32];
        line.jumpers[0].sag = None;
        line.jumpers[0].length = Some(length);
        line.jumpers[0].support = Some(JumperSupport {
            point: [low[0], low[1], low[2] + 2.0],
            length: 1.0,
            weight: 100.0,
            fraction: 0.5,
        });
        line.validate().unwrap();
        let (_, held) = solve_all(&line, &still(15.0));
        let (top, clamp, force) = held[0].support.unwrap();
        // The string is taut at its length and carries the jumper's weight
        // near its middle: less sag than the free jumper.
        assert!(((v(top) - v(clamp)).norm() - 1.0).abs() < 1e-3);
        assert!(force > 50.0 && force < 5_000.0, "{force}");
        assert!(held[0].sag < free[0].sag);
    }

    #[test]
    fn bad_jumpers_are_rejected() {
        let mut line = crate::tests::example();
        line.jumpers[0].length = Some(10.0);
        assert!(line.validate().unwrap_err().to_string().contains("either"));
        let mut line = crate::tests::example();
        line.jumpers[0].to = line.jumpers[0].from.clone();
        assert!(line.validate().is_err());
        let mut line = crate::tests::example();
        line.jumpers[0].sag = Some(0.0001);
        let lib = library();
        let shapes: Vec<_> = solve_line(&line, &lib, &still(15.0))
            .into_iter()
            .map(Result::unwrap)
            .collect();
        let err = solve_jumpers(&line, &lib, &still(15.0), &shapes).remove(0);
        assert!(err.unwrap_err().to_string().contains("taut"));
    }
}
