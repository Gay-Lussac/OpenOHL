// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Joints grouped by how they are laid out and what comes in** (the
//! automatic joint layout's L12, owner 2026-11-21: "grouping the joints
//! based on how they are laid out and what kind of members come in to
//! better understand them"). Built, in active development, not approved
//! yet.
//!
//! A joint is described from its details alone (never from names): its
//! **host** — what the other ends bolt onto: by class first (legs, then
//! bracing, then redundants, as the layout ranks it), then the largest
//! steel area, with the piece in line with it where one runs on —, **what runs
//! through** (the host's pieces one part, the same section not confirmed
//! one part, a change of section or a splice, nothing: the host ends
//! here), where it lies (the body, an arm, the top: [`mod@crate::joint_order`])
//! and, for every other end, its class, the plane it lies in against the
//! host's two flanges (in one flange's plane, or how far out of both), its
//! angle to the host, how far its bolted leg lies off that flange (its
//! roll), its size against the host's and its bolts ([`JointFeatures`]).
//!
//! From those the joint's **group** ([`JointGroup`], [`group_of`]): what
//! the ends bolt onto, what comes in, and how the incoming ends lie against
//! the host's flanges — chosen on the measured towers so that the joints of
//! one group are laid out alike (the theory page's L12 section says how:
//! the plane the ends lie in divides them best). The groups are for
//! understanding; the layout's results do not depend on them.
//!
//! Deterministic: a pure function of the joint's details. The tolerances
//! are the tool's, not a standard's.

use crate::joint_order::Zone;
use crate::joints::{Arrival, JointDetail};

/// Two pieces run on in line through a joint within this, degrees (the
/// tool's value, as Orient angles' in-line tolerance).
pub const IN_LINE_DEG: f64 = 10.0;

/// An end lies in a flange's plane within this, degrees: the owner's 2°
/// rule for a leg bolted flat ([`crate::FLAT_OFF`]).
pub const IN_PLANE_DEG: f64 = crate::FLAT_OFF;

/// An end lies near a flange's plane within this, degrees (the tool's
/// value: a triangular crossarm's face lies about 15° off a chord's
/// flange, Orient's half of the corner's departure).
pub const NEAR_PLANE_DEG: f64 = 16.0;

/// A member steep within this of the vertical is a body leg (the cosine
/// of 30°, as [`crate::joint_order::LEG_STEEP`]).
pub const STEEP: f64 = crate::joint_order::LEG_STEEP;

/// A member's class as the layout reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MemberClass {
    /// A leg (or an arm's chord: class `leg`).
    Leg,
    /// A face's bracing (class `bracing`).
    Bracing,
    /// A crossing diagonal (class `crossing`).
    Crossing,
    /// A redundant (class `redundant`).
    Redundant,
    /// No class, or another.
    Other,
}

impl MemberClass {
    /// From an arrival's class.
    pub fn of(a: &Arrival) -> Self {
        match a.class.as_deref() {
            Some("leg") => Self::Leg,
            Some("bracing") => Self::Bracing,
            Some("crossing") => Self::Crossing,
            Some("redundant") => Self::Redundant,
            _ => Self::Other,
        }
    }

    /// In words, short.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Leg => "leg",
            Self::Bracing => "bracing",
            Self::Crossing => "crossing",
            Self::Redundant => "redundant",
            Self::Other => "other",
        }
    }
}

/// What runs through the joint along the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Through {
    /// The host ends here (nothing in line with it).
    Nothing,
    /// The host's two pieces are one part (`kind = "continuous"`).
    OnePart,
    /// Two pieces of the same section in line, not one part (their
    /// bolts join them, or nothing does).
    SameSection,
    /// Two pieces of other sections in line, a splice given (covers).
    Splice,
    /// Two pieces of other sections in line, no splice given.
    SectionChange,
}

impl Through {
    /// In words, short.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Nothing => "ends here",
            Self::OnePart => "one part",
            Self::SameSection => "same section, not one part",
            Self::Splice => "spliced",
            Self::SectionChange => "section changes, no splice",
        }
    }
}

/// Where an end lies against the host's flanges.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EndPlane {
    /// In the plane of the host's flange 0 (along its section's y) or 1
    /// (along z), within [`IN_PLANE_DEG`]; how far off, degrees.
    In(usize, f64),
    /// Near a flange's plane (within [`NEAR_PLANE_DEG`]): that flange and
    /// how far off, degrees.
    Near(usize, f64),
    /// Out of both flanges' planes: the nearest and how far off, degrees.
    Out(usize, f64),
    /// No flange to measure against (the host is not an angle, or this is
    /// the host).
    Unknown,
}

