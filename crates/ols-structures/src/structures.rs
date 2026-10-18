// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Structure types (`*.olss`, one type per file) and their placement.
//!
//! **Local axes**: origin at the centre of the structure, **x to the
//! right of the line** looking ahead, **y ahead** along the line, z up
//! (right-handed). A placed structure has a position and a rotation: the
//! direction of its local x axis, degrees counter-clockwise from east.
//! z = 0 is the ground unless the type gives the level of its feet
//! (`foot = "-32 m"`, e.g. for a pylon drawn from a level under its lowest
//! crossarm): it is placed with its feet on the ground, raised by the
//! structure's height adjustment (foundations); an extension lengthens it
//! below z = 0, so everything above the feet rises by its `dz`.
//!
//! ```toml
//! format = "ols-structure"
//! version = 1
//! id = "DL380-tangent"
//! name = "380 kV double circuit, tangent"
//! kind = "lattice"
//!
//! [[attachment]]
//! set = 1
//! phase = 1
//! label = "L1 left"
//! point = ["-12.5 m", "0 m", "45 m"]
//! insulator = "I-380-double"
//!
//! [[extension]]
//! name = "+3"
//! dz = "3 m"
//! ```
//!
//! **Strings (version 2)**: what hangs on the structure, defined
//! explicitly — **points** on the structure (`[[point]]`), **strings**
//! between them (`[[string]]`: a top — a point, or another string's wire
//! point — and a wire point, with an insulator from the library or a length
//! and weight given here, optionally the compression it may take) and
//! **wire points** (`[[wire]]`, where strings meet; with a `set`, where a
//! wire is clamped). A V-string is two strings to one wire point, a V with
//! a string below a third string from that wire point, a TAK a suspension
//! string to a wire point and two strain strings from it. A wire point's
//! position may be given; otherwise it hangs below its string (a strain
//! string with a `side`: along the line), or below the middle of its
//! strings' tops where their lengths meet:
//!
//! ```toml
//! [[point]]
//! label = "A1-L"
//! point = ["-14 m", "0 m", "40 m"]
//!
//! [[point]]
//! label = "A1-R"
//! point = ["-10 m", "0 m", "40 m"]
//!
//! [[wire]]
//! label = "V1"
//!
//! [[wire]]
//! label = "W1"
//! set = 1
//! # weight = "3 kN"   # a counterweight hanging at it
//!
//! [[string]]
//! label = "S1-L"
//! top = "A1-L"
//! wire = "V1"
//! length = "4.5 m"
//! weight = "400 N"
//!
//! [[string]]
//! label = "S1-R"
//! top = "A1-R"
//! wire = "V1"
//! length = "4.5 m"
//! weight = "400 N"
//!
//! [[string]]
//! label = "S1"
//! top = "V1"
//! wire = "W1"
//! length = "0.5 m"
//! weight = "50 N"
//! ```
//!
//! Version 1 files give each string as an `[[attachment]]` (set, phase,
//! point, insulator or string): read as before, their points labelled by
//! the string's top label or `A<set>-<phase>`.
//!
//! **Simple pylons**: the insulator strings can be given in the pylon file
//! itself instead of from the insulator library — where the string hangs
//! (`point`), its length, and a label at its top (insulator attachment
//! point) and at its bottom (wire attachment point). It hangs down, or
//! with a `side` it is a strain string along the line:
//!
//! ```toml
//! id = "D380-tangent"
//! name = "380 kV, 3 crossarms each side + earth wire"
//! kind = "lattice"
//! height = "62 m"
//!
//! [[attachment]]
//! set = 1
//! phase = 1
//! point = ["-12 m", "0 m", "40 m"]
//! string = { length = "5.2 m", top = "A1", bottom = "L1" }
//! ```
//!
//! **Stick drawing**: the `[[body]]` polylines when given; otherwise a mast
//! from the centre point (the origin) up to `height` (or the highest
//! attachment) and a crossarm from the mast to every attachment point.
//!
//! **Capacity** ([`Capacity`], optional): what the structure can carry,
//! for the structure checks. Method 1, allowable spans — the wind span and
//! weight span the type is designed for, by line angle (interpolated
//! linearly; a structure turning more than the last angle fails); Method
//! 2, allowable loads at each attachment (a load tree), compared with the
//! factored loads times the load case's strength factor:
//!
//! ```toml
//! [[capacity.spans]]
//! angle = "0 deg"
//! wind_span = "400 m"
//! weight_span = "600 m"
//! min_weight_span = "-50 m"   # optional: uplift limit
//!
//! [[capacity.spans]]
//! angle = "10 deg"
//! wind_span = "300 m"
//! weight_span = "550 m"
//!
//! [[capacity.attachment]]
//! point = "A1-L"              # a point of the structure (version 1: set, phase)
//! vertical = "40 kN"          # downwards
//! uplift = "10 kN"            # optional (default: none allowed)
//! transverse = "30 kN"
//! longitudinal = "20 kN"
//! interaction = "linear"      # optional: the ratios add up (default: each on its own)
//! ```
//!
//! **Wind on the structure** ([`StructureWind`], optional): the area the
//! wind blows on, by height band, with its drag (force) coefficient; the
//! structure loads add the force of each band (weather case wind pressure
//! × drag × area) at the band's mid-height:
//!
//! ```toml
//! [[wind.band]]
//! from = "0 m"
//! to = "20 m"
//! area = "14 m²"     # projected on a plane along the line
//! drag = 3.0         # e.g. lattice faces about 2.8–3.2, round poles about 0.7–1.2
//! ```
//!
//! **Analysis** (optional): a finite-element model of the structure
//! (`model = "type.olsfe"`, next to this file; its nodes name the
//! attachments they carry) and/or a pole (`pole = "id"`, a `.olsp` file
//! in the same folder, or a `[pole]` table, [`crate::pole`]) — checked
//! with the attachment loads of each load case (`ols_structures_fe`,
//! through `ols_loads::capacity`).
//!
//! **Extension bodies** (version 3): an `[[extension]]` may carry its own
//! joints and members below the feet and leg extensions
//! ([`crate::extensions`]); a placed structure's finite-element model
//! includes the extension it stands on.
//!
//! **Body building** (version 3, written by the structure application):
//! a member's `group` (sections and reports per group), `made_by` on a
//! joint or member (the `[[generator]]` that wrote it, so a rebuild
//! replaces only that), `[[generator]]` (a generator's id, kind and
//! inputs) and `[[symmetry]]` (mirror planes kept while editing). The core
//! keeps the last two unread; `ols-structure-edit` reads them. The joints
//! and members a generator writes are ordinary `[[node]]` and
//! `[[member]]` entries.

use ols_core::units::Length;
use serde::Deserialize;

use crate::insulators::{AssemblyKind, InsulatorLibrary};
use crate::{Library, LibraryItem};

/// File extension of structure types.
pub const STRUCTURE_FILE_EXTENSION: &str = "olss";
/// Format identifier.
pub const STRUCTURE_FILE_FORMAT: &str = "ols-structure";
/// Newest format version read. Version 4 adds a string's optional limits,
/// not checked by OLS CAD (`max_load`, `max_swing`, `max_swing_across`,
/// `max_swing_along`: [`crate::InsulatorLimits`]). Version 3 adds what the structure
/// application writes: a member's `group` and `made_by`, a joint's
/// `made_by`, `[[symmetry]]` and `[[generator]]` (and `[[load_case]]`),
/// the `[pole]` table and an extension's `node`, `member` and `leg`
/// tables; versions 1 to 3 read as before.
pub const STRUCTURE_FILE_VERSION: u32 = 4;

/// Family of structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructureKind {
    /// Lattice tower.
    Lattice,
    /// Pole (steel, concrete, wood).
    Pole,
    /// Portal / gantry.
    Portal,
    /// Anything else.
    Other,
}

/// How much of a structure its file describes, worked out from what the
/// file holds ([`StructureType::detail`]). Checks that depend on the
/// structure's shape (distances to the structure body, structure analysis)
/// behave differently at each level: when designing such a check, decide
/// (with the project owner) what it does for each of the three.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Deserialize)]
#[serde(try_from = "u8")]
pub enum Detail {
    /// 1 — a stick figure: a mast and crossarms to the points where
    /// strings hang (or `[[body]]` polylines in one plane). Enough to
    /// place the wires and draw.
    #[default]
    Stick = 1,
    /// 2 — the pylon's members as sticks (`[[node]]` joints and
    /// `[[member]]`s without sections, or `[[body]]` polylines that make a
    /// 3D object); for drawings and distances to the structure.
    Outline = 2,
    /// 3 — a full design: every member with its section, the supports
    /// (or a separate finite-element model, `model`); for structure
    /// analysis.
    Design = 3,
}

impl TryFrom<u8> for Detail {
    type Error = String;

    fn try_from(v: u8) -> Result<Self, String> {
        match v {
            1 => Ok(Detail::Stick),
            2 => Ok(Detail::Outline),
            3 => Ok(Detail::Design),
            _ => Err(format!(
                "detail {v}: 1 (stick figure), 2 (outline) or 3 (full design)"
            )),
        }
    }
}

impl Detail {
    /// Its number in files.
    pub fn level(self) -> u8 {
        self as u8
    }

    /// Name in reports.
    pub fn name(self) -> &'static str {
        match self {
            Detail::Stick => "stick figure",
            Detail::Outline => "outline",
            Detail::Design => "full design",
        }
    }
}

/// Side of a strain attachment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// Strain assembly towards the ahead span (+y).
    Ahead,
    /// Strain assembly towards the back span (−y).
    Back,
}

/// An attachment point of a structure type.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeAttachment {
    /// Set number.
    pub set: u32,
    /// Phase number.
    pub phase: u32,
    /// Label.
    #[serde(default)]
    pub label: String,
    /// Insulator attachment point in local axes.
    pub point: [Length; 3],
    /// Default insulator assembly id.
    #[serde(default)]
    pub insulator: Option<String>,
    /// For strain assemblies: which span the assembly points to.
    #[serde(default)]
    pub side: Option<Side>,
    /// Or: the string given here (instead of `insulator`).
    #[serde(default)]
    pub string: Option<InlineString>,
}

/// An insulator string defined in the pylon file.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InlineString {
    /// Length from the attachment point to the wire point.
    pub length: Length,
    /// Weight (default 0).
    #[serde(default)]
    pub weight: ols_core::units::Force,
    /// Label of the top (insulator attachment point).
    #[serde(default)]
    pub top: String,
    /// Label of the bottom (wire attachment point).
    #[serde(default)]
    pub bottom: String,
}

