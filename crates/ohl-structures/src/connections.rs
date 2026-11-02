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
//! gauge_lines = 1
//! gauges = ["30 mm"]      # from the heel along the bolted leg, per line
//! pitch = "50 mm"
//! end_distance = "30 mm"
//! edge_distance = "25 mm"
//! # bolt_points = [["30 mm", "30 mm"], ["80 mm", "30 mm"]]  # instead of
//! #   the layout: along the member from the joint, across from the heel
//! # eccentric = true / restrained = false   # given instead of worked out
//!
//! [[plate]]
//! id = "G1"
//! joint = "N7"
//! thickness = "10 mm"
//! material = "S355"       # optional
//! # outline = [["0 m", "0 m", "10 m"], …]   # local axes; none: around
//! #   the bolts of the connections onto it
//! ```
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
}

impl ConnectionKind {
    /// Every kind, in the order offered.
    pub const ALL: [ConnectionKind; 4] = [
        ConnectionKind::Bolted,
        ConnectionKind::Welded,
        ConnectionKind::Pinned,
        ConnectionKind::Fixed,
    ];

    /// Name in files.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bolted => "bolted",
            Self::Welded => "welded",
            Self::Pinned => "pinned",
            Self::Fixed => "fixed",
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
    /// Gauge lines: 1 or 2.
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
    /// Each bolt where it is, instead of the layout: along the member from
    /// the joint, across the bolted leg from the heel.
    #[serde(default)]
    pub bolt_points: Vec<[Length; 2]>,
    /// Eccentric, given (none: worked out from the leg bolted).
    #[serde(default)]
    pub eccentric: Option<bool>,
    /// Held against turning, given (none: worked out from the bolts).
    #[serde(default)]
    pub restrained: Option<bool>,
    /// The eccentricity, given (none: from the gauge and the section).
    #[serde(default)]
    pub eccentricity: Option<Length>,
}

/// A gusset plate at a joint (`[[plate]]`).
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
}

impl TypeConnection {
    /// How it is held (bolted when not given).
    pub fn kind(&self) -> ConnectionKind {
        self.kind.unwrap_or_default()
    }
}

impl StructureType {
    /// The connection of `member` at `joint`, if the file gives one.
    pub fn connection(&self, member: &str, joint: &str) -> Option<&TypeConnection> {
        self.connections
            .iter()
            .find(|c| c.member == member && c.joint == joint)
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
                if !member && !plate {
                    out.push(format!(
                        "{at}: onto {o}: no member meeting at the joint or plate there"
                    ));
                }
            }
        }
        for p in &self.plates {
            if !self.nodes.iter().any(|n| n.id == p.joint) {
                out.push(format!(
                    "plate {}: no joint {} (kept, not used)",
                    p.id, p.joint
                ));
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
                if !(1..=2).contains(&n) {
                    return Err(format!("{at}: gauge_lines 1 or 2"));
                }
                if !c.gauges.is_empty() && c.gauges.len() != n as usize {
                    return Err(format!("{at}: one gauge per gauge line"));
                }
            }
            let sizes = [c.pitch, c.end_distance, c.edge_distance, c.eccentricity];
            if sizes
                .iter()
                .flatten()
                .chain(&c.gauges)
                .any(|v| v.si() < 0.0)
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
        }
        Ok(())
    }
}