impl EndPlane {
    /// The flange it lies in or near (none: out of both, or unknown).
    pub fn flange(self) -> Option<usize> {
        match self {
            Self::In(k, _) | Self::Near(k, _) => Some(k),
            _ => None,
        }
    }

    /// How far off the nearest flange's plane, degrees (unknown: none).
    pub fn off(self) -> Option<f64> {
        match self {
            Self::In(_, d) | Self::Near(_, d) | Self::Out(_, d) => Some(d),
            Self::Unknown => None,
        }
    }
}

/// One end at the joint, described against the host.
#[derive(Debug, Clone, PartialEq)]
pub struct EndFeature {
    /// The member.
    pub member: String,
    /// Its class.
    pub class: MemberClass,
    /// Its section's name.
    pub section: String,
    /// A single angle (false: built up, another shape, or no profile).
    pub angle: bool,
    /// Built up of several pieces.
    pub built_up: bool,
    /// Its larger leg (or the outline's width), m; 0 without a profile.
    pub width: f64,
    /// Its thickness, m (an angle's; else 0).
    pub thickness: f64,
    /// Its steel area as drawn, m².
    pub area: f64,
    /// On the inner face of the flange it bolts onto (as turned, or given).
    pub inner: bool,
    /// From the joint along it (unit).
    pub direction: [f64; 3],
    /// Level (within about 11° of the horizontal) and in no steep face:
    /// a plan or hip member (the joint details' `plan` face).
    pub plan: bool,
    /// The host (true for both pieces of a host running through).
    pub host: bool,
    /// One part with another end at the joint.
    pub one_part: bool,
    /// In line with another end at the joint (not the host's run: a
    /// crossing diagonal running on, a redundant's pieces).
    pub runs_on: bool,
    /// Its bolts at this end (given, or its member's design data); 0: none.
    pub bolts: u32,
    /// Its angle to the host's axis, degrees (0 to 90; the host: 0).
    pub to_host: f64,
    /// The plane it lies in against the host's flanges.
    pub plane: EndPlane,
    /// How far its bolted leg lies off the flange of `plane` (its roll),
    /// degrees (0 to 90; none: no flange, or not an angle).
    pub flat_off: Option<f64>,
    /// Its width over the host's (1: as big).
    pub size: f64,
}

/// A joint described by what comes in and how it is laid out.
#[derive(Debug, Clone, PartialEq)]
pub struct JointFeatures {
    /// The joint.
    pub joint: String,
    /// Where it lies on the tower.
    pub zone: Zone,
    /// The host's member (the biggest; none: no member with a profile).
    pub host: Option<String>,
    /// The host's class.
    pub host_class: MemberClass,
    /// The host runs steeply (a body leg's line, within 30° of the
    /// vertical).
    pub host_steep: bool,
    /// The host is a single angle.
    pub host_angle: bool,
    /// What runs through along the host.
    pub through: Through,
    /// Every end (in the joint's member order).
    pub ends: Vec<EndFeature>,
}

impl JointFeatures {
    /// The ends not of the host.
    pub fn incoming(&self) -> impl Iterator<Item = &EndFeature> {
        self.ends.iter().filter(|e| !e.host)
    }

    /// The incoming ends of class `c`.
    pub fn count(&self, c: MemberClass) -> usize {
        self.incoming().filter(|e| e.class == c).count()
    }

