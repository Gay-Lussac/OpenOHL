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

pub mod export;
pub mod jumper;
pub mod library;
pub mod route;
pub mod solve;

pub use route::{Route, Station};

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
    pub alignment: Vec<Point>,
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
    /// Display settings (phase colours).
    #[serde(default, skip_serializing_if = "Display::is_default")]
    pub display: Display,
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
    /// Display colour (`#rrggbb`); default: the colour of the section at
    /// `from`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
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
    /// V-string: the attachment points of the two legs on the structure
    /// (the insulator point is then informative, e.g. their centre). Both
    /// legs end at the wire point; their lengths are the distances to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legs: Option<[Point; 2]>,
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

/// How a section is strung.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Stringing {
    /// Horizontal tension of every span at a temperature, insulators plumb.
    Tension {
        /// Horizontal tension per wire, N.
        horizontal_tension: f64,
        /// Cable temperature, °C.
        temperature: f64,
    },
    /// Unstressed length of each span (per wire), at a temperature.
    UnstressedLengths {
        /// One length per span, metres.
        lengths: Vec<f64>,
        /// Temperature at which the lengths apply, °C.
        temperature: f64,
    },
}

/// A point load in a span.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpanLoad {
    /// Span index (0 = first span of the section).
    pub span: usize,
    /// Horizontal fraction of the span from its back point, 0–1.
    pub fraction: f64,
    /// Downward force, N.
    pub vertical: f64,
    /// Label, e.g. `"marker ball"`.
    #[serde(default)]
    pub label: String,
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
    /// Subconductors per phase (1 = single wire). Each subconductor carries
    /// the same tension; geometry is that of one wire.
    #[serde(default = "one")]
    pub bundle: u32,
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
}

fn one() -> u32 {
    1
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

    /// Checks references and basic consistency.
    pub fn validate(&self) -> Result<(), LineError> {
        let invalid = |m: String| Err(LineError::Invalid(m));
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
            let mut seen = BTreeMap::new();
            for a in &s.attachments {
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
            if sec.bundle == 0 {
                return invalid(format!("section {}: bundle must be at least 1", sec.id));
            }
        }
        let mut jumper_ids = BTreeMap::new();
        for j in &self.jumpers {
            let bad = |m: &str| invalid(format!("jumper {}: {m}", j.id));
            if jumper_ids.insert(j.id.as_str(), ()).is_some() {
                return bad("duplicate id");
            }
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
            .join("../../examples/line/synthetic-line.ols.json");
        Line::load(&path).unwrap()
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
