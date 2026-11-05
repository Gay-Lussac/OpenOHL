// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The connections of a tower's members at its joints (structure file
//! version 9, 2026-11-02; [Joint details of towers], stage 1): one
//! `[[connection]]` per member end that says more than the member's own
//! design data, and the `[[plate]]`s (gusset plates) members bolt onto.
//!
//! ```toml
//! [[connection]]
//! member = "D12"          # the member …
//! joint = "N7"            # … at its end at this joint
//! kind = "bolted"         # bolted (default), welded, pinned, fixed
//! onto = "L3"             # a member meeting at the joint, or a plate
//! leg = "long"            # the angle's leg bolted: long, short, both, none
//! bolts = { count = 2, diameter = "16 mm", hole = "17.5 mm", grade = "8.8" }
//! # or a bolt of a library: bolts = { count = 2, bolt = "M16-8.8" }
//! gauge_lines = 1        # 1 to 3
//! gauges = ["30 mm"]      # from the heel along the bolted leg, per line
//! pitch = "50 mm"
//! end_distance = "30 mm"
//! edge_distance = "25 mm"
//! # rolled_edge = true  # the leg's edge rolled (none: sheared, on the safe side)
//! # bolt_points = [["30 mm", "30 mm"], ["80 mm", "30 mm"]]  # instead of
//! #   the layout: along the member from the joint, across from the heel
//! # leg = "both" lays the layout on both legs, the bolts shared (the leg
//! #   on what it bolts onto first); other_leg_points (2026-11-05) gives
//! #   the other leg's bolts by hand, as bolt_points (theirs: the leg on
//! #   what it bolts onto)
//! # eccentric = true / restrained = false   # given instead of worked out
//! # cut = "40 mm"   # where the steel stops from the work point (none:
//! #   worked out so it fits); the axis still runs to the work point
//! # shift = "15 mm"  # the end moved sideways to fit (none: 0); its force
//! #   then passes beside the work point: an eccentricity, analysed
//! # face = "outside"  # where on what it bolts onto (2026-11-05): outside
//! #   (default), inside (the inside face of its leg), nested (an angle
//! #   heel into heel inside the other: a lap joint, one shear plane)
//!
//! [[connection]]          # one part through the joint (2026-11-05):
//! member = "L1"           #   no bolts, the two members in line, of the
//! joint = "N2"            #   same section, roll and reference
//! kind = "continuous"
//! onto = "L2"
//!
//! [[splice]]              # covers bolting members together (2026-11-05)
//! id = "S1"
//! joint = "N2"
//! members = ["L1", "L2"]
//! [[splice.cover]]        # per leg (long, short; none: the member's
//! face = "outer"          #   flat part) outer and/or inner: two on a
//! leg = "long"            #   leg, two shear planes
//! thickness = "8 mm"
//! # material = "S355"; outline = [...] as a plate's
//!
//! [[plate]]
//! id = "G1"
//! joint = "N7"
//! thickness = "10 mm"
//! material = "S355"       # optional
//! # outline = [["0 m", "0 m", "10 m"], …]   # local axes; none: around
//! #   the bolts of the connections onto it
//!
//! [[plate]]
//! id = "G2"
//! joint = "N7"
//! thickness = "10 mm"
//! welded_to = ["G1"]      # welded to these plates of the same joint: one
//! #   welded plate (an assembly) reaching every member bolted there
//!
//! [[plate.weld]]          # a weld of this plate to another (2026-11-04,
//! to = "G1"               #   still version 9): welds it to G1 as welded_to
//! size = "6 mm"           #   does; the fillet's throat a (SIA 263 6.3.2.6)
//! length = "220 mm"       # none: the seam's length from the geometry
//! kind = "fillet"         # fillet (default) or full_penetration
//! ```
//!
//! A **welded plate** is several plates welded together so that every
//! member bolting onto the joint reaches one of them (owner 2026-11-04).
//! The relation is given on either plate (`welded_to`) and read both ways;
//! the plates joined by it, directly or through others, are one assembly
//! ([`StructureType::welded_assemblies`]). Files of 2026-11-03 gave a
//! plate `kind = "welded"` (then: welded onto the joint's leg): still
//! read, the joint's plates of that kind taken as one welded assembly
//! (one alone: a plate), said by [`StructureType::connection_notes`].
//!
//! What a connection does not give is taken from the member's
//! `[member.design]` (the same at both ends) by the structure
//! application, which shows each value as given, worked out or assumed.
//! The ASCE 10-97 check reads them (stage 4.1 and 4.2 of the roadmap,
//! 2026-11-02; `ohl_structures_fe::member_ends`): each end's eccentricity
//! and restraint, its bolts (a library bolt's strengths) for the bolt
//! shear, bearing and net section at that end. A member without
//! connections computes as before.
//!
//! References are not refused when they lead nowhere (a member removed
//! by a generator's rebuild, a joint deleted): [`StructureType::connection_notes`]
//! says which connections and plates are left over, and they are kept.
//!
//! [Joint details of towers]: https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/joint-details.md

