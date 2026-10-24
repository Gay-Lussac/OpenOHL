// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Solving sections of a line in 3D.
//!
//! Each section becomes an [`ohl_wirefe::Model`]:
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
use ohl_cables::Cable;
use ohl_core::budget::Budget;
use ohl_weather::WeatherCase;
use ohl_wirefe::catenary_element::default_element_budget;
use ohl_wirefe::{CableElement, CableProperties, Model, Node, TrussElement, default_fe_budget};

use crate::{Line, LineError, Point, Section, Stringing};

/// Point loads closer than this to a span's end (along the wire), m, are
/// applied to the clamp. A marker ball 0.45 m from a free strain clamp
/// (fraction 0.001 of a 445 m span, real data) otherwise leaves a 0.45 m
/// cable piece between two free nodes — a nearly singular chain on which
/// Newton stalls; the effect on the wire's shape is far below a millimetre.
pub const LUMP_AT_CLAMP: f64 = 1.0;

/// Axial stiffness used for insulator strings, N (composite long-rod order).
pub const DEFAULT_INSULATOR_STIFFNESS: f64 = ohl_structures::insulators::DEFAULT_AXIAL_STIFFNESS;

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
    /// Radius of a subconductor's surface in the weather case, m: half the
    /// cable's iced diameter ([`WeatherCase::iced_diameter`]). Clearances
    /// are measured to this surface.
    pub surface_radius: f64,
    /// The most the wire lies off the polyline `points`, m
    /// ([`polyline_error`]).
    pub polyline_error: f64,
}

/// The most a catenary through `points` lies off their polyline, m: the
/// largest sagitta \\( R - \sqrt{R^2 - L^2/4} \\) of a piece of chord
/// length \\( L \\) on a circle of radius `radius`, the catenary's smallest
/// radius of curvature \\( R = H/w \\) (its curvature
/// \\( 1/(C \cosh^2(x/C)) \\) is at most \\( 1/C = w/H \\), at its low
/// point; an elastic catenary curves less). A curve of curvature at most
/// \\( 1/R \\) between two points stays within that sagitta of their chord.
pub fn polyline_error(points: &[Vector3<f64>], radius: f64) -> f64 {
    if !(radius > 0.0 && radius.is_finite()) {
        return 0.0;
    }
    points
        .windows(2)
        .map(|w| {
            let half = (w[1] - w[0]).norm() / 2.0;
            if half >= radius {
                // A piece longer than the circle's diameter: no bound
                // tighter than half the piece.
                half
            } else {
                radius - (radius * radius - half * half).sqrt()
            }
        })
        .fold(0.0, f64::max)
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
    /// Its span points as solved: label, position (finite elements only,
    /// else empty).
    pub span_points: Vec<(String, Point)>,
    /// The spacers it holds, as solved ([`solve_spacer_group`]).
    pub spacers: Vec<SpacerState>,
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
    /// Its limits, not checked by OpenOHL (its own, else its insulator's;
    /// absent: not given): for a plugin or a report to compare with
    /// `force` and `swing`.
    pub limits: crate::StringLimits,
    /// Its swing as solved ([`crate::swing`], in its structure's axes).
    pub swing: crate::Swing,
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
    /// Its span points' nodes on the wire, labelled.
    span_points: Vec<(String, usize)>,
    /// The spacers it holds: truss, fixed node.
    spacers: Vec<(SpacerHold, usize, usize)>,
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

/// The weather cases of a section's spans: the section's case, and spans
/// that carry a case of their own (one span iced while its neighbours are
/// not, unequal ice, …). A section solved with [`SpanCases::uniform`] is
/// solved exactly as with its one case, bit for bit.
///
/// What a span's own case sets: its load per metre (weight, ice, the wind
/// on its iced diameter, from its side), its temperature (the thermal
/// strain of its wire, and the stress-strain curves' strain at its
/// tension), the forces on the objects on it, and its surface radius. The
/// section's case (`base`) sets the rest: the wind on the strings, the
/// jumpers on the clamps, the core branch of a bimetallic cable (one per
/// section), and the rolling of a section in the blocks — a section in the
/// blocks rolls in the section's case and the spans' own cases then act on
/// it clipped in (as point and spread loads do: ice arriving on one span
/// of a clipped-in wire swings the strings into it).
#[derive(Debug, Clone, PartialEq)]
pub struct SpanCases {
    /// The section's case: every span without its own.
    pub base: WeatherCase,
    /// Spans with their own case, by span index in the section.
    pub spans: std::collections::BTreeMap<usize, WeatherCase>,
}

impl SpanCases {
    /// One case for every span.
    pub fn uniform(case: WeatherCase) -> Self {
        Self {
            base: case,
            spans: std::collections::BTreeMap::new(),
        }
    }

    /// With span `span` in `case`.
    pub fn with_span(mut self, span: usize, case: WeatherCase) -> Self {
        self.spans.insert(span, case);
        self
    }

    /// The case of span `span`.
    pub fn of(&self, span: usize) -> &WeatherCase {
        self.spans.get(&span).unwrap_or(&self.base)
    }

    /// Every span in the section's case.
    pub fn is_uniform(&self) -> bool {
        self.spans.values().all(|c| *c == self.base)
    }

    /// The cases with the wind's side swapped.
    fn flipped(&self) -> Self {
        let flip = |c: &WeatherCase| WeatherCase {
            wind_from: c.wind_from.opposite(),
            ..c.clone()
        };
        Self {
            base: flip(&self.base),
            spans: self.spans.iter().map(|(s, c)| (*s, flip(c))).collect(),
        }
    }
}

/// The cases of a section's spans, borrowed (the solver's view of
/// [`SpanCases`], or one case for all).
#[derive(Clone, Copy)]
struct Cases<'a> {
    base: &'a WeatherCase,
    own: Option<&'a std::collections::BTreeMap<usize, WeatherCase>>,
}

impl<'a> Cases<'a> {
    /// One case for every span.
    fn one(case: &'a WeatherCase) -> Self {
        Self {
            base: case,
            own: None,
        }
    }

    fn of_spans(cases: &'a SpanCases) -> Self {
        Self {
            base: &cases.base,
            own: Some(&cases.spans),
        }
    }

    /// The case of span `s`.
    fn of(&self, s: usize) -> &'a WeatherCase {
        self.own.and_then(|m| m.get(&s)).unwrap_or(self.base)
    }
}

/// Stringing reference of a section: clamp positions and unstressed lengths
/// at a reference temperature.
#[derive(Clone)]
struct Reference {
    clamps: Vec<Vector3<f64>>,
    lengths: Vec<f64>,
    temperature: f64,
    /// Each span's own weight per unit of its reference length, relative
    /// to the cable's weight per metre: 1 as cut at stringing; less where
    /// the length grew without adding weight (a permanent stretch, the
    /// stress-strain curves' strain, the core alone past the transition).
    /// The elements carry the cable's weight times this over their thermal
    /// factor: a span weighs the same hot or cold (owner decision
    /// 2026-10-21).
    weight_scale: Vec<f64>,
    /// Per what metre the elements carry the ice and wind (the line's
    /// [`crate::SolverOptions::loads_per`]; set where a weather case is
    /// solved, irrelevant in still air).
    loads_per: crate::LoadsPer,
}

impl Reference {
    /// Every span's reference length times `factor(s)`, adding no weight.
    fn lengthen(&mut self, factor: impl Fn(usize) -> f64) {
        for (s, (l, w)) in self
            .lengths
            .iter_mut()
            .zip(&mut self.weight_scale)
            .enumerate()
        {
            let k = factor(s);
            *l *= k;
            *w /= k;
        }
    }
}

/// [`span_load`] with the cable's own weight per metre scaled by `scale`
/// (ice and wind as they are): the load per unit of an element's
/// unstressed length when that length is longer than the cable as cut by
/// \\( 1/\text{scale} \\) without adding weight (thermal strain, creep,
/// set), so the span keeps its weight.
fn span_load_scaled(
    case: &WeatherCase,
    cable: &Cable,
    d: Vector3<f64>,
    scale: f64,
) -> Vector3<f64> {
    span_load(case, cable, d) + Vector3::new(0.0, 0.0, cable.weight().si() * (1.0 - scale))
}

