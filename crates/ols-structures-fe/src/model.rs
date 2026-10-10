// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Structure models (`*.olsfe`, TOML): nodes, members, sections and
//! materials, read into a [`Model`] in SI units with indices resolved.
//!
//! ```toml
//! format = "ols-structure-model"
//! version = 1
//! name = "Example tower (invented)"
//!
//! [analysis]
//! nonlinear = true          # P-Delta (default); false: first order
//! self_weight = true        # members' weight (default)
//!
//! [[material]]
//! id = "steel"
//! elasticity = "210 GPa"
//! shear_modulus = "81 GPa"  # default E / 2.6
//! density = "7850 kg/m3"    # default 0
//! yield = "355 MPa"         # optional: capacity from A·fy, W·fy
//!
//! [[section]]
//! id = "L90x9"
//! material = "steel"
//! area = "15.5 cm2"
//! iy = "116 cm4"            # bending in the local x–z plane (about y)
//! iz = "116 cm4"            # bending in the local x–y plane (about z)
//! torsion = "4.2 cm4"       # J (default iy + iz)
//! i_min = "48 cm4"          # buckling (default the smaller of iy, iz)
//! wy = "18 cm3"             # elastic moduli (frames)
//! wz = "18 cm3"
//! tension = "400 kN"        # optional allowables instead of the yield
//! compression = "250 kN"
//! moment_y = "6 kN m"
//! moment_z = "6 kN m"
//!
//! [[node]]
//! id = "B1"
//! point = ["-3 m", "-3 m", "0 m"]   # structure axes: x right, y ahead, z up
//! support = "pinned"                # fixed | pinned | ["x", "y", "z", "rx", "ry", "rz"]
//! spring = [0, 0, 0, 0, 0, 0]       # optional, N/m and N·m/rad
//!
//! [[node]]
//! id = "A1"
//! point = ["6 m", "0 m", "30 m"]
//! attachment = [1, 1]               # the type's attachment set 1, phase 1
//!
//! [[member]]
//! id = "leg1"
//! nodes = ["B1", "A1"]
//! section = "L90x9"
//! kind = "truss"            # truss (default) | frame | cable
//! reference = [0, 0, 1]     # frame: a direction in the local x–y plane
//! roll = "0 deg"            # frame: turns the local y, z about x
//! k = 1.0                   # effective length factor (buckling)
//! pretension = "10 kN"      # cable
//!
//! [[load]]                  # the model's own load case (optional)
//! node = "A1"
//! force = ["0 kN", "5 kN", "-10 kN"]
//! moment = ["0 kN m", "0 kN m", "0 kN m"]
//! ```

use std::collections::BTreeMap;
use std::path::Path;

use ols_core::units::{Angle, Area, Density, Force, Length, Stress};
use serde::Deserialize;

use crate::units::{Modulus, Moment, SecondMoment};

/// `format` of a structure model file.
pub const MODEL_FORMAT: &str = "ols-structure-model";
/// Newest format version read.
pub const MODEL_VERSION: u32 = 1;
/// File extension of structure models.
pub const MODEL_EXTENSION: &str = "olsfe";

/// Gravity, m/s².
const G: f64 = ols_core::units::STANDARD_GRAVITY;

fn yes() -> bool {
    true
}