use ohl_core::units::Length;
use serde::Deserialize;

use crate::structures::StructureType;

/// The most gauge lines a connection may have (bolt rows along the leg;
/// three on wide legs, owner 2026-11-03).
pub const MAX_GAUGE_LINES: u32 = 3;

/// The file version that brings connections and plates.
pub const CONNECTIONS_VERSION: u32 = 9;

/// How a member end is held at its joint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionKind {
    /// Bolted (onto a member or a plate).
    #[default]
    Bolted,
    /// Welded.
    Welded,
    /// A pin: free to turn.
    Pinned,
    /// Held against turning (a rigid joint).
    Fixed,
    /// One part through the joint (2026-11-05): the member runs on into
    /// `onto` (in line, the same section, roll and reference); no bolts.
    Continuous,
}

impl ConnectionKind {
    /// Every kind, in the order offered.
    pub const ALL: [ConnectionKind; 5] = [
        ConnectionKind::Bolted,
        ConnectionKind::Welded,
        ConnectionKind::Pinned,
        ConnectionKind::Fixed,
        ConnectionKind::Continuous,
    ];

    /// Name in files.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bolted => "bolted",
            Self::Welded => "welded",
            Self::Pinned => "pinned",
            Self::Fixed => "fixed",
            Self::Continuous => "continuous",
        }
    }
}

/// The leg of an angle a member end is bolted by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoltedLeg {
    /// Both legs (the whole section takes the force).
    Both,
    /// The long leg.
    Long,
    /// The short leg.
    Short,
    /// No leg bolted.
    None,
}

impl BoltedLeg {
    /// Name in files.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Both => "both",
            Self::Long => "long",
            Self::Short => "short",
            Self::None => "none",
        }
    }
}

/// Where a member end sits on the member it bolts onto (2026-11-05).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Face {
    /// On the outside face of its leg (the default, as before).
    #[default]
    Outside,
    /// On the inside face of its leg.
    Inside,
    /// An angle heel into heel inside the other (a lap joint): bolted by
    /// one leg or both, one shear plane.
    Nested,
}

impl Face {
    /// Every face, in the order offered.
    pub const ALL: [Face; 3] = [Face::Outside, Face::Inside, Face::Nested];

    /// Name in files.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Outside => "outside",
            Self::Inside => "inside",
            Self::Nested => "nested",
        }
    }
}

/// The side of a member's leg a splice's cover lies on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverFace {
    /// On the outer face (an angle's heel side).
    Outer,
    /// On the inner face.
    Inner,
}

impl CoverFace {
    /// Both, in the order offered.
    pub const ALL: [CoverFace; 2] = [CoverFace::Outer, CoverFace::Inner];

    /// Name in files.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Outer => "outer",
            Self::Inner => "inner",
        }
    }
}

/// The leg of an angle a splice's cover lies on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverLeg {
    /// The long leg.
    Long,
    /// The short leg.
    Short,
}

impl CoverLeg {
    /// Name in files.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Long => "long",
            Self::Short => "short",
        }
    }
}

/// A cover of a splice (`[[splice.cover]]`): a flat plate on one face of
/// the members' leg.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeCover {
    /// Outer or inner.
    pub face: CoverFace,
    /// The angle's leg (none: the member's flat part, or a section that is
    /// no angle).
    #[serde(default)]
    pub leg: Option<CoverLeg>,
    /// Thickness.
    pub thickness: Length,
    /// Material (none: not given).
    #[serde(default)]
    pub material: Option<String>,
    /// Its outline in local axes (none: around the bolts onto it).
    #[serde(default)]
    pub outline: Vec<[Length; 3]>,
}