/// A point of a structure type where strings hang (version 2).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypePoint {
    /// Label, unique among the type's points and wire points.
    pub label: String,
    /// What it is, in words (free text).
    #[serde(default)]
    pub description: String,
    /// Position in local axes (filled from `node` or `member` when they
    /// are given instead).
    #[serde(default)]
    pub point: Option<[Length; 3]>,
    /// Or: at a joint of the pylon.
    #[serde(default)]
    pub node: Option<String>,
    /// Or: along a member of the pylon…
    #[serde(default)]
    pub member: Option<String>,
    /// …at this fraction from its first joint (0–1).
    #[serde(default)]
    pub at: Option<f64>,
}

impl TypePoint {
    /// Its position in local axes (once the type is read).
    pub fn position(&self) -> [Length; 3] {
        self.point.unwrap_or([Length::ZERO; 3])
    }
}

/// A joint of the pylon (levels 2 and 3), in local axes. The fields for
/// the finite-element check (`support`, `spring`, `attachment`) are read
/// by `ols_structures_fe`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeNode {
    /// Id, unique among the joints.
    pub id: String,
    /// Position in local axes.
    pub point: [Length; 3],
    /// Support: `"fixed"`, `"pinned"` or the fixed directions.
    #[serde(default)]
    pub support: Option<toml::Value>,
    /// Spring stiffnesses at a support.
    #[serde(default)]
    pub spring: Option<toml::Value>,
    /// The point of the type it carries (also set by a point's `node`).
    #[serde(default)]
    pub attachment: Option<toml::Value>,
    /// The `[[generator]]` that wrote it (its id): a rebuild of that
    /// generator replaces it; none: made by hand.
    #[serde(default)]
    pub made_by: Option<String>,
}

/// A member of the pylon between two joints (levels 2 and 3). Without a
/// section it is only a stick; the finite-element fields (`kind`,
/// `reference`, `roll`, `orientation`, `k`, `lk`, `pretension`,
/// `capacity_tension`, `capacity_compression`, `dead_load_factor`) are read by
/// `ols_structures_fe`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeMember {
    /// Id, unique among the members.
    pub id: String,
    /// Its two joints.
    pub nodes: [String; 2],
    /// Section id: in the project's section library (`.olssec`) or a
    /// `[[section]]` table of the file (resolved by `ols_structures_fe`,
    /// the library first); needed for a full design.
    #[serde(default)]
    pub section: Option<String>,
    /// `truss` (default), `frame` or `cable`.
    #[serde(default)]
    pub kind: Option<String>,
    /// Frame: a direction in the local x–y plane.
    #[serde(default)]
    pub reference: Option<toml::Value>,
    /// Frame: turns the local y, z about x.
    #[serde(default)]
    pub roll: Option<toml::Value>,
    /// Effective length factor (buckling).
    #[serde(default)]
    pub k: Option<f64>,
    /// Cable: pretension.
    #[serde(default)]
    pub pretension: Option<toml::Value>,
    /// An angle's legs (local y, z) towards two structure directions
    /// (`"x+y-"`), instead of `reference` and `roll`.
    #[serde(default)]
    pub orientation: Option<toml::Value>,
    /// Effective length for buckling instead of k × length.
    #[serde(default)]
    pub lk: Option<toml::Value>,
    /// Tension capacity instead of the section's.
    #[serde(default)]
    pub capacity_tension: Option<toml::Value>,
    /// Compression capacity instead of the section's and the Euler load.
    #[serde(default)]
    pub capacity_compression: Option<toml::Value>,
    /// Factor on its self-weight, instead of `[analysis]
    /// dead_load_factor`.
    #[serde(default)]
    pub dead_load_factor: Option<toml::Value>,
    /// Its member group (`"body-leg"`, `"body-bracing-3"`…): sections set
    /// and results reported per group.
    #[serde(default)]
    pub group: Option<String>,
    /// The `[[generator]]` that wrote it (its id): a rebuild of that
    /// generator replaces it; none: made by hand.
    #[serde(default)]
    pub made_by: Option<String>,
}

/// A string of a structure type (version 2): from its top (a point, or a
/// wire point) to a wire point.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeString {
    /// Label, unique among the type's strings.
    pub label: String,
    /// What it is, in words (free text).
    #[serde(default)]
    pub description: String,
    /// Its top: a point, or another string's wire point.
    pub top: String,
    /// Its wire point.
    pub wire: String,
    /// Insulator assembly from the library (its length, weight, wind
    /// area).
    #[serde(default)]
    pub insulator: Option<String>,
    /// Or: its length…
    #[serde(default)]
    pub length: Option<Length>,
    /// …and weight (default 0).
    #[serde(default)]
    pub weight: Option<ols_core::units::Force>,
    /// Compression it may take (none: tension only).
    #[serde(default)]
    pub compression: Option<ols_core::units::Force>,
    /// Area the wind blows on (default: its insulator's, else none).
    #[serde(default)]
    pub wind_area: Option<ols_core::units::Area>,
    /// A strain (dead-end) string, else a suspension string.
    #[serde(default)]
    pub strain: bool,
    /// Its angle from straight down (0 hanging, 90 level; default 0).
    #[serde(default)]
    pub angle: Option<ols_core::units::Angle>,
    /// Its direction in plan in the type's axes (0 ahead along the line,
    /// 90 to the right, 180 back; default 0).
    #[serde(default)]
    pub direction: Option<ols_core::units::Angle>,
    /// Files written before: a strain string along the line, ahead or back
    /// (read as level, direction 0° or 180°).
    #[serde(default)]
    pub side: Option<Side>,
    /// Version 4: the tension it is rated for (default: its insulator's;
    /// not checked by OLS CAD, [`crate::InsulatorLimits`]).
    #[serde(default)]
    pub max_load: Option<ols_core::units::Force>,
    /// Version 4: the swing it may take from straight down, any direction
    /// (default: its insulator's; not checked).
    #[serde(default)]
    pub max_swing: Option<ols_core::units::Angle>,
    /// Version 4: the swing it may take across the line (default: its
    /// insulator's; not checked).
    #[serde(default)]
    pub max_swing_across: Option<ols_core::units::Angle>,
    /// Version 4: the swing it may take along the line (default: its
    /// insulator's; not checked).
    #[serde(default)]
    pub max_swing_along: Option<ols_core::units::Angle>,
}

impl TypeString {
    /// Its own limits (not its insulator's; not checked by OLS CAD).
    pub fn limits(&self) -> crate::InsulatorLimits {
        crate::InsulatorLimits::of(
            self.max_load,
            self.max_swing,
            self.max_swing_across,
            self.max_swing_along,
        )
    }

    /// Its angle from straight down and direction in plan, degrees (a
    /// `side` of files written before: level, ahead 0° or back 180°).
    pub fn angles(&self) -> (f64, f64) {
        let deg = |a: Option<ols_core::units::Angle>| a.map_or(0.0, |a| a.si().to_degrees());
        match (self.angle, self.direction, self.side) {
            (None, None, Some(Side::Ahead)) => (90.0, 0.0),
            (None, None, Some(Side::Back)) => (90.0, 180.0),
            (a, d, _) => (deg(a), deg(d)),
        }
    }
}

/// The unit vector, local axes (x right, y ahead, z up), of a string at
/// `angle` from straight down and `direction` in plan (0 ahead, 90 right),
/// degrees.
pub fn local_direction(angle: f64, direction: f64) -> [f64; 3] {
    let (a, d) = (angle.to_radians(), direction.to_radians());
    [a.sin() * d.sin(), a.sin() * d.cos(), -a.cos()]
}

/// Where strings of lengths `la` from `a` and `lb` from `b` meet: of the
/// circle both lengths reach, the point nearest `toward` (where the first
/// string's own direction points; a hanging V: the lowest).
pub fn v_meeting(a: [f64; 3], la: f64, b: [f64; 3], lb: f64, toward: [f64; 3]) -> Option<[f64; 3]> {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let d = (ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2]).sqrt();
    if d < 1e-9 || d > la + lb + 1e-9 || d < (la - lb).abs() - 1e-9 {
        return None;
    }
    let u = ab.map(|x| x / d);
    let along = (la * la - lb * lb + d * d) / (2.0 * d);
    let h = (la * la - along * along).max(0.0).sqrt();
    let c = [0, 1, 2].map(|k| a[k] + along * u[k]);
    let across = |d: [f64; 3]| {
        let dot = d[0] * u[0] + d[1] * u[1] + d[2] * u[2];
        let v = [0, 1, 2].map(|k| d[k] - dot * u[k]);
        let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        (n > 1e-9).then(|| v.map(|x| x / n))
    };
    let v = across([0, 1, 2].map(|k| toward[k] - c[k])).or_else(|| across([0.0, 0.0, -1.0]))?;
    Some([0, 1, 2].map(|k| c[k] + h * v[k]))
}

/// A wire point of a structure type (version 2).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeWire {
    /// Label, unique among the type's points and wire points.
    pub label: String,
    /// What it is, in words (free text).
    #[serde(default)]
    pub description: String,
    /// Set number: a wire is clamped here.
    #[serde(default)]
    pub set: Option<u32>,
    /// Position in local axes (default: from its strings).
    #[serde(default)]
    pub point: Option<[Length; 3]>,
    /// A weight hanging at it (a counterweight).
    #[serde(default)]
    pub weight: Option<ols_core::units::Force>,
}

pub use crate::extensions::{ExtendedBody, Extension, LegExtension};
pub use crate::pole::{InlinePole, PoleRef, PoleRing, PoleShape};

/// Allowable spans at a line angle (Method 1).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpanLimit {
    /// Line angle (default 0).
    #[serde(default)]
    pub angle: Option<ols_core::units::Angle>,
    /// Largest wind span (half the sum of the adjacent spans).
    pub wind_span: Length,
    /// Largest weight span (between the low points of the adjacent spans).
    pub weight_span: Length,
    /// Smallest weight span (uplift limit; may be negative). None: not
    /// checked.
    #[serde(default)]
    pub min_weight_span: Option<Length>,
}

/// How the load ratios of one attachment combine (Method 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Interaction {
    /// Each direction on its own: the largest ratio.
    #[default]
    None,
    /// The ratios add up.
    Linear,
}

/// Allowable loads at one attachment (Method 2), in the structure's axes.
/// A direction without a value is not checked.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentCapacity {
    /// The point of the structure.
    #[serde(default)]
    pub point: Option<String>,
    /// Or (version 1): the set…
    #[serde(default)]
    pub set: Option<u32>,
    /// …and phase of an attachment.
    #[serde(default)]
    pub phase: Option<u32>,
    /// Downwards.
    #[serde(default)]
    pub vertical: Option<ols_core::units::Force>,
    /// Upwards (none: an upward load fails when `vertical` is given).
    #[serde(default)]
    pub uplift: Option<ols_core::units::Force>,
    /// Across the line, either way.
    #[serde(default)]
    pub transverse: Option<ols_core::units::Force>,
    /// Along the line, either way.
    #[serde(default)]
    pub longitudinal: Option<ols_core::units::Force>,
    /// How the ratios combine.
    #[serde(default)]
    pub interaction: Interaction,
}

