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
//! - middle points with two `legs` are **V-strings**: a free clamp on two
//!   tension-only trusses from the (fixed) leg points; a leg pushed into
//!   compression goes slack (reported in [`SectionShape::insulators`]);
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
    /// Axial force of each insulator member at every point of the section,
    /// N (tension positive): empty for fixed points, one value for I-strings
    /// and strain assemblies, two for V-string legs (0 = slack leg).
    pub insulators: Vec<Vec<f64>>,
    /// Newton iterations used.
    pub iterations: u64,
}

struct Built {
    model: Model,
    /// Node of each section point (clamp).
    clamps: Vec<usize>,
    /// For each span, the cable elements (in order).
    span_elements: Vec<Vec<usize>>,
    /// For each section point, its insulator trusses.
    members: Vec<Vec<usize>>,
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

/// Clamp layout of a section: fixed points, suspension strings, strain
/// assemblies. Section ends are always strain points (or fixed).
enum Clamp {
    /// Wire fixed at a point (no insulator length).
    Fixed(Vector3<f64>),
    /// Suspension string: free clamp hanging from `top`.
    Suspension {
        top: Vector3<f64>,
        length: f64,
        weight: f64,
    },
    /// Strain assembly: free clamp on a member of `length` from `top`; it
    /// lines up with its conductor by equilibrium (direction and droop
    /// depend on the tension — a steep down-lead's string hangs 47° below
    /// horizontal in real data). `initial` is the starting position.
    Strain {
        top: Vector3<f64>,
        length: f64,
        weight: f64,
        initial: Vector3<f64>,
    },
    /// V-string: free clamp on two tension-only legs from `tops`; `plumb`
    /// is the still-air clamp position from the data.
    VString {
        tops: [Vector3<f64>; 2],
        lengths: [f64; 2],
        weight: f64,
        plumb: Vector3<f64>,
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
            let middle = k > 0 && k < n - 1;
            Ok(if let (true, Some(legs)) = (middle, &a.legs) {
                let tops = legs.map(v);
                Clamp::VString {
                    tops,
                    lengths: tops.map(|t| (wire - t).norm()),
                    weight: a.insulator_weight,
                    plumb: wire,
                }
            } else if middle && is_suspension(ins, wire) {
                Clamp::Suspension {
                    top: ins,
                    length: (wire - ins).norm(),
                    weight: a.insulator_weight,
                }
            } else if a.insulator.is_some() && (wire - ins).norm() > 0.01 {
                // A library strain assembly is modelled as a free member (it
                // needs its weight). Attachments given only as geometry keep
                // their clamp where the data puts it: imported clamp positions
                // already are the source's equilibrium, and freeing them
                // without weights made a 930-span comparison worse (median
                // 1.1 → 4.2 cm).
                Clamp::Strain {
                    top: ins,
                    length: (wire - ins).norm(),
                    weight: a.insulator_weight,
                    initial: wire,
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
        Clamp::Strain { initial, .. } => *initial,
        Clamp::VString { plumb, .. } => *plumb,
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
    let mut members = Vec::with_capacity(n);
    for (k, clamp) in layout.iter().enumerate() {
        match clamp {
            Clamp::Fixed(p) => {
                clamps.push(model.nodes.len());
                model.nodes.push(Node::fixed(*p));
                members.push(vec![]);
            }
            Clamp::VString {
                tops,
                lengths,
                weight,
                ..
            } => {
                let first = model.nodes.len();
                model.nodes.push(Node::fixed(tops[0]));
                model.nodes.push(Node::fixed(tops[1]));
                let clamp_node = model.nodes.len();
                clamps.push(clamp_node);
                model.nodes.push(Node::free(reference.clamps[k]));
                members.push(vec![model.trusses.len(), model.trusses.len() + 1]);
                for (leg, length) in [first, first + 1].into_iter().zip(lengths) {
                    model.trusses.push(TrussElement {
                        i: leg,
                        j: clamp_node,
                        unstressed_length: *length,
                        axial_stiffness: DEFAULT_INSULATOR_STIFFNESS,
                        tension_only: true,
                        weight: weight / 2.0,
                    });
                }
            }
            Clamp::Suspension {
                top,
                length,
                weight,
            }
            | Clamp::Strain {
                top,
                length,
                weight,
                ..
            } => {
                let top_node = model.nodes.len();
                model.nodes.push(Node::fixed(*top));
                clamps.push(model.nodes.len());
                model.nodes.push(Node::free(reference.clamps[k]));
                members.push(vec![model.trusses.len()]);
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
        members,
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
                        // Line direction at the clamp (horizontal bisector).
                        let along = (clamps[(k + 1).min(n - 1)] - clamps[k.saturating_sub(1)]).xy();
                        let along = Vector3::new(along.x, along.y, 0.0).normalize();
                        match &layout[k] {
                            Clamp::Suspension { top, .. } => {
                                let swing = position - top;
                                let transverse_only = swing - along * swing.dot(&along);
                                top + transverse_only * (swing.norm() / transverse_only.norm())
                            }
                            // A V-string swings freely along the line, about
                            // the axis through its leg tops: the same neutral
                            // direction, removed by rotating the clamp about
                            // that axis back into the plane across the line.
                            Clamp::VString { tops, .. } => {
                                let axis = (tops[1] - tops[0]).normalize();
                                let centre = (tops[0] + tops[1]) / 2.0;
                                let r = position - centre;
                                let radial = r - axis * r.dot(&axis);
                                let along = (along - axis * along.dot(&axis)).normalize();
                                let across = radial - along * radial.dot(&along);
                                centre
                                    + axis * r.dot(&axis)
                                    + across * (radial.norm() / across.norm())
                            }
                            _ => position,
                        }
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
        insulators: built
            .members
            .iter()
            .map(|m| m.iter().map(|&t| solution.truss_forces[t]).collect())
            .collect(),
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

    /// The synthetic line with T2's first attachment (a 3.5 m I-string,
    /// line along x) replaced by a V-string with legs ±3 m across the line.
    fn with_v_string() -> Line {
        let mut line = crate::tests::example();
        let t2 = line.structures.iter_mut().find(|s| s.id == "T2").unwrap();
        let a = t2
            .attachments
            .iter_mut()
            .find(|a| (a.set, a.phase) == (1, 1))
            .unwrap();
        let c = a.insulator_point;
        a.legs = Some([[c[0], c[1] - 3.0, c[2]], [c[0], c[1] + 3.0, c[2]]]);
        a.insulator_weight = 1000.0;
        line.sections[0].loads.clear();
        line.validate().unwrap();
        line
    }

    #[test]
    fn v_string_holds_the_clamp_against_wind_until_a_leg_goes_slack() {
        let lib = library();
        let case = |pa: f64| {
            let mut c = WeatherCase::still("wind", Temperature::from_celsius(15.0));
            c.wind_pressure = Stress::from_pascals(pa);
            c
        };
        let solve = |line: &Line, pa: f64| solve_line(line, &lib, &case(pa)).remove(0).unwrap();
        // Movement of T2's clamp across the line (y; the line runs along x).
        let across = |a: &SectionShape, b: &SectionShape| (a.clamps[1][1] - b.clamps[1][1]).abs();

        let v_line = with_v_string();
        let calm = solve(&v_line, 0.0);
        // Still air: both legs in tension, carrying the weight span.
        assert_eq!(calm.insulators[1].len(), 2);
        assert!(
            calm.insulators[1].iter().all(|&f| f > 1000.0),
            "{:?}",
            calm.insulators[1]
        );
        assert!(calm.insulators[0].is_empty());
        // Strung at 25 kN with the V clamped plumb: 25 kN at stringing.
        for span in &calm.spans {
            assert!(
                (span.horizontal_tension - 25_000.0).abs() < 25.0,
                "{}",
                span.horizontal_tension
            );
        }

        // 400 Pa: both legs still in tension, the clamp stays put across the
        // line (< 1 mm: only the leg stretch), while an I-string swings.
        let blown = solve(&v_line, 400.0);
        assert!(
            blown.insulators[1].iter().all(|&f| f > 0.0),
            "{:?}",
            blown.insulators[1]
        );
        let mut i_line = crate::tests::example();
        i_line.sections[0].loads.clear();
        let i_across = across(&solve(&i_line, 400.0), &solve(&i_line, 0.0));
        let v_across = across(&blown, &calm);
        assert!(
            v_across < 1e-3 && i_across > 1.0,
            "V {v_across} m, I {i_across} m"
        );

        // 800 Pa: the load leans further than the windward leg (40.6° from
        // vertical), the leeward leg goes slack and the clamp swings about the
        // windward leg's top — out and up on a circle of the leg length.
        let gale = solve(&v_line, 800.0);
        let legs = &gale.insulators[1];
        assert!(
            legs.contains(&0.0) && legs.iter().any(|&f| f > 1000.0),
            "{legs:?}"
        );
        assert!(across(&gale, &calm) > 0.1);
        let windward = v_line.structure("T2").unwrap().attachments[0].legs.unwrap()
            [usize::from(legs[1] > 0.0)];
        let radius = (v(gale.clamps[1]) - v(windward)).norm();
        assert!(
            (radius - (3.0f64.powi(2) + 3.5f64.powi(2)).sqrt()).abs() < 2e-3,
            "{radius}"
        );
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
