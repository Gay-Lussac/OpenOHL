// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A level 3 structure as plain data for the exports
//! (`ohl-structure-export`: IFC, SAF, CSV): joints with their supports,
//! members with their local axes, sections with their profile (from the
//! section library) or only their values (the file's `[[section]]`
//! tables), materials, and load cases with their loads from a
//! [`LoadTable`].
//!
//! Everything is SI (m, N, Pa, kg/m³) in the structure's axes (x right,
//! y ahead, z up).

use std::collections::BTreeMap;

use ohl_core::geometry::cross;
use ohl_sections::{SectionLibrary, Shape};
use ohl_structure_loads::{At, LoadCaseRecipe, LoadTable, MODEL_CASE};
use ohl_structures::{LibraryItem, StructureType};
use ohl_structures_fe::{MemberKind, Model};

/// A joint.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportNode {
    /// Id.
    pub id: String,
    /// Position, m.
    pub point: [f64; 3],
    /// Held degrees of freedom: x, y, z, rx, ry, rz.
    pub fixed: [bool; 6],
    /// Springs, N/m and N·m/rad (0: none).
    pub spring: [f64; 6],
}

impl ExportNode {
    /// Whether the joint is held or on springs.
    pub fn is_support(&self) -> bool {
        self.fixed.iter().any(|f| *f) || self.spring.iter().any(|k| *k != 0.0)
    }
}

/// A material.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportMaterial {
    /// Id.
    pub id: String,
    /// Display name (the id when the file gives none).
    pub name: String,
    /// Young's modulus, Pa.
    pub elasticity: f64,
    /// Shear modulus, Pa.
    pub shear_modulus: f64,
    /// Density, kg/m³.
    pub density: f64,
    /// Yield strength, Pa.
    pub yield_strength: Option<f64>,
}

impl ExportMaterial {
    /// Poisson's ratio E / 2G − 1.
    pub fn poisson(&self) -> f64 {
        self.elasticity / (2.0 * self.shear_modulus) - 1.0
    }
}

/// A section.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportSection {
    /// Id members refer to.
    pub id: String,
    /// Display name (the id when none).
    pub name: String,
    /// Material (index into [`ExportStructure::materials`]).
    pub material: usize,
    /// The profile of the section library; none for a `[[section]]` table
    /// (only its values).
    pub shape: Option<Shape>,
    /// Area, m².
    pub area: f64,
    /// Second moment about the local y axis, m⁴.
    pub iy: f64,
    /// Second moment about the local z axis, m⁴.
    pub iz: f64,
    /// Torsion constant, m⁴.
    pub it: f64,
    /// Plastic moduli about y and z, m³.
    pub wpl: [Option<f64>; 2],
    /// Width (along y) and depth (along z) of the outline box, m (0: none).
    pub size: [f64; 2],
    /// The centroid from the centre of the outline box (y, z), m: where
    /// the member's axis runs through the profile.
    pub centroid: [f64; 2],
}

/// Kind of member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportKind {
    /// Axial force only.
    Truss,
    /// Beam: axial, bending, torsion.
    Frame,
    /// Tension only (guys, with a pretension).
    Cable,
    /// A truss in tension only (compression up to its own buckling load).
    Tension,
}

impl ExportKind {
    /// Name in files.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Truss => "truss",
            Self::Frame => "frame",
            Self::Cable => "cable",
            Self::Tension => "tension",
        }
    }
}

/// A member.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportMember {
    /// Id.
    pub id: String,
    /// First and second joint (indices into [`ExportStructure::nodes`]).
    pub nodes: [usize; 2],
    /// Section (index into [`ExportStructure::sections`]).
    pub section: usize,
    /// Kind.
    pub kind: ExportKind,
    /// Local axes x (first to second joint), y (the section's width) and
    /// z (its height), unit vectors in the structure's axes; the member's
    /// reference and roll applied as in the finite-element model.
    pub axes: [[f64; 3]; 3],
    /// Length, m.
    pub length: f64,
    /// Effective length factor (buckling).
    pub k: f64,
    /// Pretension of a cable, N.
    pub pretension: f64,
    /// A tension-only member ([`ExportKind::Tension`]) that also bends
    /// and twists, as a frame, while it is in the model (the model's
    /// `bending`); false for every other kind.
    pub bending: bool,
    /// Design-checked; false: a modelling member (the model's
    /// `check = false`), exported like any other.
    pub check: bool,
}

