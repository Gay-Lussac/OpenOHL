// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A structure's members as the check reads them: the records of the
//! `member-check-full` interface in the types the code was written
//! against in OpenOHL's core (`ohl_structures_fe::model`, `::design`,
//! `::member_ends`; moved 2026-11-18), so that the formulas move
//! unchanged. Only what the SIA 263 check reads is kept.

/// How a member's ends are connected or restrained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ends {
    /// Neither end.
    None,
    /// One end.
    OneEnd,
    /// Both ends.
    BothEnds,
}

impl Ends {
    /// From its name in files ("" or another: none).
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "none" => Ends::None,
            "one_end" => Ends::OneEnd,
            "both_ends" => Ends::BothEnds,
            _ => return None,
        })
    }
}

/// Which leg of an angle its end connections bolt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectedLeg {
    /// Both legs.
    Both,
    /// The long leg only.
    Long,
    /// The short leg only.
    Short,
    /// No leg bolted.
    None,
}

impl ConnectedLeg {
    /// From its name in files ("" or another: none).
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "both" => ConnectedLeg::Both,
            "long" => ConnectedLeg::Long,
            "short" => ConnectedLeg::Short,
            "none" => ConnectedLeg::None,
            _ => return None,
        })
    }

    /// One leg only bolted (long or short).
    pub fn one_leg(self) -> bool {
        matches!(self, ConnectedLeg::Long | ConnectedLeg::Short)
    }
}

/// What a member is for in a lattice structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberClass {
    /// A leg.
    Leg,
    /// Bracing carrying the structure's loads.
    Bracing,
    /// A redundant.
    Redundant,
    /// A crossing diagonal.
    Crossing,
}

impl MemberClass {
    /// From its name in files ("" or another: none).
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "leg" => MemberClass::Leg,
            "bracing" => MemberClass::Bracing,
            "redundant" => MemberClass::Redundant,
            "crossing" => MemberClass::Crossing,
            _ => return None,
        })
    }
}

/// A bolt's strength grade ("8.8").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grade {
    /// The number before the point.
    pub major: u8,
    /// The number after it.
    pub minor: u8,
}

impl Grade {
    /// A grade from its name; none when it is not two numbers with a point
    /// between them.
    pub fn parse(text: &str) -> Option<Self> {
        let (a, b) = text.trim().split_once('.')?;
        Some(Self {
            major: a.trim().parse().ok()?,
            minor: b.trim().parse().ok()?,
        })
    }
}

/// A member's end bolts (each end alike), SI units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bolts {
    /// Bolt diameter, m.
    pub diameter: f64,
    /// Hole diameter, m.
    pub hole: Option<f64>,
    /// Bolts at one end.
    pub count: u32,
    /// Shear planes per bolt.
    pub shear_planes: u32,
    /// Shear capacity of one bolt per shear plane, N.
    pub shear_capacity: Option<f64>,
    /// Ultimate tensile strength of the bolt, Pa.
    pub ultimate: Option<f64>,
    /// Holes in one cross-section.
    pub holes: Option<u32>,
    /// Their grade.
    pub grade: Option<Grade>,
}

/// The bolts of one end's connection.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EndBolts {
    /// How many.
    pub count: u32,
    /// Diameter, m.
    pub diameter: Option<f64>,
    /// Hole, m.
    pub hole: Option<f64>,
    /// Shear planes per bolt.
    pub shear_planes: Option<u32>,
    /// Shear capacity per plane through the threads, N.
    pub shear_capacity: Option<f64>,
    /// The same through the shank, N.
    pub shear_capacity_shank: Option<f64>,
    /// Ultimate strength, Pa.
    pub ultimate: Option<f64>,
    /// The threads are in the shear plane.
    pub threads_in_shear: Option<bool>,
    /// Grade.
    pub grade: Option<Grade>,
}

/// What a member end bolts onto.
#[derive(Debug, Clone, PartialEq)]
pub enum Onto {
    /// Another member.
    Member,
    /// A plate.
    Plate,
    /// A splice's covers.
    Splice,
    /// Something else.
    Other,
}

/// One end's joint detail (`[[connection]]`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EndDesign {
    /// `bolted`, `welded`, … (none: not given).
    pub kind: Option<String>,
    /// What it bolts onto.
    pub onto: Option<Onto>,
    /// The leg bolted.
    pub leg: Option<ConnectedLeg>,
    /// Its bolts.
    pub bolts: Option<EndBolts>,
    /// Holes in one cross-section.
    pub holes: Option<u32>,
    /// Gauge lines from the heel, m.
    pub gauges: Vec<f64>,
    /// Edge distance, m.
    pub edge_distance: Option<f64>,
    /// End distance, m.
    pub end_distance: Option<f64>,
    /// Pitch, m.
    pub pitch: Option<f64>,
    /// Shear planes worked out, and why.
    pub planes: Option<(u32, String)>,
    /// Runs on through the joint with another member.
    pub continuous: bool,
}

/// A member's design data (all optional), SI units.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MemberDesign {
    /// What it is for.
    pub class: Option<MemberClass>,
    /// Buckling lengths over the length about y, z and v.
    pub length_ratios: Option<[f64; 3]>,
    /// Thickness of the connected legs, m.
    pub thickness: Option<f64>,
    /// Profiles side by side.
    pub plies: Option<u32>,
    /// The leg its end connections bolt.
    pub connected_leg: Option<ConnectedLeg>,
    /// Its end bolts.
    pub bolts: Option<Bolts>,
    /// Edge distance, m.
    pub edge_distance: Option<f64>,
    /// End distance, m.
    pub end_distance: Option<f64>,
    /// Bolt spacing, m.
    pub spacing: Option<f64>,
    /// Each end's connection.
    pub ends: [Option<EndDesign>; 2],
}

