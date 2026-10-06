// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Line data model: structures, attachment points, wire sections.
//!
//! A [`Line`] is stored as JSON (`*.ols.json`, the open exchange format of
//! ADR-0004). Coordinates are numbers in metres in the line's coordinate
//! reference system (`crs`, e.g. `"EPSG:2056"`); x east, y north, z up.
//!
//! - [`Structure`]: a tower or pole with attachment points. Each attachment
//!   (set, phase) has an **insulator point** (on the structure) and a **wire
//!   point** (where the conductor is clamped). A wire point straight below
//!   its insulator point is a suspension string; otherwise the attachment is
//!   a strain (dead-end) point. An attachment with two `legs` is a V-string.
//! - [`Section`]: one wire (or bundle) strung through attachment points, with
//!   its cable and stringing. Spans are between consecutive points.
//!
//! - [`Jumper`]: a slack cable joining two attachment points — the ends of
//!   two sections at a dead-end, or a tap from a branch to a through wire —
//!   optionally held by a support string; solved by [`jumper`].
//!
//! [`solve`] turns a section into a finite-element model (exact catenary
//! elements, insulator strings) and solves it in any weather state.

pub mod bundles;
pub mod condition;
pub mod electrical;
pub mod export;
pub mod incremental;
pub mod jumper;
pub mod library;
mod rod;
pub mod route;
pub mod solve;

pub use route::{AlignmentPoint, Route, Station};

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// File format identifier.
pub const LINE_FILE_FORMAT: &str = "ols-line";
/// Latest line file version.
pub const LINE_FILE_VERSION: u32 = 1;

fn default_format() -> String {
    LINE_FILE_FORMAT.to_owned()
}

fn default_version() -> u32 {
    LINE_FILE_VERSION
}

/// A point in the line's coordinate system, metres.
pub type Point = [f64; 3];

/// A transmission or distribution line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Line {
    /// `"ols-line"`.
    #[serde(default = "default_format")]
    pub format: String,
    /// Format version.
    #[serde(default = "default_version")]
    pub version: u32,
    /// Line name.
    pub name: String,
    /// Coordinate reference system, e.g. `"EPSG:2056"` (informative).
    #[serde(default)]
    pub crs: String,
    /// Alignment points (PIs) of the implicit single route, used when
    /// `routes` is empty (files written before routes existed).
    #[serde(default)]
    pub alignment: Vec<AlignmentPoint>,
    /// Routes (alignments with their structures); see [`route`].
    #[serde(default)]
    pub routes: Vec<Route>,
    /// Structures.
    pub structures: Vec<Structure>,
    /// Wire sections.
    pub sections: Vec<Section>,
    /// Jumpers between attachment points.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub jumpers: Vec<Jumper>,
    /// Electrical systems (phases or poles with their voltage); sections
    /// refer to them ([`electrical`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub systems: Vec<electrical::System>,
    /// Display settings (phase colours).
    #[serde(default, skip_serializing_if = "Display::is_default")]
    pub display: Display,
    /// Weather cases for the permanent stretch due to creep and load
    /// (Criteria › Creep-Stretch), by name.
    #[serde(default, skip_serializing_if = "condition::CreepStretch::is_empty")]
    pub creep_stretch: condition::CreepStretch,
    /// What the outer strands of its bimetallic cables do above the
    /// transition temperature, for the cables whose wire library entry
    /// does not say (Criteria › Bimetallic conductor model).
    #[serde(
        default,
        skip_serializing_if = "ols_cables::BimetallicModel::is_default"
    )]
    pub bimetallic: ols_cables::BimetallicModel,
    /// Those weather cases themselves, given by the program solving the
    /// line ([`crate::Line::set_weather_cases`]); not stored.
    #[serde(skip)]
    pub condition_cases: condition::ConditionCases,
}

fn default_jumper_temperature() -> f64 {
    15.0
}

fn half() -> f64 {
    0.5
}

/// A jumper: a slack cable from the wire point of one attachment to that of
/// another (solved clamp positions when the points belong to sections).
/// Its cut length is given, or follows from a sag in still air.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Jumper {
    /// Unique id.
    pub id: String,
    /// Cable id in the wire libraries.
    pub cable: String,
    /// First end.
    pub from: SectionPoint,
    /// Second end.
    pub to: SectionPoint,
    /// Unstressed (cut) length, m, at `temperature`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<f64>,
    /// Or: largest vertical distance below the end-to-end chord, m, in
    /// still air at `temperature` (jumpers without support only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sag: Option<f64>,
    /// Temperature of `length` / `sag`, °C.
    #[serde(default = "default_jumper_temperature")]
    pub temperature: f64,
    /// Optional jumper support string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support: Option<JumperSupport>,
    /// Bending stiffness EI of the cable, N·m²: the jumper leaves a
    /// section's end clamp along the conductor's axis and bends away from
    /// it, instead of hanging from a hinge. Default: an estimate from the
    /// cable ([`jumper::estimated_bending_stiffness`]); 0: hinged ends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bending_stiffness: Option<f64>,
    /// Angle at which the jumper leaves the clamp at `from`, below the
    /// conductor's axis, degrees (0: straight on along the conductor;
    /// negative: up); default [`jumper::DEFAULT_TERMINAL_ANGLE_DEG`]. Used
    /// where `from` is a section's end clamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle_from_deg: Option<f64>,
    /// The same at `to`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle_to_deg: Option<f64>,
    /// Display colour (`#rrggbb`); default: the colour of the section at
    /// `from`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Subconductors; default: the bundle of the section at `from`
    /// ([`Line::jumper_bundle`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle: Option<u32>,
    /// Distance between neighbouring subconductors, m; default: that of the
    /// section at `from`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_spacing: Option<f64>,
    /// Rotation of the bundle, degrees; default: that of the section at
    /// `from`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_rotation_deg: Option<f64>,
}

