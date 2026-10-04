// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Solving sections of a line in 3D.
//!
//! Each section becomes an [`ols_wirefe::Model`]:
//!
//! - the first and last points, and middle points whose wire point is not
//!   below the insulator point, are **strain assemblies**: a free clamp on
//!   a member from the (fixed) insulator point, lined up with its
//!   conductor by equilibrium, so it moves with every weather case; a
//!   string without a weight (geometry only, e.g. imported) gets the weight
//!   that holds it where the data puts it in the stringing state
//!   (`estimate_strain_weights`); a point whose wire and insulator points
//!   coincide is **fixed** there;
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
    /// Weight estimated for each strain assembly given without one (the
    /// whole bundle's, N), at every point of the section (`None`: given or
    /// not a strain assembly).
    pub estimated_weights: Vec<Option<f64>>,
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
    // Wind from the right of the span blows it to the left.
    transverse * (load.horizontal.si() * case.wind_from.to_left())
        - Vector3::new(0.0, 0.0, load.vertical.si())
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
#[derive(Clone)]
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
        /// No weight given: estimated from `initial`.
        estimate: bool,
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
            } else if (wire - ins).norm() > 0.01 {
                // A strain assembly is a free member, so it moves with the
                // weather. One given only as geometry, without a weight,
                // would line up with its conductor and lose the droop the
                // data shows (freed without weights, a 930-span comparison
                // got worse: median 1.1 → 4.2 cm): its weight is estimated
                // from where the data puts it.
                Clamp::Strain {
                    top: ins,
                    length: (wire - ins).norm(),
                    weight: a.insulator_weight / f64::from(section.bundle.max(1)),
                    initial: wire,
                    estimate: a.insulator.is_none()
                        && !a.strain_member
                        && a.insulator_weight == 0.0,
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

/// Weights of the strain assemblies given without one: in the stringing
/// state (still air at the stringing temperature) with those clamps held
/// where the data puts them, the weight W at which each member is in
/// equilibrium there, pinned at its top: the moment of the wires' pull F at
/// the clamp balances that of W at the member's middle,
/// \\( r \\times F + \\tfrac{r}{2} \\times (0, 0, -W) = 0 \\), r from top to
/// clamp (least squares; never below 0). Its unstressed length is the
/// data's less its stretch under that pull. Freed, the member stays where
/// the data puts it in that state and moves with the weather in the others.
///
/// Expected cost: one more stringing reference and one section solve
/// (a few ms); only for sections with such strings.
fn estimate_strain_weights(
    section: &Section,
    mut layout: Vec<Clamp>,
    cable: &Cable,
    budget: &Budget,
) -> Result<(Vec<Clamp>, Vec<Option<f64>>), LineError> {
    let mut estimated = vec![None; layout.len()];
    if !layout
        .iter()
        .any(|c| matches!(c, Clamp::Strain { estimate: true, .. }))
    {
        return Ok((layout, estimated));
    }
    let held: Vec<Clamp> = layout
        .iter()
        .map(|c| match c {
            Clamp::Strain {
                estimate: true,
                initial,
                ..
            } => Clamp::Fixed(*initial),
            other => other.clone(),
        })
        .collect();
    let reference = reference(section, &held, cable, budget)?;
    let case = WeatherCase::still(
        "stringing",
        ols_core::units::Temperature::from_celsius(reference.temperature),
    );
    let (built, solution) = solve_built(section, &held, &reference, cable, &case, true, budget)?;
    for (k, clamp) in layout.iter_mut().enumerate() {
        if let Clamp::Strain {
            top,
            length,
            weight,
            initial,
            estimate: estimate @ true,
        } = clamp
        {
            // The wires' pull on the clamp: the support's reaction, reversed.
            let pull = -solution.reactions[built.clamps[k]];
            let r = *initial - *top;
            let moment = r.cross(&pull);
            let per_newton = (r / 2.0).cross(&Vector3::new(0.0, 0.0, -1.0));
            let norm = per_newton.norm_squared();
            *weight = if norm > 0.0 {
                (-moment.dot(&per_newton) / norm).max(0.0)
            } else {
                0.0
            };
            // The data's length is the loaded one: unstressed, the member
            // is shorter by its stretch (5 mm on a 5 m string at 50 kN;
            // left in, the slack cut short tight spans' tension by up to
            // 4 % on a real line).
            let force = pull.dot(&r.normalize()) + *weight / 2.0 * (-r.z / r.norm());
            *length /= 1.0 + force.max(0.0) / DEFAULT_INSULATOR_STIFFNESS;
            *estimate = false;
            estimated[k] = Some(*weight * f64::from(section.bundle.max(1)));
        }
    }
    Ok((layout, estimated))
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
    let budget = default_fe_budget();
    let (layout, _) = estimate_strain_weights(section, clamps_of(line, section)?, cable, &budget)?;
    let r = reference(section, &layout, cable, &budget)?;
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
    use crate::condition::{Condition, Method};
    let calc = section.calculation;
    let solve = |final_state: Option<&FinalState>| match calc.method() {
        Method::Fe => solve_fe(
            line,
            section,
            cable,
            case,
            final_state.map_or(0.0, |f| f.stretch),
            budget,
        ),
        Method::Rs => solve_rs(line, section, cable, case, final_state, budget),
    };
    match calc.condition() {
        Condition::Initial => solve(None),
        Condition::Creep => solve(Some(&final_state(
            line,
            section,
            cable,
            Condition::Creep,
            budget,
        )?)),
        Condition::Load => solve(Some(&final_state(
            line,
            section,
            cable,
            Condition::Load,
            budget,
        )?)),
        Condition::MaxSag => {
            // Whichever of the two sags more in this weather case.
            let max_sag = |s: &SectionShape| s.spans.iter().map(|x| x.sag).fold(0.0, f64::max);
            let creep = final_state(line, section, cable, Condition::Creep, budget);
            let load = final_state(line, section, cable, Condition::Load, budget);
            let shapes: Vec<SectionShape> = [creep, load]
                .into_iter()
                .filter_map(Result::ok)
                .map(|f| solve(Some(&f)))
                .collect::<Result<_, _>>()?;
            shapes
                .into_iter()
                .max_by(|a, b| max_sag(a).total_cmp(&max_sag(b)))
                .ok_or_else(|| {
                    LineError::Invalid(format!(
                        "section {}: Max Sag needs the weather cases for final after creep or \
                         after load (Criteria › Creep-Stretch)",
                        section.id
                    ))
                })
        }
    }
}

/// The cable's permanent state in a condition: its material state (for
/// the non-linear model) and its permanent stretch, the extra unit length
/// at zero tension compared with the initial state.
#[derive(Debug, Clone)]
pub struct FinalState {
    /// Material state of the non-linear model (`Initial` for the linear
    /// models: their stretch is all in `stretch`).
    pub material: ols_cables::mechanics::MaterialState,
    /// Permanent stretch, unit strain.
    pub stretch: f64,
    /// Weather case that gave it.
    pub case: String,
}

/// The total load per length of `cable` in `case`, N/m (weight, ice,
/// wind).
fn load_magnitude(case: &WeatherCase, cable: &Cable) -> f64 {
    span_load(case, cable, Vector3::new(1.0, 0.0, 0.0)).norm()
}

/// A section as strung: span geometries, clamps, stringing state,
/// horizontal tension, N.
type Strung = (
    Vec<ols_sagtension::SpanGeometry>,
    Vec<Vector3<f64>>,
    ols_sagtension::LoadState,
    f64,
);

/// The section as strung, for the sag-tension model: its spans (between
/// the clamps hanging in still air at the stringing temperature), the
/// stringing state and the horizontal tension then (the mean of the
/// spans', from the finite-element model).
fn strung(
    line: &Line,
    section: &Section,
    cable: &Cable,
    budget: &Budget,
) -> Result<Strung, LineError> {
    use ols_core::units::{ForcePerLength, Length, Temperature};
    let (layout, _) = estimate_strain_weights(section, clamps_of(line, section)?, cable, budget)?;
    let reference = reference(section, &layout, cable, budget)?;
    let still = WeatherCase::still(
        "stringing",
        Temperature::from_celsius(reference.temperature),
    );
    let (built, solution) =
        solve_built(section, &layout, &reference, cable, &still, false, budget)?;
    let clamps: Vec<Vector3<f64>> = built
        .clamps
        .iter()
        .map(|&c| solution.positions[c])
        .collect();
    let h = built
        .span_elements
        .iter()
        .map(|e| solution.cables[e[0]].horizontal)
        .sum::<f64>()
        / built.span_elements.len().max(1) as f64;
    let spans = clamps
        .windows(2)
        .map(|w| {
            let d = w[1] - w[0];
            ols_sagtension::SpanGeometry::inclined(
                Length::from_si(d.xy().norm()),
                Length::from_si(d.z),
            )
        })
        .collect();
    let state = ols_sagtension::LoadState {
        load: ForcePerLength::from_si(cable.weight().si()),
        temperature: Temperature::from_celsius(reference.temperature),
    };
    Ok((spans, clamps, state, h))
}

/// The cable's permanent state after creep or after load
/// ([`crate::condition`]), from the line's creep-stretch weather cases.
///
/// - Non-linear model: after creep, the permanent set of the creep curve
///   at the creep case; after load, the longest permanent set of the
///   initial curve among the load cases.
/// - Linear, creep ∝ tension: the strain of the temperature shift at the
///   shift tension, scaled by the creep case's mean tension; no stretch
///   after load (elastic).
/// - Linear, temperature: the strain of the temperature shift; no stretch
///   after load.
///
/// Expected cost: one still-air section solve and a few sag-tension
/// solves (~1 ms each for 10 spans).
pub fn final_state(
    line: &Line,
    section: &Section,
    cable: &Cable,
    condition: crate::condition::Condition,
    budget: &Budget,
) -> Result<FinalState, LineError> {
    use crate::condition::Condition;
    use ols_cables::CableModel;
    use ols_cables::mechanics::{MaterialState, Mechanics};
    use ols_core::units::{Force, ForcePerLength};
    use ols_sagtension::nonlinear::NonlinearSection;
    let invalid = |m: String| LineError::Invalid(format!("section {}: {m}", section.id));
    let st = |e: ols_sagtension::SagTensionError| invalid(e.to_string());
    let cases = &line.condition_cases;
    let state_of = |c: &WeatherCase| ols_sagtension::LoadState {
        load: ForcePerLength::from_si(load_magnitude(c, cable)),
        temperature: c.temperature,
    };
    let (spans, _, strung_state, h) = strung(line, section, cable, budget)?;
    let mechanics = Mechanics::from_cable(cable).map_err(|e| invalid(e.to_string()))?;
    let ns = NonlinearSection::from_known_state(
        spans,
        mechanics.clone(),
        strung_state,
        Force::from_si(h),
        &MaterialState::Initial,
    )
    .map_err(st)?;
    let zero_strain = |m: &MaterialState| {
        mechanics
            .strain(Force::ZERO, strung_state.temperature, m)
            .map_err(|e| invalid(e.to_string()))
    };
    match condition {
        Condition::Creep => {
            let case = cases.creep.as_ref().ok_or_else(|| {
                invalid("no weather case for final after creep (Criteria › Creep-Stretch)".into())
            })?;
            let alpha = cable.thermal_expansion();
            match cable.model {
                CableModel::Nonlinear => {
                    let crept = ns
                        .solve(state_of(case), &MaterialState::Creep, budget)
                        .map_err(st)?;
                    let material = mechanics
                        .permanent_set(
                            Force::from_si(NonlinearSection::section_mean_tension(&crept)),
                            case.temperature,
                            &MaterialState::Creep,
                        )
                        .map_err(|e| invalid(e.to_string()))?;
                    let stretch = zero_strain(&material)? - zero_strain(&MaterialState::Initial)?;
                    Ok(FinalState {
                        material,
                        stretch: stretch.max(0.0),
                        case: case.name.clone(),
                    })
                }
                CableModel::LinearCreepTemperature => Ok(FinalState {
                    material: MaterialState::Initial,
                    stretch: alpha * cable.creep_temperature_shift.unwrap_or(0.0),
                    case: case.name.clone(),
                }),
                CableModel::LinearCreepTension => {
                    let solved = ns
                        .solve(state_of(case), &MaterialState::Initial, budget)
                        .map_err(st)?;
                    let t = NonlinearSection::section_mean_tension(&solved);
                    let at = cable.creep_shift_tension.map_or(1.0, |f| f.si());
                    Ok(FinalState {
                        material: MaterialState::Initial,
                        stretch: alpha * cable.creep_temperature_shift.unwrap_or(0.0) * t / at,
                        case: case.name.clone(),
                    })
                }
            }
        }
        Condition::Load => {
            if cases.loads.is_empty() {
                return Err(invalid(
                    "no weather case for final after load (Criteria › Creep-Stretch)".into(),
                ));
            }
            if cable.model != CableModel::Nonlinear {
                // Linear elastic: no permanent stretch from a load.
                return Ok(FinalState {
                    material: MaterialState::Initial,
                    stretch: 0.0,
                    case: cases.loads[0].name.clone(),
                });
            }
            // The case giving the most stretch.
            let mut best: Option<FinalState> = None;
            for case in &cases.loads {
                let loaded = ns
                    .solve(state_of(case), &MaterialState::Initial, budget)
                    .map_err(st)?;
                let material = mechanics
                    .permanent_set(
                        Force::from_si(NonlinearSection::section_mean_tension(&loaded)),
                        case.temperature,
                        &MaterialState::Initial,
                    )
                    .map_err(|e| invalid(e.to_string()))?;
                let stretch =
                    (zero_strain(&material)? - zero_strain(&MaterialState::Initial)?).max(0.0);
                if best.as_ref().is_none_or(|b| stretch > b.stretch) {
                    best = Some(FinalState {
                        material,
                        stretch,
                        case: case.name.clone(),
                    });
                }
            }
            Ok(best.expect("load cases given"))
        }
        Condition::Initial | Condition::MaxSag => Ok(FinalState {
            material: MaterialState::Initial,
            stretch: 0.0,
            case: String::new(),
        }),
    }
}

/// Ruling-span solution of a section in `case`: one horizontal tension for
/// all spans (the sag-tension section, non-linear cable model, with the
/// permanent state `final_state`), each span a catenary between its
/// clamps held where they hang in still air at stringing, in the plane of
/// its load (blown out by the wind). Point loads are left out.
fn solve_rs(
    line: &Line,
    section: &Section,
    cable: &Cable,
    case: &WeatherCase,
    final_state: Option<&FinalState>,
    budget: &Budget,
) -> Result<SectionShape, LineError> {
    use ols_cables::mechanics::{MaterialState, Mechanics};
    use ols_core::units::{Force, ForcePerLength, Length};
    use ols_sagtension::nonlinear::NonlinearSection;
    let invalid = |m: String| LineError::Invalid(format!("section {}: {m}", section.id));
    let st = |e: ols_sagtension::SagTensionError| invalid(e.to_string());
    let (spans, clamps, strung_state, h0) = strung(line, section, cable, budget)?;
    let mechanics = Mechanics::from_cable(cable).map_err(|e| invalid(e.to_string()))?;
    let mut ns = NonlinearSection::from_known_state(
        spans.clone(),
        mechanics,
        strung_state,
        Force::from_si(h0),
        &MaterialState::Initial,
    )
    .map_err(st)?;
    let material = match final_state {
        Some(f) => {
            if matches!(f.material, MaterialState::Initial) {
                // Linear models: the stretch lengthens the cable.
                ns.unstressed_length =
                    Length::from_si(ns.unstressed_length.si() * (1.0 + f.stretch));
            }
            f.material.clone()
        }
        None => MaterialState::Initial,
    };
    let q = load_magnitude(case, cable);
    let solved = ns
        .solve(
            ols_sagtension::LoadState {
                load: ForcePerLength::from_si(q),
                temperature: case.temperature,
            },
            &material,
            budget,
        )
        .map_err(st)?;
    let h = solved.horizontal_tension.si();
    let mut out = Vec::with_capacity(spans.len());
    for (k, sol) in solved.spans.iter().enumerate() {
        let (a, b) = (clamps[k], clamps[k + 1]);
        let d = b - a;
        let horizontal = d.xy().norm();
        // The plane of the load: the sag hangs along it (down, and to the
        // side the wind blows).
        let along_load = {
            let l = span_load(case, cable, d);
            if l.norm() > 0.0 {
                l / l.norm()
            } else {
                Vector3::new(0.0, 0.0, -1.0)
            }
        };
        let n = 64;
        let mut points = Vec::with_capacity(n + 1);
        let mut sag: f64 = 0.0;
        for i in 0..=n {
            let t = i as f64 / n as f64;
            let x = horizontal * t;
            let below = (d.z * t - sol.elevation(Length::from_si(x)).si()).max(0.0);
            sag = sag.max(below);
            points.push(a + d * t + along_load * below);
        }
        let low = *points
            .iter()
            .min_by(|p, q| p.z.total_cmp(&q.z))
            .expect("points");
        out.push(SpanShape {
            points: points.iter().map(|q| p(*q)).collect(),
            horizontal_tension: h,
            max_tension: sol.tension_left().si().max(sol.tension_right().si()),
            mid_point: p(points[n / 2]),
            low_point: p(low),
            sag,
        });
    }
    let n = clamps.len();
    Ok(SectionShape {
        section: section.id.clone(),
        clamps: clamps.iter().map(|c| p(*c)).collect(),
        spans: out,
        insulators: vec![Vec::new(); n],
        iterations: 0,
        estimated_weights: vec![None; n],
    })
}

/// Finite-element solution of a section in `case`, its cable longer by
/// the permanent `stretch` (unit strain; 0: initial).
fn solve_fe(
    line: &Line,
    section: &Section,
    cable: &Cable,
    case: &WeatherCase,
    stretch: f64,
    budget: &Budget,
) -> Result<SectionShape, LineError> {
    let (layout, estimated_weights) =
        estimate_strain_weights(section, clamps_of(line, section)?, cable, budget)?;
    let mut reference = reference(section, &layout, cable, budget)?;
    if !section.stringing.clipped() && reference.lengths.len() > 1 {
        reference = rolled(section, &layout, &reference, cable, case, budget)?;
    }
    if stretch != 0.0 {
        for l in &mut reference.lengths {
            *l *= 1.0 + stretch;
        }
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
        estimated_weights,
    })
}

/// Solves one section (it need not be the line's own: a section being
/// edited) with the default exit clauses; its cable from `library`.
pub fn solve_one(
    line: &Line,
    section: &Section,
    library: &ols_cables::Library,
    case: &WeatherCase,
) -> Result<SectionShape, LineError> {
    let cable = library.get(&section.cable).ok_or_else(|| {
        LineError::Invalid(format!(
            "section {}: cable {} is not in the wire library",
            section.id, section.cable
        ))
    })?;
    solve_section(line, section, cable, case, &default_fe_budget())
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
        // From the right (default) to the left of the span; from the left,
        // the same amount to the right.
        let first = line.sections[0].points[..2]
            .iter()
            .map(|p| v(line.attachment(p).unwrap().wire_point))
            .collect::<Vec<_>>();
        let ahead = (first[1] - first[0]).xy();
        let left_of = |d: Vector3<f64>| ahead.x * d.y - ahead.y * d.x;
        let moved = mid(&blown[0]) - mid(&calm[0]);
        assert!(left_of(moved) > 0.0, "from the right: blown to the left");
        windy.wind_from = ols_weather::WindSide::Left;
        let other = solve_line(&line, &lib, &windy);
        let back = mid(&other[0]) - mid(&calm[0]);
        assert!(left_of(back) < 0.0, "from the left: blown to the right");
        // Not the same amount: at a line angle the strings already lean one
        // way (6.26 m against 5.80 m here).
        assert!(back.xy().norm() > 1.0);
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
        // The section's end: a strain assembly (one member) or a fixed point.
        assert!(calm.insulators[0].len() <= 1);
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

    /// A dead-end string given as geometry only (no weight): its weight is
    /// estimated so that it stays where the data puts it in the stringing
    /// state (S2: clipped in at 15 °C), and it moves with the weather.
    #[test]
    fn weightless_strain_strings_stay_put_at_stringing_and_move_with_the_weather() {
        let lib = library();
        let mut line = crate::tests::example();
        // The example's dead-end strings droop less (5.7°) than the wire
        // pulls them down (8.4°): no weight holds them there (estimated 0;
        // freed, they drop 0.23 m). Drooping 12°, as a weight can hold them:
        for (structure, drop) in [("T1", 1.06), ("T5", 1.07)] {
            let st = line
                .structures
                .iter_mut()
                .find(|s| s.id == structure)
                .unwrap();
            let a = st.attachments.iter_mut().find(|a| a.phase == 2).unwrap();
            a.wire_point[2] = a.insulator_point[2] - drop;
        }
        let s2 = line.sections.iter().find(|s| s.id == "S2").unwrap();
        let cable = lib.get(&s2.cable).unwrap();
        let solve = |case: &WeatherCase| {
            solve_section(&line, s2, cable, case, &default_fe_budget()).unwrap()
        };
        let data = v(line.attachment(&s2.points[0]).unwrap().wire_point);
        let at = |s: &SectionShape| v(s.clamps[0]);
        let stringing = solve(&WeatherCase::still("15", Temperature::from_celsius(15.0)));
        // Within a few mm: the member stretches under the wire's pull (1.6 mm
        // at 16 kN), and the data's string is not quite in line with it.
        assert!(
            (at(&stringing) - data).norm() < 5e-3,
            "{}",
            at(&stringing) - data
        );
        // A member: its force is about the conductor's tension.
        assert_eq!(stringing.insulators[0].len(), 1);
        // Hot: less tension, the string droops; wind: it swings sideways.
        let hot = solve(&WeatherCase::still("80", Temperature::from_celsius(80.0)));
        assert!(
            at(&hot).z < at(&stringing).z - 0.005,
            "{}",
            at(&hot).z - at(&stringing).z
        );
        let mut windy = WeatherCase::still("wind", Temperature::from_celsius(15.0));
        windy.wind_pressure = Stress::from_pascals(600.0);
        let blown = solve(&windy);
        assert!((at(&blown) - at(&stringing)).xy().norm() > 0.01);
    }

    /// The synthetic non-linear test cable (invented curves, with creep).
    fn with_nonlinear(mut lib: ols_cables::Library) -> ols_cables::Library {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../ols-sagtension/tests/data/synthetic-cable.olsw");
        lib.add_toml(&path, &std::fs::read_to_string(&path).unwrap())
            .unwrap();
        lib
    }

    fn max_sag(s: &SectionShape) -> f64 {
        s.spans.iter().map(|x| x.sag).fold(0.0, f64::max)
    }

    #[test]
    fn calculations_have_names() {
        use crate::condition::{Calculation, Condition, Method};
        for c in Calculation::ALL {
            assert_eq!(Calculation::parse(c.label()), Some(c));
            let json = serde_json::to_string(&c).unwrap();
            assert_eq!(serde_json::from_str::<Calculation>(&json).unwrap(), c);
        }
        assert_eq!(
            Calculation::parse("max sag fe"),
            Some(Calculation::MaxSagFe)
        );
        assert_eq!(Calculation::CreepRs.condition(), Condition::Creep);
        assert_eq!(Calculation::CreepRs.method(), Method::Rs);
        assert_eq!(
            serde_json::to_string(&Calculation::CreepRs).unwrap(),
            "\"creep-rs\""
        );
        assert!(Calculation::default().is_default());
    }

    /// The ruling span (one tension, clamps held) and the finite elements
    /// (strings swing) agree closely on a section strung by tension.
    #[test]
    fn ruling_span_and_finite_elements_agree() {
        use crate::condition::Calculation;
        let lib = library();
        let mut line = crate::tests::example();
        // Point loads (S1's marker ball) are the finite elements' only.
        line.sections[0].loads.clear();
        let cable = lib.get(&line.sections[0].cable).unwrap();
        let budget = default_fe_budget();
        for t in [15.0, 60.0] {
            let case = WeatherCase::still("t", Temperature::from_celsius(t));
            line.sections[0].calculation = Calculation::InitialFe;
            let fe = solve_section(&line, &line.sections[0], cable, &case, &budget).unwrap();
            line.sections[0].calculation = Calculation::InitialRs;
            let rs = solve_section(&line, &line.sections[0], cable, &case, &budget).unwrap();
            let (a, b) = (max_sag(&fe), max_sag(&rs));
            // Within 2 %: the FE strings swing to even out the spans'
            // tensions, RS holds the clamps (1.2 % apart at 60 °C).
            assert!((a - b).abs() < 0.02 * a, "{t} °C: FE {a} RS {b}");
            // One tension in RS.
            let h: Vec<f64> = rs.spans.iter().map(|x| x.horizontal_tension).collect();
            assert!(h.iter().all(|x| (x - h[0]).abs() < 1e-9));
            assert!(rs.spans.iter().all(|x| x.points.len() == 65));
        }
        // Wind blows the RS spans sideways.
        let mut windy = WeatherCase::still("w", Temperature::from_celsius(15.0));
        windy.wind_pressure = Stress::from_pascals(500.0);
        line.sections[0].calculation = Calculation::InitialRs;
        let calm = solve_section(
            &line,
            &line.sections[0],
            cable,
            &WeatherCase::still("c", Temperature::from_celsius(15.0)),
            &budget,
        )
        .unwrap();
        let blown = solve_section(&line, &line.sections[0], cable, &windy, &budget).unwrap();
        let shift = (v(blown.spans[1].mid_point) - v(calm.spans[1].mid_point))
            .xy()
            .norm();
        assert!(shift > 1.0, "{shift}");
    }

    /// Creep as a temperature increase (linear model): after creep at
    /// 15 °C sags as initial at 15 °C + the shift.
    #[test]
    fn creep_as_a_temperature_shift() {
        use crate::condition::Calculation;
        let lib = library();
        let mut cable = lib.get("acsr-drake-26-7").unwrap().clone();
        cable.model = ols_cables::CableModel::LinearCreepTemperature;
        cable.creep_temperature_shift = Some(20.0);
        let mut line = crate::tests::example();
        let every = WeatherCase::still("everyday", Temperature::from_celsius(15.0));
        line.creep_stretch.creep_case = Some("everyday".into());
        line.set_weather_cases(std::slice::from_ref(&every));
        let budget = default_fe_budget();
        line.sections[0].calculation = Calculation::CreepFe;
        let crept = solve_section(&line, &line.sections[0], &cable, &every, &budget).unwrap();
        line.sections[0].calculation = Calculation::InitialFe;
        let warm = solve_section(
            &line,
            &line.sections[0],
            &cable,
            &WeatherCase::still("35", Temperature::from_celsius(35.0)),
            &budget,
        )
        .unwrap();
        let (a, b) = (max_sag(&crept), max_sag(&warm));
        assert!((a - b).abs() < 0.005, "creep {a} vs +20 °C {b}");
        // No stretch after load (elastic).
        line.creep_stretch.load_cases = vec!["everyday".into()];
        line.set_weather_cases(std::slice::from_ref(&every));
        let f = final_state(
            &line,
            &line.sections[0],
            &cable,
            crate::condition::Condition::Load,
            &budget,
        )
        .unwrap();
        assert_eq!(f.stretch, 0.0);
    }

    /// The non-linear model: creep and a heavy load stretch the cable
    /// permanently, so it sags more than initially; Max Sag takes the
    /// larger; a condition without its weather case is said so.
    #[test]
    fn creep_and_load_stretch_a_nonlinear_cable() {
        use crate::condition::{Calculation, Condition};
        let lib = with_nonlinear(library());
        let cable = lib.get("synthetic").unwrap();
        let mut line = crate::tests::example();
        line.sections[0].cable = "synthetic".into();
        let budget = default_fe_budget();
        let every = WeatherCase::still("everyday", Temperature::from_celsius(15.0));
        let mut ice = WeatherCase::still("heavy ice", Temperature::from_celsius(-5.0));
        ice.ice_thickness = ols_core::units::Length::from_millimeters(15.0);
        // Without criteria: said so.
        line.sections[0].calculation = Calculation::CreepFe;
        let e = solve_section(&line, &line.sections[0], cable, &every, &budget).unwrap_err();
        assert!(e.to_string().contains("Creep-Stretch"), "{e}");
        line.creep_stretch.creep_case = Some("everyday".into());
        line.creep_stretch.load_cases = vec!["everyday".into(), "heavy ice".into()];
        line.set_weather_cases(&[every.clone(), ice.clone()]);
        let creep =
            final_state(&line, &line.sections[0], cable, Condition::Creep, &budget).unwrap();
        let load = final_state(&line, &line.sections[0], cable, Condition::Load, &budget).unwrap();
        assert!(creep.stretch > 1e-5, "{}", creep.stretch);
        assert!(load.stretch > 0.0, "{}", load.stretch);
        assert_eq!(load.case, "heavy ice", "the case stretching most");
        let hot = WeatherCase::still("hot", Temperature::from_celsius(50.0));
        let mut sag = |c: Calculation| {
            line.sections[0].calculation = c;
            max_sag(&solve_section(&line, &line.sections[0], cable, &hot, &budget).unwrap())
        };
        for (initial, creep, load, max) in [
            (
                Calculation::InitialFe,
                Calculation::CreepFe,
                Calculation::LoadFe,
                Calculation::MaxSagFe,
            ),
            (
                Calculation::InitialRs,
                Calculation::CreepRs,
                Calculation::LoadRs,
                Calculation::MaxSagRs,
            ),
        ] {
            let (i, c, l, m) = (sag(initial), sag(creep), sag(load), sag(max));
            assert!(
                c > i + 0.01 && l > i,
                "{initial:?}: initial {i} creep {c} load {l}"
            );
            assert!((m - c.max(l)).abs() < 1e-9, "max sag {m} of {c}, {l}");
        }
    }
}