/// Covers bolting members together at a joint (`[[splice]]`,
/// 2026-11-05, still structure file version 9): each member end bolts
/// onto it (`onto` its id) with its own bolts; the bolts pass through
/// the member's leg and the covers on that leg, one shear plane per
/// cover.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeSplice {
    /// Id, unique among the splices, plates and members.
    pub id: String,
    /// The joint it is at.
    pub joint: String,
    /// The members it joins (meeting at the joint).
    #[serde(default)]
    pub members: Vec<String>,
    /// Its covers.
    #[serde(default, rename = "cover")]
    pub covers: Vec<TypeCover>,
}

impl TypeSplice {
    /// The covers on `leg` (an angle's; none: a member's flat part):
    /// those given for that leg and those given for no leg.
    pub fn covers_on(&self, leg: Option<CoverLeg>) -> Vec<&TypeCover> {
        self.covers
            .iter()
            .filter(|c| c.leg.is_none() || c.leg == leg)
            .collect()
    }

    /// The shear planes of a bolt through `leg` and its covers: one per
    /// cover (at least one).
    pub fn shear_planes(&self, leg: Option<CoverLeg>) -> u32 {
        (self.covers_on(leg).len() as u32).max(1)
    }
}

/// The most two members may turn from in line and still be one
/// continuous part, degrees (owner 2026-11-05).
pub const CONTINUOUS_ANGLE_DEG: f64 = 1.0;

/// The bolts of a member end: a bolt of a library (`bolt`) and what is
/// typed here (overriding the library's).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionBolts {
    /// How many.
    pub count: u32,
    /// A bolt of the bolt libraries, by id ([`crate::bolts`]).
    #[serde(default)]
    pub bolt: Option<String>,
    /// Diameter (none: the library bolt's; one of them needed).
    #[serde(default)]
    pub diameter: Option<Length>,
    /// Hole diameter (none: not given).
    #[serde(default)]
    pub hole: Option<Length>,
    /// Grade, as the standard names it (`"8.8"`, `"A394 type 0"`…).
    #[serde(default)]
    pub grade: Option<String>,
    /// Shear planes per bolt (none: not given).
    #[serde(default)]
    pub shear_planes: Option<u32>,
    /// The threads in the shear plane (none: not given).
    #[serde(default)]
    pub threads_in_shear: Option<bool>,
}

/// A member end at a joint (`[[connection]]`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeConnection {
    /// The member.
    pub member: String,
    /// The joint at one of its ends.
    pub joint: String,
    /// How it is held (none: bolted).
    #[serde(default)]
    pub kind: Option<ConnectionKind>,
    /// What it bolts onto: a member meeting at the joint or a plate.
    #[serde(default)]
    pub onto: Option<String>,
    /// The angle's leg it bolts by.
    #[serde(default)]
    pub leg: Option<BoltedLeg>,
    /// Its bolts.
    #[serde(default)]
    pub bolts: Option<ConnectionBolts>,
    /// Gauge lines: 1 to [`MAX_GAUGE_LINES`].
    #[serde(default)]
    pub gauge_lines: Option<u32>,
    /// The gauge of each line, from the heel along the bolted leg.
    #[serde(default)]
    pub gauges: Vec<Length>,
    /// Between bolts along a gauge line.
    #[serde(default)]
    pub pitch: Option<Length>,
    /// From the member's end to the first bolt.
    #[serde(default)]
    pub end_distance: Option<Length>,
    /// From the bolts to the edge of the leg.
    #[serde(default)]
    pub edge_distance: Option<Length>,
    /// The leg's edge is rolled (true) or sheared / flame-cut (false); none:
    /// taken as sheared, the larger minimum edge distance (owner
    /// 2026-11-03).
    #[serde(default)]
    pub rolled_edge: Option<bool>,
    /// Each bolt where it is, instead of the layout: along the member from
    /// the joint, across the bolted leg from the heel.
    #[serde(default)]
    pub bolt_points: Vec<[Length; 2]>,
    /// With `leg = "both"`: the bolts on the other leg where they are, as
    /// `bolt_points` (along the member from the joint, across that leg
    /// from the heel); `bolt_points` are then those on the leg lying on
    /// what it bolts onto (2026-11-05: bolts on both sides of the
    /// corner).
    #[serde(default)]
    pub other_leg_points: Vec<[Length; 2]>,
    /// Eccentric, given (none: worked out from the leg bolted).
    #[serde(default)]
    pub eccentric: Option<bool>,
    /// Held against turning, given (none: worked out from the bolts).
    #[serde(default)]
    pub restrained: Option<bool>,
    /// The eccentricity, given (none: from the gauge and the section).
    #[serde(default)]
    pub eccentricity: Option<Length>,
    /// Where the member's steel stops, from the joint's work point along
    /// the member (none: worked out so it fits — before the outstanding
    /// leg of what it bolts onto). The member's axis still runs to the
    /// work point: the forces meet there (owner 2026-11-02).
    #[serde(default)]
    pub cut: Option<Length>,
    /// A sideways shift of the member's end, so it fits (on a gusset plate,
    /// beside another member): across the member, towards what it bolts
    /// onto (the onto member's direction from the joint, else the joint's
    /// leg, less its part along the member: [`shift_direction`]), m;
    /// negative the other way. None: 0 — the axis runs to the work point.
    /// Its force then passes beside the work point: an eccentricity the
    /// analysis takes into account (owner 2026-11-02).
    #[serde(default)]
    pub shift: Option<Length>,
    /// Where it sits on what it bolts onto (2026-11-05; none: outside).
    #[serde(default)]
    pub face: Option<Face>,
}

