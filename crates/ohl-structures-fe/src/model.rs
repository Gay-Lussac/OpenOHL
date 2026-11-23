// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Structure models (`*.ohlfe`, TOML): nodes, members, sections and
//! materials, read into a [`Model`] in SI units with indices resolved.
//!
//! ```toml
//! format = "ohl-structure-model"
//! version = 1
//! name = "Example tower (invented)"
//!
//! [analysis]
//! nonlinear = true          # P-Delta (default); false: first order
//! self_weight = true        # members' weight (default)
//! drag = 1.0                # member wind: drag factor on the profiles (none: the shape's SIA 261 coefficient)
//! stabilise = true          # hold joint directions without stiffness (default)
//! dead_load_factor = 1.0    # members' self-weight × this (bolts, plates; default 1)
//! single_angle_moments = false  # true: a single angle's frame end moments in its usage (version 4)
//! capacity = "asce10-97"    # member capacities: asce10-97 (default; the bundled plugin
//!                           # asce10 since 2026-11-18) | sia263-2013 | simple (version 4)
//! tension_compression = "buckling"  # tension-only members in compression: up to their
//!                           # buckling load (default) | "none": slack once shortened (version 6)
//! # redundant_force = 2.5   # optional: redundant members also checked for this percentage of
//!                           # the compression in the member they brace (version 7)
//! # climbing_load = "1100 N"  # optional: flat members checked for this load at mid-length,
//!                           # alone (version 7)
//!

//! [[material]]
//! id = "steel"
//! elasticity = "210 GPa"
//! shear_modulus = "81 GPa"  # default E / 2.6
//! density = "7850 kg/m3"    # default 0
//! yield = "355 MPa"         # optional: capacity from A·fy, W·fy
//! ultimate = "490 MPa"      # optional: bolt bearing (version 4)
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
//! section = "L90x9"         # the section library's first, then [[section]]
//! kind = "truss"            # truss (default) | frame | cable | tension
//! # bending = true          # tension: also bends and twists, as a frame (version 3)
//! # check = false           # not design-checked: a modelling member (version 3)
//! reference = [0, 0, 1]     # frame: a direction in the local x–y plane
//! roll = "0 deg"            # turns the local y, z about x (the section turned)
//! # orientation = "x+y-"    # instead of reference: an angle's legs (local y, z)
//! #                         # towards x+, y−; a roll then turns them further
//! k = 1.0                   # effective length factor (buckling)
//! # lk = "2.4 m"            # effective length instead of k × length
//! # capacity_tension = "180 kN"      # instead of the section's capacity
//! # capacity_compression = "95 kN"   # instead of the section's and Euler's
//! # dead_load_factor = 1.3  # its self-weight × this, instead of [analysis]'s
//! pretension = "10 kN"      # cable
//! # [member.design]         # design data for a capacity method (version 4, crate::design)
//! # length_ratios = [1, 1, 1]
//!
//! [[load]]                  # the model's own load case (optional)
//! node = "A1"
//! force = ["0 kN", "5 kN", "-10 kN"]
//! moment = ["0 kN m", "0 kN m", "0 kN m"]
//! ```

use std::collections::BTreeMap;
use std::path::Path;

pub(crate) use ohl_core::geometry::cross;
use ohl_core::units::{Angle, Area, Density, Force, Length, SecondMoment, SectionModulus, Stress};
use ohl_sections::SectionLibrary;
use ohl_structures::bolts::BoltLibrary;
use ohl_structures::structures::STRUCTURE_FILE_EXTENSION;
use serde::Deserialize;

use crate::units::Moment;

/// `format` of a structure model file.
pub const MODEL_FILE_FORMAT: &str = "ohl-structure-model";
/// Newest format version read. Version 7 (2026-11-15) adds the optional
/// checks `[analysis] redundant_force` (percent) and `climbing_load`
/// ([`Analysis::redundant_force`], [`Analysis::climbing_load`]); a file is
/// written at 7 only when it gives one. Version 6 (2026-11-15) adds `[analysis]
/// tension_compression` ([`TensionCompression`]); a file is written at 6
/// only when it gives the key. Version 5 (2026-11-11) adds a plugin's
/// method as `[analysis] capacity`. Version 4 (2026-10-27) adds `[analysis]
/// single_angle_moments` and a member's design data (`[member.design]`,
/// [`crate::design`]); versions 1 to 3 read as before. Version 3 adds a member's `bending` (a
/// tension-only member that also bends, as a frame: a tower program's
/// tension-only beam) and `check = false` (a member not design-checked: a
/// modelling member, a tower program's fictitious member). Version 2 adds
/// the member kind `tension` (a truss in tension only); versions 1 and 2
/// read as before.
pub const MODEL_FILE_VERSION: u32 = 7;
/// The model file version of `[analysis] tension_compression` (2026-11-15).
pub const TENSION_COMPRESSION_MODEL_VERSION: u32 = 6;
/// The model file version of `[analysis] redundant_force` (2026-11-15).
pub const REDUNDANT_FORCE_MODEL_VERSION: u32 = 7;
/// The model file version of `[analysis] climbing_load` (2026-11-15).
pub const CLIMBING_LOAD_MODEL_VERSION: u32 = 7;
/// File extension of structure models.
pub const MODEL_FILE_EXTENSION: &str = "ohlfe";
/// Gravity, m/s².
pub(crate) const G: f64 = ohl_core::units::STANDARD_GRAVITY;

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
    #[serde(default)]
    drag: Option<f64>,
    #[serde(default = "yes")]
    stabilise: bool,
    #[serde(default)]
    dead_load_factor: Option<f64>,
    #[serde(default)]
    single_angle_moments: bool,
    #[serde(default)]
    capacity: Option<String>,
    #[serde(default)]
    tension_compression: Option<String>,
    #[serde(default)]
    redundant_force: Option<f64>,
    #[serde(default)]
    climbing_load: Option<Force>,
}