/// A string holding a jumper at one point (DESIGN_GOALS §6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JumperSupport {
    /// Attachment point of the string on the structure, m.
    pub point: Point,
    /// String length, m.
    pub length: f64,
    /// String weight, N.
    #[serde(default)]
    pub weight: f64,
    /// Where the string holds the jumper: fraction of the jumper's length
    /// from `from`, 0–1.
    #[serde(default = "half")]
    pub fraction: f64,
}

/// Default wire colours by phase number (1, 2, 3, then repeating); phase 0
/// (e.g. shield wires) is grey. Red / yellow / blue reads well on dark and
/// light backgrounds; utilities' own conventions go in [`Display`].
pub const DEFAULT_PHASE_COLORS: [&str; 3] = ["#e4572e", "#f2c14e", "#4c9be8"];
/// Colour of phase 0 wires.
pub const NEUTRAL_COLOR: &str = "#b0b8c0";

/// Display settings of a line.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Display {
    /// Wire colours (`#rrggbb`) by section phase label, e.g.
    /// `{"L1": "#8b4513", "L2": "#202020", "L3": "#808080"}`.
    #[serde(default)]
    pub phase_colors: BTreeMap<String, String>,
}

impl Display {
    fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

fn is_hex_color(text: &str) -> bool {
    text.len() == 7 && text.starts_with('#') && text[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// A structure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Structure {
    /// Unique id, e.g. `"T12"`.
    pub id: String,
    /// Structure origin (centre at ground), metres.
    pub position: Point,
    /// Orientation of the structure's local x axis (to the right of the line
    /// looking ahead), degrees counter-clockwise from east. Used to place a
    /// structure type.
    #[serde(default)]
    pub orientation_deg: f64,
    /// Structure type id from the structure library (`.olss`); its
    /// attachments are generated by [`Line::resolve`].
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub structure_type: Option<String>,
    /// Height extension of the structure type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension: Option<String>,
    /// Attachment points (explicit; for typed structures they override the
    /// type's attachments with the same set and phase).
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    /// Typed structures: attachments of the type left out, `[set, phase]`
    /// (an explicit attachment with the same set and phase still counts).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub omit: Vec<[u32; 2]>,
    /// Stick drawing (polylines, metres): from the structure type when it
    /// is placed, or given; for views and drawings only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<Vec<Point>>,
}

/// One attachment point of a structure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attachment {
    /// Set number (cross-arm / attachment group).
    pub set: u32,
    /// Phase number within the set.
    pub phase: u32,
    /// Optional label.
    #[serde(default)]
    pub label: String,
    /// Where the insulator assembly attaches to the structure, metres.
    pub insulator_point: Point,
    /// Where the wire is clamped (end of the suspension string, plumb or
    /// swung less than 45°; end of the strain assembly for dead-ends), metres.
    pub wire_point: Point,
    /// Insulator weight, N (suspension strings).
    #[serde(default)]
    pub insulator_weight: f64,
    /// Insulator assembly id from the insulator library (`.olsi`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insulator: Option<String>,
    /// Area of the insulator assembly the wind blows on, m² (from the
    /// insulator library when resolved): half its wind load is put on the
    /// clamp.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub insulator_wind_area: f64,
    /// V-string: the attachment points of the two legs on the structure
    /// (the insulator point is then informative, e.g. their centre). Both
    /// legs end at the wire point; their lengths are the distances to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legs: Option<[Point; 2]>,
    /// Label of the string's top (insulator attachment point).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub top_label: String,
    /// Label of the string's bottom (wire attachment point).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub bottom_label: String,
    /// The string is a strain string: in a section's middle or end it is
    /// solved as a free member along its conductor (like a library strain
    /// assembly) instead of holding the wire at `wire_point`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub strain_member: bool,
}

/// One point of a section: structure id + attachment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionPoint {
    /// Structure id.
    pub structure: String,
    /// Set number.
    pub set: u32,
    /// Phase number.
    pub phase: u32,
}

