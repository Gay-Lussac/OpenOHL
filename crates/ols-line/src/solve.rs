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
//! \\( e^{\alpha \Delta t} \\)) of a section clipped in, or the condition
//! of a section in the blocks — a horizontal tension, or a sag in the ruling
//! span or in one span — at which every span is sagged with the strings
//! plumb. A section in the blocks then **rolls**: in each weather case its
//! unstressed length is shared out so that every span has the same
//! horizontal tension.

use nalgebra::Vector3;
use ols_cables::Cable;
use ols_core::budget::Budget;
use ols_weather::WeatherCase;
use ols_wirefe::catenary_element::default_element_budget;
use ols_wirefe::{CableElement, CableProperties, Model, Node, TrussElement, default_fe_budget};

use crate::{Line, LineError, Point, Section, Stringing};

/// Point loads closer than this to a span's end (along the wire), m, are
/// applied to the clamp. A marker ball 0.45 m from a free strain clamp
/// (fraction 0.001 of a 445 m span, real data) otherwise leaves a 0.45 m
/// cable piece between two free nodes — a nearly singular chain on which
/// Newton stalls; the effect on the wire's shape is far below a millimetre.
pub const LUMP_AT_CLAMP: f64 = 1.0;

/// Axial stiffness used for insulator strings, N (composite long-rod order).
pub const DEFAULT_INSULATOR_STIFFNESS: f64 = 5e7;

/// A wire point hangs from its insulator point (suspension string) when it
/// is below it and swung less than 45° (horizontal offset < vertical drop).
/// Strain assemblies are nearly horizontal. Data exported by other programs
/// gives clamps at a solved condition, where strings at line angles are
/// already swung — a near-plumb rule would misread them as strain points.
const PLUMB_RATIO: f64 = 1.0;

pub(crate) fn v(p: Point) -> Vector3<f64> {
    Vector3::new(p[0], p[1], p[2])
}