impl Default for AnalysisFile {
    fn default() -> Self {
        Self {
            nonlinear: true,
            self_weight: true,
            drag: None,
            stabilise: true,
            dead_load_factor: None,
            single_angle_moments: false,
            capacity: None,
            tension_compression: None,
            redundant_force: None,
            climbing_load: None,
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
    #[serde(default)]
    ultimate: Option<Stress>,
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
    wy: Option<SectionModulus>,
    #[serde(default)]
    wz: Option<SectionModulus>,
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
    #[serde(default)]
    orientation: Option<String>,
    #[serde(default)]
    lk: Option<Length>,
    #[serde(default)]
    capacity_tension: Option<Force>,
    #[serde(default)]
    capacity_compression: Option<Force>,
    #[serde(default)]
    dead_load_factor: Option<f64>,
    #[serde(default)]
    bending: bool,
    #[serde(default = "yes")]
    check: bool,
    #[serde(default)]
    design: Option<crate::design::DesignFile>,
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
    /// A truss in tension only (version 2; a tower program's "T-Only"
    /// bracing): removed from the model while it would be compressed, by
    /// the solver's active-set iterations ([`crate::solve()`]); no
    /// pretension. With [`Member::bending`] (version 3) it also bends and
    /// twists as a frame while it is in the model (a tower program's
    /// "T-Only Beam").
    Tension,
}

impl MemberKind {
    /// Name in files.
    pub fn as_str(self) -> &'static str {
        match self {
            MemberKind::Truss => "truss",
            MemberKind::Frame => "frame",
            MemberKind::Cable => "cable",
            MemberKind::Tension => "tension",
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
    /// Ultimate tensile strength, Pa (bolt bearing in a capacity method).
    pub ultimate: Option<f64>,
    /// Allowable tension, N.
    pub tension: Option<f64>,
    /// Allowable compression (before buckling), N.
    pub compression: Option<f64>,
    /// Allowable moment about y, N·m.
    pub moment_y: Option<f64>,
    /// Allowable moment about z, N·m.
    pub moment_z: Option<f64>,
    /// The section library's profile it comes from (its width for the
    /// member wind); none for a `[[section]]` table.
    pub profile: Option<ohl_sections::Section>,
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
            ultimate: None,
            tension: None,
            compression: None,
            moment_y: None,
            moment_z: None,
            profile: None,
        }
    }

    /// A section of the section library with its material: the profile's
    /// properties (its minor principal second moment for buckling; the
    /// torsion constant iy + iz when it has none), no allowables (the
    /// capacity from the yield strength).
    pub fn from_library(
        p: &ohl_sections::Section,
        library: &SectionLibrary,
    ) -> Result<Self, String> {
        let m = library.material_of(p).ok_or_else(|| match &p.material {
            Some(m) => format!(
                "cross-section {}: no material {m} in the cross-section library",
                p.id
            ),
            None => format!("section {}: no material", p.id),
        })?;
        // (The plastic moduli are not read here: not worked out.)
        let q = p.elastic_properties();
        Ok(Self {
            id: p.id.clone(),
            area: q.area,
            iy: q.iy,
            iz: q.iz,
            j: q.it.unwrap_or(q.iy + q.iz),
            i_min: q.iv,
            wy: q.wel_y,
            wz: q.wel_z,
            e: m.elasticity,
            g: m.shear_modulus,
            density: m.density,
            yield_strength: m.yield_strength,
            ultimate: m.ultimate,
            tension: None,
            compression: None,
            moment_y: None,
            moment_z: None,
            profile: Some(p.clone()),
        })
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
    /// Roll of the local y, z axes about x, radians (with an `orientation`:
    /// after it, a correction; 2026-11-04).
    pub roll: f64,
    /// Effective length factor for buckling.
    pub k: f64,
    /// Pretension of a cable, N.
    pub pretension: f64,
    /// Effective length for buckling, m, instead of k × length (none: k ×
    /// length).
    pub lk: Option<f64>,
    /// Tension capacity, N, instead of the section's (none: the section's
    /// allowable tension, else area × yield).
    pub capacity_tension: Option<f64>,
    /// Compression capacity, N, instead of the section's and the Euler
    /// load (none: the smaller of those).
    pub capacity_compression: Option<f64>,
    /// Factor on its self-weight (bolts, plates…), instead of the
    /// analysis's ([`Analysis::dead_load_factor`]).
    pub dead_load_factor: Option<f64>,
    /// A tension-only member ([`MemberKind::Tension`]) that also takes
    /// bending and torsion, as a frame, while it is in the model (a tower
    /// program's tension-only beam); false for every other kind.
    pub bending: bool,
    /// Design-checked ([`crate::check::member_usage`]); false: a modelling
    /// member (a tower program's fictitious member), its forces solved and
    /// reported, its usage not.
    pub check: bool,
    /// Its design data for a capacity method (version 4, 2026-10-27; all
    /// optional, [`crate::design`]).
    pub design: crate::design::MemberDesign,
    /// Rigid end offsets (2026-11-02): from its first joint and from its
    /// second to where its axis ends, structure axes, m; zero: the axis
    /// runs to the joint (the default). A structure file's connection
    /// `shift` gives them (`member_ends::read_connections`): the
    /// end moved sideways so it fits, its force passing beside the joint
    /// — an eccentricity the solve takes as a rigid link to the joint
    /// ([`crate::solve()`], [`Model::eccentric_ends`]).
    pub offsets: [[f64; 3]; 2],
    /// End offsets whose **sign is not known** (2026-11-19, the owner's
    /// decision on O45 A1: an estimated eccentricity's upper bound, a
    /// plate's side), per end, structure axes, m; each part with its own
    /// unknown sign. Never in the solve: taken only in the member's own
    /// check, the moment N·e whole and sign-free — as an offset where only
    /// bars meet a joint, at frame joints too
    /// ([`crate::offsets::uncarried_moments`]). Empty: none (the default).
    pub free_offsets: [Vec<[f64; 3]>; 2],
    /// End releases (stage 4.4 of the joint details, an option of the
    /// analysis, 2026-11-23): per end, the local bending rotations the
    /// end's bolt group cannot hold, released in the solve (frames only).
    /// Set by [`crate::joint_stiffness::apply`] when
    /// [`Analysis::end_releases`] is on; none (the default): every frame
    /// end rigid, as before.
    pub releases: [crate::joint_stiffness::EndRelease; 2],
    /// Bolt slip (stage 4.4, an option of the analysis, 2026-11-23): the
    /// hole clearances of its two ends together, m — how far it can
    /// lengthen or shorten without force before its bolts bear. Set by
    /// [`crate::joint_stiffness::apply`] when [`Analysis::bolt_slip`] is
    /// on; 0 (the default): no slip.
    pub slip: f64,
    /// The share of its axial stiffness EA/L taken in a solve: 1 (the
    /// default), or the secant share of a slipping member in the bolt-slip
    /// passes ([`crate::joint_stiffness::solve_slipping`]).
    pub(crate) axial_share: f64,
}

impl Member {
    /// A member with default orientation, k = 1, no pretension, the
    /// capacities of its section.
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
            lk: None,
            capacity_tension: None,
            capacity_compression: None,
            dead_load_factor: None,
            bending: false,
            check: true,
            design: crate::design::MemberDesign::default(),
            offsets: [[0.0; 3]; 2],
            free_offsets: Default::default(),
            releases: Default::default(),
            slip: 0.0,
            axial_share: 1.0,
        }
    }

    /// Whether it has a frame's stiffness (bending, torsion, rotations at
    /// its joints) while it is in the model: a frame, or a tension-only
    /// member with [`Self::bending`] — unless the end-release option pinned
    /// both its ends both ways ([`Self::releases`], one bolt at each end):
    /// then it is a bar (its torsion left out with its bending).
    pub fn is_frame(&self) -> bool {
        self.bends_by_kind() && !self.releases.iter().all(|r| r.both())
    }