/// The direction a member end's `shift` is measured in: across the
/// member (`along`, unit, from the joint), towards `towards` (unit, from
/// the joint: the direction of what it bolts onto), its part along the
/// member removed. None: they run in line.
pub fn shift_direction(along: [f64; 3], towards: [f64; 3]) -> Option<[f64; 3]> {
    let d = along[0] * towards[0] + along[1] * towards[1] + along[2] * towards[2];
    let u = [
        towards[0] - d * along[0],
        towards[1] - d * along[1],
        towards[2] - d * along[2],
    ];
    let l = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt();
    (l > 1e-6).then(|| [u[0] / l, u[1] / l, u[2] / l])
}

/// How a gusset plate was held on the joint's leg (2026-11-03), still read:
/// since 2026-11-04 the joint's plates of kind `welded` are read as one
/// welded assembly ([`TypePlate::welded_to`]) and the kind is no longer
/// written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlateKind {
    /// Bolted onto the leg.
    #[default]
    Bolted,
    /// Welded onto the leg.
    Welded,
}

impl PlateKind {
    /// Every kind, in the order offered.
    pub const ALL: [PlateKind; 2] = [PlateKind::Bolted, PlateKind::Welded];

    /// Name in files.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bolted => "bolted",
            Self::Welded => "welded",
        }
    }
}

/// How two plates are welded (`[[plate.weld]] kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeldKind {
    /// A fillet weld (the default): its size is the throat a.
    #[default]
    Fillet,
    /// A full penetration butt weld: as strong as the weaker plate (SIA
    /// 263 6.3.3.4), not checked.
    FullPenetration,
}

impl WeldKind {
    /// Every kind, in the order offered.
    pub const ALL: [WeldKind; 2] = [WeldKind::Fillet, WeldKind::FullPenetration];

    /// Name in files.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fillet => "fillet",
            Self::FullPenetration => "full_penetration",
        }
    }

    /// In words, for the tables.
    pub fn words(self) -> &'static str {
        match self {
            Self::Fillet => "fillet",
            Self::FullPenetration => "full penetration",
        }
    }
}

/// A weld of a plate to another plate of its joint (`[[plate.weld]]`,
/// 2026-11-04, still structure file version 9): it welds the two as
/// `welded_to` does, with its size and length. A pair welded by
/// `welded_to` alone has no size: not checked (said).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeWeld {
    /// The other plate.
    pub to: String,
    /// The fillet's throat a (SIA 263 6.3.2.6, Figure 34: the height of
    /// the triangle inscribed in the weld's section); none: not given,
    /// the weld not checked.
    #[serde(default)]
    pub size: Option<Length>,
    /// Its length (the whole length welded, both sides together when
    /// welded on both; none: the seam's length where the two plates meet,
    /// worked out from the geometry, one side).
    #[serde(default)]
    pub length: Option<Length>,
    /// Fillet (none) or full penetration.
    #[serde(default)]
    pub kind: Option<WeldKind>,
}

