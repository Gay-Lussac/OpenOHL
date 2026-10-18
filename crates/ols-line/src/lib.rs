// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Line data model: structures with their strings, wire sections.
//!
//! A [`Line`] is stored as JSON (`*.ols.json`, the open exchange format of
//! ADR-0004). Coordinates are numbers in metres in the line's coordinate
//! reference system (`crs`, e.g. `"EPSG:2056"`); x east, y north, z up.
//!
//! - [`Structure`]: a tower or pole with what hangs on it, given
//!   explicitly ([`assembly`]): **points** on the structure, **strings**
//!   from a point or another string's wire point to a wire point, and
//!   **wire points** — those with a **set** number are where wires end. A
//!   V-string is two strings to one wire point; strings hang from strings
//!   (a V with a string below, a TAK).
//! - [`Section`]: one wire (or bundle) strung through wire points (structure
//!   and set), with its cable and stringing. Spans are between consecutive
//!   points.
//!
//! - [`Jumper`]: a slack cable joining two wire points — the ends of two
//!   sections at a dead-end, or a tap from a branch to a through wire —
//!   optionally held by a support string, or by support clamps on a
//!   structure's strings ([`JumperClamp`]); solved by [`jumper`].
//!
//! [`solve`] turns a section into a finite-element model (exact catenary
//! elements, insulator strings) and solves it in any weather state.

pub mod assembly;
pub mod bundles;
pub mod condition;
pub mod electrical;
pub mod export;
pub mod incremental;
pub mod jumper;
pub mod legacy;
pub mod library;
pub mod objects;
mod rod;
pub mod route;
pub mod solve;

pub use objects::{
    Axis, LineObject, ObjectKind, ObjectLibrary, ObjectType, Shape, SpreadObject, TypeKind,
};

pub use assembly::{
    AttachmentV1, InsulatorString, StringLimits, StructurePoint, Swing, WirePoint, swing,
};
pub use route::{
    Alignment, AlignmentPoint, AlignmentView, NEAR_ALIGNMENT, OldPoint, Route, Station,
};

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

use legacy::{SpanLoad, SpreadLoad};

/// File format identifier.
pub const LINE_FILE_FORMAT: &str = "ols-line";
/// Latest line file version: 6 adds a string's optional limits, not
/// checked by OLS CAD (`max_load`, `max_swing_deg`,
/// `max_swing_across_deg`, `max_swing_along_deg`: [`StringLimits`]); 5 writes a jumper's tap end as its `from` /
/// `to` ([`JumperEnd`]), its supports as points holding a held clamp
/// ([`objects::Hold`]) and a place with one key ([`SpanPlace`],
/// [`JumperPlace`]); 4 gave alignments points of their own, 3 routes with
/// alignments, 2 structures as points and strings. Older files are read
/// into the current form.
pub const LINE_FILE_VERSION: u32 = 6;

fn default_format() -> String {
    LINE_FILE_FORMAT.to_owned()
}

/// A file without a version is of the first one.
fn default_version() -> u32 {
    1
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
    /// Files of version 2 and before: the line's own alignment, used when
    /// it had no routes ([`Line::upgrade_routes`] makes it the route and
    /// alignment `main`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alignment: Vec<OldPoint>,
    /// Routes: paths of the line, each made of alignments; see [`route`].
    #[serde(default)]
    pub routes: Vec<Route>,
    /// Alignments, in their order along their routes; each belongs to
    /// one route and goes through alignment points, by id.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alignments: Vec<Alignment>,
    /// Alignment points (PIs), each on any number of alignments (none:
    /// placed, not used by an alignment yet).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alignment_points: Vec<AlignmentPoint>,
    /// Structures.
    pub structures: Vec<Structure>,
    /// Wire sections.
    pub sections: Vec<Section>,
    /// Jumpers between attachment points.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub jumpers: Vec<Jumper>,
    /// Interphase spacers: strings between two conductors in a span,
    /// clamped at span points ([`Spacer`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spacers: Vec<Spacer>,
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
    /// How the calculations are done (Criteria › Calculation options).
    #[serde(default, skip_serializing_if = "SolverOptions::is_default")]
    pub solver: SolverOptions,
    /// The project's default values (Criteria › Default values): what new
    /// objects start with.
    #[serde(default, skip_serializing_if = "Defaults::is_default")]
    pub defaults: Defaults,
    /// The ends of electrical circuits (conductor paths) that are starts
    /// (power flows in); a circuit none of whose ends is listed runs along
    /// the line, from its end that comes first ([`electrical::circuits`]).
    /// The roles order its sections and jumpers; nothing physical depends
    /// on them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub circuit_starts: Vec<SectionPoint>,
    /// What the line sends to the placed structures (Criteria › Loads
    /// for structures): which load cases, which weather cases on their
    /// own.
    #[serde(default, skip_serializing_if = "StructureLoads::is_default")]
    pub structure_loads: StructureLoads,
    /// Those weather cases themselves, given by the program solving the
    /// line ([`crate::Line::set_weather_cases`]); not stored.
    #[serde(skip)]
    pub condition_cases: condition::ConditionCases,
}

/// What the line sends to the placed structures, the cases of their
/// line-values file (Criteria › Loads for structures): the load cases
/// (`.olslc`, from a file or a design code) chosen, then the weather
/// cases chosen on their own (each a case with all factors 1).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructureLoads {
    /// The load cases sent, by name; none: all of them (the default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_cases: Option<Vec<String>>,
    /// The weather cases sent on their own, by name (default none).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub weather_cases: Vec<String>,
}

impl StructureLoads {
    /// All load cases, no weather case on its own.
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Whether load case `name` is sent.
    pub fn sends_load_case(&self, name: &str) -> bool {
        self.load_cases
            .as_ref()
            .is_none_or(|l| l.iter().any(|c| c == name))
    }
}

fn default_jumper_temperature() -> f64 {
    15.0
}

fn half() -> f64 {
    0.5
}

/// How the line's calculations are done (Criteria › Calculation
/// options); every section is solved again when they change.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SolverOptions {
    /// What of the jumpers the sections carry at their clamps.
    #[serde(default)]
    pub jumper_loads: JumperLoads,
    /// How a jumper starting from a point (a tap) and what it hangs on
    /// are solved.
    #[serde(default, skip_serializing_if = "TapLoads::is_default")]
    pub taps: TapLoads,
}

/// How taps are solved ([`Tap`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TapLoads {
    /// The full loop (the default): the jumper's own force at its tap put
    /// on the wire (or jumper) it hangs on, both solved in turn until the
    /// tap no longer moves.
    #[default]
    Loop,
    /// One calculation: the wire it hangs on carries the jumper's
    /// estimate (its weight, ice and wind up to its first support, half
    /// without), then the jumper is solved once.
    Once,
}

impl TapLoads {
    /// Every choice.
    pub const ALL: [Self; 2] = [Self::Loop, Self::Once];

    /// The default.
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// As written in files.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Loop => "loop",
            Self::Once => "once",
        }
    }

    /// From its written name.
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|j| j.as_str() == text)
    }
}

/// A point a jumper starts (or ends) at instead of a section's end: a
/// clamp object on a span point or on another jumper's point — a hinge,
/// forces only.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Tap {
    /// A span point of a section.
    Span(SpanPointRef),
    /// A point of another jumper.
    Jumper {
        /// The jumper's id.
        jumper: String,
        /// Its point's label.
        point: String,
    },
}

impl Tap {
    /// In words.
    pub fn text(&self) -> String {
        match self {
            Tap::Span(r) => format!("{} point {}", r.section, r.point),
            Tap::Jumper { jumper, point } => format!("jumper {jumper} point {point}"),
        }
    }
}

impl SolverOptions {
    /// Nothing changed from the defaults.
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// What of a jumper the sections at its ends carry at their clamps (on
/// clamps hanging from strings; a wire clamped to the structure leaves it
/// to the structure). The ruling-span method leaves jumpers out either
/// way, as every point load.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JumperLoads {
    /// An estimate (the default): each end carries the weight, ice and
    /// wind (across the line from the weather case's side, as on the
    /// jumper itself) of the jumper up to its first support, half of it
    /// without supports.
    #[default]
    Estimate,
    /// The jumper's own pull: the force the solved jumper exerts on each
    /// clamp (its tension at the end, with its stiffness), the sections
    /// and the jumpers solved in turn until the clamps no longer move.
    Pull,
}

impl JumperLoads {
    /// Every choice.
    pub const ALL: [Self; 2] = [Self::Estimate, Self::Pull];

    /// As written in files.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Estimate => "estimate",
            Self::Pull => "pull",
        }
    }

    /// From its written name.
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|j| j.as_str() == text)
    }
}

/// Values of table columns added by plugins (e.g. a design code's
/// insulation level), by `plugin.column`: kept with the object and saved
/// with it, used by the plugin, and shown (unused) where the plugin is
/// not installed.
pub type Fields = std::collections::BTreeMap<String, String>;

/// A jumper: a slack cable from the wire point of one attachment to that of
/// another (solved clamp positions when the points belong to sections).
/// Its cut length is given, or follows from a sag in still air.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "JumperFile")]
pub struct Jumper {
    /// What it is, in words (free text).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Unique id.
    pub id: String,
    /// Cable id in the wire libraries.
    pub cable: String,
    /// First end: a section's wire point, or a tap.
    pub from: JumperEnd,
    /// Second end.
    pub to: JumperEnd,
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
    /// A rigid jumper (a tube or bar): the straight member between its
    /// solved clamps, without length or sag; never taut, no thermal change.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rigid: bool,
    /// Bending stiffness EI of the cable, N·m²: the jumper leaves a
    /// section's end clamp along the conductor's axis and bends away from
    /// it, instead of hanging from a hinge. Default: estimated from the
    /// cable with `stiffness_factor`
    /// ([`jumper::estimated_bending_stiffness`]); 0: hinged ends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bending_stiffness: Option<f64>,
    /// Reduction factor of the estimated EI: the jumper's uniform effective
    /// stiffness as a share of the cable's solid-bar stiffness, 0–1;
    /// default [`jumper::DEFAULT_STIFFNESS_FACTOR`]. Unused when
    /// `bending_stiffness` is given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stiffness_factor: Option<f64>,
    /// Angle at which the jumper leaves the clamp at `from`, degrees, from
    /// its tension string's line (clamp towards the string's top; two
    /// strings: the middle of their tops; none: the conductor's axis),
    /// turned within the conductor's plane towards the weather case's load
    /// (0: along the string; negative: the other way); default
    /// [`jumper::DEFAULT_TERMINAL_ANGLE_DEG`]. Used where `from` is a
    /// section's end clamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle_from_deg: Option<f64>,
    /// The same at `to`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle_to_deg: Option<f64>,
    /// Labelled points along it, each with its object (a body, a clamp)
    /// or bare. A clamp held up by strings is a support
    /// ([`objects::Hold`], [`Jumper::supports`], [`Jumper::support`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<JumperPoint>,
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
    /// Values of columns added by plugins, by `plugin.column`
    /// ([`Fields`]).
    #[serde(default, skip_serializing_if = "Fields::is_empty")]
    pub fields: Fields,
}

/// One end of a jumper: a section's wire point (structure + set), written
/// `{"structure": "T3", "set": 2}`, or a tap (a clamp object on a span
/// point or another jumper's point), written `{"tap": {"span": …}}`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JumperEnd {
    /// A section's wire point.
    Wire(SectionPoint),
    /// A tap.
    Tap {
        /// Where.
        tap: Tap,
    },
}

impl JumperEnd {
    /// At the wire point with set `set` of structure `structure`.
    pub fn wire(structure: impl Into<String>, set: u32) -> Self {
        Self::Wire(SectionPoint::new(structure, set))
    }

    /// On tap `tap`.
    pub fn tap(tap: Tap) -> Self {
        Self::Tap { tap }
    }

    /// Its wire point, if at one.
    pub fn point(&self) -> Option<&SectionPoint> {
        match self {
            Self::Wire(p) => Some(p),
            Self::Tap { .. } => None,
        }
    }

    /// Its tap, if on one.
    pub fn as_tap(&self) -> Option<&Tap> {
        match self {
            Self::Tap { tap } => Some(tap),
            Self::Wire(_) => None,
        }
    }

    /// Whether it is at wire point `p`.
    pub fn is_at(&self, p: &SectionPoint) -> bool {
        self.point() == Some(p)
    }

    /// Its wire point, to change.
    pub fn point_mut(&mut self) -> Option<&mut SectionPoint> {
        match self {
            Self::Wire(p) => Some(p),
            Self::Tap { .. } => None,
        }
    }

    /// Its tap, to change.
    pub fn tap_mut(&mut self) -> Option<&mut Tap> {
        match self {
            Self::Tap { tap } => Some(tap),
            Self::Wire(_) => None,
        }
    }

    /// The structure of its wire point (none on a tap).
    pub fn structure(&self) -> Option<&str> {
        self.point().map(|p| p.structure.as_str())
    }
}

impl fmt::Display for JumperEnd {
    /// `T3 set 2`, `tap S1 P4` or `tap J2 K`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wire(p) => write!(f, "{} set {}", p.structure, p.set),
            Self::Tap { tap: Tap::Span(r) } => write!(f, "tap {} {}", r.section, r.point),
            Self::Tap {
                tap: Tap::Jumper { jumper, point },
            } => write!(f, "tap {jumper} {point}"),
        }
    }
}

impl PartialEq<SectionPoint> for JumperEnd {
    fn eq(&self, p: &SectionPoint) -> bool {
        self.is_at(p)
    }
}

impl PartialEq<JumperEnd> for SectionPoint {
    fn eq(&self, end: &JumperEnd) -> bool {
        end.is_at(self)
    }
}

impl From<SectionPoint> for JumperEnd {
    fn from(p: SectionPoint) -> Self {
        Self::Wire(p)
    }
}

impl From<Tap> for JumperEnd {
    fn from(tap: Tap) -> Self {
        Self::Tap { tap }
    }
}

impl Jumper {
    /// Its support clamps on a structure's strings ([`objects::Hold::Strings`]),
    /// in order from `from`, each labelled by its point.
    pub fn supports(&self) -> Vec<JumperClamp> {
        let mut out: Vec<JumperClamp> = self
            .points
            .iter()
            .filter_map(|p| {
                let o = p.object.as_ref()?;
                let objects::Hold::Strings { structure, wire } = o.hold.as_ref()? else {
                    return None;
                };
                Some(JumperClamp {
                    label: p.label.clone(),
                    structure: structure.clone(),
                    wire: wire.clone(),
                    at: p.at.share(),
                    clamp_length: o.clamp_length.unwrap_or(DEFAULT_CLAMP_LENGTH),
                })
            })
            .collect();
        out.sort_by(|a, b| a.at.total_cmp(&b.at));
        out
    }

