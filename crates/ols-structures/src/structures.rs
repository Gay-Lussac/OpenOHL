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

use ols_core::units::Length;
use serde::Deserialize;

use crate::insulators::{AssemblyKind, InsulatorLibrary};
use crate::{Library, LibraryItem};

/// File extension of structure types.
pub const STRUCTURE_FILE_EXTENSION: &str = "olss";
/// Format identifier.
pub const STRUCTURE_FILE_FORMAT: &str = "ols-structure";
/// Newest format version read.
pub const STRUCTURE_FILE_VERSION: u32 = 1;

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
    /// Height of the mast top for the stick drawing (default: the highest
    /// attachment point).
    #[serde(default)]
    pub height: Option<Length>,
    /// Where the data comes from.
    #[serde(default)]
    pub source: Option<String>,
    /// Attachment points.
    #[serde(rename = "attachment", default)]
    pub attachments: Vec<TypeAttachment>,
    /// Height extensions.
    #[serde(rename = "extension", default)]
    pub extensions: Vec<Extension>,
    /// Outline polylines (stick drawing); empty: mast and crossarms are
    /// drawn from the attachment points.
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

    fn check(&self) -> Result<(), String> {
        ols_core::files::check_header(
            self.format.as_deref(),
            self.version,
            STRUCTURE_FILE_FORMAT,
            STRUCTURE_FILE_VERSION,
            "structure",
        )?;
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

impl StructureType {
    fn extension_dz(&self, extension: Option<&str>) -> Result<f64, String> {
        match extension {
            None => Ok(0.0),
            Some(name) => self
                .extensions
                .iter()
                .find(|e| e.name == name)
                .map(|e| e.dz.si())
                .ok_or_else(|| format!("structure type {}: no extension \"{name}\"", self.id)),
        }
    }

    /// The stick drawing in local axes: the `body` polylines (raised by the
    /// extension above the ground), or a mast from the centre point to the
    /// top and a crossarm from the mast to every attachment point.
    pub fn stick(&self, extension: Option<&str>) -> Result<Vec<Vec<[f64; 3]>>, String> {
        let dz = self.extension_dz(extension)?;
        if !self.body.is_empty() {
            return Ok(self
                .body
                .iter()
                .map(|o| {
                    o.points
                        .iter()
                        .map(|p| {
                            let z = p[2].si();
                            [p[0].si(), p[1].si(), if z > 0.0 { z + dz } else { z }]
                        })
                        .collect()
                })
                .collect());
        }
        let highest = self
            .attachments
            .iter()
            .map(|a| a.point[2].si() + dz)
            .fold(0.0, f64::max);
        let top = self.height.map_or(highest, |h| (h.si() + dz).max(highest));
        let mut lines = vec![vec![[0.0, 0.0, 0.0], [0.0, 0.0, top]]];
        for a in &self.attachments {
            let p = [a.point[0].si(), a.point[1].si(), a.point[2].si() + dz];
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
