// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Structure and insulator libraries applied to a line.
//!
//! - [`Line::resolve`]: places typed structures (`"type"` + orientation +
//!   extension) from the structure library, keeps explicit attachments as
//!   overrides, and aims strain assemblies along their conductor (towards
//!   the neighbouring point of the section) — at an angle dead-end a strain
//!   assembly lines up with its span, not with the structure's axis.
//! - [`Line::derive_libraries`]: draft structure types and insulator
//!   assemblies from the line's own attachment geometry, with each
//!   structure's rotation taken from its route (bisector of the back and
//!   ahead spans) rather than from any exported orientation convention.

use std::collections::BTreeMap;

use ols_structures::derive::{Derived, SourceAttachment, SourceStructure, derive};
use ols_structures::{AssemblyKind, InsulatorLibrary, StructureLibrary};

use crate::assembly::{AttachmentV1, from_v1};
use crate::{InsulatorString, Line, LineError, Structure, StructurePoint, WirePoint};

/// Result of comparing a line's strings with a re-resolved version.
#[derive(Debug, Clone, PartialEq)]
pub struct AttachmentCheck {
    /// Largest deviation, m, of string tops and lengths (wire point
    /// positions are equilibrium results and are not compared).
    pub worst: f64,
    /// Where the largest deviation is.
    pub at: String,
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// The tops and lengths of the strings each set hangs on directly, in
/// order.
fn hanging(s: &Structure, wire: &WirePoint) -> Vec<([f64; 3], f64)> {
    let mut out: Vec<([f64; 3], f64)> = s
        .strings_to(&wire.label)
        .filter_map(|st| Some((s.node(&st.top)?.position(), s.string_length(st)?)))
        .collect();
    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// Compares the strings of `original` with those of `resolved`, set by
/// set: the tops of the strings each wire point hangs on and their
/// lengths.
pub fn compare_attachments(original: &Line, resolved: &Line) -> AttachmentCheck {
    let mut check = AttachmentCheck {
        worst: 0.0,
        at: String::new(),
    };
    for (s, r) in original.structures.iter().zip(&resolved.structures) {
        for w in &s.wires {
            let Some(set) = w.set else { continue };
            let Some(v) = r.wire_at_set(set) else {
                continue;
            };
            // Tops and string lengths define the library; wire point
            // positions are equilibrium results (swung strings at angles,
            // strain strings along their conductor) and are not compared.
            let (a, b) = (hanging(s, w), hanging(r, v));
            let mut deviations: Vec<f64> = a
                .iter()
                .zip(&b)
                .flat_map(|(x, y)| [distance(x.0, y.0), (x.1 - y.1).abs()])
                .collect();
            if a.is_empty() && b.is_empty() {
                deviations.push(distance(w.position, v.position));
            }
            if a.len() != b.len() {
                deviations.push(f64::INFINITY);
            }
            for d in deviations {
                if d > check.worst {
                    check = AttachmentCheck {
                        worst: d,
                        at: format!("{} set {set}", s.id),
                    };
                }
            }
        }
    }
    check
}

fn horizontal_unit(from: [f64; 3], to: [f64; 3]) -> Option<(f64, f64)> {
    let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
    let n = dx.hypot(dy);
    (n > 1e-9).then(|| (dx / n, dy / n))
}

/// The type's items without wire point `label` and what moves with it
/// (the strings and set-less wire points joined to it).
fn without_component(
    strings: &mut Vec<InsulatorString>,
    wires: &mut Vec<WirePoint>,
    points: &[StructurePoint],
    label: &str,
) {
    let probe = Structure {
        id: String::new(),
        position: [0.0; 3],
        orientation_deg: 0.0,
        structure_type: None,
        extension: None,
        description: String::new(),
        height_adjust: 0.0,
        points: points.to_vec(),
        strings: strings.clone(),
        wires: wires.clone(),
        omit: Vec::new(),
        omit_sets: Vec::new(),
        members: Vec::new(),
    };
    let c = probe.component(label);
    let gone: Vec<String> = c
        .wires
        .iter()
        .map(|&k| &probe.wires[k])
        .filter(|w| w.label == label || w.set.is_none())
        .map(|w| w.label.clone())
        .collect();
    let gone_strings: Vec<String> = c
        .strings
        .iter()
        .map(|&k| probe.strings[k].label.clone())
        .collect();
    wires.retain(|w| !gone.contains(&w.label));
    strings.retain(|s| !gone_strings.contains(&s.label));
}

impl Line {
    /// Returns a copy with every typed structure's points, strings and wire
    /// points generated from the libraries — the structure's own items
    /// replacing the type's with the same label (a wire point with a set
    /// replacing the type's with that set, with what moves with it), those
    /// in `omit`/`omit_sets` left out — wind areas from the insulator
    /// library, and strain strings aimed along their conductor; then
    /// validates it.
    pub fn resolve(
        &self,
        structures: &StructureLibrary,
        insulators: &InsulatorLibrary,
    ) -> Result<Line, LineError> {
        let invalid = |m: String| LineError::Invalid(m);
        let mut line = self.clone();
        for s in &mut line.structures {
            let Some(type_id) = &s.structure_type else {
                continue;
            };
            let t = structures.get(type_id).ok_or_else(|| {
                invalid(format!(
                    "structure {}: type \"{type_id}\" not in the structure library",
                    s.id
                ))
            })?;
            // The type's feet on the ground at the position, raised by the
            // height adjustment.
            let at = [
                s.position[0],
                s.position[1],
                s.position[2] + s.height_adjust,
            ];
            let placed = t
                .place(at, s.orientation_deg, s.extension.as_deref(), insulators)
                .map_err(|m| invalid(format!("structure {}: {m}", s.id)))?;
            let attachments: Vec<AttachmentV1> = placed
                .into_iter()
                .map(|p| AttachmentV1 {
                    set: p.set,
                    phase: p.phase,
                    label: p.label,
                    insulator_point: p.insulator_point,
                    wire_point: p.wire_point,
                    insulator_weight: p.insulator_weight,
                    insulator: p.insulator,
                    insulator_wind_area: 0.0,
                    legs: p.legs,
                    top_label: p.top_label,
                    bottom_label: p.bottom_label,
                    strain_member: p.strain,
                })
                .collect();
            let (mut points, mut strings, mut wires) = from_v1(&attachments);
            let placed = t
                .place_strings(at, s.orientation_deg, s.extension.as_deref(), insulators)
                .map_err(|m| invalid(format!("structure {}: {m}", s.id)))?;
            points.extend(placed.points.into_iter().map(|p| StructurePoint {
                description: p.description,
                label: p.label,
                position: p.position,
            }));
            wires.extend(placed.wires.into_iter().map(|w| WirePoint {
                description: w.description,
                label: w.label,
                position: w.position,
                set: w.set,
                weight: w.weight,
            }));
            strings.extend(placed.strings.into_iter().map(|x| InsulatorString {
                label: x.label,
                top: x.top,
                wire: x.wire,
                insulator: x.insulator,
                weight: x.weight,
                compression: x.compression,
                wind_area: x.wind_area,
                strain: x.strain,
                description: x.description,
                ..Default::default()
            }));
            // Sets replaced by the structure's own wire points, or left out.
            let replaced: Vec<u32> = s
                .wires
                .iter()
                .filter_map(|w| w.set)
                .chain(s.omit_sets.iter().copied())
                .collect();
            for set in replaced {
                if let Some(label) = wires
                    .iter()
                    .find(|w| w.set == Some(set))
                    .map(|w| w.label.clone())
                {
                    without_component(&mut strings, &mut wires, &points, &label);
                }
            }
            points.retain(|p| !s.omit.contains(&p.label));
            strings.retain(|x| !s.omit.contains(&x.label));
            wires.retain(|w| !s.omit.contains(&w.label));
            // The structure's own items over the type's.
            for p in s.points.drain(..) {
                points.retain(|q| q.label != p.label);
                points.push(p);
            }
            for x in s.strings.drain(..) {
                strings.retain(|y| y.label != x.label);
                strings.push(x);
            }
            for w in s.wires.drain(..) {
                wires.retain(|v| v.label != w.label);
                wires.push(w);
            }
            (s.points, s.strings, s.wires) = (points, strings, wires);
            s.members = t
                .place_stick(at, s.orientation_deg, s.extension.as_deref())
                .map_err(|m| invalid(format!("structure {}: {m}", s.id)))?;
        }

        // Wind areas of library assemblies (a V's shared by its legs), for
        // strings without their own.
        for s in &mut line.structures {
            let legs: BTreeMap<String, usize> = s
                .strings
                .iter()
                .map(|x| (x.wire.clone(), s.strings_to(&x.wire).count()))
                .collect();
            for x in s.strings.iter_mut().filter(|x| x.wind_area == 0.0) {
                if let Some(i) = x.insulator.as_deref().and_then(|id| insulators.get(id)) {
                    let share = if i.kind == AssemblyKind::VString {
                        1.0 / legs.get(&x.wire).copied().unwrap_or(1).max(1) as f64
                    } else {
                        1.0
                    };
                    x.wind_area = i.wind_area.si() * share;
                }
            }
        }

        // Aim the strain strings of typed structures along their conductor:
        // a wire point hanging on one strain string (from the insulator
        // library, or flagged), from a structure point.
        let typed: BTreeMap<String, bool> = line
            .structures
            .iter()
            .map(|s| (s.id.clone(), s.structure_type.is_some()))
            .collect();
        let mut aims = Vec::new();
        for section in &line.sections {
            let n = section.points.len();
            for (k, p) in section.points.iter().enumerate() {
                if !typed.get(&p.structure).copied().unwrap_or(false) {
                    continue;
                }
                let Some(s) = line.structure(&p.structure) else {
                    continue;
                };
                let Some(wire) = s.wire_at_set(p.set) else {
                    continue;
                };
                let on: Vec<&InsulatorString> = s.strings_to(&wire.label).collect();
                let [string] = on.as_slice() else {
                    continue;
                };
                let Some(top) = s.point(&string.top) else {
                    continue;
                };
                let library = string
                    .insulator
                    .as_deref()
                    .and_then(|id| insulators.get(id));
                let strain =
                    library.is_some_and(|i| i.kind == AssemblyKind::Strain) || string.strain;
                if !strain {
                    continue;
                }
                let Some(length) = s.string_length(string) else {
                    continue;
                };
                // Neighbour: the next point for a section start, the previous
                // one otherwise; towards the top of its single string, else
                // its wire point.
                let neighbour = if k + 1 < n {
                    &section.points[k + 1]
                } else {
                    &section.points[k - 1]
                };
                let Some(target) = line.structure(&neighbour.structure).and_then(|t| {
                    let w = t.wire_at_set(neighbour.set)?;
                    let mut on = t.strings_to(&w.label);
                    match (on.next(), on.next()) {
                        (Some(x), None) => t.point(&x.top).map(|q| q.position),
                        _ => None,
                    }
                    .or(Some(w.position))
                }) else {
                    continue;
                };
                // Starting position along the 3D chord to the neighbouring
                // point (slope included); the solver then lets the string
                // settle along its conductor.
                let i = top.position;
                let d = [target[0] - i[0], target[1] - i[1], target[2] - i[2]];
                let norm = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                if norm > 1e-9 {
                    let at = [0, 1, 2].map(|c| i[c] + d[c] / norm * length);
                    aims.push((p.structure.clone(), wire.label.clone(), at));
                }
            }
        }
        for (structure, label, at) in aims {
            let s = line
                .structures
                .iter_mut()
                .find(|s| s.id == structure)
                .expect("checked above");
            if let Some(w) = s.wires.iter_mut().find(|w| w.label == label) {
                w.position = at;
            }
        }
        line.validate()?;
        Ok(line)
    }

    /// Rotation of each structure's local x axis (right of the line),
    /// degrees CCW from east, from the first route it belongs to: the
    /// bisector of its back and ahead spans (the single span at route ends).
    pub fn route_rotations(&self) -> BTreeMap<String, f64> {
        let mut rotations = BTreeMap::new();
        for route in self.effective_routes() {
            let positions: Vec<[f64; 3]> = route
                .structures
                .iter()
                .filter_map(|id| self.structure(id).map(|s| s.position))
                .collect();
            for (k, id) in route.structures.iter().enumerate() {
                if rotations.contains_key(id) || k >= positions.len() {
                    continue;
                }
                let back = (k > 0)
                    .then(|| horizontal_unit(positions[k - 1], positions[k]))
                    .flatten();
                let ahead = (k + 1 < positions.len())
                    .then(|| horizontal_unit(positions[k], positions[k + 1]))
                    .flatten();
                let direction = match (back, ahead) {
                    (Some(b), Some(a)) => {
                        let (x, y) = (b.0 + a.0, b.1 + a.1);
                        if x.hypot(y) > 1e-9 { (x, y) } else { a }
                    }
                    (Some(d), None) | (None, Some(d)) => d,
                    (None, None) => continue,
                };
                // Local y = ahead direction; local x = 90° clockwise from it.
                rotations.insert(
                    id.clone(),
                    direction.1.atan2(direction.0).to_degrees() - 90.0,
                );
            }
        }
        rotations
    }

    /// Draft structure types and insulator assemblies derived from this
    /// line's attachments, with route-based rotations. Returns the libraries
    /// and a line whose structures reference the derived types.
    pub fn derive_libraries(&self, inline: bool) -> (Derived, Line) {
        let rotations = self.route_rotations();
        // Wire points where a section starts or ends: always strain points.
        let ends: std::collections::BTreeSet<(String, u32)> = self
            .sections
            .iter()
            .flat_map(|sec| [sec.points.first(), sec.points.last()])
            .flatten()
            .map(|p| (p.structure.clone(), p.set))
            .collect();
        // Each set as one string: from the structure point it hangs on
        // (the middle of several), or fixed.
        let sources: Vec<SourceStructure> = self
            .structures
            .iter()
            .map(|s| SourceStructure {
                id: s.id.clone(),
                position: s.position,
                rotation_deg: rotations.get(&s.id).copied().unwrap_or(s.orientation_deg),
                attachments: s
                    .wires
                    .iter()
                    .filter_map(|w| {
                        let set = w.set?;
                        let on: Vec<&InsulatorString> = s.strings_to(&w.label).collect();
                        let tops: Vec<[f64; 3]> = on
                            .iter()
                            .filter_map(|x| s.node(&x.top).map(|n| n.position()))
                            .collect();
                        let insulator_point = if tops.is_empty() {
                            w.position
                        } else {
                            [0, 1, 2]
                                .map(|c| tops.iter().map(|t| t[c]).sum::<f64>() / tops.len() as f64)
                        };
                        Some(SourceAttachment {
                            set,
                            phase: 1,
                            label: w.label.clone(),
                            insulator_point,
                            wire_point: w.position,
                            section_end: ends.contains(&(s.id.clone(), set)),
                            weight: on.iter().map(|x| x.weight).sum(),
                        })
                    })
                    .collect(),
            })
            .collect();
        let derived = derive(&sources, inline);
        let mut typed = self.clone();
        for (s, (source, type_id)) in typed
            .structures
            .iter_mut()
            .zip(sources.iter().zip(&derived.assignments))
        {
            s.structure_type = Some(type_id.clone());
            s.orientation_deg = source.rotation_deg;
            s.extension = None;
            s.points.clear();
            s.strings.clear();
            s.wires.clear();
        }
        (derived, typed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ols_structures::{InsulatorAssembly, LibraryItem, StructureType};
    use std::path::Path;

    fn libraries(derived: &Derived) -> (StructureLibrary, InsulatorLibrary) {
        let mut structures = StructureLibrary::default();
        let mut insulators = InsulatorLibrary::default();
        for (id, text) in &derived.insulators {
            insulators
                .insert(
                    Path::new(id),
                    InsulatorAssembly::parse(Path::new(id), text).unwrap(),
                )
                .unwrap();
        }
        for (id, text) in &derived.structure_types {
            structures
                .insert(
                    Path::new(id),
                    StructureType::parse(Path::new(id), text).unwrap(),
                )
                .unwrap();
        }
        (structures, insulators)
    }

    /// Inline strings: simple pylon files reproduce the line without an
    /// insulator library; the dead-end strings are strain members.
    #[test]
    fn derive_inline_strings() {
        let line = crate::tests::example();
        let (derived, typed) = line.derive_libraries(true);
        assert!(derived.insulators.is_empty());
        let (structures, _) = libraries(&derived);
        let resolved = typed
            .resolve(&structures, &InsulatorLibrary::default())
            .unwrap();
        assert!(compare_attachments(&line, &resolved).worst < 0.01);
        let t1 = &resolved.structures[0].strings[0];
        assert!(t1.strain && t1.insulator.is_none());
    }

    /// Derive types from the example line, resolve the typed line again and
    /// compare every attachment with the original: the round trip must
    /// reproduce the geometry (to the 1 cm rounding of derived files).
    #[test]
    fn derive_then_resolve_reproduces_the_line() {
        let line = crate::tests::example();
        let (derived, typed) = line.derive_libraries(false);
        let (structures, insulators) = libraries(&derived);
        let resolved = typed.resolve(&structures, &insulators).unwrap();
        let check = compare_attachments(&line, &resolved);
        assert!(check.worst < 0.01, "{check:?}");
        // Fewer types than structures: identical towers share a type.
        assert!(derived.structure_types.len() < line.structures.len());

        // Solving the resolved line works and stays close to the original:
        // its dead-ends are now free strain members (derived without weights)
        // where the original fixes the clamps, so allow a few decimetres.
        let cables = ols_cables::Library::load(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
        )
        .unwrap();
        let case =
            ols_weather::WeatherCase::still("15", ols_core::units::Temperature::from_celsius(15.0));
        let a = crate::solve::solve_line(&line, &cables, &case);
        let b = crate::solve::solve_line(&resolved, &cables, &case);
        for (x, y) in a.iter().zip(&b) {
            let (x, y) = (x.as_ref().unwrap(), y.as_ref().unwrap());
            for (sx, sy) in x.spans.iter().zip(&y.spans) {
                for k in 0..3 {
                    assert!(
                        (sx.mid_point[k] - sy.mid_point[k]).abs() < 0.5,
                        "{:?} vs {:?}",
                        sx.mid_point,
                        sy.mid_point
                    );
                }
            }
        }
    }

    #[test]
    fn explicit_attachments_override_the_type() {
        let line = crate::tests::example();
        let (derived, mut typed) = line.derive_libraries(false);
        let (structures, insulators) = libraries(&derived);
        // T2's first string, its top and wire point 1 m higher.
        let t2 = &line.structures[1];
        let mut wire = t2.wires[0].clone();
        let mut string = t2.strings_to(&wire.label).next().unwrap().clone();
        let mut top = t2.point(&string.top).unwrap().clone();
        wire.position[2] += 1.0;
        top.position[2] += 1.0;
        top.label = "custom top".into();
        string.top = top.label.clone();
        typed.structures[1].points = vec![top.clone()];
        typed.structures[1].strings = vec![string.clone()];
        typed.structures[1].wires = vec![wire.clone()];
        let resolved = typed.resolve(&structures, &insulators).unwrap();
        let r = &resolved.structures[1];
        let w = r.wire_at_set(wire.set.unwrap()).unwrap();
        assert_eq!(w.label, wire.label);
        let on: Vec<_> = r.strings_to(&w.label).collect();
        assert_eq!(on.len(), 1);
        assert_eq!(r.point(&on[0].top).unwrap().position, top.position);
    }

    /// The committed typed example resolves from its simple pylon files
    /// (`Structures/`, strings in the files, no insulator library) and
    /// solves; the pylons carry their stick drawings and string labels.
    #[test]
    fn typed_example_resolves_and_solves() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/synthetic-line");
        let line = Line::load(&root.join("synthetic-line-typed.ols.json")).unwrap();
        let structures = StructureLibrary::load_dir(&root.join("Structures")).unwrap();
        let resolved = line
            .resolve(&structures, &InsulatorLibrary::default())
            .unwrap();
        assert!(resolved.structures.iter().all(|s| !s.members.is_empty()));
        assert!(
            resolved.structures[1]
                .points
                .iter()
                .all(|p| p.label.starts_with('A'))
        );
        // The same geometry as the explicit example line.
        let check = compare_attachments(&crate::tests::example(), &resolved);
        assert!(check.worst < 0.01, "{check:?}");
        let cables = ols_cables::Library::load(&root.join("../../libraries/cables")).unwrap();
        let case =
            ols_weather::WeatherCase::still("15", ols_core::units::Temperature::from_celsius(15.0));
        for result in crate::solve::solve_line(&resolved, &cables, &case) {
            assert!(result.is_ok(), "{result:?}");
        }
    }

    /// A type's string with its own wind area brings it into the line;
    /// its own wins over its insulator's.
    #[test]
    fn a_type_strings_wind_area_reaches_the_line() {
        let text = "id = \"W\"\nname = \"w\"\nkind = \"lattice\"\n\
            [[point]]\nlabel = \"A1\"\npoint = [\"-6 m\", \"0 m\", \"30 m\"]\n\
            [[wire]]\nlabel = \"W1\"\nset = 1\n\
            [[string]]\nlabel = \"S1\"\ntop = \"A1\"\nwire = \"W1\"\nlength = \"4 m\"\nwind_area = \"0.35 m²\"\n";
        let mut structures = StructureLibrary::default();
        structures
            .insert(
                Path::new("w.olss"),
                StructureType::parse(Path::new("w.olss"), text).unwrap(),
            )
            .unwrap();
        let mut line = crate::tests::example();
        line.structures[1].strings.clear();
        line.structures[1].points.clear();
        line.structures[1].wires.clear();
        line.structures[1].structure_type = Some("W".into());
        line.sections.retain(|s| {
            s.points
                .iter()
                .all(|p| p.structure != line.structures[1].id)
        });
        line.jumpers.clear();
        let resolved = line
            .resolve(&structures, &InsulatorLibrary::default())
            .unwrap();
        assert_eq!(resolved.structures[1].string("S1").unwrap().wind_area, 0.35);
        assert!(StructureType::parse(Path::new("w.olss"), &text.replace("0.35", "-1")).is_err());
    }

    /// The height adjustment raises a typed structure: its points, wire
    /// points and sticks.
    #[test]
    fn the_height_adjustment_raises_the_type() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/synthetic-line");
        let mut line = Line::load(&root.join("synthetic-line-typed.ols.json")).unwrap();
        let structures = StructureLibrary::load_dir(&root.join("Structures")).unwrap();
        let none = InsulatorLibrary::default();
        let before = line.resolve(&structures, &none).unwrap();
        line.structures[1].height_adjust = 0.5;
        let after = line.resolve(&structures, &none).unwrap();
        let (a, b) = (&before.structures[1], &after.structures[1]);
        for (p, q) in a.points.iter().zip(&b.points) {
            assert!((q.position[2] - p.position[2] - 0.5).abs() < 1e-9);
        }
        for (p, q) in a.wires.iter().zip(&b.wires) {
            assert!((q.position[2] - p.position[2] - 0.5).abs() < 1e-9);
        }
        assert!((b.members[0][0][2] - a.members[0][0][2] - 0.5).abs() < 1e-9);
        assert_eq!(a.position, b.position);
        // Written only when not zero, and read back.
        let json = after.to_json();
        assert_eq!(json.matches("height_adjust").count(), 1);
        assert_eq!(
            Line::from_json(&json).unwrap().structures[1].height_adjust,
            0.5
        );
    }

    /// A typed structure leaves out the strings listed in `omit` (the
    /// sections through them go first: the line would not be valid).
    #[test]
    fn omitted_strings_of_a_type_are_left_out() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/synthetic-line");
        let mut line = Line::load(&root.join("synthetic-line-typed.ols.json")).unwrap();
        line.sections.retain(|s| s.id != "S3");
        line.jumpers.clear();
        line.structures[2].omit_sets.push(2);
        let structures = StructureLibrary::load_dir(&root.join("Structures")).unwrap();
        let resolved = line
            .resolve(&structures, &InsulatorLibrary::default())
            .unwrap();
        let mut sets: Vec<u32> = resolved.structures[2].sets().collect();
        sets.sort_unstable();
        assert_eq!(sets, vec![1, 11]);
    }

    /// Free strain strings (from simple pylon files) with a marker ball
    /// right next to the clamp: the case that stalled on a real earth wire
    /// (ball at fraction 0.001 of a 445 m span) solves, quickly.
    #[test]
    fn free_strain_strings_with_a_ball_at_the_clamp() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/synthetic-line");
        let line = Line::load(&root.join("synthetic-line-typed.ols.json")).unwrap();
        let structures = StructureLibrary::load_dir(&root.join("Structures")).unwrap();
        let mut resolved = line
            .resolve(&structures, &InsulatorLibrary::default())
            .unwrap();
        // T1's dead-end strings are free members.
        assert!(resolved.structures[0].strings.iter().all(|s| s.strain));
        for fraction in [0.001, 0.999] {
            resolved.sections[0].loads.push(crate::SpanLoad {
                span: 0,
                fraction,
                vertical: 55.0,
                label: "ball".into(),
                ..Default::default()
            });
        }
        resolved.validate().unwrap();
        let cables = ols_cables::Library::load(&root.join("../../libraries/cables")).unwrap();
        let case =
            ols_weather::WeatherCase::still("10", ols_core::units::Temperature::from_celsius(10.0));
        let shape = crate::solve::solve_line(&resolved, &cables, &case)
            .remove(0)
            .unwrap();
        assert!(shape.iterations < 30, "{} iterations", shape.iterations);
    }

    #[test]
    fn unknown_type_is_reported() {
        let mut line = crate::tests::example();
        line.structures[0].structure_type = Some("nope".into());
        let error = line
            .resolve(&StructureLibrary::default(), &InsulatorLibrary::default())
            .unwrap_err();
        assert!(error.to_string().contains("type \"nope\""), "{error}");
    }
}
