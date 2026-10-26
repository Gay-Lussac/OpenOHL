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
//! plumb (the section's unstressed length). A section in the blocks then
//! **rolls**: in each weather case, the stringing state included, its
//! unstressed length is shared out over frictionless sheaves at the
//! suspension clamps, the wire's tension equal on both sides of each (the
//! default, owner decision 2026-10-25), or — simplified, the line's
//! calculation options — so that every span has the same horizontal
//! tension.

use nalgebra::Vector3;
use ohl_cables::Cable;
use ohl_core::budget::Budget;
use ohl_weather::WeatherCase;
use ohl_wirefe::catenary_element::default_element_budget;
use ohl_wirefe::{CableElement, CableProperties, Model, Node, TrussElement, default_fe_budget};

use crate::{Line, LineError, Point, Section, Stringing};

mod coupled;
mod memo;
mod ruling_span;
pub use coupled::*;
pub(crate) use memo::live_memo;
pub use memo::{forget_solves, solve_line_memo, solve_line_memo_default};
pub use ruling_span::*;

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
    /// The span solved with its wire gone ([`solve_section_broken`]; none:
    /// the section whole). That span's shape has no points and no
    /// tension.
    pub broken: Option<usize>,
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
    /// Its limits (its own, else its insulator's; absent: not given),
    /// compared with `force` and `swing` by the string check
    /// (`ohl_clearance::strings`); the solve does not use them.
    pub limits: crate::StringLimits,
    /// Its swing as solved ([`crate::swing`], in its structure's axes).
    pub swing: crate::Swing,
    /// A dead end as solved: flagged so
    /// ([`crate::InsulatorString::strain`]), a lone string the solve
    /// treats as one, or a string holding the clamp at a section's end;
    /// else a suspension string (or one of a V, below a V, a TAK's
    /// suspension string).
    pub strain: bool,
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
    /// For each section point, whether it lost its wire (a section end
    /// next to a broken span): its holder is left out of the model, its
    /// strings hang under their own weight ([`hanging_loads`]).
    dead: Vec<bool>,
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
/// `along` (a plan unit vector): weight and ice down, and the case's full
/// wind pressure square to the wire in plan, as on the spans
/// ([`span_load`]), pushed to the side of the line the case's wind blows
/// to (physics review 2026-10-26, owner-approved: before, the wire took
/// only the part of the across-line wind normal to it, by cos² of the
/// angle between the wind and its normal, so a jumper across the line
/// took almost none — unsafe for its clearances, since a wind along the
/// line blows it out fully). A wire lying exactly across the line is
/// pushed ahead. Without a direction, as [`span_load`]. For wires whose
/// own direction does not tell the line's (jumpers).
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
    let ahead = nalgebra::Vector2::new(a[0], a[1]);
    let h = d.xy();
    // The wire's normal in plan, on the side of the line's left.
    let normal = if h.norm() > 0.0 {
        let u = h / h.norm();
        let n = nalgebra::Vector2::new(-u.y, u.x);
        let side = n.dot(&left);
        if side > 0.0 {
            n
        } else if side < 0.0 {
            -n
        } else if n.dot(&ahead) >= 0.0 {
            // Across the line exactly: no side; pushed ahead.
            n
        } else {
            -n
        }
    } else {
        left
    };
    let push = normal * (load.horizontal.si() * case.wind_from.to_left());
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
/// [`SpanCases`], or one case for all), with the section's local winds
/// ([`LocalWinds`]) and a broken span.
#[derive(Clone, Copy)]
struct Cases<'a> {
    base: &'a WeatherCase,
    own: Option<&'a std::collections::BTreeMap<usize, WeatherCase>>,
    /// The factor on the wind on the strings at each point of the
    /// section (none: 1 everywhere).
    points: Option<&'a [f64]>,
    /// The spans' cases a section in the blocks rolls in (none: the
    /// section's case): the local winds only, not the spans' own cases.
    rolling: Option<&'a std::collections::BTreeMap<usize, WeatherCase>>,
    /// A span whose wire is gone (a broken wire, roadmap O9): no element
    /// there, its two clamps held by their strings alone.
    broken: Option<usize>,
}