fn one() -> f64 {
    1.0
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AnalysisFile {
    #[serde(default = "yes")]
    nonlinear: bool,
    #[serde(default = "yes")]
    self_weight: bool,
}

impl Default for AnalysisFile {
    fn default() -> Self {
        Self {
            nonlinear: true,
            self_weight: true,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MaterialFile {
    id: String,
    elasticity: Stress,
    #[serde(default)]
    shear_modulus: Option<Stress>,
    #[serde(default)]
    density: Density,
    #[serde(default, rename = "yield")]
    yield_strength: Option<Stress>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SectionFile {
    id: String,
    material: String,
    area: Area,
    #[serde(default)]
    iy: SecondMoment,
    #[serde(default)]
    iz: SecondMoment,
    #[serde(default)]
    torsion: Option<SecondMoment>,
    #[serde(default)]
    i_min: Option<SecondMoment>,
    #[serde(default)]
    wy: Option<Modulus>,
    #[serde(default)]
    wz: Option<Modulus>,
    #[serde(default)]
    tension: Option<Force>,
    #[serde(default)]
    compression: Option<Force>,
    #[serde(default)]
    moment_y: Option<Moment>,
    #[serde(default)]
    moment_z: Option<Moment>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum SupportFile {
    Preset(String),
    Dofs(Vec<String>),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeFile {
    id: String,
    point: [Length; 3],
    #[serde(default)]
    support: Option<SupportFile>,
    #[serde(default)]
    spring: Option<[f64; 6]>,
    #[serde(default)]
    attachment: Option<AttachmentFile>,
}

/// The point of the structure at a node: its label, or (version 1) the
/// attachment's `[set, phase]` (the point `A<set>-<phase>`).
#[derive(Deserialize)]
#[serde(untagged)]
enum AttachmentFile {
    Label(String),
    SetPhase([u32; 2]),
}

impl AttachmentFile {
    fn label(&self) -> String {
        match self {
            AttachmentFile::Label(l) => l.clone(),
            AttachmentFile::SetPhase([s, p]) => format!("A{s}-{p}"),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MemberFile {
    id: String,
    nodes: [String; 2],
    section: String,
    #[serde(default)]
    kind: MemberKind,
    #[serde(default)]
    reference: Option<[f64; 3]>,
    #[serde(default)]
    roll: Angle,
    #[serde(default = "one")]
    k: f64,
    #[serde(default)]
    pretension: Force,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LoadFile {
    node: String,
    #[serde(default)]
    force: [Force; 3],
    #[serde(default)]
    moment: [Moment; 3],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelFile {
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    version: Option<u32>,
    #[serde(default)]
    name: String,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    analysis: AnalysisFile,
    #[serde(rename = "material", default)]
    materials: Vec<MaterialFile>,
    #[serde(rename = "section", default)]
    sections: Vec<SectionFile>,
    #[serde(rename = "node", default)]
    nodes: Vec<NodeFile>,
    #[serde(rename = "member", default)]
    members: Vec<MemberFile>,
    #[serde(rename = "load", default)]
    loads: Vec<LoadFile>,
}

/// Kind of member.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberKind {
    /// Axial force only, pinned ends.
    #[default]
    Truss,
    /// 3D Euler–Bernoulli beam: axial, two bending planes, torsion.
    Frame,
    /// Tension only (guys), with an optional pretension.
    Cable,
}

impl MemberKind {
    /// Name in files.
    pub fn as_str(self) -> &'static str {
        match self {
            MemberKind::Truss => "truss",
            MemberKind::Frame => "frame",
            MemberKind::Cable => "cable",
        }
    }
}

/// A node.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// Name.
    pub id: String,
    /// Position in the structure's axes (x right, y ahead, z up), m.
    pub point: [f64; 3],
    /// Fixed degrees of freedom: x, y, z, rx, ry, rz.
    pub fixed: [bool; 6],
    /// Spring stiffness per degree of freedom (N/m, N·m/rad; 0: none).
    pub spring: [f64; 6],
    /// The label of the structure type's point (where strings hang) at this
    /// node.
    pub attachment: Option<String>,
}

impl Node {
    /// A free node.
    pub fn free(id: &str, point: [f64; 3]) -> Self {
        Self {
            id: id.into(),
            point,
            fixed: [false; 6],
            spring: [0.0; 6],
            attachment: None,
        }
    }

    /// A node with every degree of freedom fixed.
    pub fn fixed(id: &str, point: [f64; 3]) -> Self {
        Self {
            fixed: [true; 6],
            ..Self::free(id, point)
        }
    }

    /// A node with its translations fixed.
    pub fn pinned(id: &str, point: [f64; 3]) -> Self {
        Self {
            fixed: [true, true, true, false, false, false],
            ..Self::free(id, point)
        }
    }

    /// Whether it is held in any way (support or spring).
    pub fn is_support(&self) -> bool {
        self.fixed.iter().any(|f| *f) || self.spring.iter().any(|k| *k != 0.0)
    }
}

/// A cross-section with its material, SI units.
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    /// Name.
    pub id: String,
    /// Area, m².
    pub area: f64,
    /// Second moment about local y (bending in the x–z plane), m⁴.
    pub iy: f64,
    /// Second moment about local z (bending in the x–y plane), m⁴.
    pub iz: f64,
    /// Torsion constant, m⁴.
    pub j: f64,
    /// Second moment for buckling, m⁴.
    pub i_min: f64,
    /// Elastic modulus about y, m³.
    pub wy: Option<f64>,
    /// Elastic modulus about z, m³.
    pub wz: Option<f64>,
    /// Young's modulus, Pa.
    pub e: f64,
    /// Shear modulus, Pa.
    pub g: f64,
    /// Density, kg/m³.
    pub density: f64,
    /// Yield strength, Pa.
    pub yield_strength: Option<f64>,
    /// Allowable tension, N.
    pub tension: Option<f64>,
    /// Allowable compression (before buckling), N.
    pub compression: Option<f64>,
    /// Allowable moment about y, N·m.
    pub moment_y: Option<f64>,
    /// Allowable moment about z, N·m.
    pub moment_z: Option<f64>,
}

impl Section {
    /// A section from its stiffness only (no capacity, no weight).
    pub fn elastic(id: &str, e: f64, area: f64, i: f64) -> Self {
        Self {
            id: id.into(),
            area,
            iy: i,
            iz: i,
            j: 2.0 * i,
            i_min: i,
            wy: None,
            wz: None,
            e,
            g: e / 2.6,
            density: 0.0,
            yield_strength: None,
            tension: None,
            compression: None,
            moment_y: None,
            moment_z: None,
        }
    }

    /// Weight per metre, N/m.
    pub fn weight(&self) -> f64 {
        self.density * self.area * G
    }
}

/// A member between two nodes.
#[derive(Debug, Clone, PartialEq)]
pub struct Member {
    /// Name.
    pub id: String,
    /// Kind.
    pub kind: MemberKind,
    /// First node (index).
    pub i: usize,
    /// Second node (index).
    pub j: usize,
    /// Section (index).
    pub section: usize,
    /// A direction in the local x–y plane (frames; none: global z, or
    /// global x for a vertical member).
    pub reference: Option<[f64; 3]>,
    /// Roll of the local y, z axes about x, radians.
    pub roll: f64,
    /// Effective length factor for buckling.
    pub k: f64,
    /// Pretension of a cable, N.
    pub pretension: f64,
}

impl Member {
    /// A member with default orientation, k = 1, no pretension.
    pub fn new(id: &str, kind: MemberKind, i: usize, j: usize, section: usize) -> Self {
        Self {
            id: id.into(),
            kind,
            i,
            j,
            section,
            reference: None,
            roll: 0.0,
            k: 1.0,
            pretension: 0.0,
        }
    }
}

/// A load at a node, structure axes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodalLoad {
    /// Node (index).
    pub node: usize,
    /// Force, N.
    pub force: [f64; 3],
    /// Moment, N·m.
    pub moment: [f64; 3],
}

/// Analysis settings of a model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Analysis {
    /// Geometric non-linearity (P-Delta); else first order.
    pub nonlinear: bool,
    /// Members' self-weight.
    pub self_weight: bool,
}

impl Default for Analysis {
    fn default() -> Self {
        Self {
            nonlinear: true,
            self_weight: true,
        }
    }
}

/// A structure model, SI units, indices resolved.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Model {
    /// Name.
    pub name: String,
    /// Where the data comes from.
    pub source: Option<String>,
    /// Analysis settings.
    pub analysis: Analysis,
    /// Nodes.
    pub nodes: Vec<Node>,
    /// Sections.
    pub sections: Vec<Section>,
    /// Members.
    pub members: Vec<Member>,
    /// The model's own loads.
    pub loads: Vec<NodalLoad>,
}

fn dof_index(name: &str) -> Option<usize> {
    ["x", "y", "z", "rx", "ry", "rz"]
        .iter()
        .position(|d| *d == name)
}

impl Model {
    /// Parses and checks a model file's text.
    pub fn parse(text: &str) -> Result<Self, String> {
        let file: ModelFile = toml::from_str(text).map_err(|e| e.to_string())?;
        ols_core::files::check_header(
            file.format.as_deref(),
            file.version,
            MODEL_FORMAT,
            MODEL_VERSION,
            "structure model",
        )?;
        let mut materials = BTreeMap::new();
        for m in &file.materials {
            if materials.insert(m.id.as_str(), m).is_some() {
                return Err(format!("material {} given twice", m.id));
            }
        }
        let mut sections = Vec::new();
        let mut section_index = BTreeMap::new();
        for s in &file.sections {
            let m = materials
                .get(s.material.as_str())
                .ok_or_else(|| format!("section {}: no material {}", s.id, s.material))?;
            let e = m.elasticity.si();
            let (iy, iz) = (s.iy.0, s.iz.0);
            if section_index.insert(s.id.clone(), sections.len()).is_some() {
                return Err(format!("section {} given twice", s.id));
            }
            sections.push(Section {
                id: s.id.clone(),
                area: s.area.si(),
                iy,
                iz,
                j: s.torsion.map_or(iy + iz, |j| j.0),
                i_min: s.i_min.map_or(iy.min(iz), |i| i.0),
                wy: s.wy.map(|w| w.0),
                wz: s.wz.map(|w| w.0),
                e,
                g: m.shear_modulus.map_or(e / 2.6, |g| g.si()),
                density: m.density.si(),
                yield_strength: m.yield_strength.map(|y| y.si()),
                tension: s.tension.map(|f| f.si()),
                compression: s.compression.map(|f| f.si()),
                moment_y: s.moment_y.map(|m| m.0),
                moment_z: s.moment_z.map(|m| m.0),
            });
        }
        let mut nodes = Vec::new();
        let mut node_index = BTreeMap::new();
        for n in &file.nodes {
            if node_index.insert(n.id.clone(), nodes.len()).is_some() {
                return Err(format!("node {} given twice", n.id));
            }
            let fixed = match &n.support {
                None => [false; 6],
                Some(SupportFile::Preset(p)) => match p.as_str() {
                    "fixed" => [true; 6],
                    "pinned" => [true, true, true, false, false, false],
                    "none" | "free" => [false; 6],
                    other => {
                        return Err(format!(
                            "node {}: support \"{other}\" (fixed, pinned, or a list of x, y, z, \
                             rx, ry, rz)",
                            n.id
                        ));
                    }
                },
                Some(SupportFile::Dofs(list)) => {
                    let mut f = [false; 6];
                    for d in list {
                        let k = dof_index(d).ok_or_else(|| {
                            format!("node {}: support \"{d}\" (x, y, z, rx, ry, rz)", n.id)
                        })?;
                        f[k] = true;
                    }
                    f
                }
            };
            nodes.push(Node {
                id: n.id.clone(),
                point: n.point.map(|x| x.si()),
                fixed,
                spring: n.spring.unwrap_or([0.0; 6]),
                attachment: n.attachment.as_ref().map(AttachmentFile::label),
            });
        }
        let node = |id: &str, what: &str| {
            node_index
                .get(id)
                .copied()
                .ok_or_else(|| format!("{what}: no node {id}"))
        };
        let mut members = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for m in &file.members {
            if !seen.insert(m.id.as_str()) {
                return Err(format!("member {} given twice", m.id));
            }
            let what = format!("member {}", m.id);
            members.push(Member {
                id: m.id.clone(),
                kind: m.kind,
                i: node(&m.nodes[0], &what)?,
                j: node(&m.nodes[1], &what)?,
                section: *section_index
                    .get(&m.section)
                    .ok_or_else(|| format!("{what}: no section {}", m.section))?,
                reference: m.reference,
                roll: m.roll.si(),
                k: m.k,
                pretension: m.pretension.si(),
            });
        }
        let loads = file
            .loads
            .iter()
            .map(|l| {
                Ok(NodalLoad {
                    node: node(&l.node, "load")?,
                    force: l.force.map(|f| f.si()),
                    moment: l.moment.map(|m| m.0),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let model = Model {
            name: file.name,
            source: file.source,
            analysis: Analysis {
                nonlinear: file.analysis.nonlinear,
                self_weight: file.analysis.self_weight,
            },
            nodes,
            sections,
            members,
            loads,
        };
        model.check()?;
        Ok(model)
    }

    /// The model of a structure type file (`.olss`) that holds its
    /// members: its `[[node]]`, `[[member]]`, `[[section]]`,
    /// `[[material]]`, `[[load]]` and `[analysis]` tables. A `[[point]]` on
    /// a joint (`node = "…"`) makes that joint carry the point's loads.
    pub fn from_structure_type(text: &str) -> Result<Self, String> {
        let file: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
        let mut model = toml::Table::new();
        for key in [
            "name", "source", "analysis", "material", "section", "member", "load",
        ] {
            if let Some(v) = file.get(key) {
                model.insert(key.into(), v.clone());
            }
        }
        let mut nodes = file
            .get("node")
            .and_then(toml::Value::as_array)
            .cloned()
            .unwrap_or_default();
        let points = file.get("point").and_then(toml::Value::as_array);
        for p in points.into_iter().flatten() {
            let (Some(label), Some(at)) = (
                p.get("label").and_then(toml::Value::as_str),
                p.get("node").and_then(toml::Value::as_str),
            ) else {
                continue;
            };
            let node = nodes
                .iter_mut()
                .filter_map(toml::Value::as_table_mut)
                .find(|n| n.get("id").and_then(toml::Value::as_str) == Some(at))
                .ok_or_else(|| format!("point {label}: no joint {at}"))?;
            match node.get("attachment").and_then(toml::Value::as_str) {
                Some(other) if other != label => {
                    return Err(format!(
                        "joint {at} carries points {other} and {label}: one point per joint"
                    ));
                }
                _ => {
                    node.insert("attachment".into(), label.into());
                }
            }
        }
        model.insert("node".into(), nodes.into());
        Self::parse(&toml::to_string(&model).map_err(|e| e.to_string())?)
    }

    /// Reads a model file (`.olsfe`), or a structure type file (`.olss`)
    /// holding its members.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if path.extension().is_some_and(|e| e == "olss") {
            Self::from_structure_type(&text)
        } else {
            Self::parse(&text)
        }
        .map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Plausibility checks: positive properties, members of non-zero
    /// length, a frame's reference not along its axis, attachments once.
    pub fn check(&self) -> Result<(), String> {
        if self.nodes.is_empty() || self.members.is_empty() {
            return Err("a model needs nodes and members".into());
        }
        for n in &self.nodes {
            if n.point.iter().any(|x| !x.is_finite())
                || n.spring.iter().any(|k| k.is_nan() || *k < 0.0)
            {
                return Err(format!("node {}: point or spring not valid", n.id));
            }
        }
        for s in &self.sections {
            let ok = [s.area, s.e, s.g].iter().all(|x| *x > 0.0)
                && [s.iy, s.iz, s.j, s.i_min, s.density]
                    .iter()
                    .all(|x| *x >= 0.0);
            let caps = [
                s.yield_strength,
                s.tension,
                s.compression,
                s.moment_y,
                s.moment_z,
                s.wy,
                s.wz,
            ];
            if !ok || caps.iter().flatten().any(|x| x.is_nan() || *x <= 0.0) {
                return Err(format!(
                    "section {}: area, moduli and capacities must be above 0",
                    s.id
                ));
            }
        }
        let mut attachments = std::collections::BTreeSet::new();
        for n in &self.nodes {
            if let Some(a) = &n.attachment
                && !attachments.insert(a)
            {
                return Err(format!("point {a} on two nodes (node {})", n.id));
            }
        }
        for m in &self.members {
            let (Some(a), Some(b)) = (self.nodes.get(m.i), self.nodes.get(m.j)) else {
                return Err(format!("member {}: node index out of range", m.id));
            };
            let s = self
                .sections
                .get(m.section)
                .ok_or_else(|| format!("member {}: section index out of range", m.id))?;
            let d = sub(b.point, a.point);
            let l = norm(d);
            if l.is_nan() || l <= 1e-9 {
                return Err(format!("member {}: zero length", m.id));
            }
            if m.k.is_nan() || m.k <= 0.0 || !m.pretension.is_finite() || m.pretension < 0.0 {
                return Err(format!(
                    "member {}: k must be above 0, pretension 0 or more",
                    m.id
                ));
            }
            if m.kind != MemberKind::Cable && m.pretension != 0.0 {
                return Err(format!("member {}: only cables take a pretension", m.id));
            }
            if m.kind == MemberKind::Frame {
                if !(s.iy > 0.0 && s.iz > 0.0 && s.j > 0.0) {
                    return Err(format!(
                        "member {}: a frame needs iy, iz and torsion above 0 (section {})",
                        m.id, s.id
                    ));
                }
                if let Some(r) = m.reference {
                    let c = cross(d, r);
                    if norm(c).is_nan() || norm(c) <= 1e-6 * l * norm(r) {
                        return Err(format!(
                            "member {}: reference along the member's axis",
                            m.id
                        ));
                    }
                }
            }
        }
        for l in &self.loads {
            if l.node >= self.nodes.len() {
                return Err("load: node index out of range".into());
            }
        }
        Ok(())
    }

    /// The node at the structure's point labelled `label`.
    pub fn attachment_node(&self, label: &str) -> Option<usize> {
        self.nodes
            .iter()
            .position(|n| n.attachment.as_deref() == Some(label))
    }

    /// Length of a member, m.
    pub fn length(&self, m: &Member) -> f64 {
        norm(sub(self.nodes[m.j].point, self.nodes[m.i].point))
    }

    /// Total weight of the members, N.
    pub fn weight(&self) -> f64 {
        self.members
            .iter()
            .map(|m| self.sections[m.section].weight() * self.length(m))
            .sum()
    }
}

pub(crate) fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn norm(a: [f64; 3]) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

pub(crate) fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