    /// Whether its kind bends: a frame, or a tension-only member with
    /// [`Self::bending`] (whatever its end releases).
    pub fn bends_by_kind(&self) -> bool {
        self.kind == MemberKind::Frame || (self.kind == MemberKind::Tension && self.bending)
    }
}

/// The reference direction (local y) of a member along the unit vector
/// `x` whose angle's legs point towards `orientation` (`"x+y-"`: the
/// first leg — the section's y, its width b — towards the structure's
/// +x, the second — the section's z, its height h — towards −y; each
/// taken normal to the member). The axes are right-handed (z = x × y):
/// when the two directions are the mirror image of that about the
/// member, the legs are swapped (y towards the second) for a section
/// whose two legs are alike (`symmetric`: iy = iz and wy = wz), and
/// refused for one whose legs differ.
pub fn orientation_reference(
    orientation: &str,
    x: [f64; 3],
    symmetric: bool,
) -> Result<[f64; 3], String> {
    let text = orientation.trim();
    let bad = || {
        format!(
            "orientation \"{text}\": two directions of the structure axes, as x+y- (the legs \
             of the angle towards +x and −y)"
        )
    };
    let chars: Vec<char> = text.chars().collect();
    if chars.len() != 4 {
        return Err(bad());
    }
    let axis = |c: char, s: char| -> Option<[f64; 3]> {
        let k = ['x', 'y', 'z']
            .iter()
            .position(|a| *a == c.to_ascii_lowercase())?;
        let sign = match s {
            '+' => 1.0,
            '-' | '−' => -1.0,
            _ => return None,
        };
        let mut v = [0.0; 3];
        v[k] = sign;
        Some(v)
    };
    let (Some(a), Some(b)) = (axis(chars[0], chars[1]), axis(chars[2], chars[3])) else {
        return Err(bad());
    };
    let normal = |v: [f64; 3]| {
        let along = dot(v, x);
        [
            v[0] - along * x[0],
            v[1] - along * x[1],
            v[2] - along * x[2],
        ]
    };
    let (pa, pb) = (normal(a), normal(b));
    if norm(pa) < 1e-3 || norm(pb) < 1e-3 {
        return Err(format!(
            "orientation \"{text}\": a direction along the member (give two across it)"
        ));
    }
    let turn = dot(x, cross(pa, pb));
    if turn.abs() < 1e-3 * norm(pa) * norm(pb) {
        return Err(format!(
            "orientation \"{text}\": the two directions are parallel across the member"
        ));
    }
    if turn > 0.0 {
        Ok(a)
    } else if symmetric {
        Ok(b)
    } else {
        let swapped = format!("{}{}{}{}", chars[2], chars[3], chars[0], chars[1]);
        Err(format!(
            "orientation \"{text}\": the mirror image of an angle for this member's direction \
             (its legs differ): give \"{swapped}\" or turn the member's nodes"
        ))
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

/// A structure type's attachment point on a node (`[[point]] node`, or a
/// point at a node's position): several may share a node.
#[derive(Debug, Clone, PartialEq)]
pub struct JointPoint {
    /// The point's label.
    pub label: String,
    /// Node (index).
    pub node: usize,
    /// From the node to the point, structure axes, m (a rigid link).
    pub offset: [f64; 3],
}

/// Analysis settings of a model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Analysis {
    /// Geometric non-linearity (P-Delta); else first order.
    pub nonlinear: bool,
    /// Members' self-weight.
    pub self_weight: bool,
    /// Drag factor of the member wind on the profiles (none given: 1, used
    /// only by a section without a shape; the others take their shape's
    /// built-in coefficient, [`crate::wind_law::code_drag`]).
    pub drag: f64,
    /// The drag factor given (none given: the shapes' built-in
    /// coefficients; 1 on a `general` section, said not on the safe side,
    /// [`crate::wind`]).
    pub drag_given: bool,
    /// Joint stabilisation ([`crate::stabilise`]) when the model would
    /// otherwise be a mechanism.
    pub stabilise: bool,
    /// Factor on the members' self-weight for what the members do not
    /// model (bolts, plates, galvanising: typically 1.1–1.4 on lattice
    /// towers, more on light bracing); a member's own replaces it.
    pub dead_load_factor: f64,
    /// The dead-load factor given (none given: 1, the members' steel only —
    /// said not on the safe side, [`crate::dead_load_note`]).
    pub dead_load_given: bool,
    /// Add a single angle's frame end moments to its usage (version 4;
    /// default false: a single angle — a profile `equal_angle` or
    /// `unequal_angle` — is checked for its axial force only, as lattice
    /// tower practice checks it, [`crate::check`]).
    pub single_angle_moments: bool,
    /// How the members' capacities are found ([`CapacityMethod`]; version
    /// 4, default ASCE 10-97).
    pub capacity: CapacityMethod,
    /// What the tension-only members carry in compression
    /// ([`TensionCompression`]; model file version 6, structure file
    /// version 14, default: up to their buckling load).
    pub tension_compression: TensionCompression,
    /// The optional redundant-member check (`[analysis] redundant_force`,
    /// percent in files; owner 2026-11-15): each redundant member
    /// ([`crate::design::MemberClass::Redundant`]) is also checked for
    /// this share (0.025: 2.5 %) of the compression in the member it braces
    /// ([`crate::check::redundant_supports`]); none (the default): not
    /// checked. Model file version 7, structure file version 16.
    pub redundant_force: Option<f64>,
    /// The optional climbing-load check (`[analysis] climbing_load`, N;
    /// owner 2026-11-15): each flat member checked in bending for this
    /// load at mid-length, alone, against first yield
    /// ([`crate::check::climbing_check`]); none (the default): not
    /// checked. Model file version 7, structure file version 16.
    pub climbing_load: Option<f64>,
    /// End releases from the joints' bolts (stage 4.4 of the joint
    /// details, 2026-11-23): an option of the analysis, off by default, not
    /// stored in any file — a run turns it on and
    /// [`crate::joint_stiffness::apply`] sets each member's
    /// [`Member::releases`].
    pub end_releases: bool,
    /// Bolt slip at the joints (stage 4.4, 2026-11-23): an option of the
    /// analysis, off by default, not stored in any file —
    /// [`crate::joint_stiffness::apply`] sets each member's
    /// [`Member::slip`].
    pub bolt_slip: bool,
}

/// What a tension-only member ([`MemberKind::Tension`]) carries in
/// compression (`[analysis] tension_compression`, owner 2026-11-15): a
/// setting of each model, so that a model can be switched and the
/// structures that then lose a support seen, named — nothing changes for
/// a model until it is switched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TensionCompression {
    /// `"buckling"` (the default, the decision of 2026-10-19): compression
    /// up to its own buckling load, capped by its design compression
    /// resistance when lower ([`crate::solve::buckling_loads`]); shortened
    /// beyond, bowed, carrying that load.
    #[default]
    Buckling,
    /// `"none"`: no compression at all, not even up to its buckling load —
    /// slack as soon as it is shortened. On the safe side for the members
    /// it relieved (they take its compression; not a bound for every
    /// member of an indeterminate structure); a joint held only by
    /// tension-only members in compression is then free, and the solve
    /// refuses the case naming it.
    None,
}