    /// Its support string from a fixed point ([`objects::Hold::String`]).
    pub fn support(&self) -> Option<JumperSupport> {
        self.points.iter().find_map(|p| {
            let o = p.object.as_ref()?;
            let objects::Hold::String {
                point,
                length,
                weight,
            } = o.hold.as_ref()?
            else {
                return None;
            };
            Some(JumperSupport {
                point: *point,
                length: *length,
                weight: *weight,
                fraction: p.at.share(),
            })
        })
    }

    /// Gives it the support clamps `clamps` (instead of those it has):
    /// points by their labels (new ones `C1`, `C2`…) holding a clamp on
    /// the strings, at their fraction.
    pub fn set_supports(&mut self, clamps: Vec<JumperClamp>) {
        let held = |p: &JumperPoint, strings: bool| {
            p.object
                .as_ref()
                .and_then(|o| o.hold.as_ref())
                .is_some_and(|h| matches!(h, objects::Hold::Strings { .. }) == strings)
        };
        let old: Vec<JumperPoint> = self
            .points
            .iter()
            .filter(|p| held(p, true))
            .cloned()
            .collect();
        self.points.retain(|p| !held(p, true));
        for c in clamps {
            let label = if !c.label.is_empty() && self.points.iter().all(|p| p.label != c.label) {
                c.label
            } else {
                free_point_label(&self.points, "C")
            };
            let before = old.iter().find(|p| p.label == label);
            let mut object = before
                .and_then(|p| p.object.clone())
                .unwrap_or_else(|| LineObject::clamp(0.0));
            object.clamp_length = Some(c.clamp_length);
            object.hold = Some(objects::Hold::Strings {
                structure: c.structure,
                wire: c.wire,
            });
            self.points.push(JumperPoint {
                label,
                at: JumperPlace::Fraction(c.at),
                description: before.map(|p| p.description.clone()).unwrap_or_default(),
                object: Some(object),
            });
        }
    }

    /// Changes its support clamps with `f` ([`Jumper::supports`],
    /// [`Jumper::set_supports`]).
    pub fn edit_supports(&mut self, f: impl FnOnce(&mut Vec<JumperClamp>)) {
        let mut clamps = self.supports();
        f(&mut clamps);
        self.set_supports(clamps);
    }

    /// Changes its support string with `f` ([`Jumper::support`],
    /// [`Jumper::set_support`]).
    pub fn edit_support(&mut self, f: impl FnOnce(&mut Option<JumperSupport>)) {
        let mut s = self.support();
        f(&mut s);
        self.set_support(s);
    }

    /// Gives it the support string `support` (or none), instead of the
    /// one it has: a point (`S1`) holding a clamp on the string.
    pub fn set_support(&mut self, support: Option<JumperSupport>) {
        let is_string = |p: &JumperPoint| {
            p.object
                .as_ref()
                .and_then(|o| o.hold.as_ref())
                .is_some_and(|h| matches!(h, objects::Hold::String { .. }))
        };
        let at = self.points.iter().position(is_string);
        let old = at.map(|k| self.points.remove(k));
        self.points.retain(|p| !is_string(p));
        let Some(s) = support else {
            return;
        };
        let mut object = old
            .as_ref()
            .and_then(|p| p.object.clone())
            .unwrap_or_else(|| LineObject::clamp(0.0));
        object.hold = Some(objects::Hold::String {
            point: s.point,
            length: s.length,
            weight: s.weight,
        });
        let point = JumperPoint {
            label: old
                .as_ref()
                .map_or_else(|| free_point_label(&self.points, "S"), |p| p.label.clone()),
            at: JumperPlace::Fraction(s.fraction),
            description: old.map(|p| p.description).unwrap_or_default(),
            object: Some(object),
        };
        self.points.push(point);
    }

    /// Its taps (none, one or both ends).
    pub fn taps(&self) -> impl Iterator<Item = &Tap> {
        [&self.from, &self.to]
            .into_iter()
            .filter_map(JumperEnd::as_tap)
    }

    /// Whether an end is a tap.
    pub fn on_tap(&self) -> bool {
        self.taps().next().is_some()
    }

    /// Whether an end is at wire point `p`.
    pub fn is_at(&self, p: &SectionPoint) -> bool {
        self.from.is_at(p) || self.to.is_at(p)
    }

    /// Its ends' wire points (not its taps).
    pub fn points(&self) -> impl Iterator<Item = &SectionPoint> {
        [&self.from, &self.to]
            .into_iter()
            .filter_map(JumperEnd::point)
    }
}

/// A jumper as written in files, older ones included: before version 5 a
/// tap end was `from_tap` / `to_tap` beside a placeholder `from` / `to`,
/// and its supports were `support` / `supports` of their own.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JumperFile {
    #[serde(default)]
    description: String,
    id: String,
    cable: String,
    from: Option<JumperEnd>,
    to: Option<JumperEnd>,
    #[serde(default)]
    length: Option<f64>,
    #[serde(default)]
    sag: Option<f64>,
    #[serde(default = "default_jumper_temperature")]
    temperature: f64,
    #[serde(default)]
    rigid: bool,
    #[serde(default)]
    support: Option<JumperSupport>,
    #[serde(default)]
    supports: Vec<JumperClamp>,
    #[serde(default)]
    bending_stiffness: Option<f64>,
    #[serde(default)]
    stiffness_factor: Option<f64>,
    #[serde(default)]
    angle_from_deg: Option<f64>,
    #[serde(default)]
    angle_to_deg: Option<f64>,
    #[serde(default)]
    points: Vec<JumperPoint>,
    #[serde(default)]
    from_tap: Option<Tap>,
    #[serde(default)]
    to_tap: Option<Tap>,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    bundle: Option<u32>,
    #[serde(default)]
    bundle_spacing: Option<f64>,
    #[serde(default)]
    bundle_rotation_deg: Option<f64>,
    #[serde(default)]
    fields: Fields,
}

impl From<JumperFile> for Jumper {
    fn from(f: JumperFile) -> Self {
        // A tap given the old way wins over the placeholder beside it.
        let end = |end: Option<JumperEnd>, tap: Option<Tap>| match tap {
            Some(tap) => JumperEnd::Tap { tap },
            None => end.unwrap_or_else(|| JumperEnd::wire("", 0)),
        };
        let mut j = Jumper {
            description: f.description,
            id: f.id,
            cable: f.cable,
            from: end(f.from, f.from_tap),
            to: end(f.to, f.to_tap),
            length: f.length,
            sag: f.sag,
            temperature: f.temperature,
            rigid: f.rigid,
            bending_stiffness: f.bending_stiffness,
            stiffness_factor: f.stiffness_factor,
            angle_from_deg: f.angle_from_deg,
            angle_to_deg: f.angle_to_deg,
            points: f.points,
            color: f.color,
            bundle: f.bundle,
            bundle_spacing: f.bundle_spacing,
            bundle_rotation_deg: f.bundle_rotation_deg,
            fields: f.fields,
        };
        // Supports given on their own before version 5: points holding
        // a held clamp.
        if !f.supports.is_empty() {
            j.set_supports(f.supports);
        }
        if f.support.is_some() {
            j.set_support(f.support);
        }
        j
    }
}

/// A string holding a jumper at one point (DESIGN_GOALS §6), from a fixed
/// point: kept as a jumper point holding a clamp held by the string
/// ([`Jumper::support`]); version 4 files gave it as `support`.
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

/// Default length of a jumper's support clamp, m.
pub const DEFAULT_CLAMP_LENGTH: f64 = 0.1;

fn default_clamp_length() -> f64 {
    DEFAULT_CLAMP_LENGTH
}

/// A support clamp on a jumper, hanging on the strings of a structure:
/// one string (an I) or two strings from structure points (a V) to a wire
/// point without a set. The clamp is a straight rigid piece of the
/// jumper, `clamp_length` long, held at its middle. It swings and turns
/// freely; the jumper leaves it in the direction it enters. Kept as a
/// jumper point holding a clamp held by the strings
/// ([`Jumper::supports`]); version 4 files listed them as `supports`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JumperClamp {
    /// The label of its point on the jumper (none given: one is chosen).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    /// The structure holding it; default: that of the jumper's `from`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structure: Option<String>,
    /// The wire point its strings hold (no set: not a section's or a
    /// jumper's end).
    pub wire: String,
    /// Where the clamp's middle is: fraction of the jumper's length from
    /// `from`, 0–1.
    pub at: f64,
    /// Length of the clamp, m (default [`DEFAULT_CLAMP_LENGTH`]).
    #[serde(default = "default_clamp_length")]
    pub clamp_length: f64,
}

impl JumperClamp {
    /// A clamp at `at` on wire point `wire` of the `from` end's structure.
    pub fn new(wire: impl Into<String>, at: f64) -> Self {
        JumperClamp {
            label: String::new(),
            structure: None,
            wire: wire.into(),
            at,
            clamp_length: DEFAULT_CLAMP_LENGTH,
        }
    }

    /// The structure holding the clamp: its own, else that of `jumper`'s
    /// `from` end.
    pub fn structure_id<'a>(&'a self, jumper: &'a Jumper) -> &'a str {
        self.structure
            .as_deref()
            .or_else(|| jumper.from.structure())
            .unwrap_or("")
    }
}

/// The strings holding a jumper's support clamp: the clamp's wire point
/// and its strings (one: an I; two: a V), each from a structure point or
/// from another string's wire point (a string below a string: its top is
/// where that wire point is solved, [`chained_top`]). An error says what
/// is wrong with the clamp's wire point.
pub fn clamp_strings<'a>(
    structure: &'a Structure,
    wire: &str,
) -> Result<(&'a WirePoint, Vec<&'a InsulatorString>), String> {
    let id = &structure.id;
    let w = structure
        .wire(wire)
        .ok_or_else(|| format!("no wire point \"{wire}\" on structure {id}"))?;
    if let Some(set) = w.set {
        return Err(format!(
            "wire point {wire} on structure {id} has set {set}: a support clamp hangs on a wire point without a set"
        ));
    }
    let strings: Vec<&InsulatorString> = structure
        .strings
        .iter()
        .filter(|s| s.wire == wire)
        .collect();
    if strings.is_empty() {
        return Err(format!(
            "no string holds wire point {wire} on structure {id}"
        ));
    }
    if strings.len() > 2 {
        return Err(format!(
            "{} strings hold wire point {wire} on structure {id}: a support clamp hangs on one string (I) or two (V)",
            strings.len()
        ));
    }
    // A top the structure does not have (yet): a typed structure as
    // stored, its type giving the point (checked once merged, where the
    // strings are checked in full).
    let from_type = |s: &InsulatorString| {
        structure.structure_type.is_some() && structure.node(&s.top).is_none()
    };
    if let Some(s) = strings.iter().find(|s| {
        structure.point(&s.top).is_none() && structure.wire(&s.top).is_none() && !from_type(s)
    }) {
        return Err(format!(
            "string {} to wire point {wire} on structure {id} hangs from \"{}\": no such point \
             or wire point",
            s.label, s.top
        ));
    }
    if let Some(s) = strings.iter().find(|s| s.top == wire) {
        return Err(format!(
            "string {} on structure {id} hangs from its own wire point {wire}",
            s.label
        ));
    }
    if structure.strings.iter().any(|s| s.top == wire) {
        return Err(format!(
            "strings hang from wire point {wire} on structure {id}: a support clamp's wire point holds nothing else"
        ));
    }
    Ok((w, strings))
}

/// The wire point with a set a string hangs from, when it hangs below
/// another string at a wire point where a section is clamped (a jumper's
/// helping string under a conductor's suspension clamp): that section's
/// clamp carries the string's pull. None from a structure point, or from a
/// wire point no section is clamped at.
pub fn chained_top(
    line: &Line,
    structure: &Structure,
    string: &InsulatorString,
) -> Option<SectionPoint> {
    let set = structure.wire(&string.top)?.set?;
    let p = SectionPoint::new(structure.id.clone(), set);
    line.sections
        .iter()
        .any(|s| s.points.contains(&p))
        .then_some(p)
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
    /// Angles in plan around a structure (string directions, the compass)
    /// turn clockwise seen from above; this is the angle that means ahead
    /// along the line, degrees (default 0: ahead 0°, right 90°; e.g. 270:
    /// right 0°, ahead 270°).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub ahead_deg: f64,
}