impl AttachmentCapacity {
    fn name(&self) -> String {
        match (&self.point, self.set, self.phase) {
            (Some(p), ..) => format!("point {p}"),
            (None, Some(s), Some(ph)) => format!("set {s} phase {ph}"),
            _ => "an attachment".into(),
        }
    }
}

/// What a structure type can carry (both methods optional).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capacity {
    /// Method 1: allowable spans by line angle.
    #[serde(default)]
    pub spans: Vec<SpanLimit>,
    /// Method 2: allowable loads per attachment.
    #[serde(rename = "attachment", default)]
    pub attachments: Vec<AttachmentCapacity>,
}

/// Allowable spans at one line angle (Method 1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AllowedSpans {
    /// Largest wind span, m.
    pub wind_span: f64,
    /// Largest weight span, m.
    pub weight_span: f64,
    /// Smallest weight span, m (none: not checked).
    pub min_weight_span: Option<f64>,
}

impl Capacity {
    /// The allowable spans at line angle `angle_deg` (either way):
    /// interpolated between the given angles, the first ones below the
    /// first angle; `None` beyond the last angle (or without Method 1).
    pub fn spans_at(&self, angle_deg: f64) -> Option<AllowedSpans> {
        let a = angle_deg.abs();
        let at = |l: &SpanLimit| l.angle.map_or(0.0, |x| x.degrees());
        let first = self.spans.first()?;
        let of = |l: &SpanLimit| AllowedSpans {
            wind_span: l.wind_span.si(),
            weight_span: l.weight_span.si(),
            min_weight_span: l.min_weight_span.map(|m| m.si()),
        };
        if a <= at(first) {
            return Some(of(first));
        }
        for w in self.spans.windows(2) {
            let (a0, a1) = (at(&w[0]), at(&w[1]));
            if a <= a1 {
                let t = (a - a0) / (a1 - a0);
                let (l0, l1) = (of(&w[0]), of(&w[1]));
                let lerp = |x: f64, y: f64| x + (y - x) * t;
                return Some(AllowedSpans {
                    wind_span: lerp(l0.wind_span, l1.wind_span),
                    weight_span: lerp(l0.weight_span, l1.weight_span),
                    min_weight_span: match (l0.min_weight_span, l1.min_weight_span) {
                        (Some(x), Some(y)) => Some(lerp(x, y)),
                        (x, y) => x.or(y),
                    },
                });
            }
        }
        None
    }

    /// The largest line angle allowed, degrees (none without Method 1).
    pub fn max_angle(&self) -> Option<f64> {
        self.spans
            .last()
            .map(|l| l.angle.map_or(0.0, |x| x.degrees()))
    }

    fn check(&self) -> Result<(), String> {
        let mut last = -1.0;
        for l in &self.spans {
            let a = l.angle.map_or(0.0, |x| x.degrees());
            if !(a > last && a < 180.0) {
                return Err("capacity.spans: angles must rise, from 0 to below 180°".into());
            }
            last = a;
            if !(l.wind_span.si() > 0.0 && l.weight_span.si() > 0.0) {
                return Err(format!("capacity.spans at {a}°: spans must be above 0"));
            }
            if l.min_weight_span
                .is_some_and(|m| m.si().is_nan() || m.si() >= l.weight_span.si())
            {
                return Err(format!(
                    "capacity.spans at {a}°: min_weight_span must be below weight_span"
                ));
            }
        }
        for c in &self.attachments {
            let positive = [c.vertical, c.uplift, c.transverse, c.longitudinal]
                .into_iter()
                .flatten()
                .all(|f| f.si() > 0.0);
            if !positive {
                return Err(format!("capacity: {}: loads must be above 0", c.name()));
            }
            match (&c.point, c.set, c.phase) {
                (Some(_), None, None) | (None, Some(_), Some(_)) => {}
                _ => {
                    return Err(format!(
                        "capacity: {}: give a point, or (version 1) a set and phase",
                        c.name()
                    ));
                }
            }
        }
        Ok(())
    }
}

/// One height band of the wind on a structure.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindBand {
    /// Bottom of the band, above the ground.
    pub from: Length,
    /// Top of the band.
    pub to: Length,
    /// Projected area facing the wind (across the line).
    pub area: ols_core::units::Area,
    /// Drag (force) coefficient.
    pub drag: f64,
}

/// The wind on a structure: its height bands.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructureWind {
    /// Bands, any order.
    #[serde(rename = "band", default)]
    pub bands: Vec<WindBand>,
}

impl StructureWind {
    fn check(&self) -> Result<(), String> {
        for b in &self.bands {
            let (from, to) = (b.from.si(), b.to.si());
            if !(from >= 0.0 && to > from && b.area.si() > 0.0 && b.drag > 0.0) {
                return Err(format!(
                    "wind band {from}–{to} m: from ≥ 0, to above from, area and drag above 0"
                ));
            }
        }
        Ok(())
    }
}

/// An outline polyline for drawings and the 3D view.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outline {
    /// Points in local axes.
    pub points: Vec<[Length; 3]>,
}

/// A structure type.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructureType {
    /// `"ols-structure"` (optional header).
    #[serde(default)]
    pub format: Option<String>,
    /// Format version (optional).
    #[serde(default)]
    pub version: Option<u32>,
    /// Unique id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Family.
    pub kind: StructureKind,
    /// Older files state the level; it now follows from the content
    /// ([`StructureType::detail`]).
    #[serde(default, rename = "detail")]
    stated_detail: Option<toml::Value>,
    /// Height of the mast top (local z) for the stick drawing (default:
    /// the highest attachment point).
    #[serde(default)]
    pub height: Option<Length>,
    /// Level of the feet in local z (default 0: z = 0 is the ground).
    #[serde(default)]
    pub foot: Option<Length>,
    /// What it is, in words (free text).
    #[serde(default)]
    pub description: String,
    /// Where the data comes from.
    #[serde(default)]
    pub source: Option<String>,
    /// Points where strings hang (version 2).
    #[serde(rename = "point", default)]
    pub points: Vec<TypePoint>,
    /// Strings (version 2).
    #[serde(rename = "string", default)]
    pub strings: Vec<TypeString>,
    /// Wire points (version 2).
    #[serde(rename = "wire", default)]
    pub wires: Vec<TypeWire>,
    /// Attachment points (version 1).
    #[serde(rename = "attachment", default)]
    pub attachments: Vec<TypeAttachment>,
    /// Height extensions.
    #[serde(rename = "extension", default)]
    pub extensions: Vec<Extension>,
    /// Outline polylines (stick drawing); empty: mast and crossarms are
    /// drawn from the attachment points.
    #[serde(rename = "body", default)]
    pub body: Vec<Outline>,
    /// Joints of the pylon (levels 2 and 3).
    #[serde(rename = "node", default)]
    pub nodes: Vec<TypeNode>,
    /// Members of the pylon (levels 2 and 3).
    #[serde(rename = "member", default)]
    pub members: Vec<TypeMember>,
    /// Finite-element check: analysis options (read by
    /// `ols_structures_fe`).
    #[serde(default)]
    pub analysis: Option<toml::Table>,
    /// Materials of the sections (read by `ols_structures_fe`).
    #[serde(rename = "material", default)]
    pub materials: Vec<toml::Table>,
    /// Member sections (read by `ols_structures_fe`).
    #[serde(rename = "section", default)]
    pub sections: Vec<toml::Table>,
    /// The model's own load case (read by `ols_structures_fe`).
    #[serde(rename = "load", default)]
    pub loads: Vec<toml::Table>,
    /// Load-case recipes (read by the structure application,
    /// `ols-structure-loads`).
    #[serde(rename = "load_case", default)]
    pub load_cases: Vec<toml::Table>,
    /// Mirror symmetry of the joints and members, kept while editing
    /// (read by the structure application, `ols-structure-edit`).
    #[serde(rename = "symmetry", default)]
    pub symmetry: Vec<toml::Table>,
    /// Body generators and their inputs, to build the body again (read by
    /// the structure application, `ols-structure-edit`); what they wrote
    /// are ordinary joints and members with a `made_by`.
    #[serde(rename = "generator", default)]
    pub generators: Vec<toml::Table>,
    /// What it can carry (structure checks).
    #[serde(default)]
    pub capacity: Option<Capacity>,
    /// The wind on the structure itself (structure loads).
    #[serde(default)]
    pub wind: Option<StructureWind>,
    /// A separate structure model for the finite-element check (`.olsfe`
    /// file, relative to this file's folder) instead of the members here.
    #[serde(default)]
    pub model: Option<String>,
    /// The pole: `pole = "id"` (a `.olsp` file in this file's folder,
    /// level 3) or a `[pole]` table ([`InlinePole`], levels 1 to 3).
    #[serde(default)]
    pub pole: Option<PoleRef>,
    /// Built from a pylon template: its id, the role and the values
    /// ([`crate::templates::recorded`]), to build it again.
    #[serde(default)]
    pub template: Option<toml::Table>,
}

/// An attachment of a placed structure, in global coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedAttachment {
    /// Set number.
    pub set: u32,
    /// Phase number.
    pub phase: u32,
    /// Label.
    pub label: String,
    /// Insulator attachment point.
    pub insulator_point: [f64; 3],
    /// Wire (clamp) point in still air.
    pub wire_point: [f64; 3],
    /// Insulator assembly id, if any.
    pub insulator: Option<String>,
    /// Insulator weight, N.
    pub insulator_weight: f64,
    /// V-string: the two leg attachment points, at ±`half_width` along the
    /// structure's local x axis from the insulator point (the centre).
    pub legs: Option<[[f64; 3]; 2]>,
    /// Label of the string's top (insulator attachment point).
    pub top_label: String,
    /// Label of the string's bottom (wire attachment point).
    pub bottom_label: String,
    /// The string is a strain string (a free member along its conductor).
    pub strain: bool,
}

/// A set of structure types, by id.
pub type StructureLibrary = Library<StructureType>;

impl LibraryItem for StructureType {
    const EXTENSION: &'static str = STRUCTURE_FILE_EXTENSION;
    const KIND: &'static str = "structure type";

    fn id(&self) -> &str {
        &self.id
    }

