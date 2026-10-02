// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Structure types (`*.olss`, one type per file) and their placement.
//!
//! **Local axes**: origin at the ground centre of the structure, **x to the
//! right of the line** looking ahead, **y ahead** along the line, z up
//! (right-handed). A placed structure has a position and a rotation: the
//! direction of its local x axis, degrees counter-clockwise from east.
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

use ols_core::units::Length;
use serde::Deserialize;

use crate::insulators::{AssemblyKind, InsulatorLibrary};
use crate::{Library, LibraryItem};

/// File extension of structure types.
pub const STRUCTURE_FILE_EXTENSION: &str = "olss";
/// Format identifier.
pub const STRUCTURE_FILE_FORMAT: &str = "ols-structure";

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
}

/// A named height extension.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Extension {
    /// Name, e.g. `"+3"`.
    pub name: String,
    /// Height added to every attachment point.
    pub dz: Length,
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
    /// Where the data comes from.
    #[serde(default)]
    pub source: Option<String>,
    /// Attachment points.
    #[serde(rename = "attachment", default)]
    pub attachments: Vec<TypeAttachment>,
    /// Height extensions.
    #[serde(rename = "extension", default)]
    pub extensions: Vec<Extension>,
    /// Outline polylines.
    #[serde(rename = "body", default)]
    pub body: Vec<Outline>,
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
}

/// A set of structure types, by id.
pub type StructureLibrary = Library<StructureType>;

impl LibraryItem for StructureType {
    const EXTENSION: &'static str = STRUCTURE_FILE_EXTENSION;
    const KIND: &'static str = "structure type";

    fn id(&self) -> &str {
        &self.id
    }

    fn check(&self) -> Result<(), String> {
        if let Some(f) = self
            .format
            .as_deref()
            .filter(|f| *f != STRUCTURE_FILE_FORMAT)
        {
            return Err(format!("format \"{f}\" is not \"{STRUCTURE_FILE_FORMAT}\""));
        }
        if self.version.is_some_and(|v| v > 1) {
            return Err("structure format version newer than supported (1)".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for a in &self.attachments {
            if !seen.insert((a.set, a.phase)) {
                return Err(format!(
                    "duplicate attachment set {} phase {}",
                    a.set, a.phase
                ));
            }
            if a.point[2].si() < 0.0 {
                return Err(format!(
                    "attachment set {} phase {} is below ground",
                    a.set, a.phase
                ));
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
        let dz = match extension {
            None => 0.0,
            Some(name) => self
                .extensions
                .iter()
                .find(|e| e.name == name)
                .map(|e| e.dz.si())
                .ok_or_else(|| format!("structure type {}: no extension \"{name}\"", self.id))?,
        };
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
                let wire_local = match assembly {
                    None => local,
                    Some(i) => match i.kind {
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
                    insulator_weight: assembly.map_or(0.0, |i| i.weight.si()),
                    legs,
                })
            })
            .collect()
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
}
