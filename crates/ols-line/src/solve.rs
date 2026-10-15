// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Solving sections of a line in 3D.
//!
//! Each section becomes an [`ols_wirefe::Model`]:
//!
//! - at every point of the section, what holds the wire is built as the
//!   structure gives it ([`crate::assembly`]): its structure points are
//!   fixed nodes, its wire points free nodes, its strings bars between them
//!   (tension only, or both ways when they may take compression); a wire
//!   point no string reaches is **fixed**. The usual shapes:
//!   - a **suspension string** (one string hanging from the structure, in
//!     the middle of the section): a free clamp on a bar of that length;
//!   - a **strain assembly** (one string at a section end, or not hanging):
//!     a free clamp on a member lined up with its conductor by
//!     equilibrium, so it moves with every weather case; one given without
//!     a weight (geometry only, e.g. imported) gets the weight that holds
//!     it where the data puts it in the stringing state
//!     (`estimate_strain_weights`);
//!   - a **V-string** (two strings to the clamp): a leg pushed into
//!     compression goes slack (reported in [`SectionShape::strings`]);
//!   - anything else: strings hanging from strings (a V with a string
//!     below, a TAK), each wire point a free node. Wire points of the same
//!     assembly where *other* sections end are held where they are given.
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
    /// The strings holding the wire at every point of the section, as
    /// solved (empty for a fixed point; none from the ruling span).
    pub strings: Vec<Vec<StringState>>,
    /// Weight estimated for each strain assembly given without one (the
    /// whole bundle's, N), at every point of the section (`None`: given or
    /// not a strain assembly).
    pub estimated_weights: Vec<Option<f64>>,
    /// Newton iterations used.
    pub iterations: u64,
    /// The loads the section puts on its structures (finite elements
    /// only; empty from the ruling span): at every point of the section,
    /// for each structure point holding it (or the wire point of a wire
    /// fixed to the structure), its label, where it is and the force on the
    /// structure, N (the whole bundle's; the support's reaction reversed).
    pub attachment_loads: Vec<Vec<Support>>,
    /// What to look at: strings beyond the compression they may take.
    pub flags: Vec<String>,
    /// The wires' pull on the clamp at every point of the section, N (the
    /// whole bundle's; finite elements only, else empty).
    pub pulls: Vec<Point>,
    /// The wire points of the strings holding the wire at every point, as
    /// solved: label, position (finite elements only, else empty).
    pub nodes: Vec<Vec<(String, Point)>>,
}

/// A string as solved.
#[derive(Debug, Clone, PartialEq)]
pub struct StringState {
    /// Its label on the structure.
    pub label: String,
    /// Its top, m.
    pub top: Point,
    /// Its bottom (wire point), m.
    pub bottom: Point,
    /// Axial force, N (the whole bundle's; tension positive).
    pub force: f64,
    /// Slack: a tension-only string pushed (it carries nothing).
    pub slack: bool,
    /// In more compression than it may take.
    pub over: bool,
}

/// A point of a structure the section pulls on.
#[derive(Debug, Clone, PartialEq)]
pub struct Support {
    /// Label of the structure point (or of the fixed wire point).
    pub label: String,
    /// Where it is, m.
    pub at: Point,
    /// Force on the structure, N (the whole bundle's).
    pub force: Point,
}

struct Built {
    model: Model,
    /// Node of each section point (clamp).
    clamps: Vec<usize>,
    /// For each span, the cable elements (in order).
    span_elements: Vec<Vec<usize>>,
    /// For each section point, its strings' trusses (with their labels).
    members: Vec<Vec<(String, usize)>>,
    /// For each section point, its fixed nodes on the structure (structure
    /// points, or the clamp of a wire fixed to the structure), labelled.
    supports: Vec<Vec<(String, usize)>>,
    /// For each section point, its holder's free nodes, labelled.
    free: Vec<Vec<(String, usize)>>,
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

/// Load per unit length on a wire with chord `d` where the line runs along
/// `along` (a plan unit vector): weight and ice down, and the case's wind
/// blowing across the line from its side, of which the wire takes the part
/// across itself (\\( \cos^2 \\) of the angle between the wind and the
/// wire's normal: all of it along the line, none across it). Without a
/// direction, as [`span_load`]. For wires whose own direction does not tell
/// the line's (jumpers).
pub(crate) fn wire_load_in_line_wind(
    case: &WeatherCase,
    cable: &Cable,
    d: Vector3<f64>,
    along: Option<[f64; 2]>,
) -> Vector3<f64> {
    let Some(a) = along else {
        return span_load(case, cable, d);
    };
    let load = case.wire_load(cable.diameter, cable.weight());
    let left = nalgebra::Vector2::new(-a[1], a[0]);
    let h = d.xy();
    // The wind across the wire: its part normal to the wire (|·| = sin of
    // the angle between them), scaled once more by that sine.
    let across = if h.norm() > 0.0 {
        let u = h / h.norm();
        left - u * left.dot(&u)
    } else {
        left
    };
    let push = across * across.norm() * (load.horizontal.si() * case.wind_from.to_left());
    Vector3::new(push.x, push.y, -load.vertical.si())
}

/// Stringing reference of a section: clamp positions and unstressed lengths
/// at a reference temperature.
#[derive(Clone)]
struct Reference {
    clamps: Vec<Vector3<f64>>,
    lengths: Vec<f64>,
    temperature: f64,
}

/// A node of a holder: fixed (on the structure, or another section's
/// clamp held where given) or free (a wire point on strings).
#[derive(Clone, Copy, PartialEq, Debug)]
enum At {
    Fixed(usize),
    Free(usize),
}

/// One string of a holder (weight and wind area per subconductor).
#[derive(Clone, Debug)]
struct Member {
    label: String,
    top: At,
    bottom: At,
    length: f64,
    weight: f64,
    wind_area: f64,
    stiffness: f64,
    /// Compression it may take, N (the whole bundle's); none: tension only.
    compression: Option<f64>,
}

/// The shape of a holder, for the passes that treat the usual ones as
/// they always were (`settled`, `plumb`).
#[derive(Clone, Copy, PartialEq, Debug)]
enum Shape {
    /// The wire fixed to the structure.
    Fixed,
    /// One string from the structure hanging below it, in the middle of the
    /// section.
    Suspension,
    /// One string from the structure, otherwise (a strain assembly).
    Strain,
    /// Two strings from the structure to the clamp, in the middle.
    VString,
    /// Anything else.
    General,
}

/// What holds the wire at one point of a section: the strings of its
/// wire point's assembly between fixed and free nodes.
#[derive(Clone, Debug)]
struct Holder {
    /// Fixed nodes: label, position, and for another section's clamp held
    /// where given (not on the structure), its set.
    fixed: Vec<(String, Vector3<f64>, Option<u32>)>,
    /// Free nodes: label, position given, load on it (another section's
    /// pull on its clamp, per subconductor).
    free: Vec<(String, Vector3<f64>, Vector3<f64>)>,
    clamp: At,
    members: Vec<Member>,
    shape: Shape,
    /// A strain assembly without a weight: weighed from where it is given.
    estimate: bool,
    /// This section reports the loads on the structure points (another
    /// section sharing the assembly comes first otherwise), and solves the
    /// assembly when coupled.
    reports: bool,
    /// Other sections end on the assembly too.
    shared: bool,
}

impl Holder {
    fn position(&self, at: At) -> Vector3<f64> {
        match at {
            At::Fixed(i) => self.fixed[i].1,
            At::Free(i) => self.free[i].1,
        }
    }