pub(crate) fn p(v: Vector3<f64>) -> Point {
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
    /// N (tension positive; the whole bundle's): empty for fixed points, one value for I-strings
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
pub(crate) fn span_load(case: &WeatherCase, cable: &Cable, d: Vector3<f64>) -> Vector3<f64> {
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
                    weight: a.insulator_weight / f64::from(section.bundle.max(1)),
                    plumb: wire,
                }
            } else if middle && is_suspension(ins, wire) {
                Clamp::Suspension {
                    top: ins,
                    length: (wire - ins).norm(),
                    weight: a.insulator_weight / f64::from(section.bundle.max(1)),
                }
            } else if (a.insulator.is_some() || a.strain_member) && (wire - ins).norm() > 0.01 {
                // A library strain assembly is modelled as a free member (it
                // needs its weight). Attachments given only as geometry keep
                // their clamp where the data puts it: imported clamp positions
                // already are the source's equilibrium, and freeing them
                // without weights made a 930-span comparison worse (median
                // 1.1 → 4.2 cm).
                Clamp::Strain {
                    top: ins,
                    length: (wire - ins).norm(),
                    weight: a.insulator_weight / f64::from(section.bundle.max(1)),
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
            if s0 - prev_s < 1e-6 || s0 < LUMP_AT_CLAMP {
                // Coincident with the previous point, or next to the clamp:
                // add to that node.
                model.nodes[prev_node].load.z -= force;
                continue;
            }
            if reference.lengths[s] - s0 < LUMP_AT_CLAMP {
                // Next to the ahead clamp: on it.
                model.nodes[clamps[s + 1]].load.z -= force;
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

/// Builds and solves a section model.
fn solve_built(
    section: &Section,
    layout: &[Clamp],
    reference: &Reference,
    cable: &Cable,
    case: &WeatherCase,
    with_loads: bool,
    budget: &Budget,
) -> Result<(Built, ols_wirefe::Solution), LineError> {
    let built = build(section, layout, reference, cable, case, with_loads)?;
    let solution = built
        .model
        .solve(budget)
        .map_err(|e| LineError::Invalid(format!("section {}: {e}", section.id)))?;
    Ok((built, solution))
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
    let (horizontal_tension, temperature) = match &section.stringing {
        Stringing::UnstressedLengths {
            lengths,
            temperature,
            ..
        } => {
            return Ok(Reference {
                clamps,
                lengths: lengths.clone(),
                temperature: *temperature,
            });
        }
        Stringing::Tension {
            horizontal_tension,
            temperature,
        } => (*horizontal_tension, *temperature),
        Stringing::Sag {
            sag,
            temperature,
            span,
        } => (
            tension_for_sag(&clamps, *span, *sag, cable.weight().si())
                .map_err(|m| invalid(format!("sag {sag} m: {m}")))?,
            *temperature,
        ),
    };
    let ea = cable.axial_stiffness().si();
    let gravity = Vector3::new(0.0, 0.0, -cable.weight().si());
    let case = WeatherCase::still(
        "stringing",
        ols_core::units::Temperature::from_celsius(temperature),
    );
    for _ in 0..20 {
        let lengths = (0..clamps.len() - 1)
            .map(|s| {
                CableProperties::unstressed_length_for_tension(
                    clamps[s + 1] - clamps[s],
                    horizontal_tension,
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
            temperature,
        };
        let (built, solved) = solve_built(section, layout, &reference, cable, &case, false, budget)
            .map_err(|e| invalid(format!("stringing: {e}")))?;
        let previous = clamps.clone();
        clamps = settled(layout, &clamps, |k| solved.positions[built.clamps[k]]);
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

/// The clamps for the next pass of a fixed point: the solved positions,
/// with suspension and V-strings swung back across the line — along it the
/// pulls of equal tensions cancel at any position (a neutral direction the
/// iteration would creep along), and the crew clamps in plumb along it.
fn settled(
    layout: &[Clamp],
    clamps: &[Vector3<f64>],
    solved: impl Fn(usize) -> Vector3<f64>,
) -> Vec<Vector3<f64>> {
    let n = clamps.len();
    (0..n)
        .map(|k| {
            let position = solved(k);
            // Line direction at the clamp: the bisector of the two spans'
            // horizontal directions (where their equal pulls cancel).
            let unit = |a: usize, b: usize| {
                let d = (clamps[b] - clamps[a]).xy();
                if d.norm() > 0.0 { d.normalize() } else { d }
            };
            let along = unit(k.saturating_sub(1), k) + unit(k, (k + 1).min(n - 1));
            let along = Vector3::new(along.x, along.y, 0.0).normalize();
            match &layout[k] {
                Clamp::Suspension { top, .. } => {
                    let swing = position - top;
                    let transverse_only = swing - along * swing.dot(&along);
                    top + transverse_only * (swing.norm() / transverse_only.norm())
                }
                // A V-string swings freely along the line, about the axis
                // through its leg tops: the same neutral direction, removed
                // by rotating the clamp about that axis back into the plane
                // across the line.
                Clamp::VString { tops, .. } => {
                    let axis = (tops[1] - tops[0]).normalize();
                    let centre = (tops[0] + tops[1]) / 2.0;
                    let r = position - centre;
                    let radial = r - axis * r.dot(&axis);
                    let along = (along - axis * along.dot(&axis)).normalize();
                    let across = radial - along * radial.dot(&along);
                    centre + axis * r.dot(&axis) + across * (radial.norm() / across.norm())
                }
                _ => position,
            }
        })
        .collect()
}

/// Sag of a catenary of parameter `c` (H / w) over a span of horizontal
/// length `s` and rise `h`: the largest vertical distance below the chord.
fn catenary_sag(c: f64, s: f64, h: f64) -> f64 {
    // y(x) = c·cosh((x − x0)/c) through (0, 0) and (s, h).
    let x0 = s / 2.0 - c * (h / (2.0 * c * (s / (2.0 * c)).sinh())).asinh();
    let y = |x: f64| c * (((x - x0) / c).cosh() - (x0 / c).cosh());
    // Deepest below the chord where the slope equals the chord's.
    let x = (x0 + c * (h / s).asinh()).clamp(0.0, s);
    h * x / s - y(x)
}

/// The horizontal tension giving sag `sag` (m) in span `span` between the
/// `clamps` (none: in the ruling span \\( \sqrt{\sum S^3 / \sum S} \\), level),
/// for weight `w` (N/m). Bisection on the tension; exit clause: 200 steps.
pub(crate) fn tension_for_sag(
    clamps: &[Vector3<f64>],
    span: Option<usize>,
    sag: f64,
    w: f64,
) -> Result<f64, String> {
    let spans: Vec<(f64, f64)> = clamps
        .windows(2)
        .map(|c| ((c[1] - c[0]).xy().norm(), c[1].z - c[0].z))
        .collect();
    let (s, h) = match span {
        Some(k) => *spans.get(k).ok_or_else(|| format!("no span {}", k + 1))?,
        None => {
            let (cubes, sum) = spans
                .iter()
                .fold((0.0, 0.0), |(c, t), (s, _)| (c + s * s * s, t + s));
            ((cubes / sum).sqrt(), 0.0)
        }
    };
    if !(s > 0.0 && sag > 0.0) {
        return Err("span and sag must be above 0".into());
    }
    let sag_for = |hh: f64| catenary_sag(hh / w, s, h);
    // Parabola estimate, then bracket and bisect (sag falls as H grows).
    let estimate = w * s * s / (8.0 * sag);
    let (mut lo, mut hi) = (estimate / 2.0, estimate * 2.0);
    let mut steps = 0;
    let mut step = || {
        steps += 1;
        if steps > 200 {
            Err("no tension found in 200 steps".to_owned())
        } else {
            Ok(())
        }
    };
    while sag_for(lo) <= sag {
        step()?;
        lo /= 2.0;
    }
    while sag_for(hi) >= sag {
        step()?;
        hi *= 2.0;
    }
    while hi - lo > 1e-10 * hi {
        step()?;
        let mid = 0.5 * (lo + hi);
        if sag_for(mid) > sag {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Ok(0.5 * (lo + hi))
}

/// A section in the blocks in a weather case: its unstressed length (the
/// sum of the reference lengths) shared out between the spans so that all
/// have the same horizontal tension — the wire rolls over the suspension
/// clamps (point loads are left out of the sharing). Bisection on the
/// tension; exit clause: 200 steps.
fn rolled(
    section: &Section,
    layout: &[Clamp],
    reference: &Reference,
    cable: &Cable,
    case: &WeatherCase,
    budget: &Budget,
) -> Result<Reference, LineError> {
    // The clamps settle where the shared-out lengths put them (strings
    // swing across the line more under ice or wind at line angles, which
    // changes the chords): fixed point, exit clause 20 passes.
    // First guess 20 kN: the bracket widens from there; later passes start
    // from the tension found.
    let (mut r, mut h) = shared_out(
        section,
        reference,
        &reference.clamps,
        cable,
        case,
        20_000.0,
        budget,
    )?;
    for _ in 0..20 {
        let (built, solved) = solve_built(section, layout, &r, cable, case, false, budget)?;
        let clamps = settled(layout, &r.clamps, |k| solved.positions[built.clamps[k]]);
        let moved = clamps
            .iter()
            .zip(&r.clamps)
            .map(|(a, b)| (a - b).norm())
            .fold(0.0, f64::max);
        if moved < 1e-6 {
            return Ok(r);
        }
        (r, h) = shared_out(section, reference, &clamps, cable, case, h, budget)?;
    }
    Err(LineError::Invalid(format!(
        "section {}: rolling did not settle in 20 passes",
        section.id
    )))
}

/// The reference's unstressed length shared out between the spans between
/// `clamps` so that all have the same horizontal tension in `case` (found
/// from `guess`, N); with that tension.
fn shared_out(
    section: &Section,
    reference: &Reference,
    clamps: &[Vector3<f64>],
    cable: &Cable,
    case: &WeatherCase,
    guess: f64,
    budget: &Budget,
) -> Result<(Reference, f64), LineError> {
    let invalid = |m: String| LineError::Invalid(format!("section {}: rolling: {m}", section.id));
    let ea = cable.axial_stiffness().si();
    let thermal =
        (cable.thermal_expansion() * (case.temperature.celsius() - reference.temperature)).exp();
    let total: f64 = reference.lengths.iter().sum();
    let chords: Vec<Vector3<f64>> = clamps.windows(2).map(|c| c[1] - c[0]).collect();
    // Unstressed lengths at the reference temperature for tension `h`.
    let lengths = |h: f64| -> Result<Vec<f64>, LineError> {
        chords
            .iter()
            .enumerate()
            .map(|(s, d)| {
                CableProperties::unstressed_length_for_tension(
                    *d,
                    h,
                    ea,
                    span_load(case, cable, *d),
                    budget,
                )
                .map(|l| l / thermal)
                .map_err(|e| invalid(format!("span {s}: {e}")))
            })
            .collect()
    };
    let excess = |h: f64| -> Result<f64, LineError> { Ok(lengths(h)?.iter().sum::<f64>() - total) };
    // Longer wire for a lower tension: bracket around the guess, then a
    // bracketed secant (Illinois): a few steps instead of ~45 halvings.
    let mut steps = 0;
    let mut step = || {
        steps += 1;
        if steps > 200 {
            Err(invalid("no common tension found in 200 steps".into()))
        } else {
            Ok(())
        }
    };
    let (mut lo, mut hi) = (guess / 1.2, guess * 1.2);
    let mut f_lo = excess(lo)?;
    while f_lo < 0.0 {
        step()?;
        (hi, lo) = (lo, lo / 2.0);
        f_lo = excess(lo)?;
    }
    let mut f_hi = excess(hi)?;
    while f_hi > 0.0 {
        step()?;
        (lo, f_lo, hi) = (hi, f_hi, hi * 2.0);
        f_hi = excess(hi)?;
    }
    let mut side = 0;
    let mut h = 0.5 * (lo + hi);
    while hi - lo > 1e-9 * hi {
        step()?;
        h = (lo * f_hi - hi * f_lo) / (f_hi - f_lo);
        if !(lo < h && h < hi) {
            h = 0.5 * (lo + hi);
        }
        let f = excess(h)?;
        if f.abs() < 1e-12 * total {
            break;
        }
        if f > 0.0 {
            (lo, f_lo) = (h, f);
            if side == 1 {
                f_hi /= 2.0;
            }
            side = 1;
        } else {
            (hi, f_hi) = (h, f);
            if side == -1 {
                f_lo /= 2.0;
            }
            side = -1;
        }
    }
    Ok((
        Reference {
            clamps: clamps.to_vec(),
            lengths: lengths(h)?,
            temperature: reference.temperature,
        },
        h,
    ))
}

/// The unstressed length of each span (m) and their temperature (°C) at
/// the section's stringing condition, on the current geometry: what a
/// section in the blocks is clipped in with.
pub fn clip_lengths(
    line: &Line,
    section: &Section,
    cable: &Cable,
) -> Result<(Vec<f64>, f64), LineError> {
    let layout = clamps_of(line, section)?;
    let r = reference(section, &layout, cable, &default_fe_budget())?;
    Ok((r.lengths, r.temperature))
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
    let mut reference = reference(section, &layout, cable, budget)?;
    if !section.stringing.clipped() && reference.lengths.len() > 1 {
        reference = rolled(section, &layout, &reference, cable, case, budget)?;
    }
    let (built, solution) = solve_built(section, &layout, &reference, cable, case, true, budget)?;
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
            // Forces of the whole bundle (the model is one subconductor).
            .map(|m| {
                m.iter()
                    .map(|&t| solution.truss_forces[t] * f64::from(section.bundle.max(1)))
                    .collect()
            })
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

    #[test]
    fn a_bundle_shares_its_strings_weight() {
        // A 3-bundle under 3 × 900 N strings swings like one wire under
        // 900 N; the reported string forces are the whole bundle's.
        let lib = library();
        let mut windy = WeatherCase::still("wind", Temperature::from_celsius(15.0));
        windy.wind_pressure = Stress::from_pascals(400.0);
        let single = crate::tests::example();
        let mut bundled = single.clone();
        bundled.sections[0].bundle = 3;
        bundled.sections[0].bundle_spacing = Some(0.4);
        for s in &mut bundled.structures {
            for a in &mut s.attachments {
                a.insulator_weight *= 3.0;
            }
        }
        bundled.validate().unwrap();
        let a = solve_line(&single, &lib, &windy).remove(0).unwrap();
        let b = solve_line(&bundled, &lib, &windy).remove(0).unwrap();
        for (p, q) in a.clamps.iter().zip(&b.clamps) {
            assert!((v(*p) - v(*q)).norm() < 1e-9, "{p:?} {q:?}");
        }
        for (f, g) in a
            .insulators
            .iter()
            .flatten()
            .zip(b.insulators.iter().flatten())
        {
            assert!((3.0 * f - g).abs() < 1e-6 * g.abs().max(1.0), "{f} {g}");
        }
        assert!(a.insulators.iter().flatten().count() > 0);
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

    #[test]
    fn catenary_sag_matches_the_parabola_for_a_shallow_span() {
        // Level 300 m span, c = 2000 m: sag ≈ S²/(8c) = 5.625 m (+0.04 %).
        let sag = super::catenary_sag(2000.0, 300.0, 0.0);
        assert!((sag - 5.625).abs() < 0.01, "{sag}");
        // Inclined: the sag from the chord is about the same.
        let inclined = super::catenary_sag(2000.0, 300.0, 30.0);
        assert!((inclined - sag).abs() < 0.05, "{inclined}");
    }

    /// Sagged to 8 m in its second span: solving in the stringing state
    /// gives that sag there, and the same tension in every span.
    #[test]
    fn sag_stringing_reproduces_its_sag() {
        let mut line = crate::tests::example();
        line.sections[0].stringing = Stringing::Sag {
            sag: 8.0,
            temperature: 15.0,
            span: Some(1),
        };
        line.sections[0].loads.clear();
        let case = WeatherCase::still("15", Temperature::from_celsius(15.0));
        let shape = solve_line(&line, &library(), &case).remove(0).unwrap();
        assert!(
            (shape.spans[1].sag - 8.0).abs() < 0.02,
            "{}",
            shape.spans[1].sag
        );
        let h0 = shape.spans[0].horizontal_tension;
        for span in &shape.spans {
            assert!((span.horizontal_tension - h0).abs() < 0.002 * h0);
        }
    }

    /// In the blocks the spans keep one tension in every weather case;
    /// clipped in, with the same lengths at stringing, they do not under
    /// ice; a structure moved changes the tension of a clipped section, not
    /// the stringing tension of one in the blocks.
    #[test]
    fn sections_in_the_blocks_roll_clipped_ones_do_not() {
        let lib = library();
        let mut line = crate::tests::example();
        line.sections[0].loads.clear();
        line.sections[0].stringing = Stringing::Tension {
            horizontal_tension: 25_000.0,
            temperature: 15.0,
        };
        let cable = lib.get(&line.sections[0].cable).unwrap().clone();
        let (lengths, temperature) = clip_lengths(&line, &line.sections[0], &cable).unwrap();
        let mut clipped = line.clone();
        clipped.sections[0].stringing = Stringing::UnstressedLengths {
            lengths,
            temperature,
            sagged: Some(Box::new(line.sections[0].stringing.clone())),
        };
        let still = WeatherCase::still("15", Temperature::from_celsius(15.0));
        let mut iced = WeatherCase::still("ice", Temperature::from_celsius(-5.0));
        iced.ice_thickness = ols_core::units::Length::from_si(0.02);
        let spread = |l: &Line, c: &WeatherCase| {
            let shape = solve_line(l, &lib, c).remove(0).unwrap();
            let h: Vec<f64> = shape.spans.iter().map(|s| s.horizontal_tension).collect();
            let (lo, hi) = h
                .iter()
                .fold((f64::MAX, 0.0f64), |(a, b), x| (a.min(*x), b.max(*x)));
            (lo, hi)
        };
        // At stringing both are the same.
        let (a, b) = spread(&line, &still);
        let (c, d) = spread(&clipped, &still);
        assert!(
            (a - c).abs() < 30.0 && (b - d).abs() < 30.0,
            "{a} {b} {c} {d}"
        );
        // Iced: one tension rolling; clipped, the strings swing to balance
        // the spans but not fully (0.1 % here).
        let (lo, hi) = spread(&line, &iced);
        let (clo, chi) = spread(&clipped, &iced);
        println!("iced: rolling {lo}–{hi}, clipped {clo}–{chi}");
        assert!(
            hi - lo < 0.2 * (chi - clo),
            "rolling {lo}–{hi}, clipped {clo}–{chi}"
        );
        assert!(chi - clo > 0.0005 * chi, "clipped {clo}–{chi}");
        // T3 moved 20 m along the line.
        let moved = |l: &Line| {
            let mut l = l.clone();
            let k = l.structures.iter().position(|s| s.id == "T3").unwrap();
            let (p, q) = (l.structures[k - 1].position, l.structures[k].position);
            let d = [q[0] - p[0], q[1] - p[1]];
            let n = d[0].hypot(d[1]);
            l.structures[k] =
                l.structures[k].moved_to([q[0] + 20.0 * d[0] / n, q[1] + 20.0 * d[1] / n, q[2]]);
            l
        };
        let (lo, hi) = spread(&moved(&line), &still);
        assert!((lo - 25_000.0).abs() < 50.0 && (hi - 25_000.0).abs() < 50.0);
        let (lo, hi) = spread(&moved(&clipped), &still);
        assert!(hi - lo > 500.0, "clipped after the move {lo}–{hi}");
    }
}