    /// The host's flanges the incoming ends (not plan, not running on)
    /// lie in or near: one bit per flange.
    pub fn flanges_used(&self) -> u8 {
        let mut b = 0u8;
        for e in self.incoming().filter(|e| !e.plan) {
            if let Some(k) = e.plane.flange() {
                b |= 1 << k;
            }
        }
        b
    }
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// An end's steel area as drawn (its outlines' areas), m².
fn area_of(a: &Arrival) -> f64 {
    let Some(p) = a.profile.as_ref() else {
        return 0.0;
    };
    p.pieces()
        .map(|q| {
            let mut s = 0.0;
            for i in 0..q.len() {
                let (u, v) = (q[i], q[(i + 1) % q.len()]);
                s += u[0] * v[1] - v[0] * u[1];
            }
            (s / 2.0).abs()
        })
        .sum()
}

/// The size key of an end: its larger leg (else the outline's width) and
/// its thickness, m.
fn size_of(a: &Arrival) -> (f64, f64) {
    let Some(p) = a.profile.as_ref() else {
        return (0.0, 0.0);
    };
    if let Some(an) = &p.angle {
        return (an.b.max(an.h), an.t);
    }
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for q in p.pieces().flatten() {
        for k in 0..2 {
            lo[k] = lo[k].min(q[k]);
            hi[k] = hi[k].max(q[k]);
        }
    }
    let w = (hi[0] - lo[0]).max(hi[1] - lo[1]);
    (if w.is_finite() { w } else { 0.0 }, 0.0)
}

fn class_rank(c: MemberClass) -> u8 {
    match c {
        MemberClass::Leg => 4,
        MemberClass::Crossing | MemberClass::Bracing => 3,
        MemberClass::Redundant => 2,
        MemberClass::Other => 1,
    }
}

/// The joint `j`, lying in `zone`, described (see the module).
pub fn joint_features(j: &JointDetail, zone: Zone) -> JointFeatures {
    let cos_line = IN_LINE_DEG.to_radians().cos();
    let n = j.arrivals.len();
    // Pieces in line: each end's partner (the first in line with it).
    let mut partner: Vec<Option<usize>> = vec![None; n];
    for i in 0..n {
        for k in 0..n {
            if k != i
                && partner[i].is_none()
                && partner[k].is_none()
                && dot(j.arrivals[i].direction, j.arrivals[k].direction) < -cos_line
            {
                partner[i] = Some(k);
                partner[k] = Some(i);
            }
        }
    }
    // The host: by class (legs first, as the layout ranks what ends bolt
    // onto), then the biggest steel area, then the larger leg; a piece in
    // line with it its run; ties by the file's order.
    let key = |i: usize| {
        let a = &j.arrivals[i];
        let (w, _) = size_of(a);
        (
            class_rank(MemberClass::of(a)),
            (area_of(a) * 1e7).round() as i64,
            (w * 1e4).round() as i64,
        )
    };
    let host = (0..n)
        .filter(|&i| j.arrivals[i].profile.is_some() && !j.arrivals[i].modelling)
        .fold(None::<usize>, |best, i| match best {
            Some(b) if key(b) >= key(i) => Some(b),
            _ => Some(i),
        });
    let host_run = host.and_then(|h| partner[h]);
    let through = match (host, host_run) {
        (Some(h), Some(r)) => {
            let (a, b) = (&j.arrivals[h], &j.arrivals[r]);
            let one = a.continuous.as_deref() == Some(b.member.as_str())
                || b.continuous.as_deref() == Some(a.member.as_str());
            let section = |x: &Arrival| x.profile.as_ref().map(|p| p.section.clone());
            let spliced = j.splices.iter().any(|s| {
                let has = |m: &str| s.members.iter().any(|x| x == m);
                has(&a.member) && has(&b.member)
            });
            if one {
                Through::OnePart
            } else if spliced {
                Through::Splice
            } else if section(a) == section(b) {
                Through::SameSection
            } else {
                Through::SectionChange
            }
        }
        _ => Through::Nothing,
    };
    let hp = host.and_then(|h| j.arrivals[h].profile.as_ref());
    let host_axis = host.map(|h| j.arrivals[h].direction);
    // The host's flanges' normals: an angle's leg along y has its face's
    // normal along z, and the other way; a built-up member's angles lie
    // along the same axes (their flanges in those planes).
    let normals: Option<[[f64; 3]; 2]> = hp
        .filter(|p| p.angle.is_some() || !p.part_angles.is_empty())
        .map(|p| [p.axes[2], p.axes[1]]);
    let host_width = host.map_or(0.0, |h| size_of(&j.arrivals[h]).0);
    let mut ends = Vec::with_capacity(n);
    for (i, a) in j.arrivals.iter().enumerate() {
        let is_host = Some(i) == host || Some(i) == host_run;
        let (width, thickness) = size_of(a);
        let p = a.profile.as_ref();
        let angle = p.is_some_and(|p| p.angle.is_some() && p.parts.is_empty());
        let built_up = p.is_some_and(|p| !p.parts.is_empty());
        let to_host = host_axis.map_or(0.0, |x| {
            dot(x, a.direction).abs().min(1.0).acos().to_degrees()
        });
        let (plane, flat_off) = match (normals, is_host) {
            (Some(nn), false) => {
                let off = |k: usize| dot(nn[k], a.direction).abs().min(1.0).asin().to_degrees();
                let k = if off(0) <= off(1) { 0 } else { 1 };
                let d = off(k);
                let plane = if d <= IN_PLANE_DEG {
                    EndPlane::In(k, d)
                } else if d <= NEAR_PLANE_DEG {
                    EndPlane::Near(k, d)
                } else {
                    EndPlane::Out(k, d)
                };
                // Its bolted leg's face normal: along z when the leg
                // bolted lies along y, else along y.
                let flat = p.filter(|_| angle).map(|p| {
                    let m = if a.leg_y { p.axes[2] } else { p.axes[1] };
                    dot(m, nn[k]).abs().min(1.0).acos().to_degrees()
                });
                (plane, flat)
            }
            _ => (EndPlane::Unknown, None),
        };
        ends.push(EndFeature {
            member: a.member.clone(),
            class: MemberClass::of(a),
            section: p.map(|p| p.section.clone()).unwrap_or_default(),
            angle,
            built_up,
            width,
            thickness,
            area: area_of(a),
            inner: a.sits.value == Some(crate::Face::Inside),
            direction: a.direction,
            plan: a.face == "plan",
            host: is_host,
            one_part: a.continuous.is_some(),
            runs_on: !is_host && partner[i].is_some(),
            bolts: a.bolts.value.as_ref().map_or(0, |b| b.count),
            to_host: if is_host { 0.0 } else { to_host },
            plane,
            flat_off,
            size: if host_width > 0.0 {
                width / host_width
            } else {
                0.0
            },
        });
    }
    let (host_class, host_steep, host_angle) = match host {
        Some(h) => {
            let a = &j.arrivals[h];
            (
                MemberClass::of(a),
                a.direction[2].abs() >= STEEP,
                ends[h].angle,
            )
        }
        None => (MemberClass::Other, false, false),
    };
    JointFeatures {
        joint: j.id.clone(),
        zone,
        host: host.map(|h| j.arrivals[h].member.clone()),
        host_class,
        host_steep,
        host_angle,
        through,
        ends,
    }
}

/// The host's pieces running on through joint `j` that are not legs (an
/// arm's chord, a body's horizontal or diagonal running on): their indices
/// among its arrivals; none where the host ends here, is a leg or is not a
/// single angle. What [`crate::piece_rolls`] reads a chord's turn from.
pub fn chord_pieces(j: &JointDetail) -> Vec<usize> {
    let f = joint_features(j, Zone::Body);
    if f.host_class == MemberClass::Leg
        || !f.host_angle
        || !matches!(f.through, Through::OnePart | Through::SameSection)
    {
        return Vec::new();
    }
    f.ends
        .iter()
        .enumerate()
        .filter(|(_, e)| e.host && e.angle)
        .map(|(i, _)| i)
        .collect()
}

/// What the joint's other ends bolt onto, and what runs through it: the
/// first part of a joint's group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HostKind {
    /// A single-angle leg running on through the joint (one part, or two
    /// pieces of one section).
    LegRunning,
    /// A single-angle leg whose section changes here (a splice).
    LegSplice,
    /// A single-angle leg ending here (a foot, the top of a leg, a smaller
    /// member in line above it).
    LegEnds,
    /// A built-up leg (two or four angles).
    BuiltUpLeg,
    /// A single angle running on through a joint of an arm or the top (a
    /// chord, an arm's horizontal).
    ChordRunning,
    /// A single angle running on through a joint of the body that is not a
    /// leg (a face's horizontal or diagonal at a panel point).
    MemberRunning,
    /// Two members running on through the joint, crossing (an X-brace's
    /// centre).
    Crossing,
    /// Nothing runs through: the biggest member ends here like the others
    /// (an arm's tip, a K-brace's apex, a peak).
    NothingThrough,
    /// A built-up member that is not a leg (a double-angle horizontal).
    BuiltUpMember,
    /// No member with a profile to bolt onto.
    NoHost,
}