/// [`span_load_scaled`] split as the finite elements carry it: per unit of
/// an element's unstressed length the cable's own weight times `scale`;
/// per metre of the stretched (real) wire the ice and wind (owner decision
/// 2026-10-21: they act on the conductor as it is, longer by its elastic
/// strain; [`ohl_wirefe::CableElement::stretched_load`]). With `per`
/// [`crate::LoadsPer::Unstretched`] (the solver option, owner decision
/// 2026-10-21) the ice and wind too per unit of unstressed length, as
/// before: none per stretched metre.
fn span_loads_split(
    case: &WeatherCase,
    cable: &Cable,
    d: Vector3<f64>,
    scale: f64,
    per: crate::LoadsPer,
) -> (Vector3<f64>, Vector3<f64>) {
    let w = cable.weight().si();
    let own = Vector3::new(0.0, 0.0, -w * scale);
    let ice_wind = span_load(case, cable, d) + Vector3::new(0.0, 0.0, w);
    match per {
        crate::LoadsPer::Stretched => (own, ice_wind),
        crate::LoadsPer::Unstretched => (own + ice_wind, Vector3::zeros()),
    }
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
    /// Its limits (reported, not checked).
    limits: crate::StringLimits,
    /// Its structure's orientation, degrees (for its swing).
    orientation_deg: f64,
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
                    stiffness: st.stiffness.unwrap_or(line.defaults.insulator_stiffness),
                    compression: st.compression,
                    limits: st.limits(),
                    orientation_deg: structure.orientation_deg,
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
            // data puts it. Without a weight is a resolved weight of 0, from
            // its own data or from an insulator giving 0 N, whether or not
            // it names one (2026-10-21: linked to 0 N catalogue entries, a
            // real line's strain strings lost the estimate, median sag
            // differences 0.7–2.4 cm → 2–9 cm); an insulator with a real
            // weight gives it.
            let estimate = shape == Shape::Strain && {
                let st = structure
                    .string(&members[0].label)
                    .expect("the holder's string");
                !st.strain && st.weight == 0.0
            };
            // A shared assembly's loads go with the first section through it
            // (solved together, with the one solving it: `Coupling::solvers`).
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

/// The holders as coupled: where the section solves a shared assembly
/// (`coupling.solvers` says which does, else the first through it), other
/// sections' clamps in it freed and pulled as `coupling.pulls` gives (the
/// whole bundle's pull of those wires on their clamp; without one, held
/// where given); where another section solves it, the clamp held where
/// `coupling.held` puts it.
fn pulled(layout: &[Holder], section: &Section, coupling: &Coupling) -> Vec<Holder> {
    let bundle = f64::from(section.bundle.max(1));
    layout
        .iter()
        .zip(&section.points)
        .map(|(h, point)| {
            let reports = match coupling.solvers.get(point) {
                Some(id) if h.shared => *id == section.id,
                _ => h.reports,
            };
            if h.shared
                && !reports
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
                return Holder {
                    reports,
                    ..h.clone()
                };
            }
            let mut out = Holder {
                reports,
                ..h.clone()
            };
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
    /// The section solving each shared assembly (its id), by the section
    /// points on it; an assembly not listed: the first section through it
    /// in the line's order.
    pub solvers: std::collections::BTreeMap<crate::SectionPoint, String>,
    /// The jumpers' load on clamps in the weather case, by section point
    /// (the whole bundle's, N, a force; [`crate::jumper::end_loads`], as
    /// the line's [`crate::JumperLoads`] says). None: from the jumpers,
    /// each taken as of this section's cable.
    pub jumpers: Option<JumperLoadMap>,
    /// The spacers this section holds as bars to fixed points (the other
    /// sections' span points), and the spacers' forces on its span points
    /// ([`solve_spacer_group`]).
    pub spacers: SpacerCoupling,
}

/// The spacers on a section in a pass of [`solve_spacer_group`] (the
/// whole bundle's values; the model of one subconductor takes its share).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpacerCoupling {
    /// The spacers it holds.
    pub holds: Vec<SpacerHold>,
    /// The force on each of its span points, by label, N.
    pub pulls: std::collections::BTreeMap<String, Point>,
}

/// A spacer held by a section: a bar from its span point `point` to a
/// fixed point `at` (the other end, where the other section puts it).
#[derive(Debug, Clone, PartialEq)]
pub struct SpacerHold {
    /// The spacer's id.
    pub id: String,
    /// The span point it is clamped at on this section.
    pub point: String,
    /// The other end.
    pub at: Point,
    /// Unstressed length, m.
    pub length: f64,
    /// Axial stiffness EA, N.
    pub stiffness: f64,
    /// It takes no compression.
    pub tension_only: bool,
    /// Its weight, N (half lands on this section's point).
    pub weight: f64,
    /// The wind on this end, N.
    pub wind: Point,
    /// The compression it may take, N (beyond: flagged).
    pub compression: Option<f64>,
}

/// A spacer as solved (reported by the section holding it).
#[derive(Debug, Clone, PartialEq)]
pub struct SpacerState {
    /// The spacer's id.
    pub id: String,
    /// Its ends: on this section, and the other.
    pub ends: [Point; 2],
    /// Axial force, N (tension positive; the whole bundle's).
    pub force: f64,
    /// The force it exerts on the other end, N.
    pub on_other: Point,
    /// Slack (tension only, pushed together).
    pub slack: bool,
    /// Beyond the compression it may take.
    pub over: bool,
}

/// The spacers in one subconductor's model (the share of each).
#[derive(Debug, Clone, Default)]
struct Extras {
    holds: Vec<SpacerHold>,
    pulls: Vec<(String, Vector3<f64>)>,
}

impl Extras {
    /// A section's share (`bundle` subconductors) of its spacers.
    fn of(spacers: &SpacerCoupling, bundle: f64) -> Self {
        Self {
            holds: spacers
                .holds
                .iter()
                .map(|h| SpacerHold {
                    stiffness: h.stiffness / bundle,
                    weight: h.weight / bundle,
                    wind: [0, 1, 2].map(|c| h.wind[c] / bundle),
                    ..h.clone()
                })
                .collect(),
            pulls: spacers
                .pulls
                .iter()
                .map(|(l, f)| (l.clone(), v(*f) / bundle))
                .collect(),
        }
    }
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
    cases: Cases<'_>,
    with_loads: bool,
    extras: &Extras,
) -> Result<Built, LineError> {
    let invalid = |m: String| LineError::Invalid(format!("section {}: {m}", section.id));
    let ea = cable.axial_stiffness().si();
    // The section's case: the wind on the strings.
    let case = cases.base;
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

    // Cable elements, split at point loads, span points and the ends of
    // spread loads along the reference shape.
    let element_budget = default_element_budget();
    let mut span_points: Vec<(String, usize)> = Vec::new();
    let mut span_elements = Vec::with_capacity(n - 1);
    for s in 0..n - 1 {
        // The span's own case (the section's, unless it has its own).
        let case = cases.of(s);
        let thermal = (cable.thermal_expansion()
            * (case.temperature.celsius() - reference.temperature))
            .exp();
        let length = reference.lengths[s] * thermal;
        let d = reference.clamps[s + 1] - reference.clamps[s];
        // Per unit of the element's (thermally longer or shorter)
        // unstressed length: the own weight spread over it, the span's
        // total weight unchanged.
        let scale = reference.weight_scale[s] / thermal;
        // Ice and wind per metre of the stretched wire.
        let (load, stretched) = span_loads_split(case, cable, d, scale, reference.loads_per);
        let shape_props = CableProperties {
            unstressed_length: reference.lengths[s],
            axial_stiffness: ea,
            load: gravity,
        };
        // (place along the wire, force, span point it is) — the span
        // points, with the forces of their objects.
        let mut sites: Vec<(f64, Vector3<f64>, Option<String>)> = Vec::new();
        // Spread loads: (from, to along the wire, per metre).
        let mut spread: Vec<(f64, f64, Vector3<f64>)> = Vec::new();
        let mut shape = None;
        let here = section.span_points.iter().any(|p| p.span == s);
        if with_loads && here {
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
            let along = |place: &crate::SpanPlace| {
                shape_props.arc_length_at_fraction(
                    d,
                    &solved,
                    place.fraction_of(d.xy().norm()).clamp(0.0, 1.0),
                )
            };
            let bundle = f64::from(section.bundle.max(1));
            sites = section
                .span_points
                .iter()
                .filter(|p| p.span == s)
                .map(|p| {
                    let [down, across] = p
                        .object
                        .as_ref()
                        .map_or([0.0; 2], |o| o.forces(case, cable.diameter.si()));
                    (
                        along(&p.at),
                        // The model is one subconductor: its share.
                        (left * across - Vector3::new(0.0, 0.0, down)) / bundle,
                        Some(p.label.clone()),
                    )
                })
                .collect();
            // Their ends are points (sites already).
            for l in section.spread_loads().iter().filter(|l| l.span == s) {
                let (a0, b0) = (along(&l.start), along(&l.end));
                let (from, to) = (a0.min(b0), a0.max(b0));
                // Per metre of the wire as cut, its share: a weight on the
                // wire, kept as the wire grows or shrinks (as its own).
                spread.push((from, to, Vector3::new(0.0, 0.0, -l.load / bundle * scale)));
            }
            sites.sort_by(|a, b| a.0.total_cmp(&b.0));
            shape = Some(solved);
        }
        // The spread loads on a piece of wire from `a` to `b` along it:
        // each by the share of the piece it covers.
        let spread_on = |a: f64, b: f64| {
            let mut extra = Vector3::zeros();
            for (from, to, q) in &spread {
                let overlap = (b.min(*to) - a.max(*from)).max(0.0);
                if b > a {
                    extra += q * (overlap / (b - a));
                }
            }
            extra
        };
        let mut elements = Vec::new();
        let (mut prev_node, mut prev_s) = (clamps[s], 0.0);
        for (s0, force, point) in sites {
            if s0 - prev_s < 1e-6 || s0 < LUMP_AT_CLAMP {
                // Coincident with the previous point, or next to the clamp:
                // add to that node.
                model.nodes[prev_node].load += force;
                if let Some(label) = point {
                    span_points.push((label, prev_node));
                }
                continue;
            }
            if reference.lengths[s] - s0 < LUMP_AT_CLAMP {
                // Next to the ahead clamp: on it.
                model.nodes[clamps[s + 1]].load += force;
                if let Some(label) = point {
                    span_points.push((label, clamps[s + 1]));
                }
                continue;
            }
            let node = model.nodes.len();
            let start =
                reference.clamps[s] + shape_props.point_at(d, shape.as_ref().expect("solved"), s0);
            let mut site = Node::free(start);
            site.load = force;
            model.nodes.push(site);
            if let Some(label) = point {
                span_points.push((label, node));
            }
            elements.push(model.cables.len());
            model.cables.push(CableElement {
                i: prev_node,
                j: node,
                properties: CableProperties {
                    unstressed_length: (s0 - prev_s) * thermal,
                    axial_stiffness: ea,
                    load: load + spread_on(prev_s, s0),
                },
                stretched_load: stretched,
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
                load: load + spread_on(prev_s, reference.lengths[s]),
            },
            stretched_load: stretched,
        });
        span_elements.push(elements);
    }
    let spacers = spacer_bars(&mut model, &span_points, extras, &invalid)?;
    Ok(Built {
        model,
        clamps,
        span_elements,
        members,
        supports,
        free: free_labels,
        spacers,
        span_points,
    })
}

/// The spacers of `extras` in a model: the forces on span points, and the
/// bars to fixed points with their weight (half on each end) and the wind
/// on this end.
fn spacer_bars(
    model: &mut Model,
    span_points: &[(String, usize)],
    extras: &Extras,
    invalid: &dyn Fn(String) -> LineError,
) -> Result<Vec<(SpacerHold, usize, usize)>, LineError> {
    let node_of = |label: &str| {
        span_points
            .iter()
            .find(|(l, _)| l == label)
            .map(|(_, n)| *n)
            .ok_or_else(|| invalid(format!("span point {label} not on the wire")))
    };
    for (label, f) in &extras.pulls {
        let n = node_of(label)?;
        model.nodes[n].load += f;
    }
    let mut out = Vec::new();
    for h in &extras.holds {
        let n = node_of(&h.point)?;
        model.nodes[n].load += v(h.wind);
        let fixed = model.nodes.len();
        model.nodes.push(Node::fixed(v(h.at)));
        model.trusses.push(TrussElement {
            i: n,
            j: fixed,
            unstressed_length: h.length,
            axial_stiffness: h.stiffness,
            tension_only: h.tension_only,
            weight: h.weight,
        });
        out.push((h.clone(), model.trusses.len() - 1, fixed));
    }
    Ok(out)
}

/// Builds and solves a section model.
fn solve_built(
    section: &Section,
    layout: &[Holder],
    reference: &Reference,
    cable: &Cable,
    cases: Cases<'_>,
    budget: &Budget,
    loads: Option<&Extras>,
) -> Result<(Built, ohl_wirefe::Solution), LineError> {
    let none = Extras::default();
    let built = build(
        section,
        layout,
        reference,
        cable,
        cases,
        loads.is_some(),
        loads.unwrap_or(&none),
    )?;
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
        ohl_core::units::Temperature::from_celsius(reference.temperature),
    );
    let (built, solution) = solve_built(
        section,
        &held,
        &reference,
        cable,
        Cases::one(&case),
        budget,
        Some(&Extras::default()),
    )?;
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
                weight_scale: vec![1.0; lengths.len()],
                loads_per: crate::LoadsPer::default(),
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
        ohl_core::units::Temperature::from_celsius(temperature),
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
            weight_scale: vec![1.0; lengths.len()],
            lengths,
            temperature,
            loads_per: crate::LoadsPer::default(),
        };
        let (built, solved) = solve_built(
            section,
            layout,
            &reference,
            cable,
            Cases::one(&case),
            budget,
            None,
        )
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
    solution: &ohl_wirefe::Solution,
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
fn clamp_pull(built: &Built, solution: &ohl_wirefe::Solution, k: usize) -> Vector3<f64> {
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
    solution: &ohl_wirefe::Solution,
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
/// for weight `w` (N/m) ([`ohl_sagtension::catenary::tension_for_sag`]).
pub(crate) fn tension_for_sag(
    clamps: &[Vector3<f64>],
    span: Option<usize>,
    sag: f64,
    w: f64,
) -> Result<f64, String> {
    use ohl_core::units::{ForcePerLength, Length};
    use ohl_sagtension::catenary::{SpanGeometry, default_solver_budget};
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
            ohl_sagtension::section::ruling_span(&spans).map_err(|e| e.to_string())?,
        ),
    };
    ohl_sagtension::catenary::tension_for_sag(
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
        let (built, solved) =
            solve_built(section, layout, &r, cable, Cases::one(case), budget, None)?;
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
    // Each span's stretch under its ice and wind (per stretched metre: per
    // unit unstressed length times the stretch), updated by every
    // evaluation; `moved` its largest change in the last one.
    let stretches = std::cell::RefCell::new(vec![1.0; chords.len()]);
    let moved = std::cell::Cell::new(0.0_f64);
    let element_budget = ohl_wirefe::catenary_element::default_element_budget();
    // Unstressed lengths at the reference temperature for tension `h`.
    let lengths = |h: f64| -> Result<Vec<f64>, LineError> {
        moved.set(0.0);
        chords
            .iter()
            .enumerate()
            .map(|(s, d)| {
                let at = |e: ohl_wirefe::ElementError| invalid(format!("span {s}: {e}"));
                let (own, stretched) = span_loads_split(
                    case,
                    cable,
                    *d,
                    reference.weight_scale[s] / thermal,
                    reference.loads_per,
                );
                let k = stretches.borrow()[s];
                let load = own + stretched * k;
                let l = CableProperties::unstressed_length_for_tension(*d, h, ea, load, budget)
                    .map_err(at)?;
                if stretched != Vector3::zeros() {
                    let props = CableProperties {
                        unstressed_length: l,
                        axial_stiffness: ea,
                        load,
                    };
                    let state = props.solve(*d, None, &element_budget).map_err(at)?;
                    let next = props.stretched_length(&state) / l;
                    if (next - k).abs() > moved.get() || next.is_nan() {
                        moved.set((next - k).abs());
                    }
                    stretches.borrow_mut()[s] = next;
                }
                Ok(l / thermal)
            })
            .collect()
    };
    let excess = |h: f64| -> Result<f64, LineError> { Ok(lengths(h)?.iter().sum::<f64>() - total) };
    // Longer wire for a lower tension: bracket around the guess (within
    // `spread`), then a bracketed secant (Illinois): a few steps instead of
    // ~45 halvings.
    let mut steps = 0;
    let mut search = |guess: f64, spread: f64| -> Result<f64, LineError> {
        let mut step = || {
            steps += 1;
            if steps > 200 {
                Err(invalid("no common tension found in 200 steps".into()))
            } else {
                Ok(())
            }
        };
        let (mut lo, mut hi) = (guess / spread, guess * spread);
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
        Ok(h)
    };
    // The stretches follow the search; once a search ends where they no
    // longer change, the lengths are those of the ice and wind on the
    // stretched spans (a fixed point: searched again from there, close by,
    // until then; exit clause [`ohl_wirefe::model::STRETCH_PASSES`]).
    let mut h = search(guess, 1.2)?;
    for _ in 0..ohl_wirefe::model::STRETCH_PASSES {
        let found = lengths(h)?;
        if moved.get() <= ohl_wirefe::model::STRETCH_TOLERANCE {
            return Ok((
                Reference {
                    clamps: clamps.to_vec(),
                    lengths: found,
                    temperature: reference.temperature,
                    weight_scale: reference.weight_scale.clone(),
                    loads_per: reference.loads_per,
                },
                h,
            ));
        }
        h = search(h, 1.0 + 1e-4)?;
    }
    Err(invalid(format!(
        "the spans' stretch under ice and wind did not settle in {} passes (still changing by \
         {:.3e})",
        ohl_wirefe::model::STRETCH_PASSES,
        moved.get()
    )))
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
    let cable = &*line.bimetallic.apply(cable);
    let cable = &*strung_cable(section, cable)?;
    let (layout, _) = estimate_strain_weights(section, holders_of(line, section)?, cable, &budget)?;
    let r = reference(section, &layout, cable, &budget)?;
    Ok((r.lengths, r.temperature))
}

/// The cable as strung in `section`: its own weight per metre of
/// unstressed cable at the section's stringing temperature (owner decision
/// 2026-10-21). The data sheet's weight is at its reference temperature
/// ([`Cable::mass_reference_temperature`], 20 °C unless the cable gives
/// one); at the stringing temperature \\( t_s \\) a metre of the cable holds
/// \\( w_{ref}\\, e^{-\alpha (t_s - t_{ref})} \\) (the multiplicative thermal
/// form of the linear finite elements), or
/// \\( w_{ref} (1 + \varepsilon(0, t_{ref})) / (1 + \varepsilon(0, t_s)) \\)
/// in the additive strain form of the stress-strain curves (the ruling-span
/// method, and the finite elements of a cable with curves). From there the
/// calculations conserve it. The cable returned gives its mass at
/// \\( t_s \\): applied again, nothing changes.
pub fn strung_cable<'a>(
    section: &Section,
    cable: &'a Cable,
) -> Result<std::borrow::Cow<'a, Cable>, LineError> {
    use ohl_core::units::Temperature;
    let t = Temperature::from_celsius(section.stringing.temperature());
    if cable.reference_temperature == Some(t) {
        return Ok(std::borrow::Cow::Borrowed(cable));
    }
    let additive =
        section.calculation.method() == crate::condition::Method::Rs || curves_of(cable).is_some();
    let factor = if additive {
        ohl_cables::mechanics::Mechanics::from_cable(cable)
            .and_then(|m| m.weight_factor(cable.mass_reference_temperature(), t))
            .map_err(|e| LineError::Invalid(format!("section {}: {e}", section.id)))?
    } else {
        cable.weight_factor_at(t)
    };
    Ok(std::borrow::Cow::Owned(cable.with_mass_at(t, factor)))
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
    solve_cases(line, section, cable, Cases::one(case), budget, coupling)
}

/// Solves one section of a line with a weather case per span
/// ([`SpanCases`]): the spans with their own case loaded by it, the others
/// by the section's case. With every span in the section's case, the
/// same as [`solve_section`] bit for bit.
///
/// Spans in different cases need the finite-element method: a section
/// solved by the ruling span (one horizontal tension for all spans) with
/// cases that differ is an error.
///
/// Expected cost: as [`solve_section`].
pub fn solve_section_span_cases(
    line: &Line,
    section: &Section,
    cable: &Cable,
    cases: &SpanCases,
    budget: &Budget,
) -> Result<SectionShape, LineError> {
    solve_section_span_cases_coupled(line, section, cable, cases, budget, &Coupling::default())
}

/// [`solve_section_span_cases`] sharing assemblies with other sections.
pub fn solve_section_span_cases_coupled(
    line: &Line,
    section: &Section,
    cable: &Cable,
    cases: &SpanCases,
    budget: &Budget,
    coupling: &Coupling,
) -> Result<SectionShape, LineError> {
    if section.calculation.method() == crate::condition::Method::Rs && !cases.is_uniform() {
        return Err(LineError::Invalid(format!(
            "section {}: spans in different weather cases need the finite-element method \
             (the ruling span has one tension for all spans)",
            section.id
        )));
    }
    // The wind's side is the line's (as [`solve_section_coupled`]).
    let flipped;
    let cases = if against_the_line(line, section) {
        flipped = cases.flipped();
        &flipped
    } else {
        cases
    };
    solve_cases(
        line,
        section,
        cable,
        Cases::of_spans(cases),
        budget,
        coupling,
    )
}