impl Display {
    fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

pub(crate) fn is_hex_color(text: &str) -> bool {
    text.len() == 7 && text.starts_with('#') && text[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Checks a section's stringing condition for its `spans`: tensions,
/// sags and lengths above 0, temperatures above absolute zero, as many
/// lengths as spans, a sagged span among them.
fn check_stringing(stringing: &Stringing, spans: usize) -> Result<(), String> {
    let t = stringing.temperature();
    if !(t.is_finite() && t > -273.15) {
        return Err(format!("stringing temperature {t} °C is not a temperature"));
    }
    match stringing {
        Stringing::Tension {
            horizontal_tension, ..
        } => {
            if !(horizontal_tension.is_finite() && *horizontal_tension > 0.0) {
                return Err("the stringing tension must be above 0".to_owned());
            }
        }
        Stringing::Sag { sag, span, .. } => {
            if !(sag.is_finite() && *sag > 0.0) {
                return Err("sag must be above 0".to_owned());
            }
            if let Some(k) = span
                && *k >= spans
            {
                return Err(format!("sag in span {} of {spans}", k + 1));
            }
        }
        Stringing::UnstressedLengths {
            lengths, sagged, ..
        } => {
            if lengths.len() != spans {
                return Err(format!("{} lengths for {spans} spans", lengths.len()));
            }
            if lengths.iter().any(|l| !(l.is_finite() && *l > 0.0)) {
                return Err("unstressed lengths must be above 0".to_owned());
            }
            if let Some(s) = sagged {
                check_stringing(s, spans).map_err(|m| format!("sagged before: {m}"))?;
            }
        }
    }
    Ok(())
}

/// A structure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "StructureFile")]
pub struct Structure {
    /// Unique id, e.g. `"T12"`.
    pub id: String,
    /// Structure origin (centre at ground), metres.
    pub position: Point,
    /// Orientation of the structure's local x axis (to the right of the line
    /// looking ahead), degrees counter-clockwise from east. Used to place a
    /// structure type.
    pub orientation_deg: f64,
    /// Structure type id from the structure library (`.olss`); its points,
    /// strings and wire points are generated by [`Line::resolve`].
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub structure_type: Option<String>,
    /// Height extension of the structure type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension: Option<String>,
    /// What it is, in words (free text).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Typed structures: how far the type's feet stand above the ground at
    /// `position` (foundations), metres; may be negative.
    #[serde(skip_serializing_if = "is_zero")]
    pub height_adjust: f64,
    /// Points on the structure where strings hang ([`assembly`]); for typed
    /// structures they replace the type's with the same label.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<StructurePoint>,
    /// Strings; for typed structures they replace the type's with the same
    /// label.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub strings: Vec<InsulatorString>,
    /// Wire points; for typed structures they replace the type's with the
    /// same label, and a wire point with a set replaces the type's with that
    /// set (and the strings only it hangs on).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub wires: Vec<WirePoint>,
    /// Typed structures: points, strings or wire points of the type left
    /// out, by label.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub omit: Vec<String>,
    /// Typed structures: the type's wire points with these sets left out,
    /// with the strings only they hang on.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub omit_sets: Vec<u32>,
    /// Stick drawing (polylines, metres): from the structure type when it
    /// is placed, or given; for views and drawings only.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<Vec<Point>>,
    /// The routes it is on, given: needed for a structure away from every
    /// alignment (a crossing line); empty: those of the alignments it
    /// stands on ([`Line::structure_routes`]).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<String>,
    /// Values of columns added by plugins, by `plugin.column`
    /// ([`Fields`]).
    #[serde(default, skip_serializing_if = "Fields::is_empty")]
    pub fields: Fields,
}

/// A structure as files give it: the current fields, or (version 1)
/// `attachments` and `omit` as `[set, phase]`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StructureFile {
    id: String,
    position: Point,
    #[serde(default)]
    orientation_deg: f64,
    #[serde(default, rename = "type")]
    structure_type: Option<String>,
    #[serde(default)]
    extension: Option<String>,
    #[serde(default)]
    description: String,
    #[serde(default)]
    height_adjust: f64,
    #[serde(default)]
    points: Vec<StructurePoint>,
    #[serde(default)]
    strings: Vec<InsulatorString>,
    #[serde(default)]
    wires: Vec<WirePoint>,
    #[serde(default)]
    attachments: Vec<AttachmentV1>,
    #[serde(default)]
    omit: Vec<OmitFile>,
    #[serde(default)]
    omit_sets: Vec<u32>,
    #[serde(default)]
    members: Vec<Vec<Point>>,
    #[serde(default)]
    routes: Vec<String>,
    #[serde(default)]
    fields: Fields,
}

/// An `omit` entry: a label, or (version 1) `[set, phase]`.
#[derive(Deserialize)]
#[serde(untagged)]
enum OmitFile {
    Label(String),
    SetPhase([u32; 2]),
}

impl From<StructureFile> for Structure {
    fn from(f: StructureFile) -> Self {
        let (mut points, mut strings, mut wires) = (f.points, f.strings, f.wires);
        if !f.attachments.is_empty() {
            let (p, s, w) = assembly::from_v1(&f.attachments);
            points.extend(p);
            strings.extend(s);
            wires.extend(w);
        }
        let mut omit = Vec::new();
        let mut omit_sets = f.omit_sets;
        for o in f.omit {
            match o {
                OmitFile::Label(l) => omit.push(l),
                OmitFile::SetPhase([set, phase]) => {
                    omit_sets.push(assembly::set_from_v1(set, phase))
                }
            }
        }
        Structure {
            id: f.id,
            position: f.position,
            orientation_deg: f.orientation_deg,
            structure_type: f.structure_type,
            extension: f.extension,
            description: f.description,
            height_adjust: f.height_adjust,
            points,
            strings,
            wires,
            omit,
            omit_sets,
            members: f.members,
            routes: f.routes,
            fields: f.fields,
        }
    }
}

/// One end of a section's span, or of a jumper: the wire point with set
/// `set` on a structure.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(from = "SectionPointFile")]
pub struct SectionPoint {
    /// Structure id.
    pub structure: String,
    /// Set number of the wire point.
    pub set: u32,
}

/// A section point as files give it (version 1 adds a phase:
/// [`assembly::set_from_v1`]).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SectionPointFile {
    structure: String,
    set: u32,
    #[serde(default)]
    phase: Option<u32>,
}

impl From<SectionPointFile> for SectionPoint {
    fn from(f: SectionPointFile) -> Self {
        SectionPoint {
            structure: f.structure,
            set: f.phase.map_or(f.set, |p| assembly::set_from_v1(f.set, p)),
        }
    }
}

impl SectionPoint {
    /// The point at set `set` of structure `structure`.
    pub fn new(structure: impl Into<String>, set: u32) -> Self {
        Self {
            structure: structure.into(),
            set,
        }
    }
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

/// A place along a jumper, from its `from` end: a fraction of its length,
/// a length along it (m), or a horizontal distance along the line (m,
/// from `from`'s clamp). Written `{"fraction": 0.5}`, `{"length": 3}` or
/// `{"distance": 2.5}`; files before version 5 could give several, read
/// by their priority (a distance over a length over the fraction).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", from = "JumperPlaceFile")]
pub enum JumperPlace {
    /// Fraction of its length from `from`, 0–1.
    Fraction(f64),
    /// Length along it from `from`, m.
    Length(f64),
    /// Horizontal distance from `from`'s clamp along the line, m.
    Distance(f64),
}

impl Default for JumperPlace {
    fn default() -> Self {
        Self::Fraction(0.0)
    }
}

/// A jumper place as files give it (one key, or several before
/// version 5).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JumperPlaceFile {
    #[serde(default)]
    fraction: f64,
    #[serde(default)]
    length: Option<f64>,
    #[serde(default)]
    distance: Option<f64>,
}

impl From<JumperPlaceFile> for JumperPlace {
    fn from(f: JumperPlaceFile) -> Self {
        match (f.distance, f.length) {
            (Some(d), _) => Self::Distance(d),
            (None, Some(l)) => Self::Length(l),
            (None, None) => Self::Fraction(f.fraction),
        }
    }
}

impl JumperPlace {
    /// The place given by a fraction, a length and a distance as files
    /// before version 5 could: a distance over a length over the
    /// fraction.
    pub fn from_parts(fraction: f64, length: Option<f64>, distance: Option<f64>) -> Self {
        JumperPlaceFile {
            fraction,
            length,
            distance,
        }
        .into()
    }

    /// Its fraction, when given by one (a support's), else 0.
    pub fn share(&self) -> f64 {
        match *self {
            Self::Fraction(f) => f,
            Self::Length(_) | Self::Distance(_) => 0.0,
        }
    }

    /// It holds: a fraction 0–1, a length or a distance ≥ 0.
    pub fn is_valid(&self) -> bool {
        match *self {
            Self::Distance(d) | Self::Length(d) => d.is_finite() && d >= 0.0,
            Self::Fraction(f) => (0.0..=1.0).contains(&f),
        }
    }

    /// Readable: `0.25`, `3 m along` or `2.5 m from the clamp`.
    pub fn text(&self) -> String {
        match self {
            Self::Distance(d) => format!("{d} m from the clamp"),
            Self::Length(l) => format!("{l} m along"),
            Self::Fraction(f) => format!("{f}"),
        }
    }
}

/// A labelled point along a jumper, with its object or bare.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JumperPoint {
    /// Label, unique on the jumper.
    pub label: String,
    /// Where along it.
    pub at: JumperPlace,
    /// What it is, in words.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// The object on it (one per point).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<LineObject>,
}

/// A place along a span: a horizontal fraction of it from its back point,
/// a horizontal distance from that point (m), or a station of the line
/// (m, along the alignment the span's structures stand on). Written
/// `{"fraction": 0.4}`, `{"distance": 30}` or `{"station": 1520}`; files
/// before version 5 could give several, read by their priority (a station
/// over a distance over the fraction).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", from = "SpanPlaceFile")]
pub enum SpanPlace {
    /// Horizontal fraction of the span from its back point, 0–1.
    Fraction(f64),
    /// Horizontal distance from the back point, m.
    Distance(f64),
    /// Station of the line, m ([`Line::span_stations`]).
    Station(f64),
}

impl Default for SpanPlace {
    fn default() -> Self {
        Self::Fraction(0.0)
    }
}

/// A span place as files give it (one key, or several before version 5).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SpanPlaceFile {
    #[serde(default)]
    fraction: f64,
    #[serde(default)]
    distance: Option<f64>,
    #[serde(default)]
    station: Option<f64>,
}

impl From<SpanPlaceFile> for SpanPlace {
    fn from(f: SpanPlaceFile) -> Self {
        match (f.station, f.distance) {
            (Some(s), _) => Self::Station(s),
            (None, Some(d)) => Self::Distance(d),
            (None, None) => Self::Fraction(f.fraction),
        }
    }
}

impl SpanPlace {
    /// At `fraction` of the span.
    pub fn at_fraction(fraction: f64) -> Self {
        Self::Fraction(fraction)
    }

    /// The place given by a fraction, a distance and a station as files
    /// before version 5 could: a station over a distance over the
    /// fraction.
    pub fn from_parts(fraction: f64, distance: Option<f64>, station: Option<f64>) -> Self {
        SpanPlaceFile {
            fraction,
            distance,
            station,
        }
        .into()
    }

    /// At `distance` m from the back point.
    pub fn at_distance(distance: f64) -> Self {
        Self::Distance(distance)
    }

    /// At station `station` m of the line.
    pub fn at_station(station: f64) -> Self {
        Self::Station(station)
    }

    /// The fraction, when given by one.
    pub fn fraction(&self) -> Option<f64> {
        match self {
            Self::Fraction(f) => Some(*f),
            _ => None,
        }
    }

    /// The distance, when given by one.
    pub fn distance(&self) -> Option<f64> {
        match self {
            Self::Distance(d) => Some(*d),
            _ => None,
        }
    }

    /// The station, when given by one.
    pub fn station(&self) -> Option<f64> {
        match self {
            Self::Station(s) => Some(*s),
            _ => None,
        }
    }

    /// Its fraction of a span `horizontal` m long (a station: turned into
    /// a fraction first, [`Line::section_with_fractions`]; 0 here).
    pub fn fraction_of(&self, horizontal: f64) -> f64 {
        match *self {
            Self::Fraction(f) => f,
            Self::Distance(d) if horizontal > 0.0 => d / horizontal,
            Self::Distance(_) | Self::Station(_) => 0.0,
        }
    }

    /// Readable: `0.25`, `120 m` or `st. 1520 m`.
    pub fn text(&self) -> String {
        match self {
            Self::Station(s) => format!("st. {s} m"),
            Self::Distance(d) => format!("{d} m"),
            Self::Fraction(f) => format!("{f}"),
        }
    }
}

/// A point on a span of a section: where a spacer (or later another
/// string) is clamped on the conductor ([`Spacer`]).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpanPoint {
    /// Label, unique on the section.
    pub label: String,
    /// Span index (0 = first span of the section).
    pub span: usize,
    /// Where along the span.
    pub at: SpanPlace,
    /// What it is, in words.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// The object on it (one per point): a body or a clamp; none: a bare
    /// point (the end of a spread object, a spacer's end).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<LineObject>,
}

/// A span point of a section, by the section's id and the point's label.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpanPointRef {
    /// Section id.
    pub section: String,
    /// The span point's label.
    pub point: String,
}

/// An interphase spacer: an insulator string between span points of two
/// conductors of the same span, keeping them apart. Fitted after
/// stringing; the sections it joins are solved together, by finite
/// elements only (not the ruling span).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spacer {
    /// Unique id.
    pub id: String,
    /// What it is, in words.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// One end.
    pub a: SpanPointRef,
    /// The other end.
    pub b: SpanPointRef,
    /// Insulator library label (`.olsi`): its weight, wind area, stiffness.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insulator: Option<String>,
    /// Its length, m (none: the distance between its ends where the
    /// wires hang at stringing).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<f64>,
    /// Weight, N (half at each end).
    #[serde(default)]
    pub weight: f64,
    /// Wind area, m².
    #[serde(default)]
    pub wind_area: f64,
    /// Axial stiffness EA, N (none: the insulator's, else the default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stiffness: Option<f64>,
    /// Compression it may take, N (none: tension only); beyond, flagged.
    /// New spacers take the project's default ([`Defaults`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compression: Option<f64>,
    /// Values of columns added by plugins, by `plugin.column`.
    #[serde(default, skip_serializing_if = "Fields::is_empty")]
    pub fields: Fields,
}

/// The default spacer compression limit, N.
pub const DEFAULT_SPACER_COMPRESSION: f64 = 10_000.0;

/// The project's default values (Criteria › Default values): what new
/// objects start with and what the calculations take where an object, a
/// cable, a case or a rule file gives no value of its own; exported and
/// imported as a file ([`DEFAULTS_FILE_FORMAT`]). Every value is listed
/// in [`DEFAULT_FIELDS`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Defaults {
    /// A new spacer's compression limit, N.
    pub spacer_compression: f64,
    /// The angle a jumper leaves its clamps at, from its tension string,
    /// turned towards the load, °.
    pub jumper_exit_angle_deg: f64,
    /// A jumper's estimated bending stiffness: this share of its cable's
    /// solid-bar stiffness, 0–1.
    pub jumper_stiffness_factor: f64,
    /// A new jumper support clamp's length, m.
    pub jumper_clamp_length: f64,
    /// An insulator string's (or spacer's) axial stiffness EA, N.
    pub insulator_stiffness: f64,
    /// A weather case's air density factor Q (P = Q·V²), Pa/(m/s)².
    pub air_density_factor: f64,
    /// A conductor's emissivity (thermal rating).
    pub thermal_emissivity: f64,
    /// A conductor's solar absorptivity.
    pub thermal_absorptivity: f64,
    /// A conductor's surface roughness R_s (CIGRE).
    pub thermal_roughness: f64,
    /// The ground's albedo (CIGRE solar heating).
    pub thermal_albedo: f64,
    /// The time step of thermal transients, s.
    pub thermal_time_step: f64,
    /// Clearance windows: the step of the wind pressure, Pa.
    pub clearance_wind_step: f64,
    /// Clearances to points: the horizontal band of vertical clearances, m.
    pub clearance_vertical_band: f64,
    /// Clearances: the search margin around the wires, m.
    pub clearance_search_margin: f64,
    /// Vertical clearance zones: the half width across the line, m.
    pub clearance_zone_width: f64,
    /// Vertical clearance zones: the depth of a slice along the line, m.
    pub clearance_zone_depth: f64,
}