    /// Points at a joint or along a member get their position.
    fn prepare(&mut self) -> Result<(), String> {
        let joint = |id: &str| -> Result<[f64; 3], String> {
            self.nodes
                .iter()
                .find(|n| n.id == id)
                .map(|n| n.point.map(|v| v.si()))
                .ok_or_else(|| format!("no joint \"{id}\""))
        };
        let mut placed = Vec::new();
        for p in &self.points {
            let bad = |m: String| format!("point {}: {m}", p.label);
            let at = match (&p.point, &p.node, &p.member, p.at) {
                (Some(_), None, None, None) => None,
                (None, Some(n), None, None) => Some(joint(n).map_err(bad)?),
                (None, None, Some(m), Some(f)) => {
                    let member = self
                        .members
                        .iter()
                        .find(|x| &x.id == m)
                        .ok_or_else(|| bad(format!("no member \"{m}\"")))?;
                    if !(0.0..=1.0).contains(&f) {
                        return Err(bad("at must be between 0 and 1".into()));
                    }
                    let (a, b) = (
                        joint(&member.nodes[0]).map_err(bad)?,
                        joint(&member.nodes[1]).map_err(bad)?,
                    );
                    Some([0, 1, 2].map(|k| a[k] + f * (b[k] - a[k])))
                }
                _ => {
                    return Err(bad(
                        "give its point, a node, or a member and at (one of them)".into(),
                    ));
                }
            };
            placed.push(at);
        }
        for (p, at) in self.points.iter_mut().zip(placed) {
            if let Some(at) = at {
                p.point = Some(at.map(Length::from_meters));
            }
        }
        Ok(())
    }

    fn check(&self) -> Result<(), String> {
        ols_core::files::check_header(
            self.format.as_deref(),
            self.version,
            STRUCTURE_FILE_FORMAT,
            STRUCTURE_FILE_VERSION,
            "structure",
        )?;
        self.check_members()?;
        let mut seen = std::collections::BTreeSet::new();
        for a in &self.attachments {
            if !seen.insert((a.set, a.phase)) {
                return Err(format!(
                    "duplicate attachment set {} phase {}",
                    a.set, a.phase
                ));
            }
            if a.point[2].si() < self.foot_level() {
                return Err(format!(
                    "attachment set {} phase {} is below ground",
                    a.set, a.phase
                ));
            }
            if let Some(st) = &a.string {
                if a.insulator.is_some() {
                    return Err(format!(
                        "attachment set {} phase {}: give an insulator or a string, not both",
                        a.set, a.phase
                    ));
                }
                if !(st.length.si() > 0.0 && st.length.si() < 50.0) || st.weight.si() < 0.0 {
                    return Err(format!(
                        "attachment set {} phase {}: string length must be between 0 and 50 m, weight ≥ 0",
                        a.set, a.phase
                    ));
                }
            }
        }
        self.check_strings()?;
        self.check_extensions()?;
        if let Some(w) = &self.wind {
            w.check()?;
        }
        if let Some(c) = &self.capacity {
            c.check()?;
            let mut points = std::collections::BTreeSet::new();
            for a in &c.attachments {
                let label = self.capacity_label(a);
                if self.point_local(&label, None).is_none() {
                    return Err(format!("capacity: {} is not a point of the type", a.name()));
                }
                if !points.insert(label) {
                    return Err(format!("capacity: {} given twice", a.name()));
                }
            }
        }
        Ok(())
    }
}

/// Local → global: rotate by `rotation_deg` (local x direction from east,
/// counter-clockwise) and translate.
pub fn to_global(position: [f64; 3], rotation_deg: f64, local: [f64; 3]) -> [f64; 3] {
    let (s, c) = rotation_deg.to_radians().sin_cos();
    [
        position[0] + c * local[0] - s * local[1],
        position[1] + s * local[0] + c * local[1],
        position[2] + local[2],
    ]
}

/// Global → local (inverse of [`to_global`]).
pub fn to_local(position: [f64; 3], rotation_deg: f64, global: [f64; 3]) -> [f64; 3] {
    let (s, c) = rotation_deg.to_radians().sin_cos();
    let (dx, dy) = (global[0] - position[0], global[1] - position[1]);
    [c * dx + s * dy, -s * dx + c * dy, global[2] - position[2]]
}

impl StructureType {
    /// Attachments of this type placed at `position` with `rotation_deg` and
    /// an optional extension. Wire points come from each attachment's
    /// insulator assembly (missing assembly: the wire is clamped at the
    /// attachment point).
    pub fn place(
        &self,
        position: [f64; 3],
        rotation_deg: f64,
        extension: Option<&str>,
        insulators: &InsulatorLibrary,
    ) -> Result<Vec<PlacedAttachment>, String> {
        let dz = self.lift(extension)?;
        self.attachments
            .iter()
            .map(|a| {
                let local = [a.point[0].si(), a.point[1].si(), a.point[2].si() + dz];
                let assembly = match &a.insulator {
                    None => None,
                    Some(id) => Some(insulators.get(id).ok_or_else(|| {
                        format!("structure type {}: insulator \"{id}\" not in the library", self.id)
                    })?),
                };
                let side_sign = |a: &TypeAttachment| match a.side {
                    Some(Side::Back) => Some(-1.0),
                    Some(Side::Ahead) => Some(1.0),
                    None => None,
                };
                let wire_local = match (assembly, &a.string) {
                    (None, Some(st)) => match side_sign(a) {
                        // Strain string: initial guess along the line (aimed
                        // and solved later, like library strain assemblies).
                        Some(sign) => [local[0], local[1] + sign * st.length.si(), local[2]],
                        None => [local[0], local[1], local[2] - st.length.si()],
                    },
                    (None, None) => local,
                    (Some(i), _) => match i.kind {
                        AssemblyKind::Suspension | AssemblyKind::VString => [local[0], local[1], local[2] - i.vertical_drop()],
                        AssemblyKind::Strain => {
                            let sign = match a.side {
                                Some(Side::Back) => -1.0,
                                Some(Side::Ahead) => 1.0,
                                None => {
                                    return Err(format!(
                                        "structure type {}: strain attachment set {} phase {} needs a side",
                                        self.id, a.set, a.phase
                                    ));
                                }
                            };
                            // Initial guess along the structure axis; `ols-line` aims it at the
                            // neighbouring span and the solver lets it find its equilibrium.
                            [local[0], local[1] + sign * i.length.si(), local[2]]
                        }
                    },
                };
                let strain = match (assembly, &a.string) {
                    (Some(i), _) => i.kind == AssemblyKind::Strain,
                    (None, Some(_)) => a.side.is_some(),
                    (None, None) => false,
                };
                let (top_label, bottom_label) = a
                    .string
                    .as_ref()
                    .map_or((String::new(), String::new()), |st| (st.top.clone(), st.bottom.clone()));
                let legs = assembly.and_then(|i| i.half_width).map(|hw| {
                    let hw = hw.si();
                    [-hw, hw].map(|dx| {
                        to_global(position, rotation_deg, [local[0] + dx, local[1], local[2]])
                    })
                });
                Ok(PlacedAttachment {
                    set: a.set,
                    phase: a.phase,
                    label: a.label.clone(),
                    insulator_point: to_global(position, rotation_deg, local),
                    wire_point: to_global(position, rotation_deg, wire_local),
                    insulator: a.insulator.clone(),
                    insulator_weight: match (assembly, &a.string) {
                        (Some(i), _) => i.weight.si(),
                        (None, Some(st)) => st.weight.si(),
                        (None, None) => 0.0,
                    },
                    legs,
                    top_label,
                    bottom_label,
                    strain,
                })
            })
            .collect()
    }
}

/// The set number of a version 1 (set, phase): the set itself for phase 1
/// (the usual case: one wire per set), else `set + 1000 · (phase + 1)` —
/// unique as long as sets stay below 1000, and the same everywhere it is
/// met (structure types, lines, project history).
pub fn set_from_v1(set: u32, phase: u32) -> u32 {
    if phase == 1 {
        set
    } else {
        set + 1000 * (phase + 1)
    }
}

/// The label of a version 1 attachment's point (its string's top label, else
/// `A<set>-<phase>`), as lines read it.
pub fn v1_point_label(a: &TypeAttachment) -> String {
    a.string
        .as_ref()
        .map(|s| s.top.clone())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| format!("A{}-{}", a.set, a.phase))
}

/// A point of a placed structure, in global coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedPoint {
    /// Label.
    pub label: String,
    /// What it is, in words.
    pub description: String,
    /// Position.
    pub position: [f64; 3],
}

/// A wire point of a placed structure, in global coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedWire {
    /// Label.
    pub label: String,
    /// What it is, in words.
    pub description: String,
    /// Position in still air.
    pub position: [f64; 3],
    /// Set number.
    pub set: Option<u32>,
    /// A weight hanging at it, N.
    pub weight: f64,
}

/// A string of a placed structure.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedString {
    /// Label.
    pub label: String,
    /// What it is, in words.
    pub description: String,
    /// Top label.
    pub top: String,
    /// Wire point label.
    pub wire: String,
    /// Insulator assembly id.
    pub insulator: Option<String>,
    /// Weight, N.
    pub weight: f64,
    /// Compression it may take, N.
    pub compression: Option<f64>,
    /// Area the wind blows on, m² (0: its insulator's, if any).
    pub wind_area: f64,
    /// Axial stiffness EA, N (its insulator's).
    pub stiffness: Option<f64>,
    /// A strain string.
    pub strain: bool,
    /// Its own limits (not checked by OLS CAD); its insulator's are the
    /// assembly's ([`crate::InsulatorAssembly::limits`]), which a line
    /// takes where the string gives none (`ols_line::Line::resolve`).
    pub limits: crate::InsulatorLimits,
}

/// The points, strings and wire points of a placed structure (version 2
/// items).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlacedStrings {
    /// Points.
    pub points: Vec<PlacedPoint>,
    /// Strings.
    pub strings: Vec<PlacedString>,
    /// Wire points.
    pub wires: Vec<PlacedWire>,
}

impl StructureType {
    /// Checks the version 2 items: labels unique, tops and wire points
    /// there, a length for strings without an insulator.
    fn check_strings(&self) -> Result<(), String> {
        let mut labels = std::collections::BTreeSet::new();
        for label in self
            .points
            .iter()
            .map(|p| &p.label)
            .chain(self.wires.iter().map(|w| &w.label))
        {
            if label.is_empty() || !labels.insert(label.as_str()) {
                return Err(format!(
                    "point or wire point \"{label}\": empty or given twice"
                ));
            }
        }
        let mut sets = std::collections::BTreeSet::new();
        for w in &self.wires {
            if let Some(set) = w.set
                && !sets.insert(set)
            {
                return Err(format!("two wire points with set {set}"));
            }
        }
        let mut strings = std::collections::BTreeSet::new();
        for st in &self.strings {
            let bad = |m: &str| Err(format!("string {}: {m}", st.label));
            if st.label.is_empty() || !strings.insert(st.label.as_str()) {
                return bad("empty label or given twice");
            }
            if !labels.contains(st.top.as_str()) {
                return bad(&format!("no point \"{}\" for its top", st.top));
            }
            if !self.wires.iter().any(|w| w.label == st.wire) {
                return bad(&format!("no wire point \"{}\"", st.wire));
            }
            match (&st.insulator, st.length) {
                (Some(_), Some(_)) => return bad("give an insulator or a length, not both"),
                (None, Some(l)) if !(l.si() > 0.0 && l.si() < 50.0) => {
                    return bad("length must be between 0 and 50 m");
                }
                _ => {}
            }
            if st
                .wind_area
                .is_some_and(|w| w.si().is_nan() || w.si() < 0.0)
            {
                return bad("wind area must be ≥ 0");
            }
            if st.weight.is_some_and(|w| w.si() < 0.0)
                || st
                    .compression
                    .is_some_and(|c| c.si().is_nan() || c.si() < 0.0)
            {
                return bad("weight and compression must be ≥ 0");
            }
            if let Err(m) = st.limits().check() {
                return bad(&m);
            }
        }
        Ok(())
    }