impl TypeWeld {
    /// Its kind (fillet when not given).
    pub fn kind(&self) -> WeldKind {
        self.kind.unwrap_or_default()
    }
}

/// A gusset plate at a joint (`[[plate]]`); a joint may have several.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypePlate {
    /// Id, unique among the plates and not a member's.
    pub id: String,
    /// The joint it is at.
    pub joint: String,
    /// Thickness.
    pub thickness: Length,
    /// Material (none: not given).
    #[serde(default)]
    pub material: Option<String>,
    /// Its outline in local axes (none: around the bolts onto it).
    #[serde(default)]
    pub outline: Vec<[Length; 3]>,
    /// Bolted or welded onto the joint's leg (2026-11-03; none: bolted).
    /// Read only: `welded` plates of a joint form one welded assembly
    /// (2026-11-04, [`StructureType::welded_assemblies`]).
    #[serde(default)]
    pub kind: Option<PlateKind>,
    /// The plates of the same joint it is welded to (2026-11-04, still
    /// structure file version 9: one welded plate made of several, so that
    /// every member bolted at the joint reaches one); given on either
    /// plate, read both ways.
    #[serde(default)]
    pub welded_to: Vec<String>,
    /// Its welds to other plates of the joint with their size and length
    /// (`[[plate.weld]]`, 2026-11-04, still version 9); each welds the two
    /// as `welded_to` does.
    #[serde(default, rename = "weld")]
    pub welds: Vec<TypeWeld>,
}

impl TypePlate {
    /// How it is held (bolted when not given).
    pub fn kind(&self) -> PlateKind {
        self.kind.unwrap_or_default()
    }

    /// The plates it names as welded to it: `welded_to` and its welds'
    /// `to`, each once.
    pub fn welded_ids(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for x in self
            .welded_to
            .iter()
            .chain(self.welds.iter().map(|w| &w.to))
        {
            if !out.contains(&x.as_str()) {
                out.push(x.as_str());
            }
        }
        out
    }
}

impl TypeConnection {
    /// How it is held (bolted when not given).
    pub fn kind(&self) -> ConnectionKind {
        self.kind.unwrap_or_default()
    }

    /// Where it sits (outside when not given).
    pub fn face(&self) -> Face {
        self.face.unwrap_or_default()
    }
}

impl StructureType {
    /// Why `member` cannot run on through `joint` into `onto` as one part
    /// (none: it can): both meet there, in line within
    /// [`CONTINUOUS_ANGLE_DEG`], of the same section, roll and reference.
    pub fn continuous_problem(&self, member: &str, onto: &str, joint: &str) -> Option<String> {
        if member == onto {
            return Some("it cannot run on into itself".into());
        }
        let find = |id: &str| {
            self.members
                .iter()
                .find(|m| m.id == id && m.nodes.contains(&joint.to_owned()))
        };
        let (Some(a), Some(b)) = (find(member), find(onto)) else {
            return Some(format!("{member} and {onto} do not both meet at {joint}"));
        };
        let at = |id: &str| {
            self.nodes
                .iter()
                .find(|n| n.id == id)
                .map(|n| n.point.map(|v| v.si()))
        };
        let away = |m: &crate::structures::TypeMember| {
            let far = if m.nodes[0] == joint {
                &m.nodes[1]
            } else {
                &m.nodes[0]
            };
            let (p, q) = (at(joint)?, at(far)?);
            let d = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
            let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            (l > 1e-9).then(|| d.map(|v| v / l))
        };
        let (Some(u), Some(v)) = (away(a), away(b)) else {
            return Some("a joint without a position".into());
        };
        // In line: they leave the joint in opposite directions.
        let cos = -(u[0] * v[0] + u[1] * v[1] + u[2] * v[2]);
        let angle = cos.clamp(-1.0, 1.0).acos().to_degrees();
        if angle > CONTINUOUS_ANGLE_DEG {
            return Some(format!(
                "{member} and {onto} turn {angle:.1}° at {joint} (at most {CONTINUOUS_ANGLE_DEG}°)"
            ));
        }
        if a.section.is_none() || a.section != b.section {
            return Some(format!("{member} and {onto} are not of the same section"));
        }
        if a.roll != b.roll || a.reference != b.reference || a.orientation != b.orientation {
            return Some(format!(
                "{member} and {onto} are not turned alike (roll, reference)"
            ));
        }
        None
    }