/// A section in its spans' cases (the wind's side already the line's).
fn solve_cases(
    line: &Line,
    section: &Section,
    cable: &Cable,
    cases: Cases<'_>,
    budget: &Budget,
    coupling: &Coupling,
) -> Result<SectionShape, LineError> {
    use crate::condition::{Condition, Method};
    // Places given by station: as fractions of their spans.
    let section = &*line.section_with_fractions(section);
    // The line's bimetallic model for cables that do not give their own.
    let own = &*line.bimetallic.apply(cable);
    // Its own weight per metre at the stringing temperature; the jumpers
    // take theirs at their own temperature.
    let cable = &*strung_cable(section, own)?;
    let calc = section.calculation;
    let solve = |final_state: Option<&FinalState>| match calc.method() {
        Method::Fe => solve_fe(
            line,
            section,
            cable,
            own,
            cases,
            final_state,
            budget,
            coupling,
        ),
        // Uniform cases only (checked by the callers).
        Method::Rs => solve_rs(line, section, cable, cases.base, final_state, budget),
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
    pub material: ohl_cables::mechanics::MaterialState,
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

/// `case` as a sag-tension load state: the resultant per metre, of which
/// the cable's own weight (conserved by the sag-tension solvers, per metre
/// of the cable as strung) and the wind.
pub(crate) fn load_state(case: &WeatherCase, cable: &Cable) -> ohl_sagtension::LoadState {
    use ohl_core::units::ForcePerLength;
    let load = case.wire_load(cable.diameter, cable.weight());
    ohl_sagtension::LoadState {
        load: ForcePerLength::from_si(load_magnitude(case, cable)),
        weight: cable.weight(),
        wind: load.horizontal,
        temperature: case.temperature,
    }
}

/// A section as strung: span geometries, clamps, stringing state,
/// horizontal tension, N.
type Strung = (
    Vec<ohl_sagtension::SpanGeometry>,
    Vec<Vector3<f64>>,
    ohl_sagtension::LoadState,
    f64,
);

/// The section as strung, for the sag-tension model: its spans (between
/// the clamps hanging in still air at the stringing temperature), the
/// stringing state and the horizontal tension then (the mean of the
/// spans', from the finite-element model).
pub(crate) fn strung(
    line: &Line,
    section: &Section,
    cable: &Cable,
    budget: &Budget,
) -> Result<Strung, LineError> {
    use ohl_core::units::{Length, Temperature};
    let (layout, _) = estimate_strain_weights(section, holders_of(line, section)?, cable, budget)?;
    let reference = reference(section, &layout, cable, budget)?;
    let still = WeatherCase::still(
        "stringing",
        Temperature::from_celsius(reference.temperature),
    );
    let (built, solution) = solve_built(
        section,
        &layout,
        &reference,
        cable,
        Cases::one(&still),
        budget,
        None,
    )?;
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
            ohl_sagtension::SpanGeometry::inclined(
                Length::from_si(d.xy().norm()),
                Length::from_si(d.z),
            )
        })
        .collect();
    let state = ohl_sagtension::LoadState::bare(
        cable.weight(),
        Temperature::from_celsius(reference.temperature),
    );
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
    use ohl_cables::CableModel;
    let cable = &*line.bimetallic.apply(cable);
    let cable = &*strung_cable(section, cable)?;
    use ohl_cables::mechanics::{MaterialState, Mechanics};
    use ohl_core::units::Force;
    use ohl_sagtension::nonlinear::NonlinearSection;
    let invalid = |m: String| LineError::Invalid(format!("section {}: {m}", section.id));
    let st = |e: ohl_sagtension::SagTensionError| invalid(e.to_string());
    let cases = &line.condition_cases;
    let state_of = |c: &WeatherCase| load_state(c, cable);
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
    use ohl_cables::mechanics::{MaterialState, Mechanics};
    use ohl_core::units::{Force, Length};
    use ohl_sagtension::nonlinear::NonlinearSection;
    let invalid = |m: String| LineError::Invalid(format!("section {}: {m}", section.id));
    let st = |e: ohl_sagtension::SagTensionError| invalid(e.to_string());
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
                // Linear models: the stretch lengthens the cable (adding
                // no weight).
                ns.lengthen(f.stretch);
            }
            f.material.clone()
        }
        None => MaterialState::Initial,
    };
    let solved = ns
        .solve(load_state(case, cable), &material, budget)
        .map_err(st)?;
    // States beyond the stress-strain curves' range (2026-10-22).
    let mut flags: Vec<String> = ns
        .range_warnings(&solved, &load_state(case, cable), &material)
        .map_err(st)?
        .into_iter()
        .map(|w| format!("stress-strain {w}"))
        .collect();
    let h = solved.horizontal_tension.si();
    let mut out = Vec::with_capacity(spans.len());
    for (k, sol) in solved.spans.iter().enumerate() {
        let (a, b) = (clamps[k], clamps[k + 1]);
        let d = b - a;
        let horizontal = d.xy().norm();
        // The span's load per metre (its own weight spread over its
        // stretched length).
        let q = sol.catenary.weight.si();
        // The plane of the load: the sag hangs along it (down, and to the
        // side the wind blows).
        let along_load = {
            let l = span_load_scaled(case, cable, d, q_scale(case, cable, q));
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
            polyline_error: polyline_error(&points, h / q),
            points: points.iter().map(|q| p(*q)).collect(),
            horizontal_tension: h,
            max_tension: sol.tension_left().si().max(sol.tension_right().si()),
            mid_point: p(points[n / 2]),
            low_point: p(low),
            sag,
            surface_radius: case.iced_diameter(cable.diameter).si() / 2.0,
        });
    }
    // Above the wire's strength (owner, 2026-10-22).
    flags.extend(rts_flags(&out, cable));
    let n = clamps.len();
    Ok(SectionShape {
        section: section.id.clone(),
        clamps: clamps.iter().map(|c| p(*c)).collect(),
        spans: out,
        strings: vec![Vec::new(); n],
        iterations: 0,
        estimated_weights: vec![None; n],
        attachment_loads: Vec::new(),
        flags,
        pulls: Vec::new(),
        nodes: Vec::new(),
        span_points: Vec::new(),
        spacers: Vec::new(),
    })
}

/// Whether a span of `shape` exceeds its wire's rated tensile strength
/// (its warning, [`rts_flags`]).
pub fn above_rts(shape: &SectionShape) -> bool {
    shape.flags.iter().any(|f| f.contains(RTS_WARNING))
}

/// What the warnings above the wire's strength say ([`rts_flags`]).
pub const RTS_WARNING: &str = "exceeds the wire's rated tensile strength";

/// The warnings of the spans whose tension at their most tensioned
/// support (`SpanShape::max_tension`, per wire) exceeds the wire type's
/// rated tensile strength (owner, 2026-10-22: always warned, whatever the
/// range warnings do), in every solver — "span 2: tension 41.20 kN at
/// the most tensioned support exceeds the wire's rated tensile strength
/// (RTS 40.00 kN, 103.0 %)". Nothing for a wire type without an RTS (0).
pub fn rts_flags(spans: &[SpanShape], cable: &Cable) -> Vec<String> {
    let rts = cable.rated_tensile_strength.si();
    if rts.is_nan() || rts <= 0.0 {
        return Vec::new();
    }
    spans
        .iter()
        .enumerate()
        .filter(|(_, s)| s.max_tension > rts)
        .map(|(k, s)| {
            format!(
                "span {}: tension {:.2} kN at the most tensioned support exceeds the wire's \
                 rated tensile strength (RTS {:.2} kN, {:.1} %)",
                k + 1,
                s.max_tension / 1000.0,
                rts / 1000.0,
                100.0 * s.max_tension / rts
            )
        })
        .collect()
}

/// The factor on the cable's own weight that makes its load per metre in
/// `case` equal to `q` (the ruling-span solution's per-metre load: the
/// weight spread over the stretched span).
fn q_scale(case: &WeatherCase, cable: &Cable, q: f64) -> f64 {
    let load = case.wire_load(cable.diameter, cable.weight());
    let (h, v, w) = (
        load.horizontal.si(),
        load.vertical.si(),
        cable.weight().si(),
    );
    let vertical = (q - h).max(0.0).sqrt() * (q + h).sqrt();
    if w > 0.0 {
        (vertical - (v - w)) / w
    } else {
        1.0
    }
}

/// The cable's stress-strain curves, when its model uses them (the
/// non-linear model with curves given).
fn curves_of(cable: &Cable) -> Option<ohl_cables::mechanics::Mechanics> {
    if cable.model != ohl_cables::CableModel::Nonlinear {
        return None;
    }
    ohl_cables::mechanics::Mechanics::from_cable(cable)
        .ok()
        .filter(|m| m.is_nonlinear())
}

/// Mean tension of each span's elements (weighted by their unstressed
/// lengths), N.
fn span_tensions(built: &Built, solution: &ohl_wirefe::Solution) -> Vec<f64> {
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
#[allow(clippy::too_many_arguments)]
fn solve_fe(
    line: &Line,
    section: &Section,
    cable: &Cable,
    jumper_cable: &Cable,
    cases: Cases<'_>,
    final_state: Option<&FinalState>,
    budget: &Budget,
    coupling: &Coupling,
) -> Result<SectionShape, LineError> {
    // The section's case: the jumpers, the rolling, the core branch.
    let case = cases.base;
    let (layout, estimated_weights) =
        estimate_strain_weights(section, holders_of(line, section)?, cable, budget)?;
    let mut reference = reference(section, &layout, cable, budget)?;
    // The ice and wind per stretched or unstretched metre (the line's
    // calculation options, 2026-10-21).
    reference.loads_per = line.solver.loads_per;
    // In the weather case, other sections' clamps in the assemblies pulled
    // as given (held in the stringing state).
    let mut loaded = pulled(&layout, section, coupling);
    // The jumpers hanging on its clamps, in the weather case (as the
    // line's calculation options say).
    let jumpers = match &coupling.jumpers {
        Some(j) => std::borrow::Cow::Borrowed(j),
        None => std::borrow::Cow::Owned(crate::jumper::end_loads(line, case, |id| {
            (id == section.cable).then_some(jumper_cable)
        })),
    };
    let bundle = f64::from(section.bundle.max(1));
    for (h, point) in loaded.iter_mut().zip(&section.points) {
        if let (Some(f), At::Free(i)) = (jumpers.get(&LoadAt::Clamp(point.clone())), h.clamp) {
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
                ohl_core::units::Temperature::from_celsius(reference.temperature),
            );
            let (b, sol) = solve_built(
                section,
                &layout,
                &reference,
                cable,
                Cases::one(&stringing),
                budget,
                None,
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
        reference.lengthen(|_| 1.0 + stretch);
    }
    if !section.stringing.clipped() && reference.lengths.len() > 1 {
        reference = rolled(section, &loaded, &reference, cable, case, budget)?;
    }
    // The spacers it holds and is pulled by (its subconductor's share),
    // and the jumpers hanging on taps on its span points.
    let mut extras = Extras::of(&coupling.spacers, bundle);
    for (at, f) in jumpers.iter() {
        if let LoadAt::Span(r) = at
            && r.section == section.id
        {
            extras.pulls.push((r.point.clone(), v(*f) / bundle));
        }
    }
    let (mut built, mut solution) = solve_built(
        section,
        &loaded,
        &reference,
        cable,
        cases,
        budget,
        Some(&extras),
    )?;
    if let (Some(m), Some(strung)) = (&curves, &strung) {
        use ohl_cables::mechanics::MaterialState;
        use ohl_core::units::{Force, Temperature};
        let material = final_state.map_or(MaterialState::Initial, |f| f.material.clone());
        let ea = cable.axial_stiffness().si();
        // Each span at its own case's temperature.
        let t_strung = reference.temperature;
        let thermal = |s: usize| {
            (cable.thermal_expansion() * (cases.of(s).temperature.celsius() - t_strung)).exp()
        };
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
        let base = reference.clone();
        let mut factors = vec![1.0; base.lengths.len()];
        let mut settled = false;
        for _ in 0..CURVE_PASSES {
            let tensions = span_tensions(&built, &solution);
            let mut change: f64 = 0.0;
            for (s, &t) in tensions.iter().enumerate() {
                let (stretched, eps_ref) = at_stringing[s];
                let eps = strain(t, cases.of(s).temperature, &material)?;
                let k = stretched * (1.0 + eps) / ((1.0 + eps_ref) * thermal(s) * (1.0 + t / ea));
                change = change.max(((k - factors[s]) / factors[s]).abs());
                factors[s] = k;
            }
            if change < 1e-9 {
                settled = true;
                break;
            }
            reference = base.clone();
            reference.lengthen(|s| factors[s]);
            (built, solution) = solve_built(
                section,
                &loaded,
                &reference,
                cable,
                cases,
                budget,
                Some(&extras),
            )?;
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
        reference.lengthen(|_| 1.0 + offset);
        (built, solution) = solve_built(
            section,
            &loaded,
            &reference,
            &core,
            cases,
            budget,
            Some(&extras),
        )?;
    }
    let pos = &solution.positions;
    let mut spans = Vec::with_capacity(built.span_elements.len());
    for (s, elements) in built.span_elements.iter().enumerate() {
        let (a, b) = (pos[built.clamps[s]], pos[built.clamps[s + 1]]);
        let chord = b - a;
        let horizontal = chord.xy().norm();
        let mut points: Vec<Vector3<f64>> = Vec::new();
        let mut max_tension: f64 = 0.0;
        let mut off: f64 = 0.0;
        for &e in elements {
            let element = &built.model.cables[e];
            let state = &solution.cables[e];
            max_tension = max_tension.max(state.tension_i).max(state.tension_j);
            let d = pos[element.j] - pos[element.i];
            // As solved: its ice and wind per stretched metre included.
            let props = solution.cable_properties(&built.model, e);
            let samples = props.shape(d, state, 64);
            // Each element a catenary of its own H and load.
            off = off.max(polyline_error(
                &samples,
                state.horizontal / props.load.norm(),
            ));
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
            surface_radius: cases.of(s).iced_diameter(cable.diameter).si() / 2.0,
            polyline_error: off,
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
                        limits: m.limits,
                        swing: crate::swing(m.orientation_deg, p(top), p(bottom)),
                    }
                })
                .collect()
        })
        .collect();
    let spacer_states: Vec<SpacerState> = built
        .spacers
        .iter()
        .map(|(h, truss, fixed)| {
            let element = &built.model.trusses[*truss];
            let (here, there) = (pos[element.i], pos[*fixed]);
            // The whole bundle's.
            let force = solution.truss_forces[*truss] * bundle;
            let towards = there - here;
            let u = if towards.norm() > 0.0 {
                towards / towards.norm()
            } else {
                Vector3::zeros()
            };
            let slack = h.tension_only && force <= 0.0 && towards.norm() < h.length;
            let over = h.compression.is_some_and(|c| -force > c);
            if over {
                flags.push(format!(
                    "spacer {}: {:.0} N of compression, more than the {:.0} N it may take",
                    h.id,
                    -force,
                    h.compression.unwrap_or(0.0)
                ));
            }
            SpacerState {
                id: h.id.clone(),
                ends: [p(here), p(there)],
                force,
                // Tension pulls the other end towards this one.
                on_other: p(-u * force),
                slack,
                over,
            }
        })
        .collect();
    // States beyond the stress-strain curves' range (2026-10-22): the
    // strain at each span's largest tension, in the case's material state.
    if let Some(m) = &curves {
        use ohl_cables::mechanics::MaterialState;
        use ohl_core::units::{Force, Temperature};
        let material = final_state.map_or(MaterialState::Initial, |f| f.material.clone());
        for (k, span) in spans.iter().enumerate() {
            let at = Temperature::from_celsius(cases.of(k).temperature.celsius());
            if let Ok(e) = m.strain(Force::from_si(span.max_tension), at, &material) {
                for r in m.beyond_range(e, at) {
                    flags.push(format!("span {}: stress-strain {r}", k + 1));
                }
            }
        }
    }
    // Above the wire's strength (owner, 2026-10-22).
    flags.extend(rts_flags(&spans, cable));
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
        span_points: built
            .span_points
            .iter()
            .map(|(label, n)| (label.clone(), p(pos[*n])))
            .collect(),
        spacers: spacer_states,
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
    solution: &ohl_wirefe::Solution,
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
    core.outer.modulus = ohl_core::units::Stress::ZERO;
    Some((core, limit / core_part.axial_stiffness().si()))
}

