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

use crate::{Attachment, Line, LineError};

/// Result of comparing a line's attachments with a re-resolved version.
#[derive(Debug, Clone, PartialEq)]
pub struct AttachmentCheck {
    /// Largest deviation, m, of attachment points and insulator lengths
    /// (clamp positions are equilibrium results and are not compared).
    pub worst: f64,
    /// Where the largest deviation is.
    pub at: String,
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Compares the attachments of `original` with those of `resolved`.
pub fn compare_attachments(original: &Line, resolved: &Line) -> AttachmentCheck {
    let mut check = AttachmentCheck {
        worst: 0.0,
        at: String::new(),
    };
    for (s, r) in original.structures.iter().zip(&resolved.structures) {
        for a in &s.attachments {
            let Some(b) = r
                .attachments
                .iter()
                .find(|b| b.set == a.set && b.phase == a.phase)
            else {
                continue;
            };
            // Attachment points and assembly lengths define the library; clamp
            // positions are equilibrium results (swung suspension strings at
            // angles, strain strings along their conductor) and are not
            // compared.
            let deviations = [
                distance(a.insulator_point, b.insulator_point),
                (distance(a.wire_point, a.insulator_point)
                    - distance(b.wire_point, b.insulator_point))
                .abs(),
            ];
            for d in deviations {
                if d > check.worst {
                    check = AttachmentCheck {
                        worst: d,
                        at: format!("{} set {} phase {}", s.id, a.set, a.phase),
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

impl Line {
    /// Returns a copy with every typed structure's attachments generated
    /// from the libraries, explicit attachments kept as overrides, and
    /// strain assemblies aimed along their conductor; then validates it.
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
            let placed = t
                .place(
                    s.position,
                    s.orientation_deg,
                    s.extension.as_deref(),
                    insulators,
                )
                .map_err(|m| invalid(format!("structure {}: {m}", s.id)))?;
            let overrides: BTreeMap<(u32, u32), Attachment> = s
                .attachments
                .drain(..)
                .map(|a| ((a.set, a.phase), a))
                .collect();
            let mut merged: Vec<Attachment> = placed
                .into_iter()
                .filter(|p| !s.omit.contains(&[p.set, p.phase]))
                .map(|p| Attachment {
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
            for (key, a) in overrides {
                match merged.iter_mut().find(|m| (m.set, m.phase) == key) {
                    Some(m) => *m = a,
                    None => merged.push(a),
                }
            }
            s.attachments = merged;
            s.members = t
                .place_stick(s.position, s.orientation_deg, s.extension.as_deref())
                .map_err(|m| invalid(format!("structure {}: {m}", s.id)))?;
        }

        // Wind areas of library assemblies.
        for s in &mut line.structures {
            for a in &mut s.attachments {
                if let Some(i) = a.insulator.as_deref().and_then(|id| insulators.get(id)) {
                    a.insulator_wind_area = i.wind_area.si();
                }
            }
        }

        // Aim strain assemblies of typed structures along their conductor.
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
                let Some(a) = line.attachment(p) else {
                    continue;
                };
                // Strain strings: from the insulator library, or given in the
                // structure type (flagged as strain members).
                let library = a.insulator.as_deref().and_then(|id| insulators.get(id));
                let strain =
                    library.is_some_and(|i| i.kind == AssemblyKind::Strain) || a.strain_member;
                if !strain {
                    continue;
                }
                let length = {
                    let (i, w) = (a.insulator_point, a.wire_point);
                    ((w[0] - i[0]).powi(2) + (w[1] - i[1]).powi(2) + (w[2] - i[2]).powi(2)).sqrt()
                };
                // Neighbour: the next point for a section start, the previous
                // one otherwise.
                let neighbour = if k + 1 < n {
                    &section.points[k + 1]
                } else {
                    &section.points[k - 1]
                };
                let Some(target) = line.attachment(neighbour) else {
                    continue;
                };
                // Starting position along the 3D chord to the neighbouring
                // point (slope included); the solver then lets the assembly
                // settle along its conductor.
                let d = [
                    target.insulator_point[0] - a.insulator_point[0],
                    target.insulator_point[1] - a.insulator_point[1],
                    target.insulator_point[2] - a.insulator_point[2],
                ];
                let norm = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                if norm > 1e-9 {
                    let wire = [
                        a.insulator_point[0] + d[0] / norm * length,
                        a.insulator_point[1] + d[1] / norm * length,
                        a.insulator_point[2] + d[2] / norm * length,
                    ];
                    aims.push((p.clone(), wire));
                }
            }
        }
        for (point, wire) in aims {
            let s = line
                .structures
                .iter_mut()
                .find(|s| s.id == point.structure)
                .expect("checked above");
            if let Some(a) = s
                .attachments
                .iter_mut()
                .find(|a| a.set == point.set && a.phase == point.phase)
            {
                a.wire_point = wire;
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
        // Attachments where a section starts or ends: always strain points.
        let ends: std::collections::BTreeSet<(String, u32, u32)> = self
            .sections
            .iter()
            .flat_map(|sec| [sec.points.first(), sec.points.last()])
            .flatten()
            .map(|p| (p.structure.clone(), p.set, p.phase))
            .collect();
        let sources: Vec<SourceStructure> = self
            .structures
            .iter()
            .map(|s| SourceStructure {
                id: s.id.clone(),
                position: s.position,
                rotation_deg: rotations.get(&s.id).copied().unwrap_or(s.orientation_deg),
                attachments: s
                    .attachments
                    .iter()
                    .map(|a| SourceAttachment {
                        set: a.set,
                        phase: a.phase,
                        label: a.label.clone(),
                        insulator_point: a.insulator_point,
                        wire_point: a.wire_point,
                        section_end: ends.contains(&(s.id.clone(), a.set, a.phase)),
                        weight: a.insulator_weight,
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
            s.attachments.clear();
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
        let t1 = &resolved.structures[0].attachments[0];
        assert!(t1.strain_member && t1.insulator.is_none());
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
        let mut custom = line.structures[1].attachments[0].clone();
        custom.insulator_point[2] += 1.0;
        custom.wire_point[2] += 1.0;
        typed.structures[1].attachments = vec![custom.clone()];
        let resolved = typed.resolve(&structures, &insulators).unwrap();
        let a = resolved.structures[1]
            .attachments
            .iter()
            .find(|a| a.set == custom.set && a.phase == custom.phase)
            .unwrap();
        assert_eq!(a.insulator_point, custom.insulator_point);
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
                .attachments
                .iter()
                .all(|a| a.top_label.starts_with('A'))
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

    /// A typed structure leaves out the strings listed in `omit` (the
    /// sections through them go first: the line would not be valid).
    #[test]
    fn omitted_strings_of_a_type_are_left_out() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/synthetic-line");
        let mut line = Line::load(&root.join("synthetic-line-typed.ols.json")).unwrap();
        line.sections.retain(|s| s.id != "S3");
        line.jumpers.clear();
        line.structures[2].omit.push([2, 1]);
        let structures = StructureLibrary::load_dir(&root.join("Structures")).unwrap();
        let resolved = line
            .resolve(&structures, &InsulatorLibrary::default())
            .unwrap();
        let keys: Vec<(u32, u32)> = resolved.structures[2]
            .attachments
            .iter()
            .map(|a| (a.set, a.phase))
            .collect();
        assert_eq!(keys, vec![(1, 1), (1, 2)]);
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
        assert!(
            resolved.structures[0]
                .attachments
                .iter()
                .all(|a| a.strain_member)
        );
        for fraction in [0.001, 0.999] {
            resolved.sections[0].loads.push(crate::SpanLoad {
                span: 0,
                fraction,
                vertical: 55.0,
                label: "ball".into(),
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
