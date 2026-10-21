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
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
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
                Some(r) => r.self_weight,
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
