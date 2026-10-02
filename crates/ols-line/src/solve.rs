// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Solving sections of a line in 3D.
//!
//! Each section becomes an [`ols_wirefe::Model`]:
//!
//! - the first and last points, and middle points whose wire point is not
//!   below the insulator point, are **fixed** at their wire point (strain);
//! - middle points whose wire point hangs below the insulator point are
//!   **suspension strings**: a free clamp on a truss of that length from the
//!   (fixed) insulator point;
//! - every span is one exact catenary element (split at point loads);
//! - wind acts perpendicular to each span, horizontally, so the cable plane
//!   swings differently in every span.
//!
//! Unstressed lengths come from the stringing: given lengths (scaled by
//! \\( e^{\alpha \Delta t} \\)), or a horizontal tension at which every span
//! is sagged with the strings plumb.

use nalgebra::Vector3;
use ols_cables::Cable;
use ols_core::budget::Budget;
use ols_weather::WeatherCase;
use ols_wirefe::catenary_element::default_element_budget;
use ols_wirefe::{CableElement, CableProperties, Model, Node, TrussElement, default_fe_budget};

use crate::{Line, LineError, Point, Section, Stringing};

/// Axial stiffness used for insulator strings, N (composite long-rod order).
pub const DEFAULT_INSULATOR_STIFFNESS: f64 = 5e7;

/// A wire point hangs from its insulator point (suspension string) when it
/// is below it and swung less than 45° (horizontal offset < vertical drop).
/// Strain assemblies are nearly horizontal. Data exported by other programs
/// gives clamps at a solved condition, where strings at line angles are
/// already swung — a near-plumb rule would misread them as strain points.
const PLUMB_RATIO: f64 = 1.0;

fn v(p: Point) -> Vector3<f64> {
    Vector3::new(p[0], p[1], p[2])
}

fn p(v: Vector3<f64>) -> Point {
    [v.x, v.y, v.z]
}

/// Solved geometry of one span.
#[derive(Debug, Clone, PartialEq)]
pub struct SpanShape {
    /// Points along the wire, back to ahead.
    pub points: Vec<Point>,
    /// Horizontal tension (perpendicular to the load) of the first piece, N.
    pub horizontal_tension: f64,
    /// Maximum tension in the span, N (per wire).
    pub max_tension: f64,
    /// Point of the wire at half the horizontal distance between the clamps.
    pub mid_point: Point,
    /// Lowest point of the wire.
    pub low_point: Point,
    /// Largest vertical distance below the clamp-to-clamp chord, m.
    pub sag: f64,
}

/// Solved geometry of a section.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionShape {
    /// Section id.
    pub section: String,
    /// Clamp (wire) position at every point of the section.
    pub clamps: Vec<Point>,
    /// One shape per span.
    pub spans: Vec<SpanShape>,
    /// Newton iterations used.
    pub iterations: u64,
}

struct Built {
    model: Model,
    /// Node of each section point (clamp).
    clamps: Vec<usize>,
    /// For each span, the cable elements (in order).
    span_elements: Vec<Vec<usize>>,
}

fn is_suspension(insulator: Vector3<f64>, wire: Vector3<f64>) -> bool {
    let d = wire - insulator;
    d.z < 0.0 && d.xy().norm() < PLUMB_RATIO * (-d.z)
}

/// Load per unit length on a span with chord `d` for a weather case:
/// vertical (weight + ice) and horizontal wind perpendicular to the span.
fn span_load(case: &WeatherCase, cable: &Cable, d: Vector3<f64>) -> Vector3<f64> {
    let load = case.wire_load(cable.diameter, cable.weight());
    let horizontal = d.xy();
    let transverse = if horizontal.norm() > 0.0 {
        Vector3::new(-horizontal.y, horizontal.x, 0.0) / horizontal.norm()
    } else {
        Vector3::zeros()
    };
    transverse * load.horizontal.si() - Vector3::new(0.0, 0.0, load.vertical.si())
}

/// Stringing reference of a section: clamp positions and unstressed lengths
/// at a reference temperature.
struct Reference {
    clamps: Vec<Vector3<f64>>,
    lengths: Vec<f64>,
    temperature: f64,
}

/// Clamp layout of a section: fixed points, or suspension strings.
enum Clamp {
    Fixed(Vector3<f64>),
    Suspension {
        top: Vector3<f64>,
        length: f64,
        weight: f64,
    },
}

fn clamps_of(line: &Line, section: &Section) -> Result<Vec<Clamp>, LineError> {
    let n = section.points.len();
    section
        .points
        .iter()
        .enumerate()
        .map(|(k, point)| {
            let a = line.attachment(point).ok_or_else(|| {
                LineError::Invalid(format!(
                    "section {}: missing attachment {point:?}",
                    section.id
                ))
            })?;
            let (ins, wire) = (v(a.insulator_point), v(a.wire_point));
            Ok(if k > 0 && k < n - 1 && is_suspension(ins, wire) {
                Clamp::Suspension {
                    top: ins,
                    length: (wire - ins).norm(),
                    weight: a.insulator_weight,
                }
            } else {
                Clamp::Fixed(wire)
            })
        })
        .collect()
}

