// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Draft structure and insulator libraries derived from an existing line.
//!
//! Each structure's attachment points are transformed to local axes with the
//! rotation given by the caller (e.g. the bisector of the line at the
//! structure); structures whose local layouts match to 1 cm become one
//! structure type. Each attachment's insulator is classified from its
//! geometry — wire point below the attachment point (swung < 45°):
//! suspension of that length; otherwise strain of that length, on the side
//! the clamp points to — and grouped to 1 cm into insulator assemblies.
//! Weights are unknown from geometry and set to 0: fill them in.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::structures::to_local;

/// One attachment of a structure to derive from (global coordinates).
#[derive(Debug, Clone, PartialEq)]
pub struct SourceAttachment {
    /// Set number.
    pub set: u32,
    /// Phase number.
    pub phase: u32,
    /// Label.
    pub label: String,
    /// Insulator attachment point.
    pub insulator_point: [f64; 3],
    /// Wire point.
    pub wire_point: [f64; 3],
    /// Whether a wire section starts or ends here. Section ends are always
    /// strain points, whatever their geometry: a steep down-lead's strain
    /// string hanging 47° below horizontal was misread as a suspension
    /// string by the geometric rule (a 5.4 m error in real data).
    pub section_end: bool,
    /// Insulator weight if known, N (0 = unknown).
    pub weight: f64,
}

/// One structure to derive from.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceStructure {
    /// Structure id.
    pub id: String,
    /// Position.
    pub position: [f64; 3],
    /// Rotation of the local x axis (right of the line), degrees CCW from east.
    pub rotation_deg: f64,
    /// Attachments.
    pub attachments: Vec<SourceAttachment>,
}

/// Result of a derivation.
#[derive(Debug, Clone, PartialEq)]
pub struct Derived {
    /// Structure types as `.olss` file contents, by id.
    pub structure_types: BTreeMap<String, String>,
    /// Insulator assemblies as `.olsi` file contents, by id.
    pub insulators: BTreeMap<String, String>,
    /// For each source structure (same order): its type id.
    pub assignments: Vec<String>,
}

/// Values closer than this are considered equal when grouping, m. Grouping
/// by tolerance rather than by rounding: rounding to fixed 1 cm buckets put
/// the same 5.025 m strain assembly into two groups (5.02 / 5.03) because of
/// floating-point noise at the bucket boundary.
const TOLERANCE: f64 = 0.01;

#[derive(Debug, Clone, PartialEq)]
struct AttachmentKey {
    set: u32,
    phase: u32,
    point: [f64; 3],
    insulator: String,
    side: Option<&'static str>,
}

impl AttachmentKey {
    fn matches(&self, other: &Self) -> bool {
        self.set == other.set
            && self.phase == other.phase
            && self.insulator == other.insulator
            && self.side == other.side
            && (0..3).all(|k| (self.point[k] - other.point[k]).abs() <= TOLERANCE)
    }
}

/// A distinct layout: its attachments (from the first member), type id,
/// member structures and labels by `"set:phase"`.
struct Layout {
    keys: Vec<AttachmentKey>,
    id: String,
    members: Vec<String>,
    labels: BTreeMap<String, String>,
}

/// An insulator assembly found so far: kind, length, weight, id.
struct Assembly {
    kind: &'static str,
    length: f64,
    weight: f64,
    id: String,
}