    /// The label a capacity entry refers to (version 1: the attachment's
    /// point label).
    pub fn capacity_label(&self, c: &AttachmentCapacity) -> String {
        if let Some(p) = &c.point {
            return p.clone();
        }
        self.attachments
            .iter()
            .find(|a| Some(a.set) == c.set && Some(a.phase) == c.phase)
            .map(v1_point_label)
            .unwrap_or_else(|| format!("A{}-{}", c.set.unwrap_or(0), c.phase.unwrap_or(0)))
    }

    /// The wire ends of the type placed at `position` with `rotation_deg`
    /// and an optional extension: for each set, its wire point (still air)
    /// and whether it hangs on strain strings only (version 1 attachments
    /// and version 2 wire points together).
    pub fn wire_ends(
        &self,
        position: [f64; 3],
        rotation_deg: f64,
        extension: Option<&str>,
        insulators: &InsulatorLibrary,
    ) -> Result<Vec<(u32, [f64; 3], bool)>, String> {
        let mut out: Vec<(u32, [f64; 3], bool)> = self
            .place(position, rotation_deg, extension, insulators)?
            .into_iter()
            .map(|a| (set_from_v1(a.set, a.phase), a.wire_point, a.strain))
            .collect();
        let placed = self.place_strings(position, rotation_deg, extension, insulators)?;
        for w in &placed.wires {
            let Some(set) = w.set else { continue };
            let on: Vec<&PlacedString> = placed
                .strings
                .iter()
                .filter(|x| x.wire == w.label)
                .collect();
            out.push((
                set,
                w.position,
                !on.is_empty() && on.iter().all(|x| x.strain),
            ));
        }
        Ok(out)
    }

    /// The sets of the type's wire points (version 1 attachments: their
    /// [`set_from_v1`]), in order.
    pub fn sets(&self) -> Vec<u32> {
        let mut sets: Vec<u32> = self
            .attachments
            .iter()
            .map(|a| set_from_v1(a.set, a.phase))
            .chain(self.wires.iter().filter_map(|w| w.set))
            .collect();
        sets.sort_unstable();
        sets.dedup();
        sets
    }

    /// The allowable loads at the point labelled `label` (Method 2).
    pub fn capacity_at(&self, label: &str) -> Option<&AttachmentCapacity> {
        self.capacity
            .as_ref()?
            .attachments
            .iter()
            .find(|c| self.capacity_label(c) == label)
    }

    /// A point's position above the feet (a version 2 point, or a version
    /// 1 attachment's point, by [`v1_point_label`]), raised by the
    /// extension.
    pub fn point_local(&self, label: &str, extension: Option<&str>) -> Option<[f64; 3]> {
        let dz = self.lift(extension).ok()?;
        self.points
            .iter()
            .find(|p| p.label == label)
            .map(TypePoint::position)
            .or_else(|| {
                self.attachments
                    .iter()
                    .find(|a| v1_point_label(a) == label)
                    .map(|a| a.point)
            })
            .map(|p| [p[0].si(), p[1].si(), p[2].si() + dz])
    }

    /// The version 2 points, strings and wire points placed at `position`
    /// with `rotation_deg` and an optional extension. A wire point without
    /// a position hangs below its one string (a strain string with a side:
    /// along the line), or below the middle of its strings' tops where
    /// their lengths meet.
    pub fn place_strings(
        &self,
        position: [f64; 3],
        rotation_deg: f64,
        extension: Option<&str>,
        insulators: &InsulatorLibrary,
    ) -> Result<PlacedStrings, String> {
        let dz = self.lift(extension)?;
        let err = |m: String| format!("structure type {}: {m}", self.id);
        let lift = |p: &[Length; 3]| [p[0].si(), p[1].si(), p[2].si() + dz];
        let mut local: std::collections::BTreeMap<&str, [f64; 3]> = self
            .points
            .iter()
            .map(|p| (p.label.as_str(), lift(&p.position())))
            .chain(
                self.wires
                    .iter()
                    .filter_map(|w| Some((w.label.as_str(), lift(w.point.as_ref()?)))),
            )
            .collect();
        let assembly = |st: &TypeString| -> Result<Option<&crate::InsulatorAssembly>, String> {
            match &st.insulator {
                None => Ok(None),
                Some(id) => insulators
                    .get(id)
                    .map(Some)
                    .ok_or_else(|| err(format!("insulator \"{id}\" not in the library"))),
            }
        };
        // Wire points without a position, from the tops placed: each pass
        // places those whose strings' tops are all placed.
        for _ in 0..=self.wires.len() {
            let mut progress = false;
            for w in &self.wires {
                if local.contains_key(w.label.as_str()) {
                    continue;
                }
                let on: Vec<&TypeString> =
                    self.strings.iter().filter(|s| s.wire == w.label).collect();
                if on.is_empty() {
                    return Err(err(format!(
                        "wire point {}: no position and no string",
                        w.label
                    )));
                }
                let Some(tops) = on
                    .iter()
                    .map(|s| local.get(s.top.as_str()).copied())
                    .collect::<Option<Vec<_>>>()
                else {
                    continue;
                };
                let lengths = on
                    .iter()
                    .map(|s| {
                        Ok(match (assembly(s)?, s.length) {
                            (Some(a), _) => a.length.si(),
                            (None, Some(l)) => l.si(),
                            (None, None) => {
                                return Err(err(format!("string {}: no length", s.label)));
                            }
                        })
                    })
                    .collect::<Result<Vec<f64>, String>>()?;
                // One string: at its length in its direction; two or more
                // from other tops (a V): where the first two's lengths meet,
                // the point nearest where the first one's direction points.
                let far = (1..tops.len()).find(|&k| {
                    (0..3)
                        .map(|c| (tops[k][c] - tops[0][c]).powi(2))
                        .sum::<f64>()
                        > 1e-12
                });
                let at = match far {
                    None => {
                        let (angle, direction) = on[0].angles();
                        let d = local_direction(angle, direction);
                        [0, 1, 2].map(|k| tops[0][k] + lengths[0] * d[k])
                    }
                    Some(k) => {
                        let (angle, direction) = on[0].angles();
                        let d = local_direction(angle, direction);
                        let toward = [0, 1, 2].map(|c| tops[0][c] + lengths[0] * d[c]);
                        v_meeting(tops[0], lengths[0], tops[k], lengths[k], toward).ok_or_else(|| {
                            err(format!(
                                "wire point {}: its strings cannot meet (too short, or too long)",
                                w.label
                            ))
                        })?
                    }
                };
                local.insert(w.label.as_str(), at);
                progress = true;
            }
            if !progress {
                break;
            }
        }
        if let Some(w) = self
            .wires
            .iter()
            .find(|w| !local.contains_key(w.label.as_str()))
        {
            return Err(err(format!(
                "wire point {}: its strings hang from points without a position",
                w.label
            )));
        }
        let global = |label: &str| to_global(position, rotation_deg, local[label]);
        // A V-string assembly's weight is shared by its legs.
        let legs = |wire: &str| {
            self.strings
                .iter()
                .filter(|s| s.wire == wire)
                .count()
                .max(1)
        };
        Ok(PlacedStrings {
            points: self
                .points
                .iter()
                .map(|p| PlacedPoint {
                    description: p.description.clone(),
                    label: p.label.clone(),
                    position: global(&p.label),
                })
                .collect(),
            wires: self
                .wires
                .iter()
                .map(|w| PlacedWire {
                    description: w.description.clone(),
                    label: w.label.clone(),
                    position: global(&w.label),
                    set: w.set,
                    weight: w.weight.map_or(0.0, |f| f.si()),
                })
                .collect(),
            strings: self
                .strings
                .iter()
                .map(|s| {
                    let a = assembly(s)?;
                    let share = match a {
                        Some(a) if a.kind == AssemblyKind::VString => 1.0 / legs(&s.wire) as f64,
                        _ => 1.0,
                    };
                    Ok(PlacedString {
                        description: s.description.clone(),
                        label: s.label.clone(),
                        top: s.top.clone(),
                        wire: s.wire.clone(),
                        insulator: s.insulator.clone(),
                        weight: match (a, s.weight) {
                            (_, Some(w)) => w.si(),
                            (Some(a), None) => a.weight.si() * share,
                            (None, None) => 0.0,
                        },
                        stiffness: a.map(|a| a.axial_stiffness.si()),
                        compression: s.compression.map(|c| c.si()),
                        wind_area: s.wind_area.map_or(0.0, |w| w.si()),
                        strain: s.strain || a.is_some_and(|a| a.kind == AssemblyKind::Strain),
                        limits: s.limits(),
                    })
                })
                .collect::<Result<_, String>>()?,
        })
    }

    /// How much of the structure the file describes, from what it holds:
    /// a full design when every member has a section and a joint is
    /// supported (or a separate `model` is named), the pylon's sticks when
    /// it has members or `[[body]]` polylines that make a 3D object (not
    /// all in one plane, [`StructureType::body_is_3d`]), else a stick
    /// figure.
    ///
    /// A pole gives its own level ([`StructureType::pole_detail`]): a
    /// named `.olsp` file or a `[pole]` with everything the pole check
    /// needs a full design, a `[pole]` outline (diameters at two heights)
    /// the outline; the higher of the two counts.
    pub fn detail(&self) -> Detail {
        let pole = self.pole_detail().unwrap_or(Detail::Stick);
        let body = if self.model.is_some() || self.missing_for_design().is_empty() {
            Detail::Design
        } else if !self.members.is_empty() || self.body_is_3d() {
            Detail::Outline
        } else {
            Detail::Stick
        };
        body.max(pole)
    }

    /// The level of the pole (none: no pole): a `.olsp` file named, or a
    /// `[pole]` the pole check can read, a full design; a `[pole]` outline
    /// in 3D (diameters at two heights at least) the outline; else a
    /// stick figure.
    pub fn pole_detail(&self) -> Option<Detail> {
        Some(match self.pole.as_ref()? {
            PoleRef::File(_) => Detail::Design,
            PoleRef::Inline(p) if p.missing_for_design().is_empty() => Detail::Design,
            PoleRef::Inline(p) if p.is_3d() => Detail::Outline,
            PoleRef::Inline(_) => Detail::Stick,
        })
    }