impl HostKind {
    /// In words, short.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LegRunning => "leg running on",
            Self::LegSplice => "leg splice",
            Self::LegEnds => "leg ends",
            Self::BuiltUpLeg => "built-up leg",
            Self::ChordRunning => "chord running on (arm, top)",
            Self::MemberRunning => "member running on (body)",
            Self::Crossing => "crossing",
            Self::NothingThrough => "nothing runs through",
            Self::BuiltUpMember => "built-up member",
            Self::NoHost => "no host",
        }
    }
}

/// What comes in (the ends not of the host, not running on): the second
/// part of a joint's group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Incoming {
    /// Redundants only.
    Redundants,
    /// Bracing (with or without redundants), no plan member.
    Bracing,
    /// A plan or hip member among them (level, in no steep face).
    WithPlan,
    /// Nothing (only members running on).
    Nothing,
}

impl Incoming {
    /// In words, short.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Redundants => "redundants",
            Self::Bracing => "bracing",
            Self::WithPlan => "with plan members",
            Self::Nothing => "nothing",
        }
    }
}

/// How the incoming ends lie against the host's flanges, the farthest
/// off: the third part of a joint's group (the measured towers' main
/// divide: an end out of a flange's plane cannot lie flat on it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Geometry {
    /// Every end within [`IN_PLANE_DEG`] of a flange's plane (they can lie
    /// flat on it).
    Flat,
    /// The farthest within [`NEAR_PLANE_DEG`] (a sloping face against a
    /// chord's flange, a tapered body).
    Near,
    /// One at least farther (a plan or hip member at a leg, a member
    /// between two faces).
    Out,
    /// No end to measure (no single-angle end, or no flange).
    None,
}