/// How a section is strung, and its status.
///
/// `tension` and `sag` are sagging conditions of a section **in the
/// blocks** (unclipped): the wire rolls over the suspension clamps, so in
/// every weather case all its spans carry the same horizontal tension and
/// share the section's unstressed length (ruling-span behaviour); moving a
/// structure sags it again to its condition. `unstressed_lengths` is a
/// section **clipped in**: each span keeps its own unstressed length, so a
/// structure moved changes the spans next to it and the suspension strings
/// swing to balance them (the effect fades along the section).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Stringing {
    /// In the blocks: horizontal tension of every span at a temperature
    /// (still air).
    Tension {
        /// Horizontal tension per wire, N.
        horizontal_tension: f64,
        /// Cable temperature, °C.
        temperature: f64,
    },
    /// In the blocks: sagged to a sag at a temperature (still air) — in
    /// the ruling span (no `span`), or in span `span` (0 = first) — with
    /// the same horizontal tension in every span.
    Sag {
        /// Sag, m (largest vertical distance from the chord).
        sag: f64,
        /// Cable temperature, °C.
        temperature: f64,
        /// The span sagged (index); none: the ruling span.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<usize>,
    },
    /// Clipped in: the unstressed length of each span (per wire), at a
    /// temperature.
    UnstressedLengths {
        /// One length per span, metres.
        lengths: Vec<f64>,
        /// Temperature at which the lengths apply, °C.
        temperature: f64,
        /// The condition it was sagged to before clipping in (to go back
        /// to the blocks).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sagged: Option<Box<Stringing>>,
    },
}

impl Stringing {
    /// Whether the section is clipped in (else it is in the blocks).
    pub fn clipped(&self) -> bool {
        matches!(self, Stringing::UnstressedLengths { .. })
    }

    /// Temperature of the condition, °C.
    pub fn temperature(&self) -> f64 {
        match self {
            Stringing::Tension { temperature, .. }
            | Stringing::Sag { temperature, .. }
            | Stringing::UnstressedLengths { temperature, .. } => *temperature,
        }
    }
}

/// A point load in a span.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpanLoad {
    /// Span index (0 = first span of the section).
    pub span: usize,
    /// Horizontal fraction of the span from its back point, 0–1.
    pub fraction: f64,
    /// Downward force, N: on the whole bundle (shared between its
    /// subconductors).
    pub vertical: f64,
    /// Label, e.g. `"marker ball"`.
    #[serde(default)]
    pub label: String,
}

fn is_zero(x: &f64) -> bool {
    *x == 0.0
}

/// A wire section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Section {
    /// Unique id.
    pub id: String,
    /// Cable id in the wire libraries (`.olsw`).
    pub cable: String,
    /// Circuit name.
    #[serde(default)]
    pub circuit: String,
    /// Phase label.
    #[serde(default)]
    pub phase_label: String,
    /// Electrical system (id in the line's `systems`) whose phase
    /// `phase_label` this section carries — given once per conductor path
    /// (sections joined by jumpers follow). None on the whole path: earthed
    /// (earth wire, OPGW). See [`electrical`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    /// Subconductors per phase (1 = single wire). Each subconductor carries
    /// the same tension and loads; the solve follows one subconductor (the
    /// bundle's centre line) with its share of the insulator weight.
    #[serde(default = "one")]
    pub bundle: u32,
    /// Bundles: distance between neighbouring subconductors, m (e.g. 0.4).
    /// They sit evenly on a circle around the centre line, in the plane
    /// across the wire ([`Section::bundle_offsets`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_spacing: Option<f64>,
    /// Bundles: rotation of the arrangement, degrees counter-clockwise
    /// looking ahead (default 0: a flat bottom — two side by side, three
    /// with one on top, four in a square).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_rotation_deg: Option<f64>,
    /// Attachment points in line order (first and last: strain points).
    pub points: Vec<SectionPoint>,
    /// Stringing.
    pub stringing: Stringing,
    /// Point loads (marker balls, spacers, …).
    #[serde(default)]
    pub loads: Vec<SpanLoad>,
    /// Display colour (`#rrggbb`), overriding the phase colour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Cable condition and method it is solved with (default Initial FE).
    #[serde(default, skip_serializing_if = "condition::Calculation::is_default")]
    pub calculation: condition::Calculation,
}

fn one() -> u32 {
    1
}

impl Structure {
    /// The structure moved to `position`, with its attachments, V-string
    /// legs and stick drawing.
    pub fn moved_to(&self, position: Point) -> Structure {
        let d = [
            position[0] - self.position[0],
            position[1] - self.position[1],
            position[2] - self.position[2],
        ];
        let shift = |p: &mut Point| {
            for k in 0..3 {
                p[k] += d[k];
            }
        };
        let mut moved = self.clone();
        moved.position = position;
        for a in &mut moved.attachments {
            shift(&mut a.insulator_point);
            shift(&mut a.wire_point);
            if let Some(legs) = &mut a.legs {
                legs.iter_mut().for_each(shift);
            }
        }
        for m in &mut moved.members {
            m.iter_mut().for_each(shift);
        }
        moved
    }
}