    /// The members are a full design (every member with a section, a
    /// supported joint, the extensions' bodies too): the structure's own
    /// finite-element model can be built.
    pub fn members_designed(&self) -> bool {
        self.missing_for_design().is_empty()
    }

    /// Whether the `[[body]]` polylines describe a 3D object: their
    /// vertices, all together, are not in one plane (within 1 mm, or a
    /// millionth of the body's size when that is more). A face drawn in a
    /// plane stays a stick figure.
    pub fn body_is_3d(&self) -> bool {
        let points: Vec<[f64; 3]> = self
            .body
            .iter()
            .flat_map(|o| o.points.iter().map(|p| p.map(|v| v.si())))
            .collect();
        let Some(&first) = points.first() else {
            return false;
        };
        let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
        let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let cross = |a: [f64; 3], b: [f64; 3]| {
            [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]
        };
        let farthest = |f: &dyn Fn([f64; 3]) -> f64| {
            points
                .iter()
                .map(|p| (f(*p), *p))
                .max_by(|a, b| a.0.total_cmp(&b.0))
                .unwrap_or((0.0, first))
        };
        // Three points far apart span the plane, if there is one.
        let (size2, a) = farthest(&|p| dot(sub(p, first), sub(p, first)));
        let tol = 1e-3_f64.max(1e-6 * size2.sqrt());
        let axis = sub(a, first);
        let (area, b) = farthest(&|p| {
            let c = cross(axis, sub(p, first));
            dot(c, c)
        });
        let length = size2.sqrt();
        if length <= tol || area.sqrt() / length <= tol {
            // One point, or all on one line.
            return false;
        }
        let n = cross(axis, sub(b, first));
        let n_len = dot(n, n).sqrt();
        points
            .iter()
            .any(|p| (dot(sub(*p, first), n) / n_len).abs() > tol)
    }

    /// What the members still lack for a full design (empty: nothing).
    pub fn missing_for_design(&self) -> Vec<String> {
        if self.members.is_empty() {
            return vec!["no members".into()];
        }
        let mut out = Vec::new();
        let bare = self.members.iter().filter(|m| m.section.is_none()).count();
        if bare > 0 {
            out.push(format!("{bare} member(s) without a section"));
        }
        if !self.nodes.iter().any(|n| n.support.is_some()) {
            out.push("no supported joint".into());
        }
        // Every extension body gets its parameters too (decision 16).
        for e in self.extensions.iter().filter(|e| e.has_body()) {
            let bare = e.members.iter().filter(|m| m.section.is_none()).count();
            if bare > 0 {
                out.push(format!(
                    "extension {}: {bare} member(s) without a section",
                    e.name
                ));
            }
            if !e.nodes.iter().any(|n| n.support.is_some()) {
                out.push(format!("extension {}: no supported joint", e.name));
            }
        }
        out
    }

    /// Joints, members and the points on them: ids unique, members between
    /// two different joints. A section id may be in the section library
    /// (not read here): the structure model resolves it.
    fn check_members(&self) -> Result<(), String> {
        let mut joints = std::collections::BTreeSet::new();
        for n in &self.nodes {
            if n.id.is_empty() || !joints.insert(n.id.as_str()) {
                return Err(format!("joint \"{}\": empty or given twice", n.id));
            }
        }
        let mut members = std::collections::BTreeSet::new();
        for m in &self.members {
            let bad = |x: String| Err(format!("member {}: {x}", m.id));
            if m.id.is_empty() || !members.insert(m.id.as_str()) {
                return bad("empty id or given twice".into());
            }
            for n in &m.nodes {
                if !joints.contains(n.as_str()) {
                    return bad(format!("no joint \"{n}\""));
                }
            }
            if m.nodes[0] == m.nodes[1] {
                return bad("both ends at the same joint".into());
            }
        }
        Ok(())
    }

    /// The level of the feet in local z ([`StructureType::foot`], default
    /// 0).
    pub fn foot_level(&self) -> f64 {
        self.foot.map_or(0.0, |f| f.si())
    }

    /// How far a placed point rises above its local z: the extension's
    /// `dz` less the level of the feet, so the feet stand on the ground.
    pub fn lift(&self, extension: Option<&str>) -> Result<f64, String> {
        let dz = match extension {
            None => 0.0,
            Some(name) => self
                .extensions
                .iter()
                .find(|e| e.name == name)
                .map(|e| e.dz.si())
                .ok_or_else(|| format!("structure type {}: no extension \"{name}\"", self.id))?,
        };
        Ok(dz - self.foot_level())
    }

    /// The stick drawing above the feet: the members and the `body`
    /// polylines (points above the feet raised by the extension), or a
    /// mast from the centre point to the top and a crossarm from the mast
    /// to every attachment point.
    ///
    /// With an extension that has a body, the members include its piece
    /// and the whole rises by its `dz` (the piece stands on the ground);
    /// leg extensions lower their joints ([`StructureType::extended`]; an
    /// extension that does not resolve is drawn without its body).
    pub fn stick(&self, extension: Option<&str>) -> Result<Vec<Vec<[f64; 3]>>, String> {
        let dz = self.lift(extension)?;
        if !self.body.is_empty() || !self.members.is_empty() {
            let foot = self.foot_level();
            let with = extension
                .and_then(|n| self.extension(n))
                .filter(|e| e.changes_model());
            let whole = with.is_some_and(|e| e.has_body());
            let extended = with
                .and_then(|e| self.extended(Some(&e.name)).ok())
                .unwrap_or_else(|| self.extended(None).expect("no extension"));
            let up = |p: &[Length; 3]| {
                let z = p[2].si();
                [
                    p[0].si(),
                    p[1].si(),
                    if z > foot || whole { z + dz } else { z - foot },
                ]
            };
            let joint = |id: &str| {
                extended
                    .nodes
                    .iter()
                    .find(|n| n.id == id)
                    .map(|n| up(&n.point))
            };
            return Ok(extended
                .members
                .iter()
                .filter_map(|m| Some(vec![joint(&m.nodes[0])?, joint(&m.nodes[1])?]))
                .chain(self.body.iter().map(|o| o.points.iter().map(up).collect()))
                .collect());
        }
        let points: Vec<[f64; 3]> = self
            .attachments
            .iter()
            .map(|a| a.point)
            .chain(self.points.iter().map(TypePoint::position))
            .map(|p| [p[0].si(), p[1].si(), p[2].si() + dz])
            .collect();
        let highest = points.iter().map(|p| p[2]).fold(0.0, f64::max);
        let top = self.height.map_or(highest, |h| (h.si() + dz).max(highest));
        let mut lines = vec![vec![[0.0, 0.0, 0.0], [0.0, 0.0, top]]];
        for p in points {
            let arm = vec![[0.0, 0.0, p[2]], p];
            if p[0].hypot(p[1]) > 1e-9 && !lines.contains(&arm) {
                lines.push(arm);
            }
        }
        Ok(lines)
    }