impl Geometry {
    /// In words, short.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Flat => "all in a flange's plane",
            Self::Near => "some a few degrees out",
            Self::Out => "some out of plane",
            Self::None => "none to measure",
        }
    }
}

/// A joint's group: what it bolts onto, what comes in, how it lies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JointGroup {
    /// What the ends bolt onto and what runs through.
    pub host: HostKind,
    /// What comes in.
    pub incoming: Incoming,
    /// How the incoming ends lie against the host's flanges.
    pub geometry: Geometry,
}

impl JointGroup {
    /// Its name: "leg running on · bracing · all in a flange's plane".
    pub fn name(&self) -> String {
        format!(
            "{} · {} · {}",
            self.host.as_str(),
            self.incoming.as_str(),
            self.geometry.as_str()
        )
    }
}

/// The group of a joint described by `f` (see the module).
pub fn group_of(f: &JointFeatures) -> JointGroup {
    let host_end = f
        .host
        .as_deref()
        .and_then(|h| f.ends.iter().find(|e| e.member == h));
    let partner = f
        .ends
        .iter()
        .find(|e| e.host && Some(e.member.as_str()) != f.host.as_deref());
    let crossing = f.incoming().filter(|e| e.runs_on).count() >= 2;
    let host = match host_end {
        None => HostKind::NoHost,
        Some(h) if h.built_up => {
            if f.host_class == MemberClass::Leg {
                HostKind::BuiltUpLeg
            } else {
                HostKind::BuiltUpMember
            }
        }
        Some(h) if !h.angle => HostKind::NoHost,
        Some(_) => {
            let runs = matches!(f.through, Through::OnePart | Through::SameSection);
            let change = matches!(f.through, Through::Splice | Through::SectionChange);
            if f.host_class == MemberClass::Leg && f.host_steep {
                if runs {
                    HostKind::LegRunning
                } else if change && partner.is_some_and(|p| p.class == MemberClass::Leg) {
                    HostKind::LegSplice
                } else {
                    HostKind::LegEnds
                }
            } else if !(runs || change) {
                HostKind::NothingThrough
            } else if crossing {
                HostKind::Crossing
            } else if f.zone == Zone::Body {
                HostKind::MemberRunning
            } else {
                HostKind::ChordRunning
            }
        }
    };
    let es: Vec<&EndFeature> = f.incoming().filter(|e| !e.runs_on && !e.one_part).collect();
    let incoming = if es.is_empty() {
        Incoming::Nothing
    } else if es.iter().any(|e| e.plan) {
        Incoming::WithPlan
    } else if es.iter().all(|e| e.class == MemberClass::Redundant) {
        Incoming::Redundants
    } else {
        Incoming::Bracing
    };
    let geometry = match es
        .iter()
        .filter(|e| e.angle)
        .filter_map(|e| e.plane.off())
        .reduce(f64::max)
    {
        None => Geometry::None,
        Some(d) if d <= IN_PLANE_DEG => Geometry::Flat,
        Some(d) if d <= NEAR_PLANE_DEG => Geometry::Near,
        Some(_) => Geometry::Out,
    };
    JointGroup {
        host,
        incoming,
        geometry,
    }
}

/// Every joint of `d` described and grouped (each in its zone of
/// `order`), in the file's order.
pub fn joint_groups(
    d: &crate::JointDetails,
    order: &crate::JointOrder,
) -> Vec<(JointFeatures, JointGroup)> {
    d.joints
        .iter()
        .map(|j| {
            let f = joint_features(j, order.zone_of(&j.id));
            let g = group_of(&f);
            (f, g)
        })
        .collect()
}
