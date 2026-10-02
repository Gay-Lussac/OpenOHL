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
//!   a strain (dead-end) point.
//! - [`Section`]: one wire (or bundle) strung through attachment points, with
//!   its cable and stringing. Spans are between consecutive points.
//!
//! [`solve`] turns a section into a finite-element model (exact catenary
//! elements, insulator strings) and solves it in any weather state.

pub mod export;
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
}

/// A structure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Structure {
    /// Unique id, e.g. `"T12"`.
    pub id: String,
    /// Structure origin (centre at ground), metres.
    pub position: Point,
    /// Orientation of the structure's transverse axis, degrees from east,
    /// counter-clockwise (informative).
    #[serde(default)]
    pub orientation_deg: f64,
    /// Attachment points.
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
                if self.attachment(p).is_none() {
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
            if sec.bundle == 0 {
                return invalid(format!("section {}: bundle must be at least 1", sec.id));
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
}