impl<'a> Cases<'a> {
    /// One case for every span.
    fn one(case: &'a WeatherCase) -> Self {
        Self {
            base: case,
            own: None,
            points: None,
            rolling: None,
            broken: None,
        }
    }

    fn of_spans(cases: &'a SpanCases) -> Self {
        Self {
            own: Some(&cases.spans),
            ..Self::one(&cases.base)
        }
    }

    /// The case of span `s`.
    fn of(&self, s: usize) -> &'a WeatherCase {
        self.own.and_then(|m| m.get(&s)).unwrap_or(self.base)
    }

    /// The factor on the wind on the strings at point `k`.
    fn at_point(&self, k: usize) -> f64 {
        self.points.and_then(|p| p.get(k)).copied().unwrap_or(1.0)
    }

    /// The cases the section rolls in: the section's case in every span,
    /// each times its local wind.
    fn for_rolling(&self) -> Self {
        Self {
            own: self.rolling,
            rolling: self.rolling,
            broken: None,
            ..*self
        }
    }

    /// Whether span `s` is the broken one.
    fn is_broken(&self, s: usize) -> bool {
        self.broken == Some(s)
    }
}

/// A section's local winds in a weather case ([`Section::wind_factors`],
/// roadmap O15): each span's case with its wind times the span's factor
/// (over its own case where it has one), the case of every span a
/// section in the blocks rolls in, and the factor on the strings' wind at
/// each point — the larger of its two spans' (conservative: the string
/// swings at least as far as with either).
struct LocalWinds {
    spans: std::collections::BTreeMap<usize, WeatherCase>,
    rolling: std::collections::BTreeMap<usize, WeatherCase>,
    points: Vec<f64>,
}

impl LocalWinds {
    /// None when no span has a factor other than 1 (the section solved
    /// as before, bit for bit).
    fn of(section: &Section, cases: Cases<'_>) -> Option<Self> {
        if !section.has_local_wind() {
            return None;
        }
        let n = section.points.len().saturating_sub(1);
        Some(Self {
            spans: (0..n)
                .map(|s| (s, cases.of(s).with_wind_factor(section.wind_factor(s))))
                .collect(),
            rolling: (0..n)
                .map(|s| (s, cases.base.with_wind_factor(section.wind_factor(s))))
                .collect(),
            points: (0..section.points.len())
                .map(|k| section.wind_factor_at_point(k))
                .collect(),
        })
    }