/// The position of a subconductor at `offset` (across, up — see
/// [`Section::bundle_offsets`]) from the centre-line point `q` of a wire
/// running from `a` to `b`: across is horizontal, to the right looking from
/// `a` to `b`; up is the vertical made square to the wire.
pub fn bundle_point(q: Point, a: Point, b: Point, offset: [f64; 2]) -> Point {
    let t = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let len = (t[0] * t[0] + t[1] * t[1] + t[2] * t[2]).sqrt();
    let t = if len > 0.0 {
        t.map(|v| v / len)
    } else {
        [1.0, 0.0, 0.0]
    };
    let h = t[0].hypot(t[1]);
    let across = if h > 1e-12 {
        [t[1] / h, -t[0] / h, 0.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let up = {
        let u = [-t[2] * t[0], -t[2] * t[1], 1.0 - t[2] * t[2]];
        let n = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt();
        if n > 1e-12 {
            u.map(|v| v / n)
        } else {
            [0.0, 1.0, 0.0]
        }
    };
    [0, 1, 2].map(|k| q[k] + offset[0] * across[k] + offset[1] * up[k])
}

impl Section {
    /// The section's bundle.
    pub fn bundle_geometry(&self) -> Bundle {
        Bundle {
            count: self.bundle.max(1),
            spacing: self.bundle_spacing,
            rotation_deg: self.bundle_rotation_deg,
        }
    }

    /// The polylines of the subconductors along a centre-line polyline of
    /// this section (see [`Bundle::polylines`]).
    pub fn subconductor_polylines(&self, points: &[Point]) -> Vec<Vec<Point>> {
        self.bundle_geometry().polylines(points)
    }

    /// Positions of the subconductors around the centre line (see
    /// [`Bundle::offsets`]).
    pub fn bundle_offsets(&self) -> Vec<[f64; 2]> {
        self.bundle_geometry().offsets()
    }
}

/// A bundle of subconductors: how many, how far apart, how turned.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bundle {
    /// Subconductors (1: a single wire).
    pub count: u32,
    /// Distance between neighbouring subconductors, m.
    pub spacing: Option<f64>,
    /// Rotation of the arrangement, degrees.
    pub rotation_deg: Option<f64>,
}

impl Bundle {
    /// A single wire.
    pub const SINGLE: Bundle = Bundle {
        count: 1,
        spacing: None,
        rotation_deg: None,
    };

    /// Positions of the subconductors around the centre line, m: (across,
    /// up) in the plane across the wire — across to the right looking
    /// ahead, up away from the ground. On a circle of radius
    /// `s / (2·sin(π/n))`; by default the two lowest subconductors are
    /// level. One entry `[0, 0]` for a single wire or a bundle without a
    /// spacing (geometry as one wire).
    pub fn offsets(&self) -> Vec<[f64; 2]> {
        let n = self.count.max(1);
        let Some(s) = self.spacing.filter(|_| n >= 2) else {
            return vec![[0.0, 0.0]];
        };
        let r = s / (2.0 * (std::f64::consts::PI / f64::from(n)).sin());
        let start = -std::f64::consts::FRAC_PI_2
            + std::f64::consts::PI / f64::from(n)
            + self.rotation_deg.unwrap_or(0.0).to_radians();
        (0..n)
            .map(|k| {
                let a = start + 2.0 * std::f64::consts::PI * f64::from(k) / f64::from(n);
                [r * a.cos(), r * a.sin()]
            })
            .collect()
    }

    /// The polylines of the subconductors along a centre-line polyline
    /// (one polyline, the same points, for a single wire). At each point
    /// the wire's direction is that between its neighbours.
    pub fn polylines(&self, points: &[Point]) -> Vec<Vec<Point>> {
        let offsets = self.offsets();
        if offsets.len() <= 1 {
            return vec![points.to_vec()];
        }
        offsets
            .iter()
            .map(|o| {
                (0..points.len())
                    .map(|k| {
                        let a = points[k.saturating_sub(1)];
                        let b = points[(k + 1).min(points.len() - 1)];
                        bundle_point(points[k], a, b, *o)
                    })
                    .collect()
            })
            .collect()
    }

    fn check(&self, what: &str) -> Result<(), String> {
        if self.count == 0 {
            return Err(format!("{what}: bundle must be at least 1"));
        }
        match (self.count, self.spacing) {
            (1, Some(_)) => Err(format!(
                "{what}: bundle_spacing is for bundles (bundle ≥ 2)"
            )),
            (_, Some(s)) if !(s.is_finite() && s > 0.0) => {
                Err(format!("{what}: bundle_spacing must be > 0 m"))
            }
            _ if self.rotation_deg.is_some_and(|r| !r.is_finite()) => {
                Err(format!("{what}: bundle_rotation_deg must be a number"))
            }
            _ => Ok(()),
        }
    }
}

impl Line {
    /// The section a jumper takes its defaults from (colour, bundle,
    /// circuit): the first section through its `from` point.
    pub fn jumper_section(&self, jumper: &Jumper) -> Option<&Section> {
        self.sections
            .iter()
            .find(|s| s.points.contains(&jumper.from))
    }

    /// A jumper's bundle: its own `bundle`, `bundle_spacing` and
    /// `bundle_rotation_deg` where given, the rest from its section
    /// ([`Line::jumper_section`]), else a single wire.
    pub fn jumper_bundle(&self, jumper: &Jumper) -> Bundle {
        let base = self
            .jumper_section(jumper)
            .map_or(Bundle::SINGLE, Section::bundle_geometry);
        let count = jumper.bundle.unwrap_or(base.count).max(1);
        Bundle {
            count,
            // A spacing only for a bundle (a jumper set to a single wire
            // drops the section's).
            spacing: jumper
                .bundle_spacing
                .or(base.spacing)
                .filter(|_| count >= 2),
            rotation_deg: jumper.bundle_rotation_deg.or(base.rotation_deg),
        }
    }
}

/// Errors in a line model.
#[derive(Debug, Clone, PartialEq)]
pub enum LineError {
    /// File could not be read or parsed.
    File(String),
    /// The model is inconsistent.
    Invalid(String),
}

impl fmt::Display for LineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(m) => write!(f, "line file: {m}"),
            Self::Invalid(m) => write!(f, "invalid line: {m}"),
        }
    }
}