/// Derives draft libraries from structures. With `inline`, each structure
/// type carries its strings in its own file (length, weight, top and
/// bottom labels: a simple pylon file) and no insulator assemblies are
/// written; otherwise attachments reference insulator assemblies.
pub fn derive(structures: &[SourceStructure], inline: bool) -> Derived {
    let mut assemblies: Vec<Assembly> = Vec::new();
    let mut layouts: Vec<Layout> = Vec::new();
    let mut assignments = Vec::with_capacity(structures.len());

    let mut assembly_id = |kind: &'static str, length: f64, weight: f64| -> String {
        if let Some(a) = assemblies.iter().find(|a| {
            a.kind == kind
                && (a.length - length).abs() <= TOLERANCE
                && (a.weight - weight).abs() <= 1.0
        }) {
            return a.id.clone();
        }
        let id = match kind {
            "suspension" => format!("I-{length:.2}m"),
            _ => format!("S-{length:.2}m"),
        };
        // Distinct assemblies that print the same get a suffix.
        let n = assemblies.iter().filter(|a| a.id.starts_with(&id)).count();
        let id = if n == 0 {
            id
        } else {
            format!("{id}-{}", n + 1)
        };
        assemblies.push(Assembly {
            kind,
            length,
            weight,
            id: id.clone(),
        });
        id
    };

    for s in structures {
        let mut keys = Vec::new();
        let mut labels = BTreeMap::new();
        for a in &s.attachments {
            let d = [
                a.wire_point[0] - a.insulator_point[0],
                a.wire_point[1] - a.insulator_point[1],
                a.wire_point[2] - a.insulator_point[2],
            ];
            let horizontal = d[0].hypot(d[1]);
            let length = (horizontal.powi(2) + d[2].powi(2)).sqrt();
            let local = to_local(s.position, s.rotation_deg, a.insulator_point);
            let (insulator, side) = if length < 0.005 {
                (String::new(), None)
            } else if !a.section_end && d[2] < 0.0 && horizontal < -d[2] {
                (assembly_id("suspension", length, a.weight), None)
            } else {
                let wire_local = to_local(s.position, s.rotation_deg, a.wire_point);
                let side = if wire_local[1] >= local[1] {
                    "ahead"
                } else {
                    "back"
                };
                (assembly_id("strain", length, a.weight), Some(side))
            };
            keys.push(AttachmentKey {
                set: a.set,
                phase: a.phase,
                point: local,
                insulator,
                side,
            });
            labels.insert(format!("{}:{}", a.set, a.phase), a.label.clone());
        }
        keys.sort_by_key(|k| (k.set, k.phase));
        let found = layouts.iter_mut().find(|l| {
            l.keys.len() == keys.len() && l.keys.iter().zip(&keys).all(|(a, b)| a.matches(b))
        });
        let id = match found {
            Some(layout) => {
                layout.members.push(s.id.clone());
                layout.id.clone()
            }
            None => {
                let id = format!("type-{:02}", layouts.len() + 1);
                layouts.push(Layout {
                    keys,
                    id: id.clone(),
                    members: vec![s.id.clone()],
                    labels,
                });
                id
            }
        };
        assignments.push(id);
    }

    let mut structure_types = BTreeMap::new();
    for layout in &layouts {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "# Derived from the geometry of an existing line — check and complete."
        );
        let _ = writeln!(
            out,
            "format = \"ols-structure\"\nversion = 1\nid = \"{}\"",
            layout.id
        );
        let _ = writeln!(
            out,
            "name = \"{} ({} structures, e.g. {})\"",
            layout.id,
            layout.members.len(),
            layout.members[0]
        );
        let _ = writeln!(out, "kind = \"other\"");
        let _ = writeln!(
            out,
            "source = \"derived from: {}\"",
            layout.members.join(", ")
        );
        for k in &layout.keys {
            let _ = writeln!(
                out,
                "\n[[attachment]]\nset = {}\nphase = {}",
                k.set, k.phase
            );
            if let Some(label) = layout
                .labels
                .get(&format!("{}:{}", k.set, k.phase))
                .filter(|l| !l.is_empty())
            {
                let _ = writeln!(out, "label = {label:?}");
            }
            // No "-0.000 m" from rounding noise.
            let c = |v: f64| if v.abs() < 5e-4 { 0.0 } else { v };
            let _ = writeln!(
                out,
                "point = [\"{:.3} m\", \"{:.3} m\", \"{:.3} m\"]",
                c(k.point[0]),
                c(k.point[1]),
                c(k.point[2])
            );
            if !k.insulator.is_empty() {
                match assemblies
                    .iter()
                    .find(|a| a.id == k.insulator)
                    .filter(|_| inline)
                {
                    Some(a) => {
                        let _ = writeln!(
                            out,
                            "string = {{ length = \"{:.3} m\", weight = \"{:.1} N\", top = \"A{}-{}\", bottom = \"W{}-{}\" }}",
                            a.length, a.weight, k.set, k.phase, k.set, k.phase
                        );
                    }
                    None => {
                        let _ = writeln!(out, "insulator = \"{}\"", k.insulator);
                    }
                }
            }
            if let Some(side) = k.side {
                let _ = writeln!(out, "side = \"{side}\"");
            }
        }
        structure_types.insert(layout.id.clone(), out);
    }
    let insulators = assemblies
        .iter()
        .filter(|_| !inline)
        .map(|a| {
            let text = assembly_toml(&a.id, a.kind, a.length, a.weight);
            (a.id.clone(), text)
        })
        .collect();
    Derived {
        structure_types,
        insulators,
        assignments,
    }
}