/// Per section of `line` (by id), whether it runs against the line's
/// direction ([`against_the_line`], the routes made once): the sections
/// solved with the wind's side swapped. A section whose answer differs
/// between two versions of a line is solved again (its wind's side).
pub fn sections_against_the_line(line: &Line) -> std::collections::BTreeMap<String, bool> {
    let views = line.alignment_views();
    line.sections
        .iter()
        .map(|s| (s.id.clone(), against_in(line, s, &views)))
        .collect()
}

/// [`against_the_line`] on the routes `views`.
fn against_in(line: &Line, section: &Section, views: &[crate::route::AlignmentView]) -> bool {
    let ends = (section.points.first(), section.points.last());
    let (Some(a), Some(b)) = ends else {
        return false;
    };
    let (Some(a), Some(b)) = (line.wire_point(a), line.wire_point(b)) else {
        return false;
    };
    let (a, b) = (a.position, b.position);
    let mid = [0.5 * (a[0] + b[0]), 0.5 * (a[1] + b[1])];
    line.direction_at_in(views, mid)
        .is_some_and(|d| (b[0] - a[0]) * d[0] + (b[1] - a[1]) * d[1] < 0.0)
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
    library: &ohl_cables::Library,
    case: &WeatherCase,
) -> Result<SectionShape, LineError> {
    let cable = library.get(&section.cable).ok_or_else(|| {
        LineError::Invalid(format!(
            "section {}: wire type {} is not in the wire library",
            section.id, section.cable
        ))
    })?;
    solve_section(line, section, cable, case, &default_fe_budget())
}

/// Solves every section; returns one result per section, in order.
/// Sections sharing an assembly are solved together ([`solve_coupled`]).
pub fn solve_line(
    line: &Line,
    library: &ohl_cables::Library,
    case: &WeatherCase,
) -> Vec<Result<SectionShape, LineError>> {
    solve_line_with(line, library, case, &default_fe_budget())
}

/// [`solve_line`] with the exit clauses of `budget` for each solve. The
/// jumpers on the clamps as the line's [`crate::JumperLoads`] says: their
/// estimated loads, or their own pull ([`jumper_pull`]).
pub fn solve_line_with(
    line: &Line,
    library: &ohl_cables::Library,
    case: &WeatherCase,
    budget: &Budget,
) -> Vec<Result<SectionShape, LineError>> {
    let estimate = crate::jumper::end_loads(line, case, |id| library.get(id));
    let mut results = solve_sections_with(line, library, case, budget, &estimate);
    if needs_pull(line) || may_be_taut(line, &results) {
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

/// Where a jumper's end loads what it hangs on: a section's clamp (its
/// section point), a span point (a tap on a span), or another jumper's
/// point (a tap on a jumper: the jumper's id, the point's label).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LoadAt {
    /// A clamp of the sections through a section point.
    Clamp(crate::SectionPoint),
    /// A span point.
    Span(crate::SpanPointRef),
    /// A point of a jumper.
    Jumper(String, String),
}

impl LoadAt {
    /// Where jumper end `end` loads.
    pub fn of(end: &crate::JumperEnd) -> Self {
        match end {
            crate::JumperEnd::Wire(p) => Self::Clamp(p.clone()),
            crate::JumperEnd::Tap {
                tap: crate::Tap::Span(r),
            } => Self::Span(r.clone()),
            crate::JumperEnd::Tap {
                tap: crate::Tap::Jumper { jumper, point },
            } => Self::Jumper(jumper.clone(), point.clone()),
        }
    }

    /// A tap's (not a section clamp).
    pub fn is_tap(&self) -> bool {
        !matches!(self, Self::Clamp(_))
    }
}

/// Whether the jumpers' own forces are put back on what they hang on, in
/// passes: the jumper pull option (clamps), or taps solved with the full
/// loop.
pub fn needs_pull(line: &Line) -> bool {
    line.solver.jumper_loads == crate::JumperLoads::Pull
        || (line.solver.taps == crate::TapLoads::Loop
            && line.jumpers.iter().any(crate::Jumper::on_tap))
}

/// Whether a jumper may be taut between the clamps solved in `results`
/// (its cut length short of their distance, within 0.5 %): its pull is
/// then put back on them whatever the jumper option ([`jumper_pull`]).
pub fn may_be_taut(line: &Line, results: &[Result<SectionShape, LineError>]) -> bool {
    if !line.jumpers.iter().any(|j| j.length.is_some() && !j.rigid) {
        return false;
    }
    // Each section point's clamp: from the first solved section holding
    // it (its first place there), as a search through them would find it.
    let mut sections: std::collections::HashMap<&str, &Section> = Default::default();
    for s in &line.sections {
        sections.entry(s.id.as_str()).or_insert(s);
    }
    let mut clamps: std::collections::HashMap<&crate::SectionPoint, Vector3<f64>> =
        Default::default();
    for s in results.iter().filter_map(|r| r.as_ref().ok()) {
        let Some(sec) = sections.get(s.section.as_str()) else {
            continue;
        };
        for (k, q) in sec.points.iter().enumerate() {
            if sec.points[..k].contains(q) {
                continue;
            }
            if let Some(c) = s.clamps.get(k) {
                clamps.entry(q).or_insert_with(|| v(*c));
            }
        }
    }
    line.jumpers.iter().any(|j| {
        let Some(length) = j.length.filter(|_| !j.rigid) else {
            return false;
        };
        let at = |e: &crate::JumperEnd| e.point().and_then(|p| clamps.get(p).copied());
        match (at(&j.from), at(&j.to)) {
            (Some(a), Some(b)) => (b - a).norm() > 0.995 * length,
            _ => false,
        }
    })
}

/// Jumper loads on what they hang on (the whole bundle's, N).
pub type JumperLoadMap = std::collections::BTreeMap<LoadAt, [f64; 3]>;

/// Every section, the jumpers' loads `jumpers` on their clamps, spread
/// over the cores (sections are independent finite-element models);
/// sections sharing an assembly then together.
fn solve_sections_with(
    line: &Line,
    library: &ohl_cables::Library,
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
    for group in spacer_groups(line) {
        solve_spacer_group(line, library, case, &group, &mut results, budget, jumpers);
    }
    results
}

/// Passes of the jumpers' pull (exit clause; the clamps settle in two or
/// three).
pub const JUMPER_PULL_PASSES: usize = 8;

/// Passes at most with a taut jumper: its pull on the clamps relaxed
/// ([`jumper_pull`]).
pub const TAUT_PULL_PASSES: usize = 40;

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
    library: &ohl_cables::Library,
    case: &WeatherCase,
    budget: &Budget,
    estimate: &JumperLoadMap,
    used: JumperLoadMap,
    results: &mut [Result<SectionShape, LineError>],
) -> JumperLoadMap {
    jumper_pull_with(line, library, case, budget, estimate, used, results, None)
}

/// [`jumper_pull`], the jumpers' solutions kept in `memo` (when given)
/// and taken from it while nothing they depend on changed (the same
/// result).
#[allow(clippy::too_many_arguments)]
pub(crate) fn jumper_pull_with(
    line: &Line,
    library: &ohl_cables::Library,
    case: &WeatherCase,
    budget: &Budget,
    estimate: &JumperLoadMap,
    mut used: JumperLoadMap,
    results: &mut [Result<SectionShape, LineError>],
    memo: Option<&crate::incremental::JumperMemo>,
) -> JumperLoadMap {
    if line.jumpers.is_empty() {
        return used;
    }
    let groups = coupled_groups(line);
    // Taut jumpers' pull on their clamps, relaxed (a stiff coupling):
    // Aitken's step from the last two residuals.
    let mut omega = 0.3_f64;
    let mut last_r: Option<Vec<Vector3<f64>>> = None;
    let mut taut_seen = false;
    for pass in 0..TAUT_PULL_PASSES {
        if pass >= JUMPER_PULL_PASSES && !taut_seen {
            break;
        }
        let shapes: Vec<SectionShape> = results
            .iter()
            .filter_map(|r| r.as_ref().ok().cloned())
            .collect();
        let solved = crate::jumper::solve_jumpers_memo(line, library, case, &shapes, &used, memo);
        // The pull on each clamp or tap: the sum of its jumpers', where all
        // of them solved; else the estimate. Clamps pulled with the jumper
        // pull option, taps with the full loop.
        let pull_clamps = line.solver.jumper_loads == crate::JumperLoads::Pull;
        let pull_taps = line.solver.taps == crate::TapLoads::Loop;
        let mut next = estimate.clone();
        let mut failed: std::collections::BTreeSet<LoadAt> = Default::default();
        let mut pulled: JumperLoadMap = Default::default();
        // The clamps of taut jumpers: always pulled.
        let mut taut: std::collections::BTreeSet<LoadAt> = Default::default();
        for (j, r) in line.jumpers.iter().zip(&solved) {
            let ends = [LoadAt::of(&j.from), LoadAt::of(&j.to)];
            if let Ok(s) = r
                && s.taut
            {
                taut.extend(ends.iter().cloned());
                taut_seen = true;
            }
            match r {
                Ok(s) => {
                    let chained = crate::jumper::chained_pulls(line, s)
                        .into_iter()
                        .map(|(p, f)| (LoadAt::Clamp(p), f));
                    for (p, f) in ends.into_iter().zip(s.end_forces).chain(chained) {
                        let e = pulled.entry(p).or_insert([0.0; 3]);
                        for c in 0..3 {
                            e[c] += f[c];
                        }
                    }
                }
                Err(_) => failed.extend(ends),
            }
        }
        // Taut clamps relaxed: from the loads used, a step of `omega` of
        // the residual.
        let keys: Vec<LoadAt> = taut
            .iter()
            .filter(|p| !failed.contains(p))
            .cloned()
            .collect();
        let r: Vec<Vector3<f64>> = keys
            .iter()
            .map(|p| {
                let g = v(pulled.get(p).copied().unwrap_or([0.0; 3]));
                g - v(used.get(p).copied().unwrap_or([0.0; 3]))
            })
            .collect();
        if let Some(prev) = &last_r
            && prev.len() == r.len()
        {
            let dr: Vec<Vector3<f64>> = r.iter().zip(prev).map(|(a, b)| a - b).collect();
            let den: f64 = dr.iter().map(|x| x.norm_squared()).sum();
            if den > 0.0 {
                let num: f64 = prev.iter().zip(&dr).map(|(a, b)| a.dot(b)).sum();
                omega = (-omega * num / den).clamp(0.02, 1.0);
            }
        }
        last_r = Some(r.clone());
        for (p, f) in pulled {
            let wanted = if p.is_tap() { pull_taps } else { pull_clamps };
            if let Some(k) = keys.iter().position(|q| *q == p) {
                let u = v(used.get(&p).copied().unwrap_or([0.0; 3])) + r[k] * omega;
                next.insert(p, [u.x, u.y, u.z]);
            } else if wanted && !failed.contains(&p) {
                next.insert(p, f);
            }
        }
        // What changed: the sections whose clamps' or span points' loads
        // did; a jumper's points' (then solved again in the next pass).
        let moved = |p: &LoadAt| {
            let a = v(next.get(p).copied().unwrap_or([0.0; 3]));
            let b = v(used.get(p).copied().unwrap_or([0.0; 3]));
            (a - b).norm() > 0.5_f64.max(1e-4 * a.norm())
        };
        let changed: Vec<&LoadAt> = next
            .keys()
            .chain(used.keys())
            .filter(|p| moved(p))
            .collect();
        let again: Vec<usize> = line
            .sections
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                changed.iter().any(|p| match p {
                    LoadAt::Clamp(q) => s.points.contains(q),
                    LoadAt::Span(r) => r.section == s.id,
                    LoadAt::Jumper(..) => false,
                })
            })
            .map(|(k, _)| k)
            .collect();
        let on_jumpers = changed.iter().any(|p| matches!(p, LoadAt::Jumper(..)));
        used = next;
        if again.is_empty() {
            if on_jumpers {
                continue;
            }
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
        for group in spacer_groups(line)
            .iter()
            .filter(|g| g.iter().any(|k| again.contains(k)))
        {
            solve_spacer_group(line, library, case, group, results, budget, &used);
        }
    }
    used
}