    /// The stick drawing placed at `position` with `rotation_deg`.
    pub fn place_stick(
        &self,
        position: [f64; 3],
        rotation_deg: f64,
        extension: Option<&str>,
    ) -> Result<Vec<Vec<[f64; 3]>>, String> {
        Ok(self
            .stick(extension)?
            .into_iter()
            .map(|l| {
                l.into_iter()
                    .map(|p| to_global(position, rotation_deg, p))
                    .collect()
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::insulators::InsulatorAssembly;
    use std::path::Path;

    fn insulators() -> InsulatorLibrary {
        let mut lib = InsulatorLibrary::default();
        for text in [
            "id = \"I5\"\nname = \"I\"\nkind = \"suspension\"\nlength = \"5 m\"\nweight = \"1 kN\"",
            "id = \"S6\"\nname = \"S\"\nkind = \"strain\"\nlength = \"6 m\"\nweight = \"2 kN\"",
            "id = \"V5\"\nname = \"V\"\nkind = \"v_string\"\nlength = \"5 m\"\nhalf_width = \"3 m\"",
        ] {
            lib.insert(
                Path::new("t.olsi"),
                InsulatorAssembly::parse(Path::new("t.olsi"), text).unwrap(),
            )
            .unwrap();
        }
        lib
    }

    fn tower() -> StructureType {
        StructureType::parse(
            Path::new("t.olss"),
            r#"
            id = "T"
            name = "test"
            kind = "lattice"
            [[attachment]]
            set = 1
            phase = 1
            point = ["-10 m", "0 m", "40 m"]
            insulator = "I5"
            [[attachment]]
            set = 2
            phase = 1
            point = ["10 m", "1 m", "40 m"]
            insulator = "S6"
            side = "ahead"
            [[extension]]
            name = "+3"
            dz = "3 m"
            "#,
        )
        .unwrap()
    }

    #[test]
    fn detail_levels() {
        let parse = |body: &str| {
            let text = format!("id = \"T\"\nname = \"t\"\nkind = \"lattice\"\n{body}");
            StructureType::parse(Path::new("t.olss"), &text).map(|t| t.detail())
        };
        assert_eq!(parse("").unwrap(), Detail::Stick);
        // A level stated by an older file is read, but the content decides.
        assert_eq!(parse("detail = 3\n").unwrap(), Detail::Stick);
        // Body polylines: level 2 only when they make a 3D object; a line
        // or a face in one plane (a skew one too) stays level 1.
        let body = |polylines: &[&str]| {
            polylines
                .iter()
                .map(|p| format!("[[body]]\npoints = [{p}]\n"))
                .collect::<String>()
        };
        let mast = "[\"0 m\", \"0 m\", \"0 m\"], [\"0 m\", \"0 m\", \"30 m\"]";
        assert_eq!(parse(&body(&[mast])).unwrap(), Detail::Stick);
        let face = "[\"-4 m\", \"0 m\", \"0 m\"], [\"0 m\", \"0 m\", \"30 m\"], [\"4 m\", \"0 m\", \"0 m\"]";
        let arm = "[\"-6 m\", \"0 m\", \"20 m\"], [\"6 m\", \"0.0005 m\", \"20 m\"]";
        assert_eq!(parse(&body(&[face, arm])).unwrap(), Detail::Stick);
        let skew =
            "[\"0 m\", \"0 m\", \"0 m\"], [\"3 m\", \"3 m\", \"0 m\"], [\"3 m\", \"3 m\", \"9 m\"]";
        assert_eq!(parse(&body(&[skew])).unwrap(), Detail::Stick);
        let side = "[\"0 m\", \"-4 m\", \"0 m\"], [\"0 m\", \"0 m\", \"30 m\"], [\"0 m\", \"4 m\", \"0 m\"]";
        assert_eq!(parse(&body(&[face, side])).unwrap(), Detail::Outline);
        let off = "[\"-6 m\", \"0 m\", \"20 m\"], [\"6 m\", \"0.01 m\", \"20 m\"]";
        assert_eq!(parse(&body(&[face, off])).unwrap(), Detail::Outline);
        assert_eq!(parse("model = \"t.olsfe\"\n").unwrap(), Detail::Design);
        // Joints and members: sticks; with sections and a support: a design.
        let joints = "[[node]]\nid = \"B\"\npoint = [\"0 m\", \"0 m\", \"0 m\"]\n\
                      [[node]]\nid = \"T\"\npoint = [\"0 m\", \"0 m\", \"30 m\"]\n";
        let member = "[[member]]\nid = \"M\"\nnodes = [\"B\", \"T\"]\n";
        assert_eq!(
            parse(&format!("{joints}{member}")).unwrap(),
            Detail::Outline
        );
        let section = "[[section]]\nid = \"L\"\nmaterial = \"steel\"\narea = \"10 cm2\"\n";
        let unsupported = format!("{joints}{section}{member}section = \"L\"\n");
        let t = StructureType::parse(
            Path::new("t.olss"),
            &format!("id = \"T\"\nname = \"t\"\nkind = \"lattice\"\n{unsupported}"),
        )
        .unwrap();
        assert_eq!(t.detail(), Detail::Outline);
        assert_eq!(
            t.missing_for_design(),
            vec!["no supported joint".to_string()]
        );
        let design = unsupported.replacen(
            "point = [\"0 m\", \"0 m\", \"0 m\"]\n",
            "point = [\"0 m\", \"0 m\", \"0 m\"]\nsupport = \"fixed\"\n",
            1,
        );
        assert_eq!(parse(&design).unwrap(), Detail::Design);
        // Members are drawn as sticks.
        assert_eq!(
            t.stick(None).unwrap(),
            vec![vec![[0.0; 3], [0.0, 0.0, 30.0]]]
        );
        // Unknown joint, a member on one joint: refused. A section not in
        // the file may be in the section library.
        for bad in [
            "[[member]]\nid = \"M\"\nnodes = [\"B\", \"X\"]\n",
            "[[member]]\nid = \"M\"\nnodes = [\"B\", \"B\"]\n",
        ] {
            assert!(parse(&format!("{joints}{bad}")).is_err(), "{bad}");
        }
        let library = "[[member]]\nid = \"M\"\nnodes = [\"B\", \"T\"]\nsection = \"Q\"\n";
        assert!(parse(&format!("{joints}{library}")).is_ok());
        assert_eq!(Detail::Outline.level(), 2);
    }

    /// Version 3: member groups, what a generator made, symmetry and the
    /// generators' inputs (kept for the structure application); older
    /// versions read, a newer one refused.
    #[test]
    fn version_3_groups_generators_and_symmetry() {
        let text = |version: u32| {
            format!(
                "format = \"ols-structure\"\nversion = {version}\n\
                 id = \"T\"\nname = \"t\"\nkind = \"lattice\"\n\
                 [[symmetry]]\nmirror = \"xy\"\n\
                 [[generator]]\nid = \"body\"\nkind = \"panels\"\n\
                 [[node]]\nid = \"B\"\npoint = [\"0 m\", \"0 m\", \"0 m\"]\nmade_by = \"body\"\n\
                 [[node]]\nid = \"T\"\npoint = [\"0 m\", \"0 m\", \"30 m\"]\n\
                 [[member]]\nid = \"M\"\nnodes = [\"B\", \"T\"]\ngroup = \"body-leg\"\nmade_by = \"body\"\n"
            )
        };
        let t = StructureType::parse(Path::new("t.olss"), &text(3)).unwrap();
        assert_eq!(t.members[0].group.as_deref(), Some("body-leg"));
        assert_eq!(t.members[0].made_by.as_deref(), Some("body"));
        assert_eq!(t.nodes[0].made_by.as_deref(), Some("body"));
        assert_eq!((t.symmetry.len(), t.generators.len()), (1, 1));
        assert_eq!(t.detail(), Detail::Outline);
        for older in [1, 2] {
            assert!(StructureType::parse(Path::new("t.olss"), &text(older)).is_ok());
        }
        let newer = StructureType::parse(Path::new("t.olss"), &text(STRUCTURE_FILE_VERSION + 1))
            .unwrap_err();
        assert!(newer.message.contains("newer"), "{}", newer.message);
    }

    /// A string's limits (version 4, not checked by OLS CAD): read with
    /// units, absent staying absent; placed with its own only (its
    /// insulator's are the assembly's); bad values refused.
    #[test]
    fn version_4_string_limits() {
        let text = "format = \"ols-structure\"\nversion = 4\nid = \"T\"\nname = \"t\"\n\
            kind = \"lattice\"\n\
            [[point]]\nlabel = \"A\"\npoint = [\"-6 m\", \"0 m\", \"30 m\"]\n\
            [[point]]\nlabel = \"B\"\npoint = [\"6 m\", \"0 m\", \"30 m\"]\n\
            [[wire]]\nlabel = \"W1\"\nset = 1\n\
            [[wire]]\nlabel = \"W2\"\nset = 2\n\
            [[string]]\nlabel = \"S1\"\ntop = \"A\"\nwire = \"W1\"\ninsulator = \"I5\"\n\
            max_load = \"120 kN\"\nmax_swing_across = \"40 deg\"\n\
            [[string]]\nlabel = \"S2\"\ntop = \"B\"\nwire = \"W2\"\nlength = \"3 m\"\n";
        let t = StructureType::parse(Path::new("t.olss"), text).unwrap();
        let l = t.strings[0].limits();
        assert_eq!(l.max_load, Some(120_000.0));
        assert!((l.max_swing_across_deg.unwrap() - 40.0).abs() < 1e-9);
        assert_eq!((l.max_swing_deg, l.max_swing_along_deg), (None, None));
        assert!(t.strings[1].limits().is_empty());
        let placed = t.place_strings([0.0; 3], 0.0, None, &insulators()).unwrap();
        assert_eq!(placed.strings[0].limits, l);
        assert!(placed.strings[1].limits.is_empty());
        for (from, to) in [("120 kN", "-1 kN"), ("40 deg", "181 deg")] {
            let worse = text.replace(from, to);
            assert!(
                StructureType::parse(Path::new("t.olss"), &worse).is_err(),
                "{to}"
            );
        }
        let typo = text.replace("max_swing_across", "max_swing_sideways");
        assert!(StructureType::parse(Path::new("t.olss"), &typo).is_err());
    }

    /// Points at a joint or along a member follow it.
    #[test]
    fn points_on_joints_and_members() {
        let text = "id = \"T\"\nname = \"t\"\nkind = \"lattice\"\n\
            [[node]]\nid = \"N1\"\npoint = [\"0 m\", \"0 m\", \"30 m\"]\n\
            [[node]]\nid = \"N2\"\npoint = [\"-10 m\", \"0 m\", \"30 m\"]\n\
            [[member]]\nid = \"arm\"\nnodes = [\"N1\", \"N2\"]\n\
            [[point]]\nlabel = \"tip\"\nnode = \"N2\"\n\
            [[point]]\nlabel = \"mid\"\nmember = \"arm\"\nat = 0.25\n";
        let t = StructureType::parse(Path::new("t.olss"), text).unwrap();
        assert_eq!(t.point_local("tip", None), Some([-10.0, 0.0, 30.0]));
        assert_eq!(t.point_local("mid", None), Some([-2.5, 0.0, 30.0]));
        for bad in [
            "[[point]]\nlabel = \"x\"\nnode = \"N9\"\n",
            "[[point]]\nlabel = \"x\"\nmember = \"arm\"\nat = 1.5\n",
            "[[point]]\nlabel = \"x\"\nnode = \"N1\"\npoint = [\"0 m\", \"0 m\", \"1 m\"]\n",
            "[[point]]\nlabel = \"x\"\n",
        ] {
            assert!(
                StructureType::parse(Path::new("t.olss"), &format!("{text}{bad}")).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn rotation_round_trip_and_axes() {
        // Line heading east: local y (ahead) = east, local x (right) = south,
        // i.e. local x points at -90°.
        let g = to_global([100.0, 200.0, 0.0], -90.0, [2.0, 5.0, 1.0]);
        assert!((g[0] - 105.0).abs() < 1e-12 && (g[1] - 198.0).abs() < 1e-12);
        let l = to_local([100.0, 200.0, 0.0], -90.0, g);
        assert!(
            (l[0] - 2.0).abs() < 1e-12 && (l[1] - 5.0).abs() < 1e-12 && (l[2] - 1.0).abs() < 1e-12
        );
    }

    #[test]
    fn placement_with_insulators_and_extension() {
        let placed = tower()
            .place([0.0, 0.0, 500.0], -90.0, Some("+3"), &insulators())
            .unwrap();
        // Suspension: wire 5 m below the attachment point (43 m with extension).
        assert_eq!(placed[0].insulator_point[2], 543.0);
        assert_eq!(placed[0].wire_point[2], 538.0);
        assert_eq!(placed[0].insulator_weight, 1000.0);
        // Strain, ahead: 6 m along the line (east when heading east).
        let d = [
            placed[1].wire_point[0] - placed[1].insulator_point[0],
            placed[1].wire_point[1] - placed[1].insulator_point[1],
        ];
        assert!((d[0] - 6.0).abs() < 1e-12 && d[1].abs() < 1e-12, "{d:?}");
        assert!(
            tower()
                .place([0.0; 3], 0.0, Some("nope"), &insulators())
                .is_err()
        );
    }

    #[test]
    fn v_string_legs_lie_across_the_line() {
        let mut t = tower();
        t.attachments[0].insulator = Some("V5".into());
        let placed = t
            .place([0.0, 0.0, 500.0], -90.0, None, &insulators())
            .unwrap();
        let v = &placed[0];
        // Leg depth sqrt(5² − 3²) = 4 m; legs ±3 m along local x (south here).
        assert!((v.wire_point[2] - 536.0).abs() < 1e-12);
        let legs = v.legs.unwrap();
        for (leg, dy) in legs.iter().zip([3.0, -3.0]) {
            assert!((leg[0] - v.insulator_point[0]).abs() < 1e-12);
            assert!(
                (leg[1] - v.insulator_point[1] - dy).abs() < 1e-12,
                "{leg:?}"
            );
            assert_eq!(leg[2], v.insulator_point[2]);
        }
        assert!(placed[1].legs.is_none());
    }

    /// A simple pylon: 3 crossarms each side and an earth wire on top,
    /// strings defined in the file.
    fn stick_pylon() -> StructureType {
        let mut text = String::from(
            "id = \"D\"\nname = \"double circuit\"\nkind = \"lattice\"\nheight = \"50 m\"\n",
        );
        for (set, x, z) in [(1, -9.0, 30.0), (2, -11.0, 36.0), (3, -9.0, 42.0)] {
            for (phase, sign) in [(1, 1.0), (2, -1.0)] {
                text.push_str(&format!(
                    "[[attachment]]\nset = {set}\nphase = {phase}\npoint = [\"{} m\", \"0 m\", \"{z} m\"]\nstring = {{ length = \"4 m\", weight = \"0.8 kN\", top = \"A{set}{phase}\", bottom = \"L{set}{phase}\" }}\n",
                    x * sign
                ));
            }
        }
        text.push_str(
            "[[attachment]]\nset = 9\nphase = 1\nlabel = \"earth wire\"\npoint = [\"0 m\", \"0 m\", \"50 m\"]\n",
        );
        text.push_str("[[extension]]\nname = \"+3\"\ndz = \"3 m\"\n");
        StructureType::parse(Path::new("d.olss"), &text).unwrap()
    }

    #[test]
    fn pylon_with_strings_in_its_file() {
        let t = stick_pylon();
        let placed = t
            .place(
                [100.0, 0.0, 500.0],
                -90.0,
                None,
                &InsulatorLibrary::default(),
            )
            .unwrap();
        // Strings hang 4 m, with their weight and labels.
        let a = &placed[0];
        assert_eq!(a.insulator_point[2] - a.wire_point[2], 4.0);
        assert_eq!(a.insulator_weight, 800.0);
        assert_eq!(
            (a.top_label.as_str(), a.bottom_label.as_str()),
            ("A11", "L11")
        );
        assert!(!a.strain && a.insulator.is_none());
        // The earth wire is clamped at its point (no string).
        let e = placed.iter().find(|p| p.set == 9).unwrap();
        assert_eq!(e.insulator_point, e.wire_point);
        // A strain string with a side lies along the line.
        let mut t2 = t.clone();
        t2.attachments[0].side = Some(Side::Ahead);
        let s = &t2
            .place([0.0; 3], 0.0, None, &InsulatorLibrary::default())
            .unwrap()[0];
        assert!(s.strain && (s.wire_point[1] - s.insulator_point[1] - 4.0).abs() < 1e-12);
        // Insulator and string together: refused.
        let mut bad = t.clone();
        bad.attachments[0].insulator = Some("I5".into());
        assert!(bad.check().is_err());
    }

    #[test]
    fn stick_drawing() {
        let t = stick_pylon();
        // Mast to 50 m and six crossarms (the earth wire sits on the mast).
        let lines = t.stick(None).unwrap();
        assert_eq!(lines.len(), 7);
        assert_eq!(lines[0], vec![[0.0, 0.0, 0.0], [0.0, 0.0, 50.0]]);
        assert!(lines.contains(&vec![[0.0, 0.0, 36.0], [-11.0, 0.0, 36.0]]));
        // The extension raises everything above the ground.
        assert_eq!(t.stick(Some("+3")).unwrap()[0][1][2], 53.0);
        // A body replaces the generated sticks.
        let mut body = t.clone();
        body.body = vec![Outline {
            points: vec![
                [Length::from_meters(-3.0), Length::ZERO, Length::ZERO],
                [Length::ZERO, Length::ZERO, Length::from_meters(50.0)],
                [Length::from_meters(3.0), Length::ZERO, Length::ZERO],
            ],
        }];
        let lines = body.stick(Some("+3")).unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0][1][2], 53.0);
        assert_eq!(lines[0][0][2], 0.0);
        // Placed: rotated and moved like the attachments.
        let placed = t.place_stick([100.0, 50.0, 500.0], 90.0, None).unwrap();
        assert_eq!(placed[0][1], [100.0, 50.0, 550.0]);
    }

    #[test]
    fn a_type_drawn_from_its_arms_stands_on_its_feet() {
        // z = 0 under the lowest crossarm, the feet 30 m below it.
        let mut text = String::from(
            "id = \"F\"\nname = \"f\"\nkind = \"lattice\"\nheight = \"12 m\"\nfoot = \"-30 m\"\n",
        );
        text.push_str("[[point]]\nlabel = \"A1\"\npoint = [\"-9 m\", \"0 m\", \"0 m\"]\n");
        text.push_str("[[wire]]\nlabel = \"W1\"\nset = 1\n");
        text.push_str(
            "[[string]]\nlabel = \"S1\"\ntop = \"A1\"\nwire = \"W1\"\nlength = \"4 m\"\n",
        );
        text.push_str("[[extension]]\nname = \"+3\"\ndz = \"3 m\"\n");
        text.push_str(
            "[[body]]\npoints = [[\"-4 m\", \"0 m\", \"-30 m\"], [\"0 m\", \"0 m\", \"12 m\"]]\n",
        );
        let t = StructureType::parse(Path::new("f.olss"), &text).unwrap();
        let none = InsulatorLibrary::default();
        let placed = t
            .place_strings([0.0, 0.0, 500.0], 0.0, None, &none)
            .unwrap();
        assert_eq!(placed.points[0].position[2], 530.0);
        assert_eq!(placed.wires[0].position[2], 526.0);
        // An extension lengthens the body: the arms rise, the feet stay.
        let placed = t
            .place_strings([0.0, 0.0, 500.0], 0.0, Some("+3"), &none)
            .unwrap();
        assert_eq!(placed.points[0].position[2], 533.0);
        let body = t.stick(Some("+3")).unwrap();
        assert_eq!((body[0][0][2], body[0][1][2]), (0.0, 45.0));
        // Generated sticks: the mast from the ground to the top.
        let mut sticks = t.clone();
        sticks.body.clear();
        assert_eq!(
            sticks.stick(None).unwrap()[0],
            vec![[0.0; 3], [0.0, 0.0, 42.0]]
        );
        // Point positions for the pole check: above the feet.
        assert_eq!(t.point_local("A1", None), Some([-9.0, 0.0, 30.0]));
    }

    #[test]
    fn rejects_bad_types() {
        let missing_side = tower();
        let mut t = missing_side.clone();
        t.attachments[1].side = None;
        assert!(
            t.place([0.0; 3], 0.0, None, &insulators())
                .unwrap_err()
                .contains("needs a side")
        );
        let text = "id = \"X\"\nname = \"x\"\nkind = \"lattice\"\n[[attachment]]\nset = 1\nphase = 1\npoint = [\"0 m\", \"0 m\", \"-1 m\"]";
        assert!(StructureType::parse(Path::new("x.olss"), text).is_err());
    }

    /// Version 2: a V with a string below and a TAK, their wire points
    /// placed from the strings (the V's tip where its legs meet, the clamp
    /// below it; the TAK's strain strings along the line), the capacity
    /// by point; the sets of both kinds of file.
    #[test]
    fn strings_hang_from_strings_in_a_type() {
        let text = r#"
            id = "VT"
            name = "V and TAK"
            kind = "lattice"
            [[point]]
            label = "L"
            point = ["-3 m", "0 m", "30 m"]
            [[point]]
            label = "R"
            point = ["3 m", "0 m", "30 m"]
            [[point]]
            label = "K"
            point = ["0 m", "0 m", "40 m"]
            [[wire]]
            label = "V"
            [[wire]]
            label = "C"
            set = 1
            [[wire]]
            label = "N"
            [[wire]]
            label = "B"
            set = 2
            [[wire]]
            label = "F"
            set = 3
            [[string]]
            label = "l"
            top = "L"
            wire = "V"
            length = "5 m"
            weight = "400 N"
            [[string]]
            label = "r"
            top = "R"
            wire = "V"
            length = "5 m"
            [[string]]
            label = "i"
            top = "V"
            wire = "C"
            insulator = "I5"
            [[string]]
            label = "k"
            top = "K"
            wire = "N"
            length = "4 m"
            [[string]]
            label = "b"
            top = "N"
            wire = "B"
            length = "2 m"
            side = "back"
            strain = true
            compression = "500 N"
            [[string]]
            label = "f"
            top = "N"
            wire = "F"
            insulator = "S6"
            side = "ahead"
            [[capacity.attachment]]
            point = "K"
            vertical = "10 kN"
        "#;
        let t = StructureType::parse(Path::new("vt.olss"), text).unwrap();
        assert_eq!(t.sets(), vec![1, 2, 3]);
        assert!(t.capacity_at("K").is_some() && t.capacity_at("L").is_none());
        let placed = t.place_strings([0.0; 3], 0.0, None, &insulators()).unwrap();
        let at = |l: &str| placed.wires.iter().find(|w| w.label == l).unwrap().position;
        let close = |a: [f64; 3], b: [f64; 3]| (0..3).all(|k| (a[k] - b[k]).abs() < 1e-9);
        // The tip 4 m below the tops (legs of 5 m, 3 m out), the clamp the
        // insulator's 5 m below it.
        assert!(close(at("V"), [0.0, 0.0, 26.0]), "{:?}", at("V"));
        assert!(close(at("C"), [0.0, 0.0, 21.0]), "{:?}", at("C"));
        assert!(close(at("N"), [0.0, 0.0, 36.0]));
        assert!(close(at("B"), [0.0, -2.0, 36.0]));
        assert!(close(at("F"), [0.0, 6.0, 36.0]));
        let string = |l: &str| placed.strings.iter().find(|x| x.label == l).unwrap();
        assert_eq!(string("l").weight, 400.0);
        assert_eq!(string("i").weight, 1000.0);
        assert!(string("f").strain && string("b").strain && !string("k").strain);
        assert_eq!(string("b").compression, Some(500.0));
        let ends = t.wire_ends([0.0; 3], 0.0, None, &insulators()).unwrap();
        assert_eq!(ends.len(), 3);
        assert!(
            ends.iter().find(|e| e.0 == 2).unwrap().2,
            "B on strain strings"
        );
        // Version 1 files still read, their sets for phases other than 1
        // their own.
        let v1 = tower();
        let mut expected: Vec<u32> = v1
            .attachments
            .iter()
            .map(|a| set_from_v1(a.set, a.phase))
            .collect();
        expected.sort_unstable();
        expected.dedup();
        assert_eq!(v1.sets(), expected);
        assert_eq!(set_from_v1(4, 2), 3004);
        // A wire point no string reaches and without a position: refused.
        let bad = text.replace("label = \"i\"", "label = \"i\"\n").replace(
            "[[string]]\n            label = \"i\"\n\n            top = \"V\"\n            wire = \"C\"\n            insulator = \"I5\"",
            "",
        );
        let t = StructureType::parse(Path::new("vt.olss"), &bad).unwrap();
        assert!(
            t.place_strings([0.0; 3], 0.0, None, &insulators())
                .unwrap_err()
                .contains("no position and no string")
        );
    }
}