fn assembly_toml(id: &str, kind: &str, length: f64, weight: f64) -> String {
    format!(
        "# Derived from a line's geometry (weight from the line when it was given) — check stiffness and wind area.\n\
         format = \"ols-insulator\"\nversion = 1\nid = \"{id}\"\nname = \"{kind} {length:.3} m (derived)\"\n\
         kind = \"{kind}\"\nlength = \"{length:.3} m\"\nweight = \"{weight:.1} N\"\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structures::{StructureType, to_global};
    use crate::{InsulatorAssembly, InsulatorLibrary, LibraryItem};
    use std::path::Path;

    fn source(id: &str, position: [f64; 3], rotation: f64) -> SourceStructure {
        let place = |l: [f64; 3]| to_global(position, rotation, l);
        SourceStructure {
            id: id.into(),
            position,
            rotation_deg: rotation,
            attachments: vec![
                SourceAttachment {
                    set: 1,
                    phase: 1,
                    label: "L1".into(),
                    insulator_point: place([-10.0, 0.0, 40.0]),
                    wire_point: place([-10.0, 0.0, 35.0]),
                    section_end: false,
                    weight: 0.0,
                },
                SourceAttachment {
                    set: 2,
                    phase: 1,
                    label: "L1 ahead".into(),
                    insulator_point: place([10.0, 1.0, 40.0]),
                    wire_point: place([10.0, 7.0, 39.5]),
                    section_end: true,
                    weight: 0.0,
                },
            ],
        }
    }

    #[test]
    fn identical_layouts_at_different_rotations_share_a_type() {
        let derived = derive(
            &[
                source("A", [0.0, 0.0, 400.0], 10.0),
                source("B", [500.0, 80.0, 420.0], 37.0),
                SourceStructure {
                    attachments: vec![source("C", [0.0; 3], 0.0).attachments[0].clone()],
                    ..source("C", [900.0, 100.0, 410.0], 50.0)
                },
            ],
            false,
        );
        assert_eq!(derived.assignments[0], derived.assignments[1]);
        assert_ne!(derived.assignments[0], derived.assignments[2]);
        assert_eq!(derived.structure_types.len(), 2);
        assert_eq!(derived.insulators.len(), 2);
    }

    #[test]
    fn inline_strings_make_simple_pylon_files() {
        let derived = derive(&[source("A", [100.0, 50.0, 400.0], 25.0)], true);
        assert!(derived.insulators.is_empty());
        let (id, text) = derived.structure_types.iter().next().unwrap();
        assert!(
            text.contains("string = { length = \"5.000 m\"") && text.contains("top = \"A1-1\""),
            "{text}"
        );
        let t = StructureType::parse(Path::new(id), text).unwrap();
        // Placed without any insulator library: same points and lengths.
        let placed = t
            .place(
                [100.0, 50.0, 400.0],
                25.0,
                None,
                &InsulatorLibrary::default(),
            )
            .unwrap();
        let src = source("A", [100.0, 50.0, 400.0], 25.0);
        let dist = |a: [f64; 3], b: [f64; 3]| {
            ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
        };
        for (p, s) in placed.iter().zip(&src.attachments) {
            assert!(dist(p.insulator_point, s.insulator_point) < 0.01);
            let (lp, ls) = (
                dist(p.wire_point, p.insulator_point),
                dist(s.wire_point, s.insulator_point),
            );
            assert!((lp - ls).abs() < 0.01);
            // The section end is a strain string (free member in the solver).
            assert_eq!(p.strain, s.section_end);
        }
    }

    #[test]
    fn derived_files_load_and_place_back() {
        let derived = derive(&[source("A", [100.0, 50.0, 400.0], 25.0)], false);
        let mut insulators = InsulatorLibrary::default();
        for (id, text) in &derived.insulators {
            insulators
                .insert(
                    Path::new(id),
                    InsulatorAssembly::parse(Path::new(id), text).unwrap(),
                )
                .unwrap();
        }
        let (id, text) = derived.structure_types.iter().next().unwrap();
        let t = StructureType::parse(Path::new(id), text).unwrap();
        // Placing the derived type reproduces the attachment points and the
        // assembly lengths; suspension clamps hang below, while a strain
        // clamp's direction comes from equilibrium in the solver.
        let placed = t
            .place([100.0, 50.0, 400.0], 25.0, None, &insulators)
            .unwrap();
        let src = source("A", [100.0, 50.0, 400.0], 25.0);
        let dist = |a: [f64; 3], b: [f64; 3]| {
            ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
        };
        for (p, s) in placed.iter().zip(&src.attachments) {
            assert!(
                dist(p.insulator_point, s.insulator_point) < 0.01,
                "{p:?} vs {s:?}"
            );
            let (lp, ls) = (
                dist(p.wire_point, p.insulator_point),
                dist(s.wire_point, s.insulator_point),
            );
            assert!((lp - ls).abs() < 0.01, "length {lp} vs {ls}");
            if !s.section_end {
                assert!(dist(p.wire_point, s.wire_point) < 0.01, "{p:?} vs {s:?}");
            }
        }
    }
}