impl std::error::Error for LineError {}

impl Line {
    /// Moves every coordinate of the line with `transform` (all points at
    /// once): structure positions, attachment, leg and stick points,
    /// alignments, jumper supports. Structure orientations follow the
    /// rotation of the grid (meridian convergence between projections),
    /// from a point 1 m along each structure's local x axis.
    pub fn transform_coordinates(
        &mut self,
        transform: impl FnOnce(&mut [Point]) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut points: Vec<Point> = Vec::new();
        // Gather in a fixed order; write back in the same order.
        for s in &self.structures {
            let r = s.orientation_deg.to_radians();
            points.push(s.position);
            points.push([
                s.position[0] + r.cos(),
                s.position[1] + r.sin(),
                s.position[2],
            ]);
            for a in &s.attachments {
                points.push(a.insulator_point);
                points.push(a.wire_point);
                points.extend(a.legs.iter().flatten());
            }
            points.extend(s.members.iter().flatten());
        }
        for r in &self.routes {
            points.extend(r.alignment.iter().map(|a| a.position));
        }
        points.extend(self.alignment.iter().map(|a| a.position));
        for j in &self.jumpers {
            points.extend(j.support.iter().map(|sp| sp.point));
        }
        transform(&mut points)?;
        let mut it = points.into_iter();
        let mut next = || it.next().expect("as many points as gathered");
        for s in &mut self.structures {
            s.position = next();
            let x = next();
            s.orientation_deg = (x[1] - s.position[1])
                .atan2(x[0] - s.position[0])
                .to_degrees();
            for a in &mut s.attachments {
                a.insulator_point = next();
                a.wire_point = next();
                if let Some(legs) = &mut a.legs {
                    for l in legs.iter_mut() {
                        *l = next();
                    }
                }
            }
            for m in &mut s.members {
                for q in m.iter_mut() {
                    *q = next();
                }
            }
        }
        for r in &mut self.routes {
            for a in &mut r.alignment {
                a.position = next();
            }
        }
        for a in &mut self.alignment {
            a.position = next();
        }
        for j in &mut self.jumpers {
            if let Some(sp) = &mut j.support {
                sp.point = next();
            }
        }
        Ok(())
    }

