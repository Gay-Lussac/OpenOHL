// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

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

use ohl_structures::derive::{Derived, SourceAttachment, SourceStructure, derive};
use ohl_structures::{AssemblyKind, InsulatorLibrary, StructureLibrary};

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
        routes: Default::default(),
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
        fields: Default::default(),
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

/// The line's own strings without a library label, taken into the
/// insulator library ([`Line::catalogue_strings`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Catalogue {
    /// Library entries to add (each its values; ids not in the library).
    pub new: Vec<ohl_structures::InsulatorAssembly>,
    /// The strings to label: (structure, string, library label).
    pub links: Vec<(String, String, String)>,
}

impl Catalogue {
    /// Nothing to do.
    pub fn is_empty(&self) -> bool {
        self.links.is_empty()
    }

    /// Puts the labels on `line`'s strings (the line as stored): each
    /// labelled string's weight, wind area and stiffness then come from
    /// its library entry (its own cleared); its length stays its own.
    pub fn apply_to(&self, line: &mut Line) {
        for (structure, label, id) in &self.links {
            if let Some(s) = line.structures.iter_mut().find(|s| s.id == *structure)
                && let Some(x) = s.strings.iter_mut().find(|x| x.label == *label)
            {
                x.insulator = Some(id.clone());
                x.weight = 0.0;
                x.wind_area = 0.0;
                x.stiffness = None;
            }
        }
    }
}

/// A string's values as a library entry has them: strain, length (mm),
/// weight (0.1 N), wind area (cm²), stiffness (N), rounded.
type StringKey = (bool, i64, i64, i64, i64);

fn string_key(strain: bool, length: f64, weight: f64, wind_area: f64, stiffness: f64) -> StringKey {
    (
        strain,
        (length * 1e3).round() as i64,
        (weight * 10.0).round() as i64,
        (wind_area * 1e4).round() as i64,
        stiffness.round() as i64,
    )
}

impl Line {
    /// The structures' own strings (of this line, as stored) without a
    /// library label, taken into the insulator library: strings of the
    /// same values (`resolved`: this line resolved — suspension or strain,
    /// length to the mm, weight, wind area, stiffness) share an entry — one
    /// of `library` with those values, else a new one (`I-3.500m-900N`,
    /// `Strain-…`); each such string gets its label. Strings of structure
    /// types are not labelled here (they are the type file's). Strings
    /// without a weight (0 N resolved: geometry only, e.g. imported) are
    /// left unlabelled: an entry has a weight, and a 0 N one would read as
    /// a real weight; unlabelled, a strain string keeps the weight the
    /// solver estimates from where the data puts it.
    pub fn catalogue_strings(&self, resolved: &Line, library: &InsulatorLibrary) -> Catalogue {
        let key_of = |a: &ohl_structures::InsulatorAssembly| {
            string_key(
                a.kind == AssemblyKind::Strain,
                a.length.si(),
                a.weight.si(),
                a.wind_area.si(),
                a.axial_stiffness.si(),
            )
        };
        let mut known: BTreeMap<StringKey, String> = BTreeMap::new();
        for a in library.iter().filter(|a| a.kind != AssemblyKind::VString) {
            known.entry(key_of(a)).or_insert_with(|| a.id.clone());
        }
        let taken = |id: &str, out: &Catalogue| {
            library.get(id).is_some() || out.new.iter().any(|a| a.id == id)
        };
        let mut out = Catalogue::default();
        for s in &self.structures {
            let Some(r) = resolved.structure(&s.id) else {
                continue;
            };
            for x in s.strings.iter().filter(|x| x.insulator.is_none()) {
                let Some(rx) = r.string(&x.label) else {
                    continue;
                };
                let Some(length) = r.string_length(rx) else {
                    continue;
                };
                if rx.weight == 0.0 {
                    // Weight unknown: not a library entry (see above).
                    continue;
                }
                let stiffness = rx.stiffness.unwrap_or(self.defaults.insulator_stiffness);
                let key = string_key(rx.strain, length, rx.weight, rx.wind_area, stiffness);
                let id = match known.get(&key) {
                    Some(id) => id.clone(),
                    None => {
                        let stem = format!(
                            "{}-{:.3}m-{:.0}N",
                            if rx.strain { "Strain" } else { "Suspension" },
                            key.1 as f64 / 1e3,
                            rx.weight
                        );
                        let id = std::iter::once(stem.clone())
                            .chain((2..).map(|n| format!("{stem}-{n}")))
                            .find(|id| !taken(id, &out))
                            .expect("a free label");
                        out.new.push(ohl_structures::InsulatorAssembly {
                            format: None,
                            version: None,
                            id: id.clone(),
                            name: format!(
                                "{} {:.3} m, {:.0} N",
                                if rx.strain {
                                    "strain string"
                                } else {
                                    "suspension string"
                                },
                                key.1 as f64 / 1e3,
                                rx.weight
                            ),
                            kind: if rx.strain {
                                AssemblyKind::Strain
                            } else {
                                AssemblyKind::Suspension
                            },
                            source: Some("taken from the model's strings".into()),
                            length: ohl_core::units::Length::from_meters(key.1 as f64 / 1e3),
                            weight: ohl_core::units::Force::from_newtons(rx.weight),
                            axial_stiffness: ohl_core::units::Force::from_newtons(stiffness),
                            wind_area: ohl_core::units::Area::from_square_meters(rx.wind_area),
                            half_width: None,
                            // The string keeps its own limits.
                            max_load: None,
                            max_swing: None,
                            max_swing_across: None,
                            max_swing_along: None,
                        });
                        known.insert(key, id.clone());
                        id
                    }
                };
                out.links.push((s.id.clone(), x.label.clone(), id));
            }
        }
        out
    }