impl ExportMember {
    /// Whether it bends at its joints (a frame, or a tension-only member
    /// with [`Self::bending`]): exported with rigid ends and a beam's
    /// behaviour.
    pub fn is_frame(&self) -> bool {
        self.kind == ExportKind::Frame || (self.kind == ExportKind::Tension && self.bending)
    }
}

/// Where a load acts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LoadPlace {
    /// At a joint (index).
    Node(usize),
    /// At a point of a member (index), a fraction `at` of its length from
    /// its first joint.
    OnMember {
        /// Member index.
        member: usize,
        /// Fraction of the length, 0–1.
        at: f64,
    },
    /// Spread evenly along a member (index): the force is per metre.
    AlongMember(usize),
}

/// A load of a case.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportLoad {
    /// The point's label, the joint's or member's id.
    pub label: String,
    /// Where.
    pub place: LoadPlace,
    /// Force in the structure's axes (z up), N (N/m along a member).
    pub force: [f64; 3],
}

/// A load case.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportLoadCase {
    /// Name.
    pub name: String,
    /// What it stands for.
    pub description: String,
    /// Factor on the members' self-weight (0: none).
    pub self_weight: f64,
    /// The loads.
    pub loads: Vec<ExportLoad>,
}

/// A structure as the exports take it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExportStructure {
    /// The structure's id.
    pub id: String,
    /// Its name.
    pub name: String,
    /// Its description.
    pub description: String,
    /// Joints.
    pub nodes: Vec<ExportNode>,
    /// Materials.
    pub materials: Vec<ExportMaterial>,
    /// Sections.
    pub sections: Vec<ExportSection>,
    /// Members.
    pub members: Vec<ExportMember>,
    /// Load cases, in table order.
    pub load_cases: Vec<ExportLoadCase>,
    /// What was left out on the way (rows without a value, loads at
    /// points no joint or member carries…).
    pub notes: Vec<String>,
    /// The members' connections at the joints as the file gives them
    /// (version 9; what a connection does not give is not filled in).
    pub connections: Vec<ohl_structures::connections::TypeConnection>,
    /// The gusset plates as the file gives them.
    pub plates: Vec<ohl_structures::connections::TypePlate>,
    /// The splices (covers) as the file gives them (2026-11-05).
    pub splices: Vec<ohl_structures::connections::TypeSplice>,
    /// The parts of members joined by continuous ends, one piece each
    /// (2026-11-05, [`crate::part_chains`]; [`export_joints`]).
    pub parts: Vec<crate::PartChain>,
    /// The bolts where the joint details put them ([`export_joints`]);
    /// empty when the file has no connection or plate.
    pub bolts: Vec<ExportBolt>,
    /// The gusset plates where the joint details put them
    /// ([`export_joints`]).
    pub joint_plates: Vec<ExportPlate>,
    /// Per member, the shift of its physical solid from its axis
    /// (structure's axes, m): its leg seated on the leg it bolts onto, as
    /// the joint view draws it ([`export_joints`]); empty or zero: on the
    /// axis. The analysis model always keeps the axes.
    pub body_offsets: Vec<[f64; 3]>,
}

/// A bolt as the exports place it, consistent with its member's solid
/// (shifted by [`ExportStructure::body_offsets`]).
#[derive(Debug, Clone, PartialEq)]
pub struct ExportBolt {
    /// The joint's id.
    pub joint: String,
    /// The member's id.
    pub member: String,
    /// Its end at the joint (0: the first joint, 1: the second).
    pub end: usize,
    /// Where it is (structure's axes, m): on the face between the leg
    /// bolted and what it bolts onto.
    pub point: [f64; 3],
    /// Its axis (unit), from that face towards the head.
    pub axis: [f64; 3],
    /// Diameter, m.
    pub diameter: f64,
    /// Hole, m (none: not given).
    pub hole: Option<f64>,
    /// Grade (none: not given).
    pub grade: Option<String>,
    /// The library bolt it is (none: typed only).
    pub bolt: Option<String>,
    /// What it clamps, m: the leg bolted (head side) and what it bolts
    /// onto (nut side).
    pub grip: [f64; 2],
    /// Across the flats of head and nut, the head's height, the nut's, m:
    /// the library bolt's, else the drawing proportions
    /// ([`crate::BOLT_DRAWN`]).
    pub sizes: [f64; 3],
    /// The shank beyond the nut, m (drawing proportion).
    pub beyond: f64,
}