    /// Reads and validates a `.ols.json` file.
    pub fn load(path: &Path) -> Result<Self, LineError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| LineError::File(format!("{}: {e}", path.display())))?;
        Self::from_json(&text).map_err(|e| match e {
            LineError::File(m) => LineError::File(format!("{}: {m}", path.display())),
            other => other,
        })
    }

    /// Parses and validates JSON text.
    pub fn from_json(text: &str) -> Result<Self, LineError> {
        let line: Self = serde_json::from_str(text).map_err(|e| LineError::File(e.to_string()))?;
        line.validate()?;
        Ok(line)
    }

    /// Serialises to pretty JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("line serialises")
    }

    /// Structure by id.
    pub fn structure(&self, id: &str) -> Option<&Structure> {
        self.structures.iter().find(|s| s.id == id)
    }

    /// Attachment referenced by a section point.
    pub fn attachment(&self, point: &SectionPoint) -> Option<&Attachment> {
        self.structure(&point.structure)?
            .attachments
            .iter()
            .find(|a| a.set == point.set && a.phase == point.phase)
    }

    /// Display colour of a section: its own colour, else the line's colour
    /// for its phase label, else a default by the phase number of its first
    /// point ([`DEFAULT_PHASE_COLORS`], [`NEUTRAL_COLOR`] for phase 0).
    pub fn section_color(&self, section: &Section) -> String {
        if let Some(c) = &section.color {
            return c.clone();
        }
        if let Some(c) = self.display.phase_colors.get(&section.phase_label) {
            return c.clone();
        }
        match section.points.first().map_or(0, |p| p.phase) {
            0 => NEUTRAL_COLOR.to_owned(),
            n => DEFAULT_PHASE_COLORS[(n as usize - 1) % DEFAULT_PHASE_COLORS.len()].to_owned(),
        }
    }

    /// Tied alignment points must be under an existing structure.
    fn check_alignment(&self, what: &str, alignment: &[AlignmentPoint]) -> Result<(), LineError> {
        for (k, a) in alignment.iter().enumerate() {
            let Some(id) = &a.structure else { continue };
            let Some(s) = self.structure(id) else {
                return Err(LineError::Invalid(format!(
                    "{what}: alignment point {k} is tied to unknown structure {id}"
                )));
            };
            let d = (a.position[0] - s.position[0]).hypot(a.position[1] - s.position[1]);
            if d > 0.001 {
                return Err(LineError::Invalid(format!(
                    "{what}: alignment point {k} is tied to {id} but {d:.3} m away from it (detach it to move it)"
                )));
            }
        }
        Ok(())
    }

    /// Checks references and basic consistency.
    pub fn validate(&self) -> Result<(), LineError> {
        let invalid = |m: String| Err(LineError::Invalid(m));
        self.bimetallic.validate().map_err(LineError::Invalid)?;
        if self.format != LINE_FILE_FORMAT {
            return invalid(format!(
                "format \"{}\" is not \"{LINE_FILE_FORMAT}\"",
                self.format
            ));
        }
        if self.version > LINE_FILE_VERSION {
            return invalid(format!(
                "version {} is newer than supported ({LINE_FILE_VERSION})",
                self.version
            ));
        }
        if let Some((label, c)) = self
            .display
            .phase_colors
            .iter()
            .find(|(_, c)| !is_hex_color(c))
        {
            return invalid(format!(
                "display: colour \"{c}\" of phase \"{label}\" is not #rrggbb"
            ));
        }
        let mut ids = BTreeMap::new();
        for s in &self.structures {
            if ids.insert(s.id.as_str(), ()).is_some() {
                return invalid(format!("duplicate structure id {}", s.id));
            }
            let finite = |p: &Point| p.iter().all(|x| x.is_finite());
            if !finite(&s.position) || !s.orientation_deg.is_finite() {
                return invalid(format!(
                    "structure {}: position or orientation not a number",
                    s.id
                ));
            }
            let mut seen = BTreeMap::new();
            for a in &s.attachments {
                let points_ok = finite(&a.insulator_point)
                    && finite(&a.wire_point)
                    && a.legs.iter().flatten().all(finite)
                    && a.insulator_weight.is_finite();
                if !points_ok {
                    return invalid(format!(
                        "structure {}: set {} phase {}: a point or weight is not a number",
                        s.id, a.set, a.phase
                    ));
                }
                if seen.insert((a.set, a.phase), ()).is_some() {
                    return invalid(format!(
                        "structure {}: duplicate attachment set {} phase {}",
                        s.id, a.set, a.phase
                    ));
                }
                if let Some(legs) = &a.legs {
                    let ok = legs.iter().all(|l| {
                        let d = [
                            a.wire_point[0] - l[0],
                            a.wire_point[1] - l[1],
                            a.wire_point[2] - l[2],
                        ];
                        d[2] < 0.0 && (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() > 0.01
                    });
                    if !ok {
                        return invalid(format!(
                            "structure {}: V-string set {} phase {}: the wire point must hang below both legs",
                            s.id, a.set, a.phase
                        ));
                    }
                }
            }
        }
        self.check_alignment("line alignment", &self.alignment)?;
        // Systems, and one system phase per conductor path.
        self.section_phases()?;
        let mut route_ids = BTreeMap::new();
        for r in &self.routes {
            if route_ids.insert(r.id.as_str(), ()).is_some() {
                return invalid(format!("duplicate route id {}", r.id));
            }
            if r.structures.len() < 2 {
                return invalid(format!("route {}: needs at least two structures", r.id));
            }
            if r.alignment.len() == 1 {
                return invalid(format!(
                    "route {}: an alignment needs at least two points",
                    r.id
                ));
            }
            self.check_alignment(&format!("route {}", r.id), &r.alignment)?;
            if let Some(s) = r.structures.iter().find(|s| self.structure(s).is_none()) {
                return invalid(format!("route {}: unknown structure {s}", r.id));
            }
        }
        let mut section_ids = BTreeMap::new();
        for sec in &self.sections {
            if section_ids.insert(sec.id.as_str(), ()).is_some() {
                return invalid(format!("duplicate section id {}", sec.id));
            }
            if sec.points.len() < 2 {
                return invalid(format!("section {}: needs at least two points", sec.id));
            }
            for p in &sec.points {
                let Some(structure) = self.structure(&p.structure) else {
                    return invalid(format!(
                        "section {}: unknown structure {}",
                        sec.id, p.structure
                    ));
                };
                // Typed structures get their attachments from the library
                // (`resolve`), which validates again afterwards.
                if structure.structure_type.is_none() && self.attachment(p).is_none() {
                    return invalid(format!(
                        "section {}: no attachment set {} phase {} on structure {}",
                        sec.id, p.set, p.phase, p.structure
                    ));
                }
            }
            let spans = sec.points.len() - 1;
            if let Stringing::UnstressedLengths { lengths, .. } = &sec.stringing
                && lengths.len() != spans
            {
                return invalid(format!(
                    "section {}: {} lengths for {spans} spans",
                    sec.id,
                    lengths.len()
                ));
            }
            if let Stringing::Sag { sag, span, .. } = &sec.stringing {
                if sag.is_nan() || *sag <= 0.0 {
                    return invalid(format!("section {}: sag must be above 0", sec.id));
                }
                if let Some(k) = span
                    && *k >= spans
                {
                    return invalid(format!(
                        "section {}: sag in span {} of {spans}",
                        sec.id,
                        k + 1
                    ));
                }
            }
            if let Some(l) = sec
                .loads
                .iter()
                .find(|l| l.span >= spans || !(0.0 < l.fraction && l.fraction < 1.0))
            {
                return invalid(format!(
                    "section {}: load {:?} is not inside a span",
                    sec.id, l.label
                ));
            }
            if let Some(c) = sec.color.as_deref().filter(|c| !is_hex_color(c)) {
                return invalid(format!("section {}: colour \"{c}\" is not #rrggbb", sec.id));
            }
            Bundle {
                count: sec.bundle,
                spacing: sec.bundle_spacing,
                rotation_deg: sec.bundle_rotation_deg,
            }
            .check(&format!("section {}", sec.id))
            .map_err(LineError::Invalid)?;
        }
        let mut jumper_ids = BTreeMap::new();
        for j in &self.jumpers {
            let bad = |m: &str| invalid(format!("jumper {}: {m}", j.id));
            if jumper_ids.insert(j.id.as_str(), ()).is_some() {
                return bad("duplicate id");
            }
            // Its own bundle values, over those it takes from its section.
            let own = Bundle {
                count: j.bundle.unwrap_or_else(|| self.jumper_bundle(j).count),
                spacing: j.bundle_spacing,
                rotation_deg: j.bundle_rotation_deg,
            };
            own.check(&format!("jumper {}", j.id))
                .map_err(LineError::Invalid)?;
            for end in [&j.from, &j.to] {
                let Some(structure) = self.structure(&end.structure) else {
                    return bad(&format!("unknown structure {}", end.structure));
                };
                if structure.structure_type.is_none() && self.attachment(end).is_none() {
                    return bad(&format!(
                        "no attachment set {} phase {} on structure {}",
                        end.set, end.phase, end.structure
                    ));
                }
            }
            if j.from == j.to {
                return bad("both ends are the same attachment");
            }
            match (j.length, j.sag) {
                (Some(l), None) if l > 0.0 => {}
                (None, Some(s)) if s > 0.0 => {}
                _ => return bad("give either a positive length or a positive sag"),
            }
            if j.bending_stiffness
                .is_some_and(|e| !(e >= 0.0 && e.is_finite()))
            {
                return bad("bending stiffness must be ≥ 0");
            }
            if [j.angle_from_deg, j.angle_to_deg]
                .into_iter()
                .flatten()
                .any(|a| !(-90.0..=90.0).contains(&a))
            {
                return bad("clamp exit angles must be between −90° and 90°");
            }
            if let Some(support) = &j.support {
                if j.length.is_none() {
                    return bad(
                        "a supported jumper needs a length (its sag depends on the support)",
                    );
                }
                if !(support.length > 0.0 && support.weight >= 0.0) {
                    return bad("support length must be > 0 and weight ≥ 0");
                }
                if !(0.0 < support.fraction && support.fraction < 1.0) {
                    return bad("support fraction must be between 0 and 1");
                }
            }
            if let Some(c) = j.color.as_deref().filter(|c| !is_hex_color(c)) {
                return bad(&format!("colour \"{c}\" is not #rrggbb"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn example() -> Line {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/synthetic-line/synthetic-line.ols.json");
        Line::load(&path).unwrap()
    }

    #[test]
    fn bundle_offsets_on_a_circle() {
        let mut s = example().sections[0].clone();
        assert_eq!(s.bundle_offsets(), [[0.0, 0.0]]);
        s.bundle = 2;
        s.bundle_spacing = Some(0.4);
        let two = s.bundle_offsets();
        assert!(
            (two[0][0] - 0.2).abs() < 1e-12 && two[0][1].abs() < 1e-12,
            "{two:?}"
        );
        assert!((two[1][0] + 0.2).abs() < 1e-12 && two[1][1].abs() < 1e-12);
        for n in 3..=8 {
            s.bundle = n;
            let o = s.bundle_offsets();
            // Neighbours 0.4 m apart; the two lowest level.
            for k in 0..o.len() {
                let (a, b) = (o[k], o[(k + 1) % o.len()]);
                assert!(
                    ((a[0] - b[0]).hypot(a[1] - b[1]) - 0.4).abs() < 1e-12,
                    "{n}: {o:?}"
                );
            }
            let mut ys: Vec<f64> = o.iter().map(|p| p[1]).collect();
            ys.sort_by(f64::total_cmp);
            assert!((ys[0] - ys[1]).abs() < 1e-12, "{n}: {o:?}");
        }
        // Three: one on top; turned by 60°: one at the bottom.
        s.bundle = 3;
        let r = 0.4 / (2.0 * (std::f64::consts::PI / 3.0).sin());
        assert!(s.bundle_offsets().iter().any(|p| (p[1] - r).abs() < 1e-12));
        s.bundle_rotation_deg = Some(60.0);
        assert!(s.bundle_offsets().iter().any(|p| (p[1] + r).abs() < 1e-12));
        // Checked in the line.
        let mut line = example();
        line.sections[0].bundle_spacing = Some(0.4);
        assert!(
            line.validate()
                .unwrap_err()
                .to_string()
                .contains("bundle ≥ 2")
        );
        line.sections[0].bundle = 2;
        line.validate().unwrap();
        line.sections[0].bundle_spacing = Some(0.0);
        assert!(line.validate().is_err());
        // Subconductor polylines along a level wire running north: a
        // 2-bundle is 0.2 m east and west of it.
        let mut sec = example().sections[0].clone();
        sec.bundle = 2;
        sec.bundle_spacing = Some(0.4);
        let centre = [[0.0, 0.0, 10.0], [0.0, 50.0, 9.0], [0.0, 100.0, 10.0]];
        let subs = sec.subconductor_polylines(&centre);
        assert_eq!(subs.len(), 2);
        for (k, p) in subs[0].iter().enumerate() {
            assert!(
                (p[0] - 0.2).abs() < 1e-9 && (p[1] - centre[k][1]).abs() < 1e-9,
                "{p:?}"
            );
        }
        assert!(subs[1].iter().all(|p| (p[0] + 0.2).abs() < 1e-9));
        sec.bundle = 1;
        sec.bundle_spacing = None;
        assert_eq!(sec.subconductor_polylines(&centre), vec![centre.to_vec()]);
    }

    #[test]
    fn jumpers_take_their_sections_bundle_unless_set() {
        let mut line = example();
        // The jumper at T3 starts on S3 (a single wire): single.
        let j = line.jumpers[0].clone();
        assert_eq!(line.jumper_section(&j).unwrap().id, "S3");
        assert_eq!(line.jumper_bundle(&j), Bundle::SINGLE);
        // S3 becomes a 3-bundle: the jumper follows.
        line.sections[2].bundle = 3;
        line.sections[2].bundle_spacing = Some(0.4);
        line.validate().unwrap();
        let b = line.jumper_bundle(&j);
        assert_eq!((b.count, b.spacing), (3, Some(0.4)));
        // Its own spacing overrides; its own count of 1 drops the spacing.
        line.jumpers[0].bundle_spacing = Some(0.3);
        let b = line.jumper_bundle(&line.jumpers[0]);
        assert_eq!((b.count, b.spacing), (3, Some(0.3)));
        line.jumpers[0].bundle_spacing = None;
        line.jumpers[0].bundle = Some(1);
        assert_eq!(line.jumper_bundle(&line.jumpers[0]), Bundle::SINGLE);
        line.validate().unwrap();
        // A spacing on a single-wire jumper is refused.
        line.jumpers[0].bundle_spacing = Some(0.3);
        assert!(line.validate().unwrap_err().to_string().contains("jumper"));
        // Saved only when set.
        let text = example().to_json();
        assert!(!text.contains("\"bundle\": 1,\n      \"bundle_spacing"));
    }

    #[test]
    fn example_loads_and_round_trips() {
        let line = example();
        assert!(!line.sections.is_empty());
        let again = Line::from_json(&line.to_json()).unwrap();
        assert_eq!(again, line);
    }

    #[test]
    fn detects_bad_references() {
        let mut line = example();
        line.sections[0].points[0].set = 999;
        assert!(
            line.validate()
                .unwrap_err()
                .to_string()
                .contains("no attachment set 999")
        );
        let mut line = example();
        line.sections[0].loads.push(SpanLoad {
            span: 99,
            fraction: 0.5,
            vertical: 55.0,
            label: "ball".into(),
        });
        assert!(line.validate().is_err());
        let mut line = example();
        line.version = 99;
        assert!(line.validate().unwrap_err().to_string().contains("newer"));
    }

    #[test]
    fn transforming_coordinates_moves_everything_and_turns_orientations() {
        let original = example();
        let mut line = original.clone();
        // A 30° rotation about the origin plus a shift.
        let (sn, cs) = 30f64.to_radians().sin_cos();
        line.transform_coordinates(|pts| {
            for p in pts.iter_mut() {
                *p = [
                    cs * p[0] - sn * p[1] + 1000.0,
                    sn * p[0] + cs * p[1] - 500.0,
                    p[2],
                ];
            }
            Ok(())
        })
        .unwrap();
        line.validate().unwrap();
        for (a, b) in original.structures.iter().zip(&line.structures) {
            let turned =
                (b.orientation_deg - a.orientation_deg - 30.0 + 540.0).rem_euclid(360.0) - 180.0;
            assert!(
                turned.abs() < 1e-9,
                "{} {}",
                a.orientation_deg,
                b.orientation_deg
            );
            // Distances inside a structure are kept.
            let d = |s: &Structure| {
                let (p, q) = (
                    s.attachments[0].insulator_point,
                    s.attachments[0].wire_point,
                );
                ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
            };
            assert!((d(a) - d(b)).abs() < 1e-9);
        }
        assert!((line.routes[0].alignment[0].position[0] - 1000.0).abs() < 1e-9);
        // A failing transformation leaves an error.
        assert!(line.transform_coordinates(|_| Err("no".into())).is_err());
    }

    #[test]
    fn section_colours_by_precedence() {
        let mut line = example();
        // Defaults by phase number of the first point.
        assert_eq!(
            line.section_color(&line.sections[0]),
            DEFAULT_PHASE_COLORS[0]
        );
        assert_eq!(
            line.section_color(&line.sections[1]),
            DEFAULT_PHASE_COLORS[1]
        );
        // The project's colour for the phase label, then the section's own.
        line.display
            .phase_colors
            .insert("L1".into(), "#8b4513".into());
        assert_eq!(line.section_color(&line.sections[0]), "#8b4513");
        assert_eq!(line.section_color(&line.sections[2]), "#8b4513");
        line.sections[2].color = Some("#00ff00".into());
        assert_eq!(line.section_color(&line.sections[2]), "#00ff00");
        let again = Line::from_json(&line.to_json()).unwrap();
        assert_eq!(again, line);
        line.display
            .phase_colors
            .insert("L2".into(), "brown".into());
        assert!(line.validate().unwrap_err().to_string().contains("#rrggbb"));
    }

    #[test]
    fn v_string_legs_must_be_above_the_clamp() {
        let mut line = example();
        let a = &mut line.structures[1].attachments[0];
        let c = a.insulator_point;
        a.legs = Some([
            [c[0], c[1] - 3.0, c[2]],
            [c[0], c[1] + 3.0, a.wire_point[2] - 1.0],
        ]);
        assert!(
            line.validate()
                .unwrap_err()
                .to_string()
                .contains("below both legs")
        );
    }
}