impl TensionCompression {
    /// Name in files.
    pub fn as_str(self) -> &'static str {
        match self {
            TensionCompression::Buckling => "buckling",
            TensionCompression::None => "none",
        }
    }

    /// The law of a name in files.
    pub fn parse(name: &str) -> Result<Self, String> {
        match name.trim() {
            "buckling" => Ok(TensionCompression::Buckling),
            "none" => Ok(TensionCompression::None),
            other => Err(format!(
                "tension_compression \"{other}\": buckling (default: compression up to the \
                 member's buckling load) or none (no compression: slack once shortened)"
            )),
        }
    }

    /// The law in words, for the notes and the member check.
    pub fn words(self) -> &'static str {
        match self {
            TensionCompression::Buckling => "compression up to their buckling load",
            TensionCompression::None => "no compression",
        }
    }
}

/// How the check finds the members' capacities (`[analysis] capacity`,
/// 2026-10-27).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CapacityMethod {
    /// A design code (the default): since 2026-11-18 every design code is
    /// a plugin's method, [`Model::capacity_plugin`] — by default ASCE
    /// 10-97 of the bundled plugin `asce10` ([`DEFAULT_METHOD`]; until
    /// 2026-11-18 this was the core's own ASCE 10-97, `Asce10`). The core
    /// runs no plugins: its runners do (`ohl-member-check`).
    #[default]
    Code,
    /// The simple check (`"simple"`): Euler buckling and gross yield —
    /// not a design code, unsafe compared with design codes.
    Simple,
}

