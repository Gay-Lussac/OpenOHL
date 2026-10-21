// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

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
//!
//! The structure types are written in the form of version 2 and on
//! (owner decision 2026-10-21): per attachment a joint of kind `attach`
//! (`[[node]]`, its id the point's label `A<set>-<phase>`), the
//! attachment point on it (`[[point]]` `node`), its wire point
//! (`[[wire]]`, the set) and its string (`[[string]]`: the assembly, or
//! with `inline` its length and weight; a strain string level, ahead or
//! back). An attachment whose wire point is at its attachment point (no
//! string) is a wire point fixed there, as before. Files written in the
//! version 1 form (`[[attachment]]`) are still read.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::structures::{set_from_v1, to_local};

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
    /// Structure types as `.ohls` file contents, by id.
    pub structure_types: BTreeMap<String, String>,
    /// Insulator assemblies as `.ohli` file contents, by id.
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
            "format = \"ohl-structure\"\nversion = 6\nid = \"{}\"",
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
        // No "-0.000 m" from rounding noise.
        let c = |v: f64| if v.abs() < 5e-4 { 0.0 } else { v };
        let point = |p: [f64; 3]| {
            format!(
                "[\"{:.3} m\", \"{:.3} m\", \"{:.3} m\"]",
                c(p[0]),
                c(p[1]),
                c(p[2])
            )
        };
        for k in &layout.keys {
            let top = format!("A{}-{}", k.set, k.phase);
            let given = layout
                .labels
                .get(&format!("{}:{}", k.set, k.phase))
                .filter(|l| !l.is_empty());
            let set = set_from_v1(k.set, k.phase);
            if k.insulator.is_empty() {
                // No string: a wire point fixed to the structure there.
                let wire = given
                    .cloned()
                    .unwrap_or_else(|| format!("W{}-{}", k.set, k.phase));
                let _ = writeln!(
                    out,
                    "\n[[wire]]\nlabel = {wire:?}\nset = {set}\npoint = {}",
                    point(k.point)
                );
                continue;
            }
            let assembly = assemblies.iter().find(|a| a.id == k.insulator);
            // The wire point's label: an inline string's bottom, else the
            // attachment's label (as version 1 files read).
            let wire = match (inline, given) {
                (false, Some(l)) => l.clone(),
                _ => format!("W{}-{}", k.set, k.phase),
            };
            let _ = writeln!(
                out,
                "\n[[node]]\nid = {top:?}\npoint = {}\nkind = \"attach\"\n\n\
                 [[point]]\nlabel = {top:?}\nnode = {top:?}\n\n\
                 [[wire]]\nlabel = {wire:?}\nset = {set}\n\n\
                 [[string]]\nlabel = \"S{}-{}\"\ntop = {top:?}\nwire = {wire:?}",
                point(k.point),
                k.set,
                k.phase
            );
            match assembly.filter(|_| inline) {
                Some(a) => {
                    let _ = writeln!(
                        out,
                        "length = \"{:.3} m\"\nweight = \"{:.1} N\"",
                        a.length, a.weight
                    );
                }
                None => {
                    let _ = writeln!(out, "insulator = \"{}\"", k.insulator);
                }
            }
            if let Some(side) = k.side {
                let direction = if side == "ahead" { 0 } else { 180 };
                let _ = writeln!(
                    out,
                    "strain = true\nangle = \"90 deg\"\ndirection = \"{direction} deg\""
                );
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
         format = \"ohl-insulator\"\nversion = 1\nid = \"{id}\"\nname = \"{kind} {length:.3} m (derived)\"\n\
         kind = \"{kind}\"\nlength = \"{length:.3} m\"\nweight = \"{weight:.1} N\"\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structures::{JointKind, JointSource, StructureType, to_global};
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

    /// The derived type placed back where its source stands: per
    /// attachment, the point where the insulator hung (on an attach joint
    /// of its label), the string's length, a suspension string's wire
    /// point where it was, a section end a strain string.
    fn placed_back(t: &StructureType, insulators: &InsulatorLibrary) {
        let placed = t
            .place_strings([100.0, 50.0, 400.0], 25.0, None, insulators)
            .unwrap();
        let src = source("A", [100.0, 50.0, 400.0], 25.0);
        let dist = |a: [f64; 3], b: [f64; 3]| {
            ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
        };
        assert_eq!(placed.points.len(), src.attachments.len());
        assert_eq!(placed.strings.len(), src.attachments.len());
        for (k, s) in src.attachments.iter().enumerate() {
            let label = format!("A{}-{}", s.set, s.phase);
            let p = placed.points.iter().find(|p| p.label == label).unwrap();
            assert!(dist(p.position, s.insulator_point) < 0.01, "{p:?} vs {s:?}");
            // Its own attach joint, of its label, the point on it.
            assert_eq!(t.point_link(&label), Some((label.as_str(), [0.0; 3])));
            assert_eq!(
                t.joint(&label).map(|j| (j.kind, j.source)),
                Some((Some(JointKind::Attach), JointSource::Node))
            );
            let x = placed.strings.iter().find(|x| x.top == label).unwrap();
            let w = placed.wires.iter().find(|w| w.label == x.wire).unwrap();
            assert_eq!(w.set, Some(s.set));
            let (lp, ls) = (
                dist(w.position, p.position),
                dist(s.wire_point, s.insulator_point),
            );
            assert!((lp - ls).abs() < 0.01, "length {lp} vs {ls}");
            // The section end is a strain string (free member in the solver).
            assert_eq!(x.strain, s.section_end, "{k}");
            if !s.section_end {
                assert!(dist(w.position, s.wire_point) < 0.01, "{w:?} vs {s:?}");
            }
        }
    }

    /// Version 2 and on, a joint per point (owner decision 2026-10-21):
    /// inline strings carry their length and weight.
    #[test]
    fn inline_strings_make_simple_pylon_files() {
        let derived = derive(&[source("A", [100.0, 50.0, 400.0], 25.0)], true);
        assert!(derived.insulators.is_empty());
        let (id, text) = derived.structure_types.iter().next().unwrap();
        assert!(text.contains("version = 6"), "{text}");
        assert!(!text.contains("[[attachment]]"), "{text}");
        assert!(
            text.contains("[[node]]\nid = \"A1-1\"") && text.contains("kind = \"attach\""),
            "{text}"
        );
        assert!(text.contains("length = \"5.000 m\""), "{text}");
        let t = StructureType::parse(Path::new(id), text).unwrap();
        assert!(t.attachments.is_empty());
        // Placed without any insulator library: same points and lengths.
        placed_back(&t, &InsulatorLibrary::default());
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
        // The wire points keep the attachments' labels.
        assert_eq!(
            t.wires.iter().map(|w| w.label.as_str()).collect::<Vec<_>>(),
            ["L1", "L1 ahead"]
        );
        placed_back(&t, &insulators);
    }

    /// An attachment without a string (its wire point at its attachment
    /// point) is a wire point fixed there, as before; the version 1 form
    /// (`[[attachment]]`) is still read.
    #[test]
    fn fixed_wire_points_and_the_old_form() {
        let mut src = source("A", [0.0; 3], 0.0);
        src.attachments[0].wire_point = src.attachments[0].insulator_point;
        let derived = derive(&[src], true);
        let (id, text) = derived.structure_types.iter().next().unwrap();
        let t = StructureType::parse(Path::new(id), text).unwrap();
        let w = t.wires.iter().find(|w| w.set == Some(1)).unwrap();
        assert_eq!(w.point.map(|p| p.map(|v| v.si())), Some([-10.0, 0.0, 40.0]));
        assert_eq!(t.points.len(), 1);
        let old = "format = \"ohl-structure\"\nversion = 1\nid = \"o\"\nname = \"o\"\n\
                   kind = \"other\"\n[[attachment]]\nset = 1\nphase = 1\n\
                   point = [\"-10 m\", \"0 m\", \"40 m\"]\n";
        let t = StructureType::parse(Path::new("o"), old).unwrap();
        assert_eq!(t.attachments.len(), 1);
    }
}