    /// `cases` with the local winds.
    fn apply<'a>(&'a self, cases: Cases<'a>) -> Cases<'a> {
        Cases {
            own: Some(&self.spans),
            points: Some(&self.points),
            rolling: Some(&self.rolling),
            ..cases
        }
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
    /// How a section in the blocks rolls (the line's
    /// [`crate::SolverOptions::blocks`]; set where a section is solved).
    blocks: crate::InTheBlocks,
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
    /// A dead end (a tension string).
    strain: bool,
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
            let mut members: Vec<Member> = members
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
                    strain: st.strain,
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
            // Dead ends as solved: flagged, a lone string treated as one,
            // or a string holding the clamp at a section's end (a TAK's
            // suspension string, its bottom the node, is not).
            for m in &mut members {
                m.strain |= shape == Shape::Strain || (!middle && m.bottom == clamp);
            }
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
    // A section end next to a broken span holds no wire.
    let dead: Vec<bool> = (0..n)
        .map(|k| {
            let back = k.checked_sub(1).is_none_or(|s| cases.is_broken(s));
            let ahead = k + 1 >= n || cases.is_broken(k);
            cases.broken.is_some() && back && ahead
        })
        .collect();
    for (k, holder) in layout.iter().enumerate() {
        if dead[k] {
            // Out of the model: a node where the clamp was, for the
            // indices (nothing joins it).
            model.nodes.push(Node::fixed(reference.clamps[k]));
            clamps.push(model.nodes.len() - 1);
            supports.push(Vec::new());
            frees.push(Vec::new());
            free_labels.push(Vec::new());
            members.push(Vec::new());
            continue;
        }
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
        if dead[k] {
            continue;
        }
        let along = reference.clamps[(k + 1).min(n - 1)] - reference.clamps[k.saturating_sub(1)];
        let horizontal = along.xy();
        if horizontal.norm() == 0.0 {
            continue;
        }
        let transverse = Vector3::new(-horizontal.y, horizontal.x, 0.0) / horizontal.norm();
        // The local wind at this point (1 without).
        let local = cases.at_point(k);
        for m in holder.members.iter().filter(|m| m.wind_area > 0.0) {
            let push = transverse
                * (0.5 * case.wind_pressure.si() * local * m.wind_area * case.wind_from.to_left());
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
        if cases.is_broken(s) {
            // The wire is gone: no element, the objects on it fall with
            // it; its clamps are held by their strings alone.
            span_elements.push(Vec::new());
            continue;
        }
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
        dead,
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
    // The stringing state as given (every span at the stringing tension,
    // as before the sheave model: the data keep their meaning).
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
                blocks: crate::InTheBlocks::default(),
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
            blocks: crate::InTheBlocks::default(),
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
/// The loads on the structure of a holder that lost its wire (a section
/// end next to a broken span, [`solve_section_broken`]): its strings
/// hang under their own weight, shared equally by its structure points,
/// with the wind on them across the line (as on the strings of the
/// section, full on the tops). Simplification: the share is exact for
/// one string (it hangs plumb), and for strings alike side by side; the
/// broken wire's piece left hanging from the clamp is not counted (it
/// lies on the ground).
fn hanging_loads(
    holder: &Holder,
    reference: &Reference,
    k: usize,
    cases: Cases<'_>,
    bundle: f64,
) -> Vec<Support> {
    let tops: Vec<&(String, Vector3<f64>, Option<u32>)> = holder
        .fixed
        .iter()
        .filter(|(_, _, o)| o.is_none())
        .collect();
    if tops.is_empty() {
        return Vec::new();
    }
    let case = cases.base;
    let n = reference.clamps.len();
    let along = reference.clamps[(k + 1).min(n - 1)] - reference.clamps[k.saturating_sub(1)];
    let h = along.xy();
    let left = if h.norm() > 0.0 {
        Vector3::new(-h.y, h.x, 0.0) / h.norm()
    } else {
        Vector3::zeros()
    };
    let weight: f64 = holder.members.iter().map(|m| m.weight).sum();
    let area: f64 = holder.members.iter().map(|m| m.wind_area).sum();
    let wind =
        left * (case.wind_pressure.si() * cases.at_point(k) * area * case.wind_from.to_left());
    let share = 1.0 / tops.len() as f64;
    tops.iter()
        .map(|(label, at, _)| Support {
            label: label.clone(),
            at: p(*at),
            // On the structure: the weight down, the wind's push.
            force: p((Vector3::new(0.0, 0.0, -weight) + wind) * share * bundle),
        })
        .collect()
}

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
    hung(built, solution, k, along, 0.0)
}

/// Where a holder's clamp hangs under the wires' pull in a solution, its
/// part along the line (unit `along`) replaced by `along_pull` (N per
/// subconductor): the assembly alone, from its solved shape, under its own
/// loads and that pull. None if it does not solve.
fn hung(
    built: &Built,
    solution: &ohl_wirefe::Solution,
    k: usize,
    along: Vector3<f64>,
    along_pull: f64,
) -> Option<Vector3<f64>> {
    let c = built.clamps[k];
    let trusses: Vec<usize> = built.members[k].iter().map(|(_, t)| *t).collect();
    let pull = clamp_pull(built, solution, k);
    let pull = pull - along * (pull.dot(&along) - along_pull);
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
/// sum of the reference lengths) shared out between the spans — the wire
/// rolls over the suspension clamps (point loads are left out of the
/// sharing):
///
/// - over frictionless sheaves (the default, [`crate::InTheBlocks`]): the
///   wire's tension equal on both sides of each ([`over_sheaves`]), the
///   strings swung to the sheaves' resultant, along the line too;
/// - simplified: all spans the same horizontal tension, the strings plumb
///   along the line. Bisection on the tension; exit clause: 200 steps.
fn rolled(
    section: &Section,
    layout: &[Holder],
    reference: &Reference,
    cable: &Cable,
    cases: Cases<'_>,
    budget: &Budget,
) -> Result<Reference, LineError> {
    // The section's case in every span (times the spans' local winds).
    let cases = cases.for_rolling();
    // The clamps settle where the shared-out lengths put them (strings
    // swing across the line more under ice or wind at line angles, which
    // changes the chords; over sheaves along it too): fixed point, exit
    // clause 20 passes.
    // First guess 20 kN: the bracket widens from there; later passes start
    // from the tension found.
    let (mut r, mut h, mut pulls) = shared_out(
        section,
        reference,
        &reference.clamps,
        cable,
        cases,
        20_000.0,
        budget,
    )?;
    for _ in 0..20 {
        let (built, solved) = solve_built(section, layout, &r, cable, cases, budget, None)?;
        let clamps = match &pulls {
            None => settled(layout, &r.clamps, &built, &solved),
            Some(p) => settled_over_sheaves(layout, &r.clamps, &built, &solved, p),
        };
        let moved = clamps
            .iter()
            .zip(&r.clamps)
            .map(|(a, b)| (a - b).norm())
            .fold(0.0, worst);
        if moved < 1e-6 {
            return Ok(r);
        }
        (r, h, pulls) = shared_out(section, reference, &clamps, cable, cases, h, budget)?;
    }
    Err(LineError::Invalid(format!(
        "section {}: rolling did not settle in 20 passes",
        section.id
    )))
}

/// [`settled`] for a section rolled over sheaves: every clamp in the
/// middle of the section hangs where its assembly balances the wires'
/// pull — across the line as solved, along it the sheaves' (`pulls`, per
/// section point, [`over_sheaves`]): the equal tension on both sides of
/// a sheave leaves a pull along the line on inclined or unequal spans,
/// which swings the string towards the span with the larger horizontal
/// tension until it balances.
fn settled_over_sheaves(
    layout: &[Holder],
    clamps: &[Vector3<f64>],
    built: &Built,
    solution: &ohl_wirefe::Solution,
    pulls: &[Vector3<f64>],
) -> Vec<Vector3<f64>> {
    let n = clamps.len();
    (0..n)
        .map(|k| {
            let position = solution.positions[built.clamps[k]];
            let middle = k > 0 && k + 1 < n;
            match layout[k].shape {
                Shape::Suspension | Shape::VString | Shape::General if middle => {
                    let unit = |a: usize, b: usize| {
                        let d = (clamps[b] - clamps[a]).xy();
                        if d.norm() > 0.0 { d.normalize() } else { d }
                    };
                    let along = unit(k - 1, k) + unit(k, k + 1);
                    let along = Vector3::new(along.x, along.y, 0.0).normalize();
                    hung(built, solution, k, along, pulls[k].dot(&along)).unwrap_or(position)
                }
                _ => position,
            }
        })
        .collect()
}

/// The wires' pulls on the clamps over sheaves, per section point
/// (none: the equal-H model).
type SheavePulls = Option<Vec<Vector3<f64>>>;

/// The reference's unstressed length shared out between the spans between
/// `clamps` so that all have the same horizontal tension in `case` (found
/// from `guess`, N); with that tension. Over sheaves (the reference's
/// [`crate::InTheBlocks`]) that sharing is the start of [`over_sheaves`],
/// whose first span's tension and pulls on the clamps come with it.
fn shared_out(
    section: &Section,
    reference: &Reference,
    clamps: &[Vector3<f64>],
    cable: &Cable,
    cases: Cases<'_>,
    guess: f64,
    budget: &Budget,
) -> Result<(Reference, f64, SheavePulls), LineError> {
    // One temperature for the section (its case's); each span its load.
    let case = cases.base;
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
                    cases.of(s),
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
            if reference.blocks == crate::InTheBlocks::Sheaves {
                let stretches = stretches.borrow().clone();
                let (r, h, pulls) = over_sheaves(
                    section, reference, clamps, cable, cases, h, stretches, budget,
                )?;
                return Ok((r, h, Some(pulls)));
            }
            return Ok((
                Reference {
                    clamps: clamps.to_vec(),
                    lengths: found,
                    temperature: reference.temperature,
                    weight_scale: reference.weight_scale.clone(),
                    loads_per: reference.loads_per,
                    blocks: reference.blocks,
                },
                h,
                None,
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

/// The reference's unstressed length shared out between the spans between
/// `clamps` over frictionless sheaves in `case` ([`ohl_sagtension::sheaves`],
/// owner decision 2026-10-25): the wire's tension (along it, at the
/// clamp: |force| of the span's catenary there, with its load — own
/// weight, ice, wind) equal on both sides of every suspension clamp. One
/// unknown, the first span's horizontal tension (from `guess`, the
/// equal-H tension), each next span's from the equal tension, the
/// section's length the residual; the spans' stretch under ice and wind
/// (`stretches`, from the equal-H sharing) a fixed point around it (exit
/// clause [`ohl_wirefe::model::STRETCH_PASSES`]). With the first span's
/// tension and the wires' pull on each clamp (per section point).
///
/// Simplification: the wire runs over the sheave at the clamp point (the
/// sheave's size left out) and the sheaves have no friction.
#[allow(clippy::too_many_arguments)]
fn over_sheaves(
    section: &Section,
    reference: &Reference,
    clamps: &[Vector3<f64>],
    cable: &Cable,
    cases: Cases<'_>,
    guess: f64,
    mut stretches: Vec<f64>,
    budget: &Budget,
) -> Result<(Reference, f64, Vec<Vector3<f64>>), LineError> {
    use ohl_sagtension::sheaves::{ShareError, SheaveSpan, share};
    let case = cases.base;
    let invalid = |m: String| {
        LineError::Invalid(format!(
            "section {}: rolling over the sheaves: {m}",
            section.id
        ))
    };
    let ea = cable.axial_stiffness().si();
    let thermal =
        (cable.thermal_expansion() * (case.temperature.celsius() - reference.temperature)).exp();
    let total: f64 = reference.lengths.iter().sum();
    let chords: Vec<Vector3<f64>> = clamps.windows(2).map(|c| c[1] - c[0]).collect();
    let n_spans = |c: &[Vector3<f64>]| c.len().saturating_sub(1);
    let element_budget = ohl_wirefe::catenary_element::default_element_budget();
    // Each span's last length (a start for the next: the tensions tried
    // lie close together).
    let last: std::cell::RefCell<Vec<Option<f64>>> =
        std::cell::RefCell::new(vec![None; n_spans(clamps)]);
    // Span `s` at horizontal tension `h` with stretch `k`: its element and
    // state, and whether it carries loads per stretched metre.
    let span = |s: usize, h: f64, k: f64| {
        let at = |e: ohl_wirefe::ElementError| invalid(format!("span {}: {e}", s + 1));
        let d = chords[s];
        let (own, stretched) = span_loads_split(
            cases.of(s),
            cable,
            d,
            reference.weight_scale[s] / thermal,
            reference.loads_per,
        );
        let load = own + stretched * k;
        let element = |l: f64| CableProperties {
            unstressed_length: l,
            axial_stiffness: ea,
            load,
        };
        // A secant on the length from the last one (2–4 element solves),
        // else the bracketed search (exact either way: to 1e-12 of H).
        let near = last.borrow()[s].and_then(|start| {
            let mut a = (start, element(start).solve(d, None, &element_budget).ok()?);
            if (a.1.horizontal - h).abs() <= 1e-12 * h {
                return Some(a);
            }
            let b0 = start * (1.0 + 1e-7);
            let mut b = (b0, element(b0).solve(d, None, &element_budget).ok()?);
            for _ in 0..20 {
                let (fa, fb) = (a.1.horizontal - h, b.1.horizontal - h);
                if (fb).abs() <= 1e-12 * h {
                    return Some(b);
                }
                if fb == fa {
                    return None;
                }
                let next = b.0 - fb * (b.0 - a.0) / (fb - fa);
                if !(next > 0.0 && (next - start).abs() < 1e-2 * start) {
                    return None;
                }
                a = b;
                b = (next, element(next).solve(d, None, &element_budget).ok()?);
            }
            None
        });
        let (l, state) = match near {
            Some(found) => found,
            None => {
                let l = CableProperties::unstressed_length_for_tension(d, h, ea, load, budget)
                    .map_err(at)?;
                (l, element(l).solve(d, None, &element_budget).map_err(at)?)
            }
        };
        last.borrow_mut()[s] = Some(l);
        Ok::<_, LineError>((element(l), state, stretched != Vector3::zeros()))
    };
    let n = chords.len();
    let mut h1 = guess;
    for _ in 0..ohl_wirefe::model::STRETCH_PASSES {
        let hs = share(
            n,
            total,
            h1,
            |s, h| {
                let (props, state, _) = span(s, h, stretches[s])?;
                Ok(Some(SheaveSpan {
                    unstressed: props.unstressed_length / thermal,
                    tension_start: state.tension_i,
                    tension_end: state.tension_j,
                }))
            },
            budget,
        )
        .map_err(|e| match e {
            ShareError::Span(e) => e,
            other => invalid(other.to_string()),
        })?;
        let mut lengths = Vec::with_capacity(n);
        let mut pulls = vec![Vector3::zeros(); n + 1];
        let mut moved: f64 = 0.0;
        for (s, &h) in hs.iter().enumerate() {
            let (props, state, per_stretched) = span(s, h, stretches[s])?;
            lengths.push(props.unstressed_length / thermal);
            pulls[s] += state.force_i;
            pulls[s + 1] += state.force_j;
            if per_stretched {
                let next = props.stretched_length(&state) / props.unstressed_length;
                moved = if next.is_nan() {
                    f64::INFINITY
                } else {
                    moved.max((next - stretches[s]).abs())
                };
                stretches[s] = next;
            }
        }
        if moved <= ohl_wirefe::model::STRETCH_TOLERANCE {
            return Ok((
                Reference {
                    clamps: clamps.to_vec(),
                    lengths,
                    temperature: reference.temperature,
                    weight_scale: reference.weight_scale.clone(),
                    loads_per: reference.loads_per,
                    blocks: reference.blocks,
                },
                hs[0],
                pulls,
            ));
        }
        h1 = hs[0];
    }
    Err(invalid(format!(
        "the spans' stretch under ice and wind did not settle in {} passes",
        ohl_wirefe::model::STRETCH_PASSES
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
    let mut r = reference(section, &layout, cable, &budget)?;
    r.loads_per = line.solver.loads_per;
    r.blocks = line.solver.blocks;
    let r = in_the_blocks_at_stringing(section, &layout, &r, cable, &budget)?;
    Ok((r.lengths, r.temperature))
}

/// A section in the blocks at its stringing condition (still air at the
/// reference's temperature): over sheaves, the reference's length shared
/// out by them ([`rolled`]: every span at the given tension with the
/// strings plumb gives the section's length, the sheaves then pass the
/// tension along the wire, so the spans' horizontal tensions differ a
/// little from the given one on unequal or inclined spans); simplified, or
/// clipped in, or of one span: the reference as it is.
fn in_the_blocks_at_stringing(
    section: &Section,
    layout: &[Holder],
    reference: &Reference,
    cable: &Cable,
    budget: &Budget,
) -> Result<Reference, LineError> {
    if reference.blocks == crate::InTheBlocks::EqualH
        || section.stringing.clipped()
        || reference.lengths.len() < 2
    {
        return Ok(reference.clone());
    }
    let still = WeatherCase::still(
        "stringing",
        ohl_core::units::Temperature::from_celsius(reference.temperature),
    );
    rolled(
        section,
        layout,
        reference,
        cable,
        Cases::one(&still),
        budget,
    )
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

/// Solves one section of a line in a weather case with the wire of span
/// `span` (0 = first) gone: a **broken wire** (roadmap O9, owner
/// 2026-10-19 and 2026-10-25), for the security loads of the design
/// codes.
///
/// The model: the section as it is in the case — its unstressed lengths
/// those of the whole section in the case's condition (a section in the
/// blocks rolled in the case before the break, then held in its clamps)
/// — with no element in the broken span. Its two clamps are then each a
/// free end held by its strings alone: a suspension string at the back
/// end swings towards the spans left behind it (the one at the ahead
/// end ahead) until the string, its weight and the remaining span's pull
/// are in equilibrium; the remaining spans of each piece re-solve with
/// the new clamp positions (each span its own tension), the other wires
/// as they are. A section end next to the broken span (its first or last
/// point) holds no wire any more: its strings hang under their own
/// weight ([`SectionShape::attachment_loads`] there: that weight and the
/// wind on them). A bundle's broken wire is the whole phase (every
/// subconductor of the bundle in that span; the solve follows one
/// subconductor for the bundle), the conservative reading.
///
/// Left out (Simplification): the dynamic impact of the break (the
/// static state after it is solved); the broken pieces lying on the
/// ground; the objects on the broken span (they fall with it). Refused:
/// a span the section does not have, the ruling-span method (it has one
/// tension for every span), a spacer or a jumper hanging on a span point
/// of the broken span. The jumpers on the clamps take their loads of the
/// whole line (the jumpers at the broken span's ends as if it held).
///
/// Expected cost: as [`solve_section`] (the swing of the strings may take
/// a few more Newton iterations).
pub fn solve_section_broken(
    line: &Line,
    section: &Section,
    cable: &Cable,
    case: &WeatherCase,
    span: usize,
    budget: &Budget,
    coupling: &Coupling,
) -> Result<SectionShape, LineError> {
    let invalid = |m: String| LineError::Invalid(format!("section {}: {m}", section.id));
    let spans = section.points.len().saturating_sub(1);
    if span >= spans {
        return Err(invalid(format!(
            "no span {} to break (it has {spans})",
            span + 1
        )));
    }
    if section.calculation.method() == crate::condition::Method::Rs {
        return Err(invalid(
            "a broken wire needs the finite-element method (the ruling span has one tension \
             for all spans)"
                .into(),
        ));
    }
    let on_broken: Vec<&str> = section
        .span_points
        .iter()
        .filter(|p| p.span == span)
        .map(|p| p.label.as_str())
        .collect();
    let here =
        |r: &crate::SpanPointRef| r.section == section.id && on_broken.contains(&r.point.as_str());
    let held = on_broken.iter().any(|label| {
        coupling.spacers.holds.iter().any(|h| h.point == *label)
            || coupling.spacers.pulls.contains_key(*label)
    }) || line.spacers.iter().any(|x| here(&x.a) || here(&x.b))
        || line.jumpers.iter().any(|j| {
            j.taps()
                .any(|t| matches!(t, crate::Tap::Span(r) if here(r)))
        });
    if held {
        return Err(invalid(format!(
            "span {}: a spacer or a jumper hangs on its span points ({}): a broken wire there \
             is not modelled",
            span + 1,
            on_broken.join(", ")
        )));
    }
    // The wind's side is the line's (as [`solve_section_coupled`]).
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
    let cases = Cases {
        broken: Some(span),
        ..Cases::one(case)
    };
    solve_cases(line, section, cable, cases, budget, coupling)
}

/// Solves every section of `line` in `case` ([`solve_line_with`]), the
/// wire of span `span` (0 = first) of section `section` broken
/// ([`solve_section_broken`]): the other sections as they are, the broken
/// one on its own (a section sharing strings with others, a TAK, is
/// broken with the others held as solved whole — Simplification). One
/// result per section, in order. Err: no section `section`.
///
/// Expected cost: one line solve and one more section solve.
pub fn solve_line_broken(
    line: &Line,
    library: &ohl_cables::Library,
    case: &WeatherCase,
    section: &str,
    span: usize,
    budget: &Budget,
) -> Result<Vec<Result<SectionShape, LineError>>, LineError> {
    let k = line
        .sections
        .iter()
        .position(|s| s.id == section)
        .ok_or_else(|| LineError::Invalid(format!("no section {section} to break a wire of")))?;
    let mut results = solve_line_with(line, library, case, budget);
    let s = &line.sections[k];
    let jumpers = crate::jumper::end_loads(line, case, |id| library.get(id));
    let coupling = Coupling {
        jumpers: Some(jumpers),
        ..Coupling::default()
    };
    results[k] = match library.get(&s.cable) {
        Some(cable) => solve_section_broken(line, s, cable, case, span, budget, &coupling),
        None => Err(LineError::Invalid(format!(
            "section {}: wire type {} is not in the wire library",
            s.id, s.cable
        ))),
    };
    Ok(results)
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
    // Its local winds (roadmap O15): each span's case with its factor.
    let local = LocalWinds::of(section, cases);
    let cases = match &local {
        Some(l) => l.apply(cases),
        None => cases,
    };
    // The ruling span has one tension for all spans: the case's wind
    // times the largest local factor of the section (on the safe side for
    // the tension; Simplification for the spans with less).
    let rs_case = cases.base.with_wind_factor(section.largest_wind_factor());
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
        Method::Rs => solve_rs(line, section, cable, &rs_case, final_state, budget),
    };
    match calc.condition() {
        Condition::Initial => solve(None),
        Condition::Creep => solve(Some(&memo::final_state(
            line,
            section,
            cable,
            Condition::Creep,
            budget,
        )?)),
        Condition::Load => solve(Some(&memo::final_state(
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
            .map(|(c, _)| solve(Some(&memo::final_state(line, section, cable, c, budget)?)))
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
    // The same in every weather case: from the memo once worked out.
    let prepared = memo::prepared(line, section, cable, budget)?;
    let (layout, estimated_weights) = (prepared.layout.clone(), prepared.weights.clone());
    let mut reference = prepared.reference.clone();
    // The ice and wind per stretched or unstretched metre (the line's
    // calculation options, 2026-10-21).
    reference.loads_per = line.solver.loads_per;
    // How it rolls in the blocks (2026-10-25).
    reference.blocks = line.solver.blocks;
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
    // (The same in every weather case: from the memo once worked out.)
    let strung = match &curves {
        Some(_) => Some(memo::stringing(
            &prepared,
            &(reference.loads_per, reference.blocks),
            || {
                let stringing = WeatherCase::still(
                    "stringing",
                    ohl_core::units::Temperature::from_celsius(reference.temperature),
                );
                // In the blocks over sheaves the spans' lengths at
                // stringing are the sheaves' sharing of the reference's.
                let at_stringing =
                    in_the_blocks_at_stringing(section, &layout, &reference, cable, budget)?;
                let (b, sol) = solve_built(
                    section,
                    &layout,
                    &at_stringing,
                    cable,
                    Cases::one(&stringing),
                    budget,
                    None,
                )?;
                Ok(span_tensions(&b, &sol))
            },
        )?),
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
        reference = rolled(section, &loaded, &reference, cable, cases, budget)?;
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
        if elements.is_empty() {
            // The broken span: no wire (no points, no tension).
            spans.push(SpanShape {
                points: Vec::new(),
                horizontal_tension: 0.0,
                max_tension: 0.0,
                mid_point: p((a + b) / 2.0),
                low_point: p(if a.z < b.z { a } else { b }),
                sag: 0.0,
                surface_radius: 0.0,
                polyline_error: 0.0,
            });
            continue;
        }
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
                        strain: m.strain,
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
    if let Some(k) = cases.broken {
        flags.push(format!("span {}: the wire broken (roadmap O9)", k + 1));
    }
    Ok(SectionShape {
        broken: cases.broken,
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
            .zip(&built.dead)
            .enumerate()
            .map(|(k, ((nodes, holder), dead))| {
                if !holder.reports {
                    return Vec::new();
                }
                if *dead {
                    return hanging_loads(holder, &reference, k, cases, bundle);
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
/// direction (its first to last wire point against the line's direction
/// at their middle, the routes made once): the sections
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

#[cfg(test)]
mod sheave_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod span_tests;

#[cfg(test)]
mod spacer_tests;

#[cfg(test)]
mod span_case_tests;

#[cfg(test)]
mod local_tests;