impl Defaults {
    /// The program's values (the one place they are given; `ols_thermal`
    /// and `ols_clearance` take theirs from here).
    pub const PROGRAM: Self = Self {
        spacer_compression: DEFAULT_SPACER_COMPRESSION,
        jumper_exit_angle_deg: jumper::DEFAULT_TERMINAL_ANGLE_DEG,
        jumper_stiffness_factor: jumper::DEFAULT_STIFFNESS_FACTOR,
        jumper_clamp_length: DEFAULT_CLAMP_LENGTH,
        insulator_stiffness: solve::DEFAULT_INSULATOR_STIFFNESS,
        air_density_factor: ols_weather::DEFAULT_AIR_DENSITY_FACTOR,
        thermal_emissivity: 0.8,
        thermal_absorptivity: 0.8,
        thermal_roughness: 0.1,
        thermal_albedo: 0.1,
        thermal_time_step: 10.0,
        clearance_wind_step: 50.0,
        clearance_vertical_band: 0.5,
        clearance_search_margin: 5.0,
        clearance_zone_width: 1.0,
        clearance_zone_depth: 0.2,
    };
}

impl Default for Defaults {
    fn default() -> Self {
        Self::PROGRAM
    }
}

/// One of the project's default values: where it is in the file, its
/// unit, its range, in words; read and written through `get` / `set`.
#[derive(Clone, Copy)]
pub struct DefaultField {
    /// Its section of the file (`[jumper]`).
    pub section: &'static str,
    /// Its key there.
    pub key: &'static str,
    /// Its unit in the file and the window (empty: a number).
    pub unit: &'static str,
    /// Lowest value; `low_open`: the lowest itself not allowed.
    pub low: f64,
    /// The lowest value is not allowed.
    pub low_open: bool,
    /// Highest value (allowed).
    pub high: f64,
    /// Title in the window.
    pub title: &'static str,
    /// What it is.
    pub help: &'static str,
    /// Reads it.
    pub get: fn(&Defaults) -> f64,
    /// Sets it.
    pub set: fn(&mut Defaults, f64),
}

impl std::fmt::Debug for DefaultField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.section, self.key)
    }
}

macro_rules! default_field {
    ($section:literal, $key:literal, $field:ident, $unit:literal, $low:expr, $open:expr, $high:expr, $title:literal, $help:literal) => {
        DefaultField {
            section: $section,
            key: $key,
            unit: $unit,
            low: $low,
            low_open: $open,
            high: $high,
            title: $title,
            help: $help,
            get: |d| d.$field,
            set: |d, v| d.$field = v,
        }
    };
}

/// Every default value of a project, in the order of the file and the
/// window.
pub const DEFAULT_FIELDS: &[DefaultField] = &[
    default_field!(
        "spacer",
        "compression",
        spacer_compression,
        "N",
        0.0,
        false,
        f64::INFINITY,
        "spacer compression limit",
        "The compression a new spacer may take; beyond, it is flagged"
    ),
    default_field!(
        "jumper",
        "exit_angle",
        jumper_exit_angle_deg,
        "°",
        0.0,
        false,
        90.0,
        "jumper exit angle",
        "The angle a jumper leaves its clamps at, from its tension string, turned towards the load (a jumper giving none)"
    ),
    default_field!(
        "jumper",
        "stiffness_factor",
        jumper_stiffness_factor,
        "",
        0.0,
        true,
        1.0,
        "jumper stiffness factor",
        "A jumper's estimated bending stiffness: this share of its cable's solid-bar stiffness (a jumper giving no EI nor factor)"
    ),
    default_field!(
        "jumper",
        "clamp_length",
        jumper_clamp_length,
        "m",
        0.0,
        true,
        1.0,
        "jumper support clamp length",
        "The length a new support clamp of a jumper starts with"
    ),
    default_field!(
        "insulator",
        "stiffness",
        insulator_stiffness,
        "N",
        0.0,
        true,
        f64::INFINITY,
        "insulator stiffness EA",
        "Axial stiffness of a string or a spacer giving none (nor its insulator)"
    ),
    default_field!(
        "weather",
        "air_density_factor",
        air_density_factor,
        "",
        0.0,
        true,
        10.0,
        "air density factor Q",
        "P = Q·V², Pa/(m/s)², for a weather case giving none: its wind velocity from its pressure"
    ),
    default_field!(
        "thermal",
        "emissivity",
        thermal_emissivity,
        "",
        0.0,
        false,
        1.0,
        "emissivity",
        "A conductor's emissivity, when neither its cable nor the ambient case gives one"
    ),
    default_field!(
        "thermal",
        "absorptivity",
        thermal_absorptivity,
        "",
        0.0,
        false,
        1.0,
        "solar absorptivity",
        "A conductor's solar absorptivity, when neither its cable nor the ambient case gives one"
    ),
    default_field!(
        "thermal",
        "roughness",
        thermal_roughness,
        "",
        0.0,
        false,
        10.0,
        "surface roughness",
        "A conductor's surface roughness R_s (CIGRE), when the ambient case gives none"
    ),
    default_field!(
        "thermal",
        "albedo",
        thermal_albedo,
        "",
        0.0,
        false,
        1.0,
        "ground albedo",
        "The ground's albedo (CIGRE solar heating), when the ambient case gives none"
    ),
    default_field!(
        "thermal",
        "time_step",
        thermal_time_step,
        "s",
        0.0,
        true,
        3600.0,
        "transient time step",
        "Time step of thermal transients"
    ),
    default_field!(
        "clearance",
        "wind_step",
        clearance_wind_step,
        "Pa",
        0.0,
        true,
        10_000.0,
        "wind pressure step",
        "Clearance windows, stepped wind: the step of the wind pressure, when the rule file's [wind] gives none"
    ),
    default_field!(
        "clearance",
        "vertical_band",
        clearance_vertical_band,
        "m",
        0.0,
        false,
        1000.0,
        "vertical band",
        "Vertical clearances to points: the horizontal band around the wire, when a rule gives none"
    ),
    default_field!(
        "clearance",
        "search_margin",
        clearance_search_margin,
        "m",
        0.0,
        false,
        1000.0,
        "search margin",
        "Clearances: the margin around the wires the targets are searched in, when a rule gives none"
    ),
    default_field!(
        "clearance",
        "zone_width",
        clearance_zone_width,
        "m",
        0.0,
        true,
        1000.0,
        "zone half width",
        "Vertical clearance windows: the zone's half width across the line a new window starts with"
    ),
    default_field!(
        "clearance",
        "zone_depth",
        clearance_zone_depth,
        "m",
        0.0,
        true,
        1000.0,
        "zone slice depth",
        "Vertical clearance windows: the depth of a slice along the line a new window starts with"
    ),
];

/// File format id of exported default values.
pub const DEFAULTS_FILE_FORMAT: &str = "ols-defaults";
/// Newest version read.
pub const DEFAULTS_FILE_VERSION: u32 = 1;
/// File extension of exported default values.
pub const DEFAULTS_FILE_EXTENSION: &str = "olsdef";

impl Defaults {
    /// Nothing changed from the program's.
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// As a file (TOML): every value, by section, in its unit, with what
    /// it is.
    pub fn to_text(&self) -> String {
        use std::fmt::Write as _;
        let mut out = format!(
            "# Default values of an OLS CAD project (Criteria › Default values).\n\
             format = \"{DEFAULTS_FILE_FORMAT}\"\nversion = {DEFAULTS_FILE_VERSION}\n"
        );
        let mut section = "";
        for f in DEFAULT_FIELDS {
            if f.section != section {
                section = f.section;
                let _ = write!(out, "\n[{section}]\n");
            }
            let v = (f.get)(self);
            let _ = writeln!(out, "# {}.", f.help);
            if f.unit.is_empty() {
                let _ = writeln!(out, "{} = {v:?}", f.key);
            } else {
                let _ = writeln!(out, "{} = \"{v} {}\"", f.key, f.unit);
            }
        }
        out
    }

    /// Reads a file; values not given keep the program's defaults.
    pub fn parse(text: &str) -> Result<Self, String> {
        let table: toml::Table = text.parse().map_err(|e: toml::de::Error| e.to_string())?;
        ols_core::files::check_header(
            table.get("format").and_then(|v| v.as_str()),
            table
                .get("version")
                .and_then(toml::Value::as_integer)
                .map(|v| v as u32),
            DEFAULTS_FILE_FORMAT,
            DEFAULTS_FILE_VERSION,
            "default values",
        )?;
        let mut d = Self::default();
        for (key, value) in &table {
            if key == "format" || key == "version" {
                continue;
            }
            let Some(section) = value.as_table() else {
                return Err(format!("unknown key \"{key}\""));
            };
            for (k, v) in section {
                let f = DEFAULT_FIELDS
                    .iter()
                    .find(|f| f.section == key && f.key == k)
                    .ok_or_else(|| format!("unknown default value [{key}] {k}"))?;
                let what = format!("[{key}] {k}");
                let number = match v {
                    toml::Value::Float(x) => *x,
                    toml::Value::Integer(i) => *i as f64,
                    toml::Value::String(s) => {
                        use ols_core::units::{Force, Length, Stress};
                        let as_unit = |v: &toml::Value| -> Option<f64> {
                            match f.unit {
                                "N" => v.clone().try_into::<Force>().ok().map(|x| x.si()),
                                "m" => v.clone().try_into::<Length>().ok().map(|x| x.si()),
                                "Pa" => v.clone().try_into::<Stress>().ok().map(|x| x.si()),
                                _ => None,
                            }
                        };
                        let t = s.trim();
                        let plain = if f.unit.is_empty() {
                            t
                        } else {
                            t.strip_suffix(f.unit).unwrap_or(t).trim_end()
                        };
                        match plain.parse::<f64>() {
                            Ok(x) => x,
                            Err(_) => as_unit(v).ok_or_else(|| {
                                format!("{what}: \"{s}\" is not a number of {}", f.unit)
                            })?,
                        }
                    }
                    _ => return Err(format!("{what}: give a number")),
                };
                (f.set)(&mut d, number);
            }
        }
        d.validate()?;
        Ok(d)
    }

    /// The values hold (each in its range).
    pub fn validate(&self) -> Result<(), String> {
        for f in DEFAULT_FIELDS {
            let v = (f.get)(self);
            let low_ok = if f.low_open { v > f.low } else { v >= f.low };
            if !(v.is_finite() && low_ok && v <= f.high) {
                let low = if f.low_open { ">" } else { "≥" };
                let high = if f.high.is_finite() {
                    format!(" and ≤ {}", f.high)
                } else {
                    String::new()
                };
                return Err(format!("the {} must be {low} {}{high}", f.title, f.low));
            }
        }
        Ok(())
    }
}

pub(crate) fn is_zero(x: &f64) -> bool {
    *x == 0.0
}

/// `prefix` and the first number not a label of `taken`.
fn free_label(taken: &[SpanPoint], prefix: &str) -> String {
    (1..)
        .map(|k| format!("{prefix}{k}"))
        .find(|l| !taken.iter().any(|p| &p.label == l))
        .expect("a free label")
}

/// A label `{prefix}1`, `{prefix}2`… not taken by `taken` (a jumper's
/// points).
fn free_point_label(taken: &[JumperPoint], prefix: &str) -> String {
    (1..)
        .map(|k| format!("{prefix}{k}"))
        .find(|l| !taken.iter().any(|p| &p.label == l))
        .expect("a free label")
}

impl Section {
    /// Adds point load `l` (as older files gave it): a point labelled
    /// `L1`, `L2`… holding its object, its label as the point's
    /// description.
    pub fn add_point_load(&mut self, l: &SpanLoad) -> String {
        let label = free_label(&self.span_points, "L");
        self.span_points.push(SpanPoint {
            label: label.clone(),
            span: l.span,
            at: l.place(),
            description: l.label.clone(),
            object: Some(l.object()),
        });
        label
    }

    /// Adds spread load `l` (as older files gave it): two bare points at
    /// its ends and a spread object between them.
    pub fn add_spread_load(&mut self, l: &SpreadLoad) {
        let mut ends = Vec::new();
        for at in [l.start, l.end] {
            let label = free_label(&self.span_points, "S");
            self.span_points.push(SpanPoint {
                label: label.clone(),
                span: l.span,
                at,
                description: String::new(),
                object: None,
            });
            ends.push(label);
        }
        self.spreads.push(SpreadObject {
            label: l.label.clone(),
            from: ends[0].clone(),
            to: ends[1].clone(),
            type_id: None,
            load: l.load,
            per_subconductor: false,
        });
    }

    /// Its points' objects, to change them.
    pub fn objects_mut(&mut self) -> impl Iterator<Item = &mut LineObject> {
        self.span_points
            .iter_mut()
            .filter_map(|p| p.object.as_mut())
    }

    /// Removes its points holding a body (its point loads).
    pub fn clear_point_loads(&mut self) {
        self.span_points.retain(|p| {
            !p.object
                .as_ref()
                .is_some_and(|o| o.kind == ObjectKind::Body)
        });
    }

    /// The points with a body or a clamp: (span, place, object).
    pub fn objects(&self) -> impl Iterator<Item = (&SpanPoint, &LineObject)> {
        self.span_points
            .iter()
            .filter_map(|p| p.object.as_ref().map(|o| (p, o)))
    }

    /// Its spread objects as loads between the places of their points,
    /// each on the whole bundle ([`SpreadObject::bundle_load`]: a load
    /// on each subconductor times the bundle); those whose points are not
    /// on one span left out (the validation refuses them).
    pub fn spread_loads(&self) -> Vec<objects::SpreadStretch> {
        self.spreads
            .iter()
            .enumerate()
            .filter_map(|(index, s)| {
                let p = |l: &str| self.span_points.iter().find(|p| p.label == l);
                let (a, b) = (p(&s.from)?, p(&s.to)?);
                (a.span == b.span).then(|| objects::SpreadStretch {
                    span: a.span,
                    start: a.at,
                    end: b.at,
                    load: s.bundle_load(self.bundle),
                    label: s.label.clone(),
                    index,
                })
            })
            .collect()
    }

    /// The indices of its spread objects starting or ending at point
    /// `label`.
    pub fn spreads_at(&self, label: &str) -> Vec<usize> {
        self.spreads
            .iter()
            .enumerate()
            .filter(|(_, s)| s.from == label || s.to == label)
            .map(|(k, _)| k)
            .collect()
    }
}

/// A wire section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "SectionFile")]
pub struct Section {
    /// What it is, in words (free text).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Unique id.
    pub id: String,
    /// Cable id in the wire libraries (`.olsw`).
    pub cable: String,
    /// The name of the circuit it is part of (all its sections give the
    /// same; the circuits themselves are traced, [`electrical::circuits`]).
    /// Files before version 5: `circuit`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub circuit_name: String,
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
    /// Labelled points on its spans, each with its object (a body, a
    /// clamp) or bare.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub span_points: Vec<SpanPoint>,
    /// Objects spread between two of its points.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spreads: Vec<SpreadObject>,
    /// Display colour (`#rrggbb`), overriding the phase colour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Cable condition and method it is solved with (default Initial FE).
    #[serde(default, skip_serializing_if = "condition::Calculation::is_default")]
    pub calculation: condition::Calculation,
    /// Values of columns added by plugins, by `plugin.column`
    /// ([`Fields`]).
    #[serde(default, skip_serializing_if = "Fields::is_empty")]
    pub fields: Fields,
}