    /// Returns a copy with every typed structure's points, strings and wire
    /// points generated from the libraries — the structure's own items
    /// replacing the type's with the same label (a wire point with a set
    /// replacing the type's with that set, with what moves with it), those
    /// in `omit`/`omit_sets` left out — wind areas from the insulator
    /// library, weights, stiffnesses and limits (not checked) of strings
    /// naming an assembly where they give none, and strain strings aimed
    /// along their conductor; then validates it.
    pub fn resolve(
        &self,
        structures: &StructureLibrary,
        insulators: &InsulatorLibrary,
    ) -> Result<Line, LineError> {
        self.resolve_with(structures, insulators, None)
    }

    /// [`Self::resolve`], each structure merged with its type kept in
    /// `cache` and taken from it while the structure is as it was (the
    /// cache belongs to these libraries: a new one with other libraries).
    /// The same line as resolving it again.
    pub fn resolve_with(
        &self,
        structures: &StructureLibrary,
        insulators: &InsulatorLibrary,
        cache: Option<&ResolveCache>,
    ) -> Result<Line, LineError> {
        let invalid = |m: String| LineError::Invalid(m);
        let mut line = self.clone();
        let mut kept = cache.and_then(|c| c.kept.lock().ok());
        for s in &mut line.structures {
            let Some(type_id) = &s.structure_type else {
                continue;
            };
            if let Some(m) = kept.as_deref()
                && let Some((stored, merged)) = m.get(&s.id)
                && stored == s
            {
                *s = merged.clone();
                continue;
            }
            let stored = kept.is_some().then(|| s.clone());
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
            points.extend(placed.points.into_iter().map(|p| {
                let (joint, offset) = t
                    .point_link(&p.label)
                    .map_or((None, [0.0; 3]), |(j, o)| (Some(j.to_owned()), o));
                StructurePoint {
                    description: p.description,
                    label: p.label,
                    position: p.position,
                    joint,
                    offset,
                    unlinked: false,
                }
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
                stiffness: x.stiffness,
                strain: x.strain,
                description: x.description,
                max_load: x.limits.max_load,
                max_swing_deg: x.limits.max_swing_deg,
                max_swing_across_deg: x.limits.max_swing_across_deg,
                max_swing_along_deg: x.limits.max_swing_along_deg,
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
            // The structure's own items over the type's, each point linked
            // to its joint.
            for mut p in s.points.drain(..) {
                link_point(&mut p, t, at, s.orientation_deg, s.extension.as_deref());
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
            if let (Some(m), Some(stored)) = (kept.as_deref_mut(), stored) {
                m.insert(s.id.clone(), (stored, s.clone()));
            }
        }
        drop(kept);

        // Wind areas of library assemblies (a V's shared by its legs), for
        // strings without their own.
        for s in &mut line.structures {
            let legs: BTreeMap<String, usize> = s
                .strings
                .iter()
                .map(|x| (x.wire.clone(), s.strings_to(&x.wire).count()))
                .collect();
            // Weight and stiffness of library assemblies too, for strings
            // without their own (a V's weight shared by its legs).
            for x in s.strings.iter_mut() {
                let Some(i) = x.insulator.as_deref().and_then(|id| insulators.get(id)) else {
                    continue;
                };
                let share = if i.kind == AssemblyKind::VString {
                    1.0 / legs.get(&x.wire).copied().unwrap_or(1).max(1) as f64
                } else {
                    1.0
                };
                if x.weight == 0.0 {
                    x.weight = i.weight.si() * share;
                }
                if x.stiffness.is_none() {
                    x.stiffness = Some(i.axial_stiffness.si());
                }
                // Its limits (not checked): its own over the assembly's.
                let l = i.limits();
                x.set_limits(x.limits().or(crate::StringLimits {
                    max_load: l.max_load,
                    max_swing_deg: l.max_swing_deg,
                    max_swing_across_deg: l.max_swing_across_deg,
                    max_swing_along_deg: l.max_swing_along_deg,
                }));
            }
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

        // The strings' lengths and directions (the type's as placed), their
        // wire points from them.
        line.place_strings()?;
        line.validate()?;
        // Merged with their types: every string's top is there.
        for s in &line.structures {
            s.check_strings().map_err(LineError::Invalid)?;
        }
        Ok(line)
    }

    /// Rotation of each structure's local x axis (right of the line),
    /// degrees CCW from east, from the first route it belongs to: the
    /// bisector of its back and ahead spans (the single span at route ends).
    pub fn route_rotations(&self) -> BTreeMap<String, f64> {
        let mut rotations = BTreeMap::new();
        for route in self.alignment_views() {
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

/// Links a typed structure's own point to its joint (`at`: the type's
/// feet as placed; `rotation_deg`, `extension`: the structure's), as
/// [`StructurePoint`] says: a point naming a joint of the type sits at the
/// joint as placed plus its offset (its written position kept while it
/// agrees within [`crate::assembly::KEEP_POSITION`]); one written before
/// the links takes the type point's joint of its label, the offset the
/// difference of the positions, or a joint at its position; else, and
/// when its joint is not in the type, it is unlinked and stays where it
/// is.
pub(crate) fn link_point(
    p: &mut StructurePoint,
    t: &ohl_structures::StructureType,
    at: crate::Point,
    rotation_deg: f64,
    extension: Option<&str>,
) {
    use ohl_structures::structures::{to_global, to_local};
    if p.unlinked {
        return;
    }
    let placed = |id: &str| {
        t.joint_local(id, extension)
            .map(|j| to_global(at, rotation_deg, j))
    };
    let distance = |a: crate::Point, b: crate::Point| {
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    };
    if let Some(id) = &p.joint {
        match placed(id) {
            Some(j) => {
                let o = to_global([0.0; 3], rotation_deg, p.offset);
                let here = [j[0] + o[0], j[1] + o[1], j[2] + o[2]];
                if distance(here, p.position) > crate::assembly::KEEP_POSITION {
                    p.position = here;
                }
            }
            None => p.unlinked = true,
        }
        return;
    }
    // Written before the links.
    let joint = match t.point_link(&p.label) {
        Some((id, _)) => Some(id.to_owned()),
        None => t
            .linkable_joints()
            .into_iter()
            .find(|j| {
                placed(&j.id).is_some_and(|q| distance(q, p.position) <= ohl_structures::SAME_JOINT)
            })
            .map(|j| j.id),
    };
    let Some(id) = joint else {
        p.unlinked = true;
        return;
    };
    let Some(j) = placed(&id) else {
        p.unlinked = true;
        return;
    };
    let d = [
        p.position[0] - j[0],
        p.position[1] - j[1],
        p.position[2] - j[2],
    ];
    p.offset = if distance(p.position, j) <= crate::assembly::KEEP_POSITION {
        [0.0; 3]
    } else {
        to_local([0.0; 3], rotation_deg, d)
    };
    p.joint = Some(id);
}

/// Structures merged with their types by [`Line::resolve_with`], kept by
/// structure id with the structure as stored: one per pair of structure
/// and insulator libraries.
#[derive(Debug, Default)]
pub struct ResolveCache {
    kept: std::sync::Mutex<std::collections::HashMap<String, (crate::Structure, crate::Structure)>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ohl_structures::{InsulatorAssembly, LibraryItem, StructureType};
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

    /// The example line's strings taken into an empty library: strings of
    /// the same values share an entry; labelled and resolved with it, each
    /// keeps its weight, wind area, stiffness and length; a second pass
    /// finds nothing; an entry already there with the same values is used.
    /// Strings without a weight (the example's dead-end strings, geometry
    /// only) are left unlabelled: no 0 N entry (2026-10-21).
    #[test]
    fn strings_are_taken_into_the_library() {
        let line = crate::tests::example();
        let empty = InsulatorLibrary::default();
        let resolved = line.resolve(&StructureLibrary::default(), &empty).unwrap();
        let c = line.catalogue_strings(&resolved, &empty);
        let strings = || line.structures.iter().flat_map(|s| &s.strings);
        let weighed = strings().filter(|x| x.weight > 0.0).count();
        assert!(
            strings().any(|x| x.weight == 0.0),
            "the example has weightless strings"
        );
        assert_eq!(c.links.len(), weighed);
        assert!(c.new.iter().all(|a| a.weight.si() > 0.0));
        for (structure, label, _) in &c.links {
            let x = line.structure(structure).unwrap().string(label).unwrap();
            assert!(x.weight > 0.0, "{structure} {label}");
        }
        assert!(
            !c.new.is_empty() && c.new.len() < weighed,
            "{}",
            c.new.len()
        );
        let mut library = InsulatorLibrary::default();
        for a in &c.new {
            a.validate().unwrap();
            let back = InsulatorAssembly::parse(Path::new(&a.id), &a.to_toml()).unwrap();
            library.insert(Path::new(&a.id), back).unwrap();
        }
        let mut labelled = line.clone();
        c.apply_to(&mut labelled);
        let again = labelled
            .resolve(&StructureLibrary::default(), &library)
            .unwrap();
        for (s, r) in resolved.structures.iter().zip(&again.structures) {
            for (x, y) in s.strings.iter().zip(&r.strings) {
                assert_eq!(y.insulator.is_some(), x.weight > 0.0);
                assert!(
                    (x.weight - y.weight).abs() < 0.05,
                    "{} {}",
                    x.weight,
                    y.weight
                );
                assert!((x.wind_area - y.wind_area).abs() < 5e-5);
                let k = |s: &InsulatorString| {
                    s.stiffness
                        .unwrap_or(crate::solve::DEFAULT_INSULATOR_STIFFNESS)
                };
                assert!((k(x) - k(y)).abs() < 0.5);
                let (lx, ly) = (s.string_length(x).unwrap(), r.string_length(y).unwrap());
                assert!((lx - ly).abs() < 1e-6, "{lx} {ly}");
            }
        }
        assert!(labelled.catalogue_strings(&again, &library).is_empty());
        // The original again, the library holding the entries: no new one.
        let c2 = line.catalogue_strings(&resolved, &library);
        assert!(c2.new.is_empty());
        assert_eq!(c2.links, c.links);
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
        let cables = ohl_cables::Library::load(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
        )
        .unwrap();
        let case =
            ohl_weather::WeatherCase::still("15", ohl_core::units::Temperature::from_celsius(15.0));
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
        let line = Line::load(&root.join("synthetic-line-typed.ohl.json")).unwrap();
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
        let cables = ohl_cables::Library::load(&root.join("../../libraries/cables")).unwrap();
        let case =
            ohl_weather::WeatherCase::still("15", ohl_core::units::Temperature::from_celsius(15.0));
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
                Path::new("w.ohls"),
                StructureType::parse(Path::new("w.ohls"), text).unwrap(),
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
        assert!(StructureType::parse(Path::new("w.ohls"), &text.replace("0.35", "-1")).is_err());
    }

    /// A string's limits (not checked): a type's string's own and those
    /// of the insulator it names reach the line — its own winning, the
    /// assembly's filling what it does not give, none given staying none;
    /// a structure's own string naming the assembly takes them too; the
    /// stored line keeps only what it gives, through the line file.
    #[test]
    fn string_limits_come_from_the_string_or_its_insulator() {
        let ohli = "format = \"ohl-insulator\"\nversion = 2\nid = \"I-lim\"\nname = \"i\"\n\
            kind = \"suspension\"\nlength = \"4 m\"\nmax_load = \"120 kN\"\n\
            max_swing = \"60 deg\"\nmax_swing_along = \"20 deg\"\n";
        let mut insulators = InsulatorLibrary::default();
        insulators
            .insert(
                Path::new("i.ohli"),
                InsulatorAssembly::parse(Path::new("i.ohli"), ohli).unwrap(),
            )
            .unwrap();
        let text = "format = \"ohl-structure\"\nversion = 4\nid = \"L\"\nname = \"l\"\n\
            kind = \"lattice\"\n\
            [[point]]\nlabel = \"A1\"\npoint = [\"-6 m\", \"0 m\", \"30 m\"]\n\
            [[point]]\nlabel = \"A2\"\npoint = [\"6 m\", \"0 m\", \"30 m\"]\n\
            [[wire]]\nlabel = \"W1\"\nset = 1\n\
            [[wire]]\nlabel = \"W2\"\nset = 2\n\
            [[string]]\nlabel = \"S1\"\ntop = \"A1\"\nwire = \"W1\"\ninsulator = \"I-lim\"\n\
            max_swing = \"45 deg\"\nmax_swing_across = \"40 deg\"\n\
            [[string]]\nlabel = \"S2\"\ntop = \"A2\"\nwire = \"W2\"\nlength = \"4 m\"\n";
        let mut structures = StructureLibrary::default();
        structures
            .insert(
                Path::new("l.ohls"),
                StructureType::parse(Path::new("l.ohls"), text).unwrap(),
            )
            .unwrap();
        let mut line = crate::tests::example();
        let id = line.structures[1].id.clone();
        line.structures[1].strings.clear();
        line.structures[1].points.clear();
        line.structures[1].wires.clear();
        line.structures[1].structure_type = Some("L".into());
        line.sections
            .retain(|s| s.points.iter().all(|p| p.structure != id));
        line.jumpers.clear();
        // Another structure's own string names the assembly, gives a load.
        let own = line.structures[2].strings[0].label.clone();
        line.structures[2].strings[0].insulator = Some("I-lim".into());
        line.structures[2].strings[0].max_load = Some(70_000.0);
        let resolved = line.resolve(&structures, &insulators).unwrap();
        let s1 = resolved.string_limits(&id, "S1").unwrap();
        assert_eq!(
            s1,
            crate::StringLimits {
                max_load: Some(120_000.0),
                max_swing_deg: Some(45.0),
                max_swing_across_deg: Some(40.0),
                max_swing_along_deg: Some(20.0),
            }
        );
        // No insulator, none given: none.
        assert!(resolved.string_limits(&id, "S2").unwrap().is_empty());
        let s3 = resolved
            .string_limits(&line.structures[2].id, &own)
            .unwrap();
        assert_eq!(s3.max_load, Some(70_000.0));
        assert_eq!(s3.max_swing_deg, Some(60.0));
        assert_eq!(s3.max_swing_across_deg, None);
        // The stored line: only its own, through the line file.
        let json = line.to_json();
        assert_eq!(json.matches("\"max_load\"").count(), 1);
        assert!(!json.contains("max_swing"));
        let back = Line::from_json(&json).unwrap();
        assert_eq!(back, line);
        assert_eq!(
            back.structures[2].strings[0].limits().max_load,
            Some(70_000.0)
        );
        // Refused: a load ≤ 0, an angle beyond 180°, an unknown key.
        let mut bad = line.clone();
        bad.structures[2].strings[0].max_load = Some(0.0);
        assert!(bad.validate().is_err());
        bad.structures[2].strings[0].max_load = None;
        bad.structures[2].strings[0].max_swing_along_deg = Some(181.0);
        assert!(bad.validate().is_err());
        assert!(
            Line::from_json(&json.replace("\"max_load\"", "\"max_loads\"")).is_err(),
            "unknown key"
        );
        assert!(
            StructureType::parse(Path::new("l.ohls"), &text.replace("45 deg", "190 deg")).is_err()
        );
    }

    /// The height adjustment raises a typed structure: its points, wire
    /// points and sticks.
    #[test]
    fn the_height_adjustment_raises_the_type() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/synthetic-line");
        let mut line = Line::load(&root.join("synthetic-line-typed.ohl.json")).unwrap();
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
        let mut line = Line::load(&root.join("synthetic-line-typed.ohl.json")).unwrap();
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
        let line = Line::load(&root.join("synthetic-line-typed.ohl.json")).unwrap();
        let structures = StructureLibrary::load_dir(&root.join("Structures")).unwrap();
        let mut resolved = line
            .resolve(&structures, &InsulatorLibrary::default())
            .unwrap();
        // T1's dead-end strings are free members.
        assert!(resolved.structures[0].strings.iter().all(|s| s.strain));
        for fraction in [0.001, 0.999] {
            resolved.sections[0].add_point_load(&crate::legacy::SpanLoad {
                span: 0,
                fraction,
                vertical: 55.0,
                label: "ball".into(),
                ..Default::default()
            });
        }
        resolved.validate().unwrap();
        let cables = ohl_cables::Library::load(&root.join("../../libraries/cables")).unwrap();
        let case =
            ohl_weather::WeatherCase::still("10", ohl_core::units::Temperature::from_celsius(10.0));
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