impl CapacityMethod {
    /// Name in files (a design code: the default's,
    /// [`DEFAULT_METHOD`]; the model's own is its plugin method's,
    /// [`PluginMethod::name`]).
    pub fn as_str(self) -> &'static str {
        match self {
            CapacityMethod::Code => DEFAULT_METHOD,
            CapacityMethod::Simple => "simple",
        }
    }

    /// The core's method of a name in files: `simple` (a design code's
    /// name — `asce10-97`, `sia263-2013`, `plugin:<plugin>/<method>` — is
    /// [`PluginMethod::parse`]'s).
    pub fn parse(name: &str) -> Result<Self, String> {
        match name.trim() {
            "simple" => Ok(CapacityMethod::Simple),
            other => Err(format!(
                "capacity \"{other}\": {}, simple, or a plugin's method \
                 (plugin:<plugin>/<method>); none given: {DEFAULT_METHOD}",
                BUNDLED_ALIASES
                    .iter()
                    .map(|a| a.0)
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }
}

impl Default for Analysis {
    fn default() -> Self {
        Self {
            nonlinear: true,
            self_weight: true,
            drag: 1.0,
            drag_given: false,
            stabilise: true,
            dead_load_factor: 1.0,
            dead_load_given: false,
            single_angle_moments: false,
            capacity: CapacityMethod::Code,
            tension_compression: TensionCompression::Buckling,
            redundant_force: None,
            climbing_load: None,
            end_releases: false,
            bolt_slip: false,
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
    /// The structure type's points on nodes (2026-10-21; a separate
    /// model's nodes name theirs by [`Node::attachment`]).
    pub joint_points: Vec<JointPoint>,
    /// The structure type's attachment points linked to no joint
    /// (`unlinked = true`, owner 2026-11-15): their loads reach no member
    /// — left out by the structure checks, said not on the safe side.
    pub unlinked_points: Vec<String>,
    /// Whether the joint details of a structure file were taken into the
    /// members' ends and end offsets (owner 2026-11-11).
    pub joint_use: JointUse,
    /// The members are checked by a plugin's method instead of
    /// [`Analysis::capacity`] (`[analysis] capacity =
    /// "plugin:<plugin>/<method>"`, model file version 5 and structure
    /// file version 12, 2026-11-11). The core does not run plugins: a
    /// runner that does ([`PluginMethod::dir`] found) replaces the
    /// members' usage; without one every checked member is reported as
    /// not checked ([`crate::check::PLUGIN_NOT_RUN`]) — never by another
    /// method in its place.
    pub capacity_plugin: Option<PluginMethod>,
    /// Per member, the compression the plugin's method resists
    /// ([`Model::capacity_plugin`]; N, none: it gives none), asked by the
    /// runner that found the plugin before any solve (2026-11-18): the
    /// solve's cap on a tension-only member's compression
    /// ([`crate::check::design_compression`]), as the core's own methods
    /// give theirs. Empty: not asked (the Euler load alone caps).
    pub method_compression: Vec<Option<f64>>,
    /// The default minimum bolt distances of the structure
    /// (`[default_bolt_minimums]`, structure file version 18; 2026-11-18,
    /// third round): what the bolt tools keep where no design code gives
    /// minimums ([`crate::bolt_rules::assumed`]) — never in a check.
    pub bolt_defaults: ohl_structures::bolts::DefaultBoltMinimums,
    /// The bolt grades set on members' bolts (`[[bolt_grade]]`, structure
    /// file version 21, owner 2026-11-19;
    /// [`crate::member_ends::apply_bolt_grades`]), each in words with how
    /// many members' bolts take it: lines for the results ("bolt grade 8.8
    /// set by command: …").
    pub bolt_grade_notes: Vec<String>,
    /// What the end-release and bolt-slip options did (stage 4.4,
    /// 2026-11-23; [`crate::joint_stiffness::apply`]); none: both off.
    pub joint_stiffness: Option<crate::joint_stiffness::Summary>,
}

/// A member-check method of a plugin, as a file names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginMethod {
    /// The plugin's name (its manifest's).
    pub plugin: String,
    /// The method's name among the plugin's.
    pub method: String,
    /// Where the plugin is installed, once a runner found it.
    pub dir: Option<std::path::PathBuf>,
    /// The structure's parameter file for the plugin, once a runner
    /// found it (`Plugins/<plugin>.toml` beside the structure, 2026-11-18):
    /// its path and text; none: no file (the method's own values).
    pub parameters: Option<(std::path::PathBuf, String)>,
}

/// The default member check's name in files (`[analysis] capacity`
/// missing): ASCE 10-97 (owner 2026-10-27), since 2026-11-18 the method of
/// the bundled plugin `asce10` ([`BUNDLED_ALIASES`]).
pub const DEFAULT_METHOD: &str = "asce10-97";

/// The names of `[analysis] capacity` that were the core's own methods
/// and are now methods of plugins bundled with OpenOHL (2026-11-18, owner
/// 2026-11-17: the codes move out of the core, files keep reading
/// unchanged): (name in files, plugin, method). Nothing is rewritten; a
/// file keeps its name and its version.
pub const BUNDLED_ALIASES: &[(&str, &str, &str)] = &[
    ("asce10-97", "asce10", "asce10-97"),
    ("sia263-2013", "sia263", "sia263-2013"),
];

impl PluginMethod {
    /// What starts a plugin's method in `[analysis] capacity`.
    pub const PREFIX: &'static str = "plugin:";

    /// `plugin:<plugin>/<method>` read, or a name of [`BUNDLED_ALIASES`]
    /// (none: not a plugin's method).
    pub fn parse(text: &str) -> Option<Result<Self, String>> {
        if let Some((_, plugin, method)) = BUNDLED_ALIASES.iter().find(|a| a.0 == text.trim()) {
            return Some(Ok(Self {
                plugin: (*plugin).to_owned(),
                method: (*method).to_owned(),
                dir: None,
                parameters: None,
            }));
        }
        let rest = text.strip_prefix(Self::PREFIX)?;
        Some(match rest.split_once('/') {
            Some((p, m)) if !p.trim().is_empty() && !m.trim().is_empty() => Ok(Self {
                plugin: p.trim().to_owned(),
                method: m.trim().to_owned(),
                dir: None,
                parameters: None,
            }),
            _ => Err(format!(
                "capacity \"{text}\": a plugin's method is written plugin:<plugin>/<method>"
            )),
        })
    }

    /// As a file names it (a bundled plugin's method by its old name,
    /// [`BUNDLED_ALIASES`]).
    pub fn name(&self) -> String {
        match self.alias() {
            Some(a) => a.to_owned(),
            None => format!("{}{}/{}", Self::PREFIX, self.plugin, self.method),
        }
    }

    /// The old name of a bundled plugin's method ([`BUNDLED_ALIASES`];
    /// none: another plugin's).
    pub fn alias(&self) -> Option<&'static str> {
        BUNDLED_ALIASES
            .iter()
            .find(|a| a.1 == self.plugin && a.2 == self.method)
            .map(|a| a.0)
    }
}

/// How a structure file's joint details (`[[connection]]`) went into the
/// model (owner 2026-11-11: "the structural calculations should be kept
/// on their simpler form with an option to consider the full joint once
/// it's modelled"): taken only when the file asks for it
/// (`joint_details = true`) and is level 4
/// ([`ohl_structures::Detail::Joints`]).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum JointUse {
    /// No connections in the file (or a model file): nothing to say.
    #[default]
    NoDetails,
    /// Connections in the file, not asked for: the simple form.
    Simple,
    /// Asked for, not applied (why, in words): the simple form.
    NotApplied(String),
    /// Taken: each end's eccentricity, restraint and bolts from its
    /// connection, a connection's `shift` as a rigid end offset.
    Taken,
}

impl JointUse {
    /// What the file's text asks for and whether its level allows it.
    pub fn of(text: &str, file: &toml::Table) -> Self {
        let has = file
            .get("connection")
            .and_then(toml::Value::as_array)
            .is_some_and(|a| !a.is_empty());
        if file.get("joint_details").and_then(toml::Value::as_bool) != Some(true) {
            return if has { Self::Simple } else { Self::NoDetails };
        }
        use ohl_structures::LibraryItem;
        match ohl_structures::StructureType::parse(Path::new("structure.ohls"), text) {
            Ok(t) => match t.joint_details_not_applied() {
                None => Self::Taken,
                Some(why) => Self::NotApplied(why),
            },
            Err(e) => Self::NotApplied(format!(
                "joint details asked for (joint_details = true) but not taken: the structure \
                 file does not read ({}); members checked in the simple form",
                e.message
            )),
        }
    }

    /// Whether the connections go into the model.
    pub fn taken(&self) -> bool {
        *self == Self::Taken
    }

    /// The line for the results (none: no connections, nothing to say).
    pub fn note(&self) -> Option<String> {
        match self {
            Self::NoDetails => None,
            Self::Simple => Some(
                "members checked in the simple form; joint details not taken (the file's \
                 connections are used only when joint_details = true, at level 4)"
                    .to_owned(),
            ),
            Self::NotApplied(why) => Some(why.clone()),
            Self::Taken => Some(
                "joint details taken: each member end's eccentricity, restraint and bolts from \
                 its connection, a connection's shift as a rigid end offset"
                    .to_owned(),
            ),
        }
    }
}

fn dof_index(name: &str) -> Option<usize> {
    ["x", "y", "z", "rx", "ry", "rz"]
        .iter()
        .position(|d| *d == name)
}

impl Model {
    /// Parses and checks a model file's text.
    pub fn parse(text: &str) -> Result<Self, String> {
        Self::parse_with(text, &SectionLibrary::default())
    }

    /// Parses and checks a model file's text, a member's section looked
    /// up in `library` first, then in the file's `[[section]]` tables.
    pub fn parse_with(text: &str, library: &SectionLibrary) -> Result<Self, String> {
        let file: ModelFile = toml::from_str(text).map_err(|e| e.to_string())?;
        ohl_core::files::check_header(
            file.format.as_deref(),
            file.version,
            MODEL_FILE_FORMAT,
            MODEL_FILE_VERSION,
            "structure model",
        )?;
        // The design code: a plugin's method (2026-11-18: every code is,
        // none named the default ASCE 10-97 of the bundled plugin asce10;
        // nothing rewritten), or the core's simple check.
        let capacity_plugin = match file
            .analysis
            .capacity
            .as_deref()
            .map_or(PluginMethod::parse(DEFAULT_METHOD), PluginMethod::parse)
        {
            None => None,
            Some(p) => Some(p.map_err(|e| format!("analysis: {e}"))?),
        };
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
            let (iy, iz) = (s.iy.si(), s.iz.si());
            if section_index.insert(s.id.clone(), sections.len()).is_some() {
                return Err(format!("section {} given twice", s.id));
            }
            sections.push(Section {
                id: s.id.clone(),
                area: s.area.si(),
                iy,
                iz,
                j: s.torsion.map_or(iy + iz, SecondMoment::si),
                i_min: s.i_min.map_or(iy.min(iz), SecondMoment::si),
                wy: s.wy.map(SectionModulus::si),
                wz: s.wz.map(SectionModulus::si),
                e,
                g: m.shear_modulus.map_or(e / 2.6, |g| g.si()),
                density: m.density.si(),
                yield_strength: m.yield_strength.map(|y| y.si()),
                ultimate: m.ultimate.map(|u| u.si()),
                tension: s.tension.map(|f| f.si()),
                compression: s.compression.map(|f| f.si()),
                moment_y: s.moment_y.map(|m| m.0),
                moment_z: s.moment_z.map(|m| m.0),
                profile: None,
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
        let mut partners: Vec<(usize, String)> = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        // Library sections taken in (one replaces a table of its id).
        let mut taken = std::collections::BTreeSet::new();
        for m in &file.members {
            if !seen.insert(m.id.as_str()) {
                return Err(format!("member {} given twice", m.id));
            }
            let what = format!("member {}", m.id);
            if let Some(p) = library.section(&m.section)
                && taken.insert(m.section.clone())
            {
                let s = Section::from_library(p, library)?;
                match section_index.get(&m.section) {
                    Some(&k) => sections[k] = s,
                    None => {
                        section_index.insert(m.section.clone(), sections.len());
                        sections.push(s);
                    }
                }
            }
            let (i, j) = (node(&m.nodes[0], &what)?, node(&m.nodes[1], &what)?);
            let section = *section_index
                .get(&m.section)
                .ok_or_else(|| format!("{what}: no section {}", m.section))?;
            let reference = match &m.orientation {
                None => m.reference,
                Some(o) => {
                    // A roll turns the legs further (a correction, owner
                    // 2026-11-04: "a correction angle for the members to
                    // rotate them on their axis"); a reference is the
                    // orientation's own.
                    if m.reference.is_some() {
                        return Err(format!("{what}: orientation or reference (not both)"));
                    }
                    let d = sub(nodes[j].point, nodes[i].point);
                    let l = norm(d);
                    if l.is_nan() || l <= 1e-9 {
                        return Err(format!("{what}: zero length"));
                    }
                    let s = &sections[section];
                    let symmetric = s.iy == s.iz && s.wy == s.wz;
                    Some(
                        orientation_reference(o, d.map(|c| c / l), symmetric)
                            .map_err(|e| format!("{what}: {e}"))?,
                    )
                }
            };
            let design = match &m.design {
                None => crate::design::MemberDesign::default(),
                Some(d) => {
                    let (design, partner) = d.read(&what)?;
                    if let Some(p) = partner {
                        partners.push((members.len(), p));
                    }
                    design
                }
            };
            members.push(Member {
                design,
                id: m.id.clone(),
                kind: m.kind,
                i,
                j,
                section,
                reference,
                roll: m.roll.si(),
                k: m.k,
                pretension: m.pretension.si(),
                lk: m.lk.map(Length::si),
                capacity_tension: m.capacity_tension.map(Force::si),
                capacity_compression: m.capacity_compression.map(Force::si),
                dead_load_factor: m.dead_load_factor,
                bending: m.bending,
                check: m.check,
                offsets: [[0.0; 3]; 2],
                free_offsets: Default::default(),
                releases: Default::default(),
                slip: 0.0,
                axial_share: 1.0,
            });
        }
        for (k, partner) in partners {
            let at = members
                .iter()
                .position(|o: &Member| o.id == partner)
                .filter(|&o| o != k)
                .ok_or_else(|| {
                    format!(
                        "member {}: design crossing: no other member {partner}",
                        members[k].id
                    )
                })?;
            members[k].design.crossing = Some(at);
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
                drag: file.analysis.drag.unwrap_or(1.0),
                drag_given: file.analysis.drag.is_some(),
                stabilise: file.analysis.stabilise,
                dead_load_factor: file.analysis.dead_load_factor.unwrap_or(1.0),
                dead_load_given: file.analysis.dead_load_factor.is_some(),
                single_angle_moments: file.analysis.single_angle_moments,
                capacity: match &file.analysis.capacity {
                    None => CapacityMethod::default(),
                    Some(_) if capacity_plugin.is_some() => CapacityMethod::default(),
                    Some(c) => CapacityMethod::parse(c).map_err(|e| format!("analysis: {e}"))?,
                },
                tension_compression: match &file.analysis.tension_compression {
                    None => TensionCompression::default(),
                    Some(t) => {
                        TensionCompression::parse(t).map_err(|e| format!("analysis: {e}"))?
                    }
                },
                redundant_force: file.analysis.redundant_force.map(|p| p / 100.0),
                climbing_load: file.analysis.climbing_load.map(|f| f.si()),
                end_releases: false,
                bolt_slip: false,
            },
            nodes,
            sections,
            members,
            loads,
            joint_points: Vec::new(),
            unlinked_points: Vec::new(),
            joint_use: JointUse::default(),
            capacity_plugin,
            method_compression: Vec::new(),
            bolt_defaults: Default::default(),
            bolt_grade_notes: Vec::new(),
            joint_stiffness: None,
        };
        model.check()?;
        Ok(model)
    }

    /// The model of a structure type file (`.ohls`) that holds its
    /// members: its `[[node]]`, `[[member]]`, `[[section]]`,
    /// `[[material]]`, `[[load]]` and `[analysis]` tables. A `[[point]]` on
    /// a joint (`node = "…"`) makes that joint carry the point's loads; one
    /// a file written before places along a member (`member`, `at`) is on
    /// a joint inserted there, the member split into continuous pieces
    /// ([`ohl_structures::on_members`]).
    pub fn from_structure_type(text: &str) -> Result<Self, String> {
        Self::from_structure_type_with(text, &SectionLibrary::default())
    }

    /// The same, a member's section looked up in `library` first.
    pub fn from_structure_type_with(text: &str, library: &SectionLibrary) -> Result<Self, String> {
        Self::from_structure_type_extended(text, library, None)
    }

    /// The model of a structure file standing on its extension
    /// `extension` (none: as written): the extension's joints and members
    /// added, the supports and leg joints as
    /// [`ohl_structures::StructureType::extended`] makes them (the rule
    /// of [`ohl_structures::extensions`]); an extension without a body or
    /// legs gives the model as written. A member's section is looked up
    /// in `library` first.
    pub fn from_structure_type_extended(
        text: &str,
        library: &SectionLibrary,
        extension: Option<&str>,
    ) -> Result<Self, String> {
        Self::from_structure_type_with_bolts(text, library, &BoltLibrary::default(), extension)
    }

    /// The same, the bolts its connections name (`[[connection]] bolts =
    /// { bolt = "…" }`) looked up in `bolts` (stage 4 of the joint
    /// details, 2026-11-02). This is the calculation's model (owner
    /// 2026-11-11): the connections are taken only when the file asks for
    /// it (`joint_details = true`) at level 4 — each member end's
    /// connection then read into its design data
    /// ([`crate::design::MemberDesign::ends`], [`crate::member_ends`]) and
    /// its `shift` into an end offset; otherwise the members are as in a
    /// tower without connections. [`Model::joint_use`] says which.
    pub fn from_structure_type_with_bolts(
        text: &str,
        library: &SectionLibrary,
        bolts: &BoltLibrary,
        extension: Option<&str>,
    ) -> Result<Self, String> {
        Self::from_structure_type_joints(text, library, bolts, extension, None)
    }

    /// The model with the joint details always read, whatever the file's
    /// option and level: for the joints' own design aid (what a joint's
    /// table and sketch show, the plates' and covers' checks, drawings
    /// and exports), never for the members' check of a run.
    pub fn from_structure_type_with_joint_details(
        text: &str,
        library: &SectionLibrary,
        bolts: &BoltLibrary,
        extension: Option<&str>,
    ) -> Result<Self, String> {
        Self::from_structure_type_joints(text, library, bolts, extension, Some(JointUse::Taken))
    }

    fn from_structure_type_joints(
        text: &str,
        library: &SectionLibrary,
        bolts: &BoltLibrary,
        extension: Option<&str>,
        joints: Option<JointUse>,
    ) -> Result<Self, String> {
        let mut file: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
        let joints = joints.unwrap_or_else(|| JointUse::of(text, &file));
        if let Some(name) = extension {
            use ohl_structures::LibraryItem;
            let t = ohl_structures::StructureType::parse(Path::new("structure.ohls"), text)
                .map_err(|e| e.message)?;
            let e = t
                .extension(name)
                .ok_or_else(|| format!("no extension \"{name}\""))?;
            if e.changes_model() {
                let x = t.extended(Some(name))?;
                with_extension(&mut file, name, &x);
            }
        }
        Self::from_structure_table(file, library, bolts, joints)
    }

    /// The model of a structure file's tables.
    fn from_structure_table(
        mut file: toml::Table,
        library: &SectionLibrary,
        bolts: &BoltLibrary,
        joints: JointUse,
    ) -> Result<Self, String> {
        ohl_structures::on_members::apply_to_table(&mut file)?;
        let mut model = toml::Table::new();
        for key in [
            "name", "source", "analysis", "material", "section", "member", "load",
        ] {
            if let Some(v) = file.get(key) {
                model.insert(key.into(), v.clone());
            }
        }
        // What only the structure file knows (a member's group, what a
        // generator made) is not part of the model.
        let strip = |items: &mut Vec<toml::Value>, keys: &[&str]| {
            for t in items.iter_mut().filter_map(toml::Value::as_table_mut) {
                for k in keys {
                    t.remove(*k);
                }
            }
        };
        if let Some(members) = model.get_mut("member").and_then(toml::Value::as_array_mut) {
            strip(members, &["group", "made_by", "bolt_grade"]);
        }
        let mut nodes = file
            .get("node")
            .and_then(toml::Value::as_array)
            .cloned()
            .unwrap_or_default();
        // A joint's kind (attach, support) is the structure's, not the model's.
        strip(&mut nodes, &["made_by", "kind"]);
        model.insert("node".into(), nodes.into());
        let mut out = Self::parse_with(
            &toml::to_string(&model).map_err(|e| e.to_string())?,
            library,
        )?;
        out.link_points(file.get("point").and_then(toml::Value::as_array))?;
        if joints.taken() {
            crate::member_ends::read_connections(&mut out, &file, bolts)?;
        }
        out.joint_use = joints;
        // The bolt grades set on members' bolts (version 21), over the
        // connections' library bolts and the members' own.
        out.bolt_grade_notes = crate::member_ends::apply_bolt_grades(&mut out, &file)?;
        // The structure's default minimum bolt distances (version 18): the
        // bolt tools' where no code gives any.
        if let Some(d) = file.get("default_bolt_minimums") {
            let d: ohl_structures::bolts::DefaultBoltMinimums = d
                .clone()
                .try_into()
                .map_err(|e| format!("default_bolt_minimums: {e}"))?;
            d.validate()?;
            out.bolt_defaults = d;
        }
        out.check()?;
        Ok(out)
    }

    /// Reads a model file (`.ohlfe`), or a structure type file (`.ohls`)
    /// holding its members.
    pub fn load(path: &Path) -> Result<Self, String> {
        Self::load_with(path, &SectionLibrary::default())
    }

    /// The same, a member's section looked up in `library` first.
    pub fn load_with(path: &Path, library: &SectionLibrary) -> Result<Self, String> {
        Self::load_extended(path, library, None)
    }

    /// The same; a structure file standing on its extension `extension`
    /// ([`Model::from_structure_type_extended`]; a model file has no
    /// extensions: refused with one). The bolts a structure file's
    /// connections name are those of the bolt libraries beside it
    /// (`Bolts/`, [`BoltLibrary::beside`]; a library file that does not
    /// read is left out, its bolts then said to be in no library).
    pub fn load_extended(
        path: &Path,
        library: &SectionLibrary,
        extension: Option<&str>,
    ) -> Result<Self, String> {
        Self::load_with_bolts(path, library, None, extension)
    }

    /// The same, the bolts from `bolts` (none: the bolt libraries beside
    /// the file).
    pub fn load_with_bolts(
        path: &Path,
        library: &SectionLibrary,
        bolts: Option<&BoltLibrary>,
        extension: Option<&str>,
    ) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if ohl_core::files::has_extension(path, STRUCTURE_FILE_EXTENSION) {
            let beside;
            let bolts = match bolts {
                Some(b) => b,
                None => {
                    beside = BoltLibrary::beside(path.parent().unwrap_or(Path::new("."))).0;
                    &beside
                }
            };
            Self::from_structure_type_with_bolts(&text, library, bolts, extension)
        } else if let Some(e) = extension {
            Err(format!("a model file has no extension \"{e}\""))
        } else {
            Self::parse_with(&text, library)
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
        if self.analysis.drag.is_nan() || self.analysis.drag <= 0.0 {
            return Err("analysis: drag must be above 0".into());
        }
        let factor = self.analysis.dead_load_factor;
        if !(factor.is_finite() && factor > 0.0) {
            return Err("analysis: dead_load_factor must be above 0".into());
        }
        if let Some(r) = self.analysis.redundant_force
            && !(r.is_finite() && r > 0.0 && r <= 1.0)
        {
            return Err(
                "analysis: redundant_force must be a percentage above 0, at most 100".into(),
            );
        }
        if let Some(p) = self.analysis.climbing_load
            && !(p.is_finite() && p > 0.0)
        {
            return Err("analysis: climbing_load must be above 0".into());
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
            if m.offsets.iter().flatten().any(|x| !x.is_finite())
                || m.free_offsets
                    .iter()
                    .flatten()
                    .flatten()
                    .any(|x| !x.is_finite())
                || norm(sub(self.end_point(m, 1), self.end_point(m, 0))) <= 1e-9
            {
                return Err(format!(
                    "member {}: end offsets not valid (its ends meet)",
                    m.id
                ));
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
            if m.bending && m.kind != MemberKind::Tension {
                return Err(format!(
                    "member {}: bending is for tension-only members (a frame always bends)",
                    m.id
                ));
            }
            for (what, v) in [
                ("lk", m.lk),
                ("capacity_tension", m.capacity_tension),
                ("capacity_compression", m.capacity_compression),
                ("dead_load_factor", m.dead_load_factor),
            ] {
                if let Some(v) = v
                    && !(v.is_finite() && v > 0.0)
                {
                    return Err(format!("member {}: {what} must be above 0", m.id));
                }
            }

            if m.is_frame() {
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

    /// Length of a member, m.
    pub fn length(&self, m: &Member) -> f64 {
        norm(sub(self.nodes[m.j].point, self.nodes[m.i].point))
    }

    /// Where end `end` (0 at its first joint, 1 at its second) of member
    /// `m` is: its joint moved by its rigid end offset
    /// ([`Member::offsets`]), structure axes, m.
    pub fn end_point(&self, m: &Member, end: usize) -> [f64; 3] {
        let p = self.nodes[if end == 0 { m.i } else { m.j }].point;
        let r = m.offsets[end];
        [p[0] + r[0], p[1] + r[1], p[2] + r[2]]
    }

    /// Whether a member's section has a profile (from the section
    /// library): the model takes member wind ([`Self::member_wind`]).
    pub fn has_profiles(&self) -> bool {
        self.members
            .iter()
            .any(|m| self.sections[m.section].profile.is_some())
    }

    /// The wind on the members whose section has a profile, as loads at
    /// their joints. Per member: `pressure` (Pa) × the drag factor (the
    /// model's `drag` when given, else the shape's built-in force
    /// coefficient, [`crate::wind_law::code_drag`]) × the profile's width
    /// × its length × the inclination factor k(β) (β between the wind and
    /// the member's normal, [`crate::wind_law::inclination`]; 2026-11-15,
    /// before sin²θ), normal to the member in the plane of the member and
    /// the wind, half at each joint. The width is the profile's for the
    /// wind's direction in the member's local y–z plane (the section's y
    /// and z). `direction`: where the wind blows to, structure axes. No
    /// member shields another.
    pub fn member_wind(&self, pressure: f64, direction: [f64; 3]) -> Vec<NodalLoad> {
        self.member_wind_with(pressure, direction, None).loads
    }

    /// The member wind with wind factors given as data (`factors`: the
    /// section library's `[wind]`: drag per shape, a lattice face's drag
    /// and shielding by solidity ratio — [`crate::wind`]), and what was
    /// applied ([`crate::wind::MemberWind::notes`]). Without factors the
    /// loads are those of [`Self::member_wind`]. Computes the faces each
    /// call: for several directions or cases, build a
    /// [`crate::wind::MemberWindSetup`] once.
    pub fn member_wind_with(
        &self,
        pressure: f64,
        direction: [f64; 3],
        factors: Option<&ohl_sections::StructureWind>,
    ) -> crate::wind::MemberWind {
        crate::wind::MemberWindSetup::new(self, factors).wind(pressure, direction)
    }

    /// The area member `member` (its place) shows to a wind blowing in
    /// `direction` (structure axes), m²: the profile's width for the
    /// wind's direction in the member's y–z plane × its length × sin θ
    /// (θ between the wind and the member: the member's silhouette seen
    /// along the wind). No drag factor. None: no such member, or its
    /// section has no profile (it is not from the section library).
    pub fn projected_area(&self, member: usize, direction: [f64; 3]) -> Option<f64> {
        let m = self.members.get(member)?;
        self.sections[m.section].profile.as_ref()?;
        let n = norm(direction);
        if n.is_nan() || n <= 0.0 {
            return Some(0.0);
        }
        Some(
            self.across(m, direction.map(|d| d / n))
                .map_or(0.0, |(_, sin, width)| width * self.length(m) * sin),
        )
    }

    /// For a member with a profile and the unit wind `w`: the wind's
    /// component normal to the member, its length (sin θ) and the
    /// profile's width for it. None: no profile, or the wind along the
    /// member.
    pub fn across(&self, m: &Member, w: [f64; 3]) -> Option<([f64; 3], f64, f64)> {
        let p = self.sections[m.section].profile.as_ref()?;
        let r = crate::element::axes(self, m);
        let row = |k: usize| [r[(k, 0)], r[(k, 1)], r[(k, 2)]];
        let x = row(0);
        let along = dot(w, x);
        // The wind's component normal to the member (length sin θ).
        let normal = [
            w[0] - along * x[0],
            w[1] - along * x[1],
            w[2] - along * x[2],
        ];
        let sin = norm(normal);
        if sin <= 1e-9 {
            return None;
        }
        let angle = dot(normal, row(2)).atan2(dot(normal, row(1)));
        Some((normal, sin, p.wind_width(Angle::from_radians(angle))))
    }

    /// The factor on member `m`'s self-weight: its own, else the
    /// analysis's.
    pub fn dead_load_factor(&self, m: &Member) -> f64 {
        m.dead_load_factor.unwrap_or(self.analysis.dead_load_factor)
    }

    /// Total weight of the members as the self-weight loads them, N: each
    /// member's weight × its dead-load factor ([`Self::steel_weight`]
    /// without the factors).
    pub fn weight(&self) -> f64 {
        self.members
            .iter()
            .map(|m| self.sections[m.section].weight() * self.dead_load_factor(m) * self.length(m))
            .sum()
    }

    /// Total weight of the members' sections, N (no dead-load factors).
    pub fn steel_weight(&self) -> f64 {
        self.members
            .iter()
            .map(|m| self.sections[m.section].weight() * self.length(m))
            .sum()
    }
}

pub(crate) fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn norm(a: [f64; 3]) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

/// The tables of a structure file with extension `name` as `x` makes it:
/// the extension's own `node` and `member` tables added (all their
/// fields), the supports of the joints standing on it dropped, the joints
/// a leg lowers moved.
fn with_extension(file: &mut toml::Table, name: &str, x: &ohl_structures::ExtendedBody) {
    let own = |key: &str| -> Vec<toml::Value> {
        file.get("extension")
            .and_then(toml::Value::as_array)
            .and_then(|a| {
                a.iter()
                    .find(|e| e.get("name").and_then(toml::Value::as_str) == Some(name))
            })
            .and_then(|e| e.get(key))
            .and_then(toml::Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let (own_nodes, own_members) = (own("node"), own("member"));
    let mut nodes = file
        .get("node")
        .and_then(toml::Value::as_array)
        .cloned()
        .unwrap_or_default();
    nodes.extend(own_nodes);
    for n in nodes.iter_mut().filter_map(toml::Value::as_table_mut) {
        let id = n
            .get("id")
            .and_then(toml::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if x.unsupported.contains(&id) {
            n.remove("support");
            n.remove("spring");
        }
        if x.lowered.iter().any(|(j, _)| *j == id)
            && let Some(at) = x.nodes.iter().find(|m| m.id == id)
        {
            let point: Vec<toml::Value> = at
                .point
                .iter()
                .map(|v| toml::Value::String(format!("{} m", v.si())))
                .collect();
            n.insert("point".into(), point.into());
        }
    }
    file.insert("node".into(), nodes.into());
    let mut members = file
        .get("member")
        .and_then(toml::Value::as_array)
        .cloned()
        .unwrap_or_default();
    members.extend(own_members);
    file.insert("member".into(), members.into());
}