fn plumb(clamp: &Clamp) -> Vector3<f64> {
    match clamp {
        Clamp::Fixed(p) => *p,
        Clamp::Suspension { top, length, .. } => top - Vector3::new(0.0, 0.0, *length),
    }
}

/// Builds the finite-element model of a section in a weather state.
#[allow(clippy::too_many_arguments)]
fn build(
    section: &Section,
    layout: &[Clamp],
    reference: &Reference,
    cable: &Cable,
    case: &WeatherCase,
    with_loads: bool,
) -> Result<Built, LineError> {
    let invalid = |m: String| LineError::Invalid(format!("section {}: {m}", section.id));
    let ea = cable.axial_stiffness().si();
    let thermal =
        (cable.thermal_expansion() * (case.temperature.celsius() - reference.temperature)).exp();
    let gravity = Vector3::new(0.0, 0.0, -cable.weight().si());
    let n = layout.len();

    let mut model = Model::default();
    let mut clamps = Vec::with_capacity(n);
    for (k, clamp) in layout.iter().enumerate() {
        match clamp {
            Clamp::Fixed(p) => {
                clamps.push(model.nodes.len());
                model.nodes.push(Node::fixed(*p));
            }
            Clamp::Suspension {
                top,
                length,
                weight,
            } => {
                let top_node = model.nodes.len();
                model.nodes.push(Node::fixed(*top));
                clamps.push(model.nodes.len());
                model.nodes.push(Node::free(reference.clamps[k]));
                model.trusses.push(TrussElement {
                    i: top_node,
                    j: top_node + 1,
                    unstressed_length: *length,
                    axial_stiffness: DEFAULT_INSULATOR_STIFFNESS,
                    tension_only: true,
                    weight: *weight,
                });
            }
        }
    }

    // Cable elements, split at point loads along the reference shape.
    let element_budget = default_element_budget();
    let mut span_elements = Vec::with_capacity(n - 1);
    for s in 0..n - 1 {
        let length = reference.lengths[s] * thermal;
        let d = reference.clamps[s + 1] - reference.clamps[s];
        let load = span_load(case, cable, d);
        let shape_props = CableProperties {
            unstressed_length: reference.lengths[s],
            axial_stiffness: ea,
            load: gravity,
        };
        let mut sites: Vec<(f64, f64)> = Vec::new();
        let mut shape = None;
        if with_loads && section.loads.iter().any(|l| l.span == s) {
            let solved = shape_props
                .solve(d, None, &element_budget)
                .map_err(|e| invalid(format!("span {s}: {e}")))?;
            sites = section
                .loads
                .iter()
                .filter(|l| l.span == s)
                .map(|l| {
                    (
                        shape_props.arc_length_at_fraction(d, &solved, l.fraction),
                        l.vertical,
                    )
                })
                .collect();
            sites.sort_by(|a, b| a.0.total_cmp(&b.0));
            shape = Some(solved);
        }
        let mut elements = Vec::new();
        let (mut prev_node, mut prev_s) = (clamps[s], 0.0);
        for (s0, force) in sites {
            if s0 - prev_s < 1e-6 {
                // Coincident with the previous point: add to that node.
                model.nodes[prev_node].load.z -= force;
                continue;
            }
            let node = model.nodes.len();
            let start =
                reference.clamps[s] + shape_props.point_at(d, shape.as_ref().expect("solved"), s0);
            let mut site = Node::free(start);
            site.load = Vector3::new(0.0, 0.0, -force);
            model.nodes.push(site);
            elements.push(model.cables.len());
            model.cables.push(CableElement {
                i: prev_node,
                j: node,
                properties: CableProperties {
                    unstressed_length: (s0 - prev_s) * thermal,
                    axial_stiffness: ea,
                    load,
                },
            });
            (prev_node, prev_s) = (node, s0);
        }
        elements.push(model.cables.len());
        model.cables.push(CableElement {
            i: prev_node,
            j: clamps[s + 1],
            properties: CableProperties {
                unstressed_length: length - prev_s * thermal,
                axial_stiffness: ea,
                load,
            },
        });
        span_elements.push(elements);
    }
    Ok(Built {
        model,
        clamps,
        span_elements,
    })
}