impl ExportBolt {
    /// The length under the head, m: both grips, the nut and the shank
    /// beyond it.
    pub fn length(&self) -> f64 {
        self.grip[0] + self.grip[1] + self.sizes[2] + self.beyond
    }
}

/// A gusset plate as the exports place it.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportPlate {
    /// Id.
    pub id: String,
    /// The joint's id.
    pub joint: String,
    /// Thickness, m.
    pub thickness: f64,
    /// Material (as the file names it).
    pub material: Option<String>,
    /// Its outline (structure's axes, m): one face of the plate; given,
    /// or around the bolts onto it and laid in their face.
    pub outline: Vec<[f64; 3]>,
    /// From the outline's face through the thickness (unit): away from
    /// the members bolted onto it.
    pub normal: [f64; 3],
    /// The outline given in the file.
    pub outline_given: bool,
    /// The members bolted onto it.
    pub members: Vec<String>,
    /// The plates welded together with it into one welded plate, itself
    /// among them (alone: itself only; 2026-11-04).
    pub assembly: Vec<String>,
    /// Its welds to the plates of its assembly listed after it (each seam
    /// once), with their size and length (2026-11-04).
    pub welds: Vec<crate::welds::Weld>,
    /// A splice's cover (2026-11-05): the splice's id, its face and leg in
    /// words ("outer, long leg"); none: a gusset plate.
    pub cover: Option<(String, String)>,
}

fn add3(a: [f64; 3], b: [f64; 3], k: f64) -> [f64; 3] {
    [a[0] + k * b[0], a[1] + k * b[1], a[2] + k * b[2]]
}

fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The unit normal of a polygon (Newell's method); none when degenerate.
fn polygon_normal(pts: &[[f64; 3]]) -> Option<[f64; 3]> {
    let mut n = [0.0; 3];
    for (k, a) in pts.iter().enumerate() {
        let b = pts[(k + 1) % pts.len()];
        n[0] += (a[1] - b[1]) * (a[2] + b[2]);
        n[1] += (a[2] - b[2]) * (a[0] + b[0]);
        n[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    let l = dot3(n, n).sqrt();
    (pts.len() >= 3 && l > 1e-12).then(|| n.map(|v| v / l))
}

/// Adds the bolts and gusset plates of the joint details to `s` (made by
/// [`export_structure`] from the same `text` and `library`), the bolts'
/// sizes from `bolts`; nothing when the file has no `[[connection]]` and
/// no `[[plate]]` (the export then stays as before).
///
/// Each bolt is where the joint view draws it ([`crate::joint_details`]),
/// so on the face of the leg it bolts onto, the member seated there by
/// its end's shift ([`crate::Arrival::offset`]). A member's physical
/// solid is shifted the same way ([`ExportStructure::body_offsets`]) when
/// the ends with bolts agree on the shift; when they differ (or none has
/// one) the solid stays on the axis and the bolts move back with it, so
/// they always go through their member's leg. A plate whose outline is
/// not given lies in the face of the bolts onto it.
pub fn export_joints(
    s: &mut ExportStructure,
    text: &str,
    library: &SectionLibrary,
    bolts: &ohl_structures::bolts::BoltLibrary,
) -> Result<(), String> {
    if s.connections.is_empty() && s.plates.is_empty() && s.splices.is_empty() {
        return Ok(());
    }
    let d = crate::joint_details(text, library, bolts)?;
    if !d.level3 {
        return Ok(());
    }
    s.parts = crate::part_chains(&d);
    // The shift of each end with bolts, per member.
    let mut shifts: BTreeMap<&str, Vec<[f64; 3]>> = BTreeMap::new();
    for j in &d.joints {
        for a in &j.arrivals {
            if !a.bolt_points.is_empty() || !a.other_points.is_empty() {
                shifts.entry(a.member.as_str()).or_default().push(a.offset);
            }
        }
    }
    let mut apart = Vec::new();
    let body: BTreeMap<&str, [f64; 3]> = shifts
        .iter()
        .map(|(m, v)| {
            let same = v
                .iter()
                .all(|o| (0..3).all(|k| (o[k] - v[0][k]).abs() <= 1e-6));
            if !same {
                apart.push(*m);
            }
            (*m, if same { v[0] } else { [0.0; 3] })
        })
        .collect();
    if !apart.is_empty() {
        s.notes.push(format!(
            "member(s) {} seated differently at their two ends: their solids \
             kept on their axes, their bolts with them",
            apart.join(", ")
        ));
    }
    s.body_offsets = s
        .members
        .iter()
        .map(|m| body.get(m.id.as_str()).copied().unwrap_or([0.0; 3]))
        .collect();
    let [flats, head, nut, beyond] = crate::BOLT_DRAWN;
    for j in &d.joints {
        for a in &j.arrivals {
            let Some(b) = a.bolts.value.as_ref() else {
                continue;
            };
            let shift = body.get(a.member.as_str()).copied().unwrap_or([0.0; 3]);
            let dia = b.diameter;
            for (p, axis, grip) in a.bolts_with_axes() {
                s.bolts.push(ExportBolt {
                    joint: j.id.clone(),
                    member: a.member.clone(),
                    end: a.end,
                    point: add3(add3(p, a.offset, -1.0), shift, 1.0),
                    axis,
                    diameter: dia,
                    hole: b.hole,
                    grade: b.grade.clone(),
                    bolt: b.bolt.clone(),
                    grip,
                    sizes: [
                        b.drawn[0].unwrap_or(flats * dia),
                        b.drawn[1].unwrap_or(head * dia),
                        b.drawn[2].unwrap_or(nut * dia),
                    ],
                    beyond: beyond * dia,
                });
            }
        }
        for p in &j.plates {
            let onto: Vec<&crate::Arrival> = j
                .arrivals
                .iter()
                .filter(|a| a.onto.value.as_deref() == Some(p.id.as_str()))
                .collect();
            // The bolts onto it as exported.
            let placed = |a: &crate::Arrival| -> Vec<[f64; 3]> {
                let shift = body.get(a.member.as_str()).copied().unwrap_or([0.0; 3]);
                a.bolt_points
                    .iter()
                    .map(|q| add3(add3(*q, a.offset, -1.0), shift, 1.0))
                    .collect()
            };
            // The first member bolted onto it: its bolts' face and side.
            let face = onto
                .iter()
                .find_map(|a| placed(a).first().map(|q| (*q, a.bolt_axis)));
            // Where the Joint view draws it (around its members' bolts and
            // on the corner), the same outline.
            let outline = p.outline.clone();
            let Some(mut normal) = polygon_normal(&outline) else {
                s.notes.push(format!(
                    "plate {} at {}: no outline to draw, left out of IFC",
                    p.id, j.id
                ));
                continue;
            };
            // Through the thickness away from the members on it: against
            // their bolts' axis (the heads are on the members' side).
            if let Some((_, axis)) = face
                && dot3(normal, axis) > 0.0
            {
                normal = normal.map(|v| -v);
            }
            s.joint_plates.push(ExportPlate {
                id: p.id.clone(),
                joint: j.id.clone(),
                thickness: p.thickness,
                material: p.material.clone(),
                outline,
                normal,
                outline_given: p.outline_given,
                members: p.members.clone(),
                assembly: p.assembly.clone(),
                welds: p.welds.clone(),
                cover: None,
            });
        }
        // The splices' covers (2026-11-05), each as a plate on its face of
        // the leg, through its thickness away from the leg.
        for sp in &j.splices {
            for c in &sp.covers {
                let (Some(normal), true) = (c.through, c.outline.len() >= 3) else {
                    s.notes.push(format!(
                        "splice {} at {}: its {} cover has no outline, left out of IFC",
                        sp.id,
                        j.id,
                        c.words()
                    ));
                    continue;
                };
                let id = format!(
                    "{}-{}{}",
                    sp.id,
                    c.face.as_str(),
                    c.leg
                        .map(|l| format!("-{}", l.as_str()))
                        .unwrap_or_default()
                );
                s.joint_plates.push(ExportPlate {
                    id: id.clone(),
                    joint: j.id.clone(),
                    thickness: c.thickness,
                    material: c.material.clone(),
                    outline: c.outline.clone(),
                    normal,
                    outline_given: c.outline_given,
                    members: sp.bolted.clone(),
                    assembly: vec![id],
                    welds: Vec::new(),
                    cover: Some((sp.id.clone(), c.words())),
                });
            }
        }
    }
    Ok(())
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn unit(a: [f64; 3]) -> [f64; 3] {
    let n = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    [a[0] / n, a[1] / n, a[2] / n]
}

/// A member's local axes as the finite-element model takes them
/// (`ohl_structures_fe`'s element axes): x along the member, y in the
/// plane of x and the reference (global z, or global x for a vertical
/// member), z = x × y; then turned by the roll about x.
pub fn member_axes(
    from: [f64; 3],
    to: [f64; 3],
    reference: Option<[f64; 3]>,
    roll: f64,
) -> [[f64; 3]; 3] {
    let x = unit(sub(to, from));
    let r = reference.unwrap_or(if x[2].abs() > 0.999 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    });
    let z = unit(cross(x, r));
    let y = cross(z, x);
    let (s, c) = roll.sin_cos();
    let y2 = [0, 1, 2].map(|k| c * y[k] + s * z[k]);
    let z2 = [0, 1, 2].map(|k| -s * y[k] + c * z[k]);
    [x, y2, z2]
}

/// The structure file `text` (level 3: every member a section, a supported
/// joint) as plain data, its sections looked up in `library` first; the
/// load cases from `table` (none: no load cases), each with the
/// self-weight factor of its recipe in `recipes` (case `model`: the
/// file's `[analysis] self_weight`; a case without a recipe: 1). Rows
/// without a value, and loads at points no joint or member carries, are
/// left out and listed in [`ExportStructure::notes`].
pub fn export_structure(
    text: &str,
    library: &SectionLibrary,
    table: Option<&LoadTable>,
    recipes: &[LoadCaseRecipe],
) -> Result<ExportStructure, String> {
    let structure = StructureType::parse(std::path::Path::new("structure.ohls"), text)
        .map_err(|e| e.message)?;
    let model = Model::from_structure_type_with(text, library)?;
    if model.members.is_empty() {
        return Err(format!(
            "structure {}: no members (exports need level 3: joints, members with \
             cross-sections, supports)",
            structure.id
        ));
    }
    let mut out = ExportStructure {
        id: structure.id.clone(),
        name: if structure.name.is_empty() {
            structure.id.clone()
        } else {
            structure.name.clone()
        },
        description: structure.description.clone(),
        connections: structure.connections.clone(),
        plates: structure.plates.clone(),
        splices: structure.splices.clone(),
        ..Default::default()
    };
    out.nodes = model
        .nodes
        .iter()
        .map(|n| ExportNode {
            id: n.id.clone(),
            point: n.point,
            fixed: n.fixed,
            spring: n.spring,
        })
        .collect();
    // Materials: the library's by id, the file's [[material]] by id.
    let file_material = |section: &str| -> Option<String> {
        structure
            .sections
            .iter()
            .find(|t| t.get("id").and_then(toml::Value::as_str) == Some(section))
            .and_then(|t| t.get("material").and_then(toml::Value::as_str))
            .map(str::to_owned)
    };
    let mut material_index: BTreeMap<String, usize> = BTreeMap::new();
    for s in &model.sections {
        let (material, profile) = match &s.profile {
            Some(p) => {
                let m = library
                    .material_of(p)
                    .ok_or_else(|| format!("cross-section {}: no material", s.id))?;
                let material = ExportMaterial {
                    id: m.id.clone(),
                    name: if m.name.is_empty() {
                        m.id.clone()
                    } else {
                        m.name.clone()
                    },
                    elasticity: m.elasticity,
                    shear_modulus: m.shear_modulus,
                    density: m.density,
                    yield_strength: m.yield_strength,
                };
                (material, Some(p))
            }
            None => {
                let id = file_material(&s.id).unwrap_or_else(|| format!("{}-material", s.id));
                let material = ExportMaterial {
                    id: id.clone(),
                    name: id,
                    elasticity: s.e,
                    shear_modulus: s.g,
                    density: s.density,
                    yield_strength: s.yield_strength,
                };
                (material, None)
            }
        };
        let m = match material_index.get(&material.id) {
            Some(&k) => k,
            None => {
                material_index.insert(material.id.clone(), out.materials.len());
                out.materials.push(material);
                out.materials.len() - 1
            }
        };
        let (shape, wpl, size, centroid, name) = match profile {
            Some(p) => {
                let q = p.properties();
                let general = matches!(p.shape, Shape::General { .. });
                (
                    Some(p.shape.clone()),
                    [q.wpl_y, q.wpl_z],
                    [q.width, q.depth],
                    if general {
                        [0.0, 0.0]
                    } else {
                        [q.centroid[0] - q.width / 2.0, q.centroid[1] - q.depth / 2.0]
                    },
                    if p.name.is_empty() {
                        p.id.clone()
                    } else {
                        p.name.clone()
                    },
                )
            }
            None => (None, [None, None], [0.0, 0.0], [0.0, 0.0], s.id.clone()),
        };
        out.sections.push(ExportSection {
            id: s.id.clone(),
            name,
            material: m,
            shape,
            area: s.area,
            iy: s.iy,
            iz: s.iz,
            it: s.j,
            wpl,
            size,
            centroid,
        });
    }
    out.members = model
        .members
        .iter()
        .map(|m| {
            let (a, b) = (model.nodes[m.i].point, model.nodes[m.j].point);
            ExportMember {
                id: m.id.clone(),
                nodes: [m.i, m.j],
                section: m.section,
                kind: match m.kind {
                    MemberKind::Truss => ExportKind::Truss,
                    MemberKind::Frame => ExportKind::Frame,
                    MemberKind::Cable => ExportKind::Cable,
                    MemberKind::Tension => ExportKind::Tension,
                },
                axes: member_axes(a, b, m.reference, m.roll),
                length: model.length(m),
                k: m.k,
                pretension: m.pretension,
                bending: m.bending,
                check: m.check,
            }
        })
        .collect();
    if let Some(table) = table {
        let missing = table.missing().count();
        if missing > 0 {
            out.notes.push(format!(
                "{missing} load row(s) without a value left out (no line value, no area…)"
            ));
        }
        for case in table.cases() {
            let recipe = recipes.iter().find(|r| r.name == case);
            let self_weight = match recipe {
                Some(r) => r.self_weight_factor(),
                None if case == MODEL_CASE => f64::from(u8::from(model.analysis.self_weight)),
                None => 1.0,
            };
            let mut loads = Vec::new();
            for r in table.case(case).filter(|r| r.missing.is_none()) {
                // Table: right, ahead, down; structure axes: z up.
                let f = [r.force[0], r.force[1], -r.force[2]];
                if f == [0.0; 3] {
                    continue;
                }
                let place = match r.at {
                    At::Point => model.attachment_node(&r.label).map(LoadPlace::Node),
                    At::Joint => model
                        .nodes
                        .iter()
                        .position(|n| n.id == r.label)
                        .map(LoadPlace::Node),
                    At::Member => model
                        .members
                        .iter()
                        .position(|m| m.id == r.label)
                        .map(LoadPlace::AlongMember),
                };
                let Some(place) = place else {
                    out.notes.push(format!(
                        "case {case}: {} {} carried by no joint or member, left out",
                        r.at.as_str(),
                        r.label
                    ));
                    continue;
                };
                let force = match place {
                    LoadPlace::AlongMember(m) => f.map(|x| x / out.members[m].length),
                    _ => f,
                };
                loads.push(ExportLoad {
                    label: r.label.clone(),
                    place,
                    force,
                });
            }
            out.load_cases.push(ExportLoadCase {
                name: case.to_owned(),
                description: recipe.map(|r| r.description.clone()).unwrap_or_default(),
                self_weight,
                loads,
            });
        }
    }
    Ok(out)
}