    /// The single string of a suspension or strain holder.
    fn single(&self) -> Option<&Member> {
        match self.members.as_slice() {
            [m] => Some(m),
            _ => None,
        }
    }
}

/// The holders of a section's points (weights and wind areas per
/// subconductor).
fn holders_of(line: &Line, section: &Section) -> Result<Vec<Holder>, LineError> {
    let n = section.points.len();
    let bundle = f64::from(section.bundle.max(1));
    let invalid = |m: String| LineError::Invalid(format!("section {}: {m}", section.id));
    section
        .points
        .iter()
        .enumerate()
        .map(|(k, point)| {
            let structure = line
                .structure(&point.structure)
                .ok_or_else(|| invalid(format!("no structure {}", point.structure)))?;
            let wire = structure.wire_at_set(point.set).ok_or_else(|| {
                invalid(format!(
                    "no wire point with set {} on {}",
                    point.set, point.structure
                ))
            })?;
            let middle = k > 0 && k < n - 1;
            let component = structure.component(&wire.label);
            if component.strings.is_empty() {
                return Ok(Holder {
                    fixed: vec![(wire.label.clone(), v(wire.position), None)],
                    free: Vec::new(),
                    clamp: At::Fixed(0),
                    members: Vec::new(),
                    shape: Shape::Fixed,
                    estimate: false,
                    reports: true,
                    shared: false,
                });
            }
            // Other sections' clamps in the assembly are held where given.
            let mut fixed: Vec<(String, Vector3<f64>, Option<u32>)> = Vec::new();
            let mut free: Vec<(String, Vector3<f64>, Vector3<f64>)> = Vec::new();
            let mut shares = false;
            // (Only where a section does end: a wire point with a set no
            // section uses is free like any other.)
            let used = |set: u32| {
                line.sections.iter().any(|s| {
                    s.points
                        .iter()
                        .any(|q| q.structure == structure.id && q.set == set)
                })
            };
            for &w in &component.wires {
                let x = &structure.wires[w];
                if x.label != wire.label && x.set.is_some_and(used) {
                    fixed.push((x.label.clone(), v(x.position), x.set));
                    shares = true;
                } else {
                    // A counterweight hanging at it (per subconductor).
                    let hanging = Vector3::new(0.0, 0.0, -x.weight / bundle);
                    free.push((x.label.clone(), v(x.position), hanging));
                }
            }
            let mut at = |label: &str| -> Result<At, LineError> {
                if let Some(i) = free.iter().position(|f| f.0 == label) {
                    return Ok(At::Free(i));
                }
                if let Some(i) = fixed.iter().position(|f| f.0 == label) {
                    return Ok(At::Fixed(i));
                }
                let q = structure
                    .point(label)
                    .ok_or_else(|| invalid(format!("no point {label} on {}", structure.id)))?;
                fixed.push((q.label.clone(), v(q.position), None));
                Ok(At::Fixed(fixed.len() - 1))
            };
            let mut members = Vec::new();
            for &i in &component.strings {
                let st = &structure.strings[i];
                let (top, bottom) = (at(&st.top)?, at(&st.wire)?);
                members.push((st, top, bottom));
            }
            let clamp = At::Free(
                free.iter()
                    .position(|f| f.0 == wire.label)
                    .expect("the wire point is free"),
            );
            let members: Vec<Member> = members
                .into_iter()
                .map(|(st, top, bottom)| Member {
                    label: st.label.clone(),
                    top,
                    bottom,
                    length: structure.string_length(st).unwrap_or(0.0),
                    weight: st.weight / bundle,
                    wind_area: st.wind_area / bundle,
                    stiffness: st.stiffness.unwrap_or(DEFAULT_INSULATOR_STIFFNESS),
                    compression: st.compression,
                })
                .collect();
            let from_structure = |m: &Member| matches!(m.top, At::Fixed(i) if fixed[i].2.is_none());
            let shape = match members.as_slice() {
                [m] if from_structure(m) && m.bottom == clamp => {
                    let top = match m.top {
                        At::Fixed(i) => fixed[i].1,
                        At::Free(_) => unreachable!(),
                    };
                    // Flagged a dead end (strain): a tension string even
                    // where the wire runs on.
                    let flagged = structure.string(&m.label).is_some_and(|s| s.strain);
                    if middle && !flagged && is_suspension(top, v(wire.position)) {
                        Shape::Suspension
                    } else {
                        Shape::Strain
                    }
                }
                [a, b]
                    if middle
                        && from_structure(a)
                        && from_structure(b)
                        && a.bottom == clamp
                        && b.bottom == clamp =>
                {
                    Shape::VString
                }
                _ => Shape::General,
            };
            // A strain assembly given only as geometry, without a weight,
            // would line up with its conductor and lose the droop the data
            // shows (freed without weights, a 930-span comparison got worse:
            // median 1.1 → 4.2 cm): its weight is estimated from where the
            // data puts it.
            let estimate = shape == Shape::Strain && {
                let st = structure
                    .string(&members[0].label)
                    .expect("the holder's string");
                st.insulator.is_none() && !st.strain && st.weight == 0.0
            };
            // A shared assembly's loads go with the first section through it.
            let reports = !shares || {
                let sets: Vec<u32> = component
                    .wires
                    .iter()
                    .filter_map(|&w| structure.wires[w].set)
                    .collect();
                let first = line.sections.iter().find(|x| {
                    x.points
                        .iter()
                        .any(|q| q.structure == structure.id && sets.contains(&q.set))
                });
                first.is_none_or(|x| x.id == section.id)
            };
            Ok(Holder {
                fixed,
                free,
                clamp,
                members,
                shape,
                estimate,
                reports,
                shared: shares,
            })
        })
        .collect()
}

/// The holders as coupled: where the section solves a shared assembly,
/// other sections' clamps in it freed and pulled as `coupling.pulls` gives
/// (the whole bundle's pull of those wires on their clamp; without one,
/// held where given); where another section solves it, the clamp held
/// where `coupling.held` puts it.
fn pulled(layout: &[Holder], section: &Section, coupling: &Coupling) -> Vec<Holder> {
    let bundle = f64::from(section.bundle.max(1));
    layout
        .iter()
        .zip(&section.points)
        .map(|(h, point)| {
            if h.shared
                && !h.reports
                && let Some(at) = coupling.held.get(point)
            {
                return Holder {
                    fixed: vec![(h.free[0].0.clone(), v(*at), Some(point.set))],
                    free: Vec::new(),
                    clamp: At::Fixed(0),
                    members: Vec::new(),
                    shape: Shape::Fixed,
                    estimate: false,
                    reports: false,
                    shared: true,
                };
            }
            let freed: Vec<(usize, Vector3<f64>)> = h
                .fixed
                .iter()
                .enumerate()
                .filter_map(|(i, (_, _, other))| {
                    let set = (*other)?;
                    let pull = coupling
                        .pulls
                        .get(&crate::SectionPoint::new(point.structure.clone(), set))?;
                    Some((i, v(*pull) / bundle))
                })
                .collect();
            if freed.is_empty() {
                return h.clone();
            }
            let mut out = h.clone();
            out.fixed.clear();
            let mut map_fixed = Vec::new();
            for (i, f) in h.fixed.iter().enumerate() {
                match freed.iter().find(|(j, _)| *j == i) {
                    Some((_, load)) => {
                        out.free.push((f.0.clone(), f.1, *load));
                        map_fixed.push(At::Free(out.free.len() - 1));
                    }
                    None => {
                        out.fixed.push(f.clone());
                        map_fixed.push(At::Fixed(out.fixed.len() - 1));
                    }
                }
            }
            let remap = |at: At| match at {
                At::Fixed(i) => map_fixed[i],
                free => free,
            };
            out.clamp = remap(h.clamp);
            for m in &mut out.members {
                m.top = remap(m.top);
                m.bottom = remap(m.bottom);
            }
            out
        })
        .collect()
}

/// How a section sharing assemblies with others is solved with them
/// ([`solve_coupled`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Coupling {
    /// The other sections' wires' pull on their clamps (the whole bundle's),
    /// by section point: on the assemblies this section solves.
    pub pulls: std::collections::BTreeMap<crate::SectionPoint, Point>,
    /// Where this section's clamps are held, by section point: on the
    /// assemblies another section solves.
    pub held: std::collections::BTreeMap<crate::SectionPoint, Point>,
    /// The jumpers' load on clamps in the weather case, by section point
    /// (the whole bundle's, N, a force; [`crate::jumper::end_loads`], as
    /// the line's [`crate::JumperLoads`] says). None: from the jumpers,
    /// each taken as of this section's cable.
    pub jumpers: Option<std::collections::BTreeMap<crate::SectionPoint, [f64; 3]>>,
}

/// The clamp's position to start from: plumb below a suspension string's
/// top, else where it is given.
fn plumb(holder: &Holder) -> Vector3<f64> {
    match (holder.shape, holder.single()) {
        (Shape::Suspension, Some(m)) => holder.position(m.top) - Vector3::new(0.0, 0.0, m.length),
        _ => holder.position(holder.clamp),
    }
}

