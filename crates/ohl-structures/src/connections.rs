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
//! # eccentric = true / restrained = false   # given instead of worked out
//! # cut = "40 mm"   # where the steel stops from the work point (none:
//! #   worked out so it fits); the axis still runs to the work point
//! # shift = "15 mm"  # the end moved sideways to fit (none: 0); its force
//! #   then passes beside the work point: an eccentricity, analysed
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
}

impl StructureType {
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