/// Stringing reference. For a tension, every span is sagged to that
/// horizontal tension with the clamps at their **equilibrium** positions:
/// plumb strings are not in equilibrium at a line angle (a 10° angle pulled
/// a string sideways and cut a 25 kN stringing to 23.1 kN in testing). Fixed
/// point: sag between the current clamps, solve, move the clamps, repeat
/// until they move less than 1 µm (exit clause: 20 passes).
///
/// During sagging each suspension clamp keeps its plumb position along the
/// line and only swings transversely: with equal tensions the longitudinal
/// pulls cancel at any longitudinal position, so that direction is neutral
/// and the iteration crept ~3 mm per pass without settling (traced). The
/// crew fixes it by clamping in plumb along the line.
fn reference(
    section: &Section,
    layout: &[Clamp],
    cable: &Cable,
    budget: &Budget,
) -> Result<Reference, LineError> {
    let invalid = |m: String| LineError::Invalid(format!("section {}: {m}", section.id));
    let mut clamps: Vec<Vector3<f64>> = layout.iter().map(plumb).collect();
    match &section.stringing {
        Stringing::UnstressedLengths {
            lengths,
            temperature,
        } => Ok(Reference {
            clamps,
            lengths: lengths.clone(),
            temperature: *temperature,
        }),
        Stringing::Tension {
            horizontal_tension,
            temperature,
        } => {
            let ea = cable.axial_stiffness().si();
            let gravity = Vector3::new(0.0, 0.0, -cable.weight().si());
            let case = WeatherCase::still(
                "stringing",
                ols_core::units::Temperature::from_celsius(*temperature),
            );
            for _ in 0..20 {
                let lengths = (0..clamps.len() - 1)
                    .map(|s| {
                        CableProperties::unstressed_length_for_tension(
                            clamps[s + 1] - clamps[s],
                            *horizontal_tension,
                            ea,
                            gravity,
                            budget,
                        )
                        .map_err(|e| invalid(format!("span {s}: {e}")))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let reference = Reference {
                    clamps: clamps.clone(),
                    lengths,
                    temperature: *temperature,
                };
                let built = build(section, layout, &reference, cable, &case, false)?;
                let solved = built
                    .model
                    .solve(budget)
                    .map_err(|e| invalid(format!("stringing: {e}")))?;
                let n = clamps.len();
                let previous = clamps.clone();
                clamps = (0..n)
                    .map(|k| {
                        let position = solved.positions[built.clamps[k]];
                        let Clamp::Suspension { top, .. } = &layout[k] else {
                            return position;
                        };
                        // Line direction at the clamp (horizontal bisector).
                        let along = (clamps[(k + 1).min(n - 1)] - clamps[k.saturating_sub(1)]).xy();
                        let along = Vector3::new(along.x, along.y, 0.0).normalize();
                        let swing = position - top;
                        let transverse_only = swing - along * swing.dot(&along);
                        top + transverse_only * (swing.norm() / transverse_only.norm())
                    })
                    .collect();
                // Converged when the clamps used for the next pass (after the
                // transverse-only projection) stop moving.
                let moved = clamps
                    .iter()
                    .zip(&previous)
                    .map(|(new, old)| (new - old).norm())
                    .fold(0.0, f64::max);
                if moved < 1e-6 {
                    return Ok(reference);
                }
            }
            Err(invalid(
                "stringing equilibrium did not settle in 20 passes".into(),
            ))
        }
    }
}

/// Solves one section of a line in a weather case.
///
/// Expected cost: one Newton solve of 3 × (suspension clamps + load points)
/// unknowns, typically well under 10 ms.
pub fn solve_section(
    line: &Line,
    section: &Section,
    cable: &Cable,
    case: &WeatherCase,
    budget: &Budget,
) -> Result<SectionShape, LineError> {
    let layout = clamps_of(line, section)?;
    let reference = reference(section, &layout, cable, budget)?;
    let built = build(section, &layout, &reference, cable, case, true)?;
    let solution = built
        .model
        .solve(budget)
        .map_err(|e| LineError::Invalid(format!("section {}: {e}", section.id)))?;
    let pos = &solution.positions;
    let mut spans = Vec::with_capacity(built.span_elements.len());
    for (s, elements) in built.span_elements.iter().enumerate() {
        let (a, b) = (pos[built.clamps[s]], pos[built.clamps[s + 1]]);
        let chord = b - a;
        let horizontal = chord.xy().norm();
        let mut points: Vec<Vector3<f64>> = Vec::new();
        let mut max_tension: f64 = 0.0;
        for &e in elements {
            let element = &built.model.cables[e];
            let state = &solution.cables[e];
            max_tension = max_tension.max(state.tension_i).max(state.tension_j);
            let d = pos[element.j] - pos[element.i];
            let samples = element.properties.shape(d, state, 64);
            let skip = usize::from(!points.is_empty());
            points.extend(samples.into_iter().skip(skip).map(|q| pos[element.i] + q));
        }
        // Mid-point at half the horizontal clamp distance (interpolated).
        let along = |q: &Vector3<f64>| (q - a).xy().dot(&chord.xy()) / horizontal;
        let half = horizontal / 2.0;
        let mid = points
            .windows(2)
            .find(|w| along(&w[0]) <= half && half <= along(&w[1]))
            .map(|w| {
                let (t0, t1) = (along(&w[0]), along(&w[1]));
                let f = if t1 > t0 {
                    (half - t0) / (t1 - t0)
                } else {
                    0.0
                };
                w[0] + (w[1] - w[0]) * f
            })
            .unwrap_or(a + chord / 2.0);
        let low = *points
            .iter()
            .min_by(|x, y| x.z.total_cmp(&y.z))
            .expect("points");
        let sag = points
            .iter()
            .map(|q| a.z + chord.z * along(q) / horizontal - q.z)
            .fold(0.0, f64::max);
        spans.push(SpanShape {
            points: points.iter().map(|q| p(*q)).collect(),
            horizontal_tension: solution.cables[elements[0]].horizontal,
            max_tension,
            mid_point: p(mid),
            low_point: p(low),
            sag,
        });
    }
    Ok(SectionShape {
        section: section.id.clone(),
        clamps: built.clamps.iter().map(|&c| p(pos[c])).collect(),
        spans,
        iterations: solution.iterations,
    })
}

/// Solves every section; returns one result per section, in order.
pub fn solve_line(
    line: &Line,
    library: &ols_cables::Library,
    case: &WeatherCase,
) -> Vec<Result<SectionShape, LineError>> {
    let budget = default_fe_budget();
    line.sections
        .iter()
        .map(|section| {
            let cable = library.get(&section.cable).ok_or_else(|| {
                LineError::Invalid(format!(
                    "section {}: cable \"{}\" not in the libraries",
                    section.id, section.cable
                ))
            })?;
            solve_section(line, section, cable, case, &budget)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ols_core::units::{Length, Stress, Temperature};
    use std::path::Path;

    fn library() -> ols_cables::Library {
        ols_cables::Library::load(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
        )
        .unwrap()
    }

    #[test]
    fn synthetic_line_solves_and_shapes_end_on_clamps() {
        let line = crate::tests::example();
        let case = WeatherCase::still("15 °C", Temperature::from_celsius(15.0));
        for result in solve_line(&line, &library(), &case) {
            let shape = result.unwrap();
            for (s, span) in shape.spans.iter().enumerate() {
                let (first, last) = (v(span.points[0]), v(*span.points.last().unwrap()));
                assert!((first - v(shape.clamps[s])).norm() < 1e-6);
                assert!((last - v(shape.clamps[s + 1])).norm() < 1e-6);
                assert!(span.sag > 0.0);
            }
        }
    }

    #[test]
    fn heat_increases_sag_and_wind_blows_out() {
        let line = crate::tests::example();
        let lib = library();
        let cold = solve_line(
            &line,
            &lib,
            &WeatherCase::still("0", Temperature::from_celsius(0.0)),
        );
        let hot = solve_line(
            &line,
            &lib,
            &WeatherCase::still("80", Temperature::from_celsius(80.0)),
        );
        let sag = |r: &Result<SectionShape, LineError>| r.as_ref().unwrap().spans[0].sag;
        assert!(sag(&hot[0]) > sag(&cold[0]));

        let mut windy = WeatherCase::still("wind", Temperature::from_celsius(15.0));
        windy.wind_pressure = Stress::from_pascals(500.0);
        let calm = solve_line(
            &line,
            &lib,
            &WeatherCase::still("calm", Temperature::from_celsius(15.0)),
        );
        let blown = solve_line(&line, &lib, &windy);
        let mid = |r: &Result<SectionShape, LineError>| v(r.as_ref().unwrap().spans[0].mid_point);
        let shift = (mid(&blown[0]) - mid(&calm[0])).xy().norm();
        assert!(
            shift > 1.0,
            "wind should blow the wire sideways, moved {shift} m"
        );
        let _ = Length::ZERO;
    }

    #[test]
    fn tension_stringing_reproduces_its_tension() {
        let mut line = crate::tests::example();
        // Strung at 25 kN at 15 °C: solving at 15 °C gives 25 kN in every span,
        // within 0.1 %: sagging holds the clamps plumb along the line, the
        // solved state lets them settle longitudinally (6 N observed).
        line.sections[0].stringing = Stringing::Tension {
            horizontal_tension: 25_000.0,
            temperature: 15.0,
        };
        line.sections[0].loads.clear();
        let case = WeatherCase::still("15", Temperature::from_celsius(15.0));
        let shape = solve_line(&line, &library(), &case).remove(0).unwrap();
        for span in &shape.spans {
            assert!(
                (span.horizontal_tension - 25_000.0).abs() < 25.0,
                "{}",
                span.horizontal_tension
            );
        }
    }
}