fn one() -> u32 {
    1
}

/// A section as written in files, older ones included: point loads and
/// spread loads of their own (before points and objects) are read into
/// points with objects ([`Section`]).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SectionFile {
    #[serde(default)]
    description: String,
    id: String,
    cable: String,
    #[serde(default, alias = "circuit")]
    circuit_name: String,
    #[serde(default)]
    phase_label: String,
    #[serde(default)]
    system: Option<String>,
    #[serde(default = "one")]
    bundle: u32,
    #[serde(default)]
    bundle_spacing: Option<f64>,
    #[serde(default)]
    bundle_rotation_deg: Option<f64>,
    points: Vec<SectionPoint>,
    stringing: Stringing,
    #[serde(default)]
    loads: Vec<SpanLoad>,
    #[serde(default)]
    spread_loads: Vec<SpreadLoad>,
    #[serde(default)]
    span_points: Vec<SpanPoint>,
    #[serde(default)]
    spreads: Vec<SpreadObject>,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    calculation: condition::Calculation,
    #[serde(default)]
    fields: Fields,
}

impl From<SectionFile> for Section {
    fn from(f: SectionFile) -> Self {
        let mut s = Section {
            description: f.description,
            id: f.id,
            cable: f.cable,
            circuit_name: f.circuit_name,
            phase_label: f.phase_label,
            system: f.system,
            bundle: f.bundle,
            bundle_spacing: f.bundle_spacing,
            bundle_rotation_deg: f.bundle_rotation_deg,
            points: f.points,
            stringing: f.stringing,
            span_points: f.span_points,
            spreads: f.spreads,
            color: f.color,
            calculation: f.calculation,
            fields: f.fields,
        };
        for l in f.loads {
            s.add_point_load(&l);
        }
        for l in f.spread_loads {
            s.add_spread_load(&l);
        }
        s
    }
}