fn cable_of<'a>(
    library: &'a ohl_cables::Library,
    section: &Section,
) -> Result<&'a Cable, LineError> {
    library.get(&section.cable).ok_or_else(|| {
        LineError::Invalid(format!(
            "section {}: wire type \"{}\" not in the libraries",
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
    // The structures by id (the first of an id, as `Line::structure`
    // finds it).
    let mut by_id: std::collections::HashMap<&str, &crate::Structure> = Default::default();
    for s in &line.structures {
        by_id.entry(s.id.as_str()).or_insert(s);
    }
    for (k, sec) in line.sections.iter().enumerate() {
        for p in &sec.points {
            let Some(s) = by_id.get(p.structure.as_str()).copied() else {
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

/// The groups of sections (indices) joined by spacers, with more than one
/// section each.
pub fn spacer_groups(line: &Line) -> Vec<Vec<usize>> {
    let n = line.sections.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let index = |id: &str| line.sections.iter().position(|s| s.id == id);
    for s in &line.spacers {
        if let (Some(a), Some(b)) = (index(&s.a.section), index(&s.b.section)) {
            let (x, y) = (find(&mut parent, a), find(&mut parent, b));
            if x != y {
                parent[x.max(y)] = x.min(y);
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

/// Passes of the spacers' coupling (exit clause; a few in practice).
pub const SPACER_PASSES: usize = 40;

/// Solves a group of sections joined by spacers ([`spacer_groups`]): each
/// spacer is held by the section at its end `a`, as a bar to where the
/// section at `b` puts its span point, and pulls that point with the
/// bar's force, half way from the last pass (they overshoot otherwise);
/// the sections are solved in turn until the forces change less than
/// 1 N (or 1e-4 of themselves). A spacer without a length takes the
/// distance between its ends in still air at the sections' stringing
/// temperatures. Each end carries half its weight and the wind on it
/// (across the span, from the weather case's side). `results` holds the
/// sections solved alone and gets the coupled ones; not settling in
/// [`SPACER_PASSES`] passes is an error for the group. Sections also
/// sharing an assembly with another (a TAK) are not supported with
/// spacers yet (an error).
///
/// Expected cost: a few solves of each section of the group.
pub fn solve_spacer_group(
    line: &Line,
    library: &ohl_cables::Library,
    case: &WeatherCase,
    group: &[usize],
    results: &mut [Result<SectionShape, LineError>],
    budget: &Budget,
    jumpers: &JumperLoadMap,
) {
    let fail = |results: &mut [Result<SectionShape, LineError>], m: String| {
        for &k in group {
            results[k] = Err(LineError::Invalid(m.clone()));
        }
    };
    let in_tak = coupled_groups(line);
    if let Some(&k) = group.iter().find(|k| in_tak.iter().any(|g| g.contains(k))) {
        fail(
            results,
            format!(
                "section {}: spacers on a section sharing an assembly are not supported yet",
                line.sections[k].id
            ),
        );
        return;
    }
    let index = |id: &str| line.sections.iter().position(|s| s.id == id);
    let spacers: Vec<&crate::Spacer> = line
        .spacers
        .iter()
        .filter(|s| index(&s.a.section).is_some_and(|k| group.contains(&k)))
        .collect();
    let point_at =
        |results: &[Result<SectionShape, LineError>], end: &crate::SpanPointRef| -> Option<Point> {
            let k = index(&end.section)?;
            results[k]
                .as_ref()
                .ok()?
                .span_points
                .iter()
                .find(|(l, _)| *l == end.point)
                .map(|(_, p)| *p)
        };
    // Lengths: given, else in still air at stringing.
    let mut lengths = std::collections::BTreeMap::new();
    let mut still: std::collections::BTreeMap<usize, Result<SectionShape, LineError>> =
        Default::default();
    for s in &spacers {
        let length = match s.length {
            Some(l) => l,
            None => {
                let mut at = Vec::new();
                for end in [&s.a, &s.b] {
                    let Some(k) = index(&end.section) else {
                        continue;
                    };
                    let shape = still.entry(k).or_insert_with(|| {
                        let section = &line.sections[k];
                        let calm = WeatherCase::still(
                            "stringing",
                            ohl_core::units::Temperature::from_celsius(
                                section.stringing.temperature(),
                            ),
                        );
                        let coupling = Coupling {
                            jumpers: Some(Default::default()),
                            ..Coupling::default()
                        };
                        cable_of(library, section).and_then(|cable| {
                            solve_section_coupled(line, section, cable, &calm, budget, &coupling)
                        })
                    });
                    if let Some(p) = shape
                        .as_ref()
                        .ok()
                        .and_then(|x| x.span_points.iter().find(|(l, _)| *l == end.point))
                    {
                        at.push(v(p.1));
                    }
                }
                match at.as_slice() {
                    [a, b] => (a - b).norm(),
                    _ => {
                        fail(
                            results,
                            format!("spacer {}: its ends did not solve at stringing", s.id),
                        );
                        return;
                    }
                }
            }
        };
        lengths.insert(s.id.clone(), length);
    }
    // The wind on a spacer, half on each end: across its span.
    let wind_of = |results: &[Result<SectionShape, LineError>], s: &crate::Spacer| -> Point {
        if case.wind_pressure.si() <= 0.0 || s.wind_area <= 0.0 {
            return [0.0; 3];
        }
        let Some(k) = index(&s.a.section) else {
            return [0.0; 3];
        };
        let Some(span) = line.sections[k]
            .span_points
            .iter()
            .find(|p| p.label == s.a.point)
            .map(|p| p.span)
        else {
            return [0.0; 3];
        };
        let Some(shape) = results[k].as_ref().ok() else {
            return [0.0; 3];
        };
        let d = v(shape.clamps[span + 1]) - v(shape.clamps[span]);
        let h = d.xy();
        if h.norm() == 0.0 {
            return [0.0; 3];
        }
        let left = Vector3::new(-h.y, h.x, 0.0) / h.norm();
        p(left * (0.5 * case.wind_pressure.si() * s.wind_area * case.wind_from.to_left()))
    };
    // The forces on the pulled ends, one vector per spacer (in `spacers`'
    // order): fixed point of g(f) — the pulled sections solved under f,
    // then the holding sections with their bars to where those put the
    // ends give the bars' forces — with Aitken's relaxation.
    let n = spacers.len();
    let mut f: Vec<Vector3<f64>> = vec![Vector3::zeros(); n];
    let mut last: Option<Vec<Vector3<f64>>> = None;
    let mut omega = 0.5;
    let solve_with = |results: &mut [Result<SectionShape, LineError>],
                      sections: &[usize],
                      f: &[Vector3<f64>]| {
        let mut couplings: std::collections::BTreeMap<usize, SpacerCoupling> = Default::default();
        for (s, force) in spacers.iter().zip(f) {
            let (Some(a), Some(b)) = (index(&s.a.section), index(&s.b.section)) else {
                continue;
            };
            let wind = wind_of(results, s);
            if let Some(at) = point_at(results, &s.b) {
                couplings.entry(a).or_default().holds.push(SpacerHold {
                    id: s.id.clone(),
                    point: s.a.point.clone(),
                    at,
                    length: lengths[&s.id],
                    stiffness: s.stiffness.unwrap_or(line.defaults.insulator_stiffness),
                    tension_only: s.compression.is_none(),
                    weight: s.weight,
                    wind,
                    compression: s.compression,
                });
            }
            // The other end: the bar's force, half its weight, the wind.
            let pull = force + Vector3::new(0.0, 0.0, -0.5 * s.weight) + v(wind);
            let e = couplings
                .entry(b)
                .or_default()
                .pulls
                .entry(s.b.point.clone())
                .or_insert([0.0; 3]);
            for c in 0..3 {
                e[c] += pull[c];
            }
        }
        let solved = crate::jumper::in_parallel(sections, |&k| {
            let section = &line.sections[k];
            let coupling = Coupling {
                jumpers: Some(jumpers.clone()),
                spacers: couplings.get(&k).cloned().unwrap_or_default(),
                ..Coupling::default()
            };
            cable_of(library, section).and_then(|cable| {
                solve_section_coupled(line, section, cable, case, budget, &coupling)
            })
        });
        for (&k, r) in sections.iter().zip(solved) {
            results[k] = r;
        }
    };
    // The sections pulled, then those holding (a section may be both).
    let pulled: Vec<usize> = group
        .iter()
        .copied()
        .filter(|k| spacers.iter().any(|s| index(&s.b.section) == Some(*k)))
        .collect();
    let holding: Vec<usize> = group
        .iter()
        .copied()
        .filter(|k| spacers.iter().any(|s| index(&s.a.section) == Some(*k)))
        .collect();
    for _ in 0..SPACER_PASSES {
        solve_with(results, &pulled, &f);
        solve_with(results, &holding, &f);
        // g(f): the bars' forces on the pulled ends.
        let mut g = Vec::with_capacity(n);
        for s in &spacers {
            let Some(a) = index(&s.a.section) else { return };
            let Ok(shape) = &results[a] else {
                return;
            };
            let Some(state) = shape.spacers.iter().find(|x| x.id == s.id) else {
                fail(
                    results,
                    format!("spacer {}: not solved on {}", s.id, s.a.section),
                );
                return;
            };
            g.push(v(state.on_other));
        }
        let r: Vec<Vector3<f64>> = g.iter().zip(&f).map(|(g, f)| g - f).collect();
        let size = r.iter().map(Vector3::norm).fold(0.0, f64::max);
        let scale = g.iter().map(Vector3::norm).fold(0.0, f64::max);
        if size < 1.0_f64.max(1e-4 * scale) {
            // Settled: the sections as solved with f ≈ g. The pulled ones
            // once more with g, so both sides agree.
            solve_with(results, &pulled, &g);
            return;
        }
        // Aitken: the step from the last two residuals.
        if let Some(prev) = &last {
            let dr: Vec<Vector3<f64>> = r.iter().zip(prev).map(|(a, b)| a - b).collect();
            let den: f64 = dr.iter().map(|x| x.norm_squared()).sum();
            if den > 0.0 {
                let num: f64 = prev.iter().zip(&dr).map(|(a, b)| a.dot(b)).sum();
                omega = (-omega * num / den).clamp(-1.0, 1.0);
                if omega.abs() < 0.05 {
                    omega = 0.05_f64.copysign(omega);
                }
            }
        }
        for (fi, ri) in f.iter_mut().zip(&r) {
            *fi += ri * omega;
        }
        last = Some(r);
    }
    fail(
        results,
        format!("the spacers did not settle in {SPACER_PASSES} passes"),
    );
}

/// Solves a group of sections sharing assemblies together (Dirichlet–
/// Neumann): one section through a shared assembly solves it, the other
/// sections' wires pulling on their clamps in it; the others hold their
/// clamp where that solution puts it, which gives their pull for the next
/// pass. The stiffest section solves it (`coupling_solvers`): a section
/// held there that pulls much harder for a small move of its clamp than
/// the solving one holds it back (a short taut span against a long
/// section on one swinging string) would make the pulls overshoot in turn.
/// The pulls are relaxed by Aitken's factor from the last two passes
/// (`relaxed`), half way at first. Until the held clamps' pulls change
/// less than 0.1 N (or 1e-6 of themselves). `results` holds the sections'
/// solutions, solved alone, and gets the coupled ones; a section that fails
/// keeps its error, the others then stay as solved alone. Not settling in
/// [`COUPLING_PASSES`] passes is an error for the group, naming the pull
/// still changing.
///
/// Expected cost: a few solves of each section of the group (5–15 passes
/// on the existing lines' TAKs).
pub fn solve_coupled(
    line: &Line,
    library: &ohl_cables::Library,
    case: &WeatherCase,
    group: &[usize],
    results: &mut [Result<SectionShape, LineError>],
    budget: &Budget,
    jumpers: &JumperLoadMap,
) {
    type Points = std::collections::BTreeMap<crate::SectionPoint, Point>;
    let solvers = coupling_solvers(line, library, case, group, results);
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
    // Where the solved assemblies put the clamps of the group's points
    // (from the section solving each).
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
                        let q = crate::SectionPoint::new(p.structure.clone(), set);
                        if solvers.get(&q).is_none_or(|id| *id == line.sections[k].id) {
                            held.insert(q, *at);
                        }
                    }
                }
            }
        }
        held
    };
    let Some(mut pulls) = pulls_of(results) else {
        return;
    };
    let mut last: Option<(Vec<Vector3<f64>>, f64)> = None;
    let mut still = String::new();
    for _ in 0..COUPLING_PASSES {
        for &k in group {
            let section = &line.sections[k];
            let coupling = Coupling {
                pulls: pulls.clone(),
                held: held_of(results),
                solvers: solvers.clone(),
                jumpers: Some(jumpers.clone()),
                ..Coupling::default()
            };
            results[k] = cable_of(library, section).and_then(|cable| {
                solve_section_coupled(line, section, cable, case, budget, &coupling)
            });
        }
        let Some(next) = pulls_of(results) else {
            return;
        };
        // The pulls another section's solution takes: those on the clamps
        // held there (the others are only results).
        let held = held_of(results);
        let mut keys = Vec::new();
        let mut r = Vec::new();
        let (mut change, mut worst) = (0.0_f64, None);
        for (p, f) in &next {
            if !held.contains_key(p) {
                pulls.insert(p.clone(), *f);
                continue;
            }
            let d = v(*f) - v(pulls.get(p).copied().unwrap_or(*f));
            let c = d.norm() / (0.1 + 1e-6 * v(*f).norm());
            if c > change {
                (change, worst) = (c, Some((p, d.norm())));
            }
            keys.push(p.clone());
            r.push(d);
        }
        if change < 1.0 {
            return;
        }
        if let Some((p, d)) = worst {
            still = format!(
                " (the pull on a clamp of set {} at {} still changing by {d:.0} N)",
                p.set, p.structure
            );
        }
        let omega = relaxed(&mut last, r.clone());
        for (point, d) in keys.into_iter().zip(r) {
            let f = v(pulls.get(&point).copied().unwrap_or([0.0; 3])) + d * omega;
            pulls.insert(point, p(f));
        }
    }
    for &k in group {
        results[k] = Err(LineError::Invalid(format!(
            "section {}: the sections sharing its strings did not settle together in \
             {COUPLING_PASSES} passes{still}",
            line.sections[k].id
        )));
    }
}

/// Aitken's relaxation factor for a fixed-point pass with residual `r`
/// (the pass's result less what it took), from the last pass's residual
/// and factor in `last` (updated): ½ at first, then between 0.02 and 1.
fn relaxed(last: &mut Option<(Vec<Vector3<f64>>, f64)>, r: Vec<Vector3<f64>>) -> f64 {
    let mut omega = 0.5;
    if let Some((prev, w)) = last.as_ref() {
        omega = *w;
        if prev.len() == r.len() {
            let dr: Vec<Vector3<f64>> = r.iter().zip(prev).map(|(a, b)| a - b).collect();
            let den: f64 = dr.iter().map(Vector3::norm_squared).sum();
            if den > 0.0 {
                let num: f64 = prev.iter().zip(&dr).map(|(a, b)| a.dot(b)).sum();
                omega = (-w * num / den).clamp(0.02, 1.0);
            }
        }
    }
    *last = Some((r, omega));
    omega
}

/// The section solving each assembly shared in `group` (its id), by the
/// section points on it: the stiffest along the line at its end there, of
/// those solved alone in `results` — the least compliance
/// \\( \sum_i L_i / EA + w^2 L_i^3 / (12 H_i^3) \\) over its spans (span
/// lengths \\( L_i \\), horizontal tensions \\( H_i \\), the case's load per
/// metre \\( w \\): the ruling span's), the first in the line's order of
/// equals. An assembly with a section not solved alone keeps the first
/// section through it.
fn coupling_solvers(
    line: &Line,
    library: &ohl_cables::Library,
    case: &WeatherCase,
    group: &[usize],
    results: &[Result<SectionShape, LineError>],
) -> std::collections::BTreeMap<crate::SectionPoint, String> {
    let compliance = |k: usize| -> Option<f64> {
        let shape = results[k].as_ref().ok()?;
        let cable = cable_of(library, &line.sections[k]).ok()?;
        let ea = cable.axial_stiffness().si();
        let w = load_magnitude(case, cable);
        let c: f64 = shape
            .spans
            .iter()
            .filter_map(|s| {
                let (a, b) = (s.points.first()?, s.points.last()?);
                let l = (v(*b) - v(*a)).xy().norm();
                let h = s.horizontal_tension.max(1.0);
                Some(l / ea + w * w * l.powi(3) / (12.0 * h.powi(3)))
            })
            .sum();
        c.is_finite().then_some(c)
    };
    let mut out = std::collections::BTreeMap::new();
    // The group's points by assembly: (structure, its sets).
    let mut assemblies: std::collections::BTreeMap<(&str, Vec<u32>), Vec<usize>> =
        Default::default();
    for &k in group {
        for p in &line.sections[k].points {
            let Some(s) = line.structure(&p.structure) else {
                continue;
            };
            let Some(w) = s.wire_at_set(p.set) else {
                continue;
            };
            let mut sets: Vec<u32> = s
                .component(&w.label)
                .wires
                .iter()
                .filter_map(|&i| s.wires[i].set)
                .collect();
            sets.sort_unstable();
            sets.dedup();
            if sets.len() > 1 {
                let e = assemblies.entry((p.structure.as_str(), sets)).or_default();
                if !e.contains(&k) {
                    e.push(k);
                }
            }
        }
    }
    for ((structure, sets), sections) in assemblies {
        let mut through: Vec<usize> = sections;
        through.sort_unstable();
        let mut best: Option<(usize, f64)> = None;
        for &k in &through {
            let Some(c) = compliance(k) else {
                best = None;
                break;
            };
            if best.is_none_or(|(_, b)| c < b) {
                best = Some((k, c));
            }
        }
        let Some((k, _)) = best else { continue };
        for set in sets {
            out.insert(
                crate::SectionPoint::new(structure.to_owned(), set),
                line.sections[k].id.clone(),
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ohl_core::units::{Length, Stress, Temperature};
    use std::path::Path;

    fn library() -> ohl_cables::Library {
        ohl_cables::Library::load(
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
        windy.wind_from = ohl_weather::WindSide::Left;
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
        for o in bundled.sections[0].objects_mut() {
            o.weight *= 3.0;
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
        assert!(line.sections[0].objects().next().is_some());
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
        for o in bundled.sections[0].objects_mut() {
            o.weight *= 3.0;
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
        let span = line.sections[0].objects().next().unwrap().0.span;
        let mut sized = line.clone();
        sized.sections[0].objects_mut().next().unwrap().shape =
            crate::Shape::Sphere { diameter: 0.6 };
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
                ..Default::default()
            });
            t2.strings.push(crate::InsulatorString {
                label: format!("leg {label}"),
                top: label.into(),
                wire: wire.clone(),
                weight: 500.0,
                ..Default::default()
            });
        }
        line.sections[0].clear_point_loads();
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
        i_line.sections[0].clear_point_loads();
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
                ..Default::default()
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
        line.sections[0].clear_point_loads();
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
        clipped.sections[0].add_point_load(&crate::legacy::SpanLoad {
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

    /// Above the wire's strength (owner, 2026-10-22): a span whose tension
    /// at its most tensioned support exceeds the wire type's RTS is warned
    /// about, by finite elements and by the ruling span; the same section
    /// with the real RTS is not.
    #[test]
    fn a_tension_above_the_rts_is_warned_in_every_solver() {
        let mut lib = library();
        // An invented weak copy of the example's wire: RTS 5 kN.
        lib.add_toml(
            Path::new("weak.ohlw"),
            "format = \"ohl-wire\"\nversion = 1\n[[cable]]\nid = \"weak\"\nname = \"weak\"\n\
             kind = \"acsr\"\ndiameter = \"1.108 in\"\nmass_per_length = \"1.094 lbm/ft\"\n\
             rated_tensile_strength = \"5 kN\"\n\
             outer = { area = \"0.6245 in²\", modulus = \"55 GPa\", thermal_expansion = 23.0e-6, strands = 26 }\n\
             core = { area = \"0.1017 in²\", modulus = \"190 GPa\", thermal_expansion = 11.5e-6, strands = 7 }\n",
        )
        .unwrap();
        let still = WeatherCase::still("still", Temperature::from_celsius(15.0));
        let line = crate::tests::example();
        let strong = solve_line(&line, &lib, &still).remove(0).unwrap();
        assert!(!above_rts(&strong), "{:?}", strong.flags);
        for rs in [false, true] {
            let mut weak = line.clone();
            weak.sections[0].cable = "weak".into();
            if rs {
                weak.sections[0].calculation = crate::condition::Calculation::INITIAL_RS;
            }
            let shape = solve_line(&weak, &lib, &still).remove(0).unwrap();
            assert!(above_rts(&shape), "rs {rs}: {:?}", shape.flags);
            let over: Vec<&String> = shape
                .flags
                .iter()
                .filter(|f| f.contains(RTS_WARNING))
                .collect();
            // Every span is above 5 kN.
            assert_eq!(over.len(), shape.spans.len(), "{over:?}");
            let t = shape.spans[0].max_tension;
            assert!(t > 5000.0);
            assert!(
                over[0].starts_with(&format!("span 1: tension {:.2} kN", t / 1000.0)),
                "{}",
                over[0]
            );
            assert!(
                over[0].contains(&format!("{:.1} %", t / 50.0)),
                "{}",
                over[0]
            );
        }
    }

    /// A string's limits (not checked) come with its solved force and
    /// swing: a swing beyond its maximum and a load beyond its rating flag
    /// nothing; the swing follows from its top and wire point.
    #[test]
    fn string_limits_are_reported_not_checked() {
        let lib = library();
        let windy = WeatherCase {
            wind_pressure: Stress::from_pascals(500.0),
            ..WeatherCase::still("wind", Temperature::from_celsius(15.0))
        };
        let mut line = crate::tests::example();
        line.sections[0].clear_point_loads();
        let t2 = line.structures.iter_mut().find(|s| s.id == "T2").unwrap();
        let wire = t2
            .wires
            .iter()
            .find(|w| w.set == Some(1))
            .unwrap()
            .label
            .clone();
        let label = t2.strings_to(&wire).next().unwrap().label.clone();
        let limits = crate::StringLimits {
            max_load: Some(1.0),
            max_swing_deg: Some(0.5),
            max_swing_across_deg: None,
            max_swing_along_deg: Some(0.1),
        };
        t2.strings
            .iter_mut()
            .find(|x| x.label == label)
            .unwrap()
            .set_limits(limits);
        line.validate().unwrap();
        let shape = solve_line(&line, &lib, &windy).remove(0).unwrap();
        let x = shape.strings[1].iter().find(|x| x.label == label).unwrap();
        assert_eq!(x.limits, limits);
        assert!(x.force > 1.0 && x.swing.total_deg > 0.5, "{x:?}");
        assert!(shape.flags.is_empty(), "nothing flagged: {:?}", shape.flags);
        let o = line.structure("T2").unwrap().orientation_deg;
        let again = crate::swing(o, x.top, x.bottom);
        assert_eq!(again, x.swing);
        // Its parts: tan² total = tan² across + tan² along.
        let t = |d: f64| d.to_radians().tan().powi(2);
        assert!(
            (t(x.swing.total_deg) - t(x.swing.across_deg) - t(x.swing.along_deg)).abs() < 1e-9,
            "{:?}",
            x.swing
        );
        // Another string: no limits given.
        assert!(shape.strings[0].iter().all(|x| x.limits.is_empty()));
    }

    /// The swing of a string: across the line positive to the right,
    /// along it positive ahead, on a structure turned any way.
    #[test]
    fn swing_angles_in_the_structures_axes() {
        // Local x (right) north: right of the line is +y global.
        let s = crate::swing(90.0, [0.0, 0.0, 10.0], [0.0, 1.0, 9.0]);
        assert!((s.total_deg - 45.0).abs() < 1e-9, "{s:?}");
        assert!((s.across_deg - 45.0).abs() < 1e-9, "{s:?}");
        assert!(s.along_deg.abs() < 1e-9, "{s:?}");
        // Ahead (local y) is west then.
        let s = crate::swing(90.0, [0.0, 0.0, 10.0], [-1.0, 0.0, 9.0]);
        assert!((s.along_deg - 45.0).abs() < 1e-9 && s.across_deg.abs() < 1e-9);
        let s = crate::swing(0.0, [0.0, 0.0, 10.0], [-1.0, 0.0, 10.0 - 3f64.sqrt()]);
        assert!((s.across_deg + 30.0).abs() < 1e-9, "{s:?}");
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
        line.sections[0].clear_point_loads();
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

    /// A TAK between a long section (four 400 m spans on suspension
    /// strings) and a short taut span (80 m), strung 5 kN harder: the
    /// short span pulls ~80 times harder for a move of its end than the
    /// long one and the TAK's string hold it back. Held where the long
    /// section put the TAK, its pull overshot in turn (the passes never
    /// settled, as on the second existing line); the short span solves the
    /// TAK, the long one holds its end there.
    #[test]
    fn a_tak_between_a_long_section_and_a_short_taut_span_settles() {
        let cable = crate::tests::example().sections[0].cable.clone();
        let suspension = |id: &str, x: f64| {
            format!(
                r#"{{"id": "{id}", "position": [{x}, 0, 0],
                    "points": [{{"label": "P", "position": [{x}, 0, 45]}}],
                    "wires": [{{"label": "W", "position": [{x}, 0, 42], "set": 1}}],
                    "strings": [{{"label": "s", "top": "P", "wire": "W", "weight": 300}}]}}"#
            )
        };
        let text = format!(
            r#"{{
            "name": "tak", "structures": [
              {{"id": "A", "position": [0, 0, 0], "wires": [{{"label": "W", "position": [0, 0, 42], "set": 1}}]}},
              {s1}, {s2}, {s3},
              {{"id": "B", "position": [1600, 0, 0],
                "points": [{{"label": "K", "position": [1600, 0, 45]}}],
                "wires": [
                  {{"label": "N", "position": [1600, 0, 42]}},
                  {{"label": "EB", "position": [1599.7, 0, 42], "set": 1}},
                  {{"label": "EF", "position": [1600.3, 0, 42], "set": 2}}],
                "strings": [
                  {{"label": "k", "top": "K", "wire": "N", "weight": 820}},
                  {{"label": "b", "top": "N", "wire": "EB"}},
                  {{"label": "f", "top": "N", "wire": "EF"}}]}},
              {{"id": "C", "position": [1680, 0, 0], "wires": [{{"label": "W", "position": [1680, 0, 42], "set": 1}}]}}],
            "sections": [
              {{"id": "X", "cable": "{cable}", "points": [{{"structure": "A", "set": 1}}, {{"structure": "S1", "set": 1}},
                {{"structure": "S2", "set": 1}}, {{"structure": "S3", "set": 1}}, {{"structure": "B", "set": 1}}],
                "stringing": {{"kind": "tension", "horizontal_tension": 20000, "temperature": 15}}}},
              {{"id": "Y", "cable": "{cable}", "points": [{{"structure": "B", "set": 2}}, {{"structure": "C", "set": 1}}],
                "stringing": {{"kind": "tension", "horizontal_tension": 25000, "temperature": 15}}}}]
            }}"#,
            s1 = suspension("S1", 400.0),
            s2 = suspension("S2", 800.0),
            s3 = suspension("S3", 1200.0),
        );
        let line = Line::from_json(&text).unwrap();
        assert_eq!(coupled_groups(&line), vec![vec![0, 1]]);
        let lib = library();
        for t in [-20.0, 15.0, 60.0] {
            let case = WeatherCase::still("t", Temperature::from_celsius(t));
            let together: Vec<SectionShape> = solve_line(&line, &lib, &case)
                .into_iter()
                .map(|r| r.unwrap_or_else(|e| panic!("{t} °C: {e}")))
                .collect();
            // Y, the stiffer, solves the TAK and reports its loads; X holds
            // its end where Y puts it (as Y's last pass but one put it:
            // within what X's pull changed by then, 0.1 N).
            let end_of_x = together[1].strings[0]
                .iter()
                .find(|s| s.label == "b")
                .map(|s| v(s.bottom))
                .expect("Y solves the TAK");
            assert!((end_of_x - v(together[0].clamps[4])).norm() < 1e-4);
            assert!(together[0].attachment_loads[4].is_empty());
            assert!(!together[1].attachment_loads[0].is_empty());
            // In balance: Y's model pulls X's end as hard as X's wires do.
            let pull_x = v(together[0].pulls[4]);
            let b = together[1].strings[0]
                .iter()
                .find(|s| s.label == "b")
                .unwrap();
            let along_b = (v(b.bottom) - v(b.top)).normalize();
            assert!(
                (pull_x.dot(&along_b) - b.force).abs() < 1e-3 * b.force + 1.0,
                "{t} °C: {} vs {}",
                pull_x.dot(&along_b),
                b.force
            );
        }
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
        line.sections[0].clear_point_loads();
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
        line.sections[0].clear_point_loads();
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
        line.sections[0].clear_point_loads();
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
        iced.ice_thickness = ohl_core::units::Length::from_si(0.02);
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

    /// The example line with S2's dead-end strings drooping 12° (as a
    /// weight can hold them), given as geometry only (no weight).
    fn with_drooping_dead_ends() -> Line {
        let mut line = crate::tests::example();
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
            // The string's geometry taken again from where it now ends.
            for x in st.strings.iter_mut().filter(|x| x.wire == wire) {
                x.length = 0.0;
            }
        }
        line.place_strings().unwrap();
        line
    }

    /// A strain assembly for the insulator library, of `weight` N.
    fn strain_assembly(id: &str, weight: f64) -> ohl_structures::InsulatorAssembly {
        use ohl_core::units::{Area, Force, Length};
        ohl_structures::InsulatorAssembly {
            format: None,
            version: None,
            id: id.into(),
            name: id.into(),
            kind: ohl_structures::AssemblyKind::Strain,
            source: None,
            length: Length::from_meters(3.0),
            weight: Force::from_newtons(weight),
            axial_stiffness: Force::from_newtons(DEFAULT_INSULATOR_STIFFNESS),
            wind_area: Area::from_square_meters(0.0),
            half_width: None,
            max_load: None,
            max_swing: None,
            max_swing_across: None,
            max_swing_along: None,
        }
    }

    /// A strain string whose resolved weight is 0 counts as without a
    /// weight whether or not it names an insulator: linked to a 0 N entry,
    /// S2 solves exactly as unlinked (its weights estimated); an entry with
    /// a real weight gives that weight (no estimate). Before 2026-10-21 the
    /// 0 N link dropped the estimate (on a real line: median sag
    /// differences 0.7–2.4 cm → 2–9 cm).
    #[test]
    fn a_zero_newton_insulator_keeps_the_estimated_weight() {
        let lib = library();
        let line = with_drooping_dead_ends();
        let s2 = line.sections.iter().find(|s| s.id == "S2").unwrap().clone();
        let cable = lib.get(&s2.cable).unwrap();
        let ends: Vec<(String, String)> = [&s2.points[0], s2.points.last().unwrap()]
            .iter()
            .map(|p| {
                let st = line.structure(&p.structure).unwrap();
                let wire = &st.wire_at_set(p.set).unwrap().label;
                (
                    st.id.clone(),
                    st.strings_to(wire).next().unwrap().label.clone(),
                )
            })
            .collect();
        let linked = |id: &str, weight: f64| {
            let mut insulators = ohl_structures::InsulatorLibrary::default();
            insulators
                .insert(Path::new(id), strain_assembly(id, weight))
                .unwrap();
            let mut l = line.clone();
            for (structure, label) in &ends {
                let st = l
                    .structures
                    .iter_mut()
                    .find(|s| s.id == *structure)
                    .unwrap();
                let x = st.strings.iter_mut().find(|x| x.label == *label).unwrap();
                assert_eq!(x.weight, 0.0);
                x.insulator = Some(id.into());
            }
            l.resolve(&ohl_structures::StructureLibrary::default(), &insulators)
                .unwrap()
        };
        let case = WeatherCase::still("80", Temperature::from_celsius(80.0));
        let solve = |l: &Line| solve_section(l, &s2, cable, &case, &default_fe_budget()).unwrap();
        let plain = solve(
            &line
                .resolve(
                    &ohl_structures::StructureLibrary::default(),
                    &ohl_structures::InsulatorLibrary::default(),
                )
                .unwrap(),
        );
        let n = plain.estimated_weights.len();
        assert!(plain.estimated_weights[0].is_some_and(|w| w > 0.0));
        assert!(plain.estimated_weights[n - 1].is_some_and(|w| w > 0.0));
        let zero = solve(&linked("Strain-3.000m-0N", 0.0));
        assert_eq!(zero.estimated_weights, plain.estimated_weights);
        assert_eq!(zero.clamps, plain.clamps);
        for (a, b) in zero.spans.iter().zip(&plain.spans) {
            assert_eq!(a.horizontal_tension, b.horizontal_tension);
            assert_eq!(a.sag, b.sag);
        }
        // A real weight overrides the estimate.
        let heavy = solve(&linked("Strain-heavy", 900.0));
        assert_eq!(heavy.estimated_weights[0], None);
        assert_eq!(heavy.estimated_weights[n - 1], None);
        assert!((heavy.clamps[0][2] - plain.clamps[0][2]).abs() > 1e-3);
    }

    /// A dead-end string given as geometry only (no weight): its weight is
    /// estimated so that it stays where the data puts it in the stringing
    /// state (S2: clipped in at 15 °C), and it moves with the weather.
    #[test]
    fn weightless_strain_strings_stay_put_at_stringing_and_move_with_the_weather() {
        let lib = library();
        // The example's dead-end strings droop less (5.7°) than the wire
        // pulls them down (8.4°): no weight holds them there (estimated 0;
        // freed, they drop 0.23 m). Drooping 12°, as a weight can hold them.
        let line = with_drooping_dead_ends();
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
    fn with_nonlinear(mut lib: ohl_cables::Library) -> ohl_cables::Library {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../ohl-sagtension/tests/data/synthetic-cable.ohlw");
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
            assert_eq!(Calculation::parse(&c.label()), Some(c));
            assert_eq!(Calculation::parse(&c.name()), Some(c));
            let json = serde_json::to_string(&c).unwrap();
            assert_eq!(serde_json::from_str::<Calculation>(&json).unwrap(), c);
        }
        assert_eq!(
            Calculation::parse("max sag fe"),
            Some(Calculation::MAX_SAG_FE)
        );
        assert_eq!(Calculation::CREEP_RS.condition(), Condition::Creep);
        assert_eq!(Calculation::CREEP_RS.method(), Method::Rs);
        assert_eq!(
            serde_json::to_string(&Calculation::CREEP_RS).unwrap(),
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
        line.sections[0].clear_point_loads();
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
        line.sections[0].clear_point_loads();
        let cable = lib.get(&line.sections[0].cable).unwrap();
        let budget = default_fe_budget();
        for t in [15.0, 60.0] {
            let case = WeatherCase::still("t", Temperature::from_celsius(t));
            line.sections[0].calculation = Calculation::INITIAL_FE;
            let fe = solve_section(&line, &line.sections[0], cable, &case, &budget).unwrap();
            line.sections[0].calculation = Calculation::INITIAL_RS;
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
        line.sections[0].calculation = Calculation::INITIAL_RS;
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
        for calculation in [Calculation::INITIAL_RS, Calculation::INITIAL_FE] {
            let mut cable = lib.get("acsr-drake-26-7").unwrap().clone();
            let mut line = crate::tests::example();
            line.sections[0].clear_point_loads();
            line.sections[0].calculation = calculation;
            let free = sag(&line, &cable);
            // The line's choice for a cable without its own …
            line.bimetallic.behavior = ohl_cables::BimetallicBehavior::BirdCage;
            let by_line = sag(&line, &cable);
            // … and the cable's own, which wins over the line's.
            cable.bimetallic_behavior = Some(ohl_cables::BimetallicBehavior::Compression);
            assert!(
                (sag(&line, &cable) - free).abs() < 1e-9,
                "the cable's own wins"
            );
            line.bimetallic = Default::default();
            cable.bimetallic_behavior = Some(ohl_cables::BimetallicBehavior::BirdCage);
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
        cable.model = ohl_cables::CableModel::LinearCreepTemperature;
        cable.creep_temperature_shift = Some(20.0);
        let mut line = crate::tests::example();
        let every = WeatherCase::still("everyday", Temperature::from_celsius(15.0));
        line.creep_stretch.creep_case = Some("everyday".into());
        line.set_weather_cases(std::slice::from_ref(&every));
        let budget = default_fe_budget();
        line.sections[0].calculation = Calculation::CREEP_FE;
        let crept = solve_section(&line, &line.sections[0], &cable, &every, &budget).unwrap();
        line.sections[0].calculation = Calculation::INITIAL_FE;
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
                sag(&line, Calculation::INITIAL_FE, t),
                sag(&line, Calculation::INITIAL_RS, t),
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

    /// The data sheet's weight at its reference temperature, taken at the
    /// stringing temperature (owner decision 2026-10-21). One dead-end span
    /// (S3's first, its wires fixed at the structures) strung at 10 °C: in
    /// still air at 10 °C the supports carry its weight, the data sheet's
    /// 20 °C value per metre at 10 °C times its unstressed length,
    /// \\( w e^{10\alpha} L_0 \\) (hand check), by the finite elements;
    /// a cable whose data sheet gives 10 °C weighs \\( w L_0 \\). The ruling
    /// span takes the additive form's \\( w / (1 - 10\alpha) \\).
    #[test]
    fn the_weight_is_the_data_sheets_at_the_stringing_temperature() {
        use crate::condition::Calculation;
        let lib = library();
        let mut line = crate::tests::example();
        line.sections.retain(|s| s.id == "S3");
        let s = &mut line.sections[0];
        s.points.truncate(2);
        s.calculation = Calculation::INITIAL_FE;
        s.stringing = Stringing::Tension {
            horizontal_tension: 20_000.0,
            temperature: 10.0,
        };
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
        let cable = lib.get(&line.sections[0].cable).unwrap().clone();
        let alpha = cable.thermal_expansion();
        let t10 = Temperature::from_celsius(10.0);
        let budget = default_fe_budget();
        let carried = |cable: &Cable| {
            let section = &line.sections[0];
            let (lengths, _) = clip_lengths(&line, section, cable).unwrap();
            let shape = solve_section(
                &line,
                section,
                cable,
                &WeatherCase::still("10", t10),
                &budget,
            )
            .unwrap();
            let up: f64 = shape
                .attachment_loads
                .iter()
                .flatten()
                .map(|s| -s.force[2])
                .sum();
            (up, lengths[0])
        };
        let (up, l0) = carried(&cable);
        let w = cable.weight().si();
        let expected = w * (10.0 * alpha).exp() * l0;
        assert!(
            ((up - expected) / expected).abs() < 1e-9,
            "{up} vs {expected}"
        );
        // Its data sheet at 10 °C: the weight as given.
        let mut at10 = cable.clone();
        at10.reference_temperature = Some(t10);
        let (up10, l10) = carried(&at10);
        assert!(((up10 - w * l10) / (w * l10)).abs() < 1e-9, "{up10}");
        // The conversion, idempotent; the ruling span's additive form.
        let section = &line.sections[0];
        let strung = strung_cable(section, &cable).unwrap();
        assert_eq!(strung.reference_temperature, Some(t10));
        assert!(matches!(
            strung_cable(section, &strung).unwrap(),
            std::borrow::Cow::Borrowed(_)
        ));
        let mut rs = section.clone();
        rs.calculation = Calculation::INITIAL_RS;
        let additive = strung_cable(&rs, &cable).unwrap().weight().si();
        assert!((additive / (w / (1.0 - 10.0 * alpha)) - 1.0).abs() < 1e-12);
    }

    /// The finite elements carry the ice per metre of the stretched wire
    /// (owner decision 2026-10-21): on the same dead-end span under 12 mm
    /// of ice the supports carry the own weight per unstressed metre and
    /// the ice per metre of the span's real length — more than the ice on
    /// its unstressed length by about its mean strain (hand check of the
    /// vertical reactions, to the strain's spread along the span).
    #[test]
    fn ice_is_carried_per_stretched_metre() {
        let lib = library();
        let mut line = crate::tests::example();
        line.sections.retain(|s| s.id == "S3");
        line.sections[0].points.truncate(2);
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
        let cable = lib.get(&line.sections[0].cable).unwrap();
        let section = &line.sections[0];
        let t = line.sections[0].stringing.temperature();
        let mut iced = WeatherCase::still("ice", Temperature::from_celsius(t));
        iced.ice_thickness = Length::from_millimeters(12.0);
        iced.ice_density = Some(ohl_core::units::Density::from_si(913.0));
        let ice =
            iced.wire_load(cable.diameter, cable.weight()).vertical.si() - cable.weight().si();
        let shape = solve_section(&line, section, cable, &iced, &default_fe_budget()).unwrap();
        let up: f64 = shape
            .attachment_loads
            .iter()
            .flatten()
            .map(|s| -s.force[2])
            .sum();
        let (lengths, _) = clip_lengths(&line, section, cable).unwrap();
        let own = strung_cable(section, cable).unwrap().weight().si() * lengths[0];
        let mean_strain = 0.5 * (shape.spans[0].horizontal_tension + shape.spans[0].max_tension)
            / cable.axial_stiffness().si();
        let expected = own + ice * lengths[0] * (1.0 + mean_strain);
        assert!(
            ((up - expected) / expected).abs() < 2e-6,
            "{up} vs {expected}: ice on the unstressed length {}",
            own + ice * lengths[0]
        );
        assert!(up > own + ice * lengths[0] * (1.0 + 0.5 * mean_strain));
        // The solver option per unstretched metre (2026-10-21, as other
        // programs): the supports carry the ice on the unstressed length
        // exactly, own + ice L0 (hand check), and the span less tension
        // and sag by about the mean strain's share of the ice.
        let mut other = line.clone();
        other.solver.loads_per = crate::LoadsPer::Unstretched;
        let section = &other.sections[0];
        let plain = solve_section(&other, section, cable, &iced, &default_fe_budget()).unwrap();
        let up_plain: f64 = plain
            .attachment_loads
            .iter()
            .flatten()
            .map(|s| -s.force[2])
            .sum();
        let expected = own + ice * lengths[0];
        assert!(
            ((up_plain - expected) / expected).abs() < 1e-9,
            "{up_plain} vs {expected}"
        );
        let less = plain.spans[0].horizontal_tension / shape.spans[0].horizontal_tension - 1.0;
        assert!(less < -1e-4 && less > -1e-3, "{less}");
        assert!(plain.spans[0].sag < shape.spans[0].sag);
        // Still air: the same either way.
        let still = WeatherCase::still("still", Temperature::from_celsius(t));
        let a = solve_section(
            &line,
            &line.sections[0],
            cable,
            &still,
            &default_fe_budget(),
        );
        let b = solve_section(&other, section, cable, &still, &default_fe_budget());
        assert_eq!(
            a.unwrap().spans[0].horizontal_tension,
            b.unwrap().spans[0].horizontal_tension
        );
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
        ice.ice_thickness = ohl_core::units::Length::from_millimeters(15.0);
        // Without criteria: said so.
        line.sections[0].calculation = Calculation::CREEP_FE;
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
            use ohl_cables::mechanics::{MaterialState, Mechanics};
            use ohl_core::units::Force;
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
                Calculation::INITIAL_FE,
                Calculation::CREEP_FE,
                Calculation::LOAD_FE,
                Calculation::MAX_SAG_FE,
            ),
            (
                Calculation::INITIAL_RS,
                Calculation::CREEP_RS,
                Calculation::LOAD_RS,
                Calculation::MAX_SAG_RS,
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

#[cfg(test)]
mod span_tests {
    use super::*;
    use ohl_core::units::Temperature;

    fn library() -> ohl_cables::Library {
        ohl_cables::Library::load(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
        )
        .unwrap()
    }

    /// Spread loads hang on the stretch of wire they cover: over a whole
    /// span the sag grows; over its first half the low point moves towards
    /// it. Span points are nodes on the wire, reported where they are
    /// placed (by fraction or distance), on the solved wire.
    #[test]
    fn spread_loads_and_span_points_on_the_wire() {
        let lib = library();
        let case = WeatherCase::still("15", Temperature::from_celsius(15.0));
        let base = crate::tests::example();
        let shape = |line: &Line| solve_line(line, &lib, &case)[0].clone().unwrap();
        let plain = shape(&base);
        let span = &plain.spans[0];
        // Over the whole first span, as heavy as the wire.
        let cable = lib.get(&base.sections[0].cable).unwrap();
        let q = cable.weight().si() * f64::from(base.sections[0].bundle.max(1));
        let mut whole = base.clone();
        whole.sections[0].add_spread_load(&crate::legacy::SpreadLoad {
            span: 0,
            start: crate::SpanPlace::at_fraction(0.0),
            end: crate::SpanPlace::at_fraction(1.0),
            load: q,
            label: "as heavy as the wire".into(),
        });
        let heavy = shape(&whole);
        assert!(
            heavy.spans[0].sag > span.sag * 1.05,
            "{} {}",
            heavy.spans[0].sag,
            span.sag
        );
        // Over the first half: the low point towards it.
        let mut half = base.clone();
        half.sections[0].add_spread_load(&crate::legacy::SpreadLoad {
            span: 0,
            start: crate::SpanPlace::at_fraction(0.0),
            end: crate::SpanPlace::at_fraction(0.5),
            load: 3.0 * q,
            label: String::new(),
        });
        let lopsided = shape(&half);
        let along = |p: Point, s: &SectionShape| {
            let (a, b) = (s.clamps[0], s.clamps[1]);
            ((p[0] - a[0]).hypot(p[1] - a[1])) / ((b[0] - a[0]).hypot(b[1] - a[1]))
        };
        assert!(
            along(lopsided.spans[0].low_point, &lopsided) < along(span.low_point, &plain) - 0.02,
            "{} {}",
            along(lopsided.spans[0].low_point, &lopsided),
            along(span.low_point, &plain)
        );
        // Span points: where placed, on the wire.
        let mut pointed = base.clone();
        let horizontal = {
            let (a, b) = (plain.clamps[0], plain.clamps[1]);
            (b[0] - a[0]).hypot(b[1] - a[1])
        };
        pointed.sections[0].span_points.push(crate::SpanPoint {
            label: "P1".into(),
            span: 0,
            at: crate::SpanPlace::at_fraction(0.25),
            description: String::new(),
            object: Default::default(),
        });
        pointed.sections[0].span_points.push(crate::SpanPoint {
            label: "P2".into(),
            span: 0,
            at: crate::SpanPlace::at_distance(0.6 * horizontal),
            description: String::new(),
            object: Default::default(),
        });
        let s = shape(&pointed);
        let mine: Vec<&(String, Point)> = s
            .span_points
            .iter()
            .filter(|(l, _)| l == "P1" || l == "P2")
            .collect();
        assert_eq!(mine.len(), 2);
        for ((label, at), f) in mine.into_iter().zip([0.25, 0.6]) {
            assert!(
                (along(*at, &s) - f).abs() < 0.01,
                "{label} {}",
                along(*at, &s)
            );
            assert!(
                at[2] < s.clamps[0][2].max(s.clamps[1][2]),
                "below the clamps"
            );
        }
        // By station: half-way between the span's structures' stations.
        let (sa, sb) = pointed
            .span_stations(
                &pointed.sections[0].points[0].structure,
                &pointed.sections[0].points[1].structure,
            )
            .expect("the span on an alignment");
        let mut by_station = base.clone();
        by_station.sections[0].span_points.push(crate::SpanPoint {
            label: "S".into(),
            span: 0,
            at: crate::SpanPlace::at_station(0.5 * (sa + sb)),
            description: String::new(),
            object: Default::default(),
        });
        by_station.validate().unwrap();
        let resolved = by_station.section_with_fractions(&by_station.sections[0]);
        assert!((resolved.span_points[0].at.fraction().unwrap() - 0.5).abs() < 1e-9);
        let at = shape(&by_station).span_points[0].1;
        let st = shape(&by_station);
        assert!((along(at, &st) - 0.5).abs() < 0.02, "{}", along(at, &st));
        // A span point alone changes nothing of the shape (the sag as
        // sampled: within a millimetre).
        assert!(
            (s.spans[0].sag - span.sag).abs() < 1e-3,
            "{} {}",
            s.spans[0].sag,
            span.sag
        );
    }

    /// A spread on each subconductor of a 2-bundle carries twice the load
    /// of one on the whole bundle: it hangs as a whole-bundle spread of
    /// twice the load, lower than the one of the same load.
    #[test]
    fn a_spread_on_each_subconductor_carries_the_bundle_times_its_load() {
        let lib = library();
        let case = WeatherCase::still("15", Temperature::from_celsius(15.0));
        let mut base = crate::tests::example();
        base.jumpers.clear();
        base.sections[0].bundle = 2;
        base.sections[0].bundle_spacing = Some(0.4);
        let with = |load: f64, per_subconductor: bool| {
            let mut l = base.clone();
            l.sections[0].add_spread_load(&crate::legacy::SpreadLoad {
                span: 0,
                start: crate::SpanPlace::at_fraction(0.0),
                end: crate::SpanPlace::at_fraction(1.0),
                load,
                label: String::new(),
            });
            l.sections[0].spreads[0].per_subconductor = per_subconductor;
            l.validate().unwrap();
            assert_eq!(
                l.sections[0].spread_loads()[0].load,
                if per_subconductor { 2.0 * load } else { load }
            );
            solve_line(&l, &lib, &case)[0].clone().unwrap().spans[0].sag
        };
        let q = 20.0;
        let (whole, each, twice) = (with(q, false), with(q, true), with(2.0 * q, false));
        assert!(each > whole + 0.01, "{each} {whole}");
        assert!((each - twice).abs() < 1e-6, "{each} {twice}");
    }
}

#[cfg(test)]
mod spacer_tests {
    use super::*;
    use ohl_core::units::Temperature;

    fn library() -> ohl_cables::Library {
        ohl_cables::Library::load(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
        )
        .unwrap()
    }

    /// The project's default values in the solve: a string without its
    /// own EA takes the project's (a soft one lets the clamps down); a
    /// jumper without its own exit angle the project's.
    #[test]
    fn the_project_defaults_are_solved_with() {
        let lib = library();
        let case = ohl_weather::WeatherCase::still("15", Temperature::from_celsius(15.0));
        let mut line = crate::tests::example();
        for s in &mut line.structures {
            for x in &mut s.strings {
                x.stiffness = None;
            }
        }
        let low = |line: &Line| -> f64 {
            solve_line(line, &lib, &case)
                .into_iter()
                .map(Result::unwrap)
                .flat_map(|s| s.clamps.clone())
                .map(|c| c[2])
                .sum()
        };
        let stiff = low(&line);
        line.defaults.insulator_stiffness = 5e5;
        let soft = low(&line);
        assert!(soft < stiff - 0.01, "{soft} {stiff}");
        // A jumper's exit angle.
        let mut line = crate::tests::example();
        let j = line
            .jumpers
            .iter_mut()
            .find(|j| j.angle_from_deg.is_none() && !j.rigid)
            .expect("a jumper without its own angle");
        let id = j.id.clone();
        let shape = |line: &Line| {
            let shapes: Vec<SectionShape> = solve_line(line, &lib, &case)
                .into_iter()
                .map(Result::unwrap)
                .collect();
            crate::jumper::solve_jumpers(line, &lib, &case, &shapes)
                .into_iter()
                .map(Result::unwrap)
                .find(|x| x.jumper == id)
                .unwrap()
                .points
        };
        let at30 = shape(&line);
        line.defaults.jumper_exit_angle_deg = 60.0;
        let at60 = shape(&line);
        let moved = at30
            .iter()
            .zip(&at60)
            .map(|(a, b)| (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f64>().sqrt())
            .fold(0.0, f64::max);
        assert!(moved > 0.01, "{moved}");
    }

    /// S1 and S2 (both T1–T2) with span points at mid-span and a spacer
    /// between them of `length` (none: from stringing).
    fn spaced(length: Option<f64>, compression: Option<f64>) -> Line {
        let mut line = crate::tests::example();
        for s in &mut line.sections[..2] {
            s.span_points.push(crate::SpanPoint {
                label: "M".into(),
                span: 0,
                at: crate::SpanPlace::at_fraction(0.5),
                description: String::new(),
                // A spacer's end: a clamp (of no weight here).
                object: Some(crate::LineObject::clamp(0.0)),
            });
        }
        line.spacers.push(crate::Spacer {
            id: "SP".into(),
            a: crate::SpanPointRef {
                section: line.sections[0].id.clone(),
                point: "M".into(),
            },
            b: crate::SpanPointRef {
                section: line.sections[1].id.clone(),
                point: "M".into(),
            },
            length,
            weight: 200.0,
            compression,
            ..Default::default()
        });
        line.validate().unwrap();
        line
    }

    /// Sections joined by a spacer, one of them changed edit after edit
    /// and brought up to date incrementally: the same shapes as a full
    /// solve (the group coupled again from each solved alone, so the
    /// result does not depend on the edits before).
    #[test]
    fn a_spacer_group_brought_up_to_date_is_a_full_solve() {
        let lib = library();
        let case = WeatherCase::still("15", Temperature::from_celsius(15.0));
        let mut line = spaced(Some(0.3), None);
        let mut live = crate::incremental::SolvedLine::solve(&line, &lib, &case);
        let id = line.sections[0].id.clone();
        for h in [22_000.0, 26_000.0, 24_000.0] {
            line.sections[0].stringing = crate::Stringing::Tension {
                horizontal_tension: h,
                temperature: 15.0,
            };
            live.update(&line, &lib, std::slice::from_ref(&id));
            let full: Vec<SectionShape> = solve_line(&line, &lib, &case)
                .into_iter()
                .map(Result::unwrap)
                .collect();
            assert_eq!(live.shapes(&line), full, "after H = {h}");
        }
    }

    fn mid(shape: &SectionShape) -> Vector3<f64> {
        v(shape.span_points[0].1)
    }

    /// A spacer between two conductors of a span: shorter than their gap,
    /// it pulls them together (its ends at its length, in tension, equal
    /// and opposite on both); of the stringing distance it takes almost
    /// nothing in still air at that temperature; longer, it pushes them
    /// apart in compression — flagged beyond its limit, slack if it takes
    /// none.
    #[test]
    fn a_spacer_joins_two_conductors() {
        let lib = library();
        let case = WeatherCase::still("15", Temperature::from_celsius(15.0));
        let solve = |line: &Line| -> Vec<SectionShape> {
            solve_line(line, &lib, &case)
                .into_iter()
                .map(|r| r.unwrap())
                .collect()
        };
        let free = solve(&crate::tests::example());
        let gap = (mid_of(&free[0]) - mid_of(&free[1])).norm();
        fn mid_of(s: &SectionShape) -> Vector3<f64> {
            let p = &s.spans[0].points;
            v(p[p.len() / 2])
        }
        assert!(gap > 1.0, "{gap}");
        // Shorter: pulled together to its length.
        let short = solve(&spaced(Some(0.7 * gap), Some(10_000.0)));
        let (a, b) = (mid(&short[0]), mid(&short[1]));
        assert!(
            ((a - b).norm() - 0.7 * gap).abs() < 0.01,
            "{} {}",
            (a - b).norm(),
            0.7 * gap
        );
        let state = &short[0].spacers[0];
        assert!(state.force > 0.0, "tension: {}", state.force);
        assert!(!state.slack && !state.over);
        assert!(short[1].spacers.is_empty(), "held by its end a");
        // Of the stringing distance (stringing at 15 °C here or not, it is
        // its length at stringing): a small force in still air.
        let set = solve(&spaced(None, Some(10_000.0)));
        let s = &set[0].spacers[0];
        assert!(
            s.force.abs() < 0.05 * state.force.abs().max(100.0),
            "{}",
            s.force
        );
        // Longer: pushed apart, in compression; flagged beyond its limit.
        let long = solve(&spaced(Some(1.3 * gap), Some(50.0)));
        let s = &long[0].spacers[0];
        assert!(s.force < 0.0, "compression: {}", s.force);
        assert!(s.over && long[0].flags.iter().any(|f| f.contains("spacer SP")));
        assert!((mid(&long[0]) - mid(&long[1])).norm() > gap);
        // Tension only: slack, nothing pushes.
        let slack = solve(&spaced(Some(1.3 * gap), None));
        let s = &slack[0].spacers[0];
        assert!(s.slack && s.force.abs() < 1.0, "{s:?}");
        // Its weight: the two clamps lower than without it.
        assert!(mid(&slack[1])[2] < mid_of(&free[1])[2] + 1e-6);
        // The incremental solve: the same.
        let line = spaced(Some(0.7 * gap), Some(10_000.0));
        let inc = crate::incremental::SolvedLine::solve(&line, &lib, &case);
        assert_eq!(inc.shapes(&line), solve(&line));
    }
}

#[cfg(test)]
mod span_case_tests {
    use super::*;
    use crate::condition::Calculation;
    use ohl_core::units::{Length, Stress, Temperature};
    use std::path::Path;

    fn library() -> ohl_cables::Library {
        ohl_cables::Library::load(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
        )
        .unwrap()
    }

    fn iced(t: f64, ice_mm: f64, wind: f64) -> WeatherCase {
        WeatherCase {
            ice_thickness: Length::from_millimeters(ice_mm),
            wind_pressure: Stress::from_pascals(wind),
            ..WeatherCase::still("iced", Temperature::from_celsius(t))
        }
    }

    /// Every span in the section's case: the section solved bit for bit
    /// as with its one case — every section of the example (rolling,
    /// clipped, marker balls, jumpers), still, hot, iced and windy; also
    /// with a span given the section's case as its own.
    #[test]
    fn one_case_for_every_span_is_the_section_solved_as_before() {
        let line = crate::tests::example();
        let lib = library();
        let budget = default_fe_budget();
        let cases = [
            WeatherCase::still("15", Temperature::from_celsius(15.0)),
            WeatherCase::still("80", Temperature::from_celsius(80.0)),
            iced(-5.0, 20.0, 400.0),
        ];
        let mut solved = 0;
        for case in &cases {
            for section in &line.sections {
                let cable = lib.get(&section.cable).unwrap();
                let one = solve_section(&line, section, cable, case, &budget);
                let all = SpanCases::uniform(case.clone());
                let uniform = solve_section_span_cases(&line, section, cable, &all, &budget);
                let given = all.clone().with_span(0, case.clone());
                assert!(given.is_uniform());
                let own = solve_section_span_cases(&line, section, cable, &given, &budget);
                match (one, uniform, own) {
                    (Ok(a), Ok(b), Ok(c)) => {
                        assert_eq!(a, b, "{} {}", section.id, case.name);
                        assert_eq!(a, c, "{} {}", section.id, case.name);
                        solved += 1;
                    }
                    (Err(a), Err(b), Err(c)) => {
                        assert_eq!(a.to_string(), b.to_string());
                        assert_eq!(a.to_string(), c.to_string());
                    }
                    (a, b, c) => panic!("{a:?}\n{b:?}\n{c:?}"),
                }
            }
        }
        assert!(solved >= 3 * line.sections.len() - 1, "{solved}");
    }

    /// One span iced (10 mm, −5 °C) in a section otherwise bare at −5 °C:
    /// that span sags more and is the only one with the iced radius; its
    /// tension rises, the strings at its ends swing into it (its clamps
    /// move towards its middle), and the neighbours are pulled tighter
    /// too. With wind, the iced span is blown further than the same span
    /// bare in the same wind (the wind on its iced diameter).
    #[test]
    fn one_iced_span_sags_and_pulls_its_strings_in() {
        let mut line = crate::tests::example();
        line.sections[0].clear_point_loads();
        let lib = library();
        let section = &line.sections[0];
        let cable = lib.get(&section.cable).unwrap();
        let budget = default_fe_budget();
        let bare = iced(-5.0, 0.0, 0.0);
        let k = 1;
        let base = solve_section(&line, section, cable, &bare, &budget).unwrap();
        let cases = SpanCases::uniform(bare.clone()).with_span(k, iced(-5.0, 10.0, 0.0));
        assert!(!cases.is_uniform());
        let one = solve_section_span_cases(&line, section, cable, &cases, &budget).unwrap();
        assert!(one.spans[k].sag > base.spans[k].sag + 0.1);
        let r_bare = cable.diameter.si() / 2.0;
        for (s, span) in one.spans.iter().enumerate() {
            let r = if s == k { r_bare + 0.01 } else { r_bare };
            assert!((span.surface_radius - r).abs() < 1e-12, "{s}");
            assert!(
                span.horizontal_tension > base.spans[s].horizontal_tension,
                "{s}"
            );
        }
        // The clamps at the iced span's ends move towards its middle.
        let at = |s: &SectionShape, i: usize| v(s.clamps[i]);
        let towards = |i: usize, j: usize| {
            let d = (at(&one, j) - at(&one, i)).xy().normalize();
            (at(&one, i) - at(&base, i)).xy().dot(&d)
        };
        assert!(towards(k, k + 1) > 0.01, "{}", towards(k, k + 1));
        assert!(towards(k + 1, k) > 0.01, "{}", towards(k + 1, k));
        // Wind on the iced diameter: blown further than bare in the same
        // wind.
        let windy = iced(-5.0, 0.0, 400.0);
        let blown_bare = solve_section(&line, section, cable, &windy, &budget).unwrap();
        let cases = SpanCases::uniform(windy.clone()).with_span(k, iced(-5.0, 10.0, 400.0));
        let blown = solve_section_span_cases(&line, section, cable, &cases, &budget).unwrap();
        let out = |s: &SectionShape| {
            let chord = (v(s.clamps[k]) + v(s.clamps[k + 1])) / 2.0;
            (v(s.spans[k].mid_point) - chord).xy().norm()
        };
        assert!(
            out(&blown) > out(&blown_bare),
            "{} {}",
            out(&blown),
            out(&blown_bare)
        );
    }

    /// The ruling span has one tension for all spans: spans in different
    /// cases are refused; in one case they solve.
    #[test]
    fn the_ruling_span_takes_one_case_only() {
        let mut line = crate::tests::example();
        line.sections[0].clear_point_loads();
        line.sections[0].calculation = Calculation::INITIAL_RS;
        let lib = library();
        let cable = lib.get(&line.sections[0].cable).unwrap();
        let bare = iced(-5.0, 0.0, 0.0);
        let budget = default_fe_budget();
        let all = SpanCases::uniform(bare.clone());
        let section = &line.sections[0];
        assert!(solve_section_span_cases(&line, section, cable, &all, &budget).is_ok());
        let one = all.with_span(1, iced(-5.0, 10.0, 0.0));
        let e = solve_section_span_cases(&line, section, cable, &one, &budget).unwrap_err();
        assert!(e.to_string().contains("finite-element"), "{e}");
    }
}