    /// The member `member` runs on into through `joint` as one part
    /// (`kind = "continuous"` given on either end, the other end not
    /// saying otherwise, and [`Self::continuous_problem`] none).
    pub fn continuous_with(&self, member: &str, joint: &str) -> Option<&str> {
        let own = self.connection(member, joint);
        let onto = match own {
            Some(c) if c.kind() == ConnectionKind::Continuous => c.onto.as_deref()?,
            Some(c) if c.kind.is_some() => return None,
            _ => self
                .connections
                .iter()
                .find(|c| {
                    c.joint == joint
                        && c.kind() == ConnectionKind::Continuous
                        && c.onto.as_deref() == Some(member)
                })
                .map(|c| c.member.as_str())?,
        };
        // The other end not saying otherwise.
        if let Some(o) = self.connection(onto, joint) {
            let agrees = o.kind.is_none()
                || (o.kind() == ConnectionKind::Continuous && o.onto.as_deref() == Some(member));
            if !agrees {
                return None;
            }
        }
        self.continuous_problem(member, onto, joint)
            .is_none()
            .then_some(onto)
    }

    /// The splice `id` at `joint`, if the file gives one.
    pub fn splice(&self, id: &str, joint: &str) -> Option<&TypeSplice> {
        self.splices.iter().find(|s| s.id == id && s.joint == joint)
    }

    /// The connection of `member` at `joint`, if the file gives one.
    pub fn connection(&self, member: &str, joint: &str) -> Option<&TypeConnection> {
        self.connections
            .iter()
            .find(|c| c.member == member && c.joint == joint)
    }

    /// The weld between plates `a` and `b` given with its data
    /// (`[[plate.weld]]` on either), if any.
    pub fn weld_between(&self, a: &str, b: &str) -> Option<&TypeWeld> {
        self.plates.iter().find_map(|p| {
            let other = if p.id == a {
                b
            } else if p.id == b {
                a
            } else {
                return None;
            };
            p.welds.iter().find(|w| w.to == other)
        })
    }

    /// The welded assemblies at `joint`: the plates joined by `welded_to`
    /// (either way, directly or through others; only plates of that joint),
    /// and the joint's plates of the 2026-11-03 `kind = "welded"` together;
    /// each in the file's order, a plate welded to none alone. Every plate
    /// of the joint is in exactly one.
    pub fn welded_assemblies(&self, joint: &str) -> Vec<Vec<String>> {
        let here: Vec<&TypePlate> = self.plates.iter().filter(|p| p.joint == joint).collect();
        let n = here.len();
        let mut root: Vec<usize> = (0..n).collect();
        fn find(root: &mut [usize], i: usize) -> usize {
            let mut i = i;
            while root[i] != i {
                root[i] = root[root[i]];
                i = root[i];
            }
            i
        }
        let join = |root: &mut Vec<usize>, a: usize, b: usize| {
            let (ra, rb) = (find(root, a), find(root, b));
            if ra != rb {
                root[ra.max(rb)] = ra.min(rb);
            }
        };
        let welded: Vec<usize> = (0..n)
            .filter(|&i| here[i].kind() == PlateKind::Welded)
            .collect();
        for w in welded.windows(2) {
            join(&mut root, w[0], w[1]);
        }
        for (i, p) in here.iter().enumerate() {
            for o in p.welded_ids() {
                if let Some(k) = here.iter().position(|q| q.id == o) {
                    join(&mut root, i, k);
                }
            }
        }
        let mut out: Vec<(usize, Vec<String>)> = Vec::new();
        for (i, p) in here.iter().enumerate() {
            let r = find(&mut root, i);
            match out.iter_mut().find(|x| x.0 == r) {
                Some(x) => x.1.push(p.id.clone()),
                None => out.push((r, vec![p.id.clone()])),
            }
        }
        out.into_iter().map(|x| x.1).collect()
    }