/// Where an end condition comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Given at the member end.
    Given,
    /// The member's design data.
    Member,
    /// Worked out from the end's connection.
    WorkedOut,
    /// Assumed on the safe side.
    Assumed,
}

impl Source {
    /// From its words.
    pub fn parse(s: &str) -> Self {
        match s {
            "given" => Source::Given,
            "design data" => Source::Member,
            "worked out" => Source::WorkedOut,
            _ => Source::Assumed,
        }
    }

    /// In words, for the results.
    pub fn words(self) -> &'static str {
        match self {
            Source::Given => "given",
            Source::Member => "design data",
            Source::WorkedOut => "worked out",
            Source::Assumed => "assumed",
        }
    }
}

/// One condition at one end.
#[derive(Debug, Clone, PartialEq)]
pub struct Condition {
    /// Its value.
    pub value: Option<bool>,
    /// Where it comes from.
    pub source: Source,
    /// Why, when worked out.
    pub why: Option<String>,
}

/// The conditions of both ends of a member, as OpenOHL works them out.
#[derive(Debug, Clone, PartialEq)]
pub struct EndConditions {
    /// Eccentric, at each end.
    pub eccentric: [Condition; 2],
    /// Each end has a `[[connection]]`.
    pub connected: [bool; 2],
}

/// The leg of the member an end bolts onto.
#[derive(Debug, Clone, PartialEq)]
pub struct OntoLeg {
    /// The member's id.
    pub id: String,
    /// Its leg's thickness, m.
    pub thickness: f64,
    /// Its steel's ultimate strength, Pa.
    pub fu: Option<f64>,
}

/// A member's section, SI units.
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    /// Area, m².
    pub area: f64,
    /// Second moments, m⁴.
    pub iy: f64,
    /// See `iy`.
    pub iz: f64,
    /// Torsion constant, m⁴.
    pub j: f64,
    /// Minor principal second moment, m⁴.
    pub i_min: f64,
    /// Elastic moduli, m³.
    pub wy: Option<f64>,
    /// See `wy`.
    pub wz: Option<f64>,
    /// Young's modulus, Pa.
    pub e: f64,
    /// Shear modulus, Pa.
    pub g: f64,
    /// Yield strength, Pa.
    pub yield_strength: Option<f64>,
    /// Ultimate strength, Pa.
    pub ultimate: Option<f64>,
    /// Allowable tension, N.
    pub tension: Option<f64>,
    /// Allowable compression, N.
    pub compression: Option<f64>,
    /// Allowable moments, N·m.
    pub moment_y: Option<f64>,
    /// See `moment_y`.
    pub moment_z: Option<f64>,
    /// The library profile.
    pub profile: Option<ohl_sections::Section>,
    /// Its properties as OpenOHL worked them out.
    pub properties: Option<ohl_sections::Properties>,
}

/// `truss`, `frame`, `cable` or `tension`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberKind {
    /// A truss.
    Truss,
    /// A frame.
    Frame,
    /// A cable.
    Cable,
    /// A tension-only member.
    Tension,
}

/// One member and its section.
#[derive(Debug, Clone, PartialEq)]
pub struct Member {
    /// Id.
    pub id: String,
    /// Kind.
    pub kind: MemberKind,
    /// Checked as a frame.
    pub frame: bool,
    /// Design data.
    pub design: MemberDesign,
    /// Its section.
    pub section: Section,
    /// Length between its joints, m.
    pub length: f64,
    /// Effective length factor.
    pub k: f64,
    /// Effective length, m.
    pub lk: Option<f64>,
    /// Tension capacity given, N.
    pub capacity_tension: Option<f64>,
    /// Compression capacity given, N.
    pub capacity_compression: Option<f64>,
    /// The joints at its two ends.
    pub joints: [String; 2],
    /// Its ends' conditions.
    pub conditions: EndConditions,
    /// The legs its ends bolt onto.
    pub onto: [Option<OntoLeg>; 2],
}

impl Member {
    /// Checked as a frame (its moments).
    pub fn is_frame(&self) -> bool {
        self.frame
    }
}

/// A member's forces in one load case.
#[derive(Debug, Clone, PartialEq)]
pub struct MemberForces {
    /// Axial force, N, tension positive.
    pub axial: f64,
    /// End forces, local axes.
    pub end_forces: [f64; 12],
    /// Slack.
    pub slack: bool,
    /// Bowed.
    pub bowed: bool,
    /// Length in the solve, m.
    pub length: f64,
    /// The load along it the end forces include, N/m, local.
    pub span_load: [f64; 3],
    /// The load along it put on its joints, N/m, local.
    pub lumped_load: [f64; 3],
}

/// The member a redundant member braces in a load case.
#[derive(Debug, Clone, PartialEq)]
pub struct Supported {
    /// The braced member's id.
    pub member: String,
    /// The joint.
    pub joint: String,
    /// The compression there, N.
    pub force: f64,
    /// The braced member is itself redundant.
    pub via_redundant: bool,
}

/// What the run's `[analysis]` says.
#[derive(Debug, Clone, PartialEq)]
pub struct Analysis {
    /// `single_angle_moments`.
    pub single_angle_moments: bool,
    /// `tension_compression = "none"`.
    pub no_compression: bool,
    /// `redundant_force` as a share.
    pub redundant_force: Option<f64>,
}