/// Builds the finite-element model of a section in a weather state.
#[allow(clippy::too_many_arguments)]
fn build(
    section: &Section,
    layout: &[Holder],
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
    let mut supports = Vec::with_capacity(n);
    let mut frees = Vec::with_capacity(n);
    let mut free_labels: Vec<Vec<(String, usize)>> = Vec::with_capacity(n);
    for (k, holder) in layout.iter().enumerate() {
        // Free nodes start where given, moved with the clamp to where the
        // reference puts it.
        let shift = reference.clamps[k] - holder.position(holder.clamp);
        let fixed_nodes: Vec<usize> = holder
            .fixed
            .iter()
            .map(|(_, position, _)| {
                model.nodes.push(Node::fixed(*position));
                model.nodes.len() - 1
            })
            .collect();
        let free_nodes: Vec<usize> = holder
            .free
            .iter()
            .map(|(_, position, load)| {
                let mut node = Node::free(position + shift);
                node.load = *load;
                model.nodes.push(node);
                model.nodes.len() - 1
            })
            .collect();
        let node = |at: At| match at {
            At::Fixed(i) => fixed_nodes[i],
            At::Free(i) => free_nodes[i],
        };
        clamps.push(node(holder.clamp));
        supports.push(
            holder
                .fixed
                .iter()
                .zip(&fixed_nodes)
                .filter(|((_, _, other), _)| other.is_none())
                .map(|((label, _, _), &i)| (label.clone(), i))
                .collect(),
        );
        frees.push(free_nodes.clone());
        free_labels.push(
            holder
                .free
                .iter()
                .zip(&free_nodes)
                .map(|((label, _, _), &i)| (label.clone(), i))
                .collect(),
        );
        members.push(
            holder
                .members
                .iter()
                .map(|m| {
                    model.trusses.push(TrussElement {
                        i: node(m.top),
                        j: node(m.bottom),
                        unstressed_length: m.length,
                        axial_stiffness: m.stiffness,
                        tension_only: m.compression.is_none(),
                        weight: m.weight,
                    });
                    (m.label.clone(), model.trusses.len() - 1)
                })
                .collect(),
        );
    }

    // Wind on the strings: half of each string's on each of its free ends,
    // across the line there.
    for (k, holder) in layout.iter().enumerate() {
        if case.wind_pressure.si() <= 0.0 {
            break;
        }
        let along = reference.clamps[(k + 1).min(n - 1)] - reference.clamps[k.saturating_sub(1)];
        let horizontal = along.xy();
        if horizontal.norm() == 0.0 {
            continue;
        }
        let transverse = Vector3::new(-horizontal.y, horizontal.x, 0.0) / horizontal.norm();
        for m in holder.members.iter().filter(|m| m.wind_area > 0.0) {
            let push = transverse
                * (0.5 * case.wind_pressure.si() * m.wind_area * case.wind_from.to_left());
            for end in [m.top, m.bottom] {
                if let At::Free(i) = end {
                    model.nodes[frees[k][i]].load += push;
                }
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
        let mut sites: Vec<(f64, Vector3<f64>)> = Vec::new();
        let mut shape = None;
        if with_loads && section.loads.iter().any(|l| l.span == s) {
            let solved = shape_props
                .solve(d, None, &element_budget)
                .map_err(|e| invalid(format!("span {s}: {e}")))?;
            // The wind across the span, from the case's side.
            let horizontal = d.xy();
            let left = if horizontal.norm() > 0.0 {
                Vector3::new(-horizontal.y, horizontal.x, 0.0) / horizontal.norm()
                    * case.wind_from.to_left()
            } else {
                Vector3::zeros()
            };
            sites = section
                .loads
                .iter()
                .filter(|l| l.span == s)
                .map(|l| {
                    let [down, across] = l.forces(case, cable.diameter.si());
                    (
                        shape_props.arc_length_at_fraction(d, &solved, l.fraction),
                        // The model is one subconductor: its share.
                        (left * across - Vector3::new(0.0, 0.0, down))
                            / f64::from(section.bundle.max(1)),
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
                model.nodes[prev_node].load += force;
                continue;
            }
            if reference.lengths[s] - s0 < LUMP_AT_CLAMP {
                // Next to the ahead clamp: on it.
                model.nodes[clamps[s + 1]].load += force;
                continue;
            }
            let node = model.nodes.len();
            let start =
                reference.clamps[s] + shape_props.point_at(d, shape.as_ref().expect("solved"), s0);
            let mut site = Node::free(start);
            site.load = force;
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
        supports,
        free: free_labels,
    })
}

/// Builds and solves a section model.
fn solve_built(
    section: &Section,
    layout: &[Holder],
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
    mut layout: Vec<Holder>,
    cable: &Cable,
    budget: &Budget,
) -> Result<(Vec<Holder>, Vec<Option<f64>>), LineError> {
    let mut estimated = vec![None; layout.len()];
    if !layout.iter().any(|h| h.estimate) {
        return Ok((layout, estimated));
    }
    // Those clamps held where the data puts them.
    let held: Vec<Holder> = layout
        .iter()
        .map(|h| {
            if h.estimate {
                Holder {
                    fixed: vec![(String::new(), h.position(h.clamp), Some(0))],
                    free: Vec::new(),
                    clamp: At::Fixed(0),
                    members: Vec::new(),
                    shape: Shape::Fixed,
                    estimate: false,
                    reports: false,
                    shared: false,
                }
            } else {
                h.clone()
            }
        })
        .collect();
    let reference = reference(section, &held, cable, budget)?;
    let case = WeatherCase::still(
        "stringing",
        ols_core::units::Temperature::from_celsius(reference.temperature),
    );
    let (built, solution) = solve_built(section, &held, &reference, cable, &case, true, budget)?;
    for (k, holder) in layout.iter_mut().enumerate() {
        if !holder.estimate {
            continue;
        }
        let (top, initial) = (
            holder.position(holder.members[0].top),
            holder.position(holder.clamp),
        );
        let member = &mut holder.members[0];
        // The wires' pull on the clamp: the support's reaction, reversed.
        let pull = -solution.reactions[built.clamps[k]];
        let r = initial - top;
        let moment = r.cross(&pull);
        let per_newton = (r / 2.0).cross(&Vector3::new(0.0, 0.0, -1.0));
        let norm = per_newton.norm_squared();
        member.weight = if norm > 0.0 {
            (-moment.dot(&per_newton) / norm).max(0.0)
        } else {
            0.0
        };
        // The data's length is the loaded one: unstressed, the member is
        // shorter by its stretch (5 mm on a 5 m string at 50 kN; left in,
        // the slack cut short tight spans' tension by up to 4 % on a real
        // line).
        let force = pull.dot(&r.normalize()) + member.weight / 2.0 * (-r.z / r.norm());
        member.length /= 1.0 + force.max(0.0) / member.stiffness;
        holder.estimate = false;
        estimated[k] = Some(member.weight * f64::from(section.bundle.max(1)));
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
    layout: &[Holder],
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
        clamps = settled(layout, &clamps, &built, &solved);
        // Converged when the clamps used for the next pass (after the
        // transverse-only projection) stop moving.
        let moved = clamps
            .iter()
            .zip(&previous)
            .map(|(new, old)| (new - old).norm())
            .fold(0.0, worst);
        if moved < 1e-6 {
            return Ok(reference);
        }
    }
    Err(invalid(
        "stringing equilibrium did not settle in 20 passes".into(),
    ))
}

/// The larger of two distances, NaN counting as infinite (a pass that went
/// wrong never looks settled).
fn worst(m: f64, d: f64) -> f64 {
    if d.is_nan() { f64::INFINITY } else { m.max(d) }
}

/// The clamps for the next pass of a fixed point: the solved positions,
/// with suspension and V-strings swung back across the line — along it the
/// pulls of equal tensions cancel at any position (a neutral direction the
/// iteration would creep along), and the crew clamps in plumb along it.
fn settled(
    layout: &[Holder],
    clamps: &[Vector3<f64>],
    built: &Built,
    solution: &ols_wirefe::Solution,
) -> Vec<Vector3<f64>> {
    let n = clamps.len();
    (0..n)
        .map(|k| {
            let position = solution.positions[built.clamps[k]];
            // Line direction at the clamp: the bisector of the two spans'
            // horizontal directions (where their equal pulls cancel).
            let unit = |a: usize, b: usize| {
                let d = (clamps[b] - clamps[a]).xy();
                if d.norm() > 0.0 { d.normalize() } else { d }
            };
            let along = unit(k.saturating_sub(1), k) + unit(k, (k + 1).min(n - 1));
            let along = Vector3::new(along.x, along.y, 0.0).normalize();
            let holder = &layout[k];
            let middle = k > 0 && k + 1 < n;
            match holder.shape {
                Shape::Suspension => {
                    let top = holder.position(holder.members[0].top);
                    let swing = position - top;
                    let transverse_only = swing - along * swing.dot(&along);
                    top + transverse_only * (swing.norm() / transverse_only.norm())
                }
                // A V-string swings freely along the line, about the axis
                // through its leg tops: the same neutral direction, removed
                // by rotating the clamp about that axis back into the plane
                // across the line.
                Shape::VString => {
                    let tops = [0, 1].map(|i| holder.position(holder.members[i].top));
                    let axis = (tops[1] - tops[0]).normalize();
                    let centre = (tops[0] + tops[1]) / 2.0;
                    let r = position - centre;
                    let radial = r - axis * r.dot(&axis);
                    let along = (along - axis * along.dot(&axis)).normalize();
                    let across = radial - along * radial.dot(&along);
                    centre + axis * r.dot(&axis) + across * (radial.norm() / across.norm())
                }
                // Strings hanging from strings: where the assembly hangs
                // under the clamp's load with its part along the line taken
                // away (the same neutral direction).
                Shape::General if middle => {
                    across_only(built, solution, k, along).unwrap_or(position)
                }
                _ => position,
            }
        })
        .collect()
}

/// The wires' pull on the clamp of section point `k` in a solution (per
/// subconductor): what balances the strings, the loads and the support
/// there.
fn clamp_pull(built: &Built, solution: &ols_wirefe::Solution, k: usize) -> Vector3<f64> {
    let c = built.clamps[k];
    let node = &built.model.nodes[c];
    if node.fixed {
        return -solution.reactions[c];
    }
    let mut on_clamp = node.load;
    for (_, t) in &built.members[k] {
        let truss = &built.model.trusses[*t];
        if truss.i != c && truss.j != c {
            continue;
        }
        let other = if truss.i == c { truss.j } else { truss.i };
        let d = solution.positions[other] - solution.positions[c];
        on_clamp += d.normalize() * solution.truss_forces[*t];
        on_clamp.z -= truss.weight / 2.0;
    }
    -on_clamp
}

/// Where a holder's clamp hangs under the wires' pull in a solution
/// (`k`: its section point), less that pull's part along the line: the
/// assembly alone, from its solved shape, under its own loads and that
/// pull. None if it does not solve.
fn across_only(
    built: &Built,
    solution: &ols_wirefe::Solution,
    k: usize,
    along: Vector3<f64>,
) -> Option<Vector3<f64>> {
    let c = built.clamps[k];
    let trusses: Vec<usize> = built.members[k].iter().map(|(_, t)| *t).collect();
    let pull = clamp_pull(built, solution, k);
    let pull = pull - along * pull.dot(&along);
    // The assembly alone: its nodes (renumbered), its strings.
    let mut nodes: Vec<usize> = Vec::new();
    for &t in &trusses {
        for i in [built.model.trusses[t].i, built.model.trusses[t].j] {
            if !nodes.contains(&i) {
                nodes.push(i);
            }
        }
    }
    let index = |i: usize| nodes.iter().position(|&n| n == i).expect("listed");
    let mut model = Model::default();
    for &i in &nodes {
        let mut node = built.model.nodes[i];
        node.position = solution.positions[i];
        if i == c {
            node.load += pull;
        }
        model.nodes.push(node);
    }
    for &t in &trusses {
        let mut truss = built.model.trusses[t];
        (truss.i, truss.j) = (index(truss.i), index(truss.j));
        model.trusses.push(truss);
    }
    let solved = model.solve(&default_fe_budget()).ok()?;
    Some(solved.positions[index(c)])
}

/// The horizontal tension giving sag `sag` (m) in span `span` between the
/// `clamps` (none: in the ruling span \\( \sqrt{\sum S^3 / \sum S} \\), level),
/// for weight `w` (N/m) ([`ols_sagtension::catenary::tension_for_sag`]).
pub(crate) fn tension_for_sag(
    clamps: &[Vector3<f64>],
    span: Option<usize>,
    sag: f64,
    w: f64,
) -> Result<f64, String> {
    use ols_core::units::{ForcePerLength, Length};
    use ols_sagtension::catenary::{SpanGeometry, default_solver_budget};
    let spans: Vec<SpanGeometry> = clamps
        .windows(2)
        .map(|c| SpanGeometry {
            span: Length::from_si((c[1] - c[0]).xy().norm()),
            rise: Length::from_si(c[1].z - c[0].z),
        })
        .collect();
    let geometry = match span {
        Some(k) => *spans.get(k).ok_or_else(|| format!("no span {}", k + 1))?,
        None => SpanGeometry::level(
            ols_sagtension::section::ruling_span(&spans).map_err(|e| e.to_string())?,
        ),
    };
    ols_sagtension::catenary::tension_for_sag(
        geometry,
        ForcePerLength::from_si(w),
        Length::from_si(sag),
        &default_solver_budget(),
    )
    .map(|c| c.horizontal_tension.si())
    .map_err(|e| e.to_string())
}

/// A section in the blocks in a weather case: its unstressed length (the
/// sum of the reference lengths) shared out between the spans so that all
/// have the same horizontal tension — the wire rolls over the suspension
/// clamps (point loads are left out of the sharing). Bisection on the
/// tension; exit clause: 200 steps.
fn rolled(
    section: &Section,
    layout: &[Holder],
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
        let clamps = settled(layout, &r.clamps, &built, &solved);
        let moved = clamps
            .iter()
            .zip(&r.clamps)
            .map(|(a, b)| (a - b).norm())
            .fold(0.0, worst);
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
    let (layout, _) = estimate_strain_weights(section, holders_of(line, section)?, cable, &budget)?;
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
    solve_section_coupled(line, section, cable, case, budget, &Coupling::default())
}

/// [`solve_section`] sharing assemblies with other sections (a TAK:
/// [`solve_coupled`]).
pub fn solve_section_coupled(
    line: &Line,
    section: &Section,
    cable: &Cable,
    case: &WeatherCase,
    budget: &Budget,
    coupling: &Coupling,
) -> Result<SectionShape, LineError> {
    use crate::condition::{Condition, Method};
    // The line's bimetallic model for cables that do not give their own.
    let cable = &*line.bimetallic.apply(cable);
    // The wind's side is the line's: a section whose points run against
    // the line's direction is solved with the side swapped, so its spans
    // blow to the same side as those entered along it.
    let flipped;
    let case = if against_the_line(line, section) {
        flipped = WeatherCase {
            wind_from: case.wind_from.opposite(),
            ..case.clone()
        };
        &flipped
    } else {
        case
    };
    let calc = section.calculation;
    let solve = |final_state: Option<&FinalState>| match calc.method() {
        Method::Fe => solve_fe(line, section, cable, case, final_state, budget, coupling),
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
            // Only the conditions whose weather case is given; any other
            // failure is the section's.
            let cases = &line.condition_cases;
            let shapes: Vec<SectionShape> = [
                (Condition::Creep, cases.creep.is_some()),
                (Condition::Load, !cases.loads.is_empty()),
            ]
            .into_iter()
            .filter(|(_, given)| *given)
            .map(|(c, _)| solve(Some(&final_state(line, section, cable, c, budget)?)))
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
    let (layout, _) = estimate_strain_weights(section, holders_of(line, section)?, cable, budget)?;
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
    let cable = &*line.bimetallic.apply(cable);
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
    // The permanent stretch is measured at the stringing tension: at zero
    // tension, outer strands that take no compression are slack and only
    // the core's set would count.
    let working_strain = |m: &MaterialState| {
        mechanics
            .strain(Force::from_si(h), strung_state.temperature, m)
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
                    let stretch =
                        working_strain(&material)? - working_strain(&MaterialState::Initial)?;
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
                let stretch = (working_strain(&material)?
                    - working_strain(&MaterialState::Initial)?)
                .max(0.0);
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
        strings: vec![Vec::new(); n],
        iterations: 0,
        estimated_weights: vec![None; n],
        attachment_loads: Vec::new(),
        flags: Vec::new(),
        pulls: Vec::new(),
        nodes: Vec::new(),
    })
}

/// The cable's stress-strain curves, when its model uses them (the
/// non-linear model with curves given).
fn curves_of(cable: &Cable) -> Option<ols_cables::mechanics::Mechanics> {
    if cable.model != ols_cables::CableModel::Nonlinear {
        return None;
    }
    ols_cables::mechanics::Mechanics::from_cable(cable)
        .ok()
        .filter(|m| m.is_nonlinear())
}

/// Mean tension of each span's elements (weighted by their unstressed
/// lengths), N.
fn span_tensions(built: &Built, solution: &ols_wirefe::Solution) -> Vec<f64> {
    built
        .span_elements
        .iter()
        .map(|elements| {
            let (mut sum, mut length) = (0.0, 0.0);
            for &e in elements {
                let l = built.model.cables[e].properties.unstressed_length;
                let st = &solution.cables[e];
                sum += 0.5 * (st.tension_i + st.tension_j) * l;
                length += l;
            }
            if length > 0.0 { sum / length } else { 0.0 }
        })
        .collect()
}

/// Passes of the secant iteration on the stress-strain curves (exit
/// clause; it converges in 3–6).
const CURVE_PASSES: usize = 30;

/// Finite-element solution of a section in `case`, in the permanent
/// state `final_state` (none: initial).
///
/// Cables with stress-strain curves (non-linear model): the elements stay
/// elastic catenaries with the cable's EA, and each span's unstressed
/// length is set so that, at the span's mean tension, its length is the
/// curves' in the case's material state (initial, after creep or after
/// load: the permanent set comes with it) — a secant iteration on the
/// spans' tensions, re-solved until those lengths stop changing
/// (\\( 10^{-9} \\) relative, at most [`CURVE_PASSES`] solves). The stringing
/// state fixes each span's length at zero strain. Other cables: linear,
/// longer by the final state's permanent stretch.
fn solve_fe(
    line: &Line,
    section: &Section,
    cable: &Cable,
    case: &WeatherCase,
    final_state: Option<&FinalState>,
    budget: &Budget,
    coupling: &Coupling,
) -> Result<SectionShape, LineError> {
    let (layout, estimated_weights) =
        estimate_strain_weights(section, holders_of(line, section)?, cable, budget)?;
    let mut reference = reference(section, &layout, cable, budget)?;
    // In the weather case, other sections' clamps in the assemblies pulled
    // as given (held in the stringing state).
    let mut loaded = pulled(&layout, section, coupling);
    // The jumpers hanging on its clamps, in the weather case (as the
    // line's calculation options say).
    let jumpers = match &coupling.jumpers {
        Some(j) => std::borrow::Cow::Borrowed(j),
        None => std::borrow::Cow::Owned(crate::jumper::end_loads(line, case, |id| {
            (id == section.cable).then_some(cable)
        })),
    };
    let bundle = f64::from(section.bundle.max(1));
    for (h, point) in loaded.iter_mut().zip(&section.points) {
        if let (Some(f), At::Free(i)) = (jumpers.get(point), h.clamp) {
            h.free[i].2 += Vector3::new(f[0], f[1], f[2]) / bundle;
        }
    }
    let curves = curves_of(cable);
    // Curves: each span's tension in the stringing state (still air at the
    // reference temperature), where the reference lengths are cut.
    let strung = match &curves {
        Some(_) => {
            let stringing = WeatherCase::still(
                "stringing",
                ols_core::units::Temperature::from_celsius(reference.temperature),
            );
            let (b, sol) = solve_built(
                section, &layout, &reference, cable, &stringing, false, budget,
            )?;
            Some(span_tensions(&b, &sol))
        }
        None => None,
    };
    // Linear: stretched before rolling: in the blocks, the longer wire is
    // shared out again so that the spans keep one horizontal tension.
    let stretch = match (&curves, final_state) {
        (None, Some(f)) => f.stretch,
        _ => 0.0,
    };
    if stretch != 0.0 {
        for l in &mut reference.lengths {
            *l *= 1.0 + stretch;
        }
    }
    if !section.stringing.clipped() && reference.lengths.len() > 1 {
        reference = rolled(section, &loaded, &reference, cable, case, budget)?;
    }
    let (mut built, mut solution) =
        solve_built(section, &loaded, &reference, cable, case, true, budget)?;
    if let (Some(m), Some(strung)) = (&curves, &strung) {
        use ols_cables::mechanics::MaterialState;
        use ols_core::units::{Force, Temperature};
        let material = final_state.map_or(MaterialState::Initial, |f| f.material.clone());
        let ea = cable.axial_stiffness().si();
        let thermal = (cable.thermal_expansion()
            * (case.temperature.celsius() - reference.temperature))
            .exp();
        let t_ref = Temperature::from_celsius(reference.temperature);
        let strain = |t: f64, at: Temperature, state: &MaterialState| {
            m.strain(Force::from_si(t), at, state)
                .map_err(|e| LineError::Invalid(format!("section {}: {e}", section.id)))
        };
        // Physical length per unit reference length at stringing, and the
        // curves' strain there (the zero-strain length between them).
        let at_stringing: Vec<(f64, f64)> = strung
            .iter()
            .map(|&t| Ok((1.0 + t / ea, strain(t, t_ref, &MaterialState::Initial)?)))
            .collect::<Result<_, LineError>>()?;
        let base = reference.lengths.clone();
        let mut factors = vec![1.0; base.len()];
        let mut settled = false;
        for _ in 0..CURVE_PASSES {
            let tensions = span_tensions(&built, &solution);
            let mut change: f64 = 0.0;
            for (s, &t) in tensions.iter().enumerate() {
                let (stretched, eps_ref) = at_stringing[s];
                let eps = strain(t, case.temperature, &material)?;
                let k = stretched * (1.0 + eps) / ((1.0 + eps_ref) * thermal * (1.0 + t / ea));
                change = change.max(((k - factors[s]) / factors[s]).abs());
                factors[s] = k;
            }
            if change < 1e-9 {
                settled = true;
                break;
            }
            for (l, (b, k)) in reference.lengths.iter_mut().zip(base.iter().zip(&factors)) {
                *l = b * k;
            }
            (built, solution) =
                solve_built(section, &loaded, &reference, cable, case, true, budget)?;
        }
        if !settled {
            return Err(LineError::Invalid(format!(
                "section {}: the stress-strain iteration did not settle in {CURVE_PASSES} solves",
                section.id
            )));
        }
    } else if let Some((core, offset)) = core_branch(cable, case, &reference, &built, &solution) {
        // Bimetallic cables without curves: past the transition, outer
        // strands limited in compression leave the core alone
        // ([`core_branch`]).
        for l in &mut reference.lengths {
            *l *= 1.0 + offset;
        }
        (built, solution) = solve_built(section, &loaded, &reference, &core, case, true, budget)?;
    }
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
        let sag = crate::jumper::sag_below_chord(&points, a, b);
        spans.push(SpanShape {
            points: points.iter().map(|q| p(*q)).collect(),
            horizontal_tension: solution.cables[elements[0]].horizontal,
            max_tension,
            mid_point: p(mid),
            low_point: p(low),
            sag,
        });
    }
    // Forces of the whole bundle (the model is one subconductor).
    let bundle = f64::from(section.bundle.max(1));
    let mut flags = Vec::new();
    let strings: Vec<Vec<StringState>> = built
        .members
        .iter()
        .zip(&loaded)
        .enumerate()
        .map(|(k, (trusses, holder))| {
            trusses
                .iter()
                .zip(&holder.members)
                .map(|((label, t), m)| {
                    let truss = &built.model.trusses[*t];
                    let force = solution.truss_forces[*t] * bundle;
                    let (top, bottom) = (pos[truss.i], pos[truss.j]);
                    let slack = truss.tension_only
                        && (bottom - top).norm() < truss.unstressed_length * (1.0 - 1e-9);
                    let over = m.compression.is_some_and(|c| -force > c);
                    if over {
                        flags.push(format!(
                            "{} string {label}: {:.0} N of compression, more than the {:.0} N it may take",
                            section.points[k].structure,
                            -force,
                            m.compression.unwrap_or(0.0)
                        ));
                    }
                    StringState {
                        label: label.clone(),
                        top: p(top),
                        bottom: p(bottom),
                        force,
                        slack,
                        over,
                    }
                })
                .collect()
        })
        .collect();
    Ok(SectionShape {
        section: section.id.clone(),
        clamps: built.clamps.iter().map(|&c| p(pos[c])).collect(),
        spans,
        strings,
        iterations: solution.iterations,
        estimated_weights,
        attachment_loads: built
            .supports
            .iter()
            .zip(&loaded)
            .map(|(nodes, holder)| {
                if !holder.reports {
                    return Vec::new();
                }
                nodes
                    .iter()
                    .map(|(label, n)| Support {
                        label: label.clone(),
                        at: p(pos[*n]),
                        force: p(-solution.reactions[*n] * bundle),
                    })
                    .collect()
            })
            .collect(),
        flags,
        pulls: (0..built.clamps.len())
            .map(|k| p(clamp_pull(&built, &solution, k) * bundle))
            .collect(),
        nodes: built
            .free
            .iter()
            .map(|nodes| {
                nodes
                    .iter()
                    .map(|(label, n)| (label.clone(), p(pos[*n])))
                    .collect()
            })
            .collect(),
    })
}

/// The core branch of a bimetallic cable, when its outer strands are in
/// more compression than they may carry (the cable's bimetallic
/// behaviour) at the mean tension of the composite solution: the
/// cable as the core alone (outer strands without stiffness: the core's
/// \\( EA \\) and expansion), and the extra strain of the outer strands'
/// constant compressive load \\( L \\): \\( \varepsilon = (T + L) / EA_c +
/// \alpha_c \Delta t \\). One branch per section (its tension varies
/// little).
fn core_branch(
    cable: &Cable,
    case: &WeatherCase,
    reference: &Reference,
    built: &Built,
    solution: &ols_wirefe::Solution,
) -> Option<(Cable, f64)> {
    // The same limit as the ruling-span path and the jumpers (curves
    // included).
    let limit = cable.outer_compression_limit_with_curves();
    let core_part = cable.core.as_ref()?;
    if !limit.is_finite() {
        return None;
    }
    let elements: Vec<usize> = built.span_elements.iter().flatten().copied().collect();
    let tension = elements
        .iter()
        .map(|&e| 0.5 * (solution.cables[e].tension_i + solution.cables[e].tension_j))
        .sum::<f64>()
        / elements.len().max(1) as f64;
    let dt = case.temperature.celsius() - reference.temperature;
    let strain = tension / cable.axial_stiffness().si() + cable.thermal_expansion() * dt;
    let outer = cable.outer.axial_stiffness().si() * (strain - cable.outer.thermal_expansion * dt);
    if outer >= -limit {
        return None;
    }
    let mut core = cable.clone();
    core.outer.modulus = ols_core::units::Stress::ZERO;
    Some((core, limit / core_part.axial_stiffness().si()))
}

/// Whether a section's points run against the line's direction (its first
/// to last wire point, against the line's direction at their middle).
fn against_the_line(line: &Line, section: &Section) -> bool {
    let ends = (section.points.first(), section.points.last());
    let (Some(a), Some(b)) = ends else {
        return false;
    };
    let (Some(a), Some(b)) = (line.wire_point(a), line.wire_point(b)) else {
        return false;
    };
    let (a, b) = (a.position, b.position);
    let mid = [0.5 * (a[0] + b[0]), 0.5 * (a[1] + b[1])];
    line.direction_at(mid)
        .is_some_and(|d| (b[0] - a[0]) * d[0] + (b[1] - a[1]) * d[1] < 0.0)
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
/// Sections sharing an assembly are solved together ([`solve_coupled`]).
pub fn solve_line(
    line: &Line,
    library: &ols_cables::Library,
    case: &WeatherCase,
) -> Vec<Result<SectionShape, LineError>> {
    solve_line_with(line, library, case, &default_fe_budget())
}

/// [`solve_line`] with the exit clauses of `budget` for each solve. The
/// jumpers on the clamps as the line's [`crate::JumperLoads`] says: their
/// estimated loads, or their own pull ([`jumper_pull`]).
pub fn solve_line_with(
    line: &Line,
    library: &ols_cables::Library,
    case: &WeatherCase,
    budget: &Budget,
) -> Vec<Result<SectionShape, LineError>> {
    let estimate = crate::jumper::end_loads(line, case, |id| library.get(id));
    let mut results = solve_sections_with(line, library, case, budget, &estimate);
    if line.solver.jumper_loads == crate::JumperLoads::Pull {
        jumper_pull(
            line,
            library,
            case,
            budget,
            &estimate,
            estimate.clone(),
            &mut results,
        );
    }
    results
}

/// Jumper loads on clamps, by section point (the whole bundle's, N).
pub type JumperLoadMap = std::collections::BTreeMap<crate::SectionPoint, [f64; 3]>;

/// Every section, the jumpers' loads `jumpers` on their clamps, spread
/// over the cores (sections are independent finite-element models);
/// sections sharing an assembly then together.
fn solve_sections_with(
    line: &Line,
    library: &ols_cables::Library,
    case: &WeatherCase,
    budget: &Budget,
    jumpers: &JumperLoadMap,
) -> Vec<Result<SectionShape, LineError>> {
    let coupling = Coupling {
        jumpers: Some(jumpers.clone()),
        ..Coupling::default()
    };
    let all: Vec<&Section> = line.sections.iter().collect();
    let mut results = crate::jumper::in_parallel(&all, |section| {
        let cable = cable_of(library, section)?;
        solve_section_coupled(line, section, cable, case, budget, &coupling)
    });
    for group in coupled_groups(line) {
        solve_coupled(line, library, case, &group, &mut results, budget, jumpers);
    }
    results
}

/// Passes of the jumpers' pull (exit clause; the clamps settle in two or
/// three).
pub const JUMPER_PULL_PASSES: usize = 8;

/// The jumpers' own pull on the clamps ([`crate::JumperLoads::Pull`]):
/// the jumpers solved between the solved clamps, the force each exerts
/// on its clamps ([`crate::jumper::JumperShape::end_forces`]) put on the
/// sections in place of the estimate, the sections whose clamps it
/// changes solved again (with their groups), and so on until it changes
/// less than 0.5 N (or 1e-4 of itself). A clamp whose jumpers did not all
/// solve keeps the estimate. `used`: the loads the sections in `results`
/// were solved with; returns those of the last pass.
///
/// Expected cost: per pass, the jumpers and the sections at them solved
/// again (two or three passes: about twice to three times the time of the
/// sections at jumpers; the others are not touched).
pub fn jumper_pull(
    line: &Line,
    library: &ols_cables::Library,
    case: &WeatherCase,
    budget: &Budget,
    estimate: &JumperLoadMap,
    mut used: JumperLoadMap,
    results: &mut [Result<SectionShape, LineError>],
) -> JumperLoadMap {
    if line.jumpers.is_empty() {
        return used;
    }
    let groups = coupled_groups(line);
    for _ in 0..JUMPER_PULL_PASSES {
        let shapes: Vec<SectionShape> = results
            .iter()
            .filter_map(|r| r.as_ref().ok().cloned())
            .collect();
        let solved = crate::jumper::solve_jumpers(line, library, case, &shapes);
        // The pull on each clamp: the sum of its jumpers', where all of
        // them solved; else the estimate.
        let mut next = estimate.clone();
        let mut failed: std::collections::BTreeSet<&crate::SectionPoint> = Default::default();
        let mut pulled: JumperLoadMap = Default::default();
        for (j, r) in line.jumpers.iter().zip(&solved) {
            match r {
                Ok(s) => {
                    for (p, f) in [(&j.from, s.end_forces[0]), (&j.to, s.end_forces[1])] {
                        let e = pulled.entry(p.clone()).or_insert([0.0; 3]);
                        for c in 0..3 {
                            e[c] += f[c];
                        }
                    }
                }
                Err(_) => {
                    failed.insert(&j.from);
                    failed.insert(&j.to);
                }
            }
        }
        for (p, f) in pulled {
            if !failed.contains(&p) {
                next.insert(p, f);
            }
        }
        // The sections whose clamps' loads changed.
        let moved = |p: &crate::SectionPoint| {
            let a = v(next.get(p).copied().unwrap_or([0.0; 3]));
            let b = v(used.get(p).copied().unwrap_or([0.0; 3]));
            (a - b).norm() > 0.5_f64.max(1e-4 * a.norm())
        };
        let again: Vec<usize> = line
            .sections
            .iter()
            .enumerate()
            .filter(|(_, s)| s.points.iter().any(moved))
            .map(|(k, _)| k)
            .collect();
        used = next;
        if again.is_empty() {
            return used;
        }
        let coupling = Coupling {
            jumpers: Some(used.clone()),
            ..Coupling::default()
        };
        // Over the cores.
        let solved = crate::jumper::in_parallel(&again, |&k| {
            let section = &line.sections[k];
            cable_of(library, section).and_then(|cable| {
                solve_section_coupled(line, section, cable, case, budget, &coupling)
            })
        });
        for (&k, r) in again.iter().zip(solved) {
            results[k] = r;
        }
        for group in groups
            .iter()
            .filter(|g| g.iter().any(|k| again.contains(k)))
        {
            solve_coupled(line, library, case, group, results, budget, &used);
        }
    }
    used
}

fn cable_of<'a>(
    library: &'a ols_cables::Library,
    section: &Section,
) -> Result<&'a Cable, LineError> {
    library.get(&section.cable).ok_or_else(|| {
        LineError::Invalid(format!(
            "section {}: cable \"{}\" not in the libraries",
            section.id, section.cable
        ))
    })
}

/// The groups of sections (indices) whose assemblies are joined — a
/// section ending on a string that another one's string hangs from too (a
/// TAK) — with more than one section each.
pub fn coupled_groups(line: &Line) -> Vec<Vec<usize>> {
    let n = line.sections.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    // The section ending at each (structure, set).
    let mut at: std::collections::BTreeMap<(&str, u32), Vec<usize>> = Default::default();
    for (k, sec) in line.sections.iter().enumerate() {
        for p in &sec.points {
            at.entry((p.structure.as_str(), p.set)).or_default().push(k);
        }
    }
    for (k, sec) in line.sections.iter().enumerate() {
        for p in &sec.points {
            let Some(s) = line.structure(&p.structure) else {
                continue;
            };
            let Some(w) = s.wire_at_set(p.set) else {
                continue;
            };
            let c = s.component(&w.label);
            for &i in &c.wires {
                let Some(set) = s.wires[i].set else { continue };
                for &j in at.get(&(p.structure.as_str(), set)).into_iter().flatten() {
                    let (a, b) = (find(&mut parent, k), find(&mut parent, j));
                    if a != b {
                        parent[a.max(b)] = a.min(b);
                    }
                }
            }
        }
    }
    let mut groups: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
    for k in 0..n {
        let r = find(&mut parent, k);
        groups.entry(r).or_default().push(k);
    }
    groups.into_values().filter(|g| g.len() > 1).collect()
}

/// Passes of the coupling between sections sharing an assembly (exit
/// clause; a TAK settles in a few).
pub const COUPLING_PASSES: usize = 30;

/// Solves a group of sections sharing assemblies together (Dirichlet–
/// Neumann): the first section through a shared assembly solves it, the
/// other sections' wires pulling on their clamps in it; the others hold
/// their clamp where that solution puts it, which gives their pull for
/// the next pass — half way from the last one (the pulls overshoot
/// otherwise). Until the pulls change less than 0.1 N (or 1e-6 of
/// themselves). `results` holds the sections' solutions, solved alone,
/// and gets the coupled ones; a section that fails keeps its error, the
/// others then stay as solved alone. Not settling in
/// [`COUPLING_PASSES`] passes is an error for the group.
///
/// Expected cost: a few solves of each section of the group.
pub fn solve_coupled(
    line: &Line,
    library: &ols_cables::Library,
    case: &WeatherCase,
    group: &[usize],
    results: &mut [Result<SectionShape, LineError>],
    budget: &Budget,
    jumpers: &JumperLoadMap,
) {
    type Points = std::collections::BTreeMap<crate::SectionPoint, Point>;
    let pulls_of = |results: &[Result<SectionShape, LineError>]| -> Option<Points> {
        let mut pulls = Points::new();
        for &k in group {
            let shape = results[k].as_ref().ok()?;
            for (p, f) in line.sections[k].points.iter().zip(&shape.pulls) {
                pulls.insert(p.clone(), *f);
            }
        }
        Some(pulls)
    };
    // Where the solved assemblies put the clamps of the group's points.
    let held_of = |results: &[Result<SectionShape, LineError>]| -> Points {
        let mut held = Points::new();
        for &k in group {
            let Ok(shape) = &results[k] else { continue };
            for (p, nodes) in line.sections[k].points.iter().zip(&shape.nodes) {
                let Some(s) = line.structure(&p.structure) else {
                    continue;
                };
                for (label, at) in nodes {
                    if let Some(set) = s.wire(label).and_then(|w| w.set)
                        && set != p.set
                    {
                        held.insert(crate::SectionPoint::new(p.structure.clone(), set), *at);
                    }
                }
            }
        }
        held
    };
    let Some(mut pulls) = pulls_of(results) else {
        return;
    };
    for _ in 0..COUPLING_PASSES {
        for &k in group {
            let section = &line.sections[k];
            let coupling = Coupling {
                pulls: pulls.clone(),
                held: held_of(results),
                jumpers: Some(jumpers.clone()),
            };
            results[k] = cable_of(library, section).and_then(|cable| {
                solve_section_coupled(line, section, cable, case, budget, &coupling)
            });
        }
        let Some(next) = pulls_of(results) else {
            return;
        };
        let change = next
            .iter()
            .map(|(p, f)| {
                let old = pulls.get(p).copied().unwrap_or([0.0; 3]);
                (v(*f) - v(old)).norm() / (0.1 + 1e-6 * v(*f).norm())
            })
            .fold(0.0, f64::max);
        if change < 1.0 {
            return;
        }
        for (p, f) in next {
            let old = pulls.get(&p).copied().unwrap_or(f);
            pulls.insert(p, [0, 1, 2].map(|c| 0.5 * (old[c] + f[c])));
        }
    }
    for &k in group {
        results[k] = Err(LineError::Invalid(format!(
            "section {}: the sections sharing its strings did not settle together in \
             {COUPLING_PASSES} passes",
            line.sections[k].id
        )));
    }
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
            .map(|p| v(line.wire_point(p).unwrap().position))
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
        let mut single = crate::tests::example();
        // Jumpers hang their weight on the clamps after stringing.
        single.jumpers.clear();
        let mut bundled = single.clone();
        bundled.sections[0].bundle = 3;
        bundled.sections[0].bundle_spacing = Some(0.4);
        for s in &mut bundled.structures {
            for a in &mut s.strings {
                a.weight *= 3.0;
            }
        }
        // Point loads are on the whole bundle too.
        for l in &mut bundled.sections[0].loads {
            l.vertical *= 3.0;
        }
        bundled.validate().unwrap();
        let a = solve_line(&single, &lib, &windy).remove(0).unwrap();
        let b = solve_line(&bundled, &lib, &windy).remove(0).unwrap();
        for (p, q) in a.clamps.iter().zip(&b.clamps) {
            assert!((v(*p) - v(*q)).norm() < 1e-9, "{p:?} {q:?}");
        }
        for (f, g) in a.strings.iter().flatten().zip(b.strings.iter().flatten()) {
            let (f, g) = (f.force, g.force);
            assert!((3.0 * f - g).abs() < 1e-6 * g.abs().max(1.0), "{f} {g}");
        }
        assert!(a.strings.iter().flatten().count() > 0);
    }

    #[test]
    fn strings_catch_wind_and_bundles_share_point_loads() {
        let lib = library();
        let still = WeatherCase::still("still", Temperature::from_celsius(15.0));
        let mut windy = still.clone();
        windy.wind_pressure = Stress::from_pascals(400.0);
        let mut line = crate::tests::example();
        // Jumpers hang their weight on the clamps after stringing.
        line.jumpers.clear();
        assert!(!line.sections[0].loads.is_empty());
        // Wind on the strings swings the clamps further.
        let mut catching = line.clone();
        for s in &mut catching.structures {
            for a in &mut s.strings {
                a.wind_area = 0.5;
            }
        }
        let rest = solve_line(&line, &lib, &still).remove(0).unwrap();
        let a = solve_line(&line, &lib, &windy).remove(0).unwrap();
        let b = solve_line(&catching, &lib, &windy).remove(0).unwrap();
        let n = rest.clamps.len();
        let mut further = 0;
        for k in 1..n - 1 {
            let (r, p, q) = (v(rest.clamps[k]), v(a.clamps[k]), v(b.clamps[k]));
            assert!((q - r).norm() >= (p - r).norm() - 1e-9, "clamp {k}");
            further += usize::from((q - r).norm() > (p - r).norm() + 1e-4);
        }
        assert!(further > 0);
        // A 3-bundle under 3 × each point load hangs like one wire.
        let mut bundled = line.clone();
        bundled.sections[0].bundle = 3;
        bundled.sections[0].bundle_spacing = Some(0.4);
        for l in &mut bundled.sections[0].loads {
            l.vertical *= 3.0;
        }
        for s in &mut bundled.structures {
            for a in &mut s.strings {
                a.weight *= 3.0;
            }
        }
        let c = solve_line(&bundled, &lib, &still).remove(0).unwrap();
        for (p, q) in rest.clamps.iter().zip(&c.clamps) {
            assert!((v(*p) - v(*q)).norm() < 1e-6, "{p:?} {q:?}");
        }
    }

    /// S1's marker ball, given a size, swings its span further in the
    /// wind and sags it more under ice; in still air it changes nothing.
    #[test]
    fn a_sized_marker_ball_catches_wind_and_ice() {
        let lib = library();
        let still = WeatherCase::still("still", Temperature::from_celsius(0.0));
        let windy = WeatherCase {
            wind_pressure: Stress::from_pascals(650.0),
            ..still.clone()
        };
        let icy = WeatherCase {
            ice_thickness: Length::from_millimeters(15.0),
            ..still.clone()
        };
        let line = crate::tests::example();
        let ball = &line.sections[0].loads[0];
        let span = ball.span;
        let mut sized = line.clone();
        sized.sections[0].loads[0].diameter = 0.6;
        let solve = |l: &Line, c: &WeatherCase| solve_line(l, &lib, c).remove(0).unwrap();
        let mid = |s: &SectionShape| v(s.spans[span].mid_point);
        let rest = mid(&solve(&line, &still));
        assert!((mid(&solve(&sized, &still)) - rest).norm() < 1e-9);
        let (a, b) = (mid(&solve(&line, &windy)), mid(&solve(&sized, &windy)));
        assert!(
            (b - rest).xy().norm() > (a - rest).xy().norm() + 0.01,
            "{a} {b}"
        );
        let (a, b) = (mid(&solve(&line, &icy)), mid(&solve(&sized, &icy)));
        assert!(b.z < a.z - 0.01, "{a} {b}");
    }

    /// The synthetic line with T2's string at set 1 (a 3.5 m I-string,
    /// line along x) replaced by a V-string: two strings from points ±3 m
    /// across the line (`VL`, `VR`), 500 N each.
    fn with_v_string() -> Line {
        let mut line = crate::tests::example();
        // Jumpers hang their weight on the clamps after stringing.
        line.jumpers.clear();
        let t2 = line.structures.iter_mut().find(|s| s.id == "T2").unwrap();
        let wire = t2.wire_at_set(1).unwrap().label.clone();
        let i = t2.strings.iter().position(|s| s.wire == wire).unwrap();
        let c = t2.node(&t2.strings[i].top).unwrap().position();
        t2.strings.remove(i);
        for (label, dy) in [("VL", -3.0), ("VR", 3.0)] {
            t2.points.push(crate::StructurePoint {
                description: Default::default(),
                label: label.into(),
                position: [c[0], c[1] + dy, c[2]],
            });
            t2.strings.push(crate::InsulatorString {
                label: format!("leg {label}"),
                top: label.into(),
                wire: wire.clone(),
                weight: 500.0,
                ..Default::default()
            });
        }
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
        let forces = |s: &SectionShape, k: usize| -> Vec<f64> {
            s.strings[k].iter().map(|x| x.force).collect()
        };
        assert_eq!(calm.strings[1].len(), 2);
        assert!(
            forces(&calm, 1).iter().all(|&f| f > 1000.0),
            "{:?}",
            forces(&calm, 1)
        );
        // The section's end: a strain assembly (one member) or a fixed point.
        assert!(calm.strings[0].len() <= 1);
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
            forces(&blown, 1).iter().all(|&f| f > 0.0),
            "{:?}",
            forces(&blown, 1)
        );
        let mut i_line = crate::tests::example();
        // Jumpers hang their weight on the clamps after stringing.
        i_line.jumpers.clear();
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
        let legs = &gale.strings[1];
        assert!(
            legs.iter().any(|l| l.slack && l.force == 0.0) && legs.iter().any(|l| l.force > 1000.0),
            "{legs:?}"
        );
        assert!(across(&gale, &calm) > 0.1);
        let taut = legs.iter().find(|l| !l.slack).unwrap();
        let windward = v(taut.top);
        let radius = (v(gale.clamps[1]) - windward).norm();
        assert!(
            (radius - (3.0f64.powi(2) + 3.5f64.powi(2)).sqrt()).abs() < 2e-3,
            "{radius}"
        );
    }

    /// The synthetic line with T2's string at set 1 replaced by a V
    /// (points `VL`, `VR` ±3 m across the line, at the string's top) to a
    /// tip `V` 0.5 m above the clamp, and a 0.5 m string from the tip down
    /// to the clamp: a V-string with a string below.
    fn with_v_and_string_below() -> Line {
        let mut line = crate::tests::example();
        let t2 = line.structures.iter_mut().find(|s| s.id == "T2").unwrap();
        let wire = t2.wire_at_set(1).unwrap().clone();
        let i = t2
            .strings
            .iter()
            .position(|s| s.wire == wire.label)
            .unwrap();
        let c = t2.node(&t2.strings[i].top).unwrap().position();
        t2.strings.remove(i);
        let w = wire.position;
        t2.wires.push(crate::WirePoint {
            label: "V".into(),
            position: [w[0], w[1], w[2] + 0.5],
            ..Default::default()
        });
        for (label, dy) in [("VL", -3.0), ("VR", 3.0)] {
            t2.points.push(crate::StructurePoint {
                description: Default::default(),
                label: label.into(),
                position: [c[0], c[1] + dy, c[2]],
            });
            t2.strings.push(crate::InsulatorString {
                label: format!("leg {label}"),
                top: label.into(),
                wire: "V".into(),
                weight: 400.0,
                wind_area: 0.2,
                ..Default::default()
            });
        }
        t2.strings.push(crate::InsulatorString {
            label: "below".into(),
            top: "V".into(),
            wire: wire.label.clone(),
            weight: 100.0,
            ..Default::default()
        });
        line.sections[0].loads.clear();
        line.validate().unwrap();
        line
    }

    /// A V with a string below: all three strings carry the wire in still
    /// air; in the wind the V's tip stays put across the line while the
    /// string below swings; with the section clipped in, a heavy load in
    /// one span pulls the clamp along the line (the V swings about its
    /// tops) — a clamp held by a short string alone would hardly move.
    #[test]
    fn a_string_hanging_from_a_v_swings_below_it() {
        let lib = library();
        let case = |pa: f64| WeatherCase {
            wind_pressure: Stress::from_pascals(pa),
            ..WeatherCase::still("wind", Temperature::from_celsius(15.0))
        };
        let line = with_v_and_string_below();
        let solve = |line: &Line, pa: f64| solve_line(line, &lib, &case(pa)).remove(0).unwrap();
        let calm = solve(&line, 0.0);
        let holder = &calm.strings[1];
        assert_eq!(holder.len(), 3);
        assert!(holder.iter().all(|s| s.force > 100.0), "{holder:?}");
        let tip = |s: &SectionShape| {
            v(s.strings[1]
                .iter()
                .find(|x| x.label == "below")
                .unwrap()
                .top)
        };
        // Hangs as given (the clamp 0.5 m below the tip).
        assert!(((tip(&calm) - v(calm.clamps[1])).norm() - 0.5).abs() < 1e-3);
        // A set on the tip that no section uses (an export's own number
        // for the V) leaves it free: the legs carry the wire.
        let mut numbered = line.clone();
        let t2 = numbered
            .structures
            .iter_mut()
            .find(|s| s.id == "T2")
            .unwrap();
        t2.wires.iter_mut().find(|w| w.label == "V").unwrap().set = Some(99);
        numbered.validate().unwrap();
        let n = solve(&numbered, 0.0);
        assert!(
            n.strings[1].iter().all(|s| s.force > 100.0),
            "{:?}",
            n.strings[1]
        );
        assert!((v(n.clamps[1]) - v(calm.clamps[1])).norm() < 1e-6);
        let blown = solve(&line, 400.0);
        let across = |a: Vector3<f64>, b: Vector3<f64>| (a.y - b.y).abs();
        assert!(
            across(tip(&blown), tip(&calm)) < 2e-3,
            "the V holds its tip"
        );
        let swing = across(v(blown.clamps[1]), v(calm.clamps[1]));
        assert!(
            swing > 0.05 && swing < 0.5,
            "the string below swings {swing} m"
        );

        // Clipped in, then a 3 kN load in the middle of the first span.
        let mut clipped = line.clone();
        let cable = lib.get(&clipped.sections[0].cable).unwrap();
        let (lengths, temperature) = clip_lengths(&clipped, &clipped.sections[0], cable).unwrap();
        clipped.sections[0].stringing = Stringing::UnstressedLengths {
            lengths,
            temperature,
            sagged: None,
        };
        let before = solve(&clipped, 0.0);
        clipped.sections[0].loads.push(crate::SpanLoad {
            span: 0,
            fraction: 0.5,
            vertical: 3000.0,
            label: "load".into(),
            ..Default::default()
        });
        let after = solve(&clipped, 0.0);
        let along = (v(after.clamps[1]) - v(before.clamps[1])).x;
        assert!(along < -0.05, "pulled towards the loaded span: {along} m");
    }

    /// A counterweight at a suspension clamp: the string swings less in
    /// the wind, the string carries the weight more.
    #[test]
    fn a_counterweight_holds_the_clamp_down() {
        let lib = library();
        let windy = WeatherCase {
            wind_pressure: Stress::from_pascals(500.0),
            ..WeatherCase::still("wind", Temperature::from_celsius(15.0))
        };
        let mut line = crate::tests::example();
        line.sections[0].loads.clear();
        let plain = solve_line(&line, &lib, &windy).remove(0).unwrap();
        let t2 = line.structures.iter_mut().find(|s| s.id == "T2").unwrap();
        let w = t2.wires.iter_mut().find(|w| w.set == Some(1)).unwrap();
        w.weight = 3000.0;
        line.validate().unwrap();
        let held = solve_line(&line, &lib, &windy).remove(0).unwrap();
        let swing = |s: &SectionShape| {
            let top = v(s.strings[1][0].top);
            let d = v(s.clamps[1]) - top;
            d.xy().norm() / -d.z
        };
        assert!(
            swing(&held) < 0.8 * swing(&plain),
            "{} {}",
            swing(&held),
            swing(&plain)
        );
        assert!(held.strings[1][0].force > plain.strings[1][0].force + 2400.0);
    }

    /// V legs that may take some compression: the leeward leg pushed in a
    /// gale holds the clamp (less swing than a tension-only V) and is
    /// flagged when pushed beyond what it may take.
    #[test]
    fn a_v_leg_may_take_compression_and_is_flagged_beyond_it() {
        let lib = library();
        let gale = WeatherCase {
            wind_pressure: Stress::from_pascals(800.0),
            ..WeatherCase::still("gale", Temperature::from_celsius(15.0))
        };
        let tension_only = with_v_string();
        let with = |limit: f64| {
            let mut line = with_v_string();
            let t2 = line.structures.iter_mut().find(|s| s.id == "T2").unwrap();
            for s in t2.strings.iter_mut().filter(|s| s.label.starts_with("leg")) {
                s.compression = Some(limit);
            }
            solve_line(&line, &lib, &gale).remove(0).unwrap()
        };
        let free = solve_line(&tension_only, &lib, &gale).remove(0).unwrap();
        let strut = with(1e9);
        let pushed = strut.strings[1]
            .iter()
            .map(|s| s.force)
            .fold(f64::INFINITY, f64::min);
        assert!(pushed < -10.0, "a leg in compression: {pushed} N");
        assert!(strut.flags.is_empty(), "{:?}", strut.flags);
        let swing = |s: &SectionShape| (s.clamps[1][1] - free.clamps[1][1]).abs();
        assert!(swing(&strut) > 0.05, "the strut holds the clamp back");
        let weak = with(-pushed / 2.0);
        assert!(weak.strings[1].iter().any(|s| s.over));
        assert_eq!(weak.flags.len(), 1, "{:?}", weak.flags);
        assert!(weak.flags[0].contains("compression"));
    }

    /// A TAK: a suspension string from the structure down to a node, two
    /// strain strings from that node to the ends of two sections. The
    /// sections pull it towards the tighter one; solved together they agree
    /// on where it is, and differ from either solved alone with the other's
    /// clamp held.
    #[test]
    fn a_tak_couples_the_sections_on_it() {
        let cable = crate::tests::example().sections[0].cable.clone();
        let text = format!(
            r#"{{
            "name": "tak", "structures": [
              {{"id": "A", "position": [0, 0, 0], "wires": [{{"label": "W", "position": [0, 0, 40], "set": 1}}]}},
              {{"id": "B", "position": [300, 0, 0],
                "points": [{{"label": "K", "position": [300, 0, 45]}}],
                "wires": [
                  {{"label": "N", "position": [300, 0, 40]}},
                  {{"label": "EB", "position": [297, 0, 40], "set": 1}},
                  {{"label": "EF", "position": [303, 0, 40], "set": 2}}],
                "strings": [
                  {{"label": "k", "top": "K", "wire": "N", "weight": 500}},
                  {{"label": "b", "top": "N", "wire": "EB", "weight": 300}},
                  {{"label": "f", "top": "N", "wire": "EF", "weight": 300}}]}},
              {{"id": "C", "position": [600, 0, 0], "wires": [{{"label": "W", "position": [600, 0, 40], "set": 1}}]}}],
            "sections": [
              {{"id": "X", "cable": "{cable}", "points": [{{"structure": "A", "set": 1}}, {{"structure": "B", "set": 1}}],
                "stringing": {{"kind": "tension", "horizontal_tension": 20000, "temperature": 15}}}},
              {{"id": "Y", "cable": "{cable}", "points": [{{"structure": "B", "set": 2}}, {{"structure": "C", "set": 1}}],
                "stringing": {{"kind": "tension", "horizontal_tension": 16000, "temperature": 15}}}}]
            }}"#
        );
        let line = Line::from_json(&text).unwrap();
        assert_eq!(coupled_groups(&line), vec![vec![0, 1]]);
        let lib = library();
        let case = WeatherCase::still("15", Temperature::from_celsius(15.0));
        let node = |s: &SectionShape, k: usize| {
            v(s.strings[k].iter().find(|x| x.label == "k").unwrap().bottom)
        };
        let budget = default_fe_budget();
        let alone: Vec<SectionShape> = line
            .sections
            .iter()
            .map(|s| solve_section(&line, s, lib.get(&cable).unwrap(), &case, &budget).unwrap())
            .collect();
        let together: Vec<SectionShape> = solve_line(&line, &lib, &case)
            .into_iter()
            .map(Result::unwrap)
            .collect();
        // X (first) solves the TAK, Y's wires pulling on its end; Y holds
        // its end where X puts it.
        let x = node(&together[0], 1);
        let end_of_y = v(together[0].strings[1]
            .iter()
            .find(|s| s.label == "f")
            .unwrap()
            .bottom);
        assert!((end_of_y - v(together[1].clamps[0])).norm() < 1e-9);
        assert!(together[1].strings[0].is_empty());
        // Pulled towards the tighter section X (towards A, −x), the strain
        // strings in line with the sections.
        assert!(x.x < 300.0 - 0.05, "{x}");
        assert!((node(&alone[0], 1) - x).norm() > 0.01);
        // In balance: X's model pulls Y's end as hard as Y's wires do.
        let pull_y = v(together[1].pulls[0]);
        let f = together[0].strings[1]
            .iter()
            .find(|s| s.label == "f")
            .unwrap();
        let along_f = (v(f.bottom) - v(f.top)).normalize();
        assert!(
            (pull_y.dot(&along_f) - f.force).abs() < 0.01 * f.force + 50.0,
            "{} vs {}",
            pull_y.dot(&along_f),
            f.force
        );
        // The structure's loads: once, by X.
        assert!(!together[0].attachment_loads[1].is_empty());
        assert!(together[1].attachment_loads[0].is_empty());
    }

    #[test]
    fn tension_stringing_reproduces_its_tension() {
        let mut line = crate::tests::example();
        // Jumpers hang their weight on the clamps after stringing.
        line.jumpers.clear();
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

    /// Sagged to 8 m in its second span: solving in the stringing state
    /// gives that sag there, and the same tension in every span.
    #[test]
    fn sag_stringing_reproduces_its_sag() {
        let mut line = crate::tests::example();
        // Jumpers hang their weight on the clamps after stringing.
        line.jumpers.clear();
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
        let ends: Vec<crate::SectionPoint> = {
            let s2 = line.sections.iter().find(|s| s.id == "S2").unwrap();
            vec![s2.points[0].clone(), s2.points.last().unwrap().clone()]
        };
        for (end, drop) in ends.iter().zip([1.06, 1.07]) {
            let st = line
                .structures
                .iter_mut()
                .find(|s| s.id == end.structure)
                .unwrap();
            let wire = st.wire_at_set(end.set).unwrap().label.clone();
            let top = st.strings_to(&wire).next().unwrap().top.clone();
            let z = st.node(&top).unwrap().position()[2] - drop;
            st.wires
                .iter_mut()
                .find(|w| w.label == wire)
                .unwrap()
                .position[2] = z;
        }
        let s2 = line.sections.iter().find(|s| s.id == "S2").unwrap();
        let cable = lib.get(&s2.cable).unwrap();
        let solve = |case: &WeatherCase| {
            solve_section(&line, s2, cable, case, &default_fe_budget()).unwrap()
        };
        let data = v(line.wire_point(&s2.points[0]).unwrap().position);
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
        assert_eq!(stringing.strings[0].len(), 1);
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

    /// The loads on the structures balance the section: their sum is the
    /// weight of the wires (and strings) hanging from them; one support per
    /// string top, the wind pushes them sideways.
    #[test]
    fn attachment_loads_balance_the_section() {
        let lib = library();
        let mut line = crate::tests::example();
        line.sections[0].loads.clear();
        let cable = lib.get(&line.sections[0].cable).unwrap();
        let budget = default_fe_budget();
        let still = WeatherCase::still("t", Temperature::from_celsius(15.0));
        let shape = solve_section(&line, &line.sections[0], cable, &still, &budget).unwrap();
        assert_eq!(shape.attachment_loads.len(), line.sections[0].points.len());
        let bundle = f64::from(line.sections[0].bundle.max(1));
        let wires: f64 = shape
            .spans
            .iter()
            .map(|s| {
                s.points
                    .windows(2)
                    .map(|w| (v(w[1]) - v(w[0])).norm())
                    .sum::<f64>()
            })
            .sum::<f64>()
            * cable.weight().si()
            * bundle;
        let down: f64 = shape
            .attachment_loads
            .iter()
            .flatten()
            .map(|l| -l.force[2])
            .sum();
        assert!(down >= wires * 0.999, "{down} vs wires {wires}");
        assert!(down < wires * 1.2, "{down} vs wires {wires}");
        // Wind: the structures are pushed with it.
        let mut windy = still.clone();
        windy.wind_pressure = Stress::from_pascals(500.0);
        let blown = solve_section(&line, &line.sections[0], cable, &windy, &budget).unwrap();
        let side = |s: &SectionShape| -> nalgebra::Vector2<f64> {
            s.attachment_loads
                .iter()
                .flatten()
                .map(|l| nalgebra::Vector2::new(l.force[0], l.force[1]))
                .sum()
        };
        assert!((side(&blown) - side(&shape)).norm() > 100.0);
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

    /// Bimetallic cables: above the transition temperature, outer strands
    /// taking no compression (bird cage) leave the core alone, expanding
    /// less than the composite: a hot ACSR sags less than with the
    /// aluminium in compression; in both methods.
    #[test]
    fn bird_cage_changes_the_hot_sag() {
        use crate::condition::Calculation;
        let lib = library();
        let budget = default_fe_budget();
        let hot = WeatherCase::still("hot", Temperature::from_celsius(180.0));
        let sag = |line: &Line, cable: &Cable| {
            max_sag(&solve_section(line, &line.sections[0], cable, &hot, &budget).unwrap())
        };
        let mut less = vec![];
        for calculation in [Calculation::InitialRs, Calculation::InitialFe] {
            let mut cable = lib.get("acsr-drake-26-7").unwrap().clone();
            let mut line = crate::tests::example();
            line.sections[0].loads.clear();
            line.sections[0].calculation = calculation;
            let free = sag(&line, &cable);
            // The line's choice for a cable without its own …
            line.bimetallic.behavior = ols_cables::BimetallicBehavior::BirdCage;
            let by_line = sag(&line, &cable);
            // … and the cable's own, which wins over the line's.
            cable.bimetallic_behavior = Some(ols_cables::BimetallicBehavior::Compression);
            assert!(
                (sag(&line, &cable) - free).abs() < 1e-9,
                "the cable's own wins"
            );
            line.bimetallic = Default::default();
            cable.bimetallic_behavior = Some(ols_cables::BimetallicBehavior::BirdCage);
            let caged = sag(&line, &cable);
            assert!((by_line - caged).abs() < 1e-9, "{by_line} vs {caged}");
            assert!(
                caged < free - 0.1,
                "{calculation:?}: bird cage {caged} vs {free}"
            );
            less.push(free - caged);
        }
        // Both methods take the same off (the sags themselves differ by
        // 2 % at 180 °C: RS holds the clamps).
        assert!(
            (less[0] - less[1]).abs() < 0.1 * less[0],
            "less sag: RS {} FE {}",
            less[0],
            less[1]
        );
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

    /// Cables with curves: the finite elements follow the stress-strain
    /// curves (secant iteration), so FE and RS agree on a span where both
    /// hold — one section with no strings swinging (a dead-end to
    /// dead-end span in still air): within the thermal forms' difference.
    #[test]
    fn finite_elements_follow_the_curves() {
        use crate::condition::Calculation;
        let lib = with_nonlinear(library());
        let cable = lib.get("synthetic").unwrap();
        let mut line = crate::tests::example();
        // The branch S3: two dead-end spans… take its first span alone.
        line.sections.retain(|s| s.id == "S3");
        let s = &mut line.sections[0];
        s.cable = "synthetic".into();
        s.points.truncate(2);
        s.stringing = Stringing::Tension {
            horizontal_tension: 20_000.0,
            temperature: 15.0,
        };
        // Wires fixed at the structures: no strings moving with the tension
        // (RS holds the clamps, FE would let strain strings swing).
        let ends = line.sections[0].points.clone();
        for p in &ends {
            let st = line
                .structures
                .iter_mut()
                .find(|x| x.id == p.structure)
                .unwrap();
            let wire = st.wire_at_set(p.set).unwrap().label.clone();
            st.strings.retain(|s| s.wire != wire);
        }
        let budget = default_fe_budget();
        let sag = |line: &Line, c: Calculation, t: f64| {
            let mut section = line.sections[0].clone();
            section.calculation = c;
            let case = WeatherCase::still("t", Temperature::from_celsius(t));
            let shape = solve_section(line, &section, cable, &case, &budget).unwrap();
            (shape.spans[0].horizontal_tension, shape.spans[0].sag)
        };
        for t in [-20.0, 15.0, 60.0] {
            let (fe, rs) = (
                sag(&line, Calculation::InitialFe, t),
                sag(&line, Calculation::InitialRs, t),
            );
            // Tensions within 0.1 %, sags within 1 cm (the methods measure
            // sag on slightly different chords: 5 mm here at stringing too).
            let (h_fe, h_rs) = (fe.0, rs.0);
            assert!(
                (h_fe - h_rs).abs() < 1e-3 * h_rs,
                "{t} °C: H FE {h_fe} RS {h_rs}"
            );
            assert!(
                (fe.1 - rs.1).abs() < 0.01,
                "{t} °C: sag FE {} RS {}",
                fe.1,
                rs.1
            );
        }
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
        // Measured at the working tension: the aluminium's set counts too
        // (at zero tension its strands are slack: only the core's would).
        {
            use ols_cables::mechanics::{MaterialState, Mechanics};
            use ols_core::units::Force;
            let m = Mechanics::from_cable(cable).unwrap();
            let t = Temperature::from_celsius(15.0);
            let at_zero = m.strain(Force::ZERO, t, &creep.material).unwrap()
                - m.strain(Force::ZERO, t, &MaterialState::Initial).unwrap();
            assert!(
                creep.stretch > 1.5 * at_zero,
                "{} vs {at_zero}",
                creep.stretch
            );
        }
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