impl Structure {
    /// The structure moved to `position`, with its points, wire points and
    /// stick drawing.
    pub fn moved_to(&self, position: Point) -> Structure {
        let d = [
            position[0] - self.position[0],
            position[1] - self.position[1],
            position[2] - self.position[2],
        ];
        let mut moved = self.clone();
        moved.position = position;
        for p in moved.points_mut() {
            for k in 0..3 {
                p[k] += d[k];
            }
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
    /// Gives it the wire points `points`: its span points (with their
    /// objects, and so what hangs on them), a sagged span and clipped
    /// lengths keep their span — found by its two ends — at its new
    /// index (the one place span indices are moved). Refused, naming
    /// them, when span points are on a span the new points no longer have
    /// (a structure put in it, taken out of it, or the section turned).
    /// Lengths of spans gone are left as they are (to be clipped again).
    pub fn set_points(&mut self, points: Vec<SectionPoint>) -> Result<(), String> {
        let old = &self.points;
        let new_index = |k: usize| -> Option<usize> {
            let (a, b) = (old.get(k)?, old.get(k + 1)?);
            points.windows(2).position(|w| &w[0] == a && &w[1] == b)
        };
        let spans: Vec<Option<usize>> =
            self.span_points.iter().map(|p| new_index(p.span)).collect();
        let lost: Vec<&str> = self
            .span_points
            .iter()
            .zip(&spans)
            .filter(|(_, m)| m.is_none())
            .map(|(p, _)| p.label.as_str())
            .collect();
        if !lost.is_empty() {
            return Err(format!(
                "section {}: span point(s) {} are on a span the new points no longer have \
                 (a structure put in or taken out of it): move or delete them first",
                self.id,
                lost.join(", ")
            ));
        }
        let sagged = match &self.stringing {
            Stringing::Sag { span: Some(k), .. } => new_index(*k),
            _ => None,
        };
        // Clipped: each new span's length from its old span, when every
        // new span was one.
        let lengths: Option<Vec<f64>> = match &self.stringing {
            Stringing::UnstressedLengths { lengths, .. } => points
                .windows(2)
                .map(|w| {
                    old.windows(2)
                        .position(|o| o[0] == w[0] && o[1] == w[1])
                        .and_then(|k| lengths.get(k).copied())
                })
                .collect(),
            _ => None,
        };
        for (p, m) in self.span_points.iter_mut().zip(spans) {
            p.span = m.expect("checked");
        }
        if let (Stringing::Sag { span: Some(k), .. }, Some(m)) = (&mut self.stringing, sagged) {
            *k = m;
        }
        if let (Stringing::UnstressedLengths { lengths, .. }, Some(new)) =
            (&mut self.stringing, lengths)
        {
            *lengths = new;
        }
        self.points = points;
        Ok(())
    }

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
    /// `s / (2·sin(π/n))`; by default the two highest subconductors are
    /// level. One entry `[0, 0]` for a single wire or a bundle without a
    /// spacing (geometry as one wire).
    pub fn offsets(&self) -> Vec<[f64; 2]> {
        let n = self.count.max(1);
        let Some(s) = self.spacing.filter(|_| n >= 2) else {
            return vec![[0.0, 0.0]];
        };
        let r = s / (2.0 * (std::f64::consts::PI / f64::from(n)).sin());
        // One at the bottom for an odd count (a triple bundle: two side by
        // side above one), a flat bottom for an even one.
        let odd = n % 2 == 1;
        let start = -std::f64::consts::FRAC_PI_2
            + if odd {
                0.0
            } else {
                std::f64::consts::PI / f64::from(n)
            }
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
    /// circuit): the first section through its `from` point — or, `from`
    /// a tap, the section it hangs on — else through its `to` end.
    pub fn jumper_section(&self, jumper: &Jumper) -> Option<&Section> {
        self.end_section(&jumper.from, 0)
            .or_else(|| self.end_section(&jumper.to, 0))
    }

    /// The section at an end of a jumper: through its section point, or
    /// that a tap hangs on (a jumper's: that jumper's section, at most
    /// `depth` 16 jumpers deep).
    fn end_section(&self, end: &JumperEnd, depth: usize) -> Option<&Section> {
        match end {
            JumperEnd::Wire(point) => self.sections.iter().find(|s| s.points.contains(point)),
            JumperEnd::Tap { tap } => self.tap_section(tap, depth),
        }
    }

    /// The section a tap hangs on: its span's, or that of the jumper it
    /// is on (through that jumper's ends, at most 16 jumpers deep).
    pub fn tap_section(&self, tap: &Tap, depth: usize) -> Option<&Section> {
        match tap {
            Tap::Span(r) => self.sections.iter().find(|s| s.id == r.section),
            Tap::Jumper { jumper, .. } if depth < 16 => {
                let host = self.jumpers.iter().find(|j| &j.id == jumper)?;
                self.end_section(&host.from, depth + 1)
                    .or_else(|| self.end_section(&host.to, depth + 1))
            }
            Tap::Jumper { .. } => None,
        }
    }

    /// The structures a jumper's end is at: its section point's, or, a
    /// tap, those of the span (or section) it hangs on.
    pub fn end_structures(&self, end: &JumperEnd) -> Vec<String> {
        match end.as_tap() {
            None => end.structure().map(str::to_owned).into_iter().collect(),
            Some(Tap::Span(r)) => self
                .sections
                .iter()
                .find(|s| s.id == r.section)
                .and_then(|s| {
                    let p = s.span_points.iter().find(|p| p.label == r.point)?;
                    Some(
                        s.points[p.span..=(p.span + 1).min(s.points.len() - 1)]
                            .iter()
                            .map(|q| q.structure.clone())
                            .collect(),
                    )
                })
                .unwrap_or_default(),
            Some(t) => self
                .tap_section(t, 0)
                .map(|s| s.points.iter().map(|q| q.structure.clone()).collect())
                .unwrap_or_default(),
        }
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
            points.extend(s.points.iter().map(|p| p.position));
            points.extend(s.wires.iter().map(|w| w.position));
            points.extend(s.members.iter().flatten());
        }
        for r in &self.routes {
            points.extend(r.legacy_alignment.iter().map(|a| a.position));
        }
        points.extend(self.alignment_points.iter().map(|a| a.position));
        points.extend(self.alignment.iter().map(|a| a.position));
        for j in &self.jumpers {
            points.extend(
                j.points
                    .iter()
                    .filter_map(|p| match p.object.as_ref()?.hold {
                        Some(objects::Hold::String { point, .. }) => Some(point),
                        _ => None,
                    }),
            );
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
            for q in s.points_mut() {
                *q = next();
            }
        }
        for r in &mut self.routes {
            for a in &mut r.legacy_alignment {
                a.position = next();
            }
        }
        for a in &mut self.alignment_points {
            a.position = next();
        }
        for a in &mut self.alignment {
            a.position = next();
        }
        for j in &mut self.jumpers {
            for p in &mut j.points {
                if let Some(objects::Hold::String { point, .. }) =
                    p.object.as_mut().and_then(|o| o.hold.as_mut())
                {
                    *point = next();
                }
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

    /// Every structure's strings: their length and direction (taken from
    /// where their wire points are, for strings without a length: files
    /// written before), their wire points placed from them
    /// ([`Structure::place_wires`]).
    pub fn place_strings(&mut self) -> Result<(), LineError> {
        for s in &mut self.structures {
            s.take_string_geometry();
            s.place_wires().map_err(LineError::Invalid)?;
        }
        Ok(())
    }

    /// Parses and validates JSON text. Files of an older version are read
    /// into the current one (and say so).
    pub fn from_json(text: &str) -> Result<Self, LineError> {
        let mut line: Self =
            serde_json::from_str(text).map_err(|e| LineError::File(e.to_string()))?;
        line.upgrade_routes();
        line.read_older();
        line.place_strings()?;
        line.validate()?;
        line.version = LINE_FILE_VERSION;
        Ok(line)
    }

    /// Relabels point `label` of section `id` (`on_jumper`: of jumper
    /// `id`) to `new_label`; its spreads, the spacers on it and the
    /// jumpers tapped on it follow.
    pub fn relabel_point(
        &mut self,
        on_jumper: bool,
        id: &str,
        label: &str,
        new_label: &str,
    ) -> Result<(), String> {
        let new_label = new_label.trim();
        if new_label.is_empty() || new_label == label {
            return Err(format!("point {label}: give a new label"));
        }
        let host = if on_jumper { "jumper" } else { "section" };
        if on_jumper {
            let j = self
                .jumpers
                .iter_mut()
                .find(|j| j.id == id)
                .ok_or_else(|| format!("no jumper {id}"))?;
            if j.points.iter().any(|p| p.label == new_label) {
                return Err(format!("{host} {id} has a point {new_label} already"));
            }
            j.points
                .iter_mut()
                .find(|p| p.label == label)
                .ok_or_else(|| format!("{host} {id} has no point {label}"))?
                .label = new_label.to_owned();
        } else {
            let s = self
                .sections
                .iter_mut()
                .find(|s| s.id == id)
                .ok_or_else(|| format!("no section {id}"))?;
            if s.span_points.iter().any(|p| p.label == new_label) {
                return Err(format!("{host} {id} has a point {new_label} already"));
            }
            s.span_points
                .iter_mut()
                .find(|p| p.label == label)
                .ok_or_else(|| format!("{host} {id} has no point {label}"))?
                .label = new_label.to_owned();
            for x in &mut s.spreads {
                for end in [&mut x.from, &mut x.to] {
                    if end == label {
                        *end = new_label.to_owned();
                    }
                }
            }
            for x in &mut self.spacers {
                for end in [&mut x.a, &mut x.b] {
                    if end.section == id && end.point == label {
                        end.point = new_label.to_owned();
                    }
                }
            }
        }
        for j in &mut self.jumpers {
            for t in [&mut j.from, &mut j.to]
                .into_iter()
                .filter_map(JumperEnd::tap_mut)
            {
                match t {
                    Tap::Span(r) if !on_jumper && r.section == id && r.point == label => {
                        r.point = new_label.to_owned();
                    }
                    Tap::Jumper { jumper, point }
                        if on_jumper && jumper == id && point == label =>
                    {
                        *point = new_label.to_owned();
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// What files written before need and the format does not say:
    /// stale circuit starts dropped ([`Line::drop_stale_circuit_starts`]),
    /// spacers' bare end points given a clamp
    /// ([`Line::clamp_spacer_ends`]).
    pub fn read_older(&mut self) {
        self.drop_stale_circuit_starts();
        self.clamp_spacer_ends();
    }

    /// Gives a spacer's bare end point a clamp object, of no weight (files
    /// written before a spacer's ends were clamps: nothing changes in the
    /// results).
    pub fn clamp_spacer_ends(&mut self) {
        for s in &self.spacers {
            for end in [&s.a, &s.b] {
                if let Some(p) = self
                    .sections
                    .iter_mut()
                    .find(|x| x.id == end.section)
                    .and_then(|x| x.span_points.iter_mut().find(|p| p.label == end.point))
                    && p.object.is_none()
                {
                    p.object = Some(LineObject::clamp(0.0));
                }
            }
        }
    }

    /// Drops the circuit starts that are no section's point, and those
    /// given twice (files written before the starts followed deletions);
    /// nothing physical depends on them.
    pub fn drop_stale_circuit_starts(&mut self) {
        let mut seen = Vec::new();
        let sections = &self.sections;
        self.circuit_starts.retain(|p| {
            let keep = !seen.contains(p) && sections.iter().any(|s| s.points.contains(p));
            seen.push(p.clone());
            keep
        });
    }

    /// Serialises to pretty JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("line serialises")
    }

    /// Structure by id.
    pub fn structure(&self, id: &str) -> Option<&Structure> {
        self.structures.iter().find(|s| s.id == id)
    }

    /// The limits of string `label` of structure `structure` (not checked
    /// by OLS CAD: for a plugin or a report to compare with the solved
    /// forces and swings). On a resolved line they include those its
    /// insulator gives ([`Line::resolve`]); none: no such string.
    pub fn string_limits(&self, structure: &str, label: &str) -> Option<StringLimits> {
        Some(self.structure(structure)?.string(label)?.limits())
    }

    /// The wire point a section point refers to.
    pub fn wire_point(&self, point: &SectionPoint) -> Option<&WirePoint> {
        self.structure(&point.structure)?.wire_at_set(point.set)
    }

    /// Whether wire point `wire` of structure `structure` is a dead end: a
    /// section ends at its set, or a string to it is flagged `strain` (a
    /// tension string); else its strings are suspension strings.
    pub fn is_dead_end(&self, structure: &Structure, wire: &str) -> bool {
        let ends = structure.wire(wire).and_then(|w| w.set).is_some_and(|set| {
            self.sections.iter().any(|s| {
                [s.points.first(), s.points.last()]
                    .into_iter()
                    .flatten()
                    .any(|p| p.structure == structure.id && p.set == set)
            })
        });
        ends || structure.strings_to(wire).any(|x| x.strain)
    }

    /// Display colour of a section: its own colour, else the line's colour
    /// for its phase label, else a default ([`DEFAULT_PHASE_COLORS`]) by the
    /// place of its phase label among its system's phases, or by the number
    /// its phase label ends with (`L2`), else [`NEUTRAL_COLOR`].
    pub fn section_color(&self, section: &Section) -> String {
        if let Some(c) = &section.color {
            return c.clone();
        }
        if let Some(c) = self.display.phase_colors.get(&section.phase_label) {
            return c.clone();
        }
        let place = section
            .system
            .as_deref()
            .and_then(|id| self.systems.iter().find(|s| s.id == id))
            .and_then(|s| s.phases.iter().position(|p| p.label == section.phase_label))
            .or_else(|| {
                let digits: String = section
                    .phase_label
                    .chars()
                    .rev()
                    .take_while(char::is_ascii_digit)
                    .collect();
                let n: usize = digits.chars().rev().collect::<String>().parse().ok()?;
                n.checked_sub(1)
            });
        match place {
            Some(k) => DEFAULT_PHASE_COLORS[k % DEFAULT_PHASE_COLORS.len()].to_owned(),
            None => NEUTRAL_COLOR.to_owned(),
        }
    }

    /// Tied alignment points must be under an existing structure.
    fn check_alignment(&self, points: &[AlignmentPoint]) -> Result<(), LineError> {
        for a in points {
            let Some(id) = &a.structure else { continue };
            let Some(s) = self.structure(id) else {
                return Err(LineError::Invalid(format!(
                    "alignment point {} is tied to unknown structure {id}",
                    a.id
                )));
            };
            let d = (a.position[0] - s.position[0]).hypot(a.position[1] - s.position[1]);
            if d > 0.001 {
                return Err(LineError::Invalid(format!(
                    "alignment point {} is tied to {id} but {d:.3} m away from it (detach it to move it)",
                    a.id
                )));
            }
        }
        Ok(())
    }

    /// Checks a jumper's support clamps: not with a `support` string, a
    /// cut length given, each clamp inside the jumper and after the one
    /// before (with room for both), its length above 0 and at most 1 m,
    /// its wire point held as [`clamp_strings`] says (a typed structure's
    /// wire points are checked once resolved).
    fn check_clamps(&self, j: &Jumper) -> Result<(), String> {
        // Supports are placed by a fraction of the jumper's length; one
        // string from a fixed point at most.
        for p in &j.points {
            if p.object.as_ref().and_then(|o| o.hold.as_ref()).is_some()
                && !matches!(p.at, JumperPlace::Fraction(_))
            {
                return Err(format!(
                    "support {}: placed by a fraction of the jumper's length",
                    p.label
                ));
            }
        }
        if j.points
            .iter()
            .filter(|p| {
                p.object
                    .as_ref()
                    .and_then(|o| o.hold.as_ref())
                    .is_some_and(|h| matches!(h, objects::Hold::String { .. }))
            })
            .count()
            > 1
        {
            return Err("one support string from a fixed point at most".into());
        }
        let supports = j.supports();
        if supports.is_empty() {
            return Ok(());
        }
        if j.support().is_some() {
            return Err(
                "give a support string (support) or support clamps (supports), not both".into(),
            );
        }
        if j.rigid {
            return Err("a rigid jumper has no support clamps".into());
        }
        let Some(length) = j.length else {
            return Err(
                "a jumper with support clamps needs a length (its sag depends on them)".into(),
            );
        };
        let mut end_before = 0.0;
        for (k, c) in supports.iter().enumerate() {
            let n = k + 1;
            if !(c.clamp_length > 0.0 && c.clamp_length <= 1.0) {
                return Err(format!(
                    "support clamp {n}: its length must be above 0 and at most 1 m"
                ));
            }
            if !(c.at > 0.0 && c.at < 1.0) {
                return Err(format!(
                    "support clamp {n}: at must be between 0 and 1 (a fraction of the length)"
                ));
            }
            let (start, end) = (
                c.at * length - c.clamp_length / 2.0,
                c.at * length + c.clamp_length / 2.0,
            );
            if start <= end_before || end >= length {
                return Err(format!(
                    "support clamp {n} at {:.2} m along the jumper overlaps {} (clamps in order \
                     from `from`, each {:.2} m long)",
                    c.at * length,
                    if end >= length {
                        "the jumper's end".to_owned()
                    } else if k == 0 {
                        "the jumper's start".to_owned()
                    } else {
                        format!("clamp {k}")
                    },
                    c.clamp_length
                ));
            }
            end_before = end;
            let id = c.structure_id(j);
            let Some(structure) = self.structure(id) else {
                return Err(format!("support clamp {n}: unknown structure {id}"));
            };
            if structure.structure_type.is_some() && structure.wire(&c.wire).is_none() {
                // From the structure's type: checked once resolved.
                continue;
            }
            clamp_strings(structure, &c.wire).map_err(|m| format!("support clamp {n}: {m}"))?;
            if supports[..k]
                .iter()
                .any(|d| d.structure_id(j) == id && d.wire == c.wire)
            {
                return Err(format!(
                    "support clamps {n} and another hang on the same wire point {} of {id}",
                    c.wire
                ));
            }
        }
        Ok(())
    }

    /// Checks references and basic consistency.
    pub fn validate(&self) -> Result<(), LineError> {
        let invalid = |m: String| Err(LineError::Invalid(m));
        self.bimetallic.validate().map_err(LineError::Invalid)?;
        ols_core::files::check_header(
            Some(&self.format),
            Some(self.version),
            LINE_FILE_FORMAT,
            LINE_FILE_VERSION,
            "line file",
        )
        .map_err(LineError::Invalid)?;
        if !(0.0..360.0).contains(&self.display.ahead_deg) {
            return invalid(format!(
                "display: the angle ahead {} must be from 0 to 360°",
                self.display.ahead_deg
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
            if !finite(&s.position)
                || !s.orientation_deg.is_finite()
                || !s.height_adjust.is_finite()
            {
                return invalid(format!(
                    "structure {}: position, orientation or height adjustment not a number",
                    s.id
                ));
            }
            s.check_strings_as_stored().map_err(LineError::Invalid)?;
        }
        if self.routes.iter().any(Route::is_legacy) || !self.alignment.is_empty() {
            return invalid(
                "routes of a version 2 file not upgraded (Line::upgrade_routes)".to_owned(),
            );
        }
        // Systems, and one system phase per conductor path.
        self.section_phases()?;
        let mut route_ids = BTreeMap::new();
        for r in &self.routes {
            if r.id.trim().is_empty() || route_ids.insert(r.id.as_str(), ()).is_some() {
                return invalid(format!("route id \"{}\": empty or given twice", r.id));
            }
        }
        let known = |r: &String| route_ids.contains_key(r.as_str());
        let mut point_ids = BTreeMap::new();
        for p in &self.alignment_points {
            if p.id.trim().is_empty() || point_ids.insert(p.id.as_str(), ()).is_some() {
                return invalid(format!(
                    "alignment point id \"{}\": empty or given twice",
                    p.id
                ));
            }
            if !p.position.iter().all(|x| x.is_finite()) {
                return invalid(format!("alignment point {}: not a number", p.id));
            }
        }
        self.check_alignment(&self.alignment_points)?;
        let mut alignment_ids = BTreeMap::new();
        for a in &self.alignments {
            if a.id.trim().is_empty() || alignment_ids.insert(a.id.as_str(), ()).is_some() {
                return invalid(format!("alignment id \"{}\": empty or given twice", a.id));
            }
            if a.legacy.is_some() {
                return invalid(format!(
                    "alignment {}: of a version 3 file, not upgraded (Line::upgrade_routes)",
                    a.id
                ));
            }
            if a.route.trim().is_empty() {
                return invalid(format!("alignment {}: belongs to no route", a.id));
            }
            if !known(&a.route) {
                return invalid(format!("alignment {}: unknown route {}", a.id, a.route));
            }
            let mut seen = BTreeMap::new();
            for p in &a.points {
                if !point_ids.contains_key(p.as_str()) {
                    return invalid(format!("alignment {}: unknown alignment point {p}", a.id));
                }
                if seen.insert(p.as_str(), ()).is_some() {
                    return invalid(format!("alignment {}: point {p} given twice", a.id));
                }
            }
        }
        if let Some((s, r)) = self
            .structures
            .iter()
            .find_map(|s| s.routes.iter().find(|r| !known(r)).map(|r| (s, r)))
        {
            return invalid(format!("structure {}: unknown route {r}", s.id));
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
                // Typed structures get their wire points from the library
                // (`resolve`), which validates again afterwards.
                if structure.structure_type.is_none() && self.wire_point(p).is_none() {
                    return invalid(format!(
                        "section {}: no wire point with set {} on structure {}",
                        sec.id, p.set, p.structure
                    ));
                }
            }
            let spans = sec.points.len() - 1;
            check_stringing(&sec.stringing, spans)
                .map_err(|m| LineError::Invalid(format!("section {}: {m}", sec.id)))?;
            let place_ok = |p: &SpanPlace| match *p {
                SpanPlace::Station(s) => s.is_finite(),
                SpanPlace::Distance(d) => d.is_finite() && d > 0.0,
                SpanPlace::Fraction(f) => 0.0 < f && f < 1.0,
            };
            // A span's ends are places too (a spread object from or to
            // a clamp).
            let end_ok =
                |p: &SpanPlace| matches!(*p, SpanPlace::Fraction(f) if f == 0.0 || f == 1.0);
            let mut labels = std::collections::BTreeSet::new();
            for p in &sec.span_points {
                if p.label.trim().is_empty() || !labels.insert(p.label.as_str()) {
                    return invalid(format!(
                        "section {}: span point labels must be given and unique ({:?})",
                        sec.id, p.label
                    ));
                }
                if p.span >= spans
                    || !(place_ok(&p.at) || (p.object.is_none() && end_ok(&p.at)))
                    || !self.inside_span(sec, p)
                {
                    return invalid(format!(
                        "section {}: span point {} is not inside a span",
                        sec.id, p.label
                    ));
                }
                if let Some(o) = &p.object {
                    o.validate().map_err(|e| {
                        LineError::Invalid(format!(
                            "section {}: the object on {}: {e}",
                            sec.id, p.label
                        ))
                    })?;
                    if o.hold.is_some() {
                        return invalid(format!(
                            "section {}: the clamp on {} is held up: only a jumper's is (a support)",
                            sec.id, p.label
                        ));
                    }
                }
            }
            let mut spread_labels = std::collections::BTreeSet::new();
            for s in &sec.spreads {
                if !s.label.is_empty() && !spread_labels.insert(s.label.as_str()) {
                    return invalid(format!(
                        "section {}: spread label {:?} given twice",
                        sec.id, s.label
                    ));
                }
                let at = |l: &str| sec.span_points.iter().find(|p| p.label == l);
                let ok = match (at(&s.from), at(&s.to)) {
                    (Some(a), Some(b)) => a.span == b.span && s.from != s.to,
                    _ => false,
                };
                if !ok || !(s.load.is_finite() && s.load >= 0.0) {
                    return invalid(format!(
                        "section {}: spread {:?} needs two points of one span and a load ≥ 0",
                        sec.id, s.label
                    ));
                }
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
                let end = match end {
                    JumperEnd::Tap { tap } => {
                        self.check_tap(j, tap)?;
                        continue;
                    }
                    JumperEnd::Wire(p) => p,
                };
                let Some(structure) = self.structure(&end.structure) else {
                    return bad(&format!("unknown structure {}", end.structure));
                };
                if structure.structure_type.is_none() && self.wire_point(end).is_none() {
                    return bad(&format!(
                        "no wire point with set {} on structure {}",
                        end.set, end.structure
                    ));
                }
            }
            if j.from == j.to {
                return bad("both ends are the same wire point or tap");
            }
            if j.on_tap() && j.length.is_none() && !j.rigid {
                return bad(
                    "a jumper on a tap is given by its length (its sag would need a design case where the tap stays put)",
                );
            }
            let mut labels = std::collections::BTreeSet::new();
            for p in &j.points {
                if p.label.trim().is_empty() || !labels.insert(p.label.as_str()) {
                    return bad(&format!(
                        "point labels must be given and unique ({:?})",
                        p.label
                    ));
                }
                if !p.at.is_valid() {
                    return bad(&format!("point {} is not along it", p.label));
                }
                if let Some(o) = &p.object {
                    o.validate().map_err(|e| {
                        LineError::Invalid(format!(
                            "jumper {}: the object on {}: {e}",
                            j.id, p.label
                        ))
                    })?;
                }
            }
            match (j.rigid, j.length, j.sag) {
                (true, None, None) => {}
                (true, ..) => return bad("a rigid jumper has no length or sag"),
                (false, Some(l), None) if l > 0.0 => {}
                (false, None, Some(s)) if s > 0.0 => {}
                _ => return bad("give either a positive length or a positive sag"),
            }
            if j.bending_stiffness
                .is_some_and(|e| !(e >= 0.0 && e.is_finite()))
            {
                return bad("bending stiffness must be ≥ 0");
            }
            if j.stiffness_factor.is_some_and(|f| !(f > 0.0 && f <= 1.0)) {
                return bad("the stiffness factor must be above 0 and at most 1");
            }
            if [j.angle_from_deg, j.angle_to_deg]
                .into_iter()
                .flatten()
                .any(|a| !(-90.0..=90.0).contains(&a))
            {
                return bad("clamp exit angles must be between −90° and 90°");
            }
            if let Some(support) = &j.support() {
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
            self.check_clamps(j)
                .map_err(|m| LineError::Invalid(format!("jumper {}: {m}", j.id)))?;
            if let Some(c) = j.color.as_deref().filter(|c| !is_hex_color(c)) {
                return bad(&format!("colour \"{c}\" is not #rrggbb"));
            }
        }
        self.defaults.validate().map_err(LineError::Invalid)?;
        // Circuit starts: section points, each once.
        let mut starts = std::collections::BTreeSet::new();
        for p in &self.circuit_starts {
            if !starts.insert((p.structure.as_str(), p.set)) {
                return invalid(format!(
                    "circuit start {} set {} given twice",
                    p.structure, p.set
                ));
            }
            if !self.sections.iter().any(|s| s.points.contains(p)) {
                return invalid(format!(
                    "circuit start {} set {} is no section's point",
                    p.structure, p.set
                ));
            }
        }
        let mut ids = std::collections::BTreeSet::new();
        for s in &self.spacers {
            let bad = |m: &str| invalid(format!("spacer {}: {m}", s.id));
            if s.id.trim().is_empty() || !ids.insert(s.id.as_str()) {
                return invalid(format!("spacer ids must be given and unique ({:?})", s.id));
            }
            if s.a.section == s.b.section {
                return bad("its ends must be on two different sections");
            }
            let mut spans = Vec::new();
            for end in [&s.a, &s.b] {
                let Some(sec) = self.sections.iter().find(|x| x.id == end.section) else {
                    return bad(&format!("no section {}", end.section));
                };
                let Some(p) = sec.span_points.iter().find(|p| p.label == end.point) else {
                    return bad(&format!(
                        "no span point {} on section {}",
                        end.point, sec.id
                    ));
                };
                if !p
                    .object
                    .as_ref()
                    .is_some_and(|o| o.kind == ObjectKind::Clamp)
                {
                    return bad(&format!(
                        "its end {} on section {} holds no clamp (put a clamp object there)",
                        end.point, sec.id
                    ));
                }
                if sec.calculation.method() != condition::Method::Fe {
                    return bad(&format!(
                        "section {} is calculated by ruling span: a spacer needs finite \
                         elements (its calculation: … FE)",
                        sec.id
                    ));
                }
                let span = &sec.points[p.span..=p.span + 1];
                let mut ends = [span[0].structure.clone(), span[1].structure.clone()];
                ends.sort();
                spans.push(ends);
            }
            if spans[0] != spans[1] {
                return bad("its ends must be in the same span (between the same two structures)");
            }
            let positive = |x: f64| x.is_finite() && x > 0.0;
            let at_least_0 = |x: f64| x.is_finite() && x >= 0.0;
            if !(at_least_0(s.weight) && at_least_0(s.wind_area))
                || s.stiffness.is_some_and(|k| !positive(k))
                || s.compression.is_some_and(|c| !at_least_0(c))
                || s.length.is_some_and(|l| !positive(l))
            {
                return bad("weight, wind area, compression ≥ 0; stiffness and length > 0");
            }
        }
        // Taps: no jumper hangs on itself, through other jumpers or not.
        self.jumper_order()?;
        Ok(())
    }

    /// Tap `tap` of jumper `j` holds: its point exists and holds a clamp;
    /// a span's section is solved by finite elements; another jumper.
    fn check_tap(&self, j: &Jumper, tap: &Tap) -> Result<(), LineError> {
        let bad = |m: String| {
            Err(LineError::Invalid(format!(
                "jumper {}: tap {}: {m}",
                j.id,
                tap.text()
            )))
        };
        let object = match tap {
            Tap::Span(r) => {
                let Some(sec) = self.sections.iter().find(|s| s.id == r.section) else {
                    return bad("no such section".into());
                };
                if sec.calculation.method() != condition::Method::Fe {
                    return bad(format!(
                        "section {} is calculated by ruling span: a tap needs finite elements",
                        sec.id
                    ));
                }
                sec.span_points
                    .iter()
                    .find(|p| p.label == r.point)
                    .map(|p| &p.object)
            }
            Tap::Jumper { jumper, point } => {
                if *jumper == j.id {
                    return bad("a jumper cannot hang on itself".into());
                }
                let Some(host) = self.jumpers.iter().find(|x| &x.id == jumper) else {
                    return bad("no such jumper".into());
                };
                host.points
                    .iter()
                    .find(|p| &p.label == point)
                    .map(|p| &p.object)
            }
        };
        match object {
            None => bad("no such point".into()),
            Some(Some(o)) if o.kind == ObjectKind::Clamp && o.hold.is_some() => {
                bad("its clamp holds the jumper up (a support), not a tap".into())
            }
            Some(Some(o)) if o.kind == ObjectKind::Clamp => Ok(()),
            Some(_) => bad("its point holds no clamp (put a clamp object there)".into()),
        }
    }

    /// The jumpers in the order they are solved: those hanging on another
    /// jumper (a tap on its point) after it; refused when jumpers hang on
    /// each other in a ring.
    pub fn jumper_order(&self) -> Result<Vec<Vec<usize>>, LineError> {
        let index: BTreeMap<&str, usize> = self
            .jumpers
            .iter()
            .enumerate()
            .map(|(k, j)| (j.id.as_str(), k))
            .collect();
        let hosts: Vec<Vec<usize>> = self
            .jumpers
            .iter()
            .map(|j| {
                j.taps()
                    .filter_map(|t| match t {
                        Tap::Jumper { jumper, .. } => index.get(jumper.as_str()).copied(),
                        Tap::Span(_) => None,
                    })
                    .collect()
            })
            .collect();
        let mut level: Vec<Option<usize>> = vec![None; self.jumpers.len()];
        for _ in 0..=self.jumpers.len() {
            let mut changed = false;
            for k in 0..self.jumpers.len() {
                if level[k].is_some() {
                    continue;
                }
                if let Some(l) = hosts[k]
                    .iter()
                    .map(|h| level[*h])
                    .try_fold(0usize, |m, x| x.map(|x| m.max(x + 1)))
                {
                    level[k] = Some(l);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        if let Some(k) = level.iter().position(Option::is_none) {
            return Err(LineError::Invalid(format!(
                "jumper {}: jumpers hang on each other in a ring (taps)",
                self.jumpers[k].id
            )));
        }
        let top = level.iter().flatten().copied().max().unwrap_or(0);
        Ok((0..=top)
            .map(|l| (0..level.len()).filter(|k| level[*k] == Some(l)).collect())
            .filter(|v: &Vec<usize>| !v.is_empty())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A line with a spacer between S1 and S2 (both T1–T2) at span
    /// points a third along.
    fn spaced() -> Line {
        let mut line = example();
        for s in &mut line.sections[..2] {
            s.span_points.push(SpanPoint {
                label: "P1".into(),
                span: 0,
                at: SpanPlace::at_fraction(1.0 / 3.0),
                description: String::new(),
                // A spacer's end: a clamp (of no weight here).
                object: Some(LineObject::clamp(0.0)),
            });
        }
        line.spacers.push(Spacer {
            id: "SP1".into(),
            a: SpanPointRef {
                section: line.sections[0].id.clone(),
                point: "P1".into(),
            },
            b: SpanPointRef {
                section: line.sections[1].id.clone(),
                point: "P1".into(),
            },
            weight: 150.0,
            compression: Some(DEFAULT_SPACER_COMPRESSION),
            ..Default::default()
        });
        line
    }

    /// Span points, spread loads and spacers: checked (labels unique,
    /// inside their span; a spacer between two sections of one span, both
    /// by finite elements — not the ruling span); places by fraction or
    /// distance; the project defaults as a file and back.
    #[test]
    fn span_points_spread_loads_and_spacers() {
        let line = spaced();
        line.validate().unwrap();
        let text = serde_json::to_string(&line).unwrap();
        assert_eq!(serde_json::from_str::<Line>(&text).unwrap(), line);
        // A ruling-span section refuses its spacer.
        let mut rs = line.clone();
        rs.sections[0].calculation = condition::Calculation::INITIAL_RS;
        assert!(
            rs.validate()
                .unwrap_err()
                .to_string()
                .contains("ruling span")
        );
        // Both ends on one section, a missing point, another span.
        let mut one = line.clone();
        one.spacers[0].b.section = one.spacers[0].a.section.clone();
        assert!(one.validate().is_err());
        let mut missing = line.clone();
        missing.spacers[0].b.point = "nope".into();
        assert!(missing.validate().is_err());
        // Its ends hold clamps; files before gave bare ends: given one.
        let mut bare = line.clone();
        for p in bare.sections[0]
            .span_points
            .iter_mut()
            .filter(|p| p.label == "P1")
        {
            p.object = None;
        }
        assert!(
            bare.validate()
                .unwrap_err()
                .to_string()
                .contains("no clamp")
        );
        bare.read_older();
        bare.validate().unwrap();
        let mut far = line.clone();
        far.spacers[0].b.section = far.sections[2].id.clone();
        far.sections[2].span_points.push(SpanPoint {
            label: "P1".into(),
            span: 0,
            at: SpanPlace::at_fraction(0.5),
            description: String::new(),
            object: Some(LineObject::clamp(0.0)),
        });
        assert!(
            far.validate()
                .unwrap_err()
                .to_string()
                .contains("same span")
        );
        // Labels unique; places inside the span.
        let mut twice = line.clone();
        let again = twice.sections[0].span_points[0].clone();
        twice.sections[0].span_points.push(again);
        assert!(twice.validate().is_err());
        let mut out = line.clone();
        out.sections[0].span_points[0].at = SpanPlace::at_fraction(1.5);
        assert!(out.validate().is_err());
        // A spread load and a point load placed by distance.
        let mut loaded = line.clone();
        loaded.sections[0].add_spread_load(&SpreadLoad {
            span: 0,
            start: SpanPlace::at_distance(20.0),
            end: SpanPlace::at_fraction(0.5),
            load: 15.0,
            label: "tray".into(),
        });
        loaded.sections[0].add_point_load(&SpanLoad {
            span: 0,
            distance: Some(40.0),
            vertical: 50.0,
            ..Default::default()
        });
        loaded.validate().unwrap();
        assert!((SpanPlace::at_distance(30.0).fraction_of(120.0) - 0.25).abs() < 1e-12);
        assert_eq!(SpanPlace::at_fraction(0.4).fraction_of(120.0), 0.4);
        let mut negative = loaded.clone();
        negative.sections[0].spreads[0].load = -1.0;
        assert!(negative.validate().is_err());
        // Defaults: 10 000 N, written and read back, changed.
        let d = Defaults::default();
        assert_eq!(d.spacer_compression, 10_000.0);
        assert_eq!(Defaults::parse(&d.to_text()).unwrap(), d);
        let mine = Defaults::parse(
            "format = \"ols-defaults\"\nversion = 1\n[spacer]\ncompression = \"8 kN\"\n",
        )
        .unwrap();
        assert_eq!(mine.spacer_compression, 8000.0);
        assert!(Defaults::parse("format = \"x\"").is_err());
        // Every value: written in its unit, read back; prefixes read.
        let mut all = Defaults::default();
        for (k, f) in DEFAULT_FIELDS.iter().enumerate() {
            let v = (f.get)(&all);
            let changed = if f.high.is_finite() {
                (v + f.high) / 2.0
            } else {
                v * 1.5
            };
            (f.set)(&mut all, if k % 2 == 0 { changed } else { v });
        }
        all.validate().unwrap();
        let text = all.to_text();
        assert!(text.contains("[clearance]") && text.contains("[thermal]"));
        assert_eq!(Defaults::parse(&text).unwrap(), all);
        let mine = Defaults::parse(
            "format = \"ols-defaults\"\nversion = 1\n[insulator]\nstiffness = \"40000 kN\"\n\
             [clearance]\nsearch_margin = \"3000 mm\"\nwind_step = \"0.1 kPa\"\n\
             [thermal]\nalbedo = 0.2\n",
        )
        .unwrap();
        assert_eq!(
            (
                mine.insulator_stiffness,
                mine.clearance_search_margin,
                mine.clearance_wind_step
            ),
            (4e7, 3.0, 100.0)
        );
        assert_eq!(mine.thermal_albedo, 0.2);
        assert_eq!(mine.jumper_exit_angle_deg, 30.0, "not given: the program's");
        // Unknown keys and values out of range are refused.
        for bad in [
            "[jumper]\nexit_angle = 95",
            "[jumper]\nstiffness_factor = 0",
            "[thermal]\nemissivity = 1.5",
            "[jumper]\nlength = 3",
            "[nothing]\nx = 1",
            "x = 1",
        ] {
            assert!(Defaults::parse(bad).is_err(), "{bad}");
        }
    }

    pub(crate) fn example() -> Line {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/synthetic-line/synthetic-line.ols.json");
        Line::load(&path).unwrap()
    }

    /// A 0.6 m marker ball as older files gave it (a point load), read as
    /// a body: its weight in still air, the wind on its (iced) circle, the
    /// ice shell on its sphere.
    #[test]
    fn a_marker_ball_takes_wind_and_ice() {
        use ols_core::units::{Density, Length, Stress, Temperature};
        use std::f64::consts::PI;
        let close = |a: f64, b: f64| assert!((a - b).abs() < 1e-9 * b.abs().max(1.0), "{a} {b}");
        let ball = SpanLoad {
            vertical: 55.0,
            diameter: 0.6,
            ..Default::default()
        };
        let still = ols_weather::WeatherCase::still("still", Temperature::from_celsius(15.0));
        assert_eq!(ball.object().forces(&still, 0.025), [55.0, 0.0]);
        let wind = ols_weather::WeatherCase {
            wind_pressure: Stress::from_pascals(650.0),
            ..still.clone()
        };
        let [down, across] = ball.object().forces(&wind, 0.025);
        close(down, 55.0);
        close(across, 650.0 * PI * 0.09); // ≈ 184 N
        let half = SpanLoad {
            drag: Some(0.5),
            ..ball.clone()
        };
        close(half.object().forces(&wind, 0.025)[1], across / 2.0);
        let given = SpanLoad {
            wind_area: Some(0.1),
            ..ball.clone()
        };
        close(given.object().forces(&wind, 0.025)[1], 65.0);
        let ice = ols_weather::WeatherCase {
            ice_thickness: Length::from_millimeters(10.0),
            ice_density: Some(Density::from_kilograms_per_cubic_meter(900.0)),
            ..wind
        };
        let [down, across] = ball.object().forces(&ice, 0.025);
        let shell = PI / 6.0 * (0.62f64.powi(3) - 0.6f64.powi(3));
        close(
            down,
            55.0 + shell * 900.0 * ols_core::units::STANDARD_GRAVITY,
        );
        close(across, 650.0 * PI / 4.0 * 0.62 * 0.62);
        let surface = SpanLoad {
            ice_area: Some(2.0),
            ..ball.clone()
        };
        close(
            surface.object().forces(&ice, 0.025)[0],
            55.0 + 2.0 * 0.01 * 900.0 * ols_core::units::STANDARD_GRAVITY,
        );
        // A point without a size: its weight only.
        let point = SpanLoad {
            vertical: 55.0,
            ..Default::default()
        };
        assert_eq!(point.object().forces(&ice, 0.025), [55.0, 0.0]);
        // Negative sizes are refused.
        let mut line = example();
        line.sections[0].objects_mut().next().unwrap().shape = Shape::Sphere { diameter: -1.0 };
        assert!(line.validate().is_err());
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
            // Neighbours 0.4 m apart; the two highest level.
            for k in 0..o.len() {
                let (a, b) = (o[k], o[(k + 1) % o.len()]);
                assert!(
                    ((a[0] - b[0]).hypot(a[1] - b[1]) - 0.4).abs() < 1e-12,
                    "{n}: {o:?}"
                );
            }
            let mut ys: Vec<f64> = o.iter().map(|p| p[1]).collect();
            ys.sort_by(|a, b| b.total_cmp(a));
            assert!((ys[0] - ys[1]).abs() < 1e-12, "{n}: {o:?}");
        }
        // Three: two side by side above one at the bottom; turned by 60°:
        // one on top.
        s.bundle = 3;
        let r = 0.4 / (2.0 * (std::f64::consts::PI / 3.0).sin());
        assert!(s.bundle_offsets().iter().any(|p| (p[1] + r).abs() < 1e-12));
        s.bundle_rotation_deg = Some(60.0);
        assert!(s.bundle_offsets().iter().any(|p| (p[1] - r).abs() < 1e-12));
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
                .contains("no wire point with set 999")
        );
        let mut line = example();
        line.sections[0].add_point_load(&SpanLoad {
            span: 99,
            fraction: 0.5,
            vertical: 55.0,
            label: "ball".into(),
            ..Default::default()
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
                let (p, q) = (s.points[0].position, s.wires[0].position);
                ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
            };
            assert!((d(a) - d(b)).abs() < 1e-9);
        }
        assert!((line.alignment_points[0].position[0] - 1000.0).abs() < 1e-9);
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
    fn strings_need_their_points() {
        let mut line = example();
        line.structures[1].strings[0].top = "nowhere".into();
        assert!(
            line.validate()
                .unwrap_err()
                .to_string()
                .contains("no point \"nowhere\"")
        );
        let mut line = example();
        let w = line.structures[1].wires[0].clone();
        line.structures[1].wires.push(WirePoint {
            label: "again".into(),
            ..w
        });
        assert!(
            line.validate()
                .unwrap_err()
                .to_string()
                .contains("two wire points with set")
        );
    }

    /// A version 1 line file reads, sections and jumpers keep their ends
    /// (phase 1: the set; other phases: their own sets), and it is written
    /// as version 2 and read back the same.
    #[test]
    fn version_1_files_read_and_write_as_version_2() {
        let line = example();
        assert!(line.structures.iter().all(|s| !s.wires.is_empty()));
        for sec in &line.sections {
            for p in &sec.points {
                assert!(line.wire_point(p).is_some(), "{p:?}");
            }
        }
        let text = line.to_json();
        assert!(!text.contains("\"phase\"") && !text.contains("attachments"));
        let again = Line::from_json(&text).unwrap();
        assert_eq!(again.structures, line.structures);
        assert_eq!(again.sections, line.sections);
    }

    fn span_point(label: &str, span: usize, at: SpanPlace) -> SpanPoint {
        SpanPoint {
            label: label.into(),
            span,
            at,
            description: String::new(),
            object: None,
        }
    }

    /// A section's points changed: its span points keep their span (by
    /// its ends); a span gone with points on it is refused.
    #[test]
    fn span_points_follow_their_span_when_points_change() {
        let mut sec = example().sections[0].clone();
        let p = sec.points.clone();
        sec.span_points = vec![span_point("A", 1, SpanPlace::at_fraction(0.5))];
        sec.stringing = Stringing::Sag {
            sag: 5.0,
            temperature: 15.0,
            span: Some(2),
        };
        // The first structure taken out: span 1 (p1–p2) becomes span 0.
        sec.set_points(p[1..].to_vec()).unwrap();
        assert_eq!(sec.span_points[0].span, 0);
        assert!(matches!(
            sec.stringing,
            Stringing::Sag { span: Some(1), .. }
        ));
        // Put back in front: span 1 again.
        sec.set_points(p.clone()).unwrap();
        assert_eq!(sec.span_points[0].span, 1);
        // A structure taken out of the middle of its span's neighbours:
        // p2 gone, the span p1–p2 no longer exists.
        let mut without = p.clone();
        without.remove(2);
        let err = sec.set_points(without).unwrap_err();
        assert!(err.contains('A'), "{err}");
        assert_eq!(sec.points, p);
        // Clipped lengths follow their spans.
        let n = p.len() - 1;
        sec.stringing = Stringing::UnstressedLengths {
            lengths: (0..n).map(|k| 100.0 + k as f64).collect(),
            temperature: 15.0,
            sagged: None,
        };
        sec.set_points(p[1..].to_vec()).unwrap();
        assert!(matches!(&sec.stringing,
            Stringing::UnstressedLengths { lengths, .. } if lengths[0] == 101.0 && lengths.len() == n - 1));
    }

    /// Spread labels unique, a span point's distance within its span,
    /// stringing values that make sense.
    #[test]
    fn validation_catches_spreads_places_and_stringing() {
        let line = example();
        let mut l = line.clone();
        let sec = &mut l.sections[0];
        sec.span_points = vec![
            span_point("A", 0, SpanPlace::at_fraction(0.2)),
            span_point("B", 0, SpanPlace::at_fraction(0.6)),
        ];
        let spread = SpreadObject {
            label: "S".into(),
            from: "A".into(),
            to: "B".into(),
            load: 5.0,
            ..Default::default()
        };
        sec.spreads = vec![spread.clone(), spread];
        assert!(l.validate().unwrap_err().to_string().contains("twice"));
        l.sections[0].spreads.pop();
        l.validate().unwrap();

        let mut far = line.clone();
        far.sections[0].span_points = vec![span_point("A", 0, SpanPlace::at_distance(1e5))];
        assert!(
            far.validate()
                .unwrap_err()
                .to_string()
                .contains("inside a span")
        );
        far.sections[0].span_points[0].at = SpanPlace::at_distance(10.0);
        far.validate().unwrap();

        let mut bad = line.clone();
        bad.sections[0].stringing = Stringing::Tension {
            horizontal_tension: -1.0,
            temperature: 15.0,
        };
        assert!(bad.validate().is_err());
        bad.sections[0].stringing = Stringing::Tension {
            horizontal_tension: 20e3,
            temperature: f64::NAN,
        };
        assert!(bad.validate().is_err());
        let spans = bad.sections[0].points.len() - 1;
        bad.sections[0].stringing = Stringing::UnstressedLengths {
            lengths: vec![0.0; spans],
            temperature: 15.0,
            sagged: None,
        };
        assert!(bad.validate().is_err());
    }

    /// Circuit starts are section points, each once; files with stale
    /// ones drop them on reading.
    #[test]
    fn circuit_starts_are_section_points() {
        let mut line = example();
        line.circuit_starts = vec![line.sections[0].points[0].clone()];
        line.validate().unwrap();
        line.circuit_starts.push(SectionPoint::new("nowhere", 1));
        assert!(line.validate().is_err());
        line.drop_stale_circuit_starts();
        assert_eq!(line.circuit_starts.len(), 1);
        line.circuit_starts.push(line.circuit_starts[0].clone());
        assert!(line.validate().is_err());
        line.circuit_starts.push(SectionPoint::new("nowhere", 1));
        let again = Line::from_json(&line.to_json()).unwrap();
        assert_eq!(again.circuit_starts, [line.sections[0].points[0].clone()]);
    }

    /// The default clamp length stays within what a clamp may be.
    #[test]
    fn default_clamp_length_at_most_a_clamp() {
        let mut d = Defaults {
            jumper_clamp_length: 1.0,
            ..Default::default()
        };
        d.validate().unwrap();
        d.jumper_clamp_length = 1.5;
        assert!(d.validate().is_err());
    }

    /// Places are one of their kinds, written with one key; files before
    /// version 5 giving several are read by their priority.
    #[test]
    fn places_are_one_kind_and_older_ones_read_by_priority() {
        let read = |t: &str| serde_json::from_str::<SpanPlace>(t).unwrap();
        assert_eq!(read(r#"{"fraction":0.4}"#), SpanPlace::Fraction(0.4));
        assert_eq!(
            read(r#"{"fraction":0.4,"distance":30.0}"#),
            SpanPlace::Distance(30.0)
        );
        assert_eq!(
            read(r#"{"fraction":0.4,"distance":30.0,"station":1520.0}"#),
            SpanPlace::Station(1520.0)
        );
        assert_eq!(
            serde_json::to_string(&SpanPlace::Distance(30.0)).unwrap(),
            r#"{"distance":30.0}"#
        );
        assert!(serde_json::from_str::<SpanPlace>(r#"{"far":1}"#).is_err());
        let jread = |t: &str| serde_json::from_str::<JumperPlace>(t).unwrap();
        assert_eq!(
            jread(r#"{"fraction":0.5,"length":2.0}"#),
            JumperPlace::Length(2.0)
        );
        assert_eq!(
            jread(r#"{"fraction":0.5,"length":2.0,"distance":3.0}"#),
            JumperPlace::Distance(3.0)
        );
        assert_eq!(
            serde_json::to_string(&JumperPlace::Fraction(0.5)).unwrap(),
            r#"{"fraction":0.5}"#
        );
    }

    /// A jumper end is a wire point or a tap; files before version 5
    /// gave a tap beside a placeholder `from`.
    #[test]
    fn jumper_ends_are_a_wire_point_or_a_tap() {
        let old = r#"{"id":"J","cable":"c","from":{"structure":"","set":0},
            "to":{"structure":"T3","set":1},"length":5.0,
            "from_tap":{"span":{"section":"S1","point":"P"}}}"#;
        let j: Jumper = serde_json::from_str(old).unwrap();
        let tap = Tap::Span(SpanPointRef {
            section: "S1".into(),
            point: "P".into(),
        });
        assert_eq!(j.from, JumperEnd::tap(tap.clone()));
        assert_eq!(j.to, JumperEnd::wire("T3", 1));
        let text = serde_json::to_string(&j).unwrap();
        assert!(
            text.contains(r#""from":{"tap":{"span":{"section":"S1","point":"P"}}}"#),
            "{text}"
        );
        assert!(!text.contains("from_tap"));
        assert_eq!(serde_json::from_str::<Jumper>(&text).unwrap(), j);
        assert_eq!(j.taps().collect::<Vec<_>>(), [&tap]);
        assert_eq!(j.to.to_string(), "T3 set 1");
        assert_eq!(j.from.to_string(), "tap S1 P");
    }

    /// Supports are jumper points holding a held clamp; files before
    /// version 5 listed them on their own.
    #[test]
    fn supports_are_points_holding_a_held_clamp() {
        let old = r#"{"id":"J","cable":"c","from":{"structure":"T3","set":2},
            "to":{"structure":"T3","set":1},"length":5.0,
            "supports":[{"wire":"JW","at":0.6,"clamp_length":0.2},
                        {"structure":"T4","wire":"JX","at":0.3}],
            "points":[{"label":"C1","at":{"fraction":0.9}}]}"#;
        let j: Jumper = serde_json::from_str(old).unwrap();
        let clamps = j.supports();
        // In order along it; labels not taken by its points.
        assert_eq!(
            clamps
                .iter()
                .map(|c| (c.label.as_str(), c.wire.as_str(), c.at))
                .collect::<Vec<_>>(),
            [("C3", "JX", 0.3), ("C2", "JW", 0.6)]
        );
        assert_eq!(clamps[1].clamp_length, 0.2);
        assert_eq!(clamps[0].structure.as_deref(), Some("T4"));
        let text = serde_json::to_string(&j).unwrap();
        assert!(!text.contains("supports"), "{text}");
        assert!(
            text.contains(r#""hold":{"strings":{"wire":"JW"}}"#),
            "{text}"
        );
        assert_eq!(serde_json::from_str::<Jumper>(&text).unwrap(), j);
        // Edited: the same points, moved.
        let mut k = j.clone();
        k.edit_supports(|c| c[1].at = 0.7);
        assert_eq!(k.supports()[1].label, "C2");
        assert_eq!(k.points.len(), 3);
        // A support string.
        let old = r#"{"id":"J","cable":"c","from":{"structure":"T3","set":2},
            "to":{"structure":"T3","set":1},"length":5.0,
            "support":{"point":[1.0,2.0,3.0],"length":2.0,"weight":50.0}}"#;
        let j: Jumper = serde_json::from_str(old).unwrap();
        let s = j.support().unwrap();
        assert_eq!(
            (s.point, s.length, s.weight, s.fraction),
            ([1.0, 2.0, 3.0], 2.0, 50.0, 0.5)
        );
        assert_eq!(j.points[0].label, "S1");
    }

    /// A tap is not on a support's clamp.
    #[test]
    fn a_tap_is_not_on_a_support() {
        let mut line = example();
        let host = line.jumpers[0].id.clone();
        line.jumpers[0].length = Some(12.0);
        line.jumpers[0].sag = None;
        line.jumpers[0].set_support(Some(JumperSupport {
            point: [0.0, 0.0, 30.0],
            length: 2.0,
            weight: 0.0,
            fraction: 0.5,
        }));
        let label = line.jumpers[0].points[0].label.clone();
        let mut tap = line.jumpers[0].clone();
        tap.id = "JT".into();
        tap.set_support(None);
        tap.from = JumperEnd::tap(Tap::Jumper {
            jumper: host,
            point: label,
        });
        line.jumpers.push(tap);
        let e = line.validate().unwrap_err().to_string();
        assert!(e.contains("support"), "{e}");
    }

    /// A point relabelled: its spreads and the spacers on it follow.
    #[test]
    fn a_relabelled_point_takes_its_spreads_and_spacers() {
        let mut line = spaced();
        let s1 = line.sections[0].id.clone();
        line.sections[0].span_points.push(SpanPoint {
            label: "Q".into(),
            span: 0,
            at: SpanPlace::at_fraction(0.6),
            ..Default::default()
        });
        line.sections[0].spreads.push(SpreadObject {
            from: "P1".into(),
            to: "Q".into(),
            load: 5.0,
            ..Default::default()
        });
        line.validate().unwrap();
        line.relabel_point(false, &s1, "P1", "P9").unwrap();
        line.validate().unwrap();
        assert_eq!(line.spacers[0].a.point, "P9");
        assert_eq!(line.sections[0].spreads[0].from, "P9");
        assert!(line.relabel_point(false, &s1, "P9", "Q").is_err());
        assert!(line.relabel_point(false, &s1, "nope", "Z").is_err());
    }
}