    /// The connections and plates whose references lead nowhere (kept,
    /// not used): one line each.
    pub fn connection_notes(&self) -> Vec<String> {
        let mut out = Vec::new();
        for c in &self.connections {
            let at = format!("connection of {} at {}", c.member, c.joint);
            match self.members.iter().find(|m| m.id == c.member) {
                None => out.push(format!("{at}: no member {} (kept, not used)", c.member)),
                Some(m) if !m.nodes.contains(&c.joint) => out.push(format!(
                    "{at}: the member does not end at joint {} (kept, not used)",
                    c.joint
                )),
                Some(_) => {}
            }
            if let Some(o) = &c.onto {
                let member = self
                    .members
                    .iter()
                    .any(|m| &m.id == o && m.nodes.contains(&c.joint));
                let plate = self.plates.iter().any(|p| &p.id == o && p.joint == c.joint);
                let splice = self.splice(o, &c.joint);
                if !member && !plate && splice.is_none() {
                    out.push(format!(
                        "{at}: onto {o}: no member meeting at the joint, plate or splice there"
                    ));
                }
                if let Some(s) = splice
                    && !s.members.contains(&c.member)
                {
                    out.push(format!("{at}: onto splice {o}, which does not join it"));
                }
                if c.kind() == ConnectionKind::Continuous && plate | splice.is_some() {
                    out.push(format!(
                        "{at}: continuous onto {o}, a plate or splice: computed as bolted onto it"
                    ));
                }
            }
            if c.kind() == ConnectionKind::Continuous {
                match &c.onto {
                    None => out.push(format!(
                        "{at}: continuous into no member (onto): computed as a splice"
                    )),
                    Some(o) if self.members.iter().any(|m| &m.id == o) => {
                        if let Some(why) = self.continuous_problem(&c.member, o, &c.joint) {
                            out.push(format!("{at}: not one part ({why}): computed as a splice"));
                        } else if self.continuous_with(&c.member, &c.joint).is_none() {
                            out.push(format!(
                                "{at}: the connection of {o} there says otherwise: computed as a \
                                 splice"
                            ));
                        }
                    }
                    Some(_) => {}
                }
                if c.bolts.is_some() {
                    out.push(format!("{at}: continuous: its bolts not used"));
                }
            }
            if !c.other_leg_points.is_empty() && c.leg != Some(BoltedLeg::Both) {
                out.push(format!(
                    "{at}: other_leg_points without leg = \"both\" (kept, not used)"
                ));
            }
        }
        for s in &self.splices {
            if !self.nodes.iter().any(|n| n.id == s.joint) {
                out.push(format!(
                    "splice {}: no joint {} (kept, not used)",
                    s.id, s.joint
                ));
            }
            for m in &s.members {
                if !self
                    .members
                    .iter()
                    .any(|x| &x.id == m && x.nodes.contains(&s.joint))
                {
                    out.push(format!(
                        "splice {}: {m} does not meet at joint {} (kept, not used)",
                        s.id, s.joint
                    ));
                }
            }
            if s.covers.is_empty() {
                out.push(format!("splice {}: no covers", s.id));
            }
        }
        for p in &self.plates {
            if !self.nodes.iter().any(|n| n.id == p.joint) {
                out.push(format!(
                    "plate {}: no joint {} (kept, not used)",
                    p.id, p.joint
                ));
            }
            for o in p.welded_ids() {
                if !self.plates.iter().any(|q| q.id == o && q.joint == p.joint) {
                    out.push(format!(
                        "plate {}: welded to {o}: no plate of joint {} (kept, not used)",
                        p.id, p.joint
                    ));
                }
            }
            if p.kind() == PlateKind::Welded {
                let with: Vec<String> = self
                    .welded_assemblies(&p.joint)
                    .into_iter()
                    .find(|a| a.contains(&p.id))
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|x| x != &p.id)
                    .collect();
                out.push(if with.is_empty() {
                    format!(
                        "plate {}: kind = \"welded\" (welded onto the leg, 2026-11-03) read as a \
                         plate; a welded plate is now several plates welded together (welded_to)",
                        p.id
                    )
                } else {
                    format!(
                        "plate {}: kind = \"welded\" (welded onto the leg, 2026-11-03) read as \
                         welded to {} (one welded plate)",
                        p.id,
                        with.join(", ")
                    )
                });
            }
        }
        out
    }

    /// The connections and plates as the file must hold them: a member end
    /// given once, plate ids unique and not a member's, sizes above 0,
    /// one or two gauge lines.
    pub(crate) fn check_connections(&self) -> Result<(), String> {
        let mut ends = std::collections::BTreeSet::new();
        for c in &self.connections {
            let at = format!("connection of {} at {}", c.member, c.joint);
            if c.member.is_empty() || c.joint.is_empty() {
                return Err("connection: member and joint needed".into());
            }
            if !ends.insert((c.member.as_str(), c.joint.as_str())) {
                return Err(format!("{at}: given twice"));
            }
            if let Some(b) = &c.bolts {
                if b.diameter.is_none() && b.bolt.is_none() {
                    return Err(format!(
                        "{at}: bolts need a bolt of a library or a diameter"
                    ));
                }
                let d = b.diameter.map_or(0.0, |d| d.si());
                if b.diameter.is_some_and(|d| d.si() <= 0.0) || b.hole.is_some_and(|h| h.si() < d) {
                    return Err(format!(
                        "{at}: a bolt diameter above 0 and a hole no smaller"
                    ));
                }
                if b.shear_planes == Some(0) {
                    return Err(format!("{at}: shear planes at least 1"));
                }
            }
            if let Some(n) = c.gauge_lines {
                if !(1..=MAX_GAUGE_LINES).contains(&n) {
                    return Err(format!("{at}: gauge_lines 1 to {MAX_GAUGE_LINES}"));
                }
                if !c.gauges.is_empty() && c.gauges.len() != n as usize {
                    return Err(format!("{at}: one gauge per gauge line"));
                }
            }
            let sizes = [
                c.pitch,
                c.end_distance,
                c.edge_distance,
                c.eccentricity,
                c.cut,
            ];
            if sizes
                .iter()
                .flatten()
                .chain(&c.gauges)
                .any(|v| v.si() < 0.0)
                || c.shift.is_some_and(|s| !s.si().is_finite())
            {
                return Err(format!("{at}: distances not below 0"));
            }
        }
        let mut plates = std::collections::BTreeSet::new();
        for p in &self.plates {
            if p.id.is_empty() || !plates.insert(p.id.as_str()) {
                return Err(format!("plate \"{}\": empty or given twice", p.id));
            }
            if self.members.iter().any(|m| m.id == p.id) {
                return Err(format!("plate {}: a member has that id", p.id));
            }
            if p.thickness.si() <= 0.0 {
                return Err(format!("plate {}: a thickness above 0", p.id));
            }
            if p.welded_ids().contains(&p.id.as_str()) {
                return Err(format!("plate {}: welded to itself", p.id));
            }
            for (k, w) in p.welds.iter().enumerate() {
                if w.size.is_some_and(|v| v.si() <= 0.0) || w.length.is_some_and(|v| v.si() <= 0.0)
                {
                    return Err(format!(
                        "plate {}: the weld to {}: a size and a length above 0",
                        p.id, w.to
                    ));
                }
                if p.welds[..k].iter().any(|x| x.to == w.to) {
                    return Err(format!("plate {}: the weld to {} given twice", p.id, w.to));
                }
            }
        }
        let mut splices = std::collections::BTreeSet::new();
        for s in &self.splices {
            if s.id.is_empty() || !splices.insert(s.id.as_str()) {
                return Err(format!("splice \"{}\": empty or given twice", s.id));
            }
            if self.members.iter().any(|m| m.id == s.id) || plates.contains(s.id.as_str()) {
                return Err(format!("splice {}: a member or a plate has that id", s.id));
            }
            for (k, c) in s.covers.iter().enumerate() {
                if c.thickness.si() <= 0.0 {
                    return Err(format!("splice {}: a cover's thickness above 0", s.id));
                }
                if s.covers[..k].iter().any(|x| {
                    x.face == c.face && (x.leg == c.leg || x.leg.is_none() || c.leg.is_none())
                }) {
                    let leg = c
                        .leg
                        .map_or("one".to_owned(), |l| format!("the {}", l.as_str()));
                    return Err(format!(
                        "splice {}: two {} covers on {leg} leg",
                        s.id,
                        c.face.as_str()
                    ));
                }
            }
        }
        // A weld given on both plates.
        for p in &self.plates {
            for w in &p.welds {
                if self
                    .plates
                    .iter()
                    .any(|q| q.id == w.to && q.welds.iter().any(|x| x.to == p.id))
                    && p.id < w.to
                {
                    return Err(format!(
                        "plates {} and {}: their weld given on both (give it on one)",
                        p.id, w.to
                    ));
                }
            }
        }
        Ok(())
    }
}
